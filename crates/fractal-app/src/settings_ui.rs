//! The graphical Settings page (left-click on the gear / ⌘,).

use crate::app::App;
use crate::commands;
use crate::ui_theme::{self, c, medium, semibold};
use eframe::egui::{self, Align, Align2, FontId, Layout, Sense, Stroke, TextEdit, Ui, Vec2};
use fractal_core::theme::Theme;

fn matches(filter: &str, title: &str, desc: &str) -> bool {
    filter.is_empty() || title.to_lowercase().contains(filter) || desc.to_lowercase().contains(filter)
}

/// Pill switch. Returns true when toggled.
fn switch(ui: &mut Ui, t: &Theme, on: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(38.0, 22.0), Sense::click());
    let mut changed = false;
    if resp.clicked() {
        *on = !*on;
        changed = true;
    }
    let anim = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let bg = if *on { c(t.ui.accent) } else { c(t.ui.fg_faint).gamma_multiply(0.45) };
    ui.painter().rect_filled(rect, 11.0, bg);
    let knob_x = rect.left() + 11.0 + anim * 16.0;
    ui.painter().circle_filled(egui::pos2(knob_x, rect.center().y), 8.0, if *on { c(t.ui.accent_fg) } else { c(t.ui.fg) });
    changed
}

fn row(ui: &mut Ui, t: &Theme, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(Vec2::new((w - 240.0).max(160.0), 0.0), Layout::top_down(Align::LEFT), |ui| {
            ui.label(egui::RichText::new(title).size(13.5).family(medium()).color(c(t.ui.fg)));
            ui.add_space(2.0);
            ui.label(egui::RichText::new(desc).size(12.0).color(c(t.ui.fg_faint)));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
    let r = ui.add_space(0.0);
    let _ = r;
}

fn divider(ui: &mut Ui, t: &Theme) {
    ui.add_space(10.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, crate::shell::alpha(t.ui.fg, 12));
    ui.add_space(10.0);
}

fn heading(ui: &mut Ui, t: &Theme, text: &str) {
    ui.add_space(30.0);
    ui.label(egui::RichText::new(text.to_uppercase()).size(10.5).family(medium()).color(c(t.ui.accent_text())));
    ui.add_space(8.0);
}

impl App {
    pub fn draw_settings_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let t = self.theme.clone();
        let theme_names: Vec<(String, bool)> = self.themes.iter().map(|x| (x.name.clone(), x.dark)).collect();
        let mut dirty = false;
        let mut new_theme: Option<String> = None;
        let mut reload_fonts = false;
        let mut open_json = false;
        let mut open_keys = false;
        let mut import_theme = false;
        let avail = ui.available_width();
        let col_w = (avail - 64.0).clamp(380.0, 780.0);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(col_w);
                ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                    ui.add_space(26.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Settings").size(26.0).family(semibold()).color(c(t.ui.fg)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if crate::widgets::ghost_button(ui, &t, "Open settings.json").clicked() {
                                open_json = true;
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.add_sized([col_w, 32.0], TextEdit::singleline(&mut self.settings_filter).hint_text("Search settings").margin(egui::Margin::symmetric(12, 8)));
                    let f = self.settings_filter.trim().to_lowercase();

                    // ---------------- Appearance ----------------
                    let show_appearance = ["theme", "zoom", "font", "minimap", "line numbers", "cursor", "appearance"].iter().any(|k| f.is_empty() || k.contains(&f) || f.contains(k));
                    if show_appearance {
                        heading(ui, &t, "Appearance");
                    }
                    if matches(&f, "Color Theme", "Choose how Fractal looks. VS Code themes can be imported.") {
                        row(ui, &t, "Color Theme", "Choose how Fractal looks. Includes the official VS Code themes.", |ui| {
                            if crate::widgets::ghost_button(ui, &t, "Import…").clicked() {
                                import_theme = true;
                            }
                            egui::ComboBox::from_id_salt("settings_theme").width(180.0).selected_text(self.theme.name.clone()).show_ui(ui, |ui| {
                                for (name, dark) in &theme_names {
                                    let label = format!("{name}   {}", if *dark { "dark" } else { "light" });
                                    if ui.selectable_label(*name == self.theme.name, label).clicked() {
                                        new_theme = Some(name.clone());
                                    }
                                }
                            });
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Interface Zoom", "Scale the whole interface (⌘+ / ⌘−).") {
                        row(ui, &t, "Interface Zoom", "Scale the whole interface (⌘+ / ⌘−).", |ui| {
                            dirty |= ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.6..=2.5).step_by(0.1)).changed();
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Editor Font Size", "Size of code text in points.") {
                        row(ui, &t, "Editor Font Size", "Size of code text in points.", |ui| {
                            dirty |= ui.add(egui::Slider::new(&mut self.settings.font_size, 9.0..=26.0).step_by(0.5)).changed();
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Editor Font Path", "Optional path to a monospace .ttf/.otf (e.g. JetBrains Mono). Empty = system default.") {
                        row(ui, &t, "Editor Font File", "Path to a monospace .ttf/.otf (e.g. JetBrains Mono). Empty uses a system font.", |ui| {
                            let r = ui.add(TextEdit::singleline(&mut self.settings.font_family).desired_width(210.0).hint_text("/path/to/font.ttf"));
                            if r.lost_focus() && r.changed() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                dirty = true;
                                reload_fonts = true;
                            }
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Minimap", "Show a zoomed-out overview on the right edge of the editor.") {
                        row(ui, &t, "Minimap", "Show a zoomed-out overview on the right edge of the editor.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.minimap);
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Line Numbers", "Show line numbers in the gutter.") {
                        row(ui, &t, "Line Numbers", "Show line numbers in the gutter.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.line_numbers);
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Cursor Blinking", "Blink the text cursor.") {
                        row(ui, &t, "Cursor Blinking", "Blink the text cursor.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.cursor_blink);
                        });
                    }

                    // ---------------- Editor ----------------
                    let show_editor = ["tab", "indent", "save", "format", "suggest", "complete", "ghost", "editor"].iter().any(|k| f.is_empty() || k.contains(&f) || f.contains(k));
                    if show_editor {
                        heading(ui, &t, "Editor");
                    }
                    if matches(&f, "Smart Suggestions", "Show completions as you type. Press Tab to accept.") {
                        row(ui, &t, "Smart Suggestions", "Completions from your code, keywords and snippets as you type. Tab accepts.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.suggestions);
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Inline Ghost Text", "Preview the top suggestion in dim text after the cursor.") {
                        row(ui, &t, "Inline Ghost Text", "Preview the top suggestion in dim text after the cursor.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.ghost_text);
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Format on Save", "Format the document automatically when saving (⌘⇧F formats on demand).") {
                        row(ui, &t, "Format on Save", "Format the document automatically when saving. ⌘⇧F formats on demand.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.format_on_save);
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Auto Save", "Save files automatically shortly after you stop typing.") {
                        row(ui, &t, "Auto Save", "Save files automatically shortly after you stop typing.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.auto_save);
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Tab Size", "Number of spaces a tab is worth for new files.") {
                        row(ui, &t, "Tab Size", "Spaces per indent level for new files (existing files keep their style).", |ui| {
                            for n in [8usize, 4, 2] {
                                if ui.selectable_label(self.settings.tab_size == n, format!(" {n} ")).clicked() {
                                    self.settings.tab_size = n;
                                    dirty = true;
                                }
                            }
                        });
                        divider(ui, &t);
                    }
                    if matches(&f, "Insert Spaces", "Use spaces instead of tab characters for new files.") {
                        row(ui, &t, "Insert Spaces", "Use spaces instead of tab characters for new files.", |ui| {
                            dirty |= switch(ui, &t, &mut self.settings.insert_spaces);
                        });
                    }

                    // ---------------- Terminal ----------------
                    if matches(&f, "Terminal Font Size", "terminal") {
                        heading(ui, &t, "Terminal");
                        row(ui, &t, "Terminal Font Size", "Text size in the integrated terminal.", |ui| {
                            dirty |= ui.add(egui::Slider::new(&mut self.settings.terminal_font_size, 9.0..=24.0).step_by(0.5)).changed();
                        });
                    }

                    // ---------------- Keyboard ----------------
                    heading(ui, &t, "Keyboard Shortcuts");
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("VS Code compatible. Customize in keybindings.json.").size(12.0).color(c(t.ui.fg_dim)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if crate::widgets::ghost_button(ui, &t, "Open keybindings.json").clicked() {
                                open_keys = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let mut shown = 0;
                    let mut seen = std::collections::HashSet::new();
                    for b in self.keymap.bindings.iter().rev() {
                        if shown >= 120 {
                            break;
                        }
                        let title = commands::title_of(&b.command).map(|s| s.to_string()).unwrap_or_else(|| b.command.clone());
                        let keys = b.keys.iter().map(|k| k.label(self.mac)).collect::<Vec<_>>().join(" ");
                        let key = format!("{}|{}", b.command, keys);
                        if !seen.insert(key) {
                            continue;
                        }
                        if !f.is_empty() && !title.to_lowercase().contains(&f) && !keys.to_lowercase().contains(&f) && !b.command.to_lowercase().contains(&f) {
                            continue;
                        }
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
                        ui.painter().text(egui::pos2(rect.left() + 2.0, rect.center().y), Align2::LEFT_CENTER, title, FontId::proportional(13.0), c(t.ui.fg));
                        let g = ui.painter().layout_no_wrap(keys, FontId::proportional(12.0), c(t.ui.fg));
                        let chip = egui::Rect::from_center_size(egui::pos2(rect.right() - g.size().x / 2.0 - 10.0, rect.center().y), Vec2::new(g.size().x + 16.0, 20.0));
                        ui.painter().rect_filled(chip, 5.0, c(t.ui.bg_input));
                        ui.painter().rect_stroke(chip, 5.0, Stroke::new(1.0, c(t.ui.border)), egui::StrokeKind::Inside);
                        ui.painter().galley(egui::pos2(chip.left() + 8.0, chip.center().y - g.size().y / 2.0), g, c(t.ui.fg));
                        shown += 1;
                    }
                    ui.add_space(40.0);
                });
            });
        });

        if let Some(n) = new_theme {
            self.set_theme(ctx, &n);
            self.settings.theme = n;
            dirty = true;
        }
        if reload_fonts {
            ui_theme::setup_fonts(ctx, &self.settings.font_family);
        }
        if dirty {
            self.settings.save();
        }
        if open_json {
            self.exec(ctx, "workbench.action.openSettingsJson");
        }
        if open_keys {
            self.exec(ctx, "workbench.action.openGlobalKeybindings");
        }
        if import_theme {
            self.exec(ctx, "fractal.importTheme");
        }
    }
}
