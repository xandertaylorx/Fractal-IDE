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

        if self.autorun_settings {
            self.autorun_settings = false;
            self.open_settings_page();
        }
        if let Some(cmd) = self.autocmd.take() {
            for c in cmd.split(',') {
                self.exec(&ctx, c.trim());
            }
        }
        if let Some(txt) = self.autotype.take() {
            if let Some(t) = self.tabs.get_mut(self.active) {
                let end = t.buf.len_chars();
                t.buf.set_single_caret(end);
                for ch in txt.chars() {
                    if ch == '\n' { t.buf.newline(); } else { t.buf.type_char(ch); }
                }
                t.reveal = true;
            }
            self.focus = Focus::Editor;
            let last = txt.chars().last();
            self.after_typing(last);
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
        if self.sidebar_visible {
            self.draw_sidebar(ui, &ctx);
        }
        if self.panel_visible {
            self.draw_bottom_panel(ui, &ctx);
        }
        self.draw_central(ui, &ctx);
        self.draw_find(&ctx);
        self.draw_hover(&ctx);
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
        let b = self.theme.ui.bg_base;
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

    pub(crate) fn ghost_text(&self) -> Option<String> {
        if !self.settings.ghost_text {
            return None;
        }
        let sg = self.suggest.as_ref()?;
        let item = sg.items.get(sg.selected)?;
        match item.kind {
            fractal_core::complete::Kind::Snippet => Some(item.insert.split('\n').next().unwrap_or("").replace("$0", "")),
            _ => item.label.strip_prefix(sg.prefix.as_str()).map(|r| r.to_string()),
        }
    }

    pub(crate) fn draw_suggest(&mut self, ctx: &egui::Context, caret: egui::Pos2, m: &editor::Metrics) {
        use fractal_core::complete::Kind as K;
        let t = self.theme.clone();
        let u = &t.ui;
        let Some(sg) = &self.suggest else { return };
        let items = sg.items.clone();
        let prefix = sg.prefix.clone();
        let selected = sg.selected;
        let screen = ctx.content_rect();
        let row_h = 22.0;
        let max_rows = 9usize;
        let rows = items.len().min(max_rows);
        let first = if selected >= max_rows { selected + 1 - max_rows } else { 0 };
        let width = 440.0f32.min(screen.width() - 24.0);
        let height = rows as f32 * row_h + 8.0;
        let x = (caret.x - prefix.chars().count() as f32 * m.char_w - 26.0).clamp(screen.left() + 8.0, (screen.right() - width - 8.0).max(screen.left() + 8.0));
        let mut y = caret.y + m.line_h + 2.0;
        if y + height > screen.bottom() - 28.0 {
            y = caret.y - height - 2.0;
        }
        let surface = fractal_core::theme::mix(u.bg, u.fg, if t.dark { 0.07 } else { 0.0 });
        let mut accept: Option<usize> = None;
        let match_col = c(u.accent_text());
        egui::Area::new(egui::Id::new("suggest_popup")).order(egui::Order::Foreground).fixed_pos(egui::pos2(x, y)).show(ctx, |ui| {
            Frame::NONE
                .fill(c(surface))
                .stroke(Stroke::new(1.0, c(u.border)))
                .corner_radius(egui::CornerRadius::same(6))
                .shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(if t.dark { 130 } else { 40 }) })
                .inner_margin(Margin::same(4))
                .show(ui, |ui| {
                    ui.set_width(width - 8.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, it) in items.iter().enumerate().skip(first).take(rows) {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width - 8.0, row_h), Sense::click());
                        let sel = i == selected;
                        if sel {
                            ui.painter().rect_filled(rect, 4.0, c(u.selection));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 4.0, c(u.bg_hover));
                        }
                        // kind icon
                        let (icon, col) = match it.kind {
                            K::Lsp => kind_icon(it.lsp_kind, &t),
                            K::Keyword => (Icon::KindKeyword, c(u.fg_dim)),
                            K::Snippet => (Icon::KindSnippet, c(u.fg_dim)),
                            K::Word => (Icon::KindText, c(u.fg_dim)),
                        };
                        icons::draw(ui.painter(), icon, egui::pos2(rect.left() + 14.0, rect.center().y), 14.0, col);
                        // label with matched characters highlighted
                        let font = FontId::proportional(13.0);
                        let lp: Vec<char> = prefix.to_lowercase().chars().collect();
                        let mut job = egui::text::LayoutJob::default();
                        let mut li = 0usize;
                        for ch in it.label.chars() {
                            let hit = li < lp.len() && ch.to_lowercase().next() == Some(lp[li]);
                            if hit {
                                li += 1;
                            }
                            let mut fmt = egui::TextFormat { font_id: font.clone(), color: if hit { match_col } else { c(u.fg) }, ..Default::default() };
                            if it.deprecated {
                                fmt.strikethrough = Stroke::new(1.0, c(u.fg_dim));
                            }
                            job.append(&ch.to_string(), 0.0, fmt);
                        }
                        let g = ui.painter().layout_job(job);
                        let lx = rect.left() + 30.0;
                        let lw = g.size().x;
                        ui.painter().with_clip_rect(rect).galley(egui::pos2(lx, rect.center().y - g.size().y / 2.0), g, c(u.fg));
                        if !it.detail.is_empty() {
                            let dr = egui::Rect::from_min_max(egui::pos2(lx + lw + 14.0, rect.top()), egui::pos2(rect.right() - 8.0, rect.bottom()));
                            if dr.width() > 24.0 {
                                let dg = ui.painter().layout_no_wrap(it.detail.clone(), FontId::proportional(12.0), if sel { c(u.fg).gamma_multiply(0.75) } else { c(u.fg_dim) });
                                let dx = (dr.right() - dg.size().x).max(dr.left());
                                ui.painter().with_clip_rect(dr).galley(egui::pos2(dx, rect.center().y - dg.size().y / 2.0), dg, c(u.fg_dim));
                            }
                        }
                        if resp.clicked() {
                            accept = Some(i);
                        }
                    }
                });
        });
        if let Some(i) = accept {
            if let Some(sg) = &mut self.suggest {
                sg.selected = i;
            }
            self.exec(ctx, "acceptSuggestion");
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
                .fill(c(fractal_core::theme::mix(u.bg, u.fg, if t.dark { 0.06 } else { 0.0 })))
                .stroke(Stroke::new(1.0, crate::shell::alpha(u.fg, 22)))
                .corner_radius(egui::CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(if t.dark { 150 } else { 45 }) })
                .inner_margin(Margin::same(10))
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
                    .fill(c(fractal_core::theme::mix(u.bg, u.fg, if t.dark { 0.08 } else { 0.0 })).gamma_multiply(alpha))
                    .stroke(Stroke::new(1.0, crate::shell::alpha(u.fg, 22).gamma_multiply(alpha)))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(16, 12))
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

/// Icon + color for an LSP CompletionItemKind (same grouping as VS Code).
fn kind_icon(kind: u32, t: &fractal_core::theme::Theme) -> (Icon, Color32) {
    let purple = Color32::from_rgb(0xb1, 0x80, 0xd7);
    let blue = Color32::from_rgb(0x75, 0xbe, 0xff);
    let orange = Color32::from_rgb(0xee, 0x9d, 0x28);
    let dim = c(t.ui.fg_dim);
    match kind {
        2 | 3 | 4 => (Icon::KindMethod, purple),
        5 | 6 | 10 => (Icon::KindVariable, blue),
        7 | 8 | 13 | 22 | 25 => (Icon::KindClass, orange),
        9 | 17 | 19 => (Icon::KindModule, dim),
        12 | 20 | 21 => (Icon::KindConstant, blue),
        14 => (Icon::KindKeyword, dim),
        15 => (Icon::KindSnippet, dim),
        _ => (Icon::KindText, dim),
    }
}
