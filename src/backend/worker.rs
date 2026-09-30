// Copyright (C) 2026 CharOfString <root@charofstring.cc>
//
//
// This software is free software: you can redistribute it and/or modify it under the terms of the
// GNU General Public License as published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This software is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with this software. If
// not, see <https://www.gnu.org/licenses/>.

//! Privileged session supervisor => only its desktop child drops to the user's UID.

use super::{
    auth::Conversation,
    desktop::{Desktop, invalid},
    login::{Request, check_host},
    logind,
};

use nix::{
    sys::signal::{SigHandler, Signal, killpg, signal},
    unistd::{Pid, User, getgrouplist, setgid, setgroups, setuid},
};

use pam_client::{Context, Flag};
use std::{
    ffi::CString,
    fs::{self, File},
    io::{self, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::MetadataExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use zeroize::Zeroize;

const PATH: &str = "/usr/local/bin:/usr/bin:/bin";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run() -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        signal_hook::flag::register(sig, Arc::clone(&stop))?;
    }

    let (tty, vt) = check_host()?;
    let request = Request::read_from(io::stdin().lock())?;
    let desktop = Desktop::read(&request.session)?;
    if desktop.kind == "x11" && !Path::new("/usr/bin/startx").is_file() {
        return Err(invalid("X11 login requires the xinit package (/usr/bin/startx)").into());
    }

    if !Path::new("/etc/pam.d/gxdm-rescue").is_file() {
        return Err(invalid("Install the gxdm-rescue PAM policy first").into());
    }

    let conservation_gen = Conversation::new(request.username.clone(), request.password);
    let mut pam = Context::new("gxdm-rescue", Some(&request.username), conservation_gen)?;
    pam.set_tty(Some(&format!("tty{vt}")))?;
    for value in [
        format!("XDG_SESSION_TYPE={}", desktop.kind),
        "XDG_SESSION_CLASS=user".into(),
        "XDG_SEAT=seat0".into(),
        format!("XDG_VTNR={vt}"),
        format!("XDG_SESSION_DESKTOP={}", desktop.name),
    ] {
        pam.putenv(value)?;
    }

    let auth = pam
        .authenticate(Flag::DISALLOW_NULL_AUTHTOK)
        .and_then(|()| pam.acct_mgmt(Flag::NONE));
    pam.conversation_mut().password.zeroize();
    auth.map_err(|_| invalid("Login failed (credentials or account policy)"))?;
    let user = User::from_name(&pam.user()?)?.ok_or_else(|| invalid("User lookup failed"))?;
    if user.uid.is_root() {
        return Err(invalid("Graphical root login is disabled").into());
    }

    if stop.load(Ordering::Relaxed) {
        return Err(invalid("Login cancelled").into());
    }

    let session = pam.open_session(Flag::NONE)?;
    let id = session
        .getenv("XDG_SESSION_ID")
        .ok_or_else(|| invalid("No logind session ID"))?;

    // Report the ID before any later error.
    println!("{id}");
    io::stdout().flush()?;
    logind::verify(id, user.uid.as_raw(), vt)?;
    let runtime = session
        .getenv("XDG_RUNTIME_DIR")
        .ok_or_else(|| invalid("No runtime directory"))?;
    if runtime != format!("/run/user/{}", user.uid) {
        return Err(invalid("Unexpected runtime directory").into());
    }

    let meta = fs::metadata(runtime)?;
    if !meta.is_dir() || meta.uid() != user.uid.as_raw() || meta.mode() & 0o077 != 0 {
        return Err(invalid("Unsafe runtime directory").into());
    }

    let mut command = desktop_command(&desktop, vt)?;
    command.env_clear();
    let environment = session.envlist();
    command.envs(environment.iter().map(|item| item.key_value()));

    // Override identity and seat metadata.
    command
        .env("HOME", &user.dir)
        .env("USER", &user.name)
        .env("LOGNAME", &user.name)
        .env("SHELL", &user.shell)
        .env("PATH", PATH)
        .env("XDG_SESSION_TYPE", desktop.kind)
        .env("XDG_CURRENT_DESKTOP", &desktop.desktops)
        .env("DESKTOP_SESSION", &desktop.name)
        .env("XDG_SESSION_DESKTOP", &desktop.name)
        .env("XDG_SESSION_CLASS", "user")
        .env("XDG_SEAT", "seat0")
        .env("XDG_VTNR", vt.to_string())
        .env("LIBSEAT_BACKEND", "logind")
        .env("XDG_RUNTIME_DIR", runtime)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={runtime}/bus"),
        )
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("XAUTHORITY");
    let groups = getgrouplist(&CString::new(user.name.as_str())?, user.gid)?;
    let home = CString::new(user.dir.as_os_str().as_encoded_bytes())?;
    let tty_fd = tty.as_raw_fd();
    let _owner = TtyOwner::give_to(&tty, &user)?;
    command
        .stdin(Stdio::from(tty.try_clone()?))
        .stdout(Stdio::from(tty.try_clone()?))
        .stderr(Stdio::from(tty.try_clone()?))
        .process_group(0);

    unsafe {
        command.pre_exec(move || {
            // The root supervisor is outside this group and can always reclaim the VT.
            if libc::tcsetpgrp(tty_fd, libc::getpgrp()) < 0 {
                return Err(io::Error::last_os_error());
            }
            for sig in [
                Signal::SIGINT,
                Signal::SIGTERM,
                Signal::SIGHUP,
                Signal::SIGTTOU,
                Signal::SIGTTIN,
                Signal::SIGTSTP,
                Signal::SIGPIPE,
            ] {
                signal(sig, SigHandler::SigDfl)?;
            }
            setgroups(&groups)?;
            setgid(user.gid)?;
            setuid(user.uid)?;
            if libc::chdir(home.as_ptr()) < 0 {
                return Err(io::Error::last_os_error());
            }
            libc::umask(0o077);
            Ok(())
        });
    }
    let result = supervise(&mut command, &stop);

    // Close while privileged.
    let close = session
        .close(Flag::NONE)
        .map_err(|_| invalid("PAM session cleanup failed"));
    result?;
    close?;
    Ok(())
}

fn executable(program: &str) -> io::Result<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return Ok(path.to_owned());
    }

    if program.contains('/') {
        return Err(invalid("Session executable must not be relative"));
    }

    PATH.split(':')
        .map(|dir| Path::new(dir).join(program))
        .find(|p| p.is_file())
        .ok_or_else(|| invalid("Session executable not found"))
}

fn desktop_command(desktop: &Desktop, vt: u32) -> io::Result<Command> {
    let program = executable(&desktop.argv[0])?;
    if desktop.kind == "wayland" {
        let mut command = Command::new(program);
        command.args(&desktop.argv[1..]);
        return Ok(command);
    }

    // startx creates an Xauthority cookie and supervises Xorg through xinit.
    let display = (0..64)
        .find(|n| {
            !Path::new(&format!("/tmp/.X{n}-lock")).exists()
                && !Path::new(&format!("/tmp/.X11-unix/X{n}")).exists()
        })
        .ok_or_else(|| invalid("No free X display"))?;
    let mut command = Command::new("/usr/bin/startx");
    command
        .arg(program)
        .args(&desktop.argv[1..])
        .args(["--", "/usr/bin/Xorg"])
        .arg(format!(":{display}"))
        .arg(format!("vt{vt}"))
        .args(["-keeptty", "-nolisten", "tcp"]);
    Ok(command)
}

fn supervise(command: &mut Command, stop: &AtomicBool) -> io::Result<()> {
    let mut child = command.spawn()?;
    let group = Pid::from_raw(child.id() as i32);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(error) => break Err(error),
            Ok(None) if stop.load(Ordering::Relaxed) => {
                let _ = killpg(group, Signal::SIGTERM);
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    if let Some(status) = child.try_wait()? {
                        return finish(group, status.success());
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                let _ = killpg(group, Signal::SIGKILL);
                break child.wait();
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
        }
    };

    // Also stop children left by a failed launcher.
    let _ = killpg(group, Signal::SIGTERM);
    finish(group, status?.success())
}

fn finish(group: Pid, success: bool) -> io::Result<()> {
    let _ = killpg(group, Signal::SIGTERM);
    if success {
        Ok(())
    } else {
        Err(invalid("Desktop session exited unsuccessfully"))
    }
}

struct TtyOwner {
    tty: File,
    uid: u32,
    gid: u32,
}

impl TtyOwner {
    fn give_to(tty: &File, user: &User) -> io::Result<Self> {
        let meta = tty.metadata()?;
        let owner = Self {
            tty: tty.try_clone()?,
            uid: meta.uid(),
            gid: meta.gid(),
        };
        nix::unistd::fchown(tty, Some(user.uid), Some(user.gid))?;
        Ok(owner)
    }
}

impl Drop for TtyOwner {
    fn drop(&mut self) {
        let _ = nix::unistd::fchown(&self.tty, Some(self.uid.into()), Some(self.gid.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_launch_preserves_arguments_without_shell() {
        let desktop = Desktop {
            argv: vec!["/bin/true".into(), "two words".into(), "$(id)".into()],
            kind: "wayland",
            name: "test".into(),
            desktops: "Test".into(),
        };
        let command = desktop_command(&desktop, 8).unwrap();
        assert_eq!(command.get_program(), "/bin/true");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["two words", "$(id)"]
        );
        assert!(executable("./local-session").is_err());
    }

    #[test]
    fn x11_launch_has_explicit_server_and_vt() {
        let desktop = Desktop {
            argv: vec!["/bin/true".into()],
            kind: "x11",
            name: "test".into(),
            desktops: "Test".into(),
        };
        let command = desktop_command(&desktop, 8).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/startx");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(&args[..3], ["/bin/true", "--", "/usr/bin/Xorg"]);
        assert_eq!(&args[4..], ["vt8", "-keeptty", "-nolisten", "tcp"]);
    }

    #[test]
    fn supervisor_handles_exit_failure_and_cancellation() {
        let stop = AtomicBool::new(false);
        assert!(supervise(Command::new("/bin/true").process_group(0), &stop).is_ok());
        assert!(supervise(Command::new("/bin/false").process_group(0), &stop).is_err());
        assert!(
            supervise(
                Command::new("/nonexistent-gxdm-test").process_group(0),
                &stop
            )
            .is_err()
        );
        stop.store(true, Ordering::Relaxed);
        assert!(supervise(Command::new("/bin/sleep").arg("30").process_group(0), &stop).is_err());
    }
}
