//! Workspace discovery (FS-only candidates + demo repos).

use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    data_dir, home_dir, read_last_cwd, read_recent_workspaces, workspace_root, Action,
    LauncherState, Repo,
};

pub fn demo_mode_enabled() -> bool {
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
pub fn demo_repos() -> Vec<Repo> {
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

const SPLASH_RULE_START_MS: u64 = 450;


pub fn discover_candidates(state: &LauncherState) -> Vec<(PathBuf, &'static str)> {
    if demo_mode_enabled() {
        // Paths only; git meta left empty for demo too (or filled sync in demo_repos).
        return demo_repos()
            .into_iter()
            .map(|r| (r.path, r.badge))
            .collect();
    }

    let home = home_dir();
    let data = data_dir();
    let root = workspace_root(&state.settings);
    let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();
    let mut seen = HashSet::new();

    for path in &state.favorites {
        push_candidate(&mut candidates, &mut seen, path.clone(), "★");
    }
    for recent in read_recent_workspaces(&data) {
        push_candidate(&mut candidates, &mut seen, recent, "recent");
    }
    if let Some(last_cwd) = read_last_cwd(&data) {
        push_candidate(&mut candidates, &mut seen, last_cwd, "last");
    }
    push_candidate(&mut candidates, &mut seen, root.clone(), "root");

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
            push_candidate(&mut candidates, &mut seen, path, "");
        }
    }

    if candidates.is_empty() {
        push_candidate(&mut candidates, &mut seen, home, "home");
    }
    candidates
}

pub fn repos_from_candidates(
    candidates: Vec<(PathBuf, &'static str)>,
    state: &LauncherState,
) -> Vec<Repo> {
    if demo_mode_enabled() {
        return demo_repos();
    }
    candidates
        .into_iter()
        .map(|(path, badge)| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let remembered_agent = state.agent_for(&path);
            Repo {
                name,
                path,
                badge,
                git_branch: None,
                git_dirty: false,
                git_ahead: 0,
                remembered_agent,
            }
        })
        .collect()
}

fn push_candidate(
    candidates: &mut Vec<(PathBuf, &'static str)>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
    badge: &'static str,
) {
    if path.is_dir() && seen.insert(path.clone()) {
        candidates.push((path, badge));
    }
}
