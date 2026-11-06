//! Git integration via the `git` CLI (works the same on every platform).

use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).env("GIT_TERMINAL_PROMPT", "0").output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { err })
    }
}

pub fn repo_root(path: &Path) -> Option<PathBuf> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    git(dir, &["rev-parse", "--show-toplevel"]).ok().map(|s| PathBuf::from(s.trim()))
}

#[derive(Clone, Debug)]
pub struct StatusEntry {
    pub path: String,
    pub index: char,
    pub worktree: char,
}

impl StatusEntry {
    pub fn staged(&self) -> bool {
        self.index != ' ' && self.index != '?'
    }
    pub fn unstaged(&self) -> bool {
        self.worktree != ' '
    }
    pub fn untracked(&self) -> bool {
        self.index == '?'
    }
    pub fn conflicted(&self) -> bool {
        matches!((self.index, self.worktree), ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D'))
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitStatus {
    pub root: PathBuf,
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    pub entries: Vec<StatusEntry>,
}

pub fn status(root: &Path) -> Result<GitStatus, String> {
    let out = git(root, &["status", "--porcelain=v1", "-b", "--untracked-files=all"])?;
    let mut st = GitStatus { root: root.to_path_buf(), ..Default::default() };
    for line in out.lines() {
        if let Some(b) = line.strip_prefix("## ") {
            let head = b.split("...").next().unwrap_or(b);
            st.branch = head.trim_start_matches("No commits yet on ").split(' ').next().unwrap_or(head).to_string();
            if let Some(i) = b.find('[') {
                let info = &b[i..];
                for part in info.trim_matches(|c| c == '[' || c == ']').split(", ") {
                    if let Some(n) = part.strip_prefix("ahead ") {
                        st.ahead = n.parse().unwrap_or(0);
                    } else if let Some(n) = part.strip_prefix("behind ") {
                        st.behind = n.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        let mut cs = line.chars();
        let (x, y) = (cs.next().unwrap(), cs.next().unwrap());
        let mut path = line[3..].to_string();
        if let Some(i) = path.find(" -> ") {
            path = path[i + 4..].to_string();
        }
        st.entries.push(StatusEntry { path: path.trim_matches('"').to_string(), index: x, worktree: y });
    }
    Ok(st)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineMark {
    Added,
    Modified,
    Deleted,
}

/// Per-line change markers for the gutter (0-based lines), vs HEAD.
pub fn line_marks(root: &Path, file: &Path, untracked: bool) -> Vec<(usize, LineMark)> {
    if untracked {
        let n = std::fs::read_to_string(file).map(|s| s.lines().count()).unwrap_or(0);
        return (0..n).map(|i| (i, LineMark::Added)).collect();
    }
    let Some(rel) = file.strip_prefix(root).ok().and_then(|p| p.to_str()) else { return vec![] };
    let Ok(out) = git(root, &["diff", "-U0", "--no-color", "HEAD", "--", rel]) else { return vec![] };
    let mut marks = vec![];
    for line in out.lines() {
        let Some(h) = line.strip_prefix("@@ ") else { continue };
        let mut parts = h.split_whitespace();
        let (Some(old), Some(new)) = (parts.next(), parts.next()) else { continue };
        let parse = |s: &str| -> (usize, usize) {
            let s = s.trim_start_matches(['-', '+']);
            let mut it = s.split(',');
            let a = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            let b = it.next().and_then(|x| x.parse().ok()).unwrap_or(1);
            (a, b)
        };
        let (_, oc) = parse(old);
        let (ns, nc) = parse(new);
        if nc == 0 {
            marks.push((ns.saturating_sub(0), LineMark::Deleted));
        } else {
            let kind = if oc == 0 { LineMark::Added } else { LineMark::Modified };
            for i in 0..nc {
                marks.push((ns - 1 + i, kind));
            }
        }
    }
    marks
}

pub fn stage(root: &Path, path: &str) -> Result<(), String> {
    git(root, &["add", "--", path]).map(|_| ())
}
pub fn stage_all(root: &Path) -> Result<(), String> {
    git(root, &["add", "-A"]).map(|_| ())
}
pub fn unstage(root: &Path, path: &str) -> Result<(), String> {
    git(root, &["reset", "-q", "HEAD", "--", path]).map(|_| ())
}
pub fn discard(root: &Path, path: &str, untracked: bool) -> Result<(), String> {
    if untracked {
        std::fs::remove_file(root.join(path)).map_err(|e| e.to_string())
    } else {
        git(root, &["checkout", "--", path]).map(|_| ())
    }
}
pub fn commit(root: &Path, msg: &str) -> Result<String, String> {
    git(root, &["commit", "-m", msg])
}
pub fn push(root: &Path) -> Result<String, String> {
    git(root, &["push"])
}
pub fn pull(root: &Path) -> Result<String, String> {
    git(root, &["pull"])
}
pub fn checkout(root: &Path, branch: &str) -> Result<String, String> {
    git(root, &["checkout", branch])
}
pub fn create_branch(root: &Path, name: &str) -> Result<String, String> {
    git(root, &["checkout", "-b", name])
}
pub fn init(root: &Path) -> Result<String, String> {
    git(root, &["init"])
}

pub fn branches(root: &Path) -> Vec<String> {
    git(root, &["branch", "--format=%(refname:short)"]).map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()).unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub hash: String,
    pub author: String,
    pub when: String,
    pub subject: String,
}

pub fn log(root: &Path, n: usize) -> Vec<LogEntry> {
    let fmt = "--pretty=format:%h%x1f%an%x1f%ar%x1f%s";
    let count = format!("-n{n}");
    git(root, &["log", &count, fmt])
        .map(|s| {
            s.lines()
                .filter_map(|l| {
                    let p: Vec<&str> = l.split('\u{1f}').collect();
                    (p.len() == 4).then(|| LogEntry { hash: p[0].into(), author: p[1].into(), when: p[2].into(), subject: p[3].into() })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn diff_text(root: &Path, path: &str, staged: bool) -> String {
    let mut args = vec!["diff", "--no-color"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);
    git(root, &args).unwrap_or_default()
}
