//! Editing and navigation operations on [`Buffer`].

use super::*;
use crate::search::SearchQuery;

impl Buffer {
    // ------------------------------------------------------------------
    // Motion
    // ------------------------------------------------------------------

    pub fn move_cursors(&mut self, m: Motion, extend: bool) {
        let vertical = matches!(m, Motion::Up | Motion::Down | Motion::PageUp(_) | Motion::PageDown(_));
        let pref = self.preferred_col;
        let mut new_pref = pref;
        for i in 0..self.sels.len() {
            let s = self.sels[i];
            let head = s.head;
            let (line, col) = self.char_to_pos(head);
            let target = match m {
                Motion::Left => {
                    if !extend && !s.is_empty() {
                        s.start()
                    } else {
                        head.saturating_sub(1)
                    }
                }
                Motion::Right => {
                    if !extend && !s.is_empty() {
                        s.end()
                    } else {
                        (head + 1).min(self.len_chars())
                    }
                }
                Motion::Up | Motion::Down | Motion::PageUp(_) | Motion::PageDown(_) => {
                    let want = pref.unwrap_or(col);
                    new_pref = Some(want);
                    let delta: isize = match m {
                        Motion::Up => -1,
                        Motion::Down => 1,
                        Motion::PageUp(n) => -(n as isize),
                        Motion::PageDown(n) => n as isize,
                        _ => 0,
                    };
                    let nl = line as isize + delta;
                    if nl < 0 {
                        0
                    } else if nl as usize >= self.line_count() {
                        self.len_chars()
                    } else {
                        self.pos_to_char(nl as usize, want)
                    }
                }
                Motion::WordLeft => self.word_left(head),
                Motion::WordRight => self.word_right(head),
                Motion::LineStart => {
                    let ls = self.line_start(line);
                    let text = self.line_string(line);
                    let first = text.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                    if first > 0 && col != first {
                        ls + first
                    } else {
                        ls
                    }
                }
                Motion::LineEnd => self.line_end(line),
                Motion::DocStart => 0,
                Motion::DocEnd => self.len_chars(),
            };
            self.sels[i] = if extend { Selection { anchor: s.anchor, head: target } } else { Selection::caret(target) };
        }
        self.preferred_col = if vertical { new_pref } else { None };
        self.normalize();
    }

    pub fn word_left(&self, mut p: usize) -> usize {
        while p > 0 && self.char_at(p - 1).map_or(false, |c| c.is_whitespace() && c != '\n') {
            p -= 1;
        }
        if p > 0 && self.char_at(p - 1) == Some('\n') {
            return p - 1;
        }
        if p == 0 {
            return 0;
        }
        let cls = char_class(self.char_at(p - 1).unwrap());
        while p > 0 && self.char_at(p - 1).map_or(false, |c| char_class(c) == cls) {
            p -= 1;
        }
        p
    }

    pub fn word_right(&self, mut p: usize) -> usize {
        let n = self.len_chars();
        while p < n && self.char_at(p).map_or(false, |c| c.is_whitespace() && c != '\n') {
            p += 1;
        }
        if p < n && self.char_at(p) == Some('\n') {
            return p + 1;
        }
        if p >= n {
            return n;
        }
        let cls = char_class(self.char_at(p).unwrap());
        while p < n && self.char_at(p).map_or(false, |c| char_class(c) == cls) {
            p += 1;
        }
        p
    }

    // ------------------------------------------------------------------
    // Text entry
    // ------------------------------------------------------------------

    /// Insert text at every cursor (typing without auto-pairing).
    pub fn insert_text(&mut self, text: &str, kind: EditKind) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let lines: Vec<&str> = text.split('\n').collect();
        let distribute = sels.len() > 1 && lines.len() == sels.len();
        let changes = sels
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let t = if distribute { lines[i].to_string() } else { text.clone() };
                Change::replace(s.start(), s.end(), t)
            })
            .collect();
        self.apply(changes, kind);
    }

    /// Paste: like insert, but re-indents nothing (keeps it predictable).
    pub fn paste(&mut self, text: &str) {
        // Full-line paste from a "copy with no selection" goes above the current line.
        if !self.has_selection() && text.ends_with('\n') && self.sels.len() == 1 {
            let (l, c) = self.char_to_pos(self.sels[0].head);
            let ls = self.line_start(l);
            let t = text.replace("\r\n", "\n");
            let n = t.chars().count();
            self.apply(vec![Change { start: ls, end: ls, text: t, sel: (n + c, n + c) }], EditKind::Other);
            return;
        }
        self.insert_text(text, EditKind::Other);
    }

    /// Type one character with auto-close / overtype / surround behaviour.
    pub fn type_char(&mut self, c: char) {
        if c == '>' {
            if let Some(close) = self.auto_close_tag() {
                let p = self.primary().head;
                self.apply(vec![Change { start: p, end: p, text: format!(">{close}"), sel: (1, 1) }], EditKind::Typing);
                return;
            }
        }
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let mut changes = vec![];
        for s in &sels {
            let next = self.char_at(s.end());
            let prev = if s.start() > 0 { self.char_at(s.start() - 1) } else { None };
            // Surround selection
            if !s.is_empty() {
                let md_pair = if self.lang.id == "markdown" && matches!(c, '*' | '_' | '~') { Some(c) } else { None };
                if let Some(close) = closer_for(c).or(md_pair) {
                    let inner = self.slice_string(s.start(), s.end());
                    let n = inner.chars().count();
                    let text = format!("{c}{inner}{close}");
                    let (a, h) = if s.head >= s.anchor { (1, 1 + n) } else { (1 + n, 1) };
                    changes.push(Change { start: s.start(), end: s.end(), text, sel: (a, h) });
                    continue;
                }
                changes.push(Change::replace(s.start(), s.end(), c.to_string()));
                continue;
            }
            // Overtype closer
            let is_closer = PAIRS.iter().any(|(_, cl)| *cl == c);
            if is_closer && next == Some(c) {
                changes.push(Change { start: s.start(), end: s.start(), text: String::new(), sel: (1, 1) });
                continue;
            }
            // Auto-close opener
            if let Some(close) = closer_for(c) {
                let next_ok = next.map_or(true, |n| n.is_whitespace() || ")]}>,;:.".contains(n));
                let quote = c == '"' || c == '\'' || c == '`';
                let prev_word = prev.map_or(false, is_word_char);
                let skip = (quote && prev_word) || (c == '\'' && self.lang.id == "rust" && prev_word);
                if next_ok && !skip {
                    changes.push(Change { start: s.start(), end: s.start(), text: format!("{c}{close}"), sel: (1, 1) });
                    continue;
                }
            }
            changes.push(Change::replace(s.start(), s.end(), c.to_string()));
        }
        // A pure overtype produces no edit; apply handles that by only moving carets.
        self.apply(changes, EditKind::Typing);
    }

    pub fn newline(&mut self) {
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let unit = self.indent_unit();
        let mut changes = vec![];
        for s in &sels {
            let (line, col) = self.char_to_pos(s.start());
            let text = self.line_string(line);
            let upto: String = text.chars().take(col).collect();
            let indent: String = upto.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            let before = upto.trim_end().chars().last();
            let after = self.char_at(s.end());
            let opens = matches!(before, Some('{') | Some('(') | Some('['))
                || (before == Some(':') && self.lang.colon_indent)
                || (self.lang.id == "html" && before == Some('>') && after == Some('<'));
            if opens {
                let closes = matches!((before, after), (Some('{'), Some('}')) | (Some('('), Some(')')) | (Some('['), Some(']')))
                    || (self.lang.id == "html" && before == Some('>') && after == Some('<'));
                if closes {
                    let mid = format!("\n{indent}{unit}");
                    let n = mid.chars().count();
                    let t = format!("{mid}\n{indent}");
                    changes.push(Change { start: s.start(), end: s.end(), text: t, sel: (n, n) });
                } else {
                    changes.push(Change::replace(s.start(), s.end(), format!("\n{indent}{unit}")));
                }
            } else {
                changes.push(Change::replace(s.start(), s.end(), format!("\n{indent}")));
            }
        }
        self.apply(changes, EditKind::Other);
    }

    pub fn backspace(&mut self) {
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let unit = self.indent.size;
        let mut changes = vec![];
        for s in &sels {
            if !s.is_empty() {
                changes.push(Change::replace(s.start(), s.end(), ""));
                continue;
            }
            let p = s.head;
            if p == 0 {
                changes.push(Change { start: 0, end: 0, text: String::new(), sel: (0, 0) });
                continue;
            }
            let prev = self.char_at(p - 1).unwrap();
            let next = self.char_at(p);
            // delete matching pair
            if let (Some(close), Some(n)) = (closer_for(prev), next) {
                if close == n {
                    changes.push(Change::replace(p - 1, p + 1, ""));
                    continue;
                }
            }
            // smart unindent over spaces
            let (line, col) = self.char_to_pos(p);
            if self.indent.use_spaces && col > 0 {
                let text = self.line_string(line);
                let upto: String = text.chars().take(col).collect();
                if upto.chars().all(|c| c == ' ') && col % unit == 0 {
                    changes.push(Change::replace(p - unit, p, ""));
                    continue;
                }
            }
            changes.push(Change::replace(p - 1, p, ""));
        }
        self.apply(changes, EditKind::Typing);
    }

    pub fn delete_forward(&mut self) {
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let n = self.len_chars();
        let changes = sels
            .iter()
            .map(|s| if s.is_empty() { Change::replace(s.head, (s.head + 1).min(n), "") } else { Change::replace(s.start(), s.end(), "") })
            .collect();
        self.apply(changes, EditKind::Typing);
    }

    pub fn delete_word_left(&mut self) {
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let changes = sels
            .iter()
            .map(|s| if s.is_empty() { Change::replace(self.word_left(s.head), s.head, "") } else { Change::replace(s.start(), s.end(), "") })
            .collect();
        self.apply(changes, EditKind::Other);
    }

    pub fn delete_word_right(&mut self) {
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let changes = sels
            .iter()
            .map(|s| if s.is_empty() { Change::replace(s.head, self.word_right(s.head), "") } else { Change::replace(s.start(), s.end(), "") })
            .collect();
        self.apply(changes, EditKind::Other);
    }

    pub fn delete_to_line_start(&mut self) {
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let changes = sels
            .iter()
            .map(|s| {
                if s.is_empty() {
                    let (l, _) = self.char_to_pos(s.head);
                    Change::replace(self.line_start(l), s.head, "")
                } else {
                    Change::replace(s.start(), s.end(), "")
                }
            })
            .collect();
        self.apply(changes, EditKind::Other);
    }

    pub fn delete_to_line_end(&mut self) {
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let changes = sels
            .iter()
            .map(|s| {
                if s.is_empty() {
                    let (l, _) = self.char_to_pos(s.head);
                    let e = self.line_end(l);
                    Change::replace(s.head, if e == s.head { (s.head + 1).min(self.len_chars()) } else { e }, "")
                } else {
                    Change::replace(s.start(), s.end(), "")
                }
            })
            .collect();
        self.apply(changes, EditKind::Other);
    }

    // ------------------------------------------------------------------
    // Line operations
    // ------------------------------------------------------------------

    /// Merged inclusive line ranges touched by the selections.
    fn selected_line_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = vec![];
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        for s in sels {
            let (l1, _) = self.char_to_pos(s.start());
            let (mut l2, c2) = self.char_to_pos(s.end());
            if !s.is_empty() && c2 == 0 && l2 > l1 {
                l2 -= 1;
            }
            if let Some(last) = ranges.last_mut() {
                if l1 <= last.1 {
                    last.1 = last.1.max(l2);
                    continue;
                }
            }
            ranges.push((l1, l2));
        }
        ranges
    }

    pub fn indent_lines(&mut self) {
        let unit = self.indent_unit();
        let ulen = unit.chars().count();
        let mut changes = vec![];
        for (l1, l2) in self.selected_line_ranges() {
            for l in l1..=l2 {
                if self.line_len(l) == 0 && l1 != l2 {
                    continue;
                }
                let s = self.line_start(l);
                changes.push(Change { start: s, end: s, text: unit.clone(), sel: (0, 0) });
            }
        }
        // keep selections: shift by applying selection mapping manually
        let old = self.sels.clone();
        let starts: Vec<usize> = changes.iter().map(|c| c.start).collect();
        self.apply(changes, EditKind::Other);
        self.sels = old
            .iter()
            .map(|s| {
                let map = |p: usize| p + ulen * starts.iter().filter(|&&st| st <= p).count();
                Selection { anchor: map(s.anchor), head: map(s.head) }
            })
            .collect();
        self.normalize();
    }

    pub fn outdent_lines(&mut self) {
        let size = self.indent.size;
        let mut changes = vec![];
        let mut removed_at: Vec<(usize, usize)> = vec![];
        for (l1, l2) in self.selected_line_ranges() {
            for l in l1..=l2 {
                let text = self.line_string(l);
                let n = if text.starts_with('\t') {
                    1
                } else {
                    text.chars().take(size).take_while(|c| *c == ' ').count()
                };
                if n > 0 {
                    let s = self.line_start(l);
                    changes.push(Change { start: s, end: s + n, text: String::new(), sel: (0, 0) });
                    removed_at.push((s, n));
                }
            }
        }
        let old = self.sels.clone();
        self.apply(changes, EditKind::Other);
        self.sels = old
            .iter()
            .map(|s| {
                let map = |p: usize| {
                    let mut shift = 0;
                    for &(st, n) in &removed_at {
                        if p >= st + n {
                            shift += n;
                        } else if p > st {
                            shift += p - st;
                        }
                    }
                    p - shift
                };
                Selection { anchor: map(s.anchor), head: map(s.head) }
            })
            .collect();
        self.normalize();
    }

    /// Tab key: indent selection lines, or insert spaces up to the next stop.
    pub fn tab(&mut self) {
        let multi_line = self.sels.iter().any(|s| !s.is_empty() && self.char_to_pos(s.start()).0 != self.char_to_pos(s.end()).0);
        if multi_line {
            self.indent_lines();
            return;
        }
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let size = self.indent.size;
        let changes = sels
            .iter()
            .map(|s| {
                if self.indent.use_spaces {
                    let (_, col) = self.char_to_pos(s.start());
                    Change::replace(s.start(), s.end(), " ".repeat(size - col % size))
                } else {
                    Change::replace(s.start(), s.end(), "\t")
                }
            })
            .collect();
        self.apply(changes, EditKind::Typing);
    }

    pub fn shift_tab(&mut self) {
        self.outdent_lines();
    }

    pub fn delete_lines(&mut self) {
        let mut changes = vec![];
        for (l1, l2) in self.selected_line_ranges() {
            let s = self.line_start(l1);
            let e = if l2 + 1 < self.line_count() { self.line_start(l2 + 1) } else { self.len_chars() };
            let s = if e == self.len_chars() && s > 0 && l2 + 1 >= self.line_count() { s - 1 } else { s };
            changes.push(Change { start: s, end: e, text: String::new(), sel: (0, 0) });
        }
        self.apply(changes, EditKind::Other);
    }

    pub fn insert_line_below(&mut self) {
        let mut changes = vec![];
        let mut seen = vec![];
        for s in self.sels.clone() {
            let (l, _) = self.char_to_pos(s.end());
            if seen.contains(&l) {
                continue;
            }
            seen.push(l);
            let indent: String = self.line_string(l).chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            let e = self.line_end(l);
            let t = format!("\n{indent}");
            let n = t.chars().count();
            changes.push(Change { start: e, end: e, text: t, sel: (n, n) });
        }
        self.apply(changes, EditKind::Other);
    }

    pub fn insert_line_above(&mut self) {
        let mut changes = vec![];
        let mut seen = vec![];
        for s in self.sels.clone() {
            let (l, _) = self.char_to_pos(s.start());
            if seen.contains(&l) {
                continue;
            }
            seen.push(l);
            let indent: String = self.line_string(l).chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            let st = self.line_start(l);
            let n = indent.chars().count();
            changes.push(Change { start: st, end: st, text: format!("{indent}\n"), sel: (n, n) });
        }
        self.apply(changes, EditKind::Other);
    }

    pub fn move_lines(&mut self, up: bool) {
        let mut changes = vec![];
        let ranges = self.selected_line_ranges();
        let last_line = self.line_count() - 1;
        for (l1, l2) in ranges {
            if up && l1 == 0 || !up && l2 >= last_line {
                continue;
            }
            let (ra, rb) = if up { (l1 - 1, l2) } else { (l1, l2 + 1) };
            let start = self.line_start(ra);
            let end = if rb + 1 < self.line_count() { self.line_start(rb + 1) } else { self.len_chars() };
            let ends_nl = end > start && self.char_at(end - 1) == Some('\n');
            let (adj, blk_a, blk_b) = if up { (l1 - 1, l1, l2) } else { (l2 + 1, l1, l2) };
            let adj_core = self.line_string(adj);
            let mut blk_lines = vec![];
            for l in blk_a..=blk_b {
                blk_lines.push(self.line_string(l));
            }
            let blk_core = blk_lines.join("\n");
            let mut text = if up { format!("{blk_core}\n{adj_core}") } else { format!("{adj_core}\n{blk_core}") };
            if ends_nl {
                text.push('\n');
            }
            let shift = adj_core.chars().count() + 1;
            // translate each selection that lies in the block
            let blk_start = self.line_start(blk_a);
            let blk_end = self.line_end(blk_b);
            let mut sel_here = None;
            for s in &self.sels {
                if s.start() >= blk_start && s.end() <= blk_end + 1 {
                    sel_here = Some(*s);
                }
            }
            let (a, h) = match sel_here {
                Some(s) => {
                    let ra_ = s.anchor - start;
                    let rh_ = s.head - start;
                    if up {
                        (ra_ - shift, rh_ - shift)
                    } else {
                        (ra_ + shift, rh_ + shift)
                    }
                }
                None => (0, 0),
            };
            changes.push(Change { start, end, text, sel: (a, h) });
        }
        if !changes.is_empty() {
            self.apply(changes, EditKind::Other);
        }
    }

    pub fn copy_lines(&mut self, up: bool) {
        let mut changes = vec![];
        for (l1, l2) in self.selected_line_ranges() {
            let start = self.line_start(l1);
            let end = if l2 + 1 < self.line_count() { self.line_start(l2 + 1) } else { self.len_chars() };
            let mut block = self.slice_string(start, end);
            let mut extra = 0;
            if !block.ends_with('\n') {
                // last line without newline
                if up {
                    block.push('\n');
                } else {
                    block.insert(0, '\n');
                    extra = 1;
                }
            }
            let at = if up { start } else { end };
            let mut sel = (0, 0);
            for s in &self.sels {
                if s.start() >= start && s.end() <= end {
                    sel = (s.anchor - start + extra, s.head - start + extra);
                }
            }
            changes.push(Change { start: at, end: at, text: block, sel });
        }
        self.apply(changes, EditKind::Other);
    }

    pub fn toggle_comment(&mut self) {
        let (open, close) = match (self.lang.line_comment, self.lang.block_comment) {
            (Some(l), _) => (l.to_string(), String::new()),
            (None, Some((o, c))) => (o.to_string(), c.to_string()),
            _ => return,
        };
        let open_sp = format!("{open} ");
        let ranges = self.selected_line_ranges();
        let mut lines = vec![];
        for (l1, l2) in &ranges {
            for l in *l1..=*l2 {
                lines.push(l);
            }
        }
        let non_blank: Vec<usize> = lines.iter().copied().filter(|&l| !self.line_string(l).trim().is_empty()).collect();
        if non_blank.is_empty() {
            return;
        }
        let all_commented = non_blank.iter().all(|&l| self.line_string(l).trim_start().starts_with(open.as_str()));
        let min_indent = non_blank.iter().map(|&l| self.line_string(l).chars().take_while(|c| c.is_whitespace()).count()).min().unwrap_or(0);
        let mut changes = vec![];
        for &l in &lines {
            let text = self.line_string(l);
            if text.trim().is_empty() {
                continue;
            }
            let ls = self.line_start(l);
            if all_commented {
                let ind = text.chars().take_while(|c| c.is_whitespace()).count();
                let rest: String = text.chars().skip(ind).collect();
                let strip = if rest.starts_with(open_sp.as_str()) { open_sp.chars().count() } else { open.chars().count() };
                changes.push(Change { start: ls + ind, end: ls + ind + strip, text: String::new(), sel: (0, 0) });
                if !close.is_empty() {
                    let t = text.trim_end();
                    let cl = if t.ends_with(&format!(" {close}")) { close.chars().count() + 1 } else { close.chars().count() };
                    let e = ls + t.chars().count();
                    if t.ends_with(close.as_str()) {
                        changes.push(Change { start: e - cl, end: e, text: String::new(), sel: (0, 0) });
                    }
                }
            } else {
                changes.push(Change { start: ls + min_indent, end: ls + min_indent, text: open_sp.clone(), sel: (0, 0) });
                if !close.is_empty() {
                    let e = ls + text.trim_end().chars().count();
                    changes.push(Change { start: e, end: e, text: format!(" {close}"), sel: (0, 0) });
                }
            }
        }
        let old = self.sels.clone();
        // map selections through the edit list
        let spans: Vec<(usize, usize, usize)> = changes.iter().map(|c| (c.start, c.end, c.text.chars().count())).collect();
        self.apply(changes, EditKind::Other);
        self.sels = old
            .iter()
            .map(|s| {
                let map = |p: usize| {
                    let mut q = p as isize;
                    for &(st, en, tl) in &spans {
                        if st < p || (st == p && en > st) {
                            let removed_before = en.min(p).saturating_sub(st);
                            q += tl as isize - removed_before as isize;
                        } else if st == p && tl > 0 && en == st {
                            // insertion exactly at caret: keep caret after it for line comments
                            q += tl as isize;
                        }
                    }
                    q.max(0) as usize
                };
                Selection { anchor: map(s.anchor), head: map(s.head) }
            })
            .collect();
        self.normalize();
    }

    // ------------------------------------------------------------------
    // Multi-cursor
    // ------------------------------------------------------------------

    pub fn add_cursor_vertical(&mut self, up: bool) {
        let mut add = vec![];
        for s in &self.sels {
            let (l, c) = self.char_to_pos(s.head);
            let want = self.preferred_col.unwrap_or(c);
            if up && l > 0 {
                add.push(Selection::caret(self.pos_to_char(l - 1, want)));
            } else if !up && l + 1 < self.line_count() {
                add.push(Selection::caret(self.pos_to_char(l + 1, want)));
            }
        }
        self.sels.extend(add);
        self.normalize();
    }

    pub fn add_next_occurrence(&mut self) {
        let last = self.primary();
        if last.is_empty() {
            let (a, b) = self.word_range_at(last.head);
            if a != b {
                let n = self.sels.len();
                self.sels[n - 1] = Selection { anchor: a, head: b };
            }
            return;
        }
        let needle = self.slice_string(last.start(), last.end());
        let hay_start = last.end();
        let tail = self.slice_string(hay_start, self.len_chars());
        let found = tail.find(&needle).map(|b| (hay_start + tail[..b].chars().count(), needle.chars().count())).or_else(|| {
            let head = self.slice_string(0, last.start());
            head.find(&needle).map(|b| (head[..b].chars().count(), needle.chars().count()))
        });
        if let Some((s, n)) = found {
            if !self.sels.iter().any(|x| x.start() == s && x.end() == s + n) {
                self.sels.push(Selection { anchor: s, head: s + n });
            }
        }
    }

    pub fn select_all_occurrences(&mut self) {
        let last = self.primary();
        let (a, b) = if last.is_empty() { self.word_range_at(last.head) } else { (last.start(), last.end()) };
        if a == b {
            return;
        }
        let needle = self.slice_string(a, b);
        let text = self.text();
        let mut sels = vec![];
        let nlen = needle.chars().count();
        for (byte, _) in text.match_indices(&needle) {
            let s = self.rope.byte_to_char(byte);
            sels.push(Selection { anchor: s, head: s + nlen });
        }
        if !sels.is_empty() {
            self.sels = sels;
        }
    }

    // ------------------------------------------------------------------
    // Find / replace
    // ------------------------------------------------------------------

    pub fn find_all(&self, q: &SearchQuery) -> Vec<(usize, usize)> {
        let Some(re) = q.compile() else { return vec![] };
        let text = self.text();
        let mut out = vec![];
        for m in re.find_iter(&text).take(50_000) {
            if m.start() == m.end() {
                continue;
            }
            out.push((self.rope.byte_to_char(m.start()), self.rope.byte_to_char(m.end())));
        }
        out
    }

    pub fn replace_matches(&mut self, q: &SearchQuery, replacement: &str, only: Option<(usize, usize)>) -> usize {
        let Some(re) = q.compile() else { return 0 };
        let matches = self.find_all(q);
        let mut changes = vec![];
        for (a, b) in matches {
            if let Some((oa, ob)) = only {
                if (a, b) != (oa, ob) {
                    continue;
                }
            }
            let found = self.slice_string(a, b);
            let rep = if q.regex { re.replace(&found, replacement).to_string() } else { replacement.to_string() };
            changes.push(Change::replace(a, b, rep));
        }
        let n = changes.len();
        if n > 0 {
            self.apply(changes, EditKind::Other);
        }
        n
    }
}

// ----------------------------------------------------------------------
// Smart editing helpers
// ----------------------------------------------------------------------

const VOID_TAGS: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];

impl Buffer {
    /// The identifier being typed before the primary caret, and where it starts.
    pub fn prefix_at_caret(&self) -> (String, usize) {
        let head = self.primary().head;
        let mut s = head;
        while s > 0 && self.char_at(s - 1).map_or(false, is_word_char) {
            s -= 1;
        }
        (self.slice_string(s, head), s)
    }

    /// Identifier counts in a window of lines around the caret.
    pub fn nearby_words(&self, radius: usize) -> std::collections::HashMap<String, u32> {
        let (line, _) = self.char_to_pos(self.primary().head);
        let a = self.line_start(line.saturating_sub(radius));
        let b_line = (line + radius).min(self.line_count().saturating_sub(1));
        let b = self.line_end(b_line);
        let mut m = std::collections::HashMap::new();
        crate::complete::index_words(&self.slice_string(a, b), &mut m);
        m
    }

    /// Replace the whole document (formatting), keeping the caret near where it was.
    pub fn replace_all_text(&mut self, new: &str) {
        let old = self.text();
        if old == new {
            return;
        }
        let head = self.primary().head;
        let (line, col) = self.char_to_pos(head);
        // keep the caret on the same line-from-end if the line count changed a lot
        let n = self.len_chars();
        let new_lines = new.split('\n').count();
        let target_line = line.min(new_lines.saturating_sub(1));
        let target_col = col;
        self.apply(vec![Change { start: 0, end: n, text: new.to_string(), sel: (0, 0) }], EditKind::Other);
        let pos = self.pos_to_char(target_line, target_col);
        self.set_single_caret(pos);
    }

    /// If typing `>` closes an open tag, return the closing tag to insert after it.
    pub fn auto_close_tag(&self) -> Option<String> {
        if !matches!(self.lang.id, "html" | "xml" | "javascriptreact" | "typescriptreact") || self.sels.len() != 1 || !self.primary().is_empty() {
            return None;
        }
        let head = self.primary().head;
        let (line, col) = self.char_to_pos(head);
        let before: String = self.line_string(line).chars().take(col).collect();
        let lt = before.rfind('<')?;
        let inner = &before[lt + 1..];
        if inner.contains('>') || inner.starts_with('/') || inner.starts_with('!') || inner.starts_with('?') || inner.ends_with('/') {
            return None;
        }
        let name: String = inner.chars().take_while(|c| c.is_alphanumeric() || *c == '-' || *c == ':' || *c == '.').collect();
        if name.is_empty() || VOID_TAGS.contains(&name.to_ascii_lowercase().as_str()) {
            return None;
        }
        // already closed right after the caret?
        let after = self.slice_string(head, (head + name.chars().count() + 3).min(self.len_chars()));
        if after.starts_with(&format!("</{name}")) {
            return None;
        }
        Some(format!("</{name}>"))
    }

    pub fn toggle_block_comment(&mut self) {
        let Some((open, close)) = self.lang.block_comment else {
            self.toggle_comment();
            return;
        };
        let mut sels = self.sels.clone();
        sels.sort_by_key(|s| s.start());
        let mut changes = vec![];
        for s in &sels {
            let (a, b) = if s.is_empty() {
                let (l, _) = self.char_to_pos(s.head);
                let text = self.line_string(l);
                let lead = text.chars().take_while(|c| c.is_whitespace()).count();
                let trail = text.chars().rev().take_while(|c| c.is_whitespace()).count();
                let ls = self.line_start(l);
                (ls + lead, ls + text.chars().count().saturating_sub(trail))
            } else {
                (s.start(), s.end())
            };
            if a >= b {
                continue;
            }
            let text = self.slice_string(a, b);
            let new = if text.starts_with(open) && text.ends_with(close) && text.len() >= open.len() + close.len() {
                let inner = &text[open.len()..text.len() - close.len()];
                inner.strip_prefix(' ').unwrap_or(inner).strip_suffix(' ').unwrap_or(inner.strip_prefix(' ').unwrap_or(inner)).to_string()
            } else {
                format!("{open} {text} {close}")
            };
            let n = new.chars().count();
            changes.push(Change { start: a, end: b, text: new, sel: (0, n) });
        }
        if !changes.is_empty() {
            self.apply(changes, EditKind::Other);
        }
    }

    /// Accept a completion: replace the typed prefix with `insert`; `$0` marks the final caret.
    pub fn accept_completion(&mut self, prefix_start: usize, insert: &str) {
        let head = self.primary().head;
        // keep the indentation of the current line for multi-line snippets
        let (line, _) = self.char_to_pos(head);
        let indent: String = self.line_string(line).chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let unit = self.indent_unit();
        let mut text = String::new();
        for (i, l) in insert.split('\n').enumerate() {
            if i > 0 {
                text.push('\n');
                text.push_str(&indent);
            }
            text.push_str(&if self.indent.use_spaces { l.to_string() } else { l.replace("    ", "\t") });
        }
        let _ = unit;
        let cursor = text.find("$0");
        let text = text.replacen("$0", "", 1);
        let total = text.chars().count();
        let c = cursor.map(|b| text_char_index(&text, b)).unwrap_or(total);
        self.apply(vec![Change { start: prefix_start, end: head, text, sel: (c, c) }], EditKind::Other);
    }
}

fn text_char_index(s: &str, byte: usize) -> usize {
    s[..byte.min(s.len())].chars().count()
}
