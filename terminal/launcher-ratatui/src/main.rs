mod new_project;
mod new_project_input;
mod new_project_ui;
mod jobs;
mod folder;
mod settings_ui;
mod theme;
mod git_meta;
mod discover;

use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    io::{self, IsTerminal, Write, stdout},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};
// (thread / Stdio live in jobs/git_meta)

use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use new_project::{
    build_init_command, compose_init_prompt, create_scaffold, display_width, ellipsize_end,
    ellipsize_front, pad_line, sliding_tail, slugify_project_name, InitAgentKind,
    InitCommand, InitPrompt, ProjectTemplate,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const MAX_WIDTH: u16 = 92;
const MAX_RECENTS: usize = 20;
const MAX_FAVORITES: usize = 20;
/// Product name (SpaceX-flavored: countdown to liftoff — agents go at T-0).
pub(crate) const APP_NAME: &str = "T-0";
/// Splash / brand line.
const APP_TAGLINE: &str = "go for launch";
/// Accent — orange-500 (#f97316), a little heat for the pad.
pub(crate) const ACCENT: Color = Color::Rgb(249, 115, 22);
/// Text on filled accent chips (dark enough for contrast on orange).
pub(crate) const ACCENT_ON: Color = Color::Rgb(23, 23, 23);
/// Dirty branch / amber metadata (fallback; prefer Theme.warn).
pub(crate) const AMBER: Color = Color::Rgb(180, 120, 0);

/// Braille spinner frames for background jobs (~100 ms each).
/// Idle tips — lowest-priority status-line content (preempted by real flashes).
const TIPS: &[&str] = &[
    ". resumes your last session",
    "space favorites a workspace",
    "1-9 picks an agent without leaving the list",
    "n creates a project · s opens settings",
    "? shows the full keymap",
    "type to filter by name or path",
    "MC_DEMO=1 t0 — screenshot-friendly fake workspaces",
    "hover a missing agent chip to install",
    "theme: Settings → UI theme (auto / light / dark)",
    "shift-enter adds a newline in New Project notes",
];

const TIP_ROTATE: Duration = Duration::from_secs(30);
/// Typewriter + color-ramp frames for tip entry (~40 ms each while reveal > 0).
const TIP_REVEAL_FRAMES: u8 = 8;
/// Soft sparkle while a tip is revealing.
const TIP_SPARKLE: &[char] = &['·', '✦', '✧', '⋆', '✦', '·'];
/// Inner chrome: title + chips + filter + tip/status + keys. `panel_rect` adds borders (2).
const PANEL_CHROME: u16 = 5;
/// Minimum outer panel height (was 12 — too squat on tall terminals).
const MIN_PANEL_HEIGHT: u16 = 22;
/// Floor for list content rows so the picker has a bit of air under short lists.
const MIN_LIST_ROWS: u16 = 14;

#[derive(Clone)]
pub(crate) struct Repo {
    name: String,
    path: PathBuf,
    badge: &'static str,
    git_branch: Option<String>,
    git_dirty: bool,
    git_ahead: u32,
    remembered_agent: Option<Action>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
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

    pub(crate) fn label(self) -> &'static str {
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

    pub(crate) fn is_available(self) -> bool {
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
pub(crate) struct SettingsFile {
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

use theme::{format_theme_label, Theme};

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
pub(crate) struct LauncherState {
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
        })
    }
}

struct Launch {
    action: Action,
    cwd: PathBuf,
}

#[derive(Default)]
struct UiHitboxes {
    action_row: u16,
    actions: Vec<(u16, u16, Action)>,
    list_top: u16,
    list_height: u16,
    /// Per visible list row: `Some(visible_repo_index)` or `None` for separators / empty.
    list_rows: Vec<Option<usize>>,
}

/// Preemptible tips on the row above the keymap (motion budget).
struct TipTicker {
    idx: usize,
    rotated_at: Instant,
    /// 0 = settled; 1..=TIP_REVEAL_FRAMES = typewriter + color ramp in progress.
    reveal: u8,
}

impl Default for TipTicker {
    fn default() -> Self {
        Self {
            idx: 0,
            rotated_at: Instant::now(),
            // Animate the first tip in on cold start.
            reveal: TIP_REVEAL_FRAMES,
        }
    }
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
pub(crate) enum TextDelete {
    Char,
    Word,
    Line,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NewProjectField {
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

    pub(crate) fn next(self) -> Self {
        let all = Self::all();
        all[(self.index() + 1) % all.len()]
    }

    pub(crate) fn prev(self) -> Self {
        let all = Self::all();
        all[(self.index() + all.len() - 1) % all.len()]
    }
}

/// New Project form state — lives in `main`; painted by `new_project_ui`.
#[derive(Clone)]
pub(crate) struct NewProjectForm {
    pub name: String,
    pub parent: PathBuf,
    pub template: ProjectTemplate,
    /// None = scaffold only (no agent available or user cycled to skip).
    pub init_agent: Option<Action>,
    pub notes: String,
    /// First visible logical line of the notes 3-row viewport.
    pub notes_scroll: u16,
    pub field: NewProjectField,
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
    folder: folder::FolderBrowser,
    folder_purpose: folder::FolderPickerPurpose,
    new_project: NewProjectForm,
    /// Background install + headless init (`jobs` module).
    jobs: jobs::Jobs,
    /// Hover dwell before auto-install of a missing CLI.
    hover_missing: Option<(Action, Instant)>,
    /// Last drawn panel rect (for progress bar placement).
    panel_area: Rect,
    /// Async git badges (paint rows first; fill in as inspect_git finishes).
    git_rx: Option<Receiver<(PathBuf, GitMeta)>>,
    git_pending: usize,
    /// When the current git fan-out started — drop channel after timeout so a stuck
    /// mount cannot pin the event loop at 40 ms forever.
    git_started_at: Option<Instant>,
    /// `?` keymap overlay on the picker.
    help_open: bool,
    /// Idle tips in the status line (preempted by real flashes).
    tips: TipTicker,
    /// Status flash color-ramp frames remaining (0 = settled).
    status_reveal: u8,
}

use git_meta::GitMeta;

impl App {
    fn new() -> Self {
        let state = LauncherState::load();
        let root = workspace_root(&state.settings);
        let candidates = discover::discover_candidates(&state);
        let paths: Vec<PathBuf> = candidates.iter().map(|(p, _)| p.clone()).collect();
        let repos = discover::repos_from_candidates(candidates, &state);
        let (git_rx, git_pending) = git_meta::spawn_git_metadata(paths);
        let git_started_at = if git_pending > 0 {
            Some(Instant::now())
        } else {
            None
        };
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
            folder: folder::FolderBrowser::open(root.clone()),
            folder_purpose: folder::FolderPickerPurpose::WorkspaceRoot,
            new_project: NewProjectForm::open(root, init_default),
            jobs: jobs::Jobs::default(),
            hover_missing: None,
            panel_area: Rect::default(),
            git_rx: Some(git_rx),
            git_pending,
            git_started_at,
            help_open: false,
            tips: TipTicker::default(),
            status_reveal: 0,
        };
        app.apply_agent_memory();
        app
    }

    /// Rebuild workspace list from FS only, then fan out git inspect in the background.
    fn refresh_repos(&mut self) {
        let candidates = discover::discover_candidates(&self.state);
        let paths: Vec<PathBuf> = candidates.iter().map(|(p, _)| p.clone()).collect();
        self.repos = discover::repos_from_candidates(candidates, &self.state);
        self.apply_filter();
        let (rx, n) = git_meta::spawn_git_metadata(paths);
        self.git_rx = Some(rx);
        self.git_pending = n;
        self.git_started_at = if n > 0 { Some(Instant::now()) } else { None };
    }

    fn poll_git_meta(&mut self) {
        // Stuck mounts hold a tx clone forever; abandon after 10 s so poll stays 250 ms.
        const GIT_META_TIMEOUT: Duration = Duration::from_secs(10);
        if let Some(started) = self.git_started_at {
            if started.elapsed() >= GIT_META_TIMEOUT && self.git_pending > 0 {
                self.git_rx = None;
                self.git_pending = 0;
                self.git_started_at = None;
                return;
            }
        }

        let Some(rx) = self.git_rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok((path, meta)) => {
                    if let Some(repo) = self.repos.iter_mut().find(|r| r.path == path) {
                        repo.git_branch = meta.branch;
                        repo.git_dirty = meta.dirty;
                        repo.git_ahead = meta.ahead;
                    }
                    self.git_pending = self.git_pending.saturating_sub(1);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.git_rx = None;
                    self.git_pending = 0;
                    self.git_started_at = None;
                    break;
                }
            }
        }
        if self.git_pending == 0 {
            self.git_rx = None;
            self.git_started_at = None;
        }
    }

    fn apply_np_action(&mut self, action: new_project_input::NpAction) {
        use new_project_input::NpAction;
        match action {
            NpAction::None => {}
            NpAction::Close => {
                self.screen = Screen::Picker;
                self.clear_status();
            }
            NpAction::OpenParentPicker => self.open_new_project_parent_picker(),
            NpAction::Create => {
                if let Err(err) = self.try_create_project() {
                    self.set_status(err);
                }
            }
            NpAction::CycleInitAgent(delta) => self.cycle_new_project_init_agent(delta),
        }
    }

    fn open_folder_picker(&mut self) {
        let start = workspace_root(&self.state.settings);
        self.folder = folder::FolderBrowser::open(start);
        self.folder_purpose = folder::FolderPickerPurpose::WorkspaceRoot;
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
        self.folder = folder::FolderBrowser::open(start);
        self.folder_purpose = folder::FolderPickerPurpose::NewProjectParent;
        self.screen = Screen::FolderPicker;
        self.clear_status();
    }

    fn confirm_folder_selection(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.set_status("not a directory");
            return;
        }
        match self.folder_purpose {
            folder::FolderPickerPurpose::WorkspaceRoot => {
                self.state.settings.workspace_root = Some(path.display().to_string());
                self.state.save();
                self.refresh_repos();
                self.screen = Screen::Settings;
                self.settings_selected = 4; // workspace root row
                self.set_status(format!("workspace root: {}", display_path(&path)));
            }
            folder::FolderPickerPurpose::NewProjectParent => {
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
        self.filter.clear();
        self.refresh_repos();
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

    /// Scaffold project; optionally start background headless init (stay in TUI).
    fn try_create_project(&mut self) -> Result<(), String> {
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
            self.set_status(format!("✦ created {} · scaffold only", slug));
            return Ok(());
        };

        if !action.is_available() {
            self.set_status(format!(
                "✦ created {} · {} not found — run init manually",
                slug,
                action.label()
            ));
            return Ok(());
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

        // Remember last launch / agent for this workspace.
        self.prepare_launch(&Launch {
            action,
            cwd: target.clone(),
        });

        self.start_background_init(action, cmd, display_path(&target));
        Ok(())
    }

    fn start_background_init(&mut self, action: Action, cmd: InitCommand, project: String) {
        match self.jobs.start_bg_init(action, cmd, project.clone()) {
            Err(e) => self.set_status(e),
            Ok(Some(status)) => self.set_status(status),
            Ok(None) => {
                self.set_status(format!(
                    "✦ created {project} · {} init…",
                    action.label()
                ));
            }
        }
    }

    fn start_install(&mut self, action: Action) {
        if action == Action::Shell || action.is_available() {
            return;
        }
        match self.jobs.start_install(action, install_recipe(action)) {
            Err(e) => self.set_status(e),
            Ok(()) => {
                self.hover_missing = None;
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
        if since.elapsed() >= DWELL && !self.jobs.install_busy() && !action.is_available() {
            self.start_install(action);
        }
    }


    fn theme(&self) -> Theme {
        Theme::from_name(&self.state.settings.ui_theme)
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
        self.status_set_at = Some(Instant::now());
        // 3-frame fake fade-in (dim→muted→text) at 40 ms poll.
        self.status_reveal = 3;
    }

    fn clear_status(&mut self) {
        self.status = None;
        self.status_set_at = None;
        self.status_reveal = 0;
    }

    /// Drop footer flashes after a few seconds so they don't stick forever.
    fn tick_status(&mut self) {
        const STATUS_TTL: Duration = Duration::from_millis(2500);
        if self.status_reveal > 0 {
            self.status_reveal -= 1;
        }
        if let Some(set_at) = self.status_set_at {
            if set_at.elapsed() >= STATUS_TTL {
                self.clear_status();
            }
        }
    }

    /// Tips: rotate every ~30 s when free; pause while a status flash or job is active.
    fn tick_tips(&mut self) {
        if self.status.is_some() || self.job_busy() {
            return;
        }
        if self.tips.reveal > 0 {
            self.tips.reveal -= 1;
            return;
        }
        if self.tips.rotated_at.elapsed() >= TIP_ROTATE {
            self.tips.idx = (self.tips.idx + 1) % TIPS.len();
            self.tips.rotated_at = Instant::now();
            self.tips.reveal = TIP_REVEAL_FRAMES;
        }
    }

    fn job_busy(&self) -> bool {
        self.jobs.any_active()
    }

    /// Color ramp into ACCENT (dim → muted → orange) while revealing.
    fn tip_style(&self, t: Theme) -> Style {
        match self.tips.reveal {
            6..=8 => Style::default().fg(t.dim),
            3..=5 => Style::default().fg(t.muted),
            1..=2 => Style::default().fg(ACCENT).add_modifier(Modifier::DIM),
            _ => Style::default().fg(ACCENT),
        }
    }

    fn tip_sparkle(&self) -> char {
        if self.tips.reveal == 0 {
            return '✦';
        }
        let i = (self.tips.rotated_at.elapsed().as_millis() as usize / 40) % TIP_SPARKLE.len();
        TIP_SPARKLE[i]
    }

    /// Progressive reveal of tip text (typewriter) during the entrance ramp.
    fn tip_visible_text(&self, tip: &str) -> String {
        if self.tips.reveal == 0 {
            return tip.to_string();
        }
        let chars: Vec<char> = tip.chars().collect();
        if chars.is_empty() {
            return String::new();
        }
        // reveal 8 → ~12% shown … reveal 1 → ~88% shown
        let progress = 1.0 - (self.tips.reveal as f32 / f32::from(TIP_REVEAL_FRAMES));
        let n = ((chars.len() as f32) * progress).ceil() as usize;
        chars.into_iter().take(n.clamp(1, tip.chars().count())).collect()
    }

    fn status_style(&self, t: Theme) -> Style {
        match self.status_reveal {
            3 => Style::default().fg(t.dim),
            2 => Style::default().fg(t.muted),
            1 | 0 => Style::default().fg(ACCENT),
            _ => Style::default().fg(ACCENT),
        }
    }

    /// One-frame brand paint before exec (zero added latency).
    fn paint_liftoff(&mut self, launch: &Launch) {
        let name = launch
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| display_path(&launch.cwd));
        self.status = Some(format!(
            "T-0 · liftoff → {} @ {}",
            launch.action.label(),
            name
        ));
        self.status_set_at = Some(Instant::now());
        self.status_reveal = 0; // full strength for the last frame
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
        if self.visible_repos.is_empty() || height == 0 {
            return;
        }
        if self.selected_visible < self.offset {
            self.offset = self.selected_visible;
            return;
        }
        // Account for section separators: they eat visual rows but not repo indices.
        // With a fixed-height list, repo-only math leaves the highlight below the fold.
        while self.offset < self.selected_visible
            && self.visual_rows_span(self.offset, self.selected_visible) > height
        {
            self.offset += 1;
        }
    }

    /// Visual list rows needed to paint repos `start..=end` (inclusive), including
    /// separators when not filtering. Matches `draw_repos` insertion rules.
    fn visual_rows_span(&self, start: usize, end: usize) -> usize {
        if end < start || end >= self.visible_repos.len() {
            return 0;
        }
        if !self.filter.is_empty() {
            return end - start + 1;
        }
        let mut rows = 0usize;
        let mut prev_badge: Option<&str> = if start > 0 {
            self.repos
                .get(self.visible_repos[start - 1])
                .map(|r| r.badge)
        } else {
            None
        };
        for vis in start..=end {
            let Some(repo) = self.repos.get(self.visible_repos[vis]) else {
                continue;
            };
            if prev_badge != Some(repo.badge) {
                rows += 1; // separator before group
                prev_badge = Some(repo.badge);
            }
            rows += 1;
        }
        rows
    }

    fn push_filter_char(&mut self, value: char) {
        if value.is_control() {
            return;
        }
        self.filter.push(value);
        self.apply_filter();
    }

    /// Bracketed paste into the workspace filter (printable chars only).
    fn push_filter_paste(&mut self, text: &str) {
        let mut changed = false;
        for c in text.chars() {
            if c.is_control() {
                continue;
            }
            self.filter.push(c);
            changed = true;
        }
        if changed {
            self.apply_filter();
        }
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
        self.refresh_repos();
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
    install_panic_hook();

    // P3: build app (starts async git) before splash so badges fill during splash.
    let mut app = App::new();
    let mut first_ui = true;

    loop {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;
        // No terminal.clear() here: in ratatui 0.30 it queries cursor position
        // (ESC[6n) and errors out after 2s if the reply is late — a browser
        // client mid-history-replay answers late, crash-looping the launcher.
        // Fullscreen draw repaints every cell anyway, so clear is redundant.

        // Cold start only: once per `mc` process, not when returning from an agent.
        if first_ui {
            first_ui = false;
            if splash_enabled() {
                let _ = run_splash(&mut terminal, app.theme());
            }
        }

        let launch = run_app(&mut terminal, &mut app);

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

/// Restore raw/alt/mouse if we panic while the TUI owns the PTY (browser or local).
/// Installed once at process start — safe if already restored.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(
            out,
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        );
        let _ = out.flush();
        previous(info);
    }));
}

/// Splash: env MC_SPLASH wins; else launcher-state settings.splash (default on).
/// Demo/mock mode skips splash so marketing screenshots are instant.
fn splash_enabled() -> bool {
    if discover::demo_mode_enabled() {
        return false;
    }
    if let Ok(value) = env::var("MC_SPLASH") {
        let v = value.trim().to_ascii_lowercase();
        return !(v == "0" || v == "off" || v == "false" || v == "no");
    }
    LauncherState::load().settings.splash
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
    theme: Theme,
) -> io::Result<()> {
    let splash = Splash::new();

    loop {
        terminal.draw(|frame| draw_splash(frame, &splash, theme))?;

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

fn draw_splash(frame: &mut Frame<'_>, splash: &Splash, t: Theme) {
    // Same full-terminal paint as the picker so light/dark match (no bare ANSI default).
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        frame.area(),
    );

    // Match main panel minimum silhouette (taller + themed).
    let area = panel_rect(frame.area(), PANEL_CHROME.saturating_add(MIN_LIST_ROWS));
    let block = Block::default()
        .title(format!(" {APP_NAME} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, area);

    let inner = inset(area, PANEL_PAD_H, PANEL_PAD_V);
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
                .bg(t.bg)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(wordmark, chunks[1]);
    }

    if elapsed >= SPLASH_TAGLINE_MS {
        let tagline = Paragraph::new(Line::from(Span::styled(
            APP_TAGLINE,
            Style::default().fg(t.muted).bg(t.bg),
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
            Style::default().fg(ACCENT).bg(t.bg),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(rule_widget, chunks[3]);
    }

    let skip = Paragraph::new(Line::from(Span::styled(
        "any key skip",
        Style::default().fg(t.dim).bg(t.bg),
    )))
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(skip, chunks[6]);
}

fn draw_app_frame(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    terminal.draw(|frame| match app.screen {
        Screen::Picker => {
            draw(frame, app);
            if app.help_open {
                draw_help_overlay(frame, app);
            }
        }
        Screen::Settings => draw_settings(frame, app),
        Screen::FolderPicker => draw_folder_picker(frame, app),
        // Popup over the picker — not a separate full-screen UI.
        Screen::NewProject => {
            draw(frame, app);
            let status = app.status.as_deref();
            app.panel_area =
                new_project_ui::draw(frame, &app.new_project, app.theme(), status);
        }
    })?;
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<Option<Launch>> {
    // Re-discover after returning from an agent (App lives across run_app entries).
    // Async git fan-out — paint is not blocked. Keep selection when the path still exists.
    if let Some(path) = app.selected_repo().map(|r| r.path.clone()) {
        app.rebuild_repos_preserving_selection(&path);
    } else {
        app.refresh_repos();
    }
    app.help_open = false;

    // Initial paint before first input (async git badges fill in on later frames).
    draw_app_frame(terminal, app)?;

    loop {
        // Dead PTY / revoked stdin (broker restart, tab closed with retained session
        // then orphaned): exit cleanly instead of busy-looping at 80%+ CPU forever.
        if !io::stdin().is_terminal() {
            return Ok(None);
        }

        // Only push a redraw over the PTY when something visible changed.
        // Idle unconditional paints waste bandwidth on the browser path.
        let mut needs_draw = false;
        let status_before = (app.status.is_some(), app.status_reveal);
        let tips_before = (app.tips.idx, app.tips.reveal);
        let git_before = app.git_pending;
        let jobs_before = app.jobs.any_active();

        app.tick_status();
        app.tick_tips();
        for notice in app.jobs.poll() {
            let jobs::JobNotice::Status(s) = notice;
            app.set_status(s);
            needs_draw = true;
        }
        app.poll_git_meta();
        // Any completed inspect changes pending; redraw once for new badges.
        if app.git_pending != git_before {
            needs_draw = true;
        }
        app.tick_hover_install();

        if (app.status.is_some(), app.status_reveal) != status_before
            || (app.tips.idx, app.tips.reveal) != tips_before
        {
            needs_draw = true;
        }
        // Spinner frames while a job runs; one more frame when the bar clears.
        if app.jobs.any_active() || jobs_before {
            needs_draw = true;
        }

        // Faster poll only while something is animating (reveal / job spinner).
        // Waiting on git metadata does not need 40 ms frames — updates redraw once.
        let poll_ms = if app.status_reveal > 0
            || app.tips.reveal > 0
            || app.jobs.any_active()
            || app.status.is_some()
        {
            40
        } else {
            250
        };

        // P1: drain all pending input, then draw once (paste / mouse motion).
        // Poll-before-read (after the first event) so `continue` in match arms is safe
        // and does not re-block on `event::read` skipping the draw.
        let has_event = match event::poll(Duration::from_millis(poll_ms)) {
            Ok(v) => v,
            // Broken pipe / revoked TTY after broker death — do not spin.
            Err(_) => return Ok(None),
        };
        if has_event {
            let mut first = true;
            loop {
                if !first {
                    match event::poll(Duration::ZERO) {
                        Ok(false) => break,
                        Err(_) => return Ok(None),
                        Ok(true) => {}
                    }
                }
                first = false;
                let ev = match event::read() {
                    Ok(e) => e,
                    Err(_) => return Ok(None),
                };
                match ev {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                needs_draw = true;
                // Help overlay intercepts keys on the picker.
                if app.help_open && app.screen == Screen::Picker {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('?') => {
                            app.help_open = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.screen == Screen::FolderPicker {
                    let action = folder::handle_key(&mut app.folder, key);
                    match action {
                        folder::FolderAction::None => {}
                        folder::FolderAction::Cancel => {
                            app.screen = match app.folder_purpose {
                                folder::FolderPickerPurpose::WorkspaceRoot => Screen::Settings,
                                folder::FolderPickerPurpose::NewProjectParent => Screen::NewProject,
                            };
                            app.clear_status();
                        }
                        folder::FolderAction::ConfirmSelected => {
                            let path = app.folder.chosen_path();
                            app.confirm_folder_selection(path);
                        }
                        folder::FolderAction::ConfirmCurrent => {
                            let path = app.folder.current_path().to_path_buf();
                            app.confirm_folder_selection(path);
                        }
                        folder::FolderAction::Status(s) => app.set_status(s),
                    }
                    continue;
                }

                if app.screen == Screen::Settings {
                    let (sel, action) = settings_ui::handle_key(key, app.settings_selected);
                    app.settings_selected = sel;
                    match action {
                        settings_ui::SettingsAction::None => {}
                        settings_ui::SettingsAction::Back => {
                            app.screen = Screen::Picker;
                            app.clear_status();
                        }
                        settings_ui::SettingsAction::Nudge(d) => app.nudge_settings_item(d),
                        settings_ui::SettingsAction::Activate => app.nudge_settings_item(1),
                    }
                    continue;
                }

                if app.screen == Screen::NewProject {
                    let action = new_project_input::handle_key(&mut app.new_project, key);
                    app.apply_np_action(action);
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
                        app.paint_liftoff(&launch);
                        draw_app_frame(terminal, app)?;
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
                        app.paint_liftoff(&launch);
                        draw_app_frame(terminal, app)?;
                        return Ok(Some(launch));
                    }
                    app.set_status("no last session — open something with enter first");
                }
                KeyCode::Char('?') if app.filter.is_empty() => {
                    app.help_open = true;
                }
                KeyCode::Char('s') | KeyCode::Char('S') if app.filter.is_empty() => {
                    app.help_open = false;
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
            Event::Mouse(mouse) if app.screen == Screen::NewProject => {
                let action = new_project_input::handle_mouse(
                    &mut app.new_project,
                    mouse,
                    app.panel_area,
                );
                app.apply_np_action(action);
                needs_draw = true;
            }
            Event::Mouse(mouse) if app.screen == Screen::FolderPicker => {
                let list = Rect {
                    x: app.panel_area.x,
                    y: app.hitboxes.list_top,
                    width: app.panel_area.width,
                    height: app.hitboxes.list_height,
                };
                let _ = folder::handle_mouse(&mut app.folder, mouse, list);
                needs_draw = true;
            }
            Event::Mouse(mouse) if app.screen == Screen::Settings => {
                let panel = app.panel_area;
                let inner = inset(panel, PANEL_PAD_H, PANEL_PAD_V);
                let lay = settings_ui::layout(inner);
                let (sel, action) =
                    settings_ui::handle_mouse(mouse, app.settings_selected, &lay);
                app.settings_selected = sel;
                match action {
                    settings_ui::SettingsAction::Activate => app.nudge_settings_item(1),
                    settings_ui::SettingsAction::Nudge(d) => app.nudge_settings_item(d),
                    _ => {}
                }
                needs_draw = true;
            }
            Event::Mouse(mouse) if app.screen == Screen::Picker => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    app.select_next_repo();
                    needs_draw = true;
                }
                MouseEventKind::ScrollUp => {
                    app.select_previous_repo();
                    needs_draw = true;
                }
                MouseEventKind::Moved => {
                    let hovered = app.action_at_mouse(mouse.column, mouse.row);
                    let before = app.hover_missing.map(|(a, _)| a);
                    app.on_hover_action(hovered);
                    let after = app.hover_missing.map(|(a, _)| a);
                    // Mouse tracking floods moves; only repaint when hover target changes.
                    if before != after {
                        needs_draw = true;
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    needs_draw = true;
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
                        let row = usize::from(mouse.row - app.hitboxes.list_top);
                        let clicked = app
                            .hitboxes
                            .list_rows
                            .get(row)
                            .copied()
                            .flatten();
                        if let Some(clicked_visible) = clicked {
                            if clicked_visible == app.selected_visible {
                                if app.selected_action != Action::Shell
                                    && !app.selected_action.is_available()
                                {
                                    app.start_install(app.selected_action);
                                } else if let Some(launch) = app.selected_launch() {
                                    app.prepare_launch(&launch);
                                    app.paint_liftoff(&launch);
                                    draw_app_frame(terminal, app)?;
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
                needs_draw = true;
            }
            Event::Paste(text) => {
                // Bracketed paste (xterm.js / modern terminals). Without this,
                // paste is ignored while the event is still consumed.
                match app.screen {
                    Screen::Picker if !app.help_open => {
                        app.push_filter_paste(&text);
                        needs_draw = true;
                    }
                    Screen::NewProject => {
                        new_project_input::handle_paste(&mut app.new_project, &text);
                        needs_draw = true;
                    }
                    _ => {}
                }
            }
            _ => {}
                }
            }
        }

        if needs_draw {
            draw_app_frame(terminal, app)?;
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

    let area = picker_panel_rect(frame.area());
    app.panel_area = area;
    let block = Block::default()
        .title(format!(" {APP_NAME} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, area);

    let inner = inset(area, PANEL_PAD_H, PANEL_PAD_V);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // chips
            Constraint::Length(1), // filter
            Constraint::Min(2),    // list
            Constraint::Length(1), // tip / status (live, colored)
            Constraint::Length(1), // keys (always visible)
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

    // Live row above keys: status flash wins, else animated tip (New Project keeps flashes in modal).
    let tip_w = chunks[4].width as usize;
    let tip_line = if app.screen == Screen::NewProject {
        Line::from(Span::styled(
            pad_line("", tip_w),
            Style::default().fg(t.dim),
        ))
    } else if let Some(status) = &app.status {
        Line::from(Span::styled(
            pad_line(status, tip_w),
            app.status_style(t),
        ))
    } else {
        let tip = TIPS[app.tips.idx % TIPS.len()];
        let body = app.tip_visible_text(tip);
        let spark = app.tip_sparkle();
        let styled = app.tip_style(t);
        let raw = format!("{spark} {body}");
        // Soft caret while typewriting.
        let with_caret = if app.tips.reveal > 0 {
            format!("{raw}▌")
        } else {
            raw
        };
        Line::from(Span::styled(pad_line(&with_caret, tip_w), styled))
    };
    frame.render_widget(Paragraph::new(tip_line), chunks[4]);

    // Stable keymap — always the same line; keys in key color, verbs dim.
    let keys = Line::from(vec![
        Span::styled("enter", Style::default().fg(t.key)),
        Span::styled(" open  ", Style::default().fg(t.dim)),
        Span::styled(".", Style::default().fg(t.key)),
        Span::styled(" resume  ", Style::default().fg(t.dim)),
        Span::styled("space", Style::default().fg(t.key)),
        Span::styled(" ★  ", Style::default().fg(t.dim)),
        Span::styled("1-9", Style::default().fg(t.key)),
        Span::styled(" agent  ", Style::default().fg(t.dim)),
        Span::styled("n", Style::default().fg(t.key)),
        Span::styled(" new  ", Style::default().fg(t.dim)),
        Span::styled("s", Style::default().fg(t.key)),
        Span::styled(" settings  ", Style::default().fg(t.dim)),
        Span::styled("?", Style::default().fg(t.key)),
        Span::styled(" help", Style::default().fg(t.dim)),
    ]);
    frame.render_widget(Paragraph::new(keys), chunks[5]);

    jobs::draw_bars(frame, &app.jobs, app.panel_area, t);
}

/// Shared outer rect for picker, settings, and folder browser — fixed silhouette.
/// List height does **not** grow with favorites/repos; extra rows scroll inside.
fn picker_panel_rect(screen: Rect) -> Rect {
    panel_rect(screen, PANEL_CHROME.saturating_add(MIN_LIST_ROWS))
}

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
    let area = picker_panel_rect(frame.area());
    app.panel_area = area;
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
    let _lay = settings_ui::draw(
        frame,
        area,
        t,
        &rows,
        app.settings_selected,
        app.status.as_deref(),
    );
}

fn draw_folder_picker(frame: &mut Frame<'_>, app: &mut App) {
    let t = app.theme();
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        frame.area(),
    );
    let area = picker_panel_rect(frame.area());
    app.panel_area = area;
    let (list_top, list_h) = folder::draw(
        frame,
        &mut app.folder,
        app.folder_purpose,
        area,
        t,
        app.status.as_deref(),
    );
    app.hitboxes.list_top = list_top;
    app.hitboxes.list_height = list_h;
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
    let w = area.width as usize;
    if app.filter.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                pad_line("/ type to filter", w),
                Style::default().fg(t.dim),
            ))),
            area,
        );
        return;
    }

    let count = format!("{}/{}", app.visible_repos.len(), app.repos.len());
    let count_w = display_width(&count);
    // "/" + query + "▌" + gap + count
    let caret = '▌';
    let prefix = format!("/{}{}", app.filter, caret);
    let avail = w.saturating_sub(count_w.saturating_add(1));
    let left = if display_width(&prefix) <= avail {
        pad_line(&prefix, avail)
    } else {
        // Keep end of query + caret visible.
        sliding_tail(&prefix, avail)
    };
    let gap = " ".repeat(w.saturating_sub(display_width(&left).saturating_add(count_w)).max(0));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            // Match empty-state dim so typing does not flash bright text.
            Span::styled(left, Style::default().fg(t.dim)),
            Span::raw(gap),
            Span::styled(count, Style::default().fg(t.dim)),
        ])),
        area,
    );
}

/// Column widths for one list frame (two-space gutters).
struct ColWidths {
    name: usize,
    branch: usize,
    agent: usize,
    path: usize,
}

fn compute_columns(area_width: u16) -> ColWidths {
    // sel(2) + name + 2 + branch(14) + 2 + agent(7) + 2 + path
    const FIXED: usize = 2 + 2 + 14 + 2 + 7 + 2;
    let w = area_width as usize;
    let name = 18usize;
    let path = w.saturating_sub(FIXED + name).max(8);
    ColWidths {
        name,
        branch: 14,
        agent: 7,
        path,
    }
}

fn section_label(badge: &str, root_display: &str) -> String {
    match badge {
        "★" => "─ ★ favorites".into(),
        "recent" => "─ recent".into(),
        "last" => "─ last".into(),
        "root" => "─ root".into(),
        _ => format!("─ {root_display}"),
    }
}

fn draw_repos(frame: &mut Frame<'_>, app: &mut App, area: Rect, t: Theme) {
    app.hitboxes.list_top = area.y;
    app.hitboxes.list_height = area.height;
    app.hitboxes.list_rows.clear();
    app.keep_selected_visible();

    let visible_rows = area.height as usize;
    let cols = compute_columns(area.width);
    // Demo repos live under ~/work; use that for the scan-section label so it
    // matches the path column (real mode still uses configured workspace root).
    let root_disp = if discover::demo_mode_enabled() {
        display_path(&discover::demo_root())
    } else {
        display_path(&workspace_root(&app.state.settings))
    };
    let filtering = !app.filter.is_empty();
    let mut lines: Vec<Line> = Vec::new();

    // Empty states.
    if app.visible_repos.is_empty() {
        let (l1, l2) = if filtering {
            (
                "no matches".to_string(),
                "esc to clear".to_string(),
            )
        } else {
            (
                format!("no git repos under {root_disp}"),
                "n creates one · s changes root".to_string(),
            )
        };
        let pad = visible_rows.saturating_sub(2) / 2;
        for _ in 0..pad {
            lines.push(Line::from(""));
            app.hitboxes.list_rows.push(None);
        }
        lines.push(Line::from(Span::styled(
            pad_line(&l1, area.width as usize),
            Style::default().fg(t.dim),
        )));
        app.hitboxes.list_rows.push(None);
        lines.push(Line::from(Span::styled(
            pad_line(&l2, area.width as usize),
            Style::default().fg(t.dim),
        )));
        app.hitboxes.list_rows.push(None);
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let show_up = app.offset > 0;
    let mut repo_i = app.offset;
    let mut prev_badge: Option<&str> = if repo_i > 0 {
        Some(app.repos[app.visible_repos[repo_i - 1]].badge)
    } else {
        None
    };

    while lines.len() < visible_rows && repo_i < app.visible_repos.len() {
        let vis_idx = repo_i;
        let repo = &app.repos[app.visible_repos[vis_idx]];

        // Section separator when group changes (unfiltered only).
        if !filtering && prev_badge != Some(repo.badge) {
            if lines.len() >= visible_rows {
                break;
            }
            let label = section_label(repo.badge, &root_disp);
            lines.push(Line::from(Span::styled(
                pad_line(&label, area.width as usize),
                Style::default().fg(t.dim),
            )));
            app.hitboxes.list_rows.push(None);
            if lines.len() >= visible_rows {
                break;
            }
        }

        let selected = vis_idx == app.selected_visible;
        let row_bg = if selected { t.surface } else { t.bg };
        let base = if selected {
            Style::default()
                .fg(t.text)
                .bg(row_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.soft).bg(row_bg)
        };
        let muted = Style::default().fg(t.dim).bg(row_bg);
        let warn = Style::default().fg(t.warn).bg(row_bg);

        let mut spans: Vec<Span> = Vec::new();
        // Selection bar ▌ or space.
        if selected {
            spans.push(Span::styled("▌", Style::default().fg(ACCENT).bg(row_bg)));
        } else {
            spans.push(Span::styled(" ", muted));
        }
        spans.push(Span::styled(" ", base));

        // ★ prefix for favorites.
        let name_budget = cols.name;
        let star = if repo.badge == "★" { "★ " } else { "" };
        let name_style = base;
        let name_spans = name_match_spans(
            &format!("{star}{}", repo.name),
            &app.filter,
            name_style,
            name_budget,
            selected,
            t,
            row_bg,
        );
        spans.extend(name_spans);
        spans.push(Span::styled("  ", base));

        // Branch + dirty/ahead (warn only on * / ↑N).
        if let Some(branch) = &repo.git_branch {
            let mut suffix = String::new();
            if repo.git_dirty {
                suffix.push('*');
            }
            if repo.git_ahead > 0 {
                suffix.push_str(&format!("↑{}", repo.git_ahead));
            }
            let core_w = cols.branch.saturating_sub(display_width(&suffix));
            let core = ellipsize_end(branch, core_w);
            let core_trim = core.trim_end();
            if suffix.is_empty() {
                spans.push(Span::styled(pad_line(core_trim, cols.branch), muted));
            } else {
                spans.push(Span::styled(pad_line(core_trim, core_w), muted));
                spans.push(Span::styled(suffix, warn));
            }
        } else {
            spans.push(Span::styled(pad_line("", cols.branch), muted));
        }

        spans.push(Span::styled("  ", base));

        // Remembered agent — dim / text, never accent.
        let agent_style = if selected {
            Style::default().fg(t.text).bg(row_bg)
        } else {
            Style::default().fg(t.dim).bg(row_bg)
        };
        if let Some(action) = repo.remembered_agent {
            spans.push(Span::styled(
                pad_line(action.label(), cols.agent),
                agent_style,
            ));
        } else {
            spans.push(Span::styled(pad_line("", cols.agent), agent_style));
        }

        spans.push(Span::styled("  ", base));

        // Path: root prefix dimmer than leaf.
        let full = display_path(&repo.path);
        let path_spans = path_column_spans(&full, &root_disp, cols.path, row_bg, t, selected);
        spans.extend(path_spans);

        // Pad remainder so surface bleeds full width.
        let used: usize = spans.iter().map(|s| display_width(s.content.as_ref())).sum();
        if used < area.width as usize {
            spans.push(Span::styled(
                " ".repeat(area.width as usize - used),
                Style::default().bg(row_bg),
            ));
        }

        lines.push(Line::from(spans));
        app.hitboxes.list_rows.push(Some(vis_idx));
        prev_badge = Some(repo.badge);
        repo_i += 1;
    }

    // Pad remaining rows.
    while lines.len() < visible_rows {
        lines.push(Line::from(Span::styled(
            " ".repeat(area.width as usize),
            Style::default().bg(t.bg),
        )));
        app.hitboxes.list_rows.push(None);
    }

    frame.render_widget(Paragraph::new(lines), area);

    // Scroll affordance ▲/▼ in corner cells.
    let more_below = repo_i < app.visible_repos.len();
    if show_up && area.width > 0 && area.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled("▲", Style::default().fg(t.dim).bg(t.bg))),
            Rect {
                x: area.x + area.width - 1,
                y: area.y,
                width: 1,
                height: 1,
            },
        );
    }
    if more_below && area.width > 0 && area.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled("▼", Style::default().fg(t.dim).bg(t.bg))),
            Rect {
                x: area.x + area.width - 1,
                y: area.y + area.height - 1,
                width: 1,
                height: 1,
            },
        );
    }
}

/// Bold matched substrings in the name (filter tokens).
fn name_match_spans(
    name: &str,
    filter: &str,
    base: Style,
    width: usize,
    selected: bool,
    t: Theme,
    row_bg: Color,
) -> Vec<Span<'static>> {
    let display = ellipsize_end(name, width);
    let raw = display.trim_end();
    let pad = width.saturating_sub(display_width(raw));
    let query = filter.trim().to_lowercase();
    if query.is_empty() {
        return vec![Span::styled(pad_line(name, width), base)];
    }

    let lower = raw.to_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for part in query.split_whitespace() {
        if part.is_empty() {
            continue;
        }
        if let Some(pos) = lower.find(part) {
            best = Some((pos, pos + part.len()));
            break;
        }
    }

    let mut spans = Vec::new();
    if let Some((start, end)) = best {
        // Byte indices on lowercased ASCII-compatible name (filters are lowercase ASCII).
        let before = raw.get(..start).unwrap_or("").to_string();
        let mid = raw.get(start..end).unwrap_or("").to_string();
        let after = raw.get(end..).unwrap_or("").to_string();
        let bold = Style::default()
            .fg(if selected { t.text } else { t.soft })
            .bg(row_bg)
            .add_modifier(Modifier::BOLD);
        if !before.is_empty() {
            spans.push(Span::styled(before, base));
        }
        if !mid.is_empty() {
            spans.push(Span::styled(mid, bold));
        }
        if !after.is_empty() {
            spans.push(Span::styled(after, base));
        }
        if pad > 0 {
            spans.push(Span::styled(" ".repeat(pad), base));
        }
    } else {
        spans.push(Span::styled(pad_line(raw, width), base));
    }
    spans
}

fn path_column_spans(
    full: &str,
    root_disp: &str,
    width: usize,
    row_bg: Color,
    t: Theme,
    _selected: bool,
) -> Vec<Span<'static>> {
    let dim = Style::default().fg(t.dim).bg(row_bg);
    let leaf_st = Style::default().fg(t.muted).bg(row_bg);

    let prefix = format!("{root_disp}/");
    if full.starts_with(&prefix) {
        let truncated = ellipsize_front(full, width);
        let tr = truncated.trim_end();
        if let Some(rest) = tr.strip_prefix(&prefix) {
            let p = pad_line(&prefix, display_width(&prefix).min(width));
            let r_w = width.saturating_sub(display_width(&p));
            return vec![
                Span::styled(p, dim),
                Span::styled(pad_line(rest, r_w), leaf_st),
            ];
        }
        return vec![Span::styled(pad_line(tr, width), dim)];
    }

    vec![Span::styled(ellipsize_front(full, width), dim)]
}

/// Content-aware centered panel (command-palette style).
fn panel_rect(screen: Rect, content_rows: u16) -> Rect {
    let width = screen.width.min(MAX_WIDTH).max(40);
    // Leave a little margin, but allow taller panels on large terminals.
    let max_h = screen.height.saturating_sub(2).max(MIN_PANEL_HEIGHT);
    let height = content_rows
        .saturating_add(2) // borders
        .max(MIN_PANEL_HEIGHT)
        .min(max_h)
        .min(screen.height.max(1));
    Rect {
        x: screen.x + screen.width.saturating_sub(width) / 2,
        y: screen.y + screen.height.saturating_sub(height) / 2,
        width: width.min(screen.width.max(1)),
        height,
    }
}

/// Inner content pad past the border (one cell more than bare border → airy labels).
pub(crate) const PANEL_PAD_H: u16 = 3;
pub(crate) const PANEL_PAD_V: u16 = 1;

pub(crate) fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect {
        x: area.x + horizontal,
        y: area.y + vertical,
        width: area.width.saturating_sub(horizontal * 2),
        height: area.height.saturating_sub(vertical * 2),
    }
}

/// FS-only workspace list (no git). Instant; safe for first paint.



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

pub(crate) fn read_last_cwd(data: &Path) -> Option<PathBuf> {
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

pub(crate) fn read_recent_workspaces(data: &Path) -> Vec<PathBuf> {
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

pub(crate) fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Canonical data dir: `~/.t-0`.
/// One-shot migrates `~/.mission-control` or `~/.grok-mission-control` when modern is missing.
/// `MC_DATA_DIR` overrides, except when it still points at a legacy path after migrate.
pub(crate) fn data_dir() -> PathBuf {
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
pub(crate) fn workspace_root(settings: &SettingsFile) -> PathBuf {
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

pub(crate) fn display_path(path: &Path) -> String {
    let home = home_dir();
    if let Ok(stripped) = path.strip_prefix(&home) {
        return format!("~/{}", stripped.display());
    }
    path.display().to_string()
}

pub(crate) fn pad_or_trim(value: &str, width: usize) -> String {
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

    // Headless project init runs in-background inside the TUI (see start_background_init).

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

pub(crate) fn init_agent_elevated(action: Action) -> bool {
    action_to_init_kind(action)
        .map(|k| k.elevated_autonomy())
        .unwrap_or(false)
}

/// `?` keymap overlay — reuses New Project popup chrome (Clear + opaque + border).
fn draw_help_overlay(frame: &mut Frame<'_>, _app: &App) {
    let t = _app.theme();
    let lines: &[&str] = &[
        "enter        open selected workspace with agent",
        ".            resume last session",
        "space        toggle favorite ★",
        "1-9          pick agent chip",
        "n            new project",
        "s            settings",
        "type         filter by name or path · esc clears",
        "e            open in editor (Default IDE)",
        "f            reveal in Finder",
        "c            copy workspace path",
        "g            open GitHub (if remote)",
        "hover        dim agent chip → install missing CLI",
        "theme        Settings → UI theme (auto / light / dark)",
        "esc / ?      close this help",
    ];
    let content = lines.len() as u16;
    let width = frame.area().width.min(72).max(40);
    // +2 = top/bottom borders. PANEL_PAD_V sits content between them.
    let height = (content + 2)
        .min(frame.area().height.saturating_sub(2))
        .max(content.saturating_add(2));
    let area = Rect {
        x: frame.area().x + frame.area().width.saturating_sub(width) / 2,
        y: frame.area().y + frame.area().height.saturating_sub(height) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.text)),
        area,
    );
    let block = Block::default()
        .title(format!(" {APP_NAME} · Keys "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(t.bg).fg(t.text));
    frame.render_widget(block, area);

    let inner = inset(area, PANEL_PAD_H, PANEL_PAD_V);
    let mut out = Vec::new();
    for line in lines {
        if line.is_empty() {
            out.push(Line::from(""));
            continue;
        }
        let (key, rest) = if let Some(idx) = line.find(char::is_whitespace) {
            (line[..idx].to_string(), line[idx..].trim_start().to_string())
        } else {
            (line.to_string(), String::new())
        };
        out.push(Line::from(vec![
            Span::styled(pad_line(&key, 12), Style::default().fg(t.key).bg(t.bg)),
            Span::styled(rest, Style::default().fg(t.dim).bg(t.bg)),
        ]));
    }
    frame.render_widget(Paragraph::new(out), inner);
}

