//! File indexing and project-wide text search.

use crate::search::SearchQuery;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn walker(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !matches!(n.as_ref(), ".git" | "node_modules" | ".DS_Store" | "target" | "__pycache__" | ".venv" | "venv")
                || e.depth() == 0
        })
        .build()
}

pub fn list_files(root: &Path, cap: usize) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in walker(root).flatten() {
        if e.file_type().map_or(false, |t| t.is_file()) {
            out.push(e.into_path());
            if out.len() >= cap {
                break;
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub path: PathBuf,
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub preview: String,
}

pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// Streams hits through `emit`; stops at `cap` or when `cancel` is set.
pub fn search_in_files(root: &Path, q: &SearchQuery, cap: usize, cancel: Arc<AtomicBool>, mut emit: impl FnMut(SearchHit)) {
    let Some(re) = q.compile() else { return };
    let mut count = 0;
    for e in walker(root).flatten() {
        if cancel.load(Ordering::Relaxed) || count >= cap {
            return;
        }
        if !e.file_type().map_or(false, |t| t.is_file()) {
            continue;
        }
        if e.metadata().map_or(false, |m| m.len() > 4_000_000) {
            continue;
        }
        let Ok(bytes) = std::fs::read(e.path()) else { continue };
        if is_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (ln, line) in text.lines().enumerate() {
            for m in re.find_iter(line) {
                if m.start() == m.end() {
                    continue;
                }
                let cs = line[..m.start()].chars().count();
                let ce = cs + line[m.start()..m.end()].chars().count();
                let preview: String = line.chars().take(300).collect();
                emit(SearchHit { path: e.path().to_path_buf(), line: ln, col_start: cs, col_end: ce, preview });
                count += 1;
                if count >= cap {
                    return;
                }
            }
        }
    }
}
