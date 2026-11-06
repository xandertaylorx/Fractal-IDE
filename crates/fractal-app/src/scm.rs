//! Source control sidebar.

use crate::icons::Icon;
use crate::job::Job;
use crate::ui_theme::c;
use crate::widgets::{accent_button, ghost_button, icon_button, section_header};
use eframe::egui::{self, Align2, FontId, ScrollArea, Sense, TextEdit, Ui, Vec2};
use fractal_core::git::{self, GitStatus, LogEntry};
use fractal_core::theme::Theme;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct Scm {
    pub status: Option<GitStatus>,
    pub repo: Option<PathBuf>,
    pub message: String,
    refresh_job: Option<Job<Result<GitStatus, String>>>,
    op_job: Option<(String, Job<Result<String, String>>)>,
    last_refresh: Instant,
    pub log: Vec<LogEntry>,
    log_job: Option<Job<Vec<LogEntry>>>,
    pub want_refresh: bool,
    pub error: Option<String>,
}

pub enum ScmAction {
    Open(PathBuf),
    Diff { path: String, staged: bool },
    Toast(String),
    BranchPicker,
    Refreshed,
}

impl Scm {
    pub fn new() -> Self {
        Self { status: None, repo: None, message: String::new(), refresh_job: None, op_job: None, last_refresh: Instant::now() - Duration::from_secs(60), log: vec![], log_job: None, want_refresh: true, error: None }
    }

    pub fn set_folder(&mut self, folder: Option<&Path>) {
        self.repo = folder.and_then(git::repo_root);
        self.status = None;
        self.log.clear();
        self.want_refresh = true;
    }

    pub fn busy(&self) -> bool {
        self.op_job.is_some()
    }

    /// Poll background jobs; returns true when a fresh status arrived.
    pub fn tick(&mut self, ctx: &egui::Context) -> Vec<ScmAction> {
        let mut out = vec![];
        if let Some(j) = &self.refresh_job {
            if let Some(r) = j.poll() {
                self.refresh_job = None;
                match r {
                    Ok(s) => {
                        self.status = Some(s);
                        self.error = None;
                        out.push(ScmAction::Refreshed);
                    }
                    Err(e) => {
                        self.status = None;
                        self.error = Some(e);
                    }
                }
            }
        }
        if let Some(j) = &self.log_job {
            if let Some(l) = j.poll() {
                self.log = l;
                self.log_job = None;
            }
        }
        if let Some((name, j)) = &self.op_job {
            if let Some(r) = j.poll() {
                let name = name.clone();
                self.op_job = None;
                match r {
                    Ok(_) => {
                        if name == "Commit" {
                            self.message.clear();
                        }
                        if matches!(name.as_str(), "Push" | "Pull" | "Commit" | "Checkout" | "Init") {
                            out.push(ScmAction::Toast(format!("{name} complete")));
                        }
                    }
                    Err(e) => out.push(ScmAction::Toast(format!("{name} failed: {}", e.lines().next().unwrap_or("")))),
                }
                self.want_refresh = true;
            }
        }
        if self.repo.is_some() && self.refresh_job.is_none() && (self.want_refresh || self.last_refresh.elapsed() > Duration::from_secs(4)) {
            self.want_refresh = false;
            self.last_refresh = Instant::now();
            let root = self.repo.clone().unwrap();
            self.refresh_job = Some(Job::spawn(ctx, move || git::status(&root)));
            if self.log_job.is_none() {
                let root = self.repo.clone().unwrap();
                self.log_job = Some(Job::spawn(ctx, move || git::log(&root, 30)));
            }
        }
        ctx.request_repaint_after(Duration::from_secs(2));
        out
    }

    pub fn run_op(&mut self, ctx: &egui::Context, name: &str, f: impl FnOnce(&Path) -> Result<String, String> + Send + 'static) {
        if let Some(root) = self.repo.clone() {
            self.op_job = Some((name.to_string(), Job::spawn(ctx, move || f(&root))));
        }
    }

    pub fn change_count(&self) -> usize {
        self.status.as_ref().map(|s| s.entries.len()).unwrap_or(0)
    }

    pub fn show(&mut self, ui: &mut Ui, theme: &Theme, folder: Option<&Path>) -> Vec<ScmAction> {
        let u = &theme.ui;
        let mut actions = vec![];
        section_header(ui, theme, "Source Control");
        if folder.is_none() {
            ui.label(egui::RichText::new("Open a folder first.").color(c(u.fg_dim)));
            return actions;
        }
        if self.repo.is_none() {
            ui.add_space(6.0);
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 0)).show(ui, |ui| {
                ui.label(egui::RichText::new("This folder isn't a Git repository.").color(c(u.fg_dim)));
                ui.add_space(8.0);
                if accent_button(ui, theme, "Initialize Repository", true).clicked() {
                    if let Some(f) = folder {
                        let _ = git::init(f);
                        self.set_folder(Some(f));
                    }
                }
            });
            return actions;
        }
        let Some(st) = self.status.clone() else {
            ui.label(egui::RichText::new(self.error.clone().unwrap_or_else(|| "Loading…".into())).color(c(u.fg_dim)));
            return actions;
        };
        let root = st.root.clone();
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
            // branch + sync row
            ui.horizontal(|ui| {
                if ghost_button(ui, theme, &format!("⑂ {}", st.branch)).clicked() {
                    actions.push(ScmAction::BranchPicker);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, theme, Icon::Refresh, "Refresh", false, 26.0).clicked() {
                        self.want_refresh = true;
                    }
                    if ghost_button(ui, theme, &format!("Push ↑{}", st.ahead)).clicked() {
                        self.run_op(ui.ctx(), "Push", git::push);
                    }
                    if ghost_button(ui, theme, &format!("Pull ↓{}", st.behind)).clicked() {
                        self.run_op(ui.ctx(), "Pull", git::pull);
                    }
                });
            });
            ui.add_space(4.0);
            let staged_n = st.entries.iter().filter(|e| e.staged()).count();
            ui.add(TextEdit::multiline(&mut self.message).desired_rows(2).desired_width(f32::INFINITY).hint_text("Message (⌘Enter to commit)"));
            ui.add_space(4.0);
            let can_commit = !self.message.trim().is_empty() && (staged_n > 0 || !st.entries.is_empty());
            let enter_commit = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) && can_commit && ui.memory(|m| m.focused().is_some());
            if (accent_button(ui, theme, if staged_n > 0 { "✓ Commit" } else { "✓ Commit All" }, can_commit && !self.busy()).clicked() || enter_commit) && can_commit {
                let msg = self.message.clone();
                let stage_all = staged_n == 0;
                self.run_op(ui.ctx(), "Commit", move |r| {
                    if stage_all {
                        git::stage_all(r)?;
                    }
                    git::commit(r, &msg)
                });
            }
        });
        ui.add_space(6.0);
        let staged: Vec<_> = st.entries.iter().filter(|e| e.staged()).cloned().collect();
        let changes: Vec<_> = st.entries.iter().filter(|e| e.unstaged()).cloned().collect();
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut ops: Vec<(String, bool, bool)> = vec![]; // (path, stage?, discard?)
            for (title, list, is_staged) in [("Staged Changes", &staged, true), ("Changes", &changes, false)] {
                if list.is_empty() {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(format!("{title}  {}", list.len())).size(11.0).strong().color(c(u.fg_dim)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if icon_button(ui, theme, if is_staged { Icon::Minus } else { Icon::Plus }, if is_staged { "Unstage All" } else { "Stage All" }, false, 22.0).clicked() {
                            for e in list.iter() {
                                ops.push((e.path.clone(), !is_staged, false));
                            }
                        }
                    });
                });
                for e in list.iter() {
                    let letter = if is_staged { e.index } else { e.worktree };
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(rect, 0.0, c(u.bg_hover));
                    }
                    let col = match letter {
                        'M' => c(u.git_modified),
                        'D' => c(u.git_deleted),
                        'A' | '?' => c(u.git_added),
                        _ => c(u.warning),
                    };
                    let name = Path::new(&e.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let dir = Path::new(&e.path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    let g = ui.painter().text(egui::pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.0), c(u.fg));
                    ui.painter().text(egui::pos2(g.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, dir, FontId::proportional(11.5), c(u.fg_faint));
                    ui.painter().text(egui::pos2(rect.right() - 12.0, rect.center().y), Align2::CENTER_CENTER, if letter == '?' { 'U' } else { letter }.to_string(), FontId::proportional(11.5), col);
                    if resp.hovered() {
                        let bx = rect.right() - 34.0;
                        let r1 = egui::Rect::from_center_size(egui::pos2(bx, rect.center().y), Vec2::splat(20.0));
                        let r2 = egui::Rect::from_center_size(egui::pos2(bx - 22.0, rect.center().y), Vec2::splat(20.0));
                        let b1 = ui.interact(r1, ui.id().with(("stage", &e.path, is_staged)), Sense::click());
                        crate::icons::draw(ui.painter(), if is_staged { Icon::Minus } else { Icon::Plus }, r1.center(), 12.0, c(u.fg));
                        if b1.on_hover_text(if is_staged { "Unstage" } else { "Stage" }).clicked() {
                            ops.push((e.path.clone(), !is_staged, false));
                        }
                        if !is_staged {
                            let b2 = ui.interact(r2, ui.id().with(("discard", &e.path)), Sense::click());
                            crate::icons::draw(ui.painter(), Icon::Undo, r2.center(), 12.0, c(u.fg));
                            if b2.on_hover_text("Discard Changes").clicked() {
                                ops.push((e.path.clone(), false, true));
                            }
                        }
                    }
                    if resp.clicked() {
                        if e.untracked() || letter == 'A' {
                            actions.push(ScmAction::Open(root.join(&e.path)));
                        } else {
                            actions.push(ScmAction::Diff { path: e.path.clone(), staged: is_staged });
                        }
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Open File").clicked() {
                            actions.push(ScmAction::Open(root.join(&e.path)));
                            ui.close();
                        }
                    });
                }
                ui.add_space(6.0);
            }
            if st.entries.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("   No changes").color(c(u.fg_dim)));
            }
            if !self.log.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("   RECENT COMMITS").size(11.0).strong().color(c(u.fg_dim)));
                for l in &self.log {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::hover());
                    if resp.hovered() {
                        ui.painter().rect_filled(rect, 0.0, c(u.bg_hover));
                    }
                    let p = ui.painter().with_clip_rect(rect);
                    p.text(egui::pos2(rect.left() + 14.0, rect.top() + 11.0), Align2::LEFT_CENTER, &l.subject, FontId::proportional(12.5), c(u.fg));
                    p.text(egui::pos2(rect.left() + 14.0, rect.top() + 27.0), Align2::LEFT_CENTER, format!("{}  ·  {}  ·  {}", l.hash, l.author, l.when), FontId::proportional(11.0), c(u.fg_faint));
                }
            }
            for (path, stage, discard) in ops {
                let pth = path.clone();
                if discard {
                    let untracked = st.entries.iter().any(|e| e.path == path && e.untracked());
                    if rfd::MessageDialog::new().set_title("Discard changes").set_description(format!("Discard changes in {path}? This cannot be undone.")).set_buttons(rfd::MessageButtons::YesNo).show() == rfd::MessageDialogResult::Yes {
                        self.run_op(ui.ctx(), "Discard", move |r| git::discard(r, &pth, untracked).map(|_| String::new()));
                    }
                } else if stage {
                    self.run_op(ui.ctx(), "Stage", move |r| git::stage(r, &pth).map(|_| String::new()));
                } else {
                    self.run_op(ui.ctx(), "Unstage", move |r| git::unstage(r, &pth).map(|_| String::new()));
                }
            }
        });
        actions
    }
}
