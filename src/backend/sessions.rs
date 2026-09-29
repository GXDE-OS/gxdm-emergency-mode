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

//! Desktop session utility.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SESSION_DIRS: [&str; 2] = ["/usr/share/xsessions", "/usr/share/wayland-sessions"];

// Read every single .desktop file and get its name, then do the mapping.
fn collect_sessions<'a>(dirs: impl IntoIterator<Item = &'a Path>) -> BTreeMap<String, PathBuf> {
    let mut sessions = BTreeMap::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };

        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
            .collect();

        paths.sort();
        for path in paths {
            if let Some(name) = read_name(&path) {
                sessions.entry(name).or_insert(path);
            }
        }
    }
    sessions
}

// Gets DesktopEntry from .desktop files.
fn read_name(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut name = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry && let Some((key, value)) = line.split_once('=') {
            match (key.trim(), value.trim()) {
                ("Hidden" | "NoDisplay", "true") => return None,
                ("Name", value) => name = Some(value.to_owned()),
                _ => {}
            }
        }
    }
    name.filter(|name| !name.is_empty())
}

// Maps session display name to their .desktop file.
pub fn session_names() -> BTreeMap<String, PathBuf> {
    collect_sessions(SESSION_DIRS.iter().map(Path::new))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a new empty folder for one test.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gxdm-sessions-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn maps_names_to_desktop_files() {
        let x11 = temp_dir("x11");
        let wayland = temp_dir("wayland");
        fs::write(x11.join("gxde.desktop"), "[Desktop Entry]\nName=GXDE\n").unwrap();
        fs::write(
            x11.join("hidden.desktop"),
            "[Desktop Entry]\nName=Hidden\nNoDisplay=true\n",
        )
        .unwrap();
        fs::write(x11.join("notes.txt"), "[Desktop Entry]\nName=Notes\n").unwrap();
        fs::write(
            wayland.join("flake.desktop"),
            "[Desktop Entry]\nName = FlakeWM\n",
        )
        .unwrap();
        // Same name as the X11 session, so the X11 one must be kept.
        fs::write(wayland.join("gxde.desktop"), "[Desktop Entry]\nName=GXDE\n").unwrap();
        // `Name` in another group must be ignored.
        fs::write(
            wayland.join("other.desktop"),
            "[Desktop Action x]\nName=Other\n",
        )
        .unwrap();

        let sessions =
            collect_sessions([x11.as_path(), wayland.as_path(), Path::new("/nonexistent")]);
        let expected = BTreeMap::from([
            ("FlakeWM".to_owned(), wayland.join("flake.desktop")),
            ("GXDE".to_owned(), x11.join("gxde.desktop")),
        ]);
        assert_eq!(sessions, expected);

        fs::remove_dir_all(x11).unwrap();
        fs::remove_dir_all(wayland).unwrap();
    }
}
