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

//! Isolate PAM and the desktop from the long-lived greeter process.

use super::{desktop::invalid, logind};
use nix::unistd::{Uid, getpgrp, tcsetpgrp};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::MetadataExt, process::CommandExt},
    },
    path::PathBuf,
    process::{Command, Stdio},
};
use zeroize::Zeroizing;

pub const WORKER_ARG: &str = "--session-worker";
const MAX_FIELD: usize = 16384;

// Do not derive Debug: a request contains a password.
pub struct Request {
    pub username: String,
    pub password: Zeroizing<String>,
    pub session: PathBuf,
}

impl Request {
    pub fn write_to(&self, mut writer: impl Write) -> io::Result<()> {
        for field in [
            self.username.as_str(),
            self.password.as_str(),
            self.session
                .to_str()
                .ok_or_else(|| invalid("Invalid session path"))?,
        ] {
            if field.len() > MAX_FIELD {
                return Err(invalid("Login field is too long"));
            }
            writer.write_all(&(field.len() as u32).to_be_bytes())?;
            writer.write_all(field.as_bytes())?;
        }
        Ok(())
    }

    pub fn read_from(mut reader: impl Read) -> io::Result<Self> {
        fn field(reader: &mut impl Read) -> io::Result<Zeroizing<String>> {
            let mut length = [0; 4];
            reader.read_exact(&mut length)?;
            let length = u32::from_be_bytes(length) as usize;
            if length > MAX_FIELD {
                return Err(invalid("Login field is too long"));
            }
            let mut bytes = Zeroizing::new(vec![0; length]);
            reader.read_exact(&mut bytes)?;
            let value = std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid login text"))?;
            if value.contains('\0') {
                return Err(invalid("NUL in login text"));
            }
            Ok(Zeroizing::new(value.to_owned()))
        }
        let username = field(&mut reader)?.to_string();
        let password = field(&mut reader)?;
        let session = PathBuf::from(field(&mut reader)?.as_str());
        Ok(Self {
            username,
            password,
            session,
        })
    }
}

// A dedicated system service is required. Never reuse a sudo/SSH/desktop session.
pub fn check_host() -> io::Result<(File, u32)> {
    if !Uid::effective().is_root() || !Uid::current().is_root() {
        return Err(invalid(
            "Login requires the root system service (not a setuid binary)",
        ));
    }
    if logind::current_session()?.is_some() {
        return Err(invalid(
            "Start from a dedicated system service, not an existing login session",
        ));
    }
    let tty = File::options().read(true).write(true).open("/dev/tty")?;
    // TIOCGDEV resolves /dev/tty to the actual controlling terminal device.
    let mut device: libc::c_uint = 0;
    if unsafe { libc::ioctl(tty.as_raw_fd(), libc::TIOCGDEV, &mut device) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let vt = libc::minor(device.into());
    if libc::major(device.into()) != 4 || !(1..=63).contains(&vt) {
        return Err(invalid("Login requires a Linux virtual terminal"));
    }
    let tty = File::options()
        .read(true)
        .write(true)
        .open(format!("/dev/tty{vt}"))?;
    Ok((tty, vt))
}

pub fn start(request: Request, stop: &std::sync::atomic::AtomicBool) -> io::Result<bool> {
    let (tty, _) = check_host()?;
    let attributes = nix::sys::termios::tcgetattr(&tty)?;
    let metadata = tty.metadata()?;
    let saved_tty = tty.try_clone()?;
    // Preserve Linux console modes too: a crashed compositor may leave graphics
    // or raw keyboard mode enabled even after its logind devices are released.
    let mut display_mode: libc::c_int = 0;
    let mut keyboard_mode: libc::c_int = 0;
    unsafe {
        if libc::ioctl(tty.as_raw_fd(), 0x4b3b, &mut display_mode) < 0
            || libc::ioctl(tty.as_raw_fd(), 0x4b44, &mut keyboard_mode) < 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    // Ignore job-control stops while handing the terminal back to the greeter.
    // This process is single-threaded; children reset this disposition before exec.
    let old = unsafe {
        nix::sys::signal::signal(
            nix::sys::signal::Signal::SIGTTOU,
            nix::sys::signal::SigHandler::SigIgn,
        )?
    };
    struct Restore {
        tty: File,
        signal: nix::sys::signal::SigHandler,
        attributes: nix::sys::termios::Termios,
        uid: u32,
        gid: u32,
        mode: u32,
        display_mode: libc::c_int,
        keyboard_mode: libc::c_int,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = tcsetpgrp(&self.tty, getpgrp());
            let _ = nix::unistd::fchown(&self.tty, Some(self.uid.into()), Some(self.gid.into()));
            let _ = nix::sys::stat::fchmod(
                &self.tty,
                nix::sys::stat::Mode::from_bits_truncate(self.mode),
            );
            let _ = nix::sys::termios::tcsetattr(
                &self.tty,
                nix::sys::termios::SetArg::TCSAFLUSH,
                &self.attributes,
            );
            unsafe {
                // KDSETMODE and KDSKBMODE restore the saved display/keyboard modes.
                libc::ioctl(self.tty.as_raw_fd(), 0x4b3a, self.display_mode);
                libc::ioctl(self.tty.as_raw_fd(), 0x4b45, self.keyboard_mode);
                let _ = nix::sys::signal::signal(nix::sys::signal::Signal::SIGTTOU, self.signal);
            }
        }
    }
    let _restore = Restore {
        tty: saved_tty,
        signal: old,
        attributes,
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.mode(),
        display_mode,
        keyboard_mode,
    };
    let mut worker = Command::new(std::env::current_exe()?)
        .arg(WORKER_ARG)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let transfer = tcsetpgrp(&tty, nix::unistd::Pid::from_raw(worker.id() as i32))
        .map_err(io::Error::from)
        .and_then(|()| request.write_to(worker.stdin.take().unwrap()));
    drop(request);
    if let Err(error) = transfer {
        let _ = worker.kill();
        let _ = worker.wait();
        return Err(error);
    }
    let mut stopping = None;
    let result = loop {
        if let Some(status) = worker.try_wait()? {
            break status;
        }
        if stop.load(std::sync::atomic::Ordering::Relaxed) && stopping.is_none() {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(worker.id() as i32),
                nix::sys::signal::Signal::SIGTERM,
            );
            stopping = Some(std::time::Instant::now());
        }
        if stopping.is_some_and(|time| time.elapsed().as_secs() >= 10) {
            let _ = worker.kill();
            break worker.wait()?;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    // The worker reports only its logind ID, never credentials or desktop output.
    let mut session = String::new();
    worker
        .stdout
        .take()
        .unwrap()
        .take(256)
        .read_to_string(&mut session)?;
    let session = session.trim();
    if !session.is_empty() && session.bytes().all(|c| c.is_ascii_alphanumeric()) {
        // Remove lingering processes even if the desktop exited or the worker crashed.
        let _ = Command::new("/usr/bin/loginctl")
            .args(["terminate-session", session])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    Ok(result.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_request_roundtrip_and_limits() {
        let request = Request {
            username: "user名".into(),
            password: Zeroizing::new("secret\n".into()),
            session: "/usr/share/xsessions/gxde.desktop".into(),
        };
        let mut bytes = Zeroizing::new(Vec::new());
        request.write_to(&mut *bytes).unwrap();
        let copy = Request::read_from(bytes.as_slice()).unwrap();
        assert_eq!(copy.username, request.username);
        assert_eq!(copy.password.as_str(), request.password.as_str());
        assert_eq!(copy.session, request.session);
        assert!(Request::read_from(&[0xff; 4][..]).is_err());
        assert!(Request::read_from(&bytes[..bytes.len() - 1]).is_err());
        assert!(Request::read_from(&[0, 0, 0, 1, 0][..]).is_err());
    }
}
