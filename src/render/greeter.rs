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

//! Foreground greeter.

use std::{collections::BTreeMap, path::PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

const LIGHT_BLUE: Color = Color::Rgb(155, 205, 245);
const SESSION: usize = 2;
const LOGIN: usize = 3;
const FOCUS_COUNT: usize = 4;
const FORM_HEIGHT: u16 = 16;

pub struct Greeter {
    username: String,
    password: String,
    sessions: Vec<(String, PathBuf)>,
    selected: usize,
    focus: usize,
    status: &'static str,
}

impl Greeter {
    // Init greeter.
    pub fn new(sessions: BTreeMap<String, PathBuf>) -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            sessions: sessions.into_iter().collect(),
            selected: 0,
            focus: 0,
            status: "",
        }
    }

    // Login logic.
    fn login(&mut self) {
        self.status = if self.username.is_empty() {
            "Username is required"
        } else {
            "Authentication not connected"
        };
    }

    // Return true only for Ctrl-C.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        match key.code {
            KeyCode::Enter if self.focus == LOGIN => self.login(),
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = (self.focus + FOCUS_COUNT - 1) % FOCUS_COUNT;
            }

            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                self.focus = (self.focus + 1) % FOCUS_COUNT;
            }

            KeyCode::Left if self.focus == SESSION && !self.sessions.is_empty() => {
                self.selected = (self.selected + self.sessions.len() - 1) % self.sessions.len();
            }

            KeyCode::Right if self.focus == SESSION && !self.sessions.is_empty() => {
                self.selected = (self.selected + 1) % self.sessions.len();
            }

            _ if self.focus < SESSION => {
                let value = if self.focus == 0 {
                    &mut self.username
                } else {
                    &mut self.password
                };
                match key.code {
                    KeyCode::Char(c) if !c.is_control() => value.push(c),
                    KeyCode::Backspace => {
                        value.pop();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        false
    }
}

// Draw login dialog.
fn draw_form(frame: &mut Frame, body: Rect, greeter: &Greeter) {
    if body.width < 36 || body.height < FORM_HEIGHT {
        frame.render_widget(Paragraph::new("Enlarge terminal to show login form"), body);
        return;
    }

    let width = body.width.min(58);
    let area = Rect::new(
        body.x + (body.width - width) / 2,
        body.y + (body.height - FORM_HEIGHT) / 2,
        width,
        FORM_HEIGHT,
    );

    let block = Block::default()
        .title(" Login ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(LIGHT_BLUE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    let session = greeter
        .sessions
        .get(greeter.selected)
        .map(|(name, _)| {
            format!(
                "< {} > ({}/{})",
                name,
                greeter.selected + 1,
                greeter.sessions.len()
            )
        })
        .unwrap_or_else(|| "No sessions available".to_owned());

    let password = "*".repeat(greeter.password.chars().count());
    for (index, (title, value)) in [
        ("Username", greeter.username.as_str()),
        ("Password", password.as_str()),
        ("Session", session.as_str()),
    ]
    .into_iter()
    .enumerate()
    {
        let style = focus_style(greeter, index);
        let line = Line::from(value);
        let scroll = if index < 2 {
            line.width()
                .saturating_sub(usize::from(rows[index].width.saturating_sub(3)))
        } else {
            0
        };
        frame.render_widget(
            Paragraph::new(line)
                .scroll((0, scroll.min(u16::MAX as usize) as u16))
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(style),
                ),
            rows[index],
        );
    }
    
    let marker = if greeter.focus == LOGIN { "❃ " } else { "" };
    frame.render_widget(
        Paragraph::new(format!("{marker}Login"))
            .alignment(Alignment::Center)
            .style(focus_style(greeter, LOGIN)),
        Rect::new(rows[3].x, rows[3].y + 1, rows[3].width, 1),
    );
    frame.render_widget(Paragraph::new(greeter.status), rows[5]);
}

// Highlight the focused item in yellow.
fn focus_style(greeter: &Greeter, index: usize) -> Style {
    let color = if greeter.focus == index {
        Color::Yellow
    } else {
        LIGHT_BLUE
    };
    Style::default().fg(color)
}

// Draw UI.
pub fn draw(frame: &mut Frame, greeter: &Greeter) {
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    draw_top(frame, top);
    draw_form(frame, body, greeter);
    draw_bottom(frame, bottom);
}

// Draw the title bar.
fn draw_top(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let style = Style::default().fg(LIGHT_BLUE);
    let title = Line::from(vec![
        Span::styled(
            "  GXDE Display Manager ",
            style.add_modifier(Modifier::BOLD),
        ),
        Span::styled("RECUSE MODE", style.add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(
        Paragraph::new(title),
        Rect::new(area.x, area.y, area.width, 1),
    );

    if area.height > 1 {
        let border = if area.width == 1 {
            "╭".to_owned()
        } else {
            format!("╭{}╮", "─".repeat(usize::from(area.width - 2)))
        };

        frame.render_widget(
            Paragraph::new(border).style(style),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}

// Draw Status bar.
fn draw_bottom(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let style = Style::default().fg(Color::Green);
    frame.render_widget(
        Paragraph::new("─".repeat(usize::from(area.width))).style(style),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(" Ctrl-C: quit ※  ↑/↓: Switch fields ※  ←/→: Switch sessions")
                .style(style),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn press(greeter: &mut Greeter, code: KeyCode) -> bool {
        greeter.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn edits_fields_and_selects_sessions() {
        let mut greeter = Greeter::new(BTreeMap::from([
            ("A".into(), PathBuf::from("a.desktop")),
            ("B".into(), PathBuf::from("b.desktop")),
        ]));
        for c in "user".chars() {
            assert!(!press(&mut greeter, KeyCode::Char(c)));
        }
        press(&mut greeter, KeyCode::Backspace);
        assert_eq!(greeter.username, "user");
        press(&mut greeter, KeyCode::Tab);
        for c in "secret".chars() {
            press(&mut greeter, KeyCode::Char(c));
        }
        assert_eq!(greeter.password, "secret");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &greeter)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("******"));
        assert!(!text.contains("secret"));
        press(&mut greeter, KeyCode::Down);
        press(&mut greeter, KeyCode::Left);
        assert_eq!(
            greeter.sessions[greeter.selected].1,
            PathBuf::from("b.desktop")
        );
        press(&mut greeter, KeyCode::Right);
        assert_eq!(greeter.selected, 0);
        press(&mut greeter, KeyCode::Up);
        assert_eq!(greeter.focus, 1);
        press(&mut greeter, KeyCode::BackTab);
        assert_eq!(greeter.focus, 0);
    }

    #[test]
    fn arrows_wrap_focus_and_login_button_gives_feedback() {
        let mut greeter = Greeter::new(BTreeMap::new());
        press(&mut greeter, KeyCode::Up);
        assert_eq!(greeter.focus, LOGIN);
        press(&mut greeter, KeyCode::Enter);
        assert_eq!(greeter.status, "Username is required");
        press(&mut greeter, KeyCode::Down);
        assert_eq!(greeter.focus, 0);
        press(&mut greeter, KeyCode::Char('u'));
        press(&mut greeter, KeyCode::Up);
        press(&mut greeter, KeyCode::Enter);
        assert_eq!(greeter.focus, LOGIN);
        assert_eq!(greeter.status, "Authentication not connected");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &greeter)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("Login"));
        assert!(text.contains("Authentication not connected"));
    }

    #[test]
    fn form_fields_keep_rounded_corners() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| draw(frame, &Greeter::new(BTreeMap::new())))
            .unwrap();
        let buffer = terminal.backend().buffer();
        // Keep rounded borders for the form and input fields only.
        for (x, y, width) in [(11, 4, 58), (12, 5, 56)] {
            assert_eq!(buffer[(x, y)].symbol(), "╭");
            assert_eq!(buffer[(x + width - 1, y)].symbol(), "╮");
        }
    }

    #[test]
    fn focus_is_highlighted_and_marker_is_only_for_login() {
        let mut greeter = Greeter::new(BTreeMap::new());
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        // Top-left corners of the three fields, then the login marker cell.
        for (focus, (x, y)) in [(12, 5), (12, 8), (12, 11), (37, 15)]
            .into_iter()
            .enumerate()
        {
            terminal.draw(|frame| draw(frame, &greeter)).unwrap();
            let buffer = terminal.backend().buffer();
            let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
            assert_eq!(text.matches('❃').count(), usize::from(focus == LOGIN));
            assert_eq!(buffer[(x, y)].fg, Color::Yellow);
            for y in 14..17 {
                let row: String = (12..68).map(|x| buffer[(x, y)].symbol()).collect();
                assert!(!row.contains(['╭', '╮', '╰', '╯', '─', '│']));
            }
            press(&mut greeter, KeyCode::Down);
        }
    }

    #[test]
    fn only_ctrl_c_exits_and_empty_sessions_are_safe() {
        let mut greeter = Greeter::new(BTreeMap::new());
        for code in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Char('c')] {
            assert!(!press(&mut greeter, code));
        }
        press(&mut greeter, KeyCode::Tab);
        press(&mut greeter, KeyCode::Tab);
        for code in [KeyCode::Left, KeyCode::Right] {
            assert!(!press(&mut greeter, code));
        }
        assert_eq!(greeter.selected, 0);
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!greeter.handle_key(KeyEvent {
            kind: KeyEventKind::Release,
            ..key
        }));
        assert!(greeter.handle_key(key));
    }

    #[test]
    fn bars_render_at_top_and_bottom() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| draw(frame, &Greeter::new(BTreeMap::new())))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row = |y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>();

        assert!(row(0).contains("GXDE Display Manager"));
        assert!(row(0).contains("RECUSE MODE"));
        assert!(row(1).starts_with('╭'));
        assert!(row(1).ends_with('╮'));
        assert!(row(22).chars().all(|c| c == '─'));
        assert!(row(23).contains("Ctrl-C: quit"));
    }

    #[test]
    fn narrow_terminal_does_not_panic() {
        let mut terminal = Terminal::new(TestBackend::new(1, 2)).unwrap();
        terminal
            .draw(|frame| draw(frame, &Greeter::new(BTreeMap::new())))
            .unwrap();
    }
}
