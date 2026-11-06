//! The window shell — Fractal's own look.
//!
//! Principles: floating rounded panels on a darker base, text over icons,
//! no hairline borders, one signal accent, Geist type, a 6px rhythm.

use crate::app::{App, Focus, SideView};
use crate::editor::{self, EditorEnv};
use crate::icons::{self, Icon};
use crate::overlay::Kind;
use crate::ui_theme::{c, medium, semibold};
use crate::widgets::ghost_button;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Frame, Margin, Panel, Rect, Sense, Ui, UiBuilder, Vec2, ViewportCommand};
use std::time::{Duration, Instant};

pub const GAP: i8 = 3;
pub const RADIUS: u8 = 12;

pub fn alpha(col: [u8; 4], a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(col[0], col[1], col[2], a)
}

/// A floating rounded panel filling the available space.
pub fn island<R>(ui: &mut Ui, fill: Color32, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(RADIUS))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            add(ui)
        })
        .inner
}

fn txt(ui: &Ui, text: &str, size: f32, family: egui::FontFamily, color: Color32) -> std::sync::Arc<egui::Galley> {
    ui.painter().layout_no_wrap(text.to_string(), FontId::new(size, family), color)
}

impl App {
    // ------------------------------------------------------------------
    // Title bar
    // ------------------------------------------------------------------
    pub fn draw_titlebar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::top("titlebar").show_separator_line(false).exact_size(46.0).frame(Frame::new().fill(c(u.bg_base))).show(ui, |ui| {
            let rect = ui.max_rect();
            let drag = ui.interact(rect, egui::Id::new("title_drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
            }
            let cy = rect.center().y;
            let mut x = rect.left() + if self.mac { 84.0 } else { 16.0 };

            // sidebar toggle
            let r = Rect::from_center_size(egui::pos2(x + 12.0, cy), Vec2::splat(28.0));
            let resp = ui.interact(r, egui::Id::new("tb_sidebar"), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 7.0, alpha(u.fg, 14));
            }
            icons::draw(ui.painter(), Icon::Sidebar, r.center(), 16.0, if self.sidebar_visible { c(u.fg) } else { c(u.fg_dim) });
            if resp.on_hover_text("Toggle sidebar  (⌘B)").clicked() {
                self.sidebar_visible = !self.sidebar_visible;
            }
            x += 38.0;

            // project · branch
            let name = self.folder.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Fractal".into());
            let g = txt(ui, &name, 13.0, medium(), c(u.fg));
            let nr = Rect::from_min_size(egui::pos2(x, cy - g.size().y / 2.0), g.size());
            let nresp = ui.interact(nr.expand2(Vec2::new(6.0, 6.0)), egui::Id::new("tb_project"), Sense::click());
            ui.painter().galley(nr.min, g.clone(), c(u.fg));
            if nresp.on_hover_text("Search files  (⌘P)").clicked() {
                self.open_overlay(Kind::QuickOpen, "");
            }
            x += g.size().x + 10.0;
            if let Some(st) = &self.scm.status {
                let b = txt(ui, &st.branch, 12.0, egui::FontFamily::Proportional, c(u.fg_faint));
                ui.painter().galley(egui::pos2(x, cy - b.size().y / 2.0), b, c(u.fg_faint));
            }

            // ---- right cluster, laid out from the right edge ----
            let mut rx = rect.right() - 16.0;
            let spec_idx = self.preferred_run();
            let spec = spec_idx.map(|i| self.run_specs[i].clone());
            let can_run = spec.is_some();
            let running_live = self.live.is_some();
            let run_term_alive = self.run_term.map_or(false, |i| self.terms.get(i).map_or(false, |t| t.alive.load(std::sync::atomic::Ordering::Relaxed)));

            let pill_h = 28.0;
            let pill = Rect::from_min_size(egui::pos2(rx - 92.0, cy - pill_h / 2.0), Vec2::new(92.0, pill_h));
            let main = Rect::from_min_max(pill.min, egui::pos2(pill.right() - 26.0, pill.bottom()));
            let dd = Rect::from_min_max(egui::pos2(pill.right() - 26.0, pill.top()), pill.max);
            let r_main = ui.interact(main, egui::Id::new("run_main"), Sense::click());
            let r_dd = ui.interact(dd, egui::Id::new("run_dd"), Sense::click());
            let hot = r_main.hovered() || r_dd.hovered();
            let (fill_a, fg) = if can_run { (if hot { 78 } else { 46 }, c(u.accent_text())) } else { (16, c(u.fg_faint)) };
            ui.painter().rect_filled(pill, 8.0, alpha(u.accent, fill_a));
            icons::draw(ui.painter(), Icon::Play, egui::pos2(main.left() + 17.0, cy), 12.0, fg);
            ui.painter().text(egui::pos2(main.left() + 30.0, cy), Align2::LEFT_CENTER, "Run", FontId::new(12.5, medium()), fg);
            icons::draw(ui.painter(), Icon::ChevronDown, dd.center(), 11.0, fg);
            let tip = spec.as_ref().map(|s| format!("{}  ·  {}   (F5)", s.label, s.detail)).unwrap_or_else(|| "Nothing to run — open a file or project".into());
            if r_main.on_hover_text(tip).clicked() {
                self.exec(ctx, "fractal.run");
            }
            if r_dd.on_hover_text("Choose what to run").clicked() {
                self.exec(ctx, "fractal.runPicker");
            }
            rx = pill.left() - 10.0;

            if running_live || run_term_alive {
                let sr = Rect::from_min_size(egui::pos2(rx - 64.0, cy - pill_h / 2.0), Vec2::new(64.0, pill_h));
                let sresp = ui.interact(sr, egui::Id::new("tb_stop"), Sense::click());
                ui.painter().rect_filled(sr, 8.0, alpha(u.error, if sresp.hovered() { 70 } else { 40 }));
                icons::draw(ui.painter(), Icon::Stop, egui::pos2(sr.left() + 16.0, cy), 11.0, c(u.error));
                ui.painter().text(egui::pos2(sr.left() + 28.0, cy), Align2::LEFT_CENTER, "Stop", FontId::new(12.5, medium()), c(u.error));
                if sresp.on_hover_text("Stop  (⇧⌘R)").clicked() {
                    self.stop_run();
                }
                rx = sr.left() - 10.0;
            }
            if running_live {
                let port = self.live.as_ref().map(|l| l.port).unwrap_or(0);
                let g = txt(ui, &format!("Live :{port}"), 12.0, egui::FontFamily::Proportional, c(u.git_added));
                ui.painter().galley(egui::pos2(rx - g.size().x, cy - g.size().y / 2.0), g.clone(), c(u.git_added));
                rx -= g.size().x + 14.0;
            }
            for (icon, tip, id, on) in [(Icon::Gear, "Settings  (right-click for settings.json)", "tb_gear", false), (Icon::Terminal, "Terminal  (⌃`)", "tb_term", self.panel_visible)] {
                let r = Rect::from_center_size(egui::pos2(rx - 14.0, cy), Vec2::splat(28.0));
                let resp = ui.interact(r, egui::Id::new(id), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 7.0, alpha(u.fg, 14));
                }
                icons::draw(ui.painter(), icon, r.center(), 16.0, if on || resp.hovered() { c(u.fg) } else { c(u.fg_dim) });
                let resp = resp.on_hover_text(tip);
                if id == "tb_gear" {
                    if resp.clicked() {
                        self.exec(ctx, "workbench.action.openSettings");
                    } else if resp.secondary_clicked() {
                        self.exec(ctx, "workbench.action.openSettingsJson");
                    }
                } else if resp.clicked() {
                    self.exec(ctx, "workbench.action.terminal.toggleTerminal");
                }
                rx -= 32.0;
            }
        });
    }

    // ------------------------------------------------------------------
    // Status bar — text only
    // ------------------------------------------------------------------
    pub fn draw_statusbar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::bottom("statusbar").show_separator_line(false).exact_size(28.0).frame(Frame::new().fill(c(u.bg_base))).show(ui, |ui| {
            let rect = ui.max_rect();
            let cy = rect.center().y;
            let font = FontId::proportional(11.5);
            let dim = c(u.fg_faint);
            let hot = c(u.fg);
            let item = |ui: &mut Ui, x: &mut f32, icon: Option<Icon>, text: &str, col: Color32, tip: &str, id: &str| -> bool {
                let g = ui.painter().layout_no_wrap(text.to_string(), font.clone(), col);
                let iw = if icon.is_some() { 15.0 } else { 0.0 };
                let r = Rect::from_min_size(egui::pos2(*x - 3.0, cy - 10.0), Vec2::new(g.size().x + iw + 6.0, 20.0));
                let resp = ui.interact(r, egui::Id::new(("status", id)), Sense::click());
                let colr = if resp.hovered() { hot } else { col };
                if let Some(ic) = icon {
                    icons::draw(ui.painter(), ic, egui::pos2(*x + 5.0, cy), 12.0, colr);
                }
                let g = ui.painter().layout_no_wrap(text.to_string(), font.clone(), colr);
                ui.painter().galley(egui::pos2(*x + iw, cy - g.size().y / 2.0), g.clone(), colr);
                *x += g.size().x + iw + 16.0;
                resp.on_hover_text(tip).clicked()
            };
            let mut x = rect.left() + 18.0;
            if let Some(st) = self.scm.status.clone() {
                let mut s = st.branch.clone();
                if st.ahead > 0 || st.behind > 0 {
                    s.push_str(&format!("  ↓{} ↑{}", st.behind, st.ahead));
                }
                if item(ui, &mut x, Some(Icon::Branch), &s, dim, "Switch branch", "branch") {
                    self.open_overlay(Kind::Branch, "");
                }
                let n = st.entries.len();
                if n > 0 && item(ui, &mut x, None, &format!("{n} changed"), dim, "Uncommitted changes", "changes") {
                    self.show_side(SideView::Scm);
                }
            }
            if self.scm.busy() {
                item(ui, &mut x, None, "working…", dim, "Git operation in progress", "busy");
            }
            if self.settings.auto_save {
                item(ui, &mut x, None, "Auto Save", dim, "Settings → Auto Save", "autosave");
            }
            if let Some(tab) = self.tab().filter(|t| !t.settings_page) {
                let errs = tab.diags.iter().filter(|d| d.severity == 1).count();
                let warns = tab.diags.iter().filter(|d| d.severity == 2).count();
                if errs > 0 {
                    item(ui, &mut x, Some(Icon::Error), &errs.to_string(), c(u.error), "Errors in this file", "errs");
                }
                if warns > 0 {
                    item(ui, &mut x, Some(Icon::Warning), &warns.to_string(), c(u.warning), "Warnings in this file", "warns");
                }
            }
            // right side (laid out right → left)
            let lsp_part = self.lsp_status();
            let mut parts: Vec<(String, &str, &str)> = vec![];
            if let Some(tab) = self.tab().filter(|t| !t.settings_page) {
                let (l, cc) = tab.buf.char_to_pos(tab.buf.primary().head);
                let line_txt = tab.buf.line_string(l);
                let vis = editor::char_col_to_vis(&line_txt, cc, tab.buf.indent.size) + 1;
                let sel = tab.buf.primary();
                let mut pos = format!("Ln {}, Col {}", l + 1, vis);
                if !sel.is_empty() {
                    pos.push_str(&format!("  ({} selected)", sel.end() - sel.start()));
                }
                if tab.buf.sels.len() > 1 {
                    pos.push_str(&format!("  ·  {} cursors", tab.buf.sels.len()));
                }
                parts.push((pos, "Go to line  (⌃G)", "pos"));
                parts.push((if tab.buf.indent.use_spaces { format!("Spaces {}", tab.buf.indent.size) } else { "Tabs".to_string() }, "Indentation", "indent"));
                if tab.buf.crlf {
                    parts.push(("CRLF".into(), "Line ending", "eol"));
                }
                if let Some((n, ready)) = &lsp_part {
                    if n.contains("not installed") {
                        parts.push((format!("{n} — copy install command"), "Language server", "lspinstall"));
                    } else {
                        parts.push((if *ready { n.clone() } else { format!("{n} starting…") }, "Language server", "lsp"));
                    }
                }
                parts.push((tab.hl.syntax_name().to_string(), "Language", "lang"));
            }
            let mut rx = rect.right() - 18.0;
            for (text, tip, id) in parts.into_iter().rev() {
                let g = ui.painter().layout_no_wrap(text.clone(), font.clone(), dim);
                rx -= g.size().x;
                let r = Rect::from_min_size(egui::pos2(rx - 3.0, cy - 10.0), Vec2::new(g.size().x + 6.0, 20.0));
                let resp = ui.interact(r, egui::Id::new(("status", id)), Sense::click());
                let col = if resp.hovered() { hot } else { dim };
                let g = ui.painter().layout_no_wrap(text, font.clone(), col);
                ui.painter().galley(egui::pos2(rx, cy - g.size().y / 2.0), g, col);
                if resp.on_hover_text(tip).clicked() {
                    match id {
                        "pos" => self.exec(ctx, "workbench.action.gotoLine"),
                        "lspinstall" => {
                            if let Some(m) = self.lsp_missing_hint() {
                                ctx.copy_text(m.install.to_string());
                                self.toast(format!("Copied: {}", m.install));
                            }
                        }
                        "indent" => {
                            let n = self.tab().map(|t| (t.buf.indent.use_spaces, t.buf.indent.size));
                            if let Some((sp, sz)) = n {
                                self.exec(ctx, if !sp { "fractal.indentSpaces4" } else if sz == 4 { "fractal.indentSpaces2" } else { "fractal.indentTabs" });
                            }
                        }
                        _ => {}
                    }
                }
                rx -= 18.0;
            }
        });
    }

    // ------------------------------------------------------------------
    // Sidebar: text tabs, no icon strip
    // ------------------------------------------------------------------
    pub fn draw_sidebar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        let margin = Margin { left: 8, right: GAP, top: 0, bottom: 0 };
        Panel::left("sidebar").show_separator_line(false).resizable(true).default_size(268.0).size_range(210.0..=560.0).frame(Frame::new().fill(c(u.bg_base)).inner_margin(margin)).show(ui, |ui| {
            island(ui, c(u.bg_side), |ui| {
                let full = ui.max_rect();
                if ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(full) {
                    self.focus = Focus::Other;
                    ctx.memory_mut(|m| {
                        if let Some(f) = m.focused() {
                            m.surrender_focus(f);
                        }
                    });
                }
                // header with text tabs
                let (hr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::hover());
                let cy = hr.center().y + 1.0;
                let mut x = hr.left() + 18.0;
                let changes = self.scm.change_count();
                for (view, label) in [(SideView::Explorer, "Files"), (SideView::Search, "Search"), (SideView::Scm, "Changes")] {
                    let active = self.side_view == view;
                    let col = if active { c(u.fg) } else { c(u.fg_faint) };
                    let g = txt(ui, label, 12.5, medium(), col);
                    let r = Rect::from_min_size(egui::pos2(x - 6.0, cy - 14.0), Vec2::new(g.size().x + 12.0 + if view == SideView::Scm && changes > 0 { 20.0 } else { 0.0 }, 28.0));
                    let resp = ui.interact(r, egui::Id::new(("side_tab", label)), Sense::click());
                    let col = if resp.hovered() && !active { c(u.fg_dim) } else { col };
                    let g = txt(ui, label, 12.5, medium(), col);
                    ui.painter().galley(egui::pos2(x, cy - g.size().y / 2.0), g.clone(), col);
                    let mut w = g.size().x;
                    if view == SideView::Scm && changes > 0 {
                        let n = txt(ui, &changes.to_string(), 11.0, egui::FontFamily::Proportional, c(u.accent_text()));
                        ui.painter().galley(egui::pos2(x + w + 6.0, cy - n.size().y / 2.0 + 0.5), n.clone(), c(u.accent_text()));
                        w += 6.0 + n.size().x;
                    }
                    if active {
                        ui.painter().rect_filled(Rect::from_min_size(egui::pos2(x, cy + 11.0), Vec2::new(w, 2.0)), 1.0, c(u.accent));
                    }
                    if resp.clicked() {
                        self.show_side(view);
                        if view == SideView::Search {
                            self.search.focus_input = true;
                        }
                    }
                    x += w + 22.0;
                }
                // explorer actions, only on hover
                if self.side_view == SideView::Explorer && self.folder.is_some() && ui.rect_contains_pointer(hr) {
                    let mut ax = hr.right() - 14.0;
                    for (icon, tip, act) in [(Icon::Refresh, "Refresh", 0), (Icon::FolderPlus, "New folder", 1), (Icon::FilePlus, "New file", 2)] {
                        let r = Rect::from_center_size(egui::pos2(ax - 11.0, cy), Vec2::splat(24.0));
                        let resp = ui.interact(r, egui::Id::new(("exp_act", act)), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 6.0, alpha(u.fg, 16));
                        }
                        icons::draw(ui.painter(), icon, r.center(), 14.0, c(u.fg_dim));
                        if resp.on_hover_text(tip).clicked() {
                            match act {
                                0 => self.explorer.refresh(),
                                1 => self.explorer.start_create(true),
                                _ => self.explorer.start_create(false),
                            }
                        }
                        ax -= 26.0;
                    }
                }
                match self.side_view {
                    SideView::Explorer => {
                        if self.folder.is_none() {
                            ui.add_space(8.0);
                            Frame::NONE.inner_margin(Margin::symmetric(18, 0)).show(ui, |ui| {
                                ui.label(egui::RichText::new("No folder open").color(c(u.fg_dim)).size(13.0));
                                ui.add_space(10.0);
                                if crate::widgets::accent_button(ui, &t, "Open Folder", true).clicked() {
                                    self.exec(ctx, "workbench.action.files.openFolder");
                                }
                                ui.add_space(18.0);
                                if !self.session.recent_folders.is_empty() {
                                    ui.label(egui::RichText::new("RECENT").size(10.5).color(c(u.fg_faint)));
                                    ui.add_space(4.0);
                                    for f in self.session.recent_folders.clone().iter().take(6) {
                                        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                        if ui.link(name).on_hover_text(f.to_string_lossy()).clicked() {
                                            self.set_folder(ctx, Some(f.clone()));
                                        }
                                    }
                                }
                            });
                        } else {
                            let active = self.active_path();
                            let git = self.git_status_map.clone();
                            let actions = self.explorer.show(ui, &t, &git, active.as_deref());
                            self.handle_explorer_actions(ctx, actions);
                        }
                    }
                    SideView::Search => {
                        let a = self.search.show(ui, &t, self.folder.as_deref());
                        self.handle_search_action(a);
                    }
                    SideView::Scm => {
                        let acts = self.scm.show(ui, &t, self.folder.as_deref());
                        for a in acts {
                            self.handle_scm_action(a);
                        }
                    }
                }
            });
        });
    }

    // ------------------------------------------------------------------
    // Terminal card
    // ------------------------------------------------------------------
    pub fn draw_bottom_panel(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        let margin = Margin { left: if self.sidebar_visible { GAP } else { 8 }, right: 8, top: GAP * 2, bottom: 0 };
        Panel::bottom("bottom_panel").show_separator_line(false).resizable(true).default_size(260.0).size_range(110.0..=900.0).frame(Frame::new().fill(c(u.bg_base)).inner_margin(margin)).show(ui, |ui| {
            island(ui, c(u.bg), |ui| {
                let (hr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::hover());
                let cy = hr.center().y;
                let mut x = hr.left() + 18.0;
                let mut close_idx = None;
                let mut select_idx = None;
                for (i, term) in self.terms.iter().enumerate() {
                    let active = i == self.active_term;
                    let dead = !term.alive.load(std::sync::atomic::Ordering::Relaxed);
                    let col = if active { c(u.fg) } else { c(u.fg_faint) };
                    let g = txt(ui, &term.title, 12.5, medium(), col);
                    let r = Rect::from_min_size(egui::pos2(x - 6.0, cy - 13.0), Vec2::new(g.size().x + 34.0, 26.0));
                    let resp = ui.interact(r, egui::Id::new(("termtab", i)), Sense::click());
                    let col = if dead { c(u.fg_faint).gamma_multiply(0.7) } else if resp.hovered() && !active { c(u.fg_dim) } else { col };
                    let g = txt(ui, &term.title, 12.5, medium(), col);
                    ui.painter().galley(egui::pos2(x, cy - g.size().y / 2.0), g.clone(), col);
                    if active {
                        ui.painter().rect_filled(Rect::from_min_size(egui::pos2(x, cy + 11.0), Vec2::new(g.size().x, 2.0)), 1.0, c(u.accent));
                    }
                    let xr = Rect::from_center_size(egui::pos2(x + g.size().x + 14.0, cy), Vec2::splat(16.0));
                    let xresp = ui.interact(xr, egui::Id::new(("termclose", i)), Sense::click());
                    if resp.hovered() || active {
                        icons::draw(ui.painter(), Icon::Close, xr.center(), 10.0, if xresp.hovered() { c(u.fg) } else { c(u.fg_faint) });
                    }
                    if xresp.clicked() {
                        close_idx = Some(i);
                    } else if resp.clicked() {
                        select_idx = Some(i);
                    }
                    x += g.size().x + 40.0;
                }
                let pr = Rect::from_center_size(egui::pos2(x + 4.0, cy), Vec2::splat(24.0));
                let presp = ui.interact(pr, egui::Id::new("term_new"), Sense::click());
                if presp.hovered() {
                    ui.painter().rect_filled(pr, 6.0, alpha(u.fg, 16));
                }
                icons::draw(ui.painter(), Icon::Plus, pr.center(), 14.0, c(u.fg_dim));
                if presp.on_hover_text("New terminal  (⌃⇧`)").clicked() {
                    self.new_terminal(ctx, None);
                }
                let cr = Rect::from_center_size(egui::pos2(hr.right() - 20.0, cy), Vec2::splat(24.0));
                let cresp = ui.interact(cr, egui::Id::new("term_hide"), Sense::click());
                if cresp.hovered() {
                    ui.painter().rect_filled(cr, 6.0, alpha(u.fg, 16));
                }
                icons::draw(ui.painter(), Icon::ChevronDown, cr.center(), 14.0, c(u.fg_dim));
                if cresp.on_hover_text("Hide panel").clicked() {
                    self.panel_visible = false;
                    self.focus = Focus::Editor;
                }
                if let Some(i) = select_idx {
                    self.active_term = i;
                    self.focus = Focus::Terminal;
                }
                if let Some(i) = close_idx {
                    self.terms[i].kill();
                    self.terms.remove(i);
                    self.run_term = match self.run_term {
                        Some(r) if r == i => None,
                        Some(r) if r > i => Some(r - 1),
                        o => o,
                    };
                    self.active_term = self.active_term.min(self.terms.len().saturating_sub(1));
                    if self.terms.is_empty() {
                        self.panel_visible = false;
                        self.focus = Focus::Editor;
                    }
                }
                if self.terms.is_empty() {
                    ui.centered_and_justified(|ui| {
                        if ghost_button(ui, &t, "Open a terminal").clicked() {
                            self.new_terminal(ctx, None);
                        }
                    });
                } else {
                    let focused = self.focus == Focus::Terminal;
                    let idx = self.active_term.min(self.terms.len() - 1);
                    let size = self.settings.terminal_font_size;
                    let pressed = self.terms[idx].show(ui, &t, size, focused);
                    if pressed {
                        self.focus = Focus::Terminal;
                        ctx.memory_mut(|m| {
                            if let Some(f) = m.focused() {
                                m.surrender_focus(f);
                            }
                        });
                    }
                }
            });
        });
    }

    // ------------------------------------------------------------------
    // Editor card: tab strip + content
    // ------------------------------------------------------------------
    pub fn draw_central(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        let margin = Margin { left: if self.sidebar_visible { GAP } else { 8 }, right: 8, top: 0, bottom: 0 };
        egui::CentralPanel::default().frame(Frame::new().fill(c(u.bg_base)).inner_margin(margin)).show(ui, |ui| {
            island(ui, c(u.bg), |ui| {
                self.editor_rect = ui.max_rect();
                if !self.tabs.is_empty() {
                    self.draw_tab_strip(ui);
                }
                if self.tabs.is_empty() {
                    self.draw_welcome(ui, ctx);
                    return;
                }
                if self.tab().map_or(false, |t| t.settings_page) {
                    self.draw_settings_page(ui, ctx);
                    return;
                }
                let m = editor::metrics(ctx, self.settings.font_size);
                let blink = if self.settings.cursor_blink {
                    let since = self.last_activity.elapsed().as_secs_f32();
                    since < 0.5 || (since % 1.0) < 0.6
                } else {
                    true
                };
                self.refresh_find();
                let matches = self.find.matches.clone();
                let cur = self.find.current;
                let focused = self.focus == Focus::Editor && !ctx.text_edit_focused();
                let env = EditorEnv {
                    theme: &t,
                    metrics: &m,
                    minimap: self.settings.minimap,
                    line_numbers: self.settings.line_numbers,
                    focused,
                    caret_visible: blink,
                    find_matches: if self.find.visible { &matches } else { &[] },
                    find_current: if self.find.visible { cur } else { None },
                    ghost: self.ghost_text(),
                };
                let active = self.active;
                let res = editor::show(ui, &mut self.tabs[active], &env);
                self.lsp_hover_update(if self.suggest.is_none() { res.hover } else { None });
                if let Some(idx) = res.cmd_click {
                    self.lsp_definition(idx);
                }
                if let (Some(caret), true) = (res.caret, self.suggest.is_some()) {
                    self.draw_suggest(ctx, caret, &m);
                }
                if res.pressed {
                    self.suggest = None;
                    self.focus = Focus::Editor;
                    self.last_activity = Instant::now();
                    ctx.memory_mut(|m| {
                        if let Some(f) = m.focused() {
                            m.surrender_focus(f);
                        }
                    });
                    if self.overlay.is_some() {
                        self.close_overlay(ctx);
                    }
                }
            });
        });
    }

    fn draw_tab_strip(&mut self, ui: &mut Ui) {
        let t = self.theme.clone();
        let u = &t.ui;
        let (strip, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::hover());
        let cy = strip.center().y;
        let font = FontId::new(12.5, medium());
        // measure to keep the active tab in view
        let widths: Vec<f32> = self.tabs.iter().map(|tab| ui.painter().layout_no_wrap(tab.title(), font.clone(), c(u.fg)).size().x + 40.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 4.0 * widths.len() as f32;
        let avail = strip.width() - 24.0;
        let mut shift = 0.0;
        if total > avail {
            let right_of_active: f32 = widths.iter().take(self.active + 1).sum::<f32>() + 4.0 * (self.active as f32 + 1.0);
            shift = (right_of_active - avail + 60.0).max(0.0).min(total - avail);
        }
        let clip = ui.painter().with_clip_rect(strip);
        let mut x = strip.left() + 12.0 - shift;
        let mut close: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut close_others: Option<usize> = None;
        let mut close_all = false;
        let mut reveal_path = None;
        for (i, tab) in self.tabs.iter().enumerate() {
            let w = widths[i];
            let r = Rect::from_min_size(egui::pos2(x, cy - 15.0), Vec2::new(w, 30.0));
            x += w + 4.0;
            if r.right() < strip.left() || r.left() > strip.right() {
                continue;
            }
            let resp = ui.interact(r.intersect(strip), egui::Id::new(("tab", i)), Sense::click());
            let active = i == self.active;
            if active {
                clip.rect_filled(r, 8.0, alpha(u.fg, 15));
            } else if resp.hovered() {
                clip.rect_filled(r, 8.0, alpha(u.fg, 8));
            }
            let name = tab.title();
            let col = if active { c(u.fg) } else if resp.hovered() { c(u.fg_dim) } else { c(u.fg_faint) };
            let g = ui.painter().layout_no_wrap(name, font.clone(), col);
            clip.galley(egui::pos2(r.left() + 14.0, cy - g.size().y / 2.0), g.clone(), col);
            let xr = Rect::from_center_size(egui::pos2(r.right() - 14.0, cy), Vec2::splat(18.0));
            let xresp = ui.interact(xr.intersect(strip), egui::Id::new(("tabclose", i)), Sense::click());
            let dirty = tab.buf.is_dirty();
            if dirty && !xresp.hovered() {
                clip.circle_filled(xr.center(), 3.0, c(u.accent));
            } else if resp.hovered() || active || xresp.hovered() {
                if xresp.hovered() {
                    clip.rect_filled(xr, 5.0, alpha(u.fg, 20));
                }
                if tab.settings_page || !dirty || xresp.hovered() {
                    icons::draw(&clip, Icon::Close, xr.center(), 10.0, if xresp.hovered() { c(u.fg) } else { c(u.fg_faint) });
                }
            }
            if xresp.clicked() || resp.middle_clicked() {
                close = Some(i);
            } else if resp.clicked() {
                select = Some(i);
            }
            resp.context_menu(|ui| {
                if ui.button("Close").clicked() {
                    close = Some(i);
                    ui.close();
                }
                if ui.button("Close Others").clicked() {
                    close_others = Some(i);
                    ui.close();
                }
                if ui.button("Close All").clicked() {
                    close_all = true;
                    ui.close();
                }
                if let Some(p) = &tab.buf.path {
                    ui.separator();
                    if ui.button("Reveal in Finder").clicked() {
                        reveal_path = Some(p.clone());
                        ui.close();
                    }
                    if ui.button("Copy Path").clicked() {
                        ui.ctx().copy_text(p.to_string_lossy().to_string());
                        ui.close();
                    }
                }
            });
        }
        if let Some(i) = select {
            self.active = i;
            self.focus = Focus::Editor;
        }
        if let Some(i) = close {
            self.close_tab(i, true);
        }
        if let Some(keep) = close_others {
            let mut i = self.tabs.len();
            while i > 0 {
                i -= 1;
                if i != keep && !self.close_tab(i, true) {
                    break;
                }
            }
        }
        if close_all {
            let mut i = self.tabs.len();
            while i > 0 {
                i -= 1;
                if !self.close_tab(i, true) {
                    break;
                }
            }
        }
        if let Some(p) = reveal_path {
            crate::app::reveal(&p);
        }
    }

    // ------------------------------------------------------------------
    // Welcome
    // ------------------------------------------------------------------
    pub fn draw_welcome(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        let rect = ui.max_rect();
        let cx = rect.center().x;
        let top = (rect.center().y - 210.0).max(rect.top() + 40.0);
        fn tri(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, cc: egui::Pos2, depth: u32, col: Color32) {
            if depth == 0 {
                p.add(egui::Shape::convex_polygon(vec![a, b, cc], col, egui::Stroke::NONE));
                return;
            }
            let ab = a.lerp(b, 0.5);
            let bc = b.lerp(cc, 0.5);
            let ca = cc.lerp(a, 0.5);
            tri(p, a, ab, ca, depth - 1, col);
            tri(p, ab, b, bc, depth - 1, col);
            tri(p, ca, bc, cc, depth - 1, col);
        }
        let s = 44.0;
        let h = s * 0.866;
        tri(ui.painter(), egui::pos2(cx, top), egui::pos2(cx - s / 2.0, top + h), egui::pos2(cx + s / 2.0, top + h), 4, c(u.accent));
        ui.painter().text(egui::pos2(cx, top + h + 40.0), Align2::CENTER_CENTER, "Fractal", FontId::new(30.0, semibold()), c(u.fg));
        ui.painter().text(egui::pos2(cx, top + h + 68.0), Align2::CENTER_CENTER, "A quiet, fast editor.", FontId::proportional(13.0), c(u.fg_dim));
        let mut y = top + h + 116.0;
        let rows: Vec<(&str, &str, &str)> = vec![
            ("Open folder", "workbench.action.files.openFolder", "⌘K ⌘O"),
            ("New file", "workbench.action.files.newUntitledFile", "⌘N"),
            ("Go to file", "workbench.action.quickOpen", "⌘P"),
            ("Command palette", "workbench.action.showCommands", "⇧⌘P"),
            ("Terminal", "workbench.action.terminal.toggleTerminal", "⌃`"),
        ];
        for (label, cmd, hint) in rows {
            let hint_txt = self.keymap.label_for(cmd).unwrap_or_else(|| hint.to_string());
            let r = Rect::from_center_size(egui::pos2(cx, y), Vec2::new(340.0, 34.0));
            let resp = ui.interact(r, egui::Id::new(("welcome", label)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 8.0, alpha(u.fg, 10));
            }
            ui.painter().text(egui::pos2(r.left() + 14.0, y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), c(u.fg));
            ui.painter().text(egui::pos2(r.right() - 14.0, y), Align2::RIGHT_CENTER, hint_txt, FontId::proportional(12.0), c(u.fg_faint));
            if resp.clicked() {
                self.exec(ctx, cmd);
            }
            y += 36.0;
        }
        if !self.session.recent_folders.is_empty() {
            y += 22.0;
            ui.painter().text(egui::pos2(cx - 170.0 + 14.0, y), Align2::LEFT_CENTER, "RECENT", FontId::proportional(10.5), c(u.fg_faint));
            y += 24.0;
            for f in self.session.recent_folders.clone().iter().take(4) {
                let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let r = Rect::from_center_size(egui::pos2(cx, y), Vec2::new(340.0, 30.0));
                let resp = ui.interact(r, egui::Id::new(("recent", f)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 8.0, alpha(u.fg, 10));
                }
                ui.painter().text(egui::pos2(r.left() + 14.0, y), Align2::LEFT_CENTER, name, FontId::proportional(13.0), c(u.accent_text()));
                if resp.on_hover_text(f.to_string_lossy()).clicked() {
                    self.set_folder(ctx, Some(f.clone()));
                }
                y += 32.0;
            }
        }
        let _ = (UiBuilder::new(), Duration::from_millis(1));
    }
}
