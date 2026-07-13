//! New Project popup: layout (single source of truth), draw, hit-test.
//! Form state lives in `main` (`NewProjectForm`); this module only paints and maps geometry.

use std::path::Path;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::new_project::{
    clamp_notes_scroll, display_width, notes_lines, notes_viewport, pad_line, sliding_tail,
    NOTES_VIEWPORT_ROWS,
};
use crate::{
    init_agent_elevated, NewProjectField, NewProjectForm, Theme, ACCENT, ACCENT_ON, APP_NAME,
};

/// help(1) · top fields(4) · notes label(1) · notes box(N) · create(1) · status(1)
const CONTENT_ROWS: u16 = 1 + 4 + 1 + NOTES_VIEWPORT_ROWS + 1 + 1;

/// Geometry for the New Project modal — used by both draw and hit-test.
#[derive(Clone, Debug)]
pub struct NpLayout {
    /// Outer bordered popup (for mouse panel bounds).
    #[allow(dead_code)]
    pub popup: Rect,
    pub inner: Rect,
    pub help: Rect,
    pub name: Rect,
    pub parent: Rect,
    pub template: Rect,
    pub init_agent: Rect,
    pub notes_label: Rect,
    pub notes_box: Rect,
    pub create: Rect,
    pub status: Rect,
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect {
        x: area.x + horizontal,
        y: area.y + vertical,
        width: area.width.saturating_sub(horizontal * 2),
        height: area.height.saturating_sub(vertical * 2),
    }
}

/// Outer popup rect (always reserves a status row so geometry has one variant).
pub fn popup_rect(screen: Rect) -> Rect {
    let width = if screen.width >= 44 {
        screen.width.min(76).max(44)
    } else {
        screen.width.max(1)
    };
    let height = CONTENT_ROWS
        .saturating_add(2) // top + bottom border
        .min(screen.height.max(1));
    Rect {
        x: screen.x + screen.width.saturating_sub(width) / 2,
        y: screen.y + screen.height.saturating_sub(height) / 2,
        width: width.min(screen.width.max(1)),
        height,
    }
}

/// Build layout from the outer popup rect (inside borders).
pub fn layout(popup: Rect) -> NpLayout {
    // Horizontal pad 2, vertical pad 0 — border already occupies top/bottom of `popup`.
    let padded = inset(popup, 2, 0);
    let inner = Rect {
        x: padded.x,
        y: padded.y.saturating_add(1),
        width: padded.width,
        height: padded.height.saturating_sub(2),
    };

    let constraints = [
        Constraint::Length(1),                 // help
        Constraint::Length(1),                 // name
        Constraint::Length(1),                 // parent
        Constraint::Length(1),                 // template
        Constraint::Length(1),                 // init
        Constraint::Length(1),                 // notes label
        Constraint::Length(NOTES_VIEWPORT_ROWS), // notes box
        Constraint::Length(1),                 // create
        Constraint::Length(1),                 // status (always reserved)
    ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    NpLayout {
        popup,
        inner,
        help: chunks[0],
        name: chunks[1],
        parent: chunks[2],
        template: chunks[3],
        init_agent: chunks[4],
        notes_label: chunks[5],
        notes_box: chunks[6],
        create: chunks[7],
        status: chunks[8],
    }
}

fn row_in(rect: Rect, row: u16) -> bool {
    row >= rect.y && row < rect.y.saturating_add(rect.height)
}

/// Hit-test using the same layout as draw (no parallel geometry).
pub fn hit_test(lay: &NpLayout, row: u16, col: u16) -> Option<NewProjectField> {
    if lay.inner.height == 0
        || col < lay.inner.x
        || col >= lay.inner.x.saturating_add(lay.inner.width)
        || row < lay.inner.y
        || row >= lay.inner.y.saturating_add(lay.inner.height)
    {
        return None;
    }
    if row_in(lay.help, row) {
        return None;
    }
    if row_in(lay.name, row) {
        return Some(NewProjectField::Name);
    }
    if row_in(lay.parent, row) {
        return Some(NewProjectField::Parent);
    }
    if row_in(lay.template, row) {
        return Some(NewProjectField::Template);
    }
    if row_in(lay.init_agent, row) {
        return Some(NewProjectField::InitAgent);
    }
    if row_in(lay.notes_label, row) || row_in(lay.notes_box, row) {
        return Some(NewProjectField::Notes);
    }
    if row_in(lay.create, row) {
        return Some(NewProjectField::Create);
    }
    // status row and any filler — not a field
    None
}

fn field_row_style(selected: bool, t: &Theme) -> Style {
    if selected {
        Style::default()
            .fg(ACCENT_ON)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(t.soft).bg(t.bg)
    }
}

fn display_path(path: &Path) -> String {
    crate::display_path(path)
}

/// Paint the New Project popup. Returns the outer popup rect (for `panel_area` / mouse).
pub fn draw(
    frame: &mut Frame<'_>,
    form: &NewProjectForm,
    t: Theme,
    status: Option<&str>,
) -> Rect {
    let area = popup_rect(frame.area());
    let lay = layout(area);

    // Opaque layer: Clear removes underneath glyphs; solid fill paints every cell.
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        area,
    );

    let block = Block::default()
        .title(format!(" {APP_NAME} · New project "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, area);

    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg)),
        lay.inner,
    );

    let col_w = lay.help.width.max(1) as usize;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(
                "tab · shift-enter=newline · enter next · create/ctrl-enter · shift-up · esc",
                col_w,
            ),
            Style::default().fg(t.dim).bg(t.bg),
        )))
        .style(Style::default().bg(t.bg)),
        lay.help,
    );

    let name_raw = if form.name.is_empty() {
        "…".to_string()
    } else {
        form.name.clone()
    };
    let parent_raw = format!("{}  ↵", display_path(&form.parent));
    let elevated = form
        .init_agent
        .map(init_agent_elevated)
        .unwrap_or(false);
    let init_label = match form.init_agent {
        Some(a) if elevated => format!("{} · full tools", a.label()),
        Some(a) => a.label().to_string(),
        None => "none (scaffold only)".into(),
    };
    let create_label = match form.init_agent {
        Some(_) if elevated => "scaffold + headless init · full tools",
        Some(_) => "scaffold + headless init",
        None => "scaffold only",
    };

    let top: [(&str, String, NewProjectField, Rect); 4] = [
        ("Name", name_raw, NewProjectField::Name, lay.name),
        ("Parent", parent_raw, NewProjectField::Parent, lay.parent),
        (
            "Template",
            form.template.label().to_string(),
            NewProjectField::Template,
            lay.template,
        ),
        (
            "Init agent",
            init_label,
            NewProjectField::InitAgent,
            lay.init_agent,
        ),
    ];

    for (label, value, field, rect) in top {
        let selected = form.field == field;
        let style = field_row_style(selected, &t);
        let marker = if selected { ">" } else { " " };
        let prefix = format!("{marker} {label:<11} ");
        let field_w = rect.width.max(1) as usize;
        let avail = field_w.saturating_sub(display_width(&prefix));
        let value_out = match field {
            NewProjectField::Name if selected => {
                let with_caret = if form.name.is_empty() {
                    format!("▌{value}")
                } else {
                    format!("{value}▌")
                };
                sliding_tail(&with_caret, avail)
            }
            NewProjectField::Name => sliding_tail(&value, avail),
            NewProjectField::Parent => sliding_tail(&value, avail),
            _ => sliding_tail(&value, avail),
        };
        let raw = format!("{prefix}{value_out}");
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(pad_line(&raw, field_w), style)))
                .style(Style::default().bg(t.bg)),
            rect,
        );
    }

    // Notes label
    let notes_selected = form.field == NewProjectField::Notes;
    let notes_label_style = field_row_style(notes_selected, &t);
    let notes_marker = if notes_selected { ">" } else { " " };
    let field_w = lay.notes_label.width.max(1) as usize;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(&format!("{notes_marker} {:<11}", "Notes"), field_w),
            notes_label_style,
        )))
        .style(Style::default().bg(t.bg)),
        lay.notes_label,
    );

    // Notes viewport (read-only clamp for paint)
    let scroll = clamp_notes_scroll(&form.notes, form.notes_scroll);
    let viewport = notes_viewport(&form.notes, scroll);
    let notes_empty = form.notes.is_empty();
    let notes_box_w = lay.notes_box.width.max(1) as usize;
    let end_line_idx = notes_lines(&form.notes).len().saturating_sub(1);
    let notes_indent = "  ";
    let notes_avail = notes_box_w.saturating_sub(display_width(notes_indent));
    let mut note_lines = Vec::new();
    for (i, line) in viewport.iter().enumerate() {
        let line_idx = scroll as usize + i;
        let mut text = if notes_empty && i == 0 {
            "optional — what is this project?".to_string()
        } else {
            line.clone()
        };
        if notes_selected && line_idx == end_line_idx {
            text = if notes_empty {
                format!("▌{text}")
            } else {
                format!("{text}▌")
            };
        }
        let text = sliding_tail(&text, notes_avail);
        let style = if notes_selected {
            Style::default().fg(ACCENT_ON).bg(ACCENT)
        } else if notes_empty {
            Style::default().fg(t.dim).bg(t.bg)
        } else {
            Style::default().fg(t.soft).bg(t.bg)
        };
        let raw = format!("{notes_indent}{text}");
        note_lines.push(Line::from(Span::styled(pad_line(&raw, notes_box_w), style)));
    }
    frame.render_widget(
        Paragraph::new(note_lines).style(Style::default().bg(t.bg)),
        lay.notes_box,
    );

    // Create
    let create_selected = form.field == NewProjectField::Create;
    let create_style = field_row_style(create_selected, &t);
    let create_marker = if create_selected { ">" } else { " " };
    let create_raw = format!("{create_marker} {:<11} {create_label}", "Create");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(&create_raw, lay.create.width.max(1) as usize),
            create_style,
        )))
        .style(Style::default().bg(t.bg)),
        lay.create,
    );

    // Status row always present; blank when no message.
    let status_w = lay.status.width.max(1) as usize;
    let status_text = status.unwrap_or("").trim();
    let status_style = if status_text.is_empty() {
        Style::default().fg(t.dim).bg(t.bg)
    } else {
        Style::default().fg(ACCENT).bg(t.bg)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(status_text, status_w),
            status_style,
        )))
        .style(Style::default().bg(t.bg)),
        lay.status,
    );

    area
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::ProjectTemplate;
    use ratatui::{backend::TestBackend, Terminal};
    use std::path::PathBuf;

    fn sample_form() -> NewProjectForm {
        NewProjectForm {
            name: String::new(),
            parent: PathBuf::from("/tmp"),
            template: ProjectTemplate::Agent,
            init_agent: None,
            notes: String::new(),
            notes_scroll: 0,
            field: NewProjectField::Name,
        }
    }

    #[test]
    fn layout_hit_test_maps_fields() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let popup = popup_rect(screen);
        let lay = layout(popup);

        // Help is not a field
        assert_eq!(hit_test(&lay, lay.help.y, lay.help.x + 1), None);

        assert_eq!(
            hit_test(&lay, lay.name.y, lay.name.x + 1),
            Some(NewProjectField::Name)
        );
        assert_eq!(
            hit_test(&lay, lay.parent.y, lay.parent.x + 1),
            Some(NewProjectField::Parent)
        );
        assert_eq!(
            hit_test(&lay, lay.template.y, lay.template.x + 1),
            Some(NewProjectField::Template)
        );
        assert_eq!(
            hit_test(&lay, lay.init_agent.y, lay.init_agent.x + 1),
            Some(NewProjectField::InitAgent)
        );
        assert_eq!(
            hit_test(&lay, lay.notes_label.y, lay.notes_label.x + 1),
            Some(NewProjectField::Notes)
        );
        // Middle of notes box
        assert_eq!(
            hit_test(&lay, lay.notes_box.y + 1, lay.notes_box.x + 1),
            Some(NewProjectField::Notes)
        );
        assert_eq!(
            hit_test(&lay, lay.create.y, lay.create.x + 1),
            Some(NewProjectField::Create)
        );
        // Status reserved but not a field
        assert_eq!(hit_test(&lay, lay.status.y, lay.status.x + 1), None);
    }

    #[test]
    fn draw_smoke_title_and_create() {
        let backend = TestBackend::new(80, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        let form = sample_form();
        let t = Theme::dark();

        terminal
            .draw(|frame| {
                let _ = draw(frame, &form, t, None);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut flat = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                flat.push_str(buf[(x, y)].symbol());
            }
            flat.push('\n');
        }
        assert!(
            flat.contains("New project") || flat.contains("T-0"),
            "expected title in buffer, got:\n{flat}"
        );
        assert!(
            flat.to_lowercase().contains("create") || flat.contains("scaffold"),
            "expected create/scaffold row, got:\n{flat}"
        );
    }

    #[test]
    fn draw_shows_status_when_present() {
        let backend = TestBackend::new(80, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        let form = sample_form();
        let t = Theme::dark();

        terminal
            .draw(|frame| {
                let _ = draw(frame, &form, t, Some("✦ created demo"));
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut flat = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                flat.push_str(buf[(x, y)].symbol());
            }
            flat.push('\n');
        }
        assert!(
            flat.contains("created demo") || flat.contains("✦"),
            "expected status text, got:\n{flat}"
        );
    }
}
