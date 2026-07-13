//! Async git metadata fan-out for workspace rows.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver},
    thread,
};

use crate::discover::demo_mode_enabled;

#[derive(Clone, Debug, Default)]
pub struct GitMeta {
    pub branch: Option<String>,
    pub dirty: bool,
    pub ahead: u32,
}

pub fn spawn_git_metadata(paths: Vec<PathBuf>) -> (Receiver<(PathBuf, GitMeta)>, usize) {
    // Demo ships pre-baked branch/dirty; real inspect would clobber badges with empty.
    if demo_mode_enabled() {
        let (tx, rx) = mpsc::channel();
        drop(tx);
        return (rx, 0);
    }
    let n = paths.len();
    let (tx, rx) = mpsc::channel();
    for path in paths {
        let tx = tx.clone();
        thread::spawn(move || {
            let (branch, dirty, ahead) = inspect_git(&path);
            let _ = tx.send((
                path,
                GitMeta {
                    branch,
                    dirty,
                    ahead,
                },
            ));
        });
    }
    (rx, n)
}

/// Git snapshot for row metadata in a single spawn: `status --porcelain=v2 --branch`
/// carries branch, dirty, and ahead at once (also covers worktrees / nested repos).
/// Failures → no git badge.
fn inspect_git(path: &Path) -> (Option<String>, bool, u32) {
    let output = match Command::new("git")
        .args([
            "-C",
            &path.display().to_string(),
            "status",
            "--porcelain=v2",
            "--branch",
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return (None, false, 0),
    };
    parse_porcelain_v2(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_porcelain_v2(text: &str) -> (Option<String>, bool, u32) {
    let mut branch = None;
    let mut dirty = false;
    let mut ahead = 0u32;
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            if head != "(detached)" && !head.is_empty() {
                branch = Some(head.to_string());
            }
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            ahead = ab
                .split_whitespace()
                .next()
                .and_then(|a| a.strip_prefix('+'))
                .and_then(|a| a.parse().ok())
                .unwrap_or(0);
        } else if !line.starts_with('#') && !line.is_empty() {
            dirty = true;
        }
    }
    (branch, dirty, ahead)
}

#[cfg(test)]
mod tests {
    use super::parse_porcelain_v2;

    #[test]
    fn porcelain_v2_branch_dirty_ahead() {
        let clean = "# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +3 -0\n";
        assert_eq!(parse_porcelain_v2(clean), (Some("main".into()), false, 3));

        let dirty = "# branch.oid abc\n# branch.head feat/x\n1 .M N... 100644 100644 100644 abc def src/main.rs\n";
        assert_eq!(parse_porcelain_v2(dirty), (Some("feat/x".into()), true, 0));

        let detached = "# branch.oid abc\n# branch.head (detached)\n";
        assert_eq!(parse_porcelain_v2(detached), (None, false, 0));

        assert_eq!(parse_porcelain_v2("# branch.head main\n"), (Some("main".into()), false, 0));
    }
}
