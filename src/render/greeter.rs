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

///! Foreground greeter.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const LIGHT_BLUE: Color = Color::Rgb(155, 205, 245);

// Draw UI.
pub fn draw(frame: &mut Frame) {
    let [top, _body, bottom] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    draw_top(frame, top);
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
            Paragraph::new(" Ctrl-C: quit ").style(style),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn bars_render_at_top_and_bottom() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(draw).unwrap();
        let buffer = terminal.backend().buffer();
        let row = |y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>();

        assert!(row(0).contains("GXDE Display Manager"));
        assert!(row(0).contains("RECUSE MODE"));
        assert!(row(1).starts_with('╭'));
        assert!(row(1).ends_with('╮'));
        assert!(row(22).chars().all(|c| c == '─'));
        assert!(row(23).contains("Ctrl-C: quit"));
        assert!(row(10).trim().is_empty());
    }

    #[test]
    fn narrow_terminal_does_not_panic() {
        let mut terminal = Terminal::new(TestBackend::new(1, 2)).unwrap();
        terminal.draw(draw).unwrap();
    }
}
