//! Text buffer: rope storage, multi-cursor selections, edits and undo/redo.
//! All offsets are in `char` units. The buffer is UI-agnostic.

use ropey::Rope;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::lang::LangInfo;

mod ops;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(p: usize) -> Self {
        Self { anchor: p, head: p }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp(usize),
    PageDown(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Other,
}

#[derive(Clone, Debug)]
struct Edit {
    at: usize,
    removed: String,
    inserted: String,
}

#[derive(Clone, Debug)]
struct Transaction {
    edits: Vec<Edit>,
    before: Vec<Selection>,
    after: Vec<Selection>,
    kind: EditKind,
    time: Instant,
}

/// A replacement of `start..end` by `text`; `sel` is the resulting selection
/// (anchor, head) relative to `start`.
#[derive(Clone, Debug)]
pub struct Change {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub sel: (usize, usize),
}

impl Change {
    pub fn replace(start: usize, end: usize, text: impl Into<String>) -> Self {
        let text: String = text.into();
        let n = text.chars().count();
        Self { start, end, text, sel: (n, n) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndentStyle {
    pub use_spaces: bool,
    pub size: usize,
}

impl Default for IndentStyle {
    fn default() -> Self {
        Self { use_spaces: true, size: 4 }
    }
}

pub struct Buffer {
    pub rope: Rope,
    pub sels: Vec<Selection>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    pub version: u64,
    pub saved_version: u64,
    pub path: Option<PathBuf>,
    pub crlf: bool,
    pub preferred_col: Option<usize>,
    pub lang: LangInfo,
    pub indent: IndentStyle,
    first_dirty_line: Option<usize>,
    pub disk_mtime: Option<std::time::SystemTime>,
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn char_class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if is_word_char(c) {
        1
    } else {
        2
    }
}

const PAIRS: [(char, char); 6] = [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\''), ('`', '`')];

fn closer_for(c: char) -> Option<char> {
    PAIRS.iter().find(|(o, _)| *o == c).map(|(_, c)| *c)
}

impl Buffer {
    pub fn new() -> Self {
        Self::from_text("", None)
    }

    pub fn from_text(text: &str, path: Option<PathBuf>) -> Self {
        let crlf = text.contains("\r\n");
        let clean;
        let t = if crlf {
            clean = text.replace("\r\n", "\n");
            clean.as_str()
        } else {
            text
        };
        let lang = LangInfo::for_path(path.as_deref());
        let mut indent = IndentStyle::default();
        // Detect indentation style from the first indented lines.
        for l in t.lines().take(200) {
            if l.starts_with('\t') {
                indent.use_spaces = false;
                break;
            }
            let n = l.len() - l.trim_start_matches(' ').len();
            if n >= 2 && !l.trim().is_empty() {
                indent.size = if n % 4 == 0 { 4 } else { 2 };
                break;
            }
        }
        Self {
            rope: Rope::from_str(t),
            sels: vec![Selection::caret(0)],
            undo: vec![],
            redo: vec![],
            version: 0,
            saved_version: 0,
            path,
            crlf,
            preferred_col: None,
            lang,
            indent,
            first_dirty_line: Some(0),
            disk_mtime: None,
        }
    }

    pub fn open(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut b = Self::from_text(&text, Some(path.to_path_buf()));
        b.disk_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        Ok(b)
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        let Some(path) = self.path.clone() else {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "no path"));
        };
        self.save_as(&path)
    }

    pub fn save_as(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        let mut text = self.rope.to_string();
        if self.crlf {
            text = text.replace('\n', "\r\n");
        }
        std::fs::write(path, text)?;
        self.path = Some(path.to_path_buf());
        self.lang = LangInfo::for_path(Some(path));
        self.saved_version = self.version;
        self.disk_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        Ok(())
    }

    /// Replace the whole content from disk (keeps undo history cleared).
    pub fn reload(&mut self, text: &str) {
        let crlf = text.contains("\r\n");
        let t = if crlf { text.replace("\r\n", "\n") } else { text.to_string() };
        self.crlf = crlf;
        self.rope = Rope::from_str(&t);
        let n = self.rope.len_chars();
        for s in &mut self.sels {
            s.anchor = s.anchor.min(n);
            s.head = s.head.min(n);
        }
        self.undo.clear();
        self.redo.clear();
        self.version += 1;
        self.saved_version = self.version;
        self.first_dirty_line = Some(0);
    }

    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn line_count(&self) -> usize {
        // ropey counts a trailing empty line after a final '\n', like editors do.
        self.rope.len_lines()
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_char(line.min(self.line_count().saturating_sub(1)))
    }

    /// Length of a line in chars, excluding the newline.
    pub fn line_len(&self, line: usize) -> usize {
        if line >= self.line_count() {
            return 0;
        }
        let l = self.rope.line(line);
        let n = l.len_chars();
        if n > 0 && l.char(n - 1) == '\n' {
            n - 1
        } else {
            n
        }
    }

    pub fn line_end(&self, line: usize) -> usize {
        self.line_start(line) + self.line_len(line)
    }

    pub fn line_string(&self, line: usize) -> String {
        if line >= self.line_count() {
            return String::new();
        }
        let s = self.line_start(line);
        self.rope.slice(s..s + self.line_len(line)).to_string()
    }

    pub fn char_to_pos(&self, idx: usize) -> (usize, usize) {
        let idx = idx.min(self.len_chars());
        let line = self.rope.char_to_line(idx);
        (line, idx - self.rope.line_to_char(line))
    }

    pub fn pos_to_char(&self, line: usize, col: usize) -> usize {
        let line = line.min(self.line_count().saturating_sub(1));
        self.line_start(line) + col.min(self.line_len(line))
    }

    pub fn char_at(&self, idx: usize) -> Option<char> {
        if idx < self.len_chars() {
            Some(self.rope.char(idx))
        } else {
            None
        }
    }

    pub fn slice_string(&self, a: usize, b: usize) -> String {
        self.rope.slice(a..b.min(self.len_chars())).to_string()
    }

    pub fn primary(&self) -> Selection {
        *self.sels.last().unwrap()
    }

    pub fn take_dirty_line(&mut self) -> Option<usize> {
        self.first_dirty_line.take()
    }

    pub fn mark_all_dirty(&mut self) {
        self.first_dirty_line = Some(0);
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn indent_unit(&self) -> String {
        if self.indent.use_spaces {
            " ".repeat(self.indent.size)
        } else {
            "\t".to_string()
        }
    }

    // ------------------------------------------------------------------
    // Selections
    // ------------------------------------------------------------------

    pub fn normalize(&mut self) {
        let n = self.len_chars();
        for s in &mut self.sels {
            s.anchor = s.anchor.min(n);
            s.head = s.head.min(n);
        }
        if self.sels.len() <= 1 {
            return;
        }
        let primary = *self.sels.last().unwrap();
        self.sels.sort_by_key(|s| (s.start(), s.end()));
        let mut out: Vec<Selection> = Vec::with_capacity(self.sels.len());
        for s in self.sels.drain(..) {
            if let Some(last) = out.last_mut() {
                let touch = if s.is_empty() || last.is_empty() { s.start() <= last.end() } else { s.start() < last.end() };
                if touch {
                    // merge, keeping direction of the later one
                    let start = last.start().min(s.start());
                    let end = last.end().max(s.end());
                    *last = if s.head >= s.anchor { Selection { anchor: start, head: end } } else { Selection { anchor: end, head: start } };
                    continue;
                }
            }
            out.push(s);
        }
        // keep primary last if it survived (best effort)
        if let Some(pos) = out.iter().position(|s| s.start() <= primary.head && primary.head <= s.end()) {
            let p = out.remove(pos);
            out.push(p);
            // re-sort order matters for edits; we sort again at apply time
            out.sort_by_key(|s| (s.start(), s.end()));
        }
        self.sels = out;
    }

    pub fn set_single_caret(&mut self, pos: usize) {
        self.sels = vec![Selection::caret(pos.min(self.len_chars()))];
        self.preferred_col = None;
    }

    pub fn set_selection(&mut self, anchor: usize, head: usize) {
        let n = self.len_chars();
        self.sels = vec![Selection { anchor: anchor.min(n), head: head.min(n) }];
        self.preferred_col = None;
    }

    pub fn add_caret(&mut self, pos: usize) {
        self.sels.push(Selection::caret(pos.min(self.len_chars())));
        self.normalize();
    }

    pub fn select_all(&mut self) {
        let n = self.len_chars();
        self.sels = vec![Selection { anchor: 0, head: n }];
    }

    pub fn collapse_to_primary(&mut self) {
        let p = self.primary();
        self.sels = vec![Selection::caret(p.head)];
    }

    pub fn word_range_at(&self, pos: usize) -> (usize, usize) {
        let n = self.len_chars();
        let pos = pos.min(n);
        let cls = |i: usize| self.char_at(i).map(char_class);
        let here = cls(pos).or_else(|| if pos > 0 { cls(pos - 1) } else { None });
        let Some(c) = here else { return (pos, pos) };
        let mut s = pos;
        let mut e = pos;
        // choose the class of the char at pos if present else previous
        let target = if cls(pos).is_some() { cls(pos).unwrap() } else { c };
        while s > 0 && cls(s - 1) == Some(target) {
            s -= 1;
        }
        while e < n && cls(e) == Some(target) {
            e += 1;
        }
        (s, e)
    }

    pub fn select_line_at(&mut self, pos: usize) {
        let (l, _) = self.char_to_pos(pos);
        let s = self.line_start(l);
        let e = if l + 1 < self.line_count() { self.line_start(l + 1) } else { self.len_chars() };
        self.sels = vec![Selection { anchor: s, head: e }];
    }

    pub fn expand_line_selection(&mut self) {
        for i in 0..self.sels.len() {
            let s = self.sels[i];
            let (l1, _) = self.char_to_pos(s.start());
            let (l2, c2) = self.char_to_pos(s.end());
            let l2 = if !s.is_empty() && c2 == 0 && l2 > l1 { l2 } else { l2 + 1 };
            let a = self.line_start(l1);
            let b = if l2 < self.line_count() { self.line_start(l2) } else { self.len_chars() };
            self.sels[i] = Selection { anchor: a, head: b };
        }
        self.normalize();
    }

    pub fn selected_text(&self) -> String {
        let mut parts = vec![];
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        for s in sels {
            if !s.is_empty() {
                parts.push(self.slice_string(s.start(), s.end()));
            }
        }
        parts.join("\n")
    }

    pub fn has_selection(&self) -> bool {
        self.sels.iter().any(|s| !s.is_empty())
    }

    /// Text for the clipboard: selection, or whole line(s) when nothing is selected.
    pub fn copy_text(&self) -> String {
        if self.has_selection() {
            return self.selected_text();
        }
        let mut lines = vec![];
        let mut last = usize::MAX;
        for s in &self.sels {
            let (l, _) = self.char_to_pos(s.head);
            if l != last {
                lines.push(self.line_string(l));
                last = l;
            }
        }
        let mut t = lines.join("\n");
        t.push('\n');
        t
    }

    // ------------------------------------------------------------------
    // Edit primitive
    // ------------------------------------------------------------------

    /// Apply non-overlapping changes (any order). Sets selections from `Change::sel`.
    pub fn apply(&mut self, mut changes: Vec<Change>, kind: EditKind) {
        if changes.is_empty() {
            return;
        }
        changes.sort_by_key(|c| (c.start, c.end));
        // drop overlapping
        let mut clean: Vec<Change> = Vec::with_capacity(changes.len());
        for c in changes {
            if let Some(l) = clean.last() {
                if c.start < l.end {
                    continue;
                }
            }
            clean.push(c);
        }
        let n = self.len_chars();
        for c in &mut clean {
            c.start = c.start.min(n);
            c.end = c.end.min(n).max(c.start);
        }
        let before = self.sels.clone();
        let first_line = self.rope.char_to_line(clean[0].start.min(n));
        let mut edits = vec![];
        for ch in clean.iter().rev() {
            if ch.end == ch.start && ch.text.is_empty() {
                continue;
            }
            let removed = if ch.end > ch.start { self.rope.slice(ch.start..ch.end).to_string() } else { String::new() };
            if ch.end > ch.start {
                self.rope.remove(ch.start..ch.end);
            }
            if !ch.text.is_empty() {
                self.rope.insert(ch.start, &ch.text);
            }
            edits.push(Edit { at: ch.start, removed, inserted: ch.text.clone() });
        }
        let mut delta: isize = 0;
        let mut sels = Vec::with_capacity(clean.len());
        for ch in &clean {
            let tl = ch.text.chars().count() as isize;
            let base = (ch.start as isize + delta) as usize;
            sels.push(Selection { anchor: base + ch.sel.0, head: base + ch.sel.1 });
            delta += tl - (ch.end - ch.start) as isize;
        }
        self.sels = sels;
        self.normalize();
        self.preferred_col = None;
        if edits.is_empty() {
            return;
        }
        self.version += 1;
        self.first_dirty_line = Some(self.first_dirty_line.map_or(first_line, |l| l.min(first_line)));
        self.redo.clear();
        let after = self.sels.clone();
        let now = Instant::now();
        if kind == EditKind::Typing {
            if let Some(last) = self.undo.last_mut() {
                if last.kind == EditKind::Typing && now.duration_since(last.time) < Duration::from_millis(1000) {
                    last.edits.extend(edits);
                    last.after = after;
                    last.time = now;
                    return;
                }
            }
        }
        self.undo.push(Transaction { edits, before, after, kind, time: now });
        if self.undo.len() > 2000 {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self) {
        let Some(tx) = self.undo.pop() else { return };
        let mut min_at = usize::MAX;
        for e in tx.edits.iter().rev() {
            let ins = e.inserted.chars().count();
            if ins > 0 {
                self.rope.remove(e.at..e.at + ins);
            }
            if !e.removed.is_empty() {
                self.rope.insert(e.at, &e.removed);
            }
            min_at = min_at.min(e.at);
        }
        self.sels = tx.before.clone();
        self.normalize();
        self.version += 1;
        self.mark_dirty_at(min_at);
        self.redo.push(tx);
    }

    pub fn redo(&mut self) {
        let Some(tx) = self.redo.pop() else { return };
        let mut min_at = usize::MAX;
        for e in tx.edits.iter() {
            let n = e.removed.chars().count();
            if n > 0 {
                self.rope.remove(e.at..e.at + n);
            }
            if !e.inserted.is_empty() {
                self.rope.insert(e.at, &e.inserted);
            }
            min_at = min_at.min(e.at);
        }
        self.sels = tx.after.clone();
        self.normalize();
        self.version += 1;
        self.mark_dirty_at(min_at);
        self.undo.push(tx);
    }

    fn mark_dirty_at(&mut self, at: usize) {
        let line = self.rope.char_to_line(at.min(self.len_chars()));
        self.first_dirty_line = Some(self.first_dirty_line.map_or(line, |l| l.min(line)));
    }
}
