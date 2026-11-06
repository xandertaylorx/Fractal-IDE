//! Project-wide search & replace sidebar.

use crate::icons::{self, Icon};
use crate::ui_theme::c;
use crate::widgets::{ghost_button, section_header, small_toggle};
use eframe::egui::{self, Align2, FontId, Key, ScrollArea, Sense, TextEdit, Ui, Vec2};
use fractal_core::fsutil::{search_in_files, SearchHit};
use fractal_core::search::SearchQuery;
use fractal_core::theme::Theme;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

pub struct SearchPanel {
    pub query: SearchQuery,
    pub replace: String,
    pub show_replace: bool,
    pub hits: Vec<SearchHit>,
    rx: Option<Receiver<SearchHit>>,
    cancel: Arc<AtomicBool>,
    pub running: bool,
    collapsed: HashSet<PathBuf>,
    pub focus_input: bool,
    last_query: SearchQuery,
}

pub enum SearchAction {
    Open { path: PathBuf, line: usize, c0: usize, c1: usize },
    ReplaceAll,
    None,
}

impl SearchPanel {
    pub fn new() -> Self {
        Self {
            query: SearchQuery::default(),
            replace: String::new(),
            show_replace: false,
            hits: vec![],
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            running: false,
            collapsed: HashSet::new(),
            focus_input: false,
            last_query: SearchQuery::default(),
        }
    }

    pub fn start(&mut self, ctx: &egui::Context, root: &Path) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.hits.clear();
        if self.query.text.is_empty() {
            self.running = false;
            self.rx = None;
            return;
        }
        let (tx, rx) = channel();
        let q = self.query.clone();
        let root = root.to_path_buf();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        self.running = true;
        self.rx = Some(rx);
        std::thread::spawn(move || {
            search_in_files(&root, &q, 5000, cancel, |h| {
                let _ = tx.send(h);
            });
            ctx.request_repaint();
        });
    }

    /// Replace every match on disk. Returns number of files changed.
    pub fn replace_all(&mut self, root: &Path) -> usize {
        let Some(re) = self.query.compile() else { return 0 };
        let files: HashSet<PathBuf> = self.hits.iter().map(|h| h.path.clone()).collect();
        let mut n = 0;
        for f in files {
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            let new = if self.query.regex { re.replace_all(&text, self.replace.as_str()).to_string() } else { re.replace_all(&text, regex::NoExpand(self.replace.as_str())).to_string() };
            if new != text && std::fs::write(&f, new).is_ok() {
                n += 1;
            }
        }
        let _ = root;
        n
    }

    pub fn show(&mut self, ui: &mut Ui, theme: &Theme, root: Option<&Path>) -> SearchAction {
        let u = &theme.ui;
        let mut action = SearchAction::None;
        if let Some(rx) = &self.rx {
            let mut got = false;
            while let Ok(h) = rx.try_recv() {
                self.hits.push(h);
                got = true;
            }
            if got {
                ui.ctx().request_repaint();
            }
            if self.cancel.load(Ordering::Relaxed) || (!got && Arc::strong_count(&self.cancel) == 1) {
                self.running = false;
            }
        }
        section_header(ui, theme, "Search");
        let Some(root) = root else {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Open a folder to search.").color(c(u.fg_dim)));
            return action;
        };
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
            let mut changed = false;
            ui.horizontal(|ui| {
                let r = ui.add_sized([ui.available_width() - 74.0, 26.0], TextEdit::singleline(&mut self.query.text).hint_text("Search").lock_focus(true));
                if self.focus_input {
                    r.request_focus();
                    self.focus_input = false;
                }
                changed |= r.changed();
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    changed = true;
                    r.request_focus();
                }
                changed |= small_toggle(ui, theme, Icon::CaseSensitive, "Match Case", &mut self.query.case_sensitive).clicked();
                changed |= small_toggle(ui, theme, Icon::WholeWord, "Whole Word", &mut self.query.whole_word).clicked();
                changed |= small_toggle(ui, theme, Icon::Regex, "Regex", &mut self.query.regex).clicked();
            });
            ui.horizontal(|ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                icons::draw(ui.painter(), if self.show_replace { Icon::ChevronDown } else { Icon::ChevronRight }, rect.center(), 12.0, c(u.fg_dim));
                if resp.clicked() {
                    self.show_replace = !self.show_replace;
                }
                if self.show_replace {
                    ui.add_sized([ui.available_width() - 4.0, 26.0], TextEdit::singleline(&mut self.replace).hint_text("Replace"));
                }
            });
            if self.show_replace && !self.hits.is_empty() && ghost_button(ui, theme, &format!("Replace all in {} files", self.hits.iter().map(|h| &h.path).collect::<HashSet<_>>().len())).clicked() {
                action = SearchAction::ReplaceAll;
            }
            if changed && self.query != self.last_query {
                self.last_query = self.query.clone();
                self.start(ui.ctx(), root);
            }
        });
        ui.add_space(4.0);
        if !self.query.text.is_empty() {
            let files = self.hits.iter().map(|h| &h.path).collect::<HashSet<_>>().len();
            let msg = if self.hits.is_empty() && !self.running { "No results".to_string() } else { format!("{} results in {} files{}", self.hits.len(), files, if self.running { " …" } else { "" }) };
            ui.label(egui::RichText::new(msg).size(11.5).color(c(u.fg_dim)));
        }
        // grouped list
        let mut groups: Vec<(PathBuf, Vec<usize>)> = vec![];
        for (i, h) in self.hits.iter().enumerate() {
            match groups.last_mut() {
                Some((p, v)) if *p == h.path => v.push(i),
                _ => groups.push((h.path.clone(), vec![i])),
            }
        }
        // flatten rows
        enum Row {
            File(usize),
            Hit(usize),
        }
        let mut rows = vec![];
        for (gi, (p, v)) in groups.iter().enumerate() {
            rows.push(Row::File(gi));
            if !self.collapsed.contains(p) {
                for &i in v {
                    rows.push(Row::Hit(i));
                }
            }
        }
        let mut toggle = None;
        ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 22.0, rows.len(), |ui, range| {
            for ri in range {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, 0.0, c(u.bg_hover));
                }
                match &rows[ri] {
                    Row::File(gi) => {
                        let (p, v) = &groups[*gi];
                        let open = !self.collapsed.contains(p);
                        icons::draw(ui.painter(), if open { Icon::ChevronDown } else { Icon::ChevronRight }, egui::pos2(rect.left() + 14.0, rect.center().y), 11.0, c(u.fg_dim));
                        let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy().to_string();
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let dir = rel.strip_suffix(&name).unwrap_or("").trim_end_matches('/').to_string();
                        icons::draw_file(ui.painter(), icons::file_icon_key(p, false, false), egui::pos2(rect.left() + 32.0, rect.center().y), 15.0);
                        let g = ui.painter().text(egui::pos2(rect.left() + 46.0, rect.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.0), c(u.fg));
                        ui.painter().text(egui::pos2(g.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, dir, FontId::proportional(11.5), c(u.fg_faint));
                        ui.painter().text(egui::pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, v.len().to_string(), FontId::proportional(11.0), c(u.fg_dim));
                        if resp.clicked() {
                            toggle = Some(p.clone());
                        }
                    }
                    Row::Hit(i) => {
                        let h = &self.hits[*i];
                        let preview = h.preview.trim_start();
                        let trim = h.preview.chars().count() - preview.chars().count();
                        let s = h.col_start.saturating_sub(trim);
                        let chars: Vec<char> = preview.chars().collect();
                        let pre: String = chars.iter().take(s).skip(s.saturating_sub(40)).collect();
                        let mid: String = chars.iter().skip(s).take(h.col_end - h.col_start).collect();
                        let post: String = chars.iter().skip(s + (h.col_end - h.col_start)).take(80).collect();
                        let font = FontId::monospace(12.0);
                        let mut x = rect.left() + 28.0;
                        let p = ui.painter().with_clip_rect(rect);
                        for (txt, col, bg) in [(pre, c(u.fg_dim), false), (mid, c(u.fg), true), (post, c(u.fg_dim), false)] {
                            let g = p.layout_no_wrap(txt, font.clone(), col);
                            if bg {
                                p.rect_filled(egui::Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), Vec2::new(g.size().x, 16.0)), 2.0, c(u.find_match).gamma_multiply(2.0));
                            }
                            let w = g.size().x;
                            p.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, col);
                            x += w;
                        }
                        if resp.clicked() {
                            action = SearchAction::Open { path: h.path.clone(), line: h.line, c0: h.col_start, c1: h.col_end };
                        }
                    }
                }
            }
        });
        if let Some(p) = toggle {
            if !self.collapsed.remove(&p) {
                self.collapsed.insert(p);
            }
        }
        action
    }
}
