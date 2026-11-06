//! File explorer tree.

use crate::icons::{self, Icon};
use crate::ui_theme::c;
use eframe::egui::{self, Align2, FontId, Key, ScrollArea, Sense, TextEdit, Ui, Vec2};
use fractal_core::theme::Theme;
use notify::{RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

pub enum ExplorerAction {
    Open(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
    Reveal(PathBuf),
    CopyPath(PathBuf),
    OpenInTerminal(PathBuf),
    Toast(String),
    FileCreated(PathBuf),
}

pub struct Explorer {
    pub root: Option<PathBuf>,
    expanded: HashSet<PathBuf>,
    cache: HashMap<PathBuf, Vec<Entry>>,
    pub selected: Option<PathBuf>,
    rename: Option<(PathBuf, String)>,
    creating: Option<(PathBuf, bool, String)>,
    dirty: Arc<AtomicBool>,
    watcher: Option<notify::RecommendedWatcher>,
    focus_edit: bool,
}

fn read_dir(dir: &Path) -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name == ".git" || name == ".DS_Store" {
                        return None;
                    }
                    let is_dir = e.path().is_dir();
                    Some(Entry { path: e.path(), name, is_dir })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

impl Explorer {
    pub fn new() -> Self {
        Self { root: None, expanded: HashSet::new(), cache: HashMap::new(), selected: None, rename: None, creating: None, dirty: Arc::new(AtomicBool::new(false)), watcher: None, focus_edit: false }
    }

    pub fn set_root(&mut self, root: Option<PathBuf>, ctx: &egui::Context) {
        self.root = root.clone();
        self.cache.clear();
        self.expanded.clear();
        self.selected = None;
        self.watcher = None;
        if let Some(r) = root {
            self.expanded.insert(r.clone());
            let dirty = self.dirty.clone();
            let ctx = ctx.clone();
            self.watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(ev) = res {
                    let noisy = ev.paths.iter().all(|p| p.components().any(|c| matches!(c.as_os_str().to_str(), Some(".git") | Some("node_modules") | Some("target"))));
                    if !noisy {
                        dirty.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                    }
                }
            })
            .ok()
            .and_then(|mut w| {
                w.watch(&r, RecursiveMode::Recursive).ok()?;
                Some(w)
            });
        }
    }

    pub fn refresh(&mut self) {
        self.cache.clear();
    }

    pub fn reveal(&mut self, path: &Path) {
        let Some(root) = &self.root else { return };
        let mut p = path.parent();
        while let Some(d) = p {
            if !d.starts_with(root) {
                break;
            }
            self.expanded.insert(d.to_path_buf());
            p = d.parent();
        }
        self.selected = Some(path.to_path_buf());
    }

    pub fn start_create(&mut self, is_dir: bool) {
        let base = match &self.selected {
            Some(p) if p.is_dir() => p.clone(),
            Some(p) => p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| self.root.clone().unwrap()),
            None => match &self.root {
                Some(r) => r.clone(),
                None => return,
            },
        };
        self.expanded.insert(base.clone());
        self.creating = Some((base, is_dir, String::new()));
        self.focus_edit = true;
    }

    fn entries(&mut self, dir: &Path) -> Vec<Entry> {
        self.cache.entry(dir.to_path_buf()).or_insert_with(|| read_dir(dir)).clone()
    }

    fn flatten(&mut self, dir: &Path, depth: usize, out: &mut Vec<(Entry, usize)>) {
        for e in self.entries(dir) {
            let open = e.is_dir && self.expanded.contains(&e.path);
            let p = e.path.clone();
            out.push((e, depth));
            if open {
                self.flatten(&p, depth + 1, out);
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, theme: &Theme, git: &HashMap<PathBuf, char>, active: Option<&Path>) -> Vec<ExplorerAction> {
        let mut actions = vec![];
        if self.dirty.swap(false, Ordering::Relaxed) {
            self.cache.clear();
        }
        let Some(root) = self.root.clone() else { return actions };
        let u = &theme.ui;
        let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        ui.add_space(2.0);
        let mut rows: Vec<(Entry, usize)> = vec![];
        self.flatten(&root, 0, &mut rows);
        // insert create row
        let mut create_at: Option<(usize, usize)> = None;
        if let Some((base, _, _)) = &self.creating {
            let depth = rows.iter().find(|(e, _)| &e.path == base).map(|(_, d)| d + 1).unwrap_or(0);
            let idx = rows.iter().position(|(e, _)| &e.path == base).map(|i| {
                // after the last descendant
                let mut j = i + 1;
                while j < rows.len() && rows[j].0.path.starts_with(base) {
                    j += 1;
                }
                j
            }).unwrap_or(0);
            create_at = Some((idx, depth));
        }
        let row_h = 27.0;
        let total = rows.len() + create_at.is_some() as usize;
        let mut toggle: Option<PathBuf> = None;
        let mut open: Option<PathBuf> = None;
        let mut select: Option<PathBuf> = None;
        let mut commit_create: Option<(PathBuf, bool, String)> = None;
        let mut cancel_create = false;
        let mut commit_rename: Option<(PathBuf, String)> = None;
        let mut cancel_rename = false;
        let mut start_rename: Option<(PathBuf, String)> = None;
        let mut delete: Option<PathBuf> = None;
        let mut new_in: Option<(PathBuf, bool)> = None;
        ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, total, |ui, range| {
            for vi in range {
                // map virtual index to row / create row
                let (is_create, ri) = match create_at {
                    Some((ci, _)) if vi == ci => (true, 0),
                    Some((ci, _)) if vi > ci => (false, vi - 1),
                    _ => (false, vi),
                };
                let (rect, resp);
                if is_create {
                    let depth = create_at.unwrap().1;
                    let (r, rr) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::hover());
                    rect = r;
                    resp = rr;
                    let inner = egui::Rect::from_min_max(egui::pos2(rect.left() + 18.0 + depth as f32 * 14.0 + 18.0, rect.top() + 1.0), egui::pos2(rect.right() - 10.0, rect.bottom() - 1.0));
                    let (_, is_dir, buf) = self.creating.as_mut().unwrap();
                    let te = ui.put(inner, TextEdit::singleline(buf).hint_text(if *is_dir { "folder name" } else { "file name" }).lock_focus(true));
                    if self.focus_edit {
                        te.request_focus();
                        self.focus_edit = false;
                    }
                    if te.lost_focus() {
                        if ui.input(|i| i.key_pressed(Key::Enter)) && !buf.trim().is_empty() {
                            let (b, d, n) = self.creating.clone().unwrap();
                            commit_create = Some((b, d, n));
                        } else {
                            cancel_create = true;
                        }
                    }
                    let _ = resp;
                    continue;
                }
                let (entry, depth) = &rows[ri];
                let (r, rr) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                rect = r;
                resp = rr;
                let is_sel = self.selected.as_ref() == Some(&entry.path);
                let is_active = active == Some(entry.path.as_path());
                let hl = egui::Rect::from_min_max(egui::pos2(rect.left() + 8.0, rect.top() + 1.0), egui::pos2(rect.right() - 8.0, rect.bottom() - 1.0));
                if is_active {
                    ui.painter().rect_filled(hl, 7.0, crate::shell::alpha(u.accent, 34));
                } else if is_sel {
                    ui.painter().rect_filled(hl, 7.0, crate::shell::alpha(u.fg, 16));
                } else if resp.hovered() {
                    ui.painter().rect_filled(hl, 7.0, crate::shell::alpha(u.fg, 9));
                }
                let x0 = rect.left() + 18.0 + *depth as f32 * 14.0;
                let cy = rect.center().y;
                // rename inline
                if let Some((rp, text)) = &mut self.rename {
                    if rp == &entry.path {
                        let inner = egui::Rect::from_min_max(egui::pos2(x0 + 18.0, rect.top() + 1.0), egui::pos2(rect.right() - 10.0, rect.bottom() - 1.0));
                        let te = ui.put(inner, TextEdit::singleline(text).lock_focus(true));
                        if self.focus_edit {
                            te.request_focus();
                            self.focus_edit = false;
                        }
                        if te.lost_focus() {
                            if ui.input(|i| i.key_pressed(Key::Enter)) {
                                commit_rename = Some((rp.clone(), text.clone()));
                            } else {
                                cancel_rename = true;
                            }
                        }
                        continue;
                    }
                }
                let is_open = entry.is_dir && self.expanded.contains(&entry.path);
                if entry.is_dir {
                    icons::draw(ui.painter(), if is_open { Icon::ChevronDown } else { Icon::ChevronRight }, egui::pos2(x0 + 5.0, cy), 12.0, c(u.fg_faint));
                }
                let status = git.get(&entry.path).copied();
                let name_col = match status {
                    Some('U') | Some('A') | Some('?') => c(u.git_added),
                    Some('M') => c(u.git_modified),
                    Some('D') => c(u.git_deleted),
                    _ => if entry.is_dir { c(u.fg) } else if is_active { c(u.fg) } else { c(u.fg).gamma_multiply(0.82) },
                };
                ui.painter().text(egui::pos2(x0 + 18.0, cy), Align2::LEFT_CENTER, &entry.name, FontId::proportional(13.0), name_col);
                if let Some(s) = status {
                    let col = match s {
                        'M' => c(u.git_modified),
                        'D' => c(u.git_deleted),
                        _ => c(u.git_added),
                    };
                    ui.painter().text(egui::pos2(rect.right() - 20.0, cy), Align2::CENTER_CENTER, if s == '?' { 'U' } else { s }.to_string(), FontId::proportional(11.0), col);
                }
                if resp.clicked() {
                    select = Some(entry.path.clone());
                    if entry.is_dir {
                        toggle = Some(entry.path.clone());
                    } else {
                        open = Some(entry.path.clone());
                    }
                }
                let entry_c = entry.clone();
                resp.context_menu(|ui| {
                    let dir_target = if entry_c.is_dir { entry_c.path.clone() } else { entry_c.path.parent().unwrap().to_path_buf() };
                    if ui.button("New File").clicked() {
                        new_in = Some((dir_target.clone(), false));
                        ui.close();
                    }
                    if ui.button("New Folder").clicked() {
                        new_in = Some((dir_target.clone(), true));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Rename").clicked() {
                        start_rename = Some((entry_c.path.clone(), entry_c.name.clone()));
                        ui.close();
                    }
                    if ui.button("Move to Trash").clicked() {
                        delete = Some(entry_c.path.clone());
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Reveal in Finder").clicked() {
                        actions.push(ExplorerAction::Reveal(entry_c.path.clone()));
                        ui.close();
                    }
                    if ui.button("Open in Terminal").clicked() {
                        actions.push(ExplorerAction::OpenInTerminal(dir_target.clone()));
                        ui.close();
                    }
                    if ui.button("Copy Path").clicked() {
                        actions.push(ExplorerAction::CopyPath(entry_c.path.clone()));
                        ui.close();
                    }
                });
            }
        });

        if let Some(p) = select {
            self.selected = Some(p);
        }
        if let Some(p) = toggle {
            if !self.expanded.remove(&p) {
                self.expanded.insert(p);
            }
        }
        if let Some(p) = open {
            actions.push(ExplorerAction::Open(p));
        }
        if let Some((p, d)) = new_in {
            self.selected = Some(p.clone());
            self.start_create(d);
        }
        if let Some((p, n)) = start_rename {
            self.rename = Some((p, n));
            self.focus_edit = true;
        }
        if cancel_create {
            self.creating = None;
        }
        if cancel_rename {
            self.rename = None;
        }
        if let Some((base, is_dir, name)) = commit_create {
            self.creating = None;
            let target = base.join(name.trim());
            let res = if is_dir { std::fs::create_dir_all(&target) } else { target.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::File::create(&target).map(|_| ())) };
            match res {
                Ok(()) => {
                    self.cache.clear();
                    self.selected = Some(target.clone());
                    if !is_dir {
                        actions.push(ExplorerAction::FileCreated(target));
                    }
                }
                Err(e) => actions.push(ExplorerAction::Toast(format!("Could not create: {e}"))),
            }
        }
        if let Some((from, name)) = commit_rename {
            self.rename = None;
            let to = from.parent().unwrap().join(name.trim());
            if to != from {
                match std::fs::rename(&from, &to) {
                    Ok(()) => {
                        self.cache.clear();
                        actions.push(ExplorerAction::Renamed(from, to));
                    }
                    Err(e) => actions.push(ExplorerAction::Toast(format!("Rename failed: {e}"))),
                }
            }
        }
        if let Some(p) = delete {
            match trash::delete(&p) {
                Ok(()) => {
                    self.cache.clear();
                    actions.push(ExplorerAction::Deleted(p));
                }
                Err(e) => actions.push(ExplorerAction::Toast(format!("Could not move to Trash: {e}"))),
            }
        }
        actions
    }
}
