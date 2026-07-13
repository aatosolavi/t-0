//! Pure helpers for New Project: scaffold, headless init recipes, text editing.
//! UI / App state stays in `main.rs`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use unicode_width::UnicodeWidthChar;

pub const NOTES_VIEWPORT_ROWS: u16 = 3;
pub const NOTES_MAX_CHARS: usize = 2000;
pub const NAME_MAX_CHARS: usize = 64;

/// Scaffold template (mechanical L1+L2 files only).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProjectTemplate {
    Agent,
    Minimal,
}

impl ProjectTemplate {
    pub fn label(self) -> &'static str {
        match self {
            ProjectTemplate::Agent => "agent",
            ProjectTemplate::Minimal => "minimal",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            ProjectTemplate::Agent => ProjectTemplate::Minimal,
            ProjectTemplate::Minimal => ProjectTemplate::Agent,
        }
    }
}

/// Agents that have a headless init recipe (not Shell).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InitAgentKind {
    Grok,
    Codex,
    Pi,
    Cursor,
    Claude,
    Amp,
    Devin,
    Droid,
}

impl InitAgentKind {
    /// True when the recipe grants elevated tool autonomy (skip prompts / force / auto-write).
    pub fn elevated_autonomy(self) -> bool {
        match self {
            InitAgentKind::Grok
            | InitAgentKind::Claude
            | InitAgentKind::Cursor
            | InitAgentKind::Droid
            | InitAgentKind::Devin => true,
            // Codex sandbox workspace-write; Pi/Amp open-ish but less "dangerous" labeled.
            InitAgentKind::Codex | InitAgentKind::Pi | InitAgentKind::Amp => false,
        }
    }
}

/// Per-agent unattended init invocation (argv only — no shell).
#[derive(Clone)]
pub struct InitCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

pub struct InitPrompt {
    pub project_name: String,
    pub template: ProjectTemplate,
    pub notes: String,
}

pub fn compose_init_prompt(p: &InitPrompt) -> String {
    let notes = p.notes.trim();
    let notes_block = if notes.is_empty() {
        "(none — sensible defaults for a new local project)".to_string()
    } else {
        notes.to_string()
    };
    format!(
        r#"Bootstrap this brand-new local git repository for agent-assisted development.

Follow the repository /init bootstrap workflow if you have it (AGENTS.md, docs/INDEX.md,
versioned git hooks, stack detection, quality scripts, verify). If you do not have an
/init skill, still produce an equivalent professional starter: AGENTS.md, docs/INDEX.md,
README, .gitignore, and versioned hooks where appropriate.

Rules:
- Work only inside this repository working directory.
- Merge with existing starter files; do not delete them blindly.
- Do not create a GitHub remote or push unless asked.
- Prefer production-complete defaults; fail loud; no placeholder stubs.

Project name: {name}
Scaffold template: {template}

User notes about the project:
{notes}

When finished, summarize files created/changed and any verification you ran."#,
        name = p.project_name,
        template = p.template.label(),
        notes = notes_block,
    )
}

/// Build argv for unattended init. Prompt is a single argv element (never shell-interpolated).
pub fn build_init_command(
    kind: InitAgentKind,
    program: String,
    prefix_args: Vec<String>,
    cwd: &Path,
    prompt: &str,
) -> InitCommand {
    let cwd_str = cwd.display().to_string();
    let mut args = prefix_args;
    match kind {
        InitAgentKind::Grok => {
            args.extend([
                "-p".into(),
                prompt.to_string(),
                "--cwd".into(),
                cwd_str,
                "--always-approve".into(),
                "--permission-mode".into(),
                "acceptEdits".into(),
            ]);
        }
        InitAgentKind::Codex => {
            args.extend([
                "exec".into(),
                "-C".into(),
                cwd_str,
                "--sandbox".into(),
                "workspace-write".into(),
                prompt.to_string(),
            ]);
        }
        InitAgentKind::Claude => {
            args.extend([
                "-p".into(),
                prompt.to_string(),
                "--dangerously-skip-permissions".into(),
            ]);
        }
        InitAgentKind::Cursor => {
            args.extend([
                "-p".into(),
                "--force".into(),
                "--trust".into(),
                "--workspace".into(),
                cwd_str,
                "--output-format".into(),
                "text".into(),
                prompt.to_string(),
            ]);
        }
        InitAgentKind::Pi => {
            args.extend(["-p".into(), "-a".into(), prompt.to_string()]);
        }
        InitAgentKind::Amp => {
            args.extend(["-x".into(), prompt.to_string()]);
        }
        InitAgentKind::Devin => {
            args.extend([
                "-p".into(),
                prompt.to_string(),
                "--permission-mode".into(),
                "accept-edits".into(),
            ]);
        }
        InitAgentKind::Droid => {
            args.extend([
                "exec".into(),
                "--cwd".into(),
                cwd_str,
                "--auto".into(),
                "medium".into(),
                prompt.to_string(),
            ]);
        }
    }
    InitCommand {
        program,
        args,
        cwd: cwd.to_path_buf(),
    }
}

/// Slug is always a single path segment (no `/`, no `..`) by construction.
pub fn slugify_project_name(raw: &str) -> Result<String, String> {
    let s = raw.trim().to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if matches!(c, ' ' | '_' | '-' | '.') {
            if !out.is_empty() && !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        return Err("name needs ascii letters/digits".into());
    }
    Ok(out)
}

const SCAFFOLD_GITIGNORE: &str = "\
.DS_Store
.env
.env.*
!.env.example
*.log
.idea/
.vscode/
node_modules/
target/
dist/
build/
";

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn write_scaffold_contents(
    target: &Path,
    slug: &str,
    template: ProjectTemplate,
    notes: &str,
) -> Result<(), String> {
    let notes_trim = notes.trim();
    let notes_section = if notes_trim.is_empty() {
        String::new()
    } else {
        format!("\n{notes_trim}\n")
    };

    let readme = match template {
        ProjectTemplate::Minimal => format!("# {slug}\n\nScaffolded by T-0.{notes_section}"),
        ProjectTemplate::Agent => format!(
            "# {slug}\n\nScaffolded by T-0. Thin agent-ready starter — your init agent will expand this.\n{notes_section}"
        ),
    };
    fs::write(target.join("README.md"), readme).map_err(|e| format!("README: {e}"))?;
    fs::write(target.join(".gitignore"), SCAFFOLD_GITIGNORE)
        .map_err(|e| format!(".gitignore: {e}"))?;

    if template == ProjectTemplate::Agent {
        fs::create_dir_all(target.join("docs")).map_err(|e| format!("docs/: {e}"))?;
        let agents = format!(
            r#"# AGENTS.md

Start with **[docs/INDEX.md](./docs/INDEX.md)**.

## Project

- **Name:** {slug}
- **Scaffold:** T-0 agent template (run agent init / `/init` to complete bootstrap)

## Code principles

1. Production-complete or hard-fail
2. Fail loud
3. Small, focused diffs
4. Evidence before claims
"#
        );
        fs::write(target.join("AGENTS.md"), agents).map_err(|e| format!("AGENTS.md: {e}"))?;
        let index = format!(
            r#"# docs/INDEX.md

Recovery map for **{slug}**.

| Doc | Why |
|-----|-----|
| [../README.md](../README.md) | Project overview |
| [../AGENTS.md](../AGENTS.md) | Agent instructions |

Add architecture and workflow links as the project grows.
"#
        );
        fs::write(target.join("docs/INDEX.md"), index)
            .map_err(|e| format!("docs/INDEX.md: {e}"))?;
    }

    let path_str = target.display().to_string();
    let init = Command::new("git")
        .args(["-C", &path_str, "init", "-b", "main"])
        .status()
        .map_err(|e| format!("git init: {e}"))?;
    if !init.success() {
        return Err("git init failed".into());
    }
    let _ = Command::new("git")
        .args(["-C", &path_str, "add", "-A"])
        .status();
    // Commit may fail without user.email — scaffold still succeeds.
    let _ = Command::new("git")
        .args(["-C", &path_str, "commit", "-m", "chore: scaffold from t0"])
        .status();
    Ok(())
}

/// Create project dir + files + git. On any failure after mkdir, removes the target.
pub fn create_scaffold(
    parent: &Path,
    slug: &str,
    template: ProjectTemplate,
    notes: &str,
    display_path: &dyn Fn(&Path) -> String,
) -> Result<PathBuf, String> {
    if !git_available() {
        return Err("git not found on PATH".into());
    }
    if !parent.is_dir() {
        return Err("parent is not a directory".into());
    }
    let target = parent.join(slug);
    if target.exists() {
        return Err(format!("{} already exists", display_path(&target)));
    }
    fs::create_dir_all(&target).map_err(|e| format!("mkdir: {e}"))?;

    if let Err(e) = write_scaffold_contents(&target, slug, template, notes) {
        let _ = fs::remove_dir_all(&target);
        return Err(e);
    }
    Ok(target)
}

/// Display column width of `s` (Unicode-aware; control chars count as 0).
pub fn display_width(s: &str) -> usize {
    s.chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

/// Pad/truncate to `width` **display** columns (Unicode-aware).
/// Truncates from the **end** (front kept) — use [`sliding_tail`] / [`front_ellipsize`]
/// when the cursor or leaf path must stay visible.
pub fn pad_line(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut cols = 0usize;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if cols + w > width {
            break;
        }
        out.push(ch);
        cols += w;
    }
    if cols < width {
        out.push_str(&" ".repeat(width - cols));
    }
    out
}

/// Sliding tail window: keep the **end** of `s` within `width` display columns.
/// When truncated, prefixes `…` so recent input / caret stay visible.
pub fn sliding_tail(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if display_width(s) <= width {
        return s.to_string();
    }
    // Single-column ellipsis when there is room; otherwise hard-clip the tail.
    let ell = '…';
    let ell_w = UnicodeWidthChar::width(ell).unwrap_or(1);
    if width <= ell_w {
        return String::from(ell);
    }
    let avail = width.saturating_sub(ell_w);
    let mut rev: Vec<char> = Vec::new();
    let mut cols = 0usize;
    for ch in s.chars().rev() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if cols + w > avail {
            break;
        }
        rev.push(ch);
        cols += w;
    }
    rev.reverse();
    let mut out = String::from(ell);
    for ch in rev {
        out.push(ch);
    }
    out
}

/// Front-ellipsize so the **leaf** (end) stays visible — for parent paths.
pub fn front_ellipsize(s: &str, width: usize) -> String {
    sliding_tail(s, width)
}

/// Logical lines of notes (always at least one empty line for empty string).
pub fn notes_lines(notes: &str) -> Vec<&str> {
    if notes.is_empty() {
        return vec![""];
    }
    notes.split('\n').collect()
}

/// Keep scroll so the end of the text is visible when typing at the end.
pub fn clamp_notes_scroll(notes: &str, scroll: u16) -> u16 {
    let n = notes_lines(notes).len();
    let max_scroll = n.saturating_sub(NOTES_VIEWPORT_ROWS as usize) as u16;
    scroll.min(max_scroll)
}

pub fn auto_scroll_notes_to_end(notes: &str) -> u16 {
    let n = notes_lines(notes).len();
    n.saturating_sub(NOTES_VIEWPORT_ROWS as usize) as u16
}

/// Visible slice of notes for a 3-row viewport.
pub fn notes_viewport(notes: &str, scroll: u16) -> Vec<String> {
    let lines = notes_lines(notes);
    let start = scroll as usize;
    let mut out = Vec::with_capacity(NOTES_VIEWPORT_ROWS as usize);
    for i in 0..NOTES_VIEWPORT_ROWS as usize {
        let idx = start + i;
        if idx < lines.len() {
            out.push(lines[idx].to_string());
        } else {
            out.push(String::new());
        }
    }
    out
}

pub fn delete_last_char(s: &mut String) {
    s.pop();
}

/// Option/Alt+Backspace: delete last word (and trailing whitespace before it).
pub fn delete_last_word(s: &mut String) {
    let trimmed_end = s.trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
    let end = trimmed_end.len();
    s.truncate(end);
    // Walk back over non-whitespace on the current line (stop at newline).
    while let Some(c) = s.chars().last() {
        if c == '\n' {
            break;
        }
        if c.is_whitespace() {
            break;
        }
        s.pop();
    }
    // Also drop spaces after the word on this line (already trimmed above).
    while let Some(c) = s.chars().last() {
        if c == ' ' || c == '\t' {
            s.pop();
        } else {
            break;
        }
    }
}

/// Cmd/Ctrl+U style: delete from last newline (or start) to end — current line.
pub fn delete_current_line(s: &mut String) {
    if let Some(pos) = s.rfind('\n') {
        s.truncate(pos + 1); // keep the newline
    } else {
        s.clear();
    }
}

pub fn env_flag_on(key: &str) -> bool {
    match std::env::var(key) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        }
        Err(_) => false,
    }
}
