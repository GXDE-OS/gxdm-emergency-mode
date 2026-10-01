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
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

// Disable core dumps before accepting credentials in either process.
fn main() -> io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if std::env::args().nth(1).as_deref() == Some(backend::login::WORKER_ARG) {
        return backend::worker::run().map_err(|error| io::Error::other(error.to_string()));
    }
    if std::env::args().nth(1).as_deref() == Some(backend::x11::CLIENT_ARG) {
        return backend::x11::run();
    }
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        signal_hook::flag::register(sig, Arc::clone(&stop))?;
    }
    ratatui::run(|terminal| run(terminal, &stop))
}

// Handle input while the greeter is running.
fn run(terminal: &mut DefaultTerminal, stop: &AtomicBool) -> io::Result<()> {
    let mut greeter = render::greeter::Greeter::new(backend::sessions::session_names());
    while !stop.load(Ordering::Relaxed) {
        terminal.draw(|frame| render::greeter::draw(frame, &greeter))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && greeter.handle_key(key)
        {
            return Ok(());
        }
        if let Some(request) = greeter.take_login() {
            ratatui::try_restore()?;
            let result = backend::login::start(request, stop);
            *terminal = ratatui::try_init()?;
            greeter.set_status(match result {
                Ok(true) => "Session ended".into(),
                Ok(false) => "Login/session failed; see service journal".into(),
                Err(error) => error.to_string(),
            });
        }
    }
    Ok(())
}
