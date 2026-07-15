//! New Project key/mouse handling → side-effect actions for App to apply.
//! Form mutations (text, field, template) happen here; App owns create/cycle/nav.
//!
//! Esc-meta prefix handling was removed: browser xterm sets `macOptionIsMeta: true`
//! (`terminal/index.html`) so Option+Backspace arrives as real Meta, not Esc then BS.
//! Word-delete still works via Alt/Ctrl+W/Super+Backspace chords.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::new_project::{
    auto_scroll_notes_to_end, clamp_notes_scroll, delete_current_line, delete_last_char,
    delete_last_word, notes_lines, NAME_MAX_CHARS, NOTES_MAX_CHARS, NOTES_VIEWPORT_ROWS,
};
use crate::new_project_ui;
use crate::{NewProjectField, NewProjectForm, TextDelete};

/// Side effects that require App (create, navigation, init cycle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NpAction {
    None,
    Close,
    OpenParentPicker,
    Create,
    CycleInitAgent(i32),
}

fn text_delete(form: &mut NewProjectForm, kind: TextDelete) {
    if !matches!(
        form.field,
        NewProjectField::Name | NewProjectField::Notes
    ) {
        return;
    }
    let is_notes = form.field == NewProjectField::Notes;
    let s = if is_notes {
        &mut form.notes
    } else {
        &mut form.name
    };
    match kind {
        TextDelete::Char => delete_last_char(s),
        TextDelete::Word => delete_last_word(s),
        TextDelete::Line => delete_current_line(s),
    }
    if is_notes {
        form.notes_scroll = clamp_notes_scroll(&form.notes, form.notes_scroll);
    }
}

/// Handle a key while New Project is open.
pub fn handle_key(form: &mut NewProjectForm, key: KeyEvent) -> NpAction {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let super_key = mods.contains(KeyModifiers::SUPER);
    let plain_or_shift = mods.is_empty() || mods == KeyModifiers::SHIFT;

    // Ctrl+Enter always creates from any field.
    if matches!(key.code, KeyCode::Enter) && ctrl {
        return NpAction::Create;
    }

    // Ctrl+U / Ctrl+W / Ctrl+Backspace on Name/Notes.
    if ctrl {
        if let KeyCode::Char(c) = key.code {
            let lower = c.to_ascii_lowercase();
            if matches!(
                form.field,
                NewProjectField::Name | NewProjectField::Notes
            ) && (lower == 'u' || lower == 'w')
            {
                if lower == 'u' {
                    text_delete(form, TextDelete::Line);
                } else {
                    text_delete(form, TextDelete::Word);
                }
                return NpAction::None;
            }
        }
        if matches!(key.code, KeyCode::Backspace | KeyCode::Delete)
            && matches!(
                form.field,
                NewProjectField::Name | NewProjectField::Notes
            )
        {
            text_delete(form, TextDelete::Word);
            return NpAction::None;
        }
    }

    match key.code {
        KeyCode::Esc => NpAction::Close,
        KeyCode::Down => {
            if form.field == NewProjectField::Notes {
                let n = notes_lines(&form.notes).len();
                let max_scroll = n.saturating_sub(NOTES_VIEWPORT_ROWS as usize) as u16;
                if form.notes_scroll < max_scroll {
                    form.notes_scroll += 1;
                } else {
                    form.field = form.field.next();
                }
            } else {
                form.field = form.field.next();
            }
            NpAction::None
        }
        KeyCode::Up => {
            if form.field == NewProjectField::Notes {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    form.field = form.field.prev();
                } else if form.notes_scroll > 0 {
                    form.notes_scroll -= 1;
                } else {
                    form.field = form.field.prev();
                }
            } else {
                form.field = form.field.prev();
            }
            NpAction::None
        }
        KeyCode::Char('j')
            if plain_or_shift
                && !matches!(
                    form.field,
                    NewProjectField::Name | NewProjectField::Notes
                ) =>
        {
            form.field = form.field.next();
            NpAction::None
        }
        KeyCode::Char('k')
            if plain_or_shift
                && !matches!(
                    form.field,
                    NewProjectField::Name | NewProjectField::Notes
                ) =>
        {
            form.field = form.field.prev();
            NpAction::None
        }
        KeyCode::Tab => {
            form.field = form.field.next();
            NpAction::None
        }
        KeyCode::BackTab => {
            form.field = form.field.prev();
            NpAction::None
        }
        KeyCode::Left => match form.field {
            NewProjectField::Template => {
                form.template = form.template.cycle();
                NpAction::None
            }
            NewProjectField::InitAgent => NpAction::CycleInitAgent(-1),
            _ => NpAction::None,
        },
        KeyCode::Right => match form.field {
            NewProjectField::Template => {
                form.template = form.template.cycle();
                NpAction::None
            }
            NewProjectField::InitAgent => NpAction::CycleInitAgent(1),
            NewProjectField::Parent => NpAction::OpenParentPicker,
            _ => NpAction::None,
        },
        KeyCode::Char(' ') if plain_or_shift => match form.field {
            NewProjectField::Name => {
                if form.name.chars().count() < NAME_MAX_CHARS {
                    form.name.push(' ');
                }
                NpAction::None
            }
            NewProjectField::Notes => {
                if form.notes.chars().count() < NOTES_MAX_CHARS {
                    form.notes.push(' ');
                    form.notes_scroll = auto_scroll_notes_to_end(&form.notes);
                }
                NpAction::None
            }
            NewProjectField::Template => {
                form.template = form.template.cycle();
                NpAction::None
            }
            NewProjectField::InitAgent => NpAction::CycleInitAgent(1),
            NewProjectField::Parent => NpAction::OpenParentPicker,
            NewProjectField::Create => NpAction::Create,
        },
        KeyCode::Enter => {
            let shift = mods.contains(KeyModifiers::SHIFT);
            if shift
                && form.field == NewProjectField::Notes
                && form.notes.chars().count() < NOTES_MAX_CHARS
            {
                form.notes.push('\n');
                form.notes_scroll = auto_scroll_notes_to_end(&form.notes);
                return NpAction::None;
            }
            match form.field {
                NewProjectField::Parent => NpAction::OpenParentPicker,
                NewProjectField::Template => {
                    form.template = form.template.cycle();
                    NpAction::None
                }
                NewProjectField::InitAgent => NpAction::CycleInitAgent(1),
                NewProjectField::Name | NewProjectField::Notes => {
                    form.field = form.field.next();
                    NpAction::None
                }
                NewProjectField::Create => NpAction::Create,
            }
        }
        KeyCode::Backspace | KeyCode::Delete => {
            if matches!(
                form.field,
                NewProjectField::Name | NewProjectField::Notes
            ) {
                if super_key {
                    text_delete(form, TextDelete::Line);
                } else if alt {
                    text_delete(form, TextDelete::Word);
                } else {
                    text_delete(form, TextDelete::Char);
                }
            }
            NpAction::None
        }
        KeyCode::Char(c) if !c.is_control() && plain_or_shift => {
            match form.field {
                NewProjectField::Name => {
                    if form.name.chars().count() < NAME_MAX_CHARS {
                        form.name.push(c);
                    }
                }
                NewProjectField::Notes => {
                    if form.notes.chars().count() < NOTES_MAX_CHARS {
                        form.notes.push(c);
                        form.notes_scroll = auto_scroll_notes_to_end(&form.notes);
                    }
                }
                _ => {}
            }
            NpAction::None
        }
        _ => NpAction::None,
    }
}

/// Bracketed paste into Name / Notes (other fields ignore).
pub fn handle_paste(form: &mut NewProjectForm, text: &str) {
    match form.field {
        NewProjectField::Name => {
            for c in text.chars() {
                if c.is_control() {
                    continue;
                }
                if form.name.chars().count() >= NAME_MAX_CHARS {
                    break;
                }
                form.name.push(c);
            }
        }
        NewProjectField::Notes => {
            let mut changed = false;
            for c in text.chars() {
                // Allow newlines in notes paste; skip other controls.
                if c != '\n' && c != '\t' && c.is_control() {
                    continue;
                }
                let ch = if c == '\t' { ' ' } else { c };
                if form.notes.chars().count() >= NOTES_MAX_CHARS {
                    break;
                }
                form.notes.push(ch);
                changed = true;
            }
            if changed {
                form.notes_scroll = auto_scroll_notes_to_end(&form.notes);
            }
        }
        _ => {}
    }
}

/// Mouse while New Project is open. `panel` is the popup rect from last draw.
pub fn handle_mouse(
    form: &mut NewProjectForm,
    mouse: MouseEvent,
    panel: Rect,
) -> NpAction {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            form.field = form.field.next();
            NpAction::None
        }
        MouseEventKind::ScrollUp => {
            form.field = form.field.prev();
            NpAction::None
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if panel.width > 0 && panel.height > 0 {
                let lay = new_project_ui::layout(panel);
                if let Some(field) = new_project_ui::hit_test(&lay, mouse.row, mouse.column) {
                    form.field = field;
                }
            }
            NpAction::None
        }
        _ => NpAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::ProjectTemplate;
    use crossterm::event::KeyEventKind;
    use std::path::PathBuf;

    fn form() -> NewProjectForm {
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

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn key_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: mods,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn enter_advances_from_name() {
        let mut f = form();
        let a = handle_key(&mut f, key(KeyCode::Enter));
        assert_eq!(a, NpAction::None);
        assert_eq!(f.field, NewProjectField::Parent);
    }

    #[test]
    fn shift_enter_newline_in_notes() {
        let mut f = form();
        f.field = NewProjectField::Notes;
        let a = handle_key(&mut f, key_mod(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(a, NpAction::None);
        assert!(f.notes.contains('\n') || f.notes.ends_with('\n') || f.notes == "\n");
        assert_eq!(f.field, NewProjectField::Notes);
    }

    #[test]
    fn esc_from_name_closes_immediately() {
        let mut f = form();
        f.field = NewProjectField::Name;
        assert_eq!(handle_key(&mut f, key(KeyCode::Esc)), NpAction::Close);
    }

    #[test]
    fn create_field_enter_creates() {
        let mut f = form();
        f.field = NewProjectField::Create;
        assert_eq!(handle_key(&mut f, key(KeyCode::Enter)), NpAction::Create);
    }

    #[test]
    fn ctrl_enter_creates_from_name() {
        let mut f = form();
        assert_eq!(
            handle_key(&mut f, key_mod(KeyCode::Enter, KeyModifiers::CONTROL)),
            NpAction::Create
        );
    }

    #[test]
    fn paste_into_name_and_notes() {
        let mut f = form();
        f.field = NewProjectField::Name;
        handle_paste(&mut f, "hello\nworld");
        assert_eq!(f.name, "helloworld"); // newlines stripped for name

        f.field = NewProjectField::Notes;
        handle_paste(&mut f, "line1\nline2");
        assert_eq!(f.notes, "line1\nline2");
    }

    #[test]
    fn typing_name_appends() {
        let mut f = form();
        handle_key(&mut f, key(KeyCode::Char('a')));
        handle_key(&mut f, key(KeyCode::Char('b')));
        assert_eq!(f.name, "ab");
    }
}
