//! Finder-style directory browser + folder-picker draw/layout/input.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::{display_path, home_dir, inset, pad_or_trim, Theme, ACCENT, ACCENT_ON};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FolderPickerPurpose {
    WorkspaceRoot,
    NewProjectParent,
}

#[derive(Clone)]
pub struct FolderEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_parent: bool,
    pub is_git: bool,
}

#[derive(Clone)]
pub struct FolderBrowser {
    cwd: PathBuf,
    pub entries: Vec<FolderEntry>,
    pub selected: usize,
    pub offset: usize,
}

impl FolderBrowser {
    pub fn open(start: PathBuf) -> Self {
        let cwd = if start.is_dir() {
            start
        } else {
            home_dir()
        };
        let mut browser = Self {
            cwd,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
        };
        browser.reload();
        browser
    }

    fn reload(&mut self) {
        self.entries = list_folder_entries(&self.cwd);
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        self.offset = 0;
        self.keep_selected_visible(12);
    }

    pub fn keep_selected_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.entries.len() - 1);
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn enter_selected(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if entry.path.is_dir() {
            self.cwd = entry.path;
            self.selected = 0;
            self.reload();
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            let parent = parent.to_path_buf();
            if parent != self.cwd {
                let left = self.cwd.clone();
                self.cwd = parent;
                self.reload();
                // Land on the folder we just left.
                if let Some(idx) = self.entries.iter().position(|e| e.path == left) {
                    self.selected = idx;
                }
            }
        }
    }

    pub fn jump(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.cwd = path;
            self.selected = 0;
            self.reload();
        }
    }

    /// Directory that would become the workspace root.
    /// Prefer selected child; `..` means parent; empty list → cwd.
    pub fn chosen_path(&self) -> PathBuf {
        match self.entries.get(self.selected) {
            Some(e) if e.is_parent => e.path.clone(),
            Some(e) => e.path.clone(),
            None => self.cwd.clone(),
        }
    }

    pub fn current_path(&self) -> &Path {
        &self.cwd
    }
}

fn list_folder_entries(cwd: &Path) -> Vec<FolderEntry> {
    let mut entries = Vec::new();

    if let Some(parent) = cwd.parent() {
        if parent != cwd {
            entries.push(FolderEntry {
                name: "..".into(),
                path: parent.to_path_buf(),
                is_parent: true,
                is_git: false,
            });
        }
    }

    let mut dirs: Vec<FolderEntry> = fs::read_dir(cwd)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            // Skip dotfiles (Finder-ish default); keep ".." only via parent row.
            if name.starts_with('.') {
                return None;
            }
            let is_git = path.join(".git").exists();
            Some(FolderEntry {
                name,
                path,
                is_parent: false,
                is_git,
            })
        })
        .collect();

    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries.extend(dirs);
    entries
}

#[derive(Clone, Debug)]
pub struct FolderLayout {
    pub path: Rect,
    pub help: Rect,
    pub list: Rect,
    pub actions: Rect,
    pub footer: Rect,
}

pub fn layout(inner: Rect) -> FolderLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    FolderLayout {
        path: chunks[0],
        help: chunks[1],
        list: chunks[2],
        actions: chunks[3],
        footer: chunks[4],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderAction {
    None,
    Cancel,
    ConfirmSelected,
    ConfirmCurrent,
    Status(String),
}

pub fn handle_key(folder: &mut FolderBrowser, key: KeyEvent) -> FolderAction {
    match key.code {
        KeyCode::Esc => FolderAction::Cancel,
        KeyCode::Down | KeyCode::Char('j') => {
            folder.select_next();
            FolderAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            folder.select_prev();
            FolderAction::None
        }
        KeyCode::Left | KeyCode::Backspace => {
            folder.go_up();
            FolderAction::None
        }
        KeyCode::Right | KeyCode::Enter => {
            folder.enter_selected();
            FolderAction::None
        }
        KeyCode::Char(' ') | KeyCode::Char('o') | KeyCode::Char('O') => FolderAction::ConfirmSelected,
        KeyCode::Char('s') | KeyCode::Char('S') => FolderAction::ConfirmCurrent,
        KeyCode::Char('h') | KeyCode::Char('H') => {
            folder.jump(home_dir());
            FolderAction::None
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            let dev = home_dir().join("dev");
            if dev.is_dir() {
                folder.jump(dev);
                FolderAction::None
            } else {
                FolderAction::Status("~/dev not found".into())
            }
        }
        KeyCode::Char('/') => {
            folder.jump(PathBuf::from("/"));
            FolderAction::None
        }
        KeyCode::Char('~') => {
            folder.jump(home_dir());
            FolderAction::None
        }
        _ => FolderAction::None,
    }
}

pub fn handle_mouse(folder: &mut FolderBrowser, mouse: MouseEvent, list: Rect) -> FolderAction {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            folder.select_next();
            FolderAction::None
        }
        MouseEventKind::ScrollUp => {
            folder.select_prev();
            FolderAction::None
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.row >= list.y && mouse.row < list.y.saturating_add(list.height) {
                let row = usize::from(mouse.row - list.y);
                let idx = folder.offset + row;
                if idx < folder.entries.len() {
                    if idx == folder.selected {
                        folder.enter_selected();
                    } else {
                        folder.selected = idx;
                    }
                }
            }
            FolderAction::None
        }
        _ => FolderAction::None,
    }
}

/// Draw folder picker. Returns (panel_area, list_top, list_height).
pub fn draw(
    frame: &mut Frame<'_>,
    folder: &mut FolderBrowser,
    purpose: FolderPickerPurpose,
    panel: Rect,
    t: Theme,
    status: Option<&str>,
) -> (u16, u16) {
    let block = Block::default()
        .title(match purpose {
            FolderPickerPurpose::WorkspaceRoot => " Choose workspace root ",
            FolderPickerPurpose::NewProjectParent => " Choose project parent ",
        })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, panel);

    let inner = inset(panel, crate::PANEL_PAD_H, crate::PANEL_PAD_V);
    let lay = layout(inner);

    let title_path = display_path(folder.current_path());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                pad_or_trim(&title_path, lay.path.width.saturating_sub(2) as usize),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ])),
        lay.path,
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "enter open folder · space use selected · s use this folder · esc cancel",
            Style::default().fg(t.dim),
        ))),
        lay.help,
    );

    let list_area = lay.list;
    let visible = list_area.height as usize;
    folder.keep_selected_visible(visible.max(1));

    let mut lines = Vec::new();
    for (i, entry) in folder
        .entries
        .iter()
        .enumerate()
        .skip(folder.offset)
        .take(visible)
    {
        let selected = i == folder.selected;
        let marker = if selected { ">" } else { " " };
        let icon = if entry.is_parent {
            "^"
        } else if entry.is_git {
            "*"
        } else {
            " "
        };
        let badge = if entry.is_git {
            "  git"
        } else if entry.is_parent {
            "  parent"
        } else {
            "/"
        };
        let name_width = list_area
            .width
            .saturating_sub(4 + badge.chars().count() as u16)
            .max(8) as usize;
        let label = if entry.is_parent {
            ".."
        } else {
            entry.name.as_str()
        };
        let name = pad_or_trim(label, name_width);
        let row_style = if selected {
            Style::default()
                .fg(ACCENT_ON)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD)
        } else if entry.is_parent {
            Style::default().fg(t.dim)
        } else if entry.is_git {
            Style::default().fg(t.text)
        } else {
            Style::default().fg(t.soft)
        };
        let badge_style = if selected {
            Style::default().fg(ACCENT_ON).bg(ACCENT)
        } else {
            Style::default().fg(t.dim)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {icon} "), row_style),
            Span::styled(name, row_style),
            Span::styled(badge, badge_style),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (empty folder)",
            Style::default().fg(t.dim),
        )));
    }
    frame.render_widget(Paragraph::new(lines), list_area);

    let chosen = display_path(&folder.chosen_path());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" select → ", Style::default().fg(ACCENT_ON).bg(ACCENT)),
            Span::styled(
                format!(
                    " {}",
                    pad_or_trim(&chosen, list_area.width.saturating_sub(12) as usize)
                ),
                Style::default().fg(t.text),
            ),
        ])),
        lay.actions,
    );

    let footer = if let Some(status) = status {
        Line::from(Span::styled(status.to_string(), Style::default().fg(ACCENT)))
    } else {
        Line::from(vec![
            Span::styled("h", Style::default().fg(t.key)),
            Span::styled(" home  ", Style::default().fg(t.dim)),
            Span::styled("d", Style::default().fg(t.key)),
            Span::styled(" ~/dev  ", Style::default().fg(t.dim)),
            Span::styled("/", Style::default().fg(t.key)),
            Span::styled(" root  ", Style::default().fg(t.dim)),
            Span::styled("←", Style::default().fg(t.key)),
            Span::styled(" up", Style::default().fg(t.dim)),
        ])
    };
    frame.render_widget(Paragraph::new(footer), lay.footer);

    (list_area.y, list_area.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_list_below_help() {
        let inner = Rect { x: 2, y: 2, width: 60, height: 20 };
        let lay = layout(inner);
        assert!(lay.list.y > lay.help.y);
        assert!(lay.list.height >= 6);
    }

    #[test]
    fn open_home_has_parent_or_entries() {
        let b = FolderBrowser::open(home_dir());
        // should not panic; may have entries
        let _ = b.chosen_path();
    }
}
