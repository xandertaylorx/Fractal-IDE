//! Language-server integration for the app (IntelliSense like VS Code).

use crate::app::{App, Focus, Suggest};
use crate::editor::{self};
use crate::ui_theme::c;
use eframe::egui::{self, Align2, Color32, FontId, Frame, Margin, Pos2, Stroke};
use fractal_core::complete::{Kind, Suggestion};
use fractal_core::lsp::{self, char_to_utf16, utf16_to_char, CompletionItem, LspClient, Missing, Reply};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

pub struct PendingCompletion {
    reply: Reply,
    path: PathBuf,
}

pub struct Cache {
    pub path: PathBuf,
    pub line: usize,
    /// Absolute char index of the word start when the request was made.
    pub anchor: usize,
    pub items: Vec<CompletionItem>,
}

pub struct HoverState {
    pub idx: usize,
    pub pos: Pos2,
    pub text: Option<String>,
    pub diag: Option<(u8, String)>,
    reply: Option<Reply>,
}

#[derive(Default)]
pub struct LspHub {
    pub clients: HashMap<String, LspClient>,
    pub missing: HashMap<String, Missing>,
    failed: HashSet<String>,
    completion: Option<PendingCompletion>,
    pub cache: Option<Cache>,
    pub hover: Option<HoverState>,
    definition: Option<Reply>,
}

impl App {
    fn lsp_key(&self, idx: usize) -> Option<(String, String, PathBuf)> {
        let t = self.tabs.get(idx)?;
        if t.settings_page {
            return None;
        }
        let path = t.buf.path.clone()?;
        let group = lsp::server_group(t.buf.lang.id);
        if group.is_empty() {
            return None;
        }
        let root = self.folder.clone().filter(|f| path.starts_with(f)).or_else(|| path.parent().map(|p| p.to_path_buf()))?;
        Some((group.to_string(), format!("{group}@{}", root.display()), root))
    }

    pub fn lsp_status(&self) -> Option<(String, bool)> {
        let (group, key, _) = self.lsp_key(self.active)?;
        if let Some(c) = self.lsp.clients.get(&key) {
            let n = c.name.replace("-langserver", "").replace("-language-server", "").replace("-lsp", "");
            return Some((n, c.is_ready()));
        }
        self.lsp.missing.get(&group).map(|m| (format!("{} not installed", m.name), false))
    }

    pub fn lsp_missing_hint(&self) -> Option<Missing> {
        let (group, _, _) = self.lsp_key(self.active)?;
        self.lsp.missing.get(&group).cloned()
    }

    pub fn lsp_tick(&mut self, ctx: &egui::Context) {
        // 1. make sure the active tab has a server
        if let Some((group, key, root)) = self.lsp_key(self.active) {
            if !self.lsp.clients.contains_key(&key) && !self.lsp.failed.contains(&key) && !self.lsp.missing.contains_key(&group) {
                match lsp::find_server(&group, Some(&root)) {
                    Ok(spec) => {
                        let cc = ctx.clone();
                        match LspClient::start(&spec.name, &spec.program, &spec.args, &root, move || cc.request_repaint()) {
                            Ok(cl) => {
                                self.lsp.clients.insert(key.clone(), cl);
                            }
                            Err(e) => {
                                self.lsp.failed.insert(key.clone());
                                self.toast(e);
                            }
                        }
                    }
                    Err(Some(m)) => {
                        let msg = format!("No {} language server found. Install it for code intelligence:  {}", group, m.install);
                        self.lsp.missing.insert(group.clone(), m);
                        self.toast(msg);
                    }
                    Err(None) => {}
                }
            }
            // 2. open / sync the active document
            self.lsp_sync_active(false);
        }
        // 3. events from all servers
        let mut diag_updates: Vec<(String, Vec<lsp::Diagnostic>)> = vec![];
        let mut dead: Vec<String> = vec![];
        for (key, cl) in &self.lsp.clients {
            for ev in cl.events.try_iter() {
                match ev {
                    lsp::LspEvent::Diagnostics { uri, items } => diag_updates.push((uri, items)),
                    lsp::LspEvent::Exited => dead.push(key.clone()),
                    lsp::LspEvent::Log(_) => {}
                }
            }
        }
        for k in dead {
            self.lsp.clients.remove(&k);
            self.lsp.failed.insert(k);
        }
        for (uri, items) in diag_updates {
            let Some(path) = lsp::uri_to_path(&uri) else { continue };
            if let Some(t) = self.tabs.iter_mut().find(|t| t.buf.path.as_deref() == Some(path.as_path())) {
                t.diags = items
                    .into_iter()
                    .map(|mut d| {
                        let lc = t.buf.line_count();
                        if d.line < lc {
                            d.col = utf16_to_char(&t.buf.line_string(d.line), d.col);
                        }
                        if d.end_line < lc {
                            d.end_col = utf16_to_char(&t.buf.line_string(d.end_line), d.end_col);
                        }
                        d
                    })
                    .collect();
            }
        }
        // 4. replies
        if let Some(p) = &self.lsp.completion {
            match p.reply.try_recv() {
                Ok(Ok(v)) => {
                    let items = lsp::parse_completion(&v);
                    let path = p.path.clone();
                    self.lsp.completion = None;
                    if let Some(t) = self.tabs.get(self.active) {
                        if t.buf.path.as_deref() == Some(path.as_path()) {
                            let head = t.buf.primary().head;
                            let (line, _) = t.buf.char_to_pos(head);
                            let (_, start) = t.buf.prefix_at_caret();
                            self.lsp.cache = Some(Cache { path, line, anchor: start, items });
                            self.lsp_build_suggest(true);
                        }
                    }
                }
                Ok(Err(_)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => self.lsp.completion = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(h) = &mut self.lsp.hover {
            if let Some(r) = &h.reply {
                match r.try_recv() {
                    Ok(res) => {
                        h.text = res.ok().and_then(|v| lsp::parse_hover(&v));
                        h.reply = None;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => h.reply = None,
                    Err(_) => {}
                }
            }
        }
        if let Some(r) = &self.lsp.definition {
            match r.try_recv() {
                Ok(res) => {
                    self.lsp.definition = None;
                    if let Some(loc) = res.ok().map(|v| lsp::parse_definition(&v)).and_then(|l| l.into_iter().next()) {
                        let text = std::fs::read_to_string(&loc.path).unwrap_or_default();
                        let line_txt = text.lines().nth(loc.line).unwrap_or("");
                        let col = utf16_to_char(line_txt, loc.col);
                        let word_len = line_txt.chars().skip(col).take_while(|c| c.is_alphanumeric() || *c == '_').count();
                        self.open_file(&loc.path, Some((loc.line, col, col + word_len)));
                    } else {
                        self.toast("No definition found");
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.lsp.definition = None,
                Err(_) => {}
            }
        }
    }

    /// Open/sync the active document with its server. `force` skips the debounce.
    pub fn lsp_sync_active(&mut self, force: bool) {
        let Some((_, key, _)) = self.lsp_key(self.active) else { return };
        let Some(cl) = self.lsp.clients.get_mut(&key) else { return };
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let Some(path) = t.buf.path.clone() else { return };
        if !cl.is_open(&path) {
            cl.did_open(&path, t.buf.lang.id, &t.buf.text());
            t.lsp_synced = t.buf.version;
        } else if t.lsp_synced != t.buf.version && (force || t.last_edit.elapsed() > Duration::from_millis(120)) {
            cl.did_change(&path, &t.buf.text());
            t.lsp_synced = t.buf.version;
        }
    }

    pub fn lsp_did_close(&mut self, path: &std::path::Path) {
        for cl in self.lsp.clients.values_mut() {
            if cl.is_open(path) {
                cl.did_close(path);
            }
        }
    }

    pub fn lsp_did_save(&mut self, path: &std::path::Path) {
        for cl in self.lsp.clients.values_mut() {
            if cl.is_open(path) {
                cl.did_save(path);
            }
        }
    }

    fn has_lsp_for_active(&self) -> bool {
        self.lsp_key(self.active).map_or(false, |(_, k, _)| self.lsp.clients.contains_key(&k))
    }

    /// Called after the user typed (Some(c)) or deleted (None).
    pub fn after_typing(&mut self, ch: Option<char>) {
        if !self.settings.suggestions || self.focus != Focus::Editor {
            self.suggest = None;
            return;
        }
        if !self.has_lsp_for_active() {
            match ch {
                Some(c) if !(c.is_alphanumeric() || c == '_') => self.suggest = None,
                _ => self.compute_suggest(2),
            }
            return;
        }
        let key = self.lsp_key(self.active).map(|k| k.1).unwrap();
        let trig: Vec<char> = self.lsp.clients.get(&key).map(|c| c.trigger_chars()).unwrap_or_default();
        let is_trigger = ch.map_or(false, |c| trig.contains(&c) || c == '.');
        let is_ident = ch.map_or(true, |c| c.is_alphanumeric() || c == '_');
        if !is_ident && !is_trigger {
            self.suggest = None;
            self.lsp.cache = None;
            return;
        }
        // is the cache still valid for this word?
        let valid = self.tabs.get(self.active).map_or(false, |t| {
            let head = t.buf.primary().head;
            let (line, _) = t.buf.char_to_pos(head);
            let (_, start) = t.buf.prefix_at_caret();
            self.lsp.cache.as_ref().map_or(false, |c| c.path.as_deref_eq(&t.buf.path) && c.line == line && c.anchor == start)
        });
        if valid && !is_trigger {
            if !self.lsp_build_suggest(ch.is_none()) {
                self.suggest = None;
            }
            return;
        }
        // a short word start (1+ chars) or a trigger char → ask the server
        let prefix_len = self.tabs.get(self.active).map_or(0, |t| t.buf.prefix_at_caret().0.chars().count());
        if is_trigger || prefix_len >= 1 {
            self.lsp_request_completion(if is_trigger { ch } else { None });
        } else {
            self.suggest = None;
        }
    }

    pub fn lsp_request_completion(&mut self, trigger: Option<char>) {
        let Some((_, key, _)) = self.lsp_key(self.active) else { return };
        self.lsp_sync_active(true);
        let Some(t) = self.tabs.get(self.active) else { return };
        let Some(path) = t.buf.path.clone() else { return };
        let head = t.buf.primary().head;
        let (line, col) = t.buf.char_to_pos(head);
        let col16 = char_to_utf16(&t.buf.line_string(line), col);
        if let Some(cl) = self.lsp.clients.get(&key) {
            let reply = cl.completion(&path, line, col16, trigger);
            self.lsp.completion = Some(PendingCompletion { reply, path });
        }
    }

    /// Filter + rank cached server items for the current word. Returns false if nothing matches.
    pub fn lsp_build_suggest(&mut self, allow_empty: bool) -> bool {
        let Some(cache) = &self.lsp.cache else { return false };
        let Some(t) = self.tabs.get(self.active) else { return false };
        let (prefix, start) = t.buf.prefix_at_caret();
        if prefix.is_empty() && !allow_empty {
            return false;
        }
        let lp = prefix.to_lowercase();
        let mut scored: Vec<(i32, &CompletionItem)> = vec![];
        for it in &cache.items {
            let ft = it.filter_text.to_lowercase();
            let score = if lp.is_empty() {
                1
            } else if it.filter_text.starts_with(&prefix) {
                4
            } else if ft.starts_with(&lp) {
                3
            } else if ft.contains(&lp) {
                2
            } else if initials_match(&it.filter_text, &prefix) {
                1
            } else {
                continue;
            };
            scored.push((score, it));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.sort_text.cmp(&b.1.sort_text)).then(a.1.label.cmp(&b.1.label)));
        let line_txt = t.buf.line_string(cache.line);
        let items: Vec<Suggestion> = scored
            .into_iter()
            .take(100)
            .map(|(_, it)| {
                let edit_start = it.edit.as_ref().map(|e| t.buf.pos_to_char(e.line, utf16_to_char(&t.buf.line_string(e.line), e.col)));
                let raw = it.edit.as_ref().map(|e| e.new_text.clone()).unwrap_or_else(|| it.insert_text.clone());
                let insert = if it.is_snippet {
                    let (text, caret) = lsp::snippet_to_text(&raw);
                    match caret {
                        Some(c) => {
                            let mut out: String = text.chars().take(c).collect();
                            out.push_str("$0");
                            out.extend(text.chars().skip(c));
                            out
                        }
                        None => text,
                    }
                } else {
                    raw
                };
                Suggestion { label: it.label.clone(), insert, kind: Kind::Lsp, detail: it.detail.clone(), lsp_kind: it.kind, deprecated: it.deprecated, edit_start }
            })
            .collect();
        let _ = line_txt;
        if items.is_empty() {
            return false;
        }
        self.suggest = Some(Suggest { items, selected: 0, start, prefix });
        true
    }

    pub fn lsp_hover_update(&mut self, hover: Option<(usize, Pos2)>) {
        let Some((idx, pos)) = hover else {
            self.lsp.hover = None;
            return;
        };
        if self.lsp.hover.as_ref().map_or(false, |h| h.idx == idx) {
            return;
        }
        let Some(t) = self.tabs.get(self.active) else { return };
        let (line, col) = t.buf.char_to_pos(idx);
        let diag = t.diags.iter().find(|d| (d.line, d.col) <= (line, col) && (line, col) < (d.end_line, d.end_col.max(d.col + 1))).map(|d| (d.severity, d.message.clone()));
        let mut reply = None;
        if let (Some((_, key, _)), Some(path)) = (self.lsp_key(self.active), t.buf.path.clone()) {
            if let Some(cl) = self.lsp.clients.get(&key) {
                if cl.is_ready() {
                    let col16 = char_to_utf16(&t.buf.line_string(line), col);
                    reply = Some(cl.hover(&path, line, col16));
                }
            }
        }
        self.lsp.hover = Some(HoverState { idx, pos, text: None, diag, reply });
    }

    pub fn lsp_definition(&mut self, idx: usize) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let Some(path) = t.buf.path.clone() else { return };
        let (line, col) = t.buf.char_to_pos(idx);
        let col16 = char_to_utf16(&t.buf.line_string(line), col);
        if let Some((_, key, _)) = self.lsp_key(self.active) {
            if let Some(cl) = self.lsp.clients.get(&key) {
                self.lsp.definition = Some(cl.definition(&path, line, col16));
                return;
            }
        }
        self.toast("No language server for this file");
    }

    pub fn draw_hover(&mut self, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        let Some(h) = &self.lsp.hover else { return };
        if h.text.is_none() && h.diag.is_none() {
            return;
        }
        let pos = h.pos + egui::vec2(8.0, 18.0);
        let text = h.text.clone();
        let diag = h.diag.clone();
        egui::Area::new(egui::Id::new("lsp_hover")).order(egui::Order::Tooltip).fixed_pos(pos).interactable(false).show(ctx, |ui| {
            Frame::NONE
                .fill(c(u.bg_input))
                .stroke(Stroke::new(1.0, c(u.border)))
                .corner_radius(egui::CornerRadius::same(6))
                .shadow(egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(if t.dark { 120 } else { 35 }) })
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_max_width(560.0);
                    if let Some((sev, msg)) = &diag {
                        let col = if *sev == 1 { c(u.error) } else if *sev == 2 { c(u.warning) } else { c(u.accent) };
                        ui.label(egui::RichText::new(msg).size(12.5).color(col));
                        if text.is_some() {
                            ui.add_space(6.0);
                        }
                    }
                    if let Some(txt) = &text {
                        let mut shown = 0;
                        for l in txt.lines() {
                            if shown >= 14 {
                                ui.label(egui::RichText::new("…").color(c(u.fg_dim)));
                                break;
                            }
                            let first = shown == 0;
                            ui.label(egui::RichText::new(l).size(12.5).font(if first { FontId::monospace(12.5) } else { FontId::proportional(12.5) }).color(if first { c(u.fg) } else { c(u.fg_dim) }));
                            shown += 1;
                        }
                    }
                });
        });
        let _ = Align2::LEFT_TOP;
        let _ = editor::Tab::title;
    }
}

/// `gupd` → getUserProfileData / get_user_profile_data (word initials), not loose subsequences.
fn initials_match(label: &str, prefix: &str) -> bool {
    if prefix.chars().count() < 2 {
        return false;
    }
    let mut initials = String::new();
    let mut prev_lower = false;
    let mut at_start = true;
    for ch in label.chars() {
        if ch == '_' || ch == '-' || ch == '.' {
            at_start = true;
            prev_lower = false;
            continue;
        }
        if at_start || (ch.is_uppercase() && prev_lower) {
            initials.extend(ch.to_lowercase());
        }
        at_start = false;
        prev_lower = ch.is_lowercase();
    }
    initials.starts_with(&prefix.to_lowercase())
}

trait OptPathEq {
    fn as_deref_eq(&self, other: &Option<PathBuf>) -> bool;
}
impl OptPathEq for PathBuf {
    fn as_deref_eq(&self, other: &Option<PathBuf>) -> bool {
        other.as_ref() == Some(self)
    }
}
