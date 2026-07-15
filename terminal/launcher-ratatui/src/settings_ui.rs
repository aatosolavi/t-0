//! Settings screen: layout, draw, hit-test, navigation keys/mouse.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::new_project::display_width;
use crate::{inset, Theme, ACCENT, APP_NAME};

pub const ITEM_COUNT: usize = 6;

#[derive(Clone, Debug)]
pub struct SettingsLayout {
    pub help: Rect,
    pub options: Rect,
    pub status: Rect,
}

pub fn layout(inner: Rect) -> SettingsLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(inner);
    SettingsLayout {
        help: chunks[0],
        options: chunks[1],
        status: chunks[2],
    }
}

/// Hit-test option rows (0..ITEM_COUNT-1). Help/status → None.
pub fn hit_test(lay: &SettingsLayout, row: u16, col: u16) -> Option<usize> {
    if col < lay.options.x || col >= lay.options.x.saturating_add(lay.options.width) {
        return None;
    }
    if row < lay.options.y || row >= lay.options.y.saturating_add(lay.options.height) {
        return None;
    }
    let idx = usize::from(row - lay.options.y);
    if idx < ITEM_COUNT {
        Some(idx)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAction {
    None,
    Back,
    Nudge(i32),
    Activate,
}

pub fn handle_key(key: KeyEvent, selected: usize) -> (usize, SettingsAction) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('s') | KeyCode::Char('S') => (selected, SettingsAction::Back),
        KeyCode::Down | KeyCode::Char('j') => {
            let s = (selected + 1).min(ITEM_COUNT.saturating_sub(1));
            (s, SettingsAction::None)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let s = selected.saturating_sub(1);
            (s, SettingsAction::None)
        }
        KeyCode::Left => (selected, SettingsAction::Nudge(-1)),
        KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
            (selected, SettingsAction::Nudge(1))
        }
        _ => (selected, SettingsAction::None),
    }
}

pub fn handle_mouse(
    mouse: MouseEvent,
    selected: usize,
    lay: &SettingsLayout,
) -> (usize, SettingsAction) {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            let s = (selected + 1).min(ITEM_COUNT.saturating_sub(1));
            (s, SettingsAction::None)
        }
        MouseEventKind::ScrollUp => (selected.saturating_sub(1), SettingsAction::None),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(row) = hit_test(lay, mouse.row, mouse.column) {
                if row == selected {
                    (selected, SettingsAction::Activate)
                } else {
                    (row, SettingsAction::None)
                }
            } else {
                (selected, SettingsAction::None)
            }
        }
        _ => (selected, SettingsAction::None),
    }
}

/// Draw settings. `rows` are preformatted option strings (6 items).
pub fn draw(
    frame: &mut Frame<'_>,
    panel: Rect,
    t: Theme,
    rows: &[String],
    selected: usize,
    status: Option<&str>,
) -> SettingsLayout {
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        frame.area(),
    );
    let block = Block::default()
        .title(format!(" {APP_NAME} · Settings "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, panel);

    let inner = inset(panel, crate::PANEL_PAD_H, crate::PANEL_PAD_V);
    let lay = layout(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "←/→ cycle · enter open  · esc/s back",
            Style::default().fg(t.dim),
        ))),
        lay.help,
    );

    let mut lines = Vec::new();
    let w = lay.options.width as usize;
    for (i, row) in rows.iter().enumerate().take(ITEM_COUNT) {
        let is_sel = i == selected;
        let row_bg = if is_sel { t.surface } else { t.bg };
        let style = if is_sel {
            Style::default()
                .fg(t.text)
                .bg(row_bg)
                .add_modifier(Modifier::BOLD)
        } else if i >= 5 {
            Style::default().fg(t.dim).bg(row_bg)
        } else {
            Style::default().fg(t.soft).bg(row_bg)
        };
        let mut spans = vec![
            if is_sel {
                Span::styled("▌", Style::default().fg(ACCENT).bg(row_bg))
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            },
            Span::styled(format!(" {row}"), style),
        ];
        let used: usize = spans.iter().map(|s| display_width(s.content.as_ref())).sum();
        if used < w {
            spans.push(Span::styled(
                " ".repeat(w - used),
                Style::default().bg(row_bg),
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), lay.options);

    if let Some(status) = status {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                status.to_string(),
                Style::default().fg(ACCENT),
            ))),
            lay.status,
        );
    }
    lay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_options_rows() {
        let inner = Rect { x: 2, y: 2, width: 60, height: 12 };
        let lay = layout(inner);
        assert_eq!(hit_test(&lay, lay.help.y, lay.help.x + 1), None);
        assert_eq!(
            hit_test(&lay, lay.options.y, lay.options.x + 1),
            Some(0)
        );
        if lay.options.height > 1 {
            assert_eq!(
                hit_test(&lay, lay.options.y + 1, lay.options.x + 1),
                Some(1)
            );
        }
    }

    #[test]
    fn handle_key_esc_back() {
        let k = KeyEvent::from(KeyCode::Esc);
        let (_, a) = handle_key(k, 0);
        assert_eq!(a, SettingsAction::Back);
    }
}
