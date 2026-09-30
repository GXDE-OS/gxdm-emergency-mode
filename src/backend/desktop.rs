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

//! Read trusted desktop launch commands without invoking a shell.

use std::{collections::BTreeMap, fs, io, os::unix::fs::MetadataExt, path::Path};

pub struct Desktop {
    pub argv: Vec<String>,
    pub kind: &'static str,
    pub name: String,
    pub desktops: String,
}

pub fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

impl Desktop {
    pub fn read(path: &Path) -> io::Result<Self> {
        let path = path.canonicalize()?;
        let kind = match path.parent().and_then(Path::to_str) {
            Some("/usr/share/xsessions") => "x11",
            Some("/usr/share/wayland-sessions") => "wayland",
            _ => return Err(invalid("Session must be in a system session directory")),
        };
        // Only administrators may supply launch commands or replace their parents.
        for component in path.ancestors() {
            let meta = fs::metadata(component)?;
            if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
                return Err(invalid(
                    "Session path must be root-owned and not writable by others",
                ));
            }
        }
        if path.extension().is_none_or(|ext| ext != "desktop")
            || !path.is_file()
            || fs::metadata(&path)?.len() > 65536
        {
            return Err(invalid("Invalid desktop file"));
        }
        let text = fs::read_to_string(&path)?;
        let mut values = BTreeMap::new();
        let mut in_entry = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
            } else if in_entry
                && !line.starts_with('#')
                && let Some((key, value)) = line.split_once('=')
            {
                values.insert(key.trim(), value.trim());
            }
        }
        if ["Hidden", "NoDisplay"]
            .iter()
            .any(|key| values.get(key) == Some(&"true"))
        {
            return Err(invalid("Session is hidden"));
        }
        let exec = values
            .get("Exec")
            .ok_or_else(|| invalid("Session has no Exec"))?;
        let argv = parse_exec(exec)?;
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let desktops = values
            .get("DesktopNames")
            .unwrap_or(&"")
            .trim_end_matches(';')
            .replace(';', ":");
        Ok(Self {
            argv,
            kind,
            name,
            desktops,
        })
    }
}

// Session Exec lines normally have no field codes. Reject unsupported codes
// rather than guessing at shell syntax or expanding files in a login session.
fn parse_exec(value: &str) -> io::Result<Vec<String>> {
    let mut decoded = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        decoded.push(if c == '\\' {
            match chars.next() {
                Some('s') => ' ',
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\r',
                Some('\\') => '\\',
                _ => return Err(invalid("Invalid desktop string escape")),
            }
        } else {
            c
        });
    }
    let mut args = Vec::new();
    let mut arg = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = decoded.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => match chars.next() {
                Some(c @ ('"' | '`' | '$' | '\\')) => {
                    arg.push(c);
                    started = true;
                }
                _ => return Err(invalid("Invalid Exec escape")),
            },
            '%' => match chars.next() {
                Some('%') => {
                    arg.push('%');
                    started = true;
                }
                _ => return Err(invalid("Session Exec field codes are not supported")),
            },
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut arg));
                    started = false;
                }
            }
            c if c.is_control() || c == '\0' => return Err(invalid("Control character in Exec")),
            c => {
                arg.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(invalid("Unclosed Exec quote"));
    }
    if started {
        args.push(arg);
    }
    if args
        .first()
        .is_none_or(|s| s.is_empty() || s.starts_with('-') || s.contains('='))
    {
        return Err(invalid("Missing or invalid session executable"));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_is_argv_not_shell() {
        assert_eq!(
            parse_exec("/usr/bin/startdde").unwrap(),
            ["/usr/bin/startdde"]
        );
        assert_eq!(
            parse_exec("compositor --label \"two words\" \"\" %%").unwrap(),
            ["compositor", "--label", "two words", "", "%"]
        );
        assert_eq!(
            parse_exec("app $(id) ; rm").unwrap(),
            ["app", "$(id)", ";", "rm"]
        );
        for bad in ["", "-app", "A=B app", "app %u", "app \"bad", "app \\q"] {
            assert!(parse_exec(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_untrusted_session_directory() {
        assert!(Desktop::read(Path::new("/etc/passwd")).is_err());
    }
}
