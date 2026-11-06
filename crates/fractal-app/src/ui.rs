//! Window layout: title bar, activity bar, sidebar, tabs, panels, status bar.

use crate::app::{App, Focus, SideView};
use crate::editor::{self, EditorEnv};
use crate::icons::{self, Icon};
use crate::overlay::Kind;
use crate::ui_theme::c;
use crate::widgets::{accent_button, ghost_button, icon_button, small_toggle};
use eframe::egui::{self, Align2, Color32, FontId, Frame, Margin, Panel, Rect, Sense, Stroke, TextEdit, Ui, UiBuilder, Vec2, ViewportCommand};
use fractal_core::run::RunKind;
use std::time::{Duration, Instant};

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if (self.applied_zoom - self.settings.ui_scale).abs() > 1e-3 {
            ctx.set_zoom_factor(self.settings.ui_scale);
            self.applied_zoom = self.settings.ui_scale;
        }
        self.handle_close_request(&ctx);
        self.handle_dropped(&ctx);
        self.handle_input(&ctx);
        self.tick(&ctx);
        self.refresh_find();
        if self.autorun && !self.run_specs.is_empty() && self.run_job.is_none() {
            self.autorun = false;
            self.exec(&ctx, "fractal.run");
        }

        let title = match self.tab() {
            Some(t) => format!("{}{} — Fractal", t.title(), if t.buf.is_dirty() { " ●" } else { "" }),
            None => "Fractal".to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        self.draw_titlebar(ui, &ctx);
        self.draw_statusbar(ui, &ctx);
        self.draw_activity_bar(ui);
        if self.sidebar_visible {
            self.draw_sidebar(ui, &ctx);
        }
        if self.panel_visible {
            self.draw_bottom_panel(ui, &ctx);
        }
        self.draw_tabbar(ui);
        self.draw_central(ui, &ctx);
        self.draw_find(&ctx);
        self.show_overlay(&ctx);
        self.draw_toasts(&ctx);

        if self.settings.cursor_blink && self.focus == Focus::Editor && !self.tabs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }

    fn on_exit(&mut self) {
        self.save_session();
        self.settings.save();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let b = self.theme.ui.bg;
        [b[0] as f32 / 255.0, b[1] as f32 / 255.0, b[2] as f32 / 255.0, 1.0]
    }
}

impl App {
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_confirmed {
            if self.has_dirty() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                let r = rfd::MessageDialog::new()
                    .set_title("Unsaved changes")
                    .set_description("You have unsaved changes. Save them before quitting?")
                    .set_buttons(rfd::MessageButtons::YesNoCancelCustom("Save All".into(), "Quit Without Saving".into(), "Cancel".into()))
                    .show();
                match r {
                    rfd::MessageDialogResult::Custom(s) if s == "Save All" => {
                        self.exec(ctx, "workbench.action.files.saveAll");
                        if !self.has_dirty() {
                            self.quit_confirmed = true;
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    }
                    rfd::MessageDialogResult::Custom(s) if s == "Quit Without Saving" => {
                        self.quit_confirmed = true;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_dropped(&mut self, ctx: &egui::Context) {
        let files: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        for f in files {
            if f.is_dir() {
                self.set_folder(ctx, Some(f));
            } else {
                self.open_file(&f, None);
            }
        }
    }

    // ------------------------------------------------------------------
    fn draw_titlebar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::top("titlebar").exact_size(40.0).frame(Frame::new().fill(c(u.bg_title))).show(ui, |ui| {
            let rect = ui.max_rect();
            let drag = ui.interact(rect, egui::Id::new("title_drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
            }
            ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, c(u.border)));
            let cy = rect.center().y;
            let left = rect.left() + if self.mac { 80.0 } else { 12.0 };
            // sidebar toggle
            let r = Rect::from_center_size(egui::pos2(left + 14.0, cy), Vec2::splat(28.0));
            ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
                if icon_button(ui, &t, Icon::Sidebar, "Toggle Sidebar", self.sidebar_visible, 28.0).clicked() {
                    self.sidebar_visible = !self.sidebar_visible;
                }
            });

            // command center
            let name = self.folder.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Fractal".into());
            let w = (rect.width() * 0.34).clamp(180.0, 440.0);
            let cc = Rect::from_center_size(egui::pos2(rect.center().x, cy), Vec2::new(w, 28.0));
            let resp = ui.interact(cc, egui::Id::new("cmd_center"), Sense::click());
            ui.painter().rect_filled(cc, 8.0, if resp.hovered() { c(u.bg_hover) } else { c(u.bg_input) });
            ui.painter().rect_stroke(cc, 8.0, Stroke::new(1.0, c(u.border)), egui::StrokeKind::Inside);
            icons::draw(ui.painter(), Icon::Search, egui::pos2(cc.left() + 16.0, cy), 13.0, c(u.fg_dim));
            ui.painter().text(cc.center(), Align2::CENTER_CENTER, name, FontId::proportional(12.5), c(u.fg_dim));
            if resp.on_hover_text("Search files, run commands (⌘P / ⇧⌘P)").clicked() {
                self.open_overlay(Kind::QuickOpen, "");
            }

            // right cluster (from the right edge)
            let mut x = rect.right() - 12.0;
            // settings
            x -= 28.0;
            let r = Rect::from_min_size(egui::pos2(x, cy - 14.0), Vec2::splat(28.0));
            ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
                if icon_button(ui, &t, Icon::Gear, "Settings (JSON)", false, 28.0).clicked() {
                    self.exec(ctx, "workbench.action.openSettings");
                }
            });
            x -= 30.0;
            let r = Rect::from_min_size(egui::pos2(x, cy - 14.0), Vec2::splat(28.0));
            ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
                if icon_button(ui, &t, Icon::Panel, "Toggle Terminal Panel", self.panel_visible, 28.0).clicked() {
                    self.exec(ctx, "workbench.action.togglePanel");
                }
            });
            x -= 30.0;
            let r = Rect::from_min_size(egui::pos2(x, cy - 14.0), Vec2::splat(28.0));
            ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
                if icon_button(ui, &t, Icon::Palette, "Color Theme", false, 28.0).clicked() {
                    self.exec(ctx, "workbench.action.selectTheme");
                }
            });
            x -= 12.0;

            // Run cluster
            let spec_idx = self.preferred_run();
            let spec = spec_idx.map(|i| self.run_specs[i].clone());
            let can_run = spec.is_some();
            let running_live = self.live.is_some();
            let run_term_alive = self.run_term.map_or(false, |i| self.terms.get(i).map_or(false, |t| t.alive.load(std::sync::atomic::Ordering::Relaxed)));
            // dropdown chevron
            let dd = Rect::from_min_size(egui::pos2(x - 26.0, cy - 14.0), Vec2::new(26.0, 28.0));
            let main_w = 70.0;
            let main = Rect::from_min_size(egui::pos2(dd.left() - main_w, cy - 14.0), Vec2::new(main_w, 28.0));
            let pill = Rect::from_min_max(main.min, dd.max);
            let base = if can_run { c(u.accent) } else { c(u.accent).gamma_multiply(0.35) };
            let r_main = ui.interact(main, egui::Id::new("run_main"), Sense::click());
            let r_dd = ui.interact(dd, egui::Id::new("run_dd"), Sense::click());
            let hover = r_main.hovered() || r_dd.hovered();
            ui.painter().rect_filled(pill, 8.0, if hover && can_run { base.gamma_multiply(1.12) } else { base });
            ui.painter().line_segment([egui::pos2(dd.left(), cy - 8.0), egui::pos2(dd.left(), cy + 8.0)], Stroke::new(1.0, c(u.accent_fg).gamma_multiply(0.35)));
            icons::draw(ui.painter(), Icon::Play, egui::pos2(main.left() + 19.0, cy), 13.0, c(u.accent_fg));
            ui.painter().text(egui::pos2(main.left() + 32.0, cy), Align2::LEFT_CENTER, "Run", FontId::proportional(13.0), c(u.accent_fg));
            icons::draw(ui.painter(), Icon::ChevronDown, dd.center(), 11.0, c(u.accent_fg));
            let tip = spec.as_ref().map(|s| format!("{}  ·  {}   (F5)", s.label, s.detail)).unwrap_or_else(|| "Nothing to run — open a file or project".into());
            if r_main.on_hover_text(tip).clicked() {
                self.exec(ctx, "fractal.run");
            }
            if r_dd.on_hover_text("Choose what to run").clicked() {
                self.exec(ctx, "fractal.runPicker");
            }
            // stop button
            if running_live || run_term_alive {
                let sr = Rect::from_min_size(egui::pos2(pill.left() - 34.0, cy - 14.0), Vec2::splat(28.0));
                ui.scope_builder(UiBuilder::new().max_rect(sr), |ui| {
                    if icon_button(ui, &t, Icon::Stop, "Stop (⇧⌘R)", false, 28.0).clicked() {
                        self.stop_run();
                    }
                });
            }
            if running_live {
                let port = self.live.as_ref().map(|l| l.port).unwrap_or(0);
                let txt = format!("● Live :{port}");
                ui.painter().text(egui::pos2(pill.left() - 42.0, cy), Align2::RIGHT_CENTER, txt, FontId::proportional(11.5), c(u.git_added));
            }
        });
    }

    // ------------------------------------------------------------------
    fn draw_statusbar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::bottom("statusbar").exact_size(24.0).frame(Frame::new().fill(c(u.bg_status))).show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, c(u.border)));
            let cy = rect.center().y;
            let font = FontId::proportional(11.5);
            let col = c(u.fg_status);
            // left items
            let mut x = rect.left() + 12.0;
            let mut item = |ui: &mut Ui, x: &mut f32, icon: Option<Icon>, text: String, tip: &str, id: &str| -> bool {
                let g = ui.painter().layout_no_wrap(text, font.clone(), col);
                let iw = if icon.is_some() { 16.0 } else { 0.0 };
                let r = Rect::from_min_size(egui::pos2(*x - 4.0, rect.top() + 2.0), Vec2::new(g.size().x + iw + 8.0, 20.0));
                let resp = ui.interact(r, egui::Id::new(("status", id)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 4.0, c(u.bg_hover));
                }
                if let Some(ic) = icon {
                    icons::draw(ui.painter(), ic, egui::pos2(*x + 6.0, cy), 13.0, col);
                }
                ui.painter().galley(egui::pos2(*x + iw, cy - g.size().y / 2.0), g.clone(), col);
                *x += g.size().x + iw + 14.0;
                resp.on_hover_text(tip).clicked()
            };
            if let Some(st) = self.scm.status.clone() {
                let mut s = st.branch.clone();
                if st.ahead > 0 || st.behind > 0 {
                    s.push_str(&format!("  ↓{} ↑{}", st.behind, st.ahead));
                }
                if item(ui, &mut x, Some(Icon::Branch), s, "Switch branch", "branch") {
                    self.open_overlay(Kind::Branch, "");
                }
                let n = st.entries.len();
                if n > 0 && item(ui, &mut x, Some(Icon::Commit), format!("{n} changed"), "Uncommitted changes", "changes") {
                    self.show_side(SideView::Scm);
                }
            }
            if self.scm.busy() {
                item(ui, &mut x, Some(Icon::Refresh), "Working…".into(), "Git operation in progress", "busy");
            }
            if self.settings.auto_save {
                item(ui, &mut x, Some(Icon::Check), "Auto Save".into(), "Click to toggle", "autosave");
            }
            // right items
            let mut parts: Vec<(String, &str, &str)> = vec![];
            if let Some(tab) = self.tab() {
                let (l, cc) = tab.buf.char_to_pos(tab.buf.primary().head);
                let line_txt = tab.buf.line_string(l);
                let vis = editor::char_col_to_vis(&line_txt, cc, tab.buf.indent.size) + 1;
                let sel = tab.buf.primary();
                let mut pos = format!("Ln {}, Col {}", l + 1, vis);
                if !sel.is_empty() {
                    pos.push_str(&format!(" ({} selected)", sel.end() - sel.start()));
                }
                if tab.buf.sels.len() > 1 {
                    pos.push_str(&format!("  [{} cursors]", tab.buf.sels.len()));
                }
                parts.push((pos, "Go to line (^G)", "pos"));
                let ind = if tab.buf.indent.use_spaces { format!("Spaces: {}", tab.buf.indent.size) } else { "Tab Size".to_string() };
                parts.push((ind, "Indentation", "indent"));
                parts.push(("UTF-8".into(), "Encoding", "enc"));
                parts.push((if tab.buf.crlf { "CRLF".into() } else { "LF".into() }, "Line ending", "eol"));
                parts.push((tab.hl.syntax_name().to_string(), "Language", "lang"));
            }
            let mut rx = rect.right() - 12.0;
            for (text, tip, id) in parts.into_iter().rev() {
                let g = ui.painter().layout_no_wrap(text, font.clone(), col);
                rx -= g.size().x;
                let r = Rect::from_min_size(egui::pos2(rx - 4.0, rect.top() + 2.0), Vec2::new(g.size().x + 8.0, 20.0));
                let resp = ui.interact(r, egui::Id::new(("status", id)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 4.0, c(u.bg_hover));
                }
                ui.painter().galley(egui::pos2(rx, cy - g.size().y / 2.0), g, col);
                if resp.on_hover_text(tip).clicked() {
                    match id {
                        "pos" => self.exec(ctx, "workbench.action.gotoLine"),
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
    fn draw_activity_bar(&mut self, ui: &mut Ui) {
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::left("activity").exact_size(48.0).resizable(false).frame(Frame::new().fill(c(u.bg_side))).show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, c(u.border)));
            let items = [(SideView::Explorer, Icon::Files, "Explorer (⇧⌘E)"), (SideView::Search, Icon::Search, "Search (⇧⌘F)"), (SideView::Scm, Icon::Git, "Source Control (⌃⇧G)")];
            let mut y = rect.top() + 8.0;
            for (view, icon, tip) in items {
                let r = Rect::from_min_size(egui::pos2(rect.left(), y), Vec2::new(48.0, 44.0));
                let resp = ui.interact(r, egui::Id::new(("act", tip)), Sense::click());
                let active = self.side_view == view && self.sidebar_visible;
                if active {
                    ui.painter().rect_filled(Rect::from_min_size(r.min + Vec2::new(0.0, 8.0), Vec2::new(2.5, 28.0)), 1.5, c(u.accent));
                }
                icons::draw(ui.painter(), icon, r.center(), 22.0, if active || resp.hovered() { c(u.fg) } else { c(u.fg_dim) });
                if view == SideView::Scm {
                    let n = self.scm.change_count();
                    if n > 0 {
                        let b = egui::pos2(r.center().x + 9.0, r.center().y + 9.0);
                        ui.painter().circle_filled(b, 8.0, c(u.accent));
                        ui.painter().text(b, Align2::CENTER_CENTER, if n > 99 { "99+".into() } else { n.to_string() }, FontId::proportional(9.5), c(u.accent_fg));
                    }
                }
                if resp.on_hover_text(tip).clicked() {
                    if active {
                        self.sidebar_visible = false;
                    } else {
                        self.show_side(view);
                    }
                }
                y += 46.0;
            }
        });
    }

    // ------------------------------------------------------------------
    fn draw_sidebar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::left("sidebar").resizable(true).default_size(268.0).size_range(180.0..=560.0).frame(Frame::new().fill(c(u.bg_side))).show(ui, |ui| {
            let rect = ui.max_rect();
            if ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(rect) {
                self.focus = Focus::Other;
                ctx.memory_mut(|m| {
                    if let Some(f) = m.focused() {
                        m.surrender_focus(f);
                    }
                });
            }
            match self.side_view {
                SideView::Explorer => {
                    if self.folder.is_none() {
                        ui.add_space(16.0);
                        Frame::NONE.inner_margin(Margin::symmetric(14, 0)).show(ui, |ui| {
                            ui.label(egui::RichText::new("You have not yet opened a folder.").color(c(u.fg_dim)));
                            ui.add_space(10.0);
                            if accent_button(ui, &t, "Open Folder", true).clicked() {
                                self.exec(ctx, "workbench.action.files.openFolder");
                            }
                            ui.add_space(14.0);
                            if !self.session.recent_folders.is_empty() {
                                ui.label(egui::RichText::new("RECENT").size(11.0).color(c(u.fg_faint)));
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
    }

    // ------------------------------------------------------------------
    fn draw_bottom_panel(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::bottom("bottom_panel").resizable(true).default_size(250.0).size_range(100.0..=900.0).frame(Frame::new().fill(c(u.bg_panel))).show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, c(u.border)));
            // header
            let (hr, _) = ui.allocate_exact_size(Vec2::new(rect.width(), 30.0), Sense::hover());
            let mut x = hr.left() + 14.0;
            ui.painter().text(egui::pos2(x, hr.center().y), Align2::LEFT_CENTER, "TERMINAL", FontId::proportional(11.0), c(u.fg));
            x += 82.0;
            let mut close_idx = None;
            let mut select_idx = None;
            for (i, term) in self.terms.iter().enumerate() {
                let label = term.title.clone();
                let g = ui.painter().layout_no_wrap(label, FontId::proportional(12.0), c(u.fg));
                let w = g.size().x + 52.0;
                let r = Rect::from_min_size(egui::pos2(x, hr.top() + 3.0), Vec2::new(w, 24.0));
                let resp = ui.interact(r, egui::Id::new(("termtab", i)), Sense::click());
                let active = i == self.active_term;
                if active {
                    ui.painter().rect_filled(r, 6.0, c(u.bg_hover));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 6.0, c(u.bg_hover).gamma_multiply(0.6));
                }
                let dead = !term.alive.load(std::sync::atomic::Ordering::Relaxed);
                icons::draw(ui.painter(), Icon::Terminal, egui::pos2(r.left() + 13.0, r.center().y), 12.0, if dead { c(u.fg_faint) } else { c(u.fg_dim) });
                ui.painter().galley(egui::pos2(r.left() + 32.0, r.center().y - g.size().y / 2.0), g, if active { c(u.fg) } else { c(u.fg_dim) });
                let xr = Rect::from_center_size(egui::pos2(r.right() - 10.0, r.center().y), Vec2::splat(16.0));
                let xresp = ui.interact(xr, egui::Id::new(("termclose", i)), Sense::click());
                if resp.hovered() || active {
                    icons::draw(ui.painter(), Icon::Close, xr.center(), 10.0, if xresp.hovered() { c(u.fg) } else { c(u.fg_faint) });
                }
                if xresp.clicked() {
                    close_idx = Some(i);
                } else if resp.clicked() {
                    select_idx = Some(i);
                }
                x += w + 4.0;
            }
            let pr = Rect::from_min_size(egui::pos2(x, hr.top() + 2.0), Vec2::splat(26.0));
            ui.scope_builder(UiBuilder::new().max_rect(pr), |ui| {
                if icon_button(ui, &t, Icon::Plus, "New Terminal (⌃⇧`)", false, 26.0).clicked() {
                    self.new_terminal(ctx, None);
                }
            });
            let cr = Rect::from_min_size(egui::pos2(hr.right() - 34.0, hr.top() + 2.0), Vec2::splat(26.0));
            ui.scope_builder(UiBuilder::new().max_rect(cr), |ui| {
                if icon_button(ui, &t, Icon::Close, "Close Panel", false, 26.0).clicked() {
                    self.panel_visible = false;
                    self.focus = Focus::Editor;
                }
            });
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
            // body
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
    }

    // ------------------------------------------------------------------
    fn draw_tabbar(&mut self, ui: &mut Ui) {
        if self.tabs.is_empty() {
            return;
        }
        let t = self.theme.clone();
        let u = &t.ui;
        Panel::top("tabbar").exact_size(36.0).frame(Frame::new().fill(c(u.bg_tab))).show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, c(u.border)));
            let mut close: Option<usize> = None;
            let mut select: Option<usize> = None;
            let mut close_others: Option<usize> = None;
            let mut close_all = false;
            let mut reveal_path = None;
            egui::ScrollArea::horizontal().id_salt("tabs_scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let name = tab.title();
                        let g = ui.painter().layout_no_wrap(name.clone(), FontId::proportional(12.5), c(u.fg));
                        let w = g.size().x + 62.0;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
                        let active = i == self.active;
                        if active {
                            ui.painter().rect_filled(r, 0.0, c(u.bg_tab_active));
                            ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width(), 2.0)), 0.0, c(u.accent));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, c(u.bg_hover).gamma_multiply(0.5));
                        }
                        ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, c(u.border)));
                        let key = tab.buf.path.as_deref().map(|p| icons::file_icon_key(p, false, false)).unwrap_or("document");
                        icons::draw_file(ui.painter(), key, egui::pos2(r.left() + 18.0, r.center().y), 15.0);
                        ui.painter().galley(egui::pos2(r.left() + 32.0, r.center().y - g.size().y / 2.0), g, if active { c(u.fg) } else { c(u.fg_dim) });
                        let xr = Rect::from_center_size(egui::pos2(r.right() - 14.0, r.center().y), Vec2::splat(18.0));
                        let xresp = ui.interact(xr, egui::Id::new(("tabclose", i)), Sense::click());
                        let dirty = tab.buf.is_dirty();
                        if dirty && !xresp.hovered() {
                            ui.painter().circle_filled(xr.center(), 4.0, c(u.fg_dim));
                        } else if resp.hovered() || active || xresp.hovered() {
                            if xresp.hovered() {
                                ui.painter().rect_filled(xr, 4.0, c(u.bg_hover));
                            }
                            icons::draw(ui.painter(), Icon::Close, xr.center(), 11.0, c(u.fg_dim));
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
                        if active && self.last_activity.elapsed() < Duration::from_millis(50) {
                            ui.scroll_to_rect(r, None);
                        }
                    }
                });
            });
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
        });
    }

    // ------------------------------------------------------------------
    fn draw_central(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        egui::CentralPanel::default().frame(Frame::new().fill(c(u.bg))).show(ui, |ui| {
            self.editor_rect = ui.max_rect();
            if self.tabs.is_empty() {
                self.draw_welcome(ui, ctx);
                return;
            }
            // breadcrumbs
            if let Some(tab) = self.tab() {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
                let text = match (&tab.buf.path, &self.folder) {
                    (Some(p), Some(f)) => p.strip_prefix(f).unwrap_or(p).components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("  ›  "),
                    (Some(p), None) => p.to_string_lossy().to_string(),
                    _ => tab.title(),
                };
                ui.painter().text(egui::pos2(rect.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(11.5), c(u.fg_dim));
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
            };
            let active = self.active;
            let res = editor::show(ui, &mut self.tabs[active], &env);
            if res.pressed {
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
    }

    fn draw_welcome(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let u = &t.ui;
        let rect = ui.max_rect();
        let cx = rect.center().x;
        let top = rect.center().y - 190.0;
        // sierpinski logo
        fn tri(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, cc: egui::Pos2, depth: u32, col: Color32) {
            if depth == 0 {
                p.add(egui::Shape::convex_polygon(vec![a, b, cc], col, Stroke::NONE));
                return;
            }
            let ab = a.lerp(b, 0.5);
            let bc = b.lerp(cc, 0.5);
            let ca = cc.lerp(a, 0.5);
            tri(p, a, ab, ca, depth - 1, col);
            tri(p, ab, b, bc, depth - 1, col);
            tri(p, ca, bc, cc, depth - 1, col);
        }
        let s = 84.0;
        let h = s * 0.866;
        let ty = top.max(rect.top() + 30.0);
        tri(ui.painter(), egui::pos2(cx, ty), egui::pos2(cx - s / 2.0, ty + h), egui::pos2(cx + s / 2.0, ty + h), 4, c(u.accent));
        ui.painter().text(egui::pos2(cx, ty + h + 36.0), Align2::CENTER_CENTER, "Fractal", FontId::proportional(30.0), c(u.fg));
        ui.painter().text(egui::pos2(cx, ty + h + 62.0), Align2::CENTER_CENTER, "Fast, minimal, VS Code–compatible. Built in Rust.", FontId::proportional(13.0), c(u.fg_dim));
        let mut y = ty + h + 100.0;
        let rows: Vec<(&str, &str, &str)> = vec![
            ("Open Folder", "workbench.action.files.openFolder", "⌘K ⌘O"),
            ("New File", "workbench.action.files.newUntitledFile", "⌘N"),
            ("Go to File", "workbench.action.quickOpen", "⌘P"),
            ("Command Palette", "workbench.action.showCommands", "⇧⌘P"),
            ("Toggle Terminal", "workbench.action.terminal.toggleTerminal", "⌃`"),
        ];
        for (label, cmd, hint) in rows {
            let hint_txt = self.keymap.label_for(cmd).unwrap_or_else(|| hint.to_string());
            let r = Rect::from_center_size(egui::pos2(cx, y), Vec2::new(320.0, 32.0));
            let resp = ui.interact(r, egui::Id::new(("welcome", label)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 8.0, c(u.bg_hover));
            }
            ui.painter().text(egui::pos2(r.left() + 14.0, y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), c(u.fg));
            ui.painter().text(egui::pos2(r.right() - 14.0, y), Align2::RIGHT_CENTER, hint_txt, FontId::proportional(12.0), c(u.fg_dim));
            if resp.clicked() {
                self.exec(ctx, cmd);
            }
            y += 36.0;
        }
        if !self.session.recent_folders.is_empty() {
            y += 14.0;
            ui.painter().text(egui::pos2(cx - 160.0, y), Align2::LEFT_CENTER, "RECENT", FontId::proportional(11.0), c(u.fg_faint));
            y += 22.0;
            for f in self.session.recent_folders.clone().iter().take(4) {
                let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let r = Rect::from_center_size(egui::pos2(cx, y), Vec2::new(320.0, 28.0));
                let resp = ui.interact(r, egui::Id::new(("recent", f)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 8.0, c(u.bg_hover));
                }
                ui.painter().text(egui::pos2(r.left() + 14.0, y), Align2::LEFT_CENTER, name, FontId::proportional(13.0), c(u.accent));
                if resp.on_hover_text(f.to_string_lossy()).clicked() {
                    self.set_folder(ctx, Some(f.clone()));
                }
                y += 30.0;
            }
        }
    }

    // ------------------------------------------------------------------
    fn draw_find(&mut self, ctx: &egui::Context) {
        if !self.find.visible || self.tabs.is_empty() {
            return;
        }
        let t = self.theme.clone();
        let u = &t.ui;
        let width = if self.find.replace_visible { 430.0 } else { 430.0 };
        let pos = egui::pos2((self.editor_rect.right() - width - 24.0).max(self.editor_rect.left() + 10.0), self.editor_rect.top() + 34.0);
        let id = egui::Id::new("find_input");
        self.find.input_id = Some(id);
        let mut next = false;
        let mut prev = false;
        let mut close = false;
        let mut replace_one = false;
        let mut replace_all = false;
        egui::Area::new(egui::Id::new("find_widget")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            Frame::NONE
                .fill(c(u.bg_side))
                .stroke(Stroke::new(1.0, c(u.border)))
                .corner_radius(egui::CornerRadius::same(10))
                .shadow(egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(if t.dark { 120 } else { 40 }) })
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(width - 16.0);
                    ui.horizontal(|ui| {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                        icons::draw(ui.painter(), if self.find.replace_visible { Icon::ChevronDown } else { Icon::ChevronRight }, rect.center(), 12.0, c(u.fg_dim));
                        if resp.clicked() {
                            self.find.replace_visible = !self.find.replace_visible;
                        }
                        let te = ui.add_sized([170.0, 26.0], TextEdit::singleline(&mut self.find.query.text).id(id).hint_text("Find").lock_focus(true));
                        if self.find.focus_input {
                            te.request_focus();
                            self.find.focus_input = false;
                            if let Some(mut st) = TextEdit::load_state(ui.ctx(), id) {
                                let n = self.find.query.text.chars().count();
                                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(n))));
                                st.store(ui.ctx(), id);
                            }
                        }
                        small_toggle(ui, &t, Icon::CaseSensitive, "Match Case", &mut self.find.query.case_sensitive);
                        small_toggle(ui, &t, Icon::WholeWord, "Whole Word", &mut self.find.query.whole_word);
                        small_toggle(ui, &t, Icon::Regex, "Regex", &mut self.find.query.regex);
                        let count = if self.find.query.text.is_empty() {
                            String::new()
                        } else if self.find.matches.is_empty() {
                            "No results".to_string()
                        } else {
                            format!("{} of {}", self.find.current.map_or(0, |i| i + 1), self.find.matches.len())
                        };
                        ui.label(egui::RichText::new(count).size(11.5).color(c(u.fg_dim)));
                        if icon_button(ui, &t, Icon::Up, "Previous (⇧↩)", false, 22.0).clicked() {
                            prev = true;
                        }
                        if icon_button(ui, &t, Icon::Down, "Next (↩)", false, 22.0).clicked() {
                            next = true;
                        }
                        if icon_button(ui, &t, Icon::Close, "Close (Esc)", false, 22.0).clicked() {
                            close = true;
                        }
                    });
                    if self.find.replace_visible {
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.add_sized([170.0, 26.0], TextEdit::singleline(&mut self.find.replace).hint_text("Replace"));
                            if ghost_button(ui, &t, "Replace").clicked() {
                                replace_one = true;
                            }
                            if ghost_button(ui, &t, "All").clicked() {
                                replace_all = true;
                            }
                        });
                    }
                });
        });
        if next {
            self.find_step(true);
        }
        if prev {
            self.find_step(false);
        }
        if close {
            self.exec(ctx, "closeFindWidget");
        }
        if replace_one {
            self.find_replace(false);
        }
        if replace_all {
            self.find_replace(true);
        }
    }

    // ------------------------------------------------------------------
    fn draw_toasts(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }
        let t = self.theme.clone();
        let u = &t.ui;
        let screen = ctx.content_rect();
        let mut y = screen.bottom() - 40.0;
        for (i, toast) in self.toasts.iter().enumerate().rev() {
            let age = toast.born.elapsed().as_secs_f32();
            let alpha = if age > 4.2 { (5.0 - age) / 0.8 } else { 1.0 }.clamp(0.0, 1.0);
            let id = egui::Id::new(("toast", i));
            let resp = egui::Area::new(id).order(egui::Order::Tooltip).anchor(Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -(screen.bottom() - y))).show(ctx, |ui| {
                Frame::NONE
                    .fill(c(u.bg_side).gamma_multiply(alpha))
                    .stroke(Stroke::new(1.0, c(u.border).gamma_multiply(alpha)))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(14, 10))
                    .shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha((90.0 * alpha) as u8) })
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.label(egui::RichText::new(&toast.text).color(c(u.fg).gamma_multiply(alpha)).size(12.5));
                    });
            });
            y -= resp.response.rect.height() + 8.0;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
