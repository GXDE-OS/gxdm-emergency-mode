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

//! Publish the X server environment before starting desktop services.

use super::desktop::invalid;
use nix::unistd::Uid;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::PathBuf,
    process::Command,
};

pub const CLIENT_ARG: &str = "--x11-client";
const VARIABLES: &[&str] = &[
    "DISPLAY",
    "XAUTHORITY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "XDG_SESSION_TYPE",
    "DESKTOP_SESSION",
];

pub fn run() -> io::Result<()> {
    if Uid::effective().is_root() || Uid::current().is_root() {
        return Err(invalid("The X11 client must run as the logged-in user"));
    }
    let desktop = client_command(std::env::args_os().skip(2))?;
    for key in ["DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS"] {
        if std::env::var_os(key).is_none_or(|value| value.is_empty()) {
            return Err(invalid("Missing X11 client environment"));
        }
    }

    // Only xinit's client has the final DISPLAY and XAUTHORITY. Updating these
    // in the worker is too early; D-Bus services would start without X access.
    let status = Command::new("/usr/bin/dbus-update-activation-environment")
        .arg("--systemd")
        .args(
            VARIABLES
                .iter()
                .filter(|key| std::env::var_os(key).is_some()),
        )
        .status()?;
    if !status.success() {
        return Err(io::Error::other(
            "Could not publish X11 activation environment",
        ));
    }
    let (mut session, _wrapper) = session_command(&desktop)?;
    if session.status()?.success() {
        Ok(())
    } else {
        Err(io::Error::other("Xsession exited unsuccessfully"))
    }
}

// Debian Xsession accepts one command string, not an argv array. A private
// script preserves argument boundaries without asking Xsession to parse them.
fn session_command(desktop: &Command) -> io::Result<(Command, Option<SessionScript>)> {
    let mut command = Command::new("/etc/X11/Xsession");
    let program = desktop.get_program();
    if desktop.get_args().len() == 0
        && !program
            .as_bytes()
            .iter()
            .any(|b| b.is_ascii_whitespace() || b"*?[".contains(b))
    {
        command.arg(program);
        return Ok((command, None));
    }
    let script = SessionScript::create(desktop)?;
    command.arg(&script.0);
    Ok((command, Some(script)))
}

struct SessionScript(PathBuf);

impl SessionScript {
    fn create(desktop: &Command) -> io::Result<Self> {
        // The worker has already checked this runtime directory's ownership.
        let runtime = PathBuf::from(format!("/run/user/{}", Uid::current()));
        for attempt in 0..128 {
            let path = runtime.join(format!("gxdm-xsession-{}-{attempt}", std::process::id()));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o700)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let script = Self(path);
            file.write_all(&script_body(desktop))?;
            return Ok(script);
        }
        Err(io::Error::other("Could not create Xsession launcher"))
    }
}

impl Drop for SessionScript {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn script_body(desktop: &Command) -> Vec<u8> {
    let mut body =
        b"#!/bin/sh\n# Remove the launcher before the desktop starts.\n/bin/rm -f -- \"$0\"\nexec"
            .to_vec();
    for arg in std::iter::once(desktop.get_program()).chain(desktop.get_args()) {
        body.push(b' ');
        quote_arg(&mut body, arg);
    }
    body.push(b'\n');
    body
}

fn quote_arg(out: &mut Vec<u8>, arg: &OsStr) {
    out.push(b'\'');
    for byte in arg.as_bytes() {
        if *byte == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(*byte);
        }
    }
    out.push(b'\'');
}

fn client_command(mut args: impl Iterator<Item = OsString>) -> io::Result<Command> {
    let program = args
        .next()
        .ok_or_else(|| invalid("Missing X11 desktop command"))?;
    if !std::path::Path::new(&program).is_absolute() {
        return Err(invalid("X11 desktop command must be absolute"));
    }
    let mut command = Command::new(program);
    command.args(args);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_preserves_arguments_without_a_shell() {
        let command = client_command(
            ["/bin/true", "two words", "$(id)", "--option"]
                .into_iter()
                .map(OsString::from),
        )
        .unwrap();
        assert_eq!(command.get_program(), "/bin/true");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["two words", "$(id)", "--option"]
        );
    }

    #[test]
    fn client_rejects_missing_or_relative_program() {
        assert!(client_command(std::iter::empty()).is_err());
        assert!(client_command([OsString::from("startdde")].into_iter()).is_err());
    }

    #[test]
    fn wrapper_quotes_shell_metacharacters_and_empty_arguments() {
        let args = [
            "",
            "two words",
            "$(id)",
            "'quoted'",
            "*",
            "line\nbreak",
            "--",
        ];
        let mut command = Command::new("/usr/bin/printf");
        command.arg("%s\\0").args(args);
        let body = script_body(&command);
        // Skip the self-removal line: this test creates no launcher file.
        let exec = body.splitn(4, |b| *b == b'\n').nth(3).unwrap();
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(OsStr::from_bytes(exec))
            .output()
            .unwrap();
        assert!(output.status.success());
        let expected: Vec<u8> = args.iter().flat_map(|s| s.bytes().chain([0])).collect();
        assert_eq!(output.stdout, expected);
    }

    #[test]
    fn plain_desktop_uses_xsession_without_a_wrapper() {
        let (command, wrapper) = session_command(&Command::new("/usr/bin/startdde")).unwrap();
        assert_eq!(command.get_program(), "/etc/X11/Xsession");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["/usr/bin/startdde"]
        );
        assert!(wrapper.is_none());
    }
}
