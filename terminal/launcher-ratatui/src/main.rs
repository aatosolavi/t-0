use std::{
    collections::HashSet,
    env,
    fs,
    io::{self, stdout},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
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

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const DEFAULT_ROOT: &str = "dev";
const MAX_WIDTH: u16 = 92;
const MAX_RECENTS: usize = 20;
const RECENTS_FILE: &str = ".grok-mission-control/recent-workspaces.txt";

#[derive(Clone)]
struct Repo {
    name: String,
    path: PathBuf,
    badge: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Grok,
    Codex,
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
            Action::Claude,
            Action::Amp,
            Action::Devin,
            Action::Droid,
            Action::Shell,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Action::Grok => "Grok",
            Action::Codex => "Codex",
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

    fn index(self) -> usize {
        Action::all()
            .iter()
            .position(|action| *action == self)
            .unwrap_or(0)
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
    repos: Vec<Repo>,
    visible_repos: Vec<usize>,
    selected_visible: usize,
    selected_action: Action,
    filter: String,
    offset: usize,
    hitboxes: UiHitboxes,
}

impl App {
    fn new() -> Self {
        let repos = discover_repos();
        let visible_repos = (0..repos.len()).collect();
        Self {
            repos,
            visible_repos,
            selected_visible: 0,
            selected_action: Action::Grok,
            filter: String::new(),
            offset: 0,
            hitboxes: UiHitboxes::default(),
        }
    }

    fn selected_launch(&self) -> Option<Launch> {
        let repo_index = *self.visible_repos.get(self.selected_visible)?;
        Some(Launch {
            action: self.selected_action,
            cwd: self.repos[repo_index].path.clone(),
        })
    }

    fn select_next_repo(&mut self) {
        self.selected_visible =
            (self.selected_visible + 1).min(self.visible_repos.len().saturating_sub(1));
        self.keep_selected_visible();
    }

    fn select_previous_repo(&mut self) {
        self.selected_visible = self.selected_visible.saturating_sub(1);
        self.keep_selected_visible();
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
    }
}

fn main() -> io::Result<()> {
    let mut out = stdout();

    loop {
        enable_raw_mode()?;
        execute!(out, EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

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

        out = stdout();
    }
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
                        return Ok(Some(launch));
                    }
                }
                KeyCode::Backspace => app.pop_filter_char(),
                KeyCode::Down => app.select_next_repo(),
                KeyCode::Up => app.select_previous_repo(),
                KeyCode::Right | KeyCode::Tab => app.select_next_action(),
                KeyCode::Left | KeyCode::BackTab => app.select_previous_action(),
                KeyCode::Char(value @ '1'..='7') => {
                    app.select_action_by_number(value.to_digit(10).unwrap_or(0) as u8);
                }
                KeyCode::Char(value) => app.push_filter_char(value),
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
                        let clicked_visible = app.offset + usize::from(mouse.row - app.hitboxes.list_top);
                        if clicked_visible < app.visible_repos.len() {
                            if clicked_visible == app.selected_visible {
                                if let Some(launch) = app.selected_launch() {
                                    return Ok(Some(launch));
                                }
                            }
                            app.selected_visible = clicked_visible;
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
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" in a workspace", Style::default().fg(Color::Gray)),
    ]);
    frame.render_widget(Paragraph::new(title), chunks[0]);

    draw_actions(frame, app, chunks[1]);
    draw_filter(frame, app, chunks[2]);
    draw_repos(frame, app, chunks[3]);

    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("enter", Style::default().fg(Color::White)),
            Span::styled(" open  ", Style::default().fg(Color::DarkGray)),
            Span::styled("1-7/tab", Style::default().fg(Color::White)),
            Span::styled(" app  ", Style::default().fg(Color::DarkGray)),
            Span::styled("up/down", Style::default().fg(Color::White)),
            Span::styled(" workspace  ", Style::default().fg(Color::DarkGray)),
            Span::styled("click selected", Style::default().fg(Color::White)),
            Span::styled(" open  ", Style::default().fg(Color::DarkGray)),
            Span::styled("type", Style::default().fg(Color::White)),
            Span::styled(" filter", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(Span::styled(
            "backspace edits filter; esc clears filter or closes",
            Style::default().fg(Color::DarkGray),
        )),
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

        // Number prefix doubles as the 1–7 keyboard shortcut.
        let label = format!(" {} {} ", index + 1, action.label());
        let width = label.chars().count() as u16;
        let style = if *action == app.selected_action {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
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
        "type to filter".to_string()
    } else {
        app.filter.clone()
    };
    let style = if app.filter.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
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

        lines.push(Line::from(vec![
            Span::styled(format!("{marker} "), Style::default().fg(Color::Cyan)),
            Span::styled(pad_or_trim(&repo.name, 24), style),
            Span::styled("  ", Style::default()),
            Span::styled(
                pad_or_trim(&display_path(&repo.path), area.width.saturating_sub(40) as usize),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(repo.badge, Style::default().fg(Color::DarkGray)),
        ]));
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

fn discover_repos() -> Vec<Repo> {
    let home = home_dir();
    let root = home.join(DEFAULT_ROOT);
    let mut repos = Vec::new();
    let mut seen = HashSet::new();

    for recent in read_recent_workspaces(&home) {
        add_repo(&mut repos, &mut seen, recent, "recent");
    }

    if let Some(last_cwd) = read_last_cwd(&home) {
        add_repo(&mut repos, &mut seen, last_cwd, "last");
    }

    add_repo(&mut repos, &mut seen, root.clone(), "root");

    if let Ok(entries) = fs::read_dir(root) {
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
            add_repo(&mut repos, &mut seen, path, "");
        }
    }

    if repos.is_empty() {
        add_repo(&mut repos, &mut seen, home, "home");
    }

    repos
}

fn add_repo(repos: &mut Vec<Repo>, seen: &mut HashSet<PathBuf>, path: PathBuf, badge: &'static str) {
    if !path.is_dir() || !seen.insert(path.clone()) {
        return;
    }

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    repos.push(Repo { name, path, badge });
}

fn read_last_cwd(home: &Path) -> Option<PathBuf> {
    let state_path = home.join(".grok-mission-control/terminal-state.json");
    let text = fs::read_to_string(state_path).ok()?;
    let key = "\"lastCwd\"";
    let after_key = text.split(key).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let raw = after_colon.strip_prefix('"')?.split('"').next()?;
    let path = PathBuf::from(raw);
    path.is_dir().then_some(path)
}

fn recent_workspaces_path(home: &Path) -> PathBuf {
    home.join(RECENTS_FILE)
}

fn read_recent_workspaces(home: &Path) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    fs::read_to_string(recent_workspaces_path(home))
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

    let home = home_dir();
    let recents_path = recent_workspaces_path(&home);
    let mut recents = vec![path.to_path_buf()];
    let mut seen = HashSet::from([path.to_path_buf()]);

    for recent in read_recent_workspaces(&home) {
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
