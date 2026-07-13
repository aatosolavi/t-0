mod new_project;

use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    io::{self, stdout, BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use new_project::{
    auto_scroll_notes_to_end, build_init_command, clamp_notes_scroll, compose_init_prompt,
    create_scaffold, delete_current_line, delete_last_char, delete_last_word, display_width,
    env_flag_on, front_ellipsize, notes_viewport, pad_line, sliding_tail, slugify_project_name,
    InitAgentKind, InitCommand, InitPrompt, ProjectTemplate, NAME_MAX_CHARS, NOTES_MAX_CHARS,
    NOTES_VIEWPORT_ROWS,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const MAX_WIDTH: u16 = 92;
const MAX_RECENTS: usize = 20;
const MAX_FAVORITES: usize = 20;
/// Product name (SpaceX-flavored: countdown to liftoff — agents go at T-0).
const APP_NAME: &str = "T-0";
/// Splash / brand line.
const APP_TAGLINE: &str = "go for launch";
/// Accent — orange-500 (#f97316), a little heat for the pad.
const ACCENT: Color = Color::Rgb(249, 115, 22);
/// Text on filled accent chips (dark enough for contrast on orange).
const ACCENT_ON: Color = Color::Rgb(23, 23, 23);
/// Dirty branch / amber metadata.
const AMBER: Color = Color::Rgb(180, 120, 0);

#[derive(Clone)]
struct Repo {
    name: String,
    path: PathBuf,
    badge: &'static str,
    git_branch: Option<String>,
    git_dirty: bool,
    git_ahead: u32,
    remembered_agent: Option<Action>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Grok,
    Codex,
    Pi,
    Cursor,
    Claude,
    Amp,
    Devin,
    Droid,
    Shell,
}

impl Action {
    fn all() -> &'static [Action] {
        &[
            Action::Grok,
            Action::Codex,
            Action::Pi,
            Action::Cursor,
            Action::Claude,
            Action::Amp,
            Action::Devin,
            Action::Droid,
            Action::Shell,
        ]
    }

    fn id(self) -> &'static str {
        match self {
            Action::Grok => "grok",
            Action::Codex => "codex",
            Action::Pi => "pi",
            Action::Cursor => "cursor",
            Action::Claude => "claude",
            Action::Amp => "amp",
            Action::Devin => "devin",
            Action::Droid => "droid",
            Action::Shell => "shell",
        }
    }

    fn from_id(value: &str) -> Option<Action> {
        match value {
            "grok" => Some(Action::Grok),
            "codex" => Some(Action::Codex),
            "pi" => Some(Action::Pi),
            "cursor" => Some(Action::Cursor),
            "claude" => Some(Action::Claude),
            "amp" => Some(Action::Amp),
            "devin" => Some(Action::Devin),
            "droid" => Some(Action::Droid),
            "shell" => Some(Action::Shell),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Action::Grok => "Grok",
            Action::Codex => "Codex",
            Action::Pi => "Pi",
            Action::Cursor => "Cursor",
            Action::Claude => "Claude",
            Action::Amp => "Amp",
            Action::Devin => "Devin",
            Action::Droid => "Droid",
            Action::Shell => "Shell",
        }
    }

    fn env_command_key(self) -> Option<&'static str> {
        match self {
            Action::Shell => None,
            Action::Grok => Some("GROK_TERMINAL_GROK_COMMAND"),
            Action::Codex => Some("GROK_TERMINAL_CODEX_COMMAND"),
            Action::Pi => Some("GROK_TERMINAL_PI_COMMAND"),
            Action::Cursor => Some("GROK_TERMINAL_CURSOR_COMMAND"),
            Action::Claude => Some("GROK_TERMINAL_CLAUDE_COMMAND"),
            Action::Amp => Some("GROK_TERMINAL_AMP_COMMAND"),
            Action::Devin => Some("GROK_TERMINAL_DEVIN_COMMAND"),
            Action::Droid => Some("GROK_TERMINAL_DROID_COMMAND"),
        }
    }

    fn default_command(self) -> Option<&'static str> {
        match self {
            Action::Shell => None,
            Action::Grok => Some("grok"),
            Action::Codex => Some("codex"),
            Action::Pi => Some("pi"),
            // Cursor Agent CLI (`agent` / `cursor-agent`), not the IDE shim named `cursor`.
            Action::Cursor => Some("agent"),
            Action::Claude => Some("claude"),
            Action::Amp => Some("amp"),
            Action::Devin => Some("devin"),
            Action::Droid => Some("droid"),
        }
    }

    fn resolve_command(self) -> Option<String> {
        let key = self.env_command_key()?;
        if let Ok(value) = env::var(key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
        // Cursor: prefer `agent`, then `cursor-agent` (the IDE `cursor` shim is not the agent).
        if self == Action::Cursor {
            if command_available("agent") {
                return Some("agent".into());
            }
            if command_available("cursor-agent") {
                return Some("cursor-agent".into());
            }
            return Some("agent".into());
        }
        let default = self.default_command()?;
        Some(default.to_string())
    }

    fn is_available(self) -> bool {
        match self.resolve_command() {
            None => true, // Shell always available
            Some(command) => command_available(&command),
        }
    }

    fn index(self) -> usize {
        Action::all()
            .iter()
            .position(|action| *action == self)
            .unwrap_or(0)
    }

    fn first_available() -> Action {
        Action::all()
            .iter()
            .copied()
            .find(|action| action.is_available())
            .unwrap_or(Action::Shell)
    }

    fn resolve_available(self) -> Action {
        if self.is_available() {
            self
        } else {
            Action::first_available()
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct LastLaunch {
    cwd: String,
    action: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default = "default_true")]
    splash: bool,
    /// Action id, e.g. "grok", "cursor", "pi"
    #[serde(default)]
    default_agent: Option<String>,
    /// macOS app name for `e` (open -a), e.g. "Cursor", "Visual Studio Code".
    /// None / "auto" = first installed from IDE_OPTIONS.
    #[serde(default)]
    default_ide: Option<String>,
    /// "auto" | "dark" | "light" — T-0 panel palette.
    /// auto follows terminal (COLORFGBG / MC_UI_THEME) then OS appearance.
    #[serde(default = "default_theme")]
    ui_theme: String,
    /// Absolute path for workspace scan root (Finder picker). Wins over MC_WORKSPACE_ROOT.
    #[serde(default)]
    workspace_root: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "auto".into()
}

/// Cycle order for Settings → Default IDE (`e` key).
/// Windsurf was rebranded to Devin Desktop (Cognition); keep Windsurf as open fallback.
const IDE_OPTIONS: &[&str] = &[
    "auto",
    "Cursor",
    "Visual Studio Code",
    "Zed",
    "Devin Desktop",
];

const UI_THEME_OPTIONS: &[&str] = &["auto", "dark", "light"];

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            splash: true,
            default_agent: None,
            default_ide: None,
            ui_theme: default_theme(),
            workspace_root: None,
        }
    }
}

/// Palette that stays readable on both terminal backgrounds.
#[derive(Clone, Copy)]
struct Theme {
    bg: Color,
    text: Color,
    muted: Color,
    dim: Color,
    key: Color,
    border: Color,
    /// Unselected agent chip / list row.
    soft: Color,
}

impl Theme {
    fn dark() -> Self {
        Self {
            bg: Color::Rgb(20, 20, 20),
            text: Color::White,
            muted: Color::Gray,
            dim: Color::DarkGray,
            key: Color::White,
            border: Color::DarkGray,
            soft: Color::Gray,
        }
    }

    fn light() -> Self {
        // Stronger contrast muted text (zinc-ish); needs truecolor (Ghostty / xterm.js).
        Self {
            bg: Color::Rgb(250, 250, 250),
            text: Color::Rgb(23, 23, 23),
            muted: Color::Rgb(82, 82, 91),   // zinc-600
            dim: Color::Rgb(113, 113, 122), // zinc-500
            key: Color::Rgb(24, 24, 27),
            border: Color::Rgb(161, 161, 170),
            soft: Color::Rgb(63, 63, 70), // zinc-700
        }
    }

    fn from_name(name: &str) -> Self {
        match resolved_theme_mode(name) {
            "light" => Self::light(),
            _ => Self::dark(),
        }
    }
}

/// Resolve preference to concrete `"light"` | `"dark"`.
fn resolved_theme_mode(preference: &str) -> &'static str {
    if preference.eq_ignore_ascii_case("light") {
        return "light";
    }
    if preference.eq_ignore_ascii_case("dark") {
        return "dark";
    }
    // auto (or unknown) — detect terminal / OS.
    if detect_system_is_light() {
        "light"
    } else {
        "dark"
    }
}

fn format_theme_label(preference: &str) -> String {
    if preference.eq_ignore_ascii_case("auto")
        || (!preference.eq_ignore_ascii_case("light")
            && !preference.eq_ignore_ascii_case("dark"))
    {
        format!("auto ({})", resolved_theme_mode("auto"))
    } else {
        preference.to_ascii_lowercase()
    }
}

/// Best-effort light/dark detection for `ui_theme = auto`.
/// Order: MC_UI_THEME → COLORFGBG → macOS appearance → dark.
fn detect_system_is_light() -> bool {
    if let Ok(v) = env::var("MC_UI_THEME") {
        let v = v.trim();
        if v.eq_ignore_ascii_case("light") {
            return true;
        }
        if v.eq_ignore_ascii_case("dark") {
            return false;
        }
    }

    if let Some(is_light) = colorfgbg_is_light() {
        return is_light;
    }

    #[cfg(target_os = "macos")]
    {
        return macos_appearance_is_light();
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Parse `COLORFGBG` (e.g. `15;0` = light fg / dark bg). Returns None if unset/unparseable.
fn colorfgbg_is_light() -> Option<bool> {
    let v = env::var("COLORFGBG").ok()?;
    let bg = v
        .split([';', ':'])
        .filter(|s| !s.is_empty())
        .last()?
        .trim()
        .parse::<u16>()
        .ok()?;
    // xterm convention: 7 and 15 are light backgrounds; 0–6 / 8–14 are dark-ish.
    Some(bg == 7 || bg == 15)
}

#[cfg(target_os = "macos")]
fn macos_appearance_is_light() -> bool {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

    if let Ok(guard) = CACHE.lock() {
        if let Some((at, is_light)) = *guard {
            if at.elapsed() < Duration::from_secs(5) {
                return is_light;
            }
        }
    }

    // `AppleInterfaceStyle` is "Dark" when dark; the key is missing in light mode.
    let is_light = match Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .output()
    {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            !s.trim().eq_ignore_ascii_case("Dark")
        }
        Err(_) => false, // fall back dark if defaults unavailable
    };

    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((Instant::now(), is_light));
    }
    is_light
}

#[derive(Clone, Serialize, Deserialize)]
struct LauncherStateFile {
    version: u32,
    #[serde(default)]
    last: Option<LastLaunch>,
    #[serde(default)]
    favorites: Vec<String>,
    #[serde(default)]
    agents: HashMap<String, String>,
    #[serde(default)]
    settings: SettingsFile,
}

impl Default for LauncherStateFile {
    fn default() -> Self {
        Self {
            version: 1,
            last: None,
            favorites: Vec::new(),
            agents: HashMap::new(),
            settings: SettingsFile::default(),
        }
    }
}

#[derive(Clone, Default)]
struct LauncherState {
    last: Option<(PathBuf, Action)>,
    favorites: Vec<PathBuf>,
    agents: HashMap<PathBuf, Action>,
    settings: SettingsFile,
}

impl LauncherState {
    fn load() -> Self {
        let path = launcher_state_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(file) = serde_json::from_str::<LauncherStateFile>(&text) else {
            return Self::default();
        };

        let mut favorites = Vec::new();
        let mut seen = HashSet::new();
        for raw in file.favorites {
            let p = PathBuf::from(raw);
            if p.is_dir() && seen.insert(p.clone()) {
                favorites.push(p);
            }
            if favorites.len() >= MAX_FAVORITES {
                break;
            }
        }

        let mut agents = HashMap::new();
        for (raw, action_id) in file.agents {
            let p = PathBuf::from(raw);
            if !p.is_dir() {
                continue;
            }
            if let Some(action) = Action::from_id(&action_id) {
                agents.insert(p, action);
            }
        }

        let last = file.last.and_then(|entry| {
            let cwd = PathBuf::from(entry.cwd);
            if !cwd.is_dir() {
                return None;
            }
            let action = Action::from_id(&entry.action)?;
            Some((cwd, action))
        });

        Self {
            last,
            favorites,
            agents,
            settings: file.settings,
        }
    }

    fn save(&self) {
        let data = data_dir();
        let _ = fs::create_dir_all(&data);

        let favorites: Vec<String> = self
            .favorites
            .iter()
            .filter(|path| path.is_dir())
            .take(MAX_FAVORITES)
            .map(|path| path.display().to_string())
            .collect();

        let mut agents = HashMap::new();
        for (path, action) in &self.agents {
            if path.is_dir() {
                agents.insert(path.display().to_string(), action.id().to_string());
            }
        }

        let last = self.last.as_ref().and_then(|(cwd, action)| {
            if !cwd.is_dir() {
                return None;
            }
            Some(LastLaunch {
                cwd: cwd.display().to_string(),
                action: action.id().to_string(),
            })
        });

        let file = LauncherStateFile {
            version: 1,
            last,
            favorites,
            agents,
            settings: self.settings.clone(),
        };

        if let Ok(json) = serde_json::to_string_pretty(&file) {
            let _ = fs::write(launcher_state_path(), format!("{json}\n"));
        }
    }

    fn default_action(&self) -> Action {
        self.settings
            .default_agent
            .as_deref()
            .and_then(Action::from_id)
            .map(|a| a.resolve_available())
            .unwrap_or_else(Action::first_available)
    }

    fn remember_launch(&mut self, cwd: &Path, action: Action) {
        if !cwd.is_dir() {
            return;
        }
        self.last = Some((cwd.to_path_buf(), action));
        self.agents.insert(cwd.to_path_buf(), action);
        self.save();
    }

    fn agent_for(&self, path: &Path) -> Option<Action> {
        self.agents.get(path).copied()
    }

    fn toggle_favorite(&mut self, path: &Path) -> bool {
        if !path.is_dir() {
            return false;
        }
        if let Some(index) = self.favorites.iter().position(|fav| fav == path) {
            self.favorites.remove(index);
        } else {
            self.favorites.insert(0, path.to_path_buf());
            if self.favorites.len() > MAX_FAVORITES {
                self.favorites.truncate(MAX_FAVORITES);
            }
        }
        self.save();
        true
    }

    fn continue_last(&self) -> Option<Launch> {
        let (cwd, action) = self.last.as_ref()?;
        if !cwd.is_dir() {
            return None;
        }
        Some(Launch {
            cwd: cwd.clone(),
            action: action.resolve_available(),
            init: None,
        })
    }
}

struct Launch {
    action: Action,
    cwd: PathBuf,
    /// When set, run a harness-neutral headless init recipe (argv, no shell).
    init: Option<InitCommand>,
}

#[derive(Default)]
struct UiHitboxes {
    action_row: u16,
    actions: Vec<(u16, u16, Action)>,
    list_top: u16,
    list_height: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Picker,
    Settings,
    /// Finder-style directory browser for workspace root or new-project parent.
    FolderPicker,
    /// Create folder/repo + optional headless agent init.
    NewProject,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FolderPickerPurpose {
    WorkspaceRoot,
    NewProjectParent,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NewProjectField {
    Name,
    Parent,
    Template,
    InitAgent,
    Notes,
    Create,
}

impl NewProjectField {
    fn all() -> &'static [NewProjectField] {
        &[
            NewProjectField::Name,
            NewProjectField::Parent,
            NewProjectField::Template,
            NewProjectField::InitAgent,
            NewProjectField::Notes,
            NewProjectField::Create,
        ]
    }

    fn index(self) -> usize {
        Self::all()
            .iter()
            .position(|f| *f == self)
            .unwrap_or(0)
    }

    fn next(self) -> Self {
        let all = Self::all();
        all[(self.index() + 1) % all.len()]
    }

    fn prev(self) -> Self {
        let all = Self::all();
        all[(self.index() + all.len() - 1) % all.len()]
    }
}

struct NewProjectForm {
    name: String,
    parent: PathBuf,
    template: ProjectTemplate,
    /// None = scaffold only (no agent available or user cycled to skip).
    init_agent: Option<Action>,
    notes: String,
    /// First visible logical line of the notes 3-row viewport.
    notes_scroll: u16,
    field: NewProjectField,
}

impl NewProjectForm {
    fn open(parent: PathBuf, default_agent: Option<Action>) -> Self {
        Self {
            name: String::new(),
            parent,
            template: ProjectTemplate::Agent,
            init_agent: default_agent,
            notes: String::new(),
            notes_scroll: 0,
            field: NewProjectField::Name,
        }
    }
}

#[derive(Clone)]
struct FolderEntry {
    name: String,
    path: PathBuf,
    is_parent: bool,
    is_git: bool,
}

#[derive(Clone)]
struct FolderBrowser {
    cwd: PathBuf,
    entries: Vec<FolderEntry>,
    selected: usize,
    offset: usize,
}

impl FolderBrowser {
    fn open(start: PathBuf) -> Self {
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

    fn keep_selected_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.entries.len() - 1);
    }

    fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn enter_selected(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if entry.path.is_dir() {
            self.cwd = entry.path;
            self.selected = 0;
            self.reload();
        }
    }

    fn go_up(&mut self) {
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

    fn jump(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.cwd = path;
            self.selected = 0;
            self.reload();
        }
    }

    /// Directory that would become the workspace root.
    /// Prefer selected child; `..` means parent; empty list → cwd.
    fn chosen_path(&self) -> PathBuf {
        match self.entries.get(self.selected) {
            Some(e) if e.is_parent => e.path.clone(),
            Some(e) => e.path.clone(),
            None => self.cwd.clone(),
        }
    }

    fn current_path(&self) -> &Path {
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

/// Background CLI install progress (shown under the main panel).
struct InstallUi {
    action: Action,
    fraction: f32,
    message: String,
    /// When set, bar lingers briefly then clears.
    finished_at: Option<Instant>,
    failed: bool,
}

enum InstallEvent {
    Progress {
        action: Action,
        fraction: f32,
        message: String,
    },
    Done {
        action: Action,
    },
    Failed {
        action: Action,
        error: String,
    },
}

struct App {
    state: LauncherState,
    repos: Vec<Repo>,
    visible_repos: Vec<usize>,
    selected_visible: usize,
    selected_action: Action,
    filter: String,
    offset: usize,
    hitboxes: UiHitboxes,
    /// Ephemeral footer flash for side actions (copy, open, errors).
    status: Option<String>,
    status_set_at: Option<Instant>,
    screen: Screen,
    settings_selected: usize,
    /// Finder-style browser when choosing workspace root.
    folder: FolderBrowser,
    folder_purpose: FolderPickerPurpose,
    new_project: NewProjectForm,
    /// Active or finishing CLI install.
    install: Option<InstallUi>,
    install_rx: Option<Receiver<InstallEvent>>,
    /// Hover dwell before auto-install of a missing CLI.
    hover_missing: Option<(Action, Instant)>,
    /// Last drawn panel rect (for progress bar placement).
    panel_area: Rect,
}

impl App {
    fn new() -> Self {
        let state = LauncherState::load();
        let root = workspace_root(&state.settings);
        let repos = discover_repos(&state);
        let visible_repos = (0..repos.len()).collect();
        let default_action = state.default_action();
        let init_default = default_init_agent(&state.settings);
        let mut app = Self {
            state,
            repos,
            visible_repos,
            selected_visible: 0,
            selected_action: default_action,
            filter: String::new(),
            offset: 0,
            hitboxes: UiHitboxes::default(),
            status: None,
            status_set_at: None,
            screen: Screen::Picker,
            settings_selected: 0,
            folder: FolderBrowser::open(root.clone()),
            folder_purpose: FolderPickerPurpose::WorkspaceRoot,
            new_project: NewProjectForm::open(root, init_default),
            install: None,
            install_rx: None,
            hover_missing: None,
            panel_area: Rect::default(),
        };
        app.apply_agent_memory();
        app
    }

    fn open_folder_picker(&mut self) {
        let start = workspace_root(&self.state.settings);
        self.folder = FolderBrowser::open(start);
        self.folder_purpose = FolderPickerPurpose::WorkspaceRoot;
        self.screen = Screen::FolderPicker;
        self.clear_status();
    }

    fn open_new_project(&mut self) {
        let parent = workspace_root(&self.state.settings);
        let init = default_init_agent(&self.state.settings);
        self.new_project = NewProjectForm::open(parent, init);
        self.screen = Screen::NewProject;
        self.clear_status();
    }

    fn open_new_project_parent_picker(&mut self) {
        let start = if self.new_project.parent.is_dir() {
            self.new_project.parent.clone()
        } else {
            workspace_root(&self.state.settings)
        };
        self.folder = FolderBrowser::open(start);
        self.folder_purpose = FolderPickerPurpose::NewProjectParent;
        self.screen = Screen::FolderPicker;
        self.clear_status();
    }

    fn confirm_folder_selection(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.set_status("not a directory");
            return;
        }
        match self.folder_purpose {
            FolderPickerPurpose::WorkspaceRoot => {
                self.state.settings.workspace_root = Some(path.display().to_string());
                self.state.save();
                self.repos = discover_repos(&self.state);
                self.apply_filter();
                self.screen = Screen::Settings;
                self.settings_selected = 4; // workspace root row
                self.set_status(format!("workspace root: {}", display_path(&path)));
            }
            FolderPickerPurpose::NewProjectParent => {
                self.new_project.parent = path.clone();
                self.screen = Screen::NewProject;
                self.new_project.field = NewProjectField::Parent;
                self.set_status(format!("parent: {}", display_path(&path)));
            }
        }
    }

    fn cycle_new_project_init_agent(&mut self, delta: i32) {
        let agents = eligible_init_agents();
        // Cycle: none (scaffold only) + available headless-capable agents.
        if agents.is_empty() {
            self.new_project.init_agent = None;
            self.set_status("no headless init agents available");
            return;
        }
        let current = self.new_project.init_agent;
        let mut idx = 0usize;
        if let Some(cur) = current {
            if let Some(i) = agents.iter().position(|a| *a == cur) {
                idx = i + 1; // offset by None slot
            }
        }
        let len = agents.len() + 1;
        let next = ((idx as i32 + delta).rem_euclid(len as i32)) as usize;
        self.new_project.init_agent = if next == 0 {
            None
        } else {
            Some(agents[next - 1])
        };
        let label = self
            .new_project
            .init_agent
            .map(|a| a.label().to_string())
            .unwrap_or_else(|| "none (scaffold only)".into());
        self.set_status(format!("init agent: {label}"));
    }

    fn select_repo_by_path(&mut self, path: &Path) {
        self.repos = discover_repos(&self.state);
        self.filter.clear();
        self.apply_filter();
        if let Some((vis_i, _)) = self
            .visible_repos
            .iter()
            .enumerate()
            .find(|(_, ri)| self.repos.get(**ri).map(|r| r.path == path).unwrap_or(false))
        {
            self.selected_visible = vis_i;
            self.keep_selected_visible();
            self.apply_agent_memory();
        }
    }

    /// Scaffold project; returns Launch for headless init, or None if scaffold-only.
    fn try_create_project(&mut self) -> Result<Option<Launch>, String> {
        let slug = slugify_project_name(&self.new_project.name)?;
        let target = create_scaffold(
            &self.new_project.parent,
            &slug,
            self.new_project.template,
            &self.new_project.notes,
            &display_path,
        )?;
        // Ensure nested / scaffold-only parents still appear via recents.
        record_recent_workspace(&target);
        self.select_repo_by_path(&target);
        self.screen = Screen::Picker;

        let notes = self.new_project.notes.clone();
        let template = self.new_project.template;
        let init_agent = self.new_project.init_agent;

        let Some(action) = init_agent else {
            self.set_status(format!("created {} · scaffold only", display_path(&target)));
            return Ok(None);
        };

        if !action.is_available() {
            self.set_status(format!(
                "created {} · {} not found — run init manually",
                display_path(&target),
                action.label()
            ));
            return Ok(None);
        }

        let kind = action_to_init_kind(action)
            .ok_or_else(|| "shell cannot run headless init".to_string())?;
        let (program, prefix) = resolve_program(action)
            .ok_or_else(|| format!("no command for {}", action.label()))?;
        let prompt = compose_init_prompt(&InitPrompt {
            project_name: slug.clone(),
            template,
            notes,
        });
        let cmd = build_init_command(kind, program, prefix, &target, &prompt);
        self.set_status(format!(
            "created {} · running {} init…",
            display_path(&target),
            action.label()
        ));
        let launch = Launch {
            action,
            cwd: target,
            init: Some(cmd),
        };
        self.prepare_launch(&launch);
        Ok(Some(launch))
    }

    fn install_busy(&self) -> bool {
        matches!(
            &self.install,
            Some(InstallUi {
                finished_at: None,
                ..
            })
        )
    }

    fn start_install(&mut self, action: Action) {
        if action == Action::Shell || action.is_available() {
            return;
        }
        if self.install_busy() {
            self.set_status("install already running");
            return;
        }
        let Some(cmdline) = install_recipe(action) else {
            self.set_status(format!(
                "no install recipe for {} yet",
                action.label().to_ascii_lowercase()
            ));
            return;
        };

        let (tx, rx) = mpsc::channel();
        self.install_rx = Some(rx);
        self.install = Some(InstallUi {
            action,
            fraction: 0.02,
            message: format!("installing {}…", action.label().to_ascii_lowercase()),
            finished_at: None,
            failed: false,
        });
        self.hover_missing = None;

        let label = action.label().to_ascii_lowercase();
        thread::spawn(move || {
            let _ = tx.send(InstallEvent::Progress {
                action,
                fraction: 0.05,
                message: format!("installing {label}…"),
            });

            let mut child = match Command::new("sh")
                .args(["-lc", &cmdline])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(InstallEvent::Failed {
                        action,
                        error: format!("spawn failed: {e}"),
                    });
                    return;
                }
            };

            // Drain stdout/stderr so the child doesn't block; bump progress on output.
            let mut lines = 0u32;
            if let Some(out) = child.stdout.take() {
                let reader = BufReader::new(out);
                for line in reader.lines().flatten() {
                    lines = lines.saturating_add(1);
                    let frac = (0.08 + (lines as f32) * 0.04).min(0.92);
                    let msg = if line.len() > 64 {
                        format!("{}…", &line[..61])
                    } else {
                        line
                    };
                    if tx
                        .send(InstallEvent::Progress {
                            action,
                            fraction: frac,
                            message: msg,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            if let Some(err) = child.stderr.take() {
                let reader = BufReader::new(err);
                for line in reader.lines().flatten() {
                    lines = lines.saturating_add(1);
                    let frac = (0.08 + (lines as f32) * 0.03).min(0.95);
                    let msg = if line.len() > 64 {
                        format!("{}…", &line[..61])
                    } else {
                        line
                    };
                    let _ = tx.send(InstallEvent::Progress {
                        action,
                        fraction: frac,
                        message: msg,
                    });
                }
            }

            match child.wait() {
                Ok(status) if status.success() => {
                    let _ = tx.send(InstallEvent::Done { action });
                }
                Ok(status) => {
                    let _ = tx.send(InstallEvent::Failed {
                        action,
                        error: format!("install exited {status}"),
                    });
                }
                Err(e) => {
                    let _ = tx.send(InstallEvent::Failed {
                        action,
                        error: format!("wait failed: {e}"),
                    });
                }
            }
        });
    }

    fn poll_install(&mut self) {
        let Some(rx) = self.install_rx.as_ref() else {
            // Linger then clear finished bar.
            if let Some(ui) = &self.install {
                if let Some(done_at) = ui.finished_at {
                    if done_at.elapsed() >= Duration::from_millis(1600) {
                        self.install = None;
                    }
                }
            }
            return;
        };

        loop {
            match rx.try_recv() {
                Ok(InstallEvent::Progress {
                    action,
                    fraction,
                    message,
                }) => {
                    self.install = Some(InstallUi {
                        action,
                        fraction,
                        message,
                        finished_at: None,
                        failed: false,
                    });
                }
                Ok(InstallEvent::Done { action }) => {
                    self.install = Some(InstallUi {
                        action,
                        fraction: 1.0,
                        message: format!("{} ready", action.label().to_ascii_lowercase()),
                        finished_at: Some(Instant::now()),
                        failed: false,
                    });
                    self.install_rx = None;
                    // Prefer the newly installed agent if still on that chip.
                    if self.selected_action == action && action.is_available() {
                        // no-op select; availability flips on next is_available()
                    }
                    break;
                }
                Ok(InstallEvent::Failed { action, error }) => {
                    self.install = Some(InstallUi {
                        action,
                        fraction: 1.0,
                        message: format!(
                            "{} failed: {}",
                            action.label().to_ascii_lowercase(),
                            error.to_ascii_lowercase()
                        ),
                        finished_at: Some(Instant::now()),
                        failed: true,
                    });
                    self.install_rx = None;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.install_rx = None;
                    break;
                }
            }
        }

        if let Some(ui) = &self.install {
            if let Some(done_at) = ui.finished_at {
                if done_at.elapsed() >= Duration::from_millis(1600) {
                    self.install = None;
                }
            }
        }
    }

    fn action_at_mouse(&self, column: u16, row: u16) -> Option<Action> {
        if row != self.hitboxes.action_row {
            return None;
        }
        for (start, end, action) in &self.hitboxes.actions {
            if column >= *start && column <= *end {
                return Some(*action);
            }
        }
        None
    }

    fn on_hover_action(&mut self, action: Option<Action>) {
        match action {
            Some(a) if !a.is_available() && a != Action::Shell && install_recipe(a).is_some() => {
                match self.hover_missing {
                    Some((prev, _)) if prev == a => {}
                    _ => {
                        self.hover_missing = Some((a, Instant::now()));
                    }
                }
            }
            _ => {
                self.hover_missing = None;
            }
        }
    }

    fn tick_hover_install(&mut self) {
        const DWELL: Duration = Duration::from_millis(400);
        let Some((action, since)) = self.hover_missing else {
            return;
        };
        if since.elapsed() >= DWELL && !self.install_busy() && !action.is_available() {
            self.start_install(action);
        }
    }

    fn settings_item_count() -> usize {
        // splash, default agent, default IDE, ui theme, workspace root, state dir (ro)
        6
    }

    fn theme(&self) -> Theme {
        Theme::from_name(&self.state.settings.ui_theme)
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
        self.status_set_at = Some(Instant::now());
    }

    fn clear_status(&mut self) {
        self.status = None;
        self.status_set_at = None;
    }

    /// Drop footer flashes after a few seconds so they don't stick forever.
    fn tick_status(&mut self) {
        const STATUS_TTL: Duration = Duration::from_millis(2500);
        if let Some(set_at) = self.status_set_at {
            if set_at.elapsed() >= STATUS_TTL {
                self.clear_status();
            }
        }
    }

    fn selected_repo(&self) -> Option<&Repo> {
        let repo_index = *self.visible_repos.get(self.selected_visible)?;
        self.repos.get(repo_index)
    }

    fn selected_launch(&self) -> Option<Launch> {
        let repo = self.selected_repo()?;
        Some(Launch {
            action: self.selected_action,
            cwd: repo.path.clone(),
            init: None,
        })
    }

    fn apply_agent_memory(&mut self) {
        let Some(repo) = self.selected_repo() else {
            return;
        };
        // Prefer row memory (includes demo fixtures); fall back to state map / default.
        let from_row = repo.remembered_agent;
        let path = repo.path.clone();
        if let Some(action) = from_row.or_else(|| self.state.agent_for(&path)) {
            self.selected_action = action.resolve_available();
        } else {
            self.selected_action = self.state.default_action();
        }
    }

    /// `delta` +1 forward, -1 back (wraps). Splash always flips.
    fn nudge_settings_item(&mut self, delta: i32) {
        match self.settings_selected {
            0 => {
                self.state.settings.splash = !self.state.settings.splash;
                self.state.save();
                self.set_status(if self.state.settings.splash {
                    "splash: on (cold start)"
                } else {
                    "splash: off"
                });
            }
            1 => {
                let actions: Vec<Action> = Action::all()
                    .iter()
                    .copied()
                    .filter(|a| *a != Action::Shell && a.is_available())
                    .collect();
                if actions.is_empty() {
                    self.set_status("no agents available");
                    return;
                }
                let current = self
                    .state
                    .settings
                    .default_agent
                    .as_deref()
                    .and_then(Action::from_id)
                    .unwrap_or(actions[0]);
                let idx = actions.iter().position(|a| *a == current).unwrap_or(0);
                let len = actions.len() as i32;
                let next_idx = (idx as i32 + delta).rem_euclid(len) as usize;
                let next = actions[next_idx];
                self.state.settings.default_agent = Some(next.id().to_string());
                self.state.save();
                self.set_status(format!("default agent: {}", next.label()));
            }
            2 => {
                let current = self
                    .state
                    .settings
                    .default_ide
                    .as_deref()
                    .map(|s| {
                        // Legacy saved value after Windsurf → Devin Desktop rebrand.
                        if s == "Windsurf" {
                            "Devin Desktop"
                        } else {
                            s
                        }
                    })
                    .unwrap_or("auto");
                let idx = IDE_OPTIONS
                    .iter()
                    .position(|name| *name == current)
                    .unwrap_or(0);
                let len = IDE_OPTIONS.len() as i32;
                let next_idx = (idx as i32 + delta).rem_euclid(len) as usize;
                let next = IDE_OPTIONS[next_idx];
                self.state.settings.default_ide = if next == "auto" {
                    None
                } else {
                    Some(next.to_string())
                };
                self.state.save();
                self.set_status(format!("default ide: {next}"));
            }
            3 => {
                let current = self.state.settings.ui_theme.as_str();
                let idx = UI_THEME_OPTIONS
                    .iter()
                    .position(|name| *name == current)
                    .unwrap_or(0);
                let len = UI_THEME_OPTIONS.len() as i32;
                let next_idx = (idx as i32 + delta).rem_euclid(len) as usize;
                let next = UI_THEME_OPTIONS[next_idx];
                self.state.settings.ui_theme = next.to_string();
                self.state.save();
                self.set_status(format!("ui theme: {}", format_theme_label(next)));
            }
            4 => {
                // Workspace root — open Finder-style browser (ignore delta direction).
                self.open_folder_picker();
            }
            _ => {}
        }
    }

    fn select_next_repo(&mut self) {
        self.selected_visible =
            (self.selected_visible + 1).min(self.visible_repos.len().saturating_sub(1));
        self.keep_selected_visible();
        self.apply_agent_memory();
    }

    fn select_previous_repo(&mut self) {
        self.selected_visible = self.selected_visible.saturating_sub(1);
        self.keep_selected_visible();
        self.apply_agent_memory();
    }

    fn select_repo_visible(&mut self, visible_index: usize) {
        if visible_index < self.visible_repos.len() {
            self.selected_visible = visible_index;
            self.keep_selected_visible();
            self.apply_agent_memory();
        }
    }

    fn select_next_action(&mut self) {
        let actions = Action::all();
        let next = (self.selected_action.index() + 1) % actions.len();
        self.selected_action = actions[next];
    }

    fn select_previous_action(&mut self) {
        let actions = Action::all();
        let prev = (self.selected_action.index() + actions.len() - 1) % actions.len();
        self.selected_action = actions[prev];
    }

    fn select_action_by_number(&mut self, number: u8) {
        let index = usize::from(number.saturating_sub(1));
        if let Some(action) = Action::all().get(index) {
            self.selected_action = *action;
        }
    }

    fn keep_selected_visible(&mut self) {
        let height = self.hitboxes.list_height.max(1) as usize;
        if self.selected_visible < self.offset {
            self.offset = self.selected_visible;
        } else if self.selected_visible >= self.offset + height {
            self.offset = self.selected_visible + 1 - height;
        }
    }

    fn push_filter_char(&mut self, value: char) {
        if value.is_control() {
            return;
        }
        self.filter.push(value);
        self.apply_filter();
    }

    fn pop_filter_char(&mut self) {
        self.filter.pop();
        self.apply_filter();
    }

    fn clear_filter(&mut self) {
        self.filter.clear();
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let query = self.filter.trim().to_lowercase();
        self.visible_repos = self
            .repos
            .iter()
            .enumerate()
            .filter_map(|(index, repo)| repo_matches(repo, &query).then_some(index))
            .collect();
        self.selected_visible = 0;
        self.offset = 0;
        self.apply_agent_memory();
    }

    fn toggle_favorite_selected(&mut self) {
        let Some(path) = self.selected_repo().map(|repo| repo.path.clone()) else {
            return;
        };
        if !self.state.toggle_favorite(&path) {
            return;
        }
        self.rebuild_repos_preserving_selection(&path);
    }

    fn rebuild_repos_preserving_selection(&mut self, selected_path: &Path) {
        self.repos = discover_repos(&self.state);
        self.apply_filter();
        if let Some(visible_index) = self
            .visible_repos
            .iter()
            .position(|repo_index| self.repos[*repo_index].path == selected_path)
        {
            self.selected_visible = visible_index;
            self.keep_selected_visible();
        }
        self.apply_agent_memory();
    }

    fn prepare_launch(&mut self, launch: &Launch) {
        // Only remember when the action can actually run.
        if launch.action == Action::Shell || launch.action.is_available() {
            self.state.remember_launch(&launch.cwd, launch.action);
        }
    }

    /// Side actions stay in the launcher (Finder muscle memory).
    fn run_side_action(&mut self, kind: SideAction) {
        let Some(repo) = self.selected_repo().map(|r| r.clone()) else {
            self.set_status("no workspace selected");
            return;
        };

        match kind {
            SideAction::Editor => {
                // Silent success — no "opened in …" flash; errors still surface.
                if let Err(err) =
                    open_in_editor(&repo.path, self.state.settings.default_ide.as_deref())
                {
                    self.set_status(err.to_ascii_lowercase());
                }
            }
            SideAction::Finder => match open_in_finder(&repo.path) {
                Ok(()) => self.set_status("opened in finder"),
                Err(err) => self.set_status(err.to_ascii_lowercase()),
            },
            SideAction::CopyPath => match copy_path_to_clipboard(&repo.path) {
                Ok(()) => self.set_status(format!("copied {}", display_path(&repo.path))),
                Err(err) => self.set_status(err.to_ascii_lowercase()),
            },
            SideAction::GitHub => match open_github(&repo.path) {
                Ok(url) => self.set_status(format!("opened {url}")),
                Err(err) => self.set_status(err.to_ascii_lowercase()),
            },
        }
    }
}

#[derive(Clone, Copy)]
enum SideAction {
    Editor,
    Finder,
    CopyPath,
    GitHub,
}

fn main() -> io::Result<()> {
    let mut first_ui = true;

    loop {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Cold start only: once per `mc` process, not when returning from an agent.
        if first_ui {
            first_ui = false;
            if splash_enabled() {
                let _ = run_splash(&mut terminal);
            }
        }

        let launch = run_app(&mut terminal);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        )?;
        terminal.show_cursor()?;

        match launch? {
            Some(launch) => {
                if launch.action == Action::Shell {
                    replace_with_shell(launch)?;
                } else {
                    run_child_app(launch)?;
                }
            }
            None => return Ok(()),
        }
    }
}

/// Splash: env MC_SPLASH wins; else launcher-state settings.splash (default on).
/// Demo/mock mode skips splash so marketing screenshots are instant.
fn splash_enabled() -> bool {
    if demo_mode_enabled() {
        return false;
    }
    if let Ok(value) = env::var("MC_SPLASH") {
        let v = value.trim().to_ascii_lowercase();
        return !(v == "0" || v == "off" || v == "false" || v == "no");
    }
    LauncherState::load().settings.splash
}

/// Marketing / screenshot mode: fake public-looking workspaces (no personal scan).
/// Enable with `MC_DEMO=1` or `MC_MOCK=1`.
fn demo_mode_enabled() -> bool {
    for key in ["MC_DEMO", "MC_MOCK"] {
        if let Ok(value) = env::var(key) {
            let v = value.trim().to_ascii_lowercase();
            if v == "1" || v == "true" || v == "yes" || v == "on" {
                return true;
            }
        }
    }
    false
}

/// Demo workspaces under `~/work/...` so path column shows clean `~/work/foo` (not /tmp).
fn demo_root() -> PathBuf {
    home_dir().join("work")
}

/// Ensure empty dirs exist so path columns and side-actions don't look broken.
fn ensure_demo_dirs(root: &Path) {
    let names = [
        "northwind",
        "payload",
        "relay",
        "orbit",
        "harbor",
        "signal",
        "ledger",
        "t-0",
    ];
    for name in names {
        let dir = root.join(name);
        let _ = fs::create_dir_all(&dir);
    }
}

/// Hardcoded workspace rows for screenshots / demos (no real discovery).
fn demo_repos() -> Vec<Repo> {
    let root = demo_root();
    ensure_demo_dirs(&root);

    // name, dir, branch, dirty, ahead, agent, badge
    let entries: &[(&str, &str, &str, bool, u32, Option<Action>, &str)] = &[
        ("northwind", "northwind", "main", false, 0, Some(Action::Claude), "★"),
        ("payload", "payload", "feature/auth", true, 2, Some(Action::Grok), "★"),
        ("relay", "relay", "develop", false, 0, Some(Action::Cursor), "recent"),
        ("orbit", "orbit", "main", true, 1, Some(Action::Claude), "recent"),
        ("harbor", "harbor", "release/1.2", false, 0, Some(Action::Codex), ""),
        ("signal", "signal", "main", false, 0, Some(Action::Grok), "last"),
        ("ledger", "ledger", "feat/import", true, 0, Some(Action::Pi), ""),
        ("t-0", "t-0", "main", false, 0, Some(Action::Claude), "root"),
    ];

    entries
        .iter()
        .map(|(name, dir, branch, dirty, ahead, agent, badge)| Repo {
            name: (*name).to_string(),
            path: root.join(dir),
            badge: *badge,
            git_branch: Some((*branch).to_string()),
            git_dirty: *dirty,
            git_ahead: *ahead,
            remembered_agent: *agent,
        })
        .collect()
}

const SPLASH_TOTAL_MS: u64 = 750;
const SPLASH_WORDMARK_MS: u64 = 120;
const SPLASH_TAGLINE_MS: u64 = 280;
const SPLASH_RULE_START_MS: u64 = 450;
const SPLASH_RULE_END_MS: u64 = 600;

struct Splash {
    started: Instant,
}

impl Splash {
    fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn done(&self) -> bool {
        self.elapsed_ms() >= SPLASH_TOTAL_MS
    }

    fn rule_progress(&self) -> f32 {
        let t = self.elapsed_ms();
        if t < SPLASH_RULE_START_MS {
            return 0.0;
        }
        if t >= SPLASH_RULE_END_MS {
            return 1.0;
        }
        (t - SPLASH_RULE_START_MS) as f32 / (SPLASH_RULE_END_MS - SPLASH_RULE_START_MS) as f32
    }
}

fn run_splash(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    let splash = Splash::new();

    loop {
        terminal.draw(|frame| draw_splash(frame, &splash))?;

        if splash.done() {
            return Ok(());
        }

        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => return Ok(()),
                Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) => {
                    return Ok(());
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}

fn draw_splash(frame: &mut Frame<'_>, splash: &Splash) {
    let area = centered_rect(frame.area());
    let block = Block::default()
        .title(format!(" {APP_NAME} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(block, area);

    let inner = inset(area, 2, 1);
    let elapsed = splash.elapsed_ms();

    // Vertical stack: spacer / wordmark / tagline / rule / spacer / skip
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    if elapsed >= SPLASH_WORDMARK_MS {
        let wordmark = Paragraph::new(Line::from(Span::styled(
            APP_NAME,
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(wordmark, chunks[1]);
    }

    if elapsed >= SPLASH_TAGLINE_MS {
        let tagline = Paragraph::new(Line::from(Span::styled(
            APP_TAGLINE,
            Style::default().fg(Color::Gray),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(tagline, chunks[2]);
    }

    // Accent rule grows left → right under the tagline.
    let progress = splash.rule_progress();
    if progress > 0.0 {
        let max_rule = (inner.width as usize / 2).clamp(8, 28);
        let len = ((max_rule as f32) * progress).round() as usize;
        let len = len.max(1).min(max_rule);
        let rule = "─".repeat(len);
        let rule_widget = Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(ACCENT),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(rule_widget, chunks[3]);
    }

    let skip = Paragraph::new(Line::from(Span::styled(
        "any key skip",
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(skip, chunks[6]);
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<Option<Launch>> {
    let mut app = App::new();

    loop {
        app.tick_status();
        app.poll_install();
        app.tick_hover_install();
        terminal.draw(|frame| match app.screen {
            Screen::Picker => draw(frame, &mut app),
            Screen::Settings => draw_settings(frame, &mut app),
            Screen::FolderPicker => draw_folder_picker(frame, &mut app),
            // Popup over the picker — not a separate full-screen UI.
            Screen::NewProject => {
                draw(frame, &mut app);
                draw_new_project_popup(frame, &mut app);
            }
        })?;

        // Poll shorter while a status flash or install bar is active.
        let poll_ms = if app.status.is_some() || app.install.is_some() {
            40
        } else {
            250
        };
        if !event::poll(Duration::from_millis(poll_ms))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if app.screen == Screen::FolderPicker {
                    match key.code {
                        KeyCode::Esc => {
                            app.screen = match app.folder_purpose {
                                FolderPickerPurpose::WorkspaceRoot => Screen::Settings,
                                FolderPickerPurpose::NewProjectParent => Screen::NewProject,
                            };
                            app.clear_status();
                        }
                        KeyCode::Down | KeyCode::Char('j') => app.folder.select_next(),
                        KeyCode::Up | KeyCode::Char('k') => app.folder.select_prev(),
                        KeyCode::Left | KeyCode::Backspace => app.folder.go_up(),
                        KeyCode::Right | KeyCode::Enter => app.folder.enter_selected(),
                        // Use highlighted folder (or parent when on ..)
                        KeyCode::Char(' ') | KeyCode::Char('o') | KeyCode::Char('O') => {
                            let path = app.folder.chosen_path();
                            app.confirm_folder_selection(path);
                        }
                        // Use the directory we're currently viewing (path bar)
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            let path = app.folder.current_path().to_path_buf();
                            app.confirm_folder_selection(path);
                        }
                        KeyCode::Char('h') | KeyCode::Char('H') => {
                            app.folder.jump(home_dir());
                        }
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            let dev = home_dir().join("dev");
                            if dev.is_dir() {
                                app.folder.jump(dev);
                            } else {
                                app.set_status("~/dev not found");
                            }
                        }
                        KeyCode::Char('/') => {
                            app.folder.jump(PathBuf::from("/"));
                        }
                        KeyCode::Char('~') => {
                            app.folder.jump(home_dir());
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.screen == Screen::Settings {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('s') | KeyCode::Char('S') => {
                            app.screen = Screen::Picker;
                            app.clear_status();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.settings_selected = (app.settings_selected + 1)
                                .min(App::settings_item_count().saturating_sub(1));
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.settings_selected = app.settings_selected.saturating_sub(1);
                        }
                        // ← back · → / enter / space forward
                        KeyCode::Left => app.nudge_settings_item(-1),
                        KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                            app.nudge_settings_item(1);
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.screen == Screen::NewProject {
                    let mods = key.modifiers;
                    let ctrl = mods.contains(KeyModifiers::CONTROL);
                    let alt = mods.contains(KeyModifiers::ALT);
                    let super_key = mods.contains(KeyModifiers::SUPER);
                    // Char insert only for plain typing (or Shift for uppercase) —
                    // Ctrl/Alt/Super chords must never type into Name/Notes.
                    let plain_or_shift =
                        mods.is_empty() || mods == KeyModifiers::SHIFT;

                    // Ctrl+Enter always creates from any field.
                    if matches!(key.code, KeyCode::Enter) && ctrl {
                        match app.try_create_project() {
                            Ok(Some(launch)) => return Ok(Some(launch)),
                            Ok(None) => {}
                            Err(err) => app.set_status(err),
                        }
                        continue;
                    }

                    // Ctrl+U / Ctrl+W on Name/Notes (reliable kill-line / kill-word).
                    if ctrl {
                        if let KeyCode::Char(c) = key.code {
                            let lower = c.to_ascii_lowercase();
                            if matches!(
                                app.new_project.field,
                                NewProjectField::Name | NewProjectField::Notes
                            ) && (lower == 'u' || lower == 'w')
                            {
                                let is_notes = app.new_project.field == NewProjectField::Notes;
                                let s = if is_notes {
                                    &mut app.new_project.notes
                                } else {
                                    &mut app.new_project.name
                                };
                                if lower == 'u' {
                                    delete_current_line(s);
                                } else {
                                    delete_last_word(s);
                                }
                                if is_notes {
                                    app.new_project.notes_scroll =
                                        clamp_notes_scroll(&app.new_project.notes, app.new_project.notes_scroll);
                                }
                                continue;
                            }
                        }
                    }

                    match key.code {
                        KeyCode::Esc => {
                            app.screen = Screen::Picker;
                            app.clear_status();
                        }
                        KeyCode::Down => {
                            if app.new_project.field == NewProjectField::Notes {
                                let n = new_project::notes_lines(&app.new_project.notes).len();
                                let max_scroll =
                                    n.saturating_sub(NOTES_VIEWPORT_ROWS as usize) as u16;
                                if app.new_project.notes_scroll < max_scroll {
                                    app.new_project.notes_scroll += 1;
                                } else {
                                    app.new_project.field = app.new_project.field.next();
                                }
                            } else {
                                app.new_project.field = app.new_project.field.next();
                            }
                        }
                        KeyCode::Up => {
                            if app.new_project.field == NewProjectField::Notes {
                                if app.new_project.notes_scroll > 0 {
                                    app.new_project.notes_scroll -= 1;
                                } else {
                                    app.new_project.field = app.new_project.field.prev();
                                }
                            } else {
                                app.new_project.field = app.new_project.field.prev();
                            }
                        }
                        KeyCode::Char('j')
                            if plain_or_shift
                                && !matches!(
                                    app.new_project.field,
                                    NewProjectField::Name | NewProjectField::Notes
                                ) =>
                        {
                            app.new_project.field = app.new_project.field.next();
                        }
                        KeyCode::Char('k')
                            if plain_or_shift
                                && !matches!(
                                    app.new_project.field,
                                    NewProjectField::Name | NewProjectField::Notes
                                ) =>
                        {
                            app.new_project.field = app.new_project.field.prev();
                        }
                        KeyCode::Tab => {
                            app.new_project.field = app.new_project.field.next();
                        }
                        KeyCode::BackTab => {
                            app.new_project.field = app.new_project.field.prev();
                        }
                        KeyCode::Left => match app.new_project.field {
                            NewProjectField::Template => {
                                app.new_project.template = app.new_project.template.cycle();
                            }
                            NewProjectField::InitAgent => {
                                app.cycle_new_project_init_agent(-1);
                            }
                            _ => {}
                        },
                        KeyCode::Right => match app.new_project.field {
                            NewProjectField::Template => {
                                app.new_project.template = app.new_project.template.cycle();
                            }
                            NewProjectField::InitAgent => {
                                app.cycle_new_project_init_agent(1);
                            }
                            NewProjectField::Parent => {
                                app.open_new_project_parent_picker();
                            }
                            NewProjectField::Create => {
                                match app.try_create_project() {
                                    Ok(Some(launch)) => return Ok(Some(launch)),
                                    Ok(None) => {}
                                    Err(err) => app.set_status(err),
                                }
                            }
                            _ => {}
                        },
                        KeyCode::Char(' ') if plain_or_shift => match app.new_project.field {
                            NewProjectField::Name => {
                                if app.new_project.name.chars().count() < NAME_MAX_CHARS {
                                    app.new_project.name.push(' ');
                                }
                            }
                            NewProjectField::Notes => {
                                if app.new_project.notes.chars().count() < NOTES_MAX_CHARS {
                                    app.new_project.notes.push(' ');
                                    app.new_project.notes_scroll =
                                        auto_scroll_notes_to_end(&app.new_project.notes);
                                }
                            }
                            NewProjectField::Template => {
                                app.new_project.template = app.new_project.template.cycle();
                            }
                            NewProjectField::InitAgent => {
                                app.cycle_new_project_init_agent(1);
                            }
                            NewProjectField::Parent => {
                                app.open_new_project_parent_picker();
                            }
                            NewProjectField::Create => {
                                match app.try_create_project() {
                                    Ok(Some(launch)) => return Ok(Some(launch)),
                                    Ok(None) => {}
                                    Err(err) => app.set_status(err),
                                }
                            }
                        },
                        KeyCode::Enter => match app.new_project.field {
                            NewProjectField::Parent => {
                                app.open_new_project_parent_picker();
                            }
                            NewProjectField::Template => {
                                app.new_project.template = app.new_project.template.cycle();
                            }
                            NewProjectField::InitAgent => {
                                app.cycle_new_project_init_agent(1);
                            }
                            NewProjectField::Name => {
                                // Enter on Name advances — does not create.
                                app.new_project.field = app.new_project.field.next();
                            }
                            NewProjectField::Notes => {
                                // Enter inserts newline; create only from Create / Ctrl+Enter.
                                if app.new_project.notes.chars().count() < NOTES_MAX_CHARS {
                                    app.new_project.notes.push('\n');
                                    app.new_project.notes_scroll =
                                        auto_scroll_notes_to_end(&app.new_project.notes);
                                }
                            }
                            NewProjectField::Create => {
                                match app.try_create_project() {
                                    Ok(Some(launch)) => return Ok(Some(launch)),
                                    Ok(None) => {}
                                    Err(err) => app.set_status(err),
                                }
                            }
                        },
                        KeyCode::Backspace => match app.new_project.field {
                            NewProjectField::Name | NewProjectField::Notes => {
                                let is_notes = app.new_project.field == NewProjectField::Notes;
                                let s = if is_notes {
                                    &mut app.new_project.notes
                                } else {
                                    &mut app.new_project.name
                                };
                                if super_key {
                                    delete_current_line(s);
                                } else if alt {
                                    delete_last_word(s);
                                } else {
                                    delete_last_char(s);
                                }
                                if is_notes {
                                    app.new_project.notes_scroll = clamp_notes_scroll(
                                        &app.new_project.notes,
                                        app.new_project.notes_scroll,
                                    );
                                }
                            }
                            _ => {}
                        },
                        KeyCode::Char(c) if !c.is_control() && plain_or_shift => {
                            match app.new_project.field {
                                NewProjectField::Name => {
                                    if app.new_project.name.chars().count() < NAME_MAX_CHARS {
                                        app.new_project.name.push(c);
                                    }
                                }
                                NewProjectField::Notes => {
                                    if app.new_project.notes.chars().count() < NOTES_MAX_CHARS {
                                        app.new_project.notes.push(c);
                                        app.new_project.notes_scroll =
                                            auto_scroll_notes_to_end(&app.new_project.notes);
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                KeyCode::Esc => {
                    if app.filter.is_empty() {
                        return Ok(None);
                    }
                    app.clear_filter();
                }
                KeyCode::Enter => {
                    // Missing agent with a recipe → install instead of failing launch.
                    if app.selected_action != Action::Shell && !app.selected_action.is_available()
                    {
                        app.start_install(app.selected_action);
                        continue;
                    }
                    if let Some(launch) = app.selected_launch() {
                        app.prepare_launch(&launch);
                        return Ok(Some(launch));
                    }
                }
                KeyCode::Backspace => app.pop_filter_char(),
                KeyCode::Down => app.select_next_repo(),
                KeyCode::Up => app.select_previous_repo(),
                KeyCode::Right | KeyCode::Tab => app.select_next_action(),
                KeyCode::Left | KeyCode::BackTab => app.select_previous_action(),
                KeyCode::Char(value @ '1'..='9') => {
                    app.select_action_by_number(value.to_digit(10).unwrap_or(0) as u8);
                    if app.selected_action != Action::Shell && !app.selected_action.is_available()
                    {
                        app.start_install(app.selected_action);
                    }
                }
                KeyCode::Char(' ') if app.filter.is_empty() => {
                    app.clear_status();
                    app.toggle_favorite_selected();
                }
                KeyCode::Char('.') if app.filter.is_empty() => {
                    app.clear_status();
                    if let Some(launch) = app.state.continue_last() {
                        app.prepare_launch(&launch);
                        return Ok(Some(launch));
                    }
                    app.set_status("no last session — open something with enter first");
                }
                KeyCode::Char('s') | KeyCode::Char('S') if app.filter.is_empty() => {
                    app.screen = Screen::Settings;
                    app.settings_selected = 0;
                    app.clear_status();
                }
                KeyCode::Char('n') | KeyCode::Char('N') if app.filter.is_empty() => {
                    app.open_new_project();
                }
                KeyCode::Char('e') | KeyCode::Char('E') if app.filter.is_empty() => {
                    app.run_side_action(SideAction::Editor);
                }
                KeyCode::Char('f') | KeyCode::Char('F') if app.filter.is_empty() => {
                    app.run_side_action(SideAction::Finder);
                }
                KeyCode::Char('c') | KeyCode::Char('C') if app.filter.is_empty() => {
                    app.run_side_action(SideAction::CopyPath);
                }
                KeyCode::Char('g') | KeyCode::Char('G') if app.filter.is_empty() => {
                    app.run_side_action(SideAction::GitHub);
                }
                KeyCode::Char(value) => {
                    app.clear_status();
                    app.push_filter_char(value);
                }
                _ => {}
            }
            }
            Event::Mouse(mouse) if app.screen == Screen::NewProject => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    app.new_project.field = app.new_project.field.next();
                }
                MouseEventKind::ScrollUp => {
                    app.new_project.field = app.new_project.field.prev();
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    // Click a field row inside the popup (matches draw: border + inset + help + fields).
                    let panel = app.panel_area;
                    if panel.width > 0
                        && panel.height > 0
                        && mouse.column >= panel.x
                        && mouse.column < panel.x.saturating_add(panel.width)
                        && mouse.row >= panel.y
                        && mouse.row < panel.y.saturating_add(panel.height)
                    {
                        let inner = inset(panel, 2, 1);
                        // First row of inner = help; fields start at +1
                        let fields_top = inner.y.saturating_add(1);
                        if mouse.row >= fields_top {
                            let row = (mouse.row - fields_top) as usize;
                            let fields = NewProjectField::all();
                            if row < fields.len() {
                                app.new_project.field = fields[row];
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::Mouse(mouse) if app.screen == Screen::FolderPicker => match mouse.kind {
                MouseEventKind::ScrollDown => app.folder.select_next(),
                MouseEventKind::ScrollUp => app.folder.select_prev(),
                MouseEventKind::Down(MouseButton::Left) => {
                    let list_bottom = app.hitboxes.list_top + app.hitboxes.list_height;
                    if mouse.row >= app.hitboxes.list_top && mouse.row < list_bottom {
                        let row = (mouse.row - app.hitboxes.list_top) as usize;
                        let idx = app.folder.offset + row;
                        if idx < app.folder.entries.len() {
                            if idx == app.folder.selected {
                                // Second click on same row → enter
                                app.folder.enter_selected();
                            } else {
                                app.folder.selected = idx;
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::Mouse(mouse) if app.screen == Screen::Picker => match mouse.kind {
                MouseEventKind::ScrollDown => app.select_next_repo(),
                MouseEventKind::ScrollUp => app.select_previous_repo(),
                MouseEventKind::Moved => {
                    let hovered = app.action_at_mouse(mouse.column, mouse.row);
                    app.on_hover_action(hovered);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    let mut hit_action = None;
                    for (start, end, action) in &app.hitboxes.actions {
                        if mouse.row == app.hitboxes.action_row
                            && mouse.column >= *start
                            && mouse.column <= *end
                        {
                            hit_action = Some(*action);
                            break;
                        }
                    }
                    if let Some(action) = hit_action {
                        app.selected_action = action;
                        if !action.is_available() && action != Action::Shell {
                            app.start_install(action);
                        }
                    }

                    let list_bottom = app.hitboxes.list_top + app.hitboxes.list_height;
                    if mouse.row >= app.hitboxes.list_top && mouse.row < list_bottom {
                        let clicked_visible =
                            app.offset + usize::from(mouse.row - app.hitboxes.list_top);
                        if clicked_visible < app.visible_repos.len() {
                            if clicked_visible == app.selected_visible {
                                if app.selected_action != Action::Shell
                                    && !app.selected_action.is_available()
                                {
                                    app.start_install(app.selected_action);
                                } else if let Some(launch) = app.selected_launch() {
                                    app.prepare_launch(&launch);
                                    return Ok(Some(launch));
                                }
                            }
                            app.select_repo_visible(clicked_visible);
                        }
                    }
                }
                _ => {}
            },
            Event::Resize(_, _) => {
                app.keep_selected_visible();
                if app.screen == Screen::FolderPicker {
                    app.folder
                        .keep_selected_visible(app.hitboxes.list_height.max(1) as usize);
                }
            }
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let t = app.theme();
    // Paint full terminal so light mode isn't washed-out ANSI grays on white.
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        frame.area(),
    );

    let area = centered_rect(frame.area());
    app.panel_area = area;
    let block = Block::default()
        .title(format!(" {APP_NAME} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, area);

    let inner = inset(area, 2, 1);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(inner);

    let title = Line::from(vec![
        Span::styled("Launch ", Style::default().fg(t.muted)),
        Span::styled(
            app.selected_action.label(),
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" in a workspace", Style::default().fg(t.muted)),
    ]);
    frame.render_widget(Paragraph::new(title), chunks[0]);

    draw_actions(frame, app, chunks[1], t);
    draw_filter(frame, app, chunks[2], t);
    draw_repos(frame, app, chunks[3], t);

    // While New Project popup is open, status lives only in the modal — not the main footer.
    let footer_second = if app.screen == Screen::NewProject {
        Line::from(Span::styled(
            "n new · e editor · f finder · c copy · g github · hover dim app to install",
            Style::default().fg(t.dim),
        ))
    } else if let Some(status) = &app.status {
        Line::from(Span::styled(status.clone(), Style::default().fg(ACCENT)))
    } else {
        Line::from(Span::styled(
            "n new · e editor · f finder · c copy · g github · hover dim app to install",
            Style::default().fg(t.dim),
        ))
    };
    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("enter", Style::default().fg(t.key)),
            Span::styled(" open  ", Style::default().fg(t.dim)),
            Span::styled(".", Style::default().fg(t.key)),
            Span::styled(" resume  ", Style::default().fg(t.dim)),
            Span::styled("space", Style::default().fg(t.key)),
            Span::styled(" fav  ", Style::default().fg(t.dim)),
            Span::styled("1-9", Style::default().fg(t.key)),
            Span::styled(" app  ", Style::default().fg(t.dim)),
            Span::styled("s", Style::default().fg(t.key)),
            Span::styled(" settings  ", Style::default().fg(t.dim)),
            Span::styled("type", Style::default().fg(t.key)),
            Span::styled(" filter", Style::default().fg(t.dim)),
        ]),
        footer_second,
    ]);
    frame.render_widget(footer, chunks[4]);

    draw_install_bar(frame, app, t);
}

fn draw_install_bar(frame: &mut Frame<'_>, app: &App, t: Theme) {
    let Some(ui) = &app.install else {
        return;
    };
    let panel = app.panel_area;
    let screen = frame.area();
    // Place just below the main panel when there's room; otherwise clamp to bottom.
    let y = (panel.y + panel.height).min(screen.height.saturating_sub(2));
    if y + 1 >= screen.height {
        return;
    }
    let bar_area = Rect {
        x: panel.x,
        y,
        width: panel.width.max(10),
        height: 2,
    };
    if bar_area.y + bar_area.height > screen.y + screen.height {
        return;
    }

    let label = ui.action.label().to_ascii_lowercase();
    let pct = (ui.fraction * 100.0).round() as u16;
    let head = if ui.failed {
        format!("install {label} failed")
    } else if ui.finished_at.is_some() {
        format!("install {label} done")
    } else {
        format!("installing {label}  {pct}%")
    };
    let msg = if ui.message.len() > bar_area.width as usize {
        format!("{}…", &ui.message[..bar_area.width.saturating_sub(1) as usize])
    } else {
        ui.message.clone()
    };

    let color = if ui.failed {
        Color::Red
    } else {
        ACCENT
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            head,
            Style::default().fg(color).bg(t.bg),
        ))),
        Rect {
            x: bar_area.x,
            y: bar_area.y,
            width: bar_area.width,
            height: 1,
        },
    );

    let inner_w = bar_area.width.saturating_sub(2) as usize;
    let filled = ((ui.fraction * inner_w as f32).round() as usize).min(inner_w);
    let bar = format!(
        "[{}{}]",
        "█".repeat(filled),
        "░".repeat(inner_w.saturating_sub(filled))
    );
    // Prefer showing live install log line if it fits; else the bar.
    let line2 = if !msg.is_empty() && ui.finished_at.is_none() && !ui.failed {
        msg
    } else {
        bar
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            line2,
            Style::default().fg(color).bg(t.bg),
        ))),
        Rect {
            x: bar_area.x,
            y: bar_area.y + 1,
            width: bar_area.width,
            height: 1,
        },
    );
}

/// Known install recipes for missing agent CLIs (run via `sh -lc`).
/// Default: only **pinned npm global packages** (no silent curl|bash).
/// Script installs require `MC_ALLOW_SCRIPT_INSTALL=1`.
fn install_recipe(action: Action) -> Option<String> {
    let allow_scripts = env::var("MC_ALLOW_SCRIPT_INSTALL")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false);

    match action {
        Action::Codex => Some("npm install -g @openai/codex".into()),
        Action::Claude => Some("npm install -g @anthropic-ai/claude-code".into()),
        Action::Pi => Some("npm install -g @earendil-works/pi-coding-agent".into()),
        Action::Cursor if allow_scripts => Some(
            "curl -fsSL https://cursor.com/install | bash"
                .into(),
        ),
        Action::Grok if allow_scripts => {
            Some("curl -fsSL https://x.ai/install.sh | bash".into())
        }
        Action::Amp | Action::Devin | Action::Droid | Action::Cursor | Action::Grok => None,
        Action::Shell => None,
    }
}

fn draw_settings(frame: &mut Frame<'_>, app: &mut App) {
    let t = app.theme();
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        frame.area(),
    );

    let area = centered_rect(frame.area());
    let block = Block::default()
        .title(format!(" {APP_NAME} · Settings "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, area);

    let inner = inset(area, 2, 1);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // help
            Constraint::Min(4),    // options
            Constraint::Length(1), // spacer — pushes status one row lower
            Constraint::Length(1), // status
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "←/→ cycle · enter open  · esc/s back",
            Style::default().fg(t.dim),
        ))),
        chunks[0],
    );

    let splash = if app.state.settings.splash {
        "on"
    } else {
        "off"
    };
    let default_agent = app
        .state
        .settings
        .default_agent
        .as_deref()
        .and_then(Action::from_id)
        .map(|a| a.label())
        .unwrap_or("auto (first available)");
    let default_ide = app
        .state
        .settings
        .default_ide
        .as_deref()
        .map(|s| if s == "Windsurf" { "Devin Desktop" } else { s })
        .unwrap_or("auto");
    let ui_theme = format_theme_label(&app.state.settings.ui_theme);
    let root = workspace_root(&app.state.settings);

    let rows = [
        format!("Splash (cold start)     {splash}"),
        format!("Default agent           {default_agent}"),
        format!("Default IDE (e)         {default_ide}"),
        format!("UI theme                {ui_theme}"),
        format!("Workspace root          {}  ↵", display_path(&root)),
        format!("State dir               {}", display_path(&data_dir())),
    ];

    let mut lines = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let selected = i == app.settings_selected;
        let style = if selected {
            Style::default()
                .fg(ACCENT_ON)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD)
        } else if i >= 5 {
            // state dir is read-only
            Style::default().fg(t.dim)
        } else {
            Style::default().fg(t.soft)
        };
        let marker = if selected { ">" } else { " " };
        lines.push(Line::from(Span::styled(format!("{marker} {row}"), style)));
    }
    frame.render_widget(Paragraph::new(lines), chunks[1]);
    // chunks[2] = empty spacer

    // Status one row lower than the option list.
    if let Some(status) = &app.status {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                status.clone(),
                Style::default().fg(ACCENT),
            ))),
            chunks[3],
        );
    }
}

fn draw_folder_picker(frame: &mut Frame<'_>, app: &mut App) {
    let t = app.theme();
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        frame.area(),
    );

    // Slightly taller panel so browsing feels roomy.
    let area = folder_picker_rect(frame.area());
    app.panel_area = area;

    let title_path = display_path(app.folder.current_path());
    let picker_title = match app.folder_purpose {
        FolderPickerPurpose::WorkspaceRoot => " Choose workspace root ",
        FolderPickerPurpose::NewProjectParent => " Choose project parent ",
    };
    let block = Block::default()
        .title(picker_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, area);

    let inner = inset(area, 2, 1);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // path bar
            Constraint::Length(1), // help
            Constraint::Min(6),    // directory list
            Constraint::Length(1), // footer actions
            Constraint::Length(1), // status / shortcuts
        ])
        .split(inner);

    // Path bar — like Finder's location strip.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                pad_or_trim(&title_path, chunks[0].width.saturating_sub(2) as usize),
                Style::default()
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "enter open folder · space use selected · s use this folder · esc cancel",
            Style::default().fg(t.dim),
        ))),
        chunks[1],
    );

    // Directory listing
    let list_area = chunks[2];
    app.hitboxes.list_top = list_area.y;
    app.hitboxes.list_height = list_area.height;
    let visible = list_area.height as usize;
    app.folder.keep_selected_visible(visible.max(1));

    let mut lines = Vec::new();
    for (i, entry) in app
        .folder
        .entries
        .iter()
        .enumerate()
        .skip(app.folder.offset)
        .take(visible)
    {
        let selected = i == app.folder.selected;
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

    // Primary actions row
    let chosen = display_path(&app.folder.chosen_path());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" select → ", Style::default().fg(ACCENT_ON).bg(ACCENT)),
            Span::styled(
                format!(" {}", pad_or_trim(&chosen, list_area.width.saturating_sub(12) as usize)),
                Style::default().fg(t.text),
            ),
        ])),
        chunks[3],
    );

    let footer = if let Some(status) = &app.status {
        Line::from(Span::styled(status.clone(), Style::default().fg(ACCENT)))
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
    frame.render_widget(Paragraph::new(footer), chunks[4]);
}

fn folder_picker_rect(screen: Rect) -> Rect {
    let width = screen.width.min(MAX_WIDTH).max(48);
    let height = screen.height.saturating_sub(2).min(28).max(14);
    Rect {
        x: screen.x + screen.width.saturating_sub(width) / 2,
        y: screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn draw_actions(frame: &mut Frame<'_>, app: &mut App, area: Rect, t: Theme) {
    let mut spans = Vec::new();
    let mut x = area.x;
    app.hitboxes.action_row = area.y;
    app.hitboxes.actions.clear();

    for (index, action) in Action::all().iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
            x += 1;
        }

        // Number prefix doubles as the 1–9 keyboard shortcut.
        // Dim chips when the CLI is missing from PATH.
        let available = action.is_available();
        let label = format!(" {} {} ", index + 1, action.label());
        let width = label.chars().count() as u16;
        let style = if *action == app.selected_action {
            if available {
                Style::default()
                    .fg(ACCENT_ON)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(t.text)
                    .bg(t.dim)
                    .add_modifier(Modifier::BOLD)
            }
        } else if available {
            Style::default().fg(t.soft)
        } else {
            Style::default().fg(t.dim)
        };

        app.hitboxes
            .actions
            .push((x, x.saturating_add(width.saturating_sub(1)), *action));
        spans.push(Span::styled(label, style));
        x += width;
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_filter(frame: &mut Frame<'_>, app: &App, area: Rect, t: Theme) {
    let value = if app.filter.is_empty() {
        "type to filter".to_string()
    } else {
        app.filter.clone()
    };
    let style = if app.filter.is_empty() {
        Style::default().fg(t.dim)
    } else {
        Style::default().fg(ACCENT)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("/", Style::default().fg(t.dim)),
            Span::styled(value, style),
        ])),
        area,
    );
}

fn draw_repos(frame: &mut Frame<'_>, app: &mut App, area: Rect, t: Theme) {
    app.hitboxes.list_top = area.y;
    app.hitboxes.list_height = area.height;
    app.keep_selected_visible();

    let visible_rows = area.height as usize;
    let mut lines = Vec::new();

    for (visible_index, repo_index) in app
        .visible_repos
        .iter()
        .enumerate()
        .skip(app.offset)
        .take(visible_rows)
    {
        let repo = &app.repos[*repo_index];
        let selected = visible_index == app.selected_visible;
        let marker = if selected { ">" } else { " " };
        let style = if selected {
            Style::default()
                .fg(t.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.soft)
        };

        let badge_style = if repo.badge == "★" {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(t.dim)
        };

        // Layout: > name  branch*  agent  path  badge
        let mut spans = vec![
            Span::styled(format!("{marker} "), Style::default().fg(ACCENT)),
            Span::styled(pad_or_trim(&repo.name, 18), style),
            Span::raw(" "),
        ];

        if let Some(branch) = &repo.git_branch {
            let mut branch_label = branch.clone();
            if repo.git_dirty {
                branch_label.push('*');
            }
            if repo.git_ahead > 0 {
                branch_label.push_str(&format!("↑{}", repo.git_ahead));
            }
            let branch_style = if repo.git_dirty {
                Style::default().fg(AMBER)
            } else {
                Style::default().fg(t.dim)
            };
            spans.push(Span::styled(pad_or_trim(&branch_label, 14), branch_style));
        } else {
            spans.push(Span::styled(pad_or_trim("", 14), Style::default()));
        }

        spans.push(Span::raw(" "));
        if let Some(action) = repo.remembered_agent {
            spans.push(Span::styled(
                pad_or_trim(action.label(), 7),
                Style::default().fg(ACCENT),
            ));
        } else {
            spans.push(Span::styled(pad_or_trim("", 7), Style::default()));
        }

        spans.push(Span::raw(" "));
        let path_width = area.width.saturating_sub(50) as usize;
        spans.push(Span::styled(
            pad_or_trim(&display_path(&repo.path), path_width),
            Style::default().fg(t.dim),
        ));
        spans.push(Span::styled(repo.badge, badge_style));

        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No workspaces match this filter",
            Style::default().fg(t.dim),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn centered_rect(screen: Rect) -> Rect {
    let width = screen.width.min(MAX_WIDTH).max(40);
    let height = screen.height.saturating_sub(4).min(24).max(12);
    Rect {
        x: screen.x + screen.width.saturating_sub(width) / 2,
        y: screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect {
        x: area.x + horizontal,
        y: area.y + vertical,
        width: area.width.saturating_sub(horizontal * 2),
        height: area.height.saturating_sub(vertical * 2),
    }
}

fn discover_repos(state: &LauncherState) -> Vec<Repo> {
    if demo_mode_enabled() {
        return demo_repos();
    }

    let home = home_dir();
    let data = data_dir();
    let root = workspace_root(&state.settings);
    let mut repos = Vec::new();
    let mut seen = HashSet::new();

    // 1) Favorites first (pin order, newest pin first).
    for path in &state.favorites {
        add_repo(&mut repos, &mut seen, path.clone(), "★", state);
    }

    // 2) Recents.
    for recent in read_recent_workspaces(&data) {
        add_repo(&mut repos, &mut seen, recent, "recent", state);
    }

    // 3) Last terminal cwd.
    if let Some(last_cwd) = read_last_cwd(&data) {
        add_repo(&mut repos, &mut seen, last_cwd, "last", state);
    }

    // 4) Workspace root + git children.
    add_repo(&mut repos, &mut seen, root.clone(), "root", state);

    if let Ok(entries) = fs::read_dir(&root) {
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.file_name()?.to_string_lossy().starts_with('.') {
                    return None;
                }
                if path.join(".git").exists() {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();
        paths.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));

        for path in paths {
            add_repo(&mut repos, &mut seen, path, "", state);
        }
    }

    if repos.is_empty() {
        add_repo(&mut repos, &mut seen, home, "home", state);
    }

    repos
}

fn add_repo(
    repos: &mut Vec<Repo>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
    badge: &'static str,
    state: &LauncherState,
) {
    if !path.is_dir() || !seen.insert(path.clone()) {
        return;
    }

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let (git_branch, git_dirty, git_ahead) = inspect_git(&path);
    let remembered_agent = state.agent_for(&path);
    repos.push(Repo {
        name,
        path,
        badge,
        git_branch,
        git_dirty,
        git_ahead,
        remembered_agent,
    });
}

/// Fast-ish git snapshot for row metadata. Failures → no git badge.
fn inspect_git(path: &Path) -> (Option<String>, bool, u32) {
    if !path.join(".git").exists() {
        // Also accept worktrees / nested: try rev-parse
        let ok = Command::new("git")
            .args(["-C", &path.display().to_string(), "rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            return (None, false, 0);
        }
    }

    let path_str = path.display().to_string();

    let branch = Command::new("git")
        .args(["-C", &path_str, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty() && s != "HEAD");

    let dirty = Command::new("git")
        .args(["-C", &path_str, "status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let ahead = Command::new("git")
        .args(["-C", &path_str, "rev-list", "--count", "@{u}..HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(0);

    (branch, dirty, ahead)
}

fn open_in_finder(path: &Path) -> Result<(), String> {
    let status = Command::new("open")
        .arg(path)
        .status()
        .map_err(|e| format!("Finder: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Finder: open failed".into())
    }
}

fn copy_path_to_clipboard(path: &Path) -> Result<(), String> {
    let text = path.display().to_string();
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("pbcopy: {e}"))?;
    use std::io::Write;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("pbcopy write: {e}"))?;
    }
    let status = child.wait().map_err(|e| format!("pbcopy: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("pbcopy failed".into())
    }
}

/// Open the workspace folder in a GUI editor / IDE.
///
/// Order: `MC_EDITOR` env → settings default IDE → auto-detect apps → CLI fallbacks.
/// Note: `~/.local/bin/cursor` is often an **agent shim**, not the IDE — we use `open -a`.
fn open_in_editor(path: &Path, preferred_ide: Option<&str>) -> Result<String, String> {
    let path_str = path.display().to_string();

    // 1) Explicit editor command env (argv only — no shell interpolation of path).
    for key in ["MC_EDITOR", "VISUAL", "EDITOR"] {
        if let Ok(cmd) = env::var(key) {
            let cmd = cmd.trim();
            if cmd.is_empty() {
                continue;
            }
            let mut parts = cmd.split_whitespace();
            let Some(bin) = parts.next() else {
                continue;
            };
            let mut args: Vec<String> = parts.map(|s| s.to_string()).collect();
            args.push(path_str.clone());
            if Command::new(bin).args(&args).spawn().is_ok() {
                return Ok(bin.to_string());
            }
        }
    }

    let try_open_app = |name: &str| -> bool {
        Command::new("open")
            .args(["-a", name, &path_str])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };

    // 2) User setting: default IDE (macOS app name).
    if let Some(name) = preferred_ide {
        let name = name.trim();
        if !name.is_empty() && name != "auto" {
            // Devin Desktop is the Windsurf rebrand; try both app names.
            let aliases: &[&str] = if name == "Devin Desktop" || name == "Windsurf" {
                &["Devin Desktop", "Windsurf"]
            } else {
                &[name]
            };
            for alias in aliases {
                if try_open_app(alias) {
                    return Ok((*alias).to_string());
                }
            }
        }
    }

    // 3) Auto: known app bundles, then open -a by name.
    let app_candidates = [
        ("Cursor", "/Applications/Cursor.app"),
        ("Visual Studio Code", "/Applications/Visual Studio Code.app"),
        ("Zed", "/Applications/Zed.app"),
        ("Devin Desktop", "/Applications/Devin Desktop.app"),
        ("Windsurf", "/Applications/Windsurf.app"), // legacy install name
    ];
    for (name, app_path) in app_candidates {
        if Path::new(app_path).exists() && try_open_app(name) {
            return Ok(name.into());
        }
    }
    for name in ["Cursor", "Visual Studio Code", "Zed", "Devin Desktop", "Windsurf"] {
        if try_open_app(name) {
            return Ok(name.into());
        }
    }

    // 4) GUI CLIs — skip the broken `cursor` agent-shim.
    for bin in ["code", "subl", "zed", "windsurf"] {
        if command_available(bin) && Command::new(bin).arg(&path_str).spawn().is_ok() {
            return Ok(bin.into());
        }
    }

    Err("no ide found — set default ide in settings (s) or mc_editor".into())
}

fn open_github(path: &Path) -> Result<String, String> {
    let path_str = path.display().to_string();
    let output = Command::new("git")
        .args(["-C", &path_str, "remote", "get-url", "origin"])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err("no git remote 'origin'".into());
    }
    let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if remote.is_empty() {
        return Err("empty origin url".into());
    }
    let url = remote_to_https(&remote).ok_or_else(|| format!("unsupported remote: {remote}"))?;
    let status = Command::new("open")
        .arg(&url)
        .status()
        .map_err(|e| format!("open: {e}"))?;
    if status.success() {
        Ok(url)
    } else {
        Err("failed to open browser".into())
    }
}

/// git@github.com:org/repo.git → https://github.com/org/repo
fn remote_to_https(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches(".git");
    if let Some(rest) = remote.strip_prefix("git@github.com:") {
        return Some(format!("https://github.com/{rest}"));
    }
    if let Some(rest) = remote.strip_prefix("ssh://git@github.com/") {
        return Some(format!("https://github.com/{rest}"));
    }
    if remote.starts_with("https://github.com/") || remote.starts_with("http://github.com/") {
        return Some(remote.to_string());
    }
    // Generic https remotes: open as-is
    if remote.starts_with("https://") || remote.starts_with("http://") {
        return Some(remote.to_string());
    }
    // git@host:path
    if let Some((user_host, path)) = remote.split_once(':') {
        if let Some(host) = user_host.strip_prefix("git@") {
            return Some(format!("https://{host}/{path}"));
        }
    }
    None
}

fn launcher_state_path() -> PathBuf {
    data_dir().join("launcher-state.json")
}

fn read_last_cwd(data: &Path) -> Option<PathBuf> {
    let state_path = data.join("terminal-state.json");
    let text = fs::read_to_string(state_path).ok()?;
    let key = "\"lastCwd\"";
    let after_key = text.split(key).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let raw = after_colon.strip_prefix('"')?.split('"').next()?;
    let path = PathBuf::from(raw);
    path.is_dir().then_some(path)
}

fn recent_workspaces_path(data: &Path) -> PathBuf {
    data.join("recent-workspaces.txt")
}

fn read_recent_workspaces(data: &Path) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    fs::read_to_string(recent_workspaces_path(data))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_dir() && seen.insert(path.clone()))
        .take(MAX_RECENTS)
        .collect()
}

fn record_recent_workspace(path: &Path) {
    if !path.is_dir() {
        return;
    }

    let data = data_dir();
    let recents_path = recent_workspaces_path(&data);
    let mut recents = vec![path.to_path_buf()];
    let mut seen = HashSet::from([path.to_path_buf()]);

    for recent in read_recent_workspaces(&data) {
        if seen.insert(recent.clone()) {
            recents.push(recent);
        }
        if recents.len() >= MAX_RECENTS {
            break;
        }
    }

    if let Some(parent) = recents_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let text = recents
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(recents_path, format!("{text}\n"));
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Canonical data dir: `~/.t-0`.
/// One-shot migrates `~/.mission-control` or `~/.grok-mission-control` when modern is missing.
/// `MC_DATA_DIR` overrides, except when it still points at a legacy path after migrate.
fn data_dir() -> PathBuf {
    let home = home_dir();
    let modern = home.join(".t-0");
    let legacies = [
        home.join(".mission-control"),
        home.join(".grok-mission-control"),
    ];

    if !modern.exists() {
        for legacy in &legacies {
            if !legacy.is_dir() {
                continue;
            }
            match fs::rename(legacy, &modern) {
                Ok(()) => {
                    eprintln!(
                        "[t0] Migrated data dir: {} → {}",
                        legacy.display(),
                        modern.display()
                    );
                    break;
                }
                Err(err) => {
                    eprintln!(
                        "[t0] Could not migrate {} → {}: {err}",
                        legacy.display(),
                        modern.display()
                    );
                }
            }
        }
    }

    if let Ok(value) = env::var("MC_DATA_DIR") {
        let path = expand_path(&value);
        // Stale LaunchAgent env after rename — use modern.
        if legacies.iter().any(|l| path == *l) && modern.is_dir() {
            return modern;
        }
        return path;
    }

    if modern.is_dir() {
        return modern;
    }
    for legacy in &legacies {
        if legacy.is_dir() {
            return legacy.clone();
        }
    }
    modern
}

/// Resolve workspace scan root.
/// Order: settings (Finder picker) → MC_WORKSPACE_ROOT → GROK_TERMINAL_START_CWD → ~/dev → $HOME.
fn workspace_root(settings: &SettingsFile) -> PathBuf {
    if let Some(ref raw) = settings.workspace_root {
        let path = expand_path(raw);
        if path.is_dir() {
            return path;
        }
    }
    if let Ok(value) = env::var("MC_WORKSPACE_ROOT") {
        let path = expand_path(&value);
        if path.is_dir() {
            return path;
        }
    }
    if let Ok(value) = env::var("GROK_TERMINAL_START_CWD") {
        let path = expand_path(&value);
        if path.is_dir() {
            return path;
        }
    }
    let home = home_dir();
    let dev = home.join("dev");
    if dev.is_dir() {
        dev
    } else {
        home
    }
}

fn expand_path(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

fn command_available(command: &str) -> bool {
    let first = command.split_whitespace().next().unwrap_or(command);
    if first.contains('/') {
        return Path::new(first).is_file();
    }
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|dir| {
        let candidate = dir.join(first);
        candidate.is_file()
    })
}

fn repo_matches(repo: &Repo, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let haystack = format!("{} {}", repo.name, display_path(&repo.path)).to_lowercase();
    query
        .split_whitespace()
        .all(|part| haystack.contains(part))
}

fn display_path(path: &Path) -> String {
    let home = home_dir();
    if let Ok(stripped) = path.strip_prefix(&home) {
        return format!("~/{}", stripped.display());
    }
    path.display().to_string()
}

fn pad_or_trim(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let count = value.chars().count();
    if count <= width {
        return format!("{value:<width$}");
    }

    if width <= 1 {
        return ".".to_string();
    }

    let suffix = "...";
    let keep = width.saturating_sub(suffix.len()).max(1);
    let mut output = value.chars().take(keep).collect::<String>();
    output.push_str(&suffix[..suffix.len().min(width.saturating_sub(output.len()))]);
    output
}

fn replace_with_shell(launch: Launch) -> io::Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    record_recent_workspace(&launch.cwd);

    #[cfg(unix)]
    {
        if launch.action != Action::Shell {
            unreachable!("only shell should replace launcher");
        }
        let error = Command::new(&shell)
            .arg("-l")
            .current_dir(&launch.cwd)
            .exec();
        return Err(error);
    }

    #[cfg(not(unix))]
    {
        if launch.action != Action::Shell {
            unreachable!("only shell should replace launcher");
        }
        let status = Command::new(&shell).current_dir(&launch.cwd).status()?;
        std::process::exit(status.code().unwrap_or(0));
    }
}

fn run_child_app(launch: Launch) -> io::Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    record_recent_workspace(&launch.cwd);

    // Harness-neutral headless init: argv only, no shell interpolation of user notes.
    if let Some(init) = launch.init {
        if env::var("MC_INIT_DRY_RUN").ok().as_deref() == Some("1") {
            eprintln!(
                "[{APP_NAME} init dry-run] {} {:?} cwd={}",
                init.program,
                init.args,
                init.cwd.display()
            );
            eprintln!("Press enter to return to {APP_NAME}...");
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
            return Ok(());
        }
        eprintln!(
            "[{APP_NAME}] headless init via {} in {}",
            launch.action.label(),
            init.cwd.display()
        );
        let status = Command::new(&init.program)
            .args(&init.args)
            .current_dir(&init.cwd)
            .status()?;
        if !status.success() {
            eprintln!(
                "[{} init exited with {}]",
                launch.action.label(),
                status
            );
            eprintln!("Press enter to return to {APP_NAME}...");
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
        } else if !env_flag_on("MC_INIT_NO_PAUSE") {
            // Pause on success so the agent summary stays readable (set MC_INIT_NO_PAUSE=1 to skip).
            eprintln!("init finished — press enter");
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
        }
        return Ok(());
    }

    let Some(command) = launch.action.resolve_command() else {
        return replace_with_shell(launch);
    };

    if !launch.action.is_available() {
        eprintln!(
            "[{}] not found on PATH (looked for `{}`). Install the CLI or set {}.",
            launch.action.label(),
            command,
            launch
                .action
                .env_command_key()
                .unwrap_or("MC_*_COMMAND"),
        );
        eprintln!("Press enter to return to {APP_NAME}...");
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
        return Ok(());
    }

    let status = Command::new(&shell)
        .arg("-lc")
        .arg(format!("exec {command}"))
        .current_dir(&launch.cwd)
        .status()?;

    if !status.success() {
        eprintln!("[{} exited with {}]", launch.action.label(), status);
        eprintln!("Press enter to return to {APP_NAME}...");
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
    }

    Ok(())
}

// ── New project: form UI (pure helpers live in `new_project`) ──────────────

fn eligible_init_agents() -> Vec<Action> {
    Action::all()
        .iter()
        .copied()
        .filter(|a| *a != Action::Shell && a.is_available())
        .collect()
}

fn default_init_agent(settings: &SettingsFile) -> Option<Action> {
    let eligible = eligible_init_agents();
    if eligible.is_empty() {
        return None;
    }
    if let Some(id) = settings.default_agent.as_deref() {
        if let Some(action) = Action::from_id(id) {
            if eligible.iter().any(|a| *a == action) {
                return Some(action);
            }
        }
    }
    if eligible.iter().any(|a| *a == Action::Grok) {
        return Some(Action::Grok);
    }
    Some(eligible[0])
}

/// Split resolved command into program + argv prefix.
/// Whitespace-split only — paths with spaces in custom `GROK_TERMINAL_*_COMMAND` /
/// `MC_*_COMMAND` env overrides are not supported.
fn resolve_program(action: Action) -> Option<(String, Vec<String>)> {
    let cmd = action.resolve_command()?;
    let mut parts = cmd.split_whitespace();
    let program = parts.next()?.to_string();
    let prefix: Vec<String> = parts.map(|s| s.to_string()).collect();
    Some((program, prefix))
}

fn action_to_init_kind(action: Action) -> Option<InitAgentKind> {
    match action {
        Action::Grok => Some(InitAgentKind::Grok),
        Action::Codex => Some(InitAgentKind::Codex),
        Action::Pi => Some(InitAgentKind::Pi),
        Action::Cursor => Some(InitAgentKind::Cursor),
        Action::Claude => Some(InitAgentKind::Claude),
        Action::Amp => Some(InitAgentKind::Amp),
        Action::Devin => Some(InitAgentKind::Devin),
        Action::Droid => Some(InitAgentKind::Droid),
        Action::Shell => None,
    }
}

fn init_agent_elevated(action: Action) -> bool {
    action_to_init_kind(action)
        .map(|k| k.elevated_autonomy())
        .unwrap_or(false)
}

fn field_row_style(
    selected: bool,
    dim_placeholder: bool,
    t: &Theme,
) -> Style {
    if selected {
        Style::default()
            .fg(ACCENT_ON)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else if dim_placeholder {
        Style::default().fg(t.dim).bg(t.bg)
    } else {
        Style::default().fg(t.soft).bg(t.bg)
    }
}

/// Modal popup over the picker (not a full-screen replacement).
fn draw_new_project_popup(frame: &mut Frame<'_>, app: &mut App) {
    let t = app.theme();
    let area = new_project_popup_rect(frame.area());
    app.panel_area = area;

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

    let inner = inset(area, 2, 1);
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg)),
        inner,
    );

    // help 1 · Name/Parent/Template/Init 4 · Notes label 1 · notes box 3 · Create 1 · status 1
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // help
            Constraint::Length(4), // name / parent / template / init
            Constraint::Length(1), // notes label
            Constraint::Length(NOTES_VIEWPORT_ROWS), // notes box
            Constraint::Length(1), // create
            Constraint::Min(0),    // filler (opaque)
            Constraint::Length(1), // status
        ])
        .split(inner);

    let col_w = chunks[0].width.max(1) as usize;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(
                "tab fields · enter newline in notes · ctrl-enter create · opt-bs word · ctrl-u line · esc",
                col_w,
            ),
            Style::default().fg(t.dim).bg(t.bg),
        )))
        .style(Style::default().bg(t.bg)),
        chunks[0],
    );

    let name_raw = if app.new_project.name.is_empty() {
        "…".to_string()
    } else {
        app.new_project.name.clone()
    };
    let parent_raw = format!("{}  ↵", display_path(&app.new_project.parent));
    let elevated = app
        .new_project
        .init_agent
        .map(init_agent_elevated)
        .unwrap_or(false);
    let init_label = match app.new_project.init_agent {
        Some(a) if elevated => format!("{} · full tools", a.label()),
        Some(a) => a.label().to_string(),
        None => "none (scaffold only)".into(),
    };
    let create_label = match app.new_project.init_agent {
        Some(_) if elevated => "scaffold + headless init · full tools",
        Some(_) => "scaffold + headless init",
        None => "scaffold only",
    };

    let top_rows: [(&str, String, NewProjectField); 4] = [
        ("Name", name_raw, NewProjectField::Name),
        ("Parent", parent_raw, NewProjectField::Parent),
        (
            "Template",
            app.new_project.template.label().to_string(),
            NewProjectField::Template,
        ),
        ("Init agent", init_label, NewProjectField::InitAgent),
    ];

    let field_w = chunks[1].width.max(1) as usize;
    let mut top_lines = Vec::new();
    for (label, value, field) in top_rows {
        let selected = app.new_project.field == field;
        let style = field_row_style(selected, false, &t);
        let marker = if selected { ">" } else { " " };
        let prefix = format!("{marker} {label:<11} ");
        let avail = field_w.saturating_sub(display_width(&prefix));
        // Name: sliding tail keeps caret + recent chars; Parent: front-ellipsize keeps leaf.
        let value_out = match field {
            NewProjectField::Name if selected => {
                let with_caret = if app.new_project.name.is_empty() {
                    format!("▌{value}")
                } else {
                    format!("{value}▌")
                };
                sliding_tail(&with_caret, avail)
            }
            NewProjectField::Name => sliding_tail(&value, avail),
            NewProjectField::Parent => front_ellipsize(&value, avail),
            _ => sliding_tail(&value, avail),
        };
        let raw = format!("{prefix}{value_out}");
        top_lines.push(Line::from(Span::styled(pad_line(&raw, field_w), style)));
    }
    frame.render_widget(
        Paragraph::new(top_lines).style(Style::default().bg(t.bg)),
        chunks[1],
    );

    // Notes label row
    let notes_selected = app.new_project.field == NewProjectField::Notes;
    let notes_label_style = field_row_style(notes_selected, false, &t);
    let notes_marker = if notes_selected { ">" } else { " " };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(&format!("{notes_marker} {:<11}", "Notes"), field_w),
            notes_label_style,
        )))
        .style(Style::default().bg(t.bg)),
        chunks[2],
    );

    // 3-row notes viewport (append-mode caret at end of text).
    app.new_project.notes_scroll =
        clamp_notes_scroll(&app.new_project.notes, app.new_project.notes_scroll);
    let viewport = notes_viewport(&app.new_project.notes, app.new_project.notes_scroll);
    let notes_empty = app.new_project.notes.is_empty();
    let notes_box_w = chunks[3].width.max(1) as usize;
    let end_line_idx = new_project::notes_lines(&app.new_project.notes)
        .len()
        .saturating_sub(1);
    let notes_indent = "  ";
    let notes_avail = notes_box_w.saturating_sub(display_width(notes_indent));
    let mut note_lines = Vec::new();
    for (i, line) in viewport.iter().enumerate() {
        let line_idx = app.new_project.notes_scroll as usize + i;
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
        // Sliding tail so long lines keep the caret / recent input visible.
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
        chunks[3],
    );

    // Create row
    let create_selected = app.new_project.field == NewProjectField::Create;
    let create_style = field_row_style(create_selected, false, &t);
    let create_marker = if create_selected { ">" } else { " " };
    let create_raw = format!("{create_marker} {:<11} {create_label}", "Create");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(&create_raw, chunks[4].width.max(1) as usize),
            create_style,
        )))
        .style(Style::default().bg(t.bg)),
        chunks[4],
    );

    // Filler keeps opacity if popup is taller than content.
    if chunks[5].height > 0 {
        frame.render_widget(
            Block::default().style(Style::default().bg(t.bg)),
            chunks[5],
        );
    }

    let status_w = chunks[6].width.max(1) as usize;
    let status_text = app.status.as_deref().unwrap_or(" ");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pad_line(status_text, status_w),
            Style::default().fg(ACCENT).bg(t.bg),
        )))
        .style(Style::default().bg(t.bg)),
        chunks[6],
    );
}

fn new_project_popup_rect(screen: Rect) -> Rect {
    // Prefer a roomy popup, but never force mins larger than the screen.
    let width = if screen.width >= 44 {
        screen.width.min(76).max(44)
    } else {
        screen.width.max(1)
    };
    // Taller for multi-line notes (help+4 fields+label+3 notes+create+status ≈ 12 + chrome).
    let preferred_h = screen.height.saturating_sub(2).min(22);
    let height = if preferred_h >= 18 {
        preferred_h
    } else {
        screen.height.max(1)
    };
    Rect {
        x: screen.x + screen.width.saturating_sub(width) / 2,
        y: screen.y + screen.height.saturating_sub(height) / 2,
        width: width.min(screen.width.max(1)),
        height: height.min(screen.height.max(1)),
    }
}
