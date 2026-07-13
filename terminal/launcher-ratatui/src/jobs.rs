//! Background jobs: CLI install + headless project init (one module, stacked bars).

use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::new_project::{display_width, env_flag_on, sliding_tail, InitCommand};
use crate::{Action, Theme, ACCENT};

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];

pub struct InstallUi {
    pub action: Action,
    pub fraction: f32,
    pub message: String,
    pub finished_at: Option<Instant>,
    pub failed: bool,
    pub started_at: Instant,
}

enum InstallEvent {
    Progress {
        action: Action,
        fraction: f32,
        message: String,
    },
    Done { action: Action },
    Failed { action: Action, error: String },
}

pub struct BgInitUi {
    pub agent: Action,
    pub project: String,
    pub message: String,
    pub finished_at: Option<Instant>,
    pub failed: bool,
    pub started_at: Instant,
}

enum BgInitEvent {
    Line(String),
    Done { ok: bool, summary: String },
}

/// Status flashes the job system wants the App footer to show.
#[derive(Debug, Clone)]
pub enum JobNotice {
    Status(String),
}

#[derive(Default)]
pub struct Jobs {
    pub install: Option<InstallUi>,
    install_rx: Option<Receiver<InstallEvent>>,
    pub bg_init: Option<BgInitUi>,
    bg_init_rx: Option<Receiver<BgInitEvent>>,
}

impl Jobs {
    pub fn any_active(&self) -> bool {
        self.install.is_some() || self.bg_init.is_some()
    }

    pub fn install_busy(&self) -> bool {
        matches!(&self.install, Some(InstallUi { finished_at: None, .. }))
    }

    pub fn bg_init_busy(&self) -> bool {
        matches!(&self.bg_init, Some(BgInitUi { finished_at: None, .. }))
    }

    pub fn start_install(
        &mut self,
        action: Action,
        recipe: Option<String>,
    ) -> Result<(), String> {
        if action == Action::Shell || action.is_available() {
            return Ok(());
        }
        if self.install_busy() {
            return Err("install already running".into());
        }
        let Some(cmdline) = recipe else {
            return Err(format!(
                "no install recipe for {} yet",
                action.label().to_ascii_lowercase()
            ));
        };

        let (tx, rx) = mpsc::channel();
        self.install_rx = Some(rx);
        self.install = Some(InstallUi {
            action,
            fraction: 0.02,
            message: format!("installing {}…", action.label().to_ascii_lowercase()),
            finished_at: None,
            failed: false,
            started_at: Instant::now(),
        });

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

            let mut lines = 0u32;
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().flatten() {
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
                for line in BufReader::new(err).lines().flatten() {
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
        Ok(())
    }

    pub fn start_bg_init(
        &mut self,
        action: Action,
        cmd: InitCommand,
        project: String,
    ) -> Result<Option<String>, String> {
        // Ok(Some(status)) for dry-run status; Ok(None) when spawned
        if self.bg_init_busy() {
            return Err("init already running".into());
        }
        if env_flag_on("MC_INIT_DRY_RUN") {
            return Ok(Some(format!(
                "created {project} · dry-run: {} {:?}",
                cmd.program, cmd.args
            )));
        }

        let (tx, rx) = mpsc::channel();
        self.bg_init_rx = Some(rx);
        self.bg_init = Some(BgInitUi {
            agent: action,
            project: project.clone(),
            message: format!("starting {}…", action.label().to_ascii_lowercase()),
            finished_at: None,
            failed: false,
            started_at: Instant::now(),
        });

        let label = action.label().to_string();
        thread::spawn(move || {
            let mut child = match Command::new(&cmd.program)
                .args(&cmd.args)
                .current_dir(&cmd.cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(BgInitEvent::Done {
                        ok: false,
                        summary: format!("failed to start {label}: {e}"),
                    });
                    return;
                }
            };

            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let tx_out = tx.clone();
            let tx_err = tx.clone();
            let out_h = thread::spawn(move || {
                if let Some(out) = stdout {
                    for line in BufReader::new(out).lines().flatten() {
                        if tx_out.send(BgInitEvent::Line(line)).is_err() {
                            break;
                        }
                    }
                }
            });
            let err_h = thread::spawn(move || {
                if let Some(err) = stderr {
                    for line in BufReader::new(err).lines().flatten() {
                        if tx_err.send(BgInitEvent::Line(line)).is_err() {
                            break;
                        }
                    }
                }
            });
            let _ = out_h.join();
            let _ = err_h.join();

            match child.wait() {
                Ok(status) if status.success() => {
                    let _ = tx.send(BgInitEvent::Done {
                        ok: true,
                        summary: format!("{label} init finished"),
                    });
                }
                Ok(status) => {
                    let _ = tx.send(BgInitEvent::Done {
                        ok: false,
                        summary: format!("{label} init exited {status}"),
                    });
                }
                Err(e) => {
                    let _ = tx.send(BgInitEvent::Done {
                        ok: false,
                        summary: format!("{label} init wait failed: {e}"),
                    });
                }
            }
        });
        Ok(None)
    }

    /// Poll both job channels. Returns notices for the status line.
    pub fn poll(&mut self) -> Vec<JobNotice> {
        let mut notices = Vec::new();
        notices.extend(self.poll_install());
        notices.extend(self.poll_bg_init());
        notices
    }

    fn poll_install(&mut self) -> Vec<JobNotice> {
        let Some(rx) = self.install_rx.as_ref() else {
            if let Some(ui) = &self.install {
                if let Some(done_at) = ui.finished_at {
                    if done_at.elapsed() >= Duration::from_millis(1600) {
                        self.install = None;
                    }
                }
            }
            return Vec::new();
        };

        loop {
            match rx.try_recv() {
                Ok(InstallEvent::Progress {
                    action,
                    fraction,
                    message,
                }) => {
                    let started_at = self
                        .install
                        .as_ref()
                        .map(|u| u.started_at)
                        .unwrap_or_else(Instant::now);
                    self.install = Some(InstallUi {
                        action,
                        fraction,
                        message,
                        finished_at: None,
                        failed: false,
                        started_at,
                    });
                }
                Ok(InstallEvent::Done { action }) => {
                    let started_at = self
                        .install
                        .as_ref()
                        .map(|u| u.started_at)
                        .unwrap_or_else(Instant::now);
                    self.install = Some(InstallUi {
                        action,
                        fraction: 1.0,
                        message: format!("{} ready", action.label().to_ascii_lowercase()),
                        finished_at: Some(Instant::now()),
                        failed: false,
                        started_at,
                    });
                    self.install_rx = None;
                    break;
                }
                Ok(InstallEvent::Failed { action, error }) => {
                    let started_at = self
                        .install
                        .as_ref()
                        .map(|u| u.started_at)
                        .unwrap_or_else(Instant::now);
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
                        started_at,
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
        Vec::new()
    }

    fn poll_bg_init(&mut self) -> Vec<JobNotice> {
        let mut notices = Vec::new();
        let Some(rx) = self.bg_init_rx.as_ref() else {
            if let Some(ui) = &self.bg_init {
                if let Some(done_at) = ui.finished_at {
                    if done_at.elapsed() >= Duration::from_secs(6) {
                        self.bg_init = None;
                    }
                }
            }
            return notices;
        };

        loop {
            match rx.try_recv() {
                Ok(BgInitEvent::Line(line)) => {
                    let msg = truncate_status_line(&line, 72);
                    if let Some(ui) = &mut self.bg_init {
                        ui.message = msg;
                    }
                }
                Ok(BgInitEvent::Done { ok, summary }) => {
                    let project = self
                        .bg_init
                        .as_ref()
                        .map(|u| u.project.clone())
                        .unwrap_or_default();
                    let agent = self
                        .bg_init
                        .as_ref()
                        .map(|u| u.agent)
                        .unwrap_or(Action::Shell);
                    let msg = truncate_status_line(&summary, 72);
                    let started_at = self
                        .bg_init
                        .as_ref()
                        .map(|u| u.started_at)
                        .unwrap_or_else(Instant::now);
                    self.bg_init = Some(BgInitUi {
                        agent,
                        project: project.clone(),
                        message: msg.clone(),
                        finished_at: Some(Instant::now()),
                        failed: !ok,
                        started_at,
                    });
                    self.bg_init_rx = None;
                    notices.push(JobNotice::Status(format!("{project} · {msg}")));
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.bg_init_rx = None;
                    break;
                }
            }
        }

        if let Some(ui) = &self.bg_init {
            if let Some(done_at) = ui.finished_at {
                if done_at.elapsed() >= Duration::from_secs(6) {
                    self.bg_init = None;
                }
            }
        }
        notices
    }
}

fn spinner_frame(started: Instant) -> char {
    let ms = started.elapsed().as_millis() as usize;
    SPINNER[(ms / 100) % SPINNER.len()]
}

pub fn truncate_status_line(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if display_width(s) <= width {
        return s.to_string();
    }
    sliding_tail(s, width)
}

fn panel_bar_area(panel: Rect, screen: Rect, row_offset: u16) -> Option<Rect> {
    let y = (panel.y + panel.height + row_offset).min(screen.height.saturating_sub(2));
    if y + 1 >= screen.height {
        return None;
    }
    let bar_area = Rect {
        x: panel.x,
        y,
        width: panel.width.max(10),
        height: 2,
    };
    if bar_area.y + bar_area.height > screen.y + screen.height {
        return None;
    }
    Some(bar_area)
}

pub fn draw_bars(frame: &mut Frame<'_>, jobs: &Jobs, panel: Rect, t: Theme) {
    let screen = frame.area();
    if let Some(ui) = &jobs.install {
        if let Some(bar_area) = panel_bar_area(panel, screen, 0) {
            draw_install_into(frame, ui, bar_area, t);
        }
    }
    if let Some(ui) = &jobs.bg_init {
        let offset = if jobs.install.is_some() { 2 } else { 0 };
        if let Some(bar_area) = panel_bar_area(panel, screen, offset) {
            draw_bg_init_into(frame, ui, bar_area, t);
        }
    }
}

fn draw_install_into(frame: &mut Frame<'_>, ui: &InstallUi, bar_area: Rect, t: Theme) {
    let label = ui.action.label().to_ascii_lowercase();
    let pct = (ui.fraction * 100.0).round() as u16;
    let spinning = ui.finished_at.is_none() && !ui.failed;
    let spin = if spinning {
        format!("{} ", spinner_frame(ui.started_at))
    } else {
        String::new()
    };
    let head = if ui.failed {
        format!("install {label} failed")
    } else if ui.finished_at.is_some() {
        format!("install {label} done")
    } else {
        format!("{spin}installing {label}  {pct}%")
    };
    let msg = truncate_status_line(&ui.message, bar_area.width as usize);
    let color = if ui.failed { Color::Red } else { ACCENT };

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

fn draw_bg_init_into(frame: &mut Frame<'_>, ui: &BgInitUi, bar_area: Rect, t: Theme) {
    let agent = ui.agent.label().to_ascii_lowercase();
    let spinning = ui.finished_at.is_none() && !ui.failed;
    let spin = if spinning {
        format!("{} ", spinner_frame(ui.started_at))
    } else {
        String::new()
    };
    let head = if ui.failed {
        format!("init {agent} failed · {}", ui.project)
    } else if ui.finished_at.is_some() {
        format!("init {agent} done · {}", ui.project)
    } else {
        format!("{spin}init {agent}… · {}", ui.project)
    };
    let color = if ui.failed { Color::Red } else { ACCENT };
    let msg = truncate_status_line(&ui.message, bar_area.width as usize);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate_status_line(&head, bar_area.width as usize),
            Style::default().fg(color).bg(t.bg),
        ))),
        Rect {
            x: bar_area.x,
            y: bar_area.y,
            width: bar_area.width,
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default()
                .fg(if ui.failed { Color::Red } else { t.soft })
                .bg(t.bg),
        ))),
        Rect {
            x: bar_area.x,
            y: bar_area.y + 1,
            width: bar_area.width,
            height: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_active_false_when_empty() {
        let j = Jobs::default();
        assert!(!j.any_active());
        assert!(!j.install_busy());
        assert!(!j.bg_init_busy());
    }

    #[test]
    fn truncate_status_respects_width() {
        let s = truncate_status_line("hello world this is long", 10);
        assert!(display_width(&s) <= 10);
    }
}
