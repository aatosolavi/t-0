use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    io::{self, stdout},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const MAX_WIDTH: u16 = 92;
const MAX_RECENTS: usize = 20;
const MAX_FAVORITES: usize = 20;
/// Mission Control accent — orange-500 (#f97316).
const ACCENT: Color = Color::Rgb(249, 115, 22);
/// Text on filled accent chips (dark enough for contrast on orange).
const ACCENT_ON: Color = Color::Rgb(23, 23, 23);

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
            Action::Claude => Some("claude"),
            Action::Amp => Some("amp"),
            Action::Devin => Some("devin"),
            Action::Droid => Some("droid"),
        }
    }

    fn resolve_command(self) -> Option<String> {
        let key = self.env_command_key()?;
        let default = self.default_command()?;
        Some(env::var(key).unwrap_or_else(|_| default.to_string()))
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
struct LauncherStateFile {
    version: u32,
    #[serde(default)]
    last: Option<LastLaunch>,
    #[serde(default)]
    favorites: Vec<String>,
    #[serde(default)]
    agents: HashMap<String, String>,
}

impl Default for LauncherStateFile {
    fn default() -> Self {
        Self {
            version: 1,
            last: None,
            favorites: Vec::new(),
            agents: HashMap::new(),
        }
    }
}

#[derive(Clone, Default)]
struct LauncherState {
    last: Option<(PathBuf, Action)>,
    favorites: Vec<PathBuf>,
    agents: HashMap<PathBuf, Action>,
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
        };

        if let Ok(json) = serde_json::to_string_pretty(&file) {
            let _ = fs::write(launcher_state_path(), format!("{json}\n"));
        }
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
}

impl App {
    fn new() -> Self {
        let state = LauncherState::load();
        let repos = discover_repos(&state);
        let visible_repos = (0..repos.len()).collect();
        let mut app = Self {
            state,
            repos,
            visible_repos,
            selected_visible: 0,
            selected_action: Action::first_available(),
            filter: String::new(),
            offset: 0,
            hitboxes: UiHitboxes::default(),
            status: None,
        };
        app.apply_agent_memory();
        app
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    fn clear_status(&mut self) {
        self.status = None;
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
        if let Some(action) = self.state.agent_for(&repo.path) {
            self.selected_action = action.resolve_available();
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
            SideAction::Editor => match open_in_editor(&repo.path) {
                Ok(label) => self.set_status(format!("opened in {label}")),
                Err(err) => self.set_status(err),
            },
            SideAction::Finder => match open_in_finder(&repo.path) {
                Ok(()) => self.set_status("opened in Finder"),
                Err(err) => self.set_status(err),
            },
            SideAction::CopyPath => match copy_path_to_clipboard(&repo.path) {
                Ok(()) => self.set_status(format!("copied {}", display_path(&repo.path))),
                Err(err) => self.set_status(err),
            },
            SideAction::GitHub => match open_github(&repo.path) {
                Ok(url) => self.set_status(format!("opened {url}")),
                Err(err) => self.set_status(err),
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

/// Splash runs unless MC_SPLASH is 0 / off / false.
fn splash_enabled() -> bool {
    match env::var("MC_SPLASH") {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            !(v == "0" || v == "off" || v == "false" || v == "no")
        }
        Err(_) => true,
    }
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
        .title(" Mission Control ")
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
            "Mission Control",
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(wordmark, chunks[1]);
    }

    if elapsed >= SPLASH_TAGLINE_MS {
        let tagline = Paragraph::new(Line::from(Span::styled(
            "finder for agents",
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
        terminal.draw(|frame| draw(frame, &mut app))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => {
                    if app.filter.is_empty() {
                        return Ok(None);
                    }
                    app.clear_filter();
                }
                KeyCode::Enter => {
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
                KeyCode::Char(value @ '1'..='8') => {
                    app.select_action_by_number(value.to_digit(10).unwrap_or(0) as u8);
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
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => app.select_next_repo(),
                MouseEventKind::ScrollUp => app.select_previous_repo(),
                MouseEventKind::Down(MouseButton::Left) => {
                    for (start, end, action) in &app.hitboxes.actions {
                        if mouse.row == app.hitboxes.action_row
                            && mouse.column >= *start
                            && mouse.column <= *end
                        {
                            app.selected_action = *action;
                            break;
                        }
                    }

                    let list_bottom = app.hitboxes.list_top + app.hitboxes.list_height;
                    if mouse.row >= app.hitboxes.list_top && mouse.row < list_bottom {
                        let clicked_visible =
                            app.offset + usize::from(mouse.row - app.hitboxes.list_top);
                        if clicked_visible < app.visible_repos.len() {
                            if clicked_visible == app.selected_visible {
                                if let Some(launch) = app.selected_launch() {
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
            Event::Resize(_, _) => app.keep_selected_visible(),
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = centered_rect(frame.area());
    let block = Block::default()
        .title(" Mission Control ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
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
        Span::styled("Open ", Style::default().fg(Color::Gray)),
        Span::styled(
            app.selected_action.label(),
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" in a workspace", Style::default().fg(Color::Gray)),
    ]);
    frame.render_widget(Paragraph::new(title), chunks[0]);

    draw_actions(frame, app, chunks[1]);
    draw_filter(frame, app, chunks[2]);
    draw_repos(frame, app, chunks[3]);

    let footer_second = if let Some(status) = &app.status {
        Line::from(Span::styled(status.clone(), Style::default().fg(ACCENT)))
    } else {
        Line::from(Span::styled(
            "e editor · f Finder · c copy · g GitHub · dim app = missing CLI",
            Style::default().fg(Color::DarkGray),
        ))
    };
    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("enter", Style::default().fg(Color::White)),
            Span::styled(" open  ", Style::default().fg(Color::DarkGray)),
            Span::styled(".", Style::default().fg(Color::White)),
            Span::styled(" cont  ", Style::default().fg(Color::DarkGray)),
            Span::styled("space", Style::default().fg(Color::White)),
            Span::styled(" fav  ", Style::default().fg(Color::DarkGray)),
            Span::styled("1-8", Style::default().fg(Color::White)),
            Span::styled(" app  ", Style::default().fg(Color::DarkGray)),
            Span::styled("type", Style::default().fg(Color::White)),
            Span::styled(" filter", Style::default().fg(Color::DarkGray)),
        ]),
        footer_second,
    ]);
    frame.render_widget(footer, chunks[4]);
}

fn draw_actions(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    app.hitboxes.action_row = area.y;
    app.hitboxes.actions.clear();

    for (index, action) in Action::all().iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
            x += 1;
        }

        // Number prefix doubles as the 1–8 keyboard shortcut.
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
                    .fg(Color::Black)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            }
        } else if available {
            Style::default().fg(Color::Gray)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        app.hitboxes
            .actions
            .push((x, x.saturating_add(width.saturating_sub(1)), *action));
        spans.push(Span::styled(label, style));
        x += width;
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_filter(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let value = if app.filter.is_empty() {
        "type to filter · . continue · space fav".to_string()
    } else {
        app.filter.clone()
    };
    let style = if app.filter.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(ACCENT)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled(value, style),
        ])),
        area,
    );
}

fn draw_repos(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let badge_style = if repo.badge == "★" {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(Color::DarkGray)
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
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
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
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(repo.badge, badge_style));

        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No workspaces match this filter",
            Style::default().fg(Color::DarkGray),
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
    let home = home_dir();
    let data = data_dir();
    let root = workspace_root();
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

fn open_in_editor(path: &Path) -> Result<String, String> {
    let path_str = path.display().to_string();
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    // Prefer explicit env (supports multi-word like `cursor -g`). GUI CLIs next.
    for key in ["MC_EDITOR", "VISUAL", "EDITOR"] {
        if let Ok(cmd) = env::var(key) {
            let cmd = cmd.trim();
            if cmd.is_empty() {
                continue;
            }
            // Quote path for the shell; leave the command as the user wrote it.
            let script = format!("{cmd} '{path_str}'");
            if Command::new(&shell)
                .args(["-lc", &script])
                .spawn()
                .is_ok()
            {
                return Ok(cmd.to_string());
            }
        }
    }

    for bin in ["cursor", "code", "subl", "zed"] {
        if command_available(bin) && Command::new(bin).arg(&path_str).spawn().is_ok() {
            return Ok(bin.into());
        }
    }

    for app in ["Cursor", "Visual Studio Code", "Zed"] {
        let status = Command::new("open")
            .args(["-a", app, &path_str])
            .status();
        if matches!(status, Ok(s) if s.success()) {
            return Ok(app.into());
        }
    }

    Err("no editor found (set MC_EDITOR or install cursor/code)".into())
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

/// Prefer MC_DATA_DIR, then ~/.mission-control, then legacy ~/.grok-mission-control.
fn data_dir() -> PathBuf {
    if let Ok(value) = env::var("MC_DATA_DIR") {
        return expand_path(&value);
    }
    let home = home_dir();
    let modern = home.join(".mission-control");
    let legacy = home.join(".grok-mission-control");
    if modern.is_dir() {
        modern
    } else if legacy.is_dir() {
        legacy
    } else {
        modern
    }
}

/// Prefer MC_WORKSPACE_ROOT, then GROK_TERMINAL_START_CWD, then ~/dev if present, else $HOME.
fn workspace_root() -> PathBuf {
    if let Ok(value) = env::var("MC_WORKSPACE_ROOT") {
        return expand_path(&value);
    }
    if let Ok(value) = env::var("GROK_TERMINAL_START_CWD") {
        return expand_path(&value);
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
        eprintln!("Press enter to return to Mission Control...");
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
        eprintln!("Press enter to return to Mission Control...");
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
    }

    Ok(())
}
