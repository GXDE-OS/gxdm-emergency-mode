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

mod backend;
mod render;

use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use std::io;

// Start the terminal UI.
fn main() -> io::Result<()> {
    ratatui::run(run)
}

// Handle input while the greeter is running.
fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut greeter = render::greeter::Greeter::new(backend::sessions::session_names());
    loop {
        terminal.draw(|frame| render::greeter::draw(frame, &greeter))?;
        if let Event::Key(key) = event::read()?
            && greeter.handle_key(key)
        {
            return Ok(());
        }
    }
}
