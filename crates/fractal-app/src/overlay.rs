//! Quick-pick overlay: command palette, go to file, themes, branches …

use crate::ui_theme::c;
use eframe::egui::{self, Align2, FontId, Key, Modifiers, ScrollArea, Sense, TextEdit, Vec2};
use fractal_core::theme::Theme;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    Command(String),
    File(PathBuf),
    Theme(String),
    Run(usize),
    Branch(String),
    NewBranch(String),
    Folder(PathBuf),
    Line(usize),
    OpenTab(usize),
    Nothing,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    pub detail: String,
    pub right: String,
    pub payload: Payload,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    QuickOpen,
    Theme,
    Run,
    Branch,
    Recent,
}

pub struct Overlay {
    pub kind: Kind,
    pub input: String,
    pub selected: usize,
    pub items: Vec<Item>,
    pub commands: Vec<Item>,
    pub placeholder: String,
    visible: Vec<usize>,
    last_input: Option<String>,
    scroll_to: bool,
    first_frame: bool,
    pub original_theme: Option<String>,
    last_previewed: Option<usize>,
}

pub enum Outcome {
    None,
    Close,
    Accept(Payload, String),
    Preview(Payload),
}

impl Overlay {
    pub fn new(kind: Kind, input: &str, placeholder: &str, items: Vec<Item>, commands: Vec<Item>) -> Self {
        Self { kind, input: input.to_string(), selected: 0, items, commands, placeholder: placeholder.to_string(), visible: vec![], last_input: None, scroll_to: true, first_frame: true, original_theme: None, last_previewed: None }
    }

    fn refresh(&mut self) {
        let matcher = SkimMatcherV2::default();
        self.visible.clear();
        let (query, in_commands) = if self.kind == Kind::QuickOpen && self.input.starts_with('>') { (self.input[1..].trim().to_string(), true) } else { (self.input.trim().to_string(), false) };
        let list: &Vec<Item> = if in_commands { &self.commands } else { &self.items };
        if query.is_empty() {
            self.visible = (0..list.len().min(400)).collect();
        } else {
            let mut scored: Vec<(i64, usize)> = list
                .iter()
                .enumerate()
                .filter_map(|(i, it)| {
                    let name_score = matcher.fuzzy_match(&it.label, &query).map(|s| s * 2);
                    let full = matcher.fuzzy_match(&format!("{} {}", it.label, it.detail), &query);
                    name_score.or(full).map(|s| (s, i))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.visible = scored.into_iter().take(400).map(|(_, i)| i).collect();
        }
        if self.selected >= self.visible.len() {
            self.selected = 0;
        }
        self.scroll_to = true;
    }

    fn current_list(&self) -> &Vec<Item> {
        if self.kind == Kind::QuickOpen && self.input.starts_with('>') {
            &self.commands
        } else {
            &self.items
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, theme: &Theme, keybinding: &dyn Fn(&str) -> Option<String>) -> Outcome {
        let u = &theme.ui;
        let mut outcome = Outcome::None;
        if self.last_input.as_deref() != Some(&self.input) {
            self.last_input = Some(self.input.clone());
            self.refresh();
        }
        // go-to-line mode: ":12"
        let goto_line = if self.kind == Kind::QuickOpen { self.input.strip_prefix(':').map(|s| s.trim().parse::<usize>().ok()) } else { None };

        let (down, up, enter, esc) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Escape)));
        if down && !self.visible.is_empty() {
            self.selected = (self.selected + 1) % self.visible.len();
            self.scroll_to = true;
        }
        if up && !self.visible.is_empty() {
            self.selected = (self.selected + self.visible.len() - 1) % self.visible.len();
            self.scroll_to = true;
        }
        if esc {
            return Outcome::Close;
        }

        let screen = ctx.content_rect();
        let width = 620.0f32.min(screen.width() - 40.0);
        let pos = egui::pos2(screen.center().x - width / 2.0, screen.top() + 54.0);
        egui::Area::new(egui::Id::new("overlay")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::NONE
                .fill(c(u.bg_side))
                .stroke(egui::Stroke::new(1.0, c(u.border)))
                .corner_radius(egui::CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 14], blur: 40, spread: 0, color: egui::Color32::from_black_alpha(if theme.dark { 170 } else { 60 }) })
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(width - 16.0);
                    let te = ui.add_sized([width - 16.0, 34.0], TextEdit::singleline(&mut self.input).hint_text(&self.placeholder).font(FontId::proportional(15.0)).margin(egui::Margin::symmetric(10, 8)).lock_focus(true));
                    if self.first_frame {
                        te.request_focus();
                        self.first_frame = false;
                    }
                    ui.add_space(6.0);
                    if let Some(g) = goto_line {
                        let msg = match g {
                            Some(n) => format!("Press Enter to go to line {n}"),
                            None => "Type a line number".to_string(),
                        };
                        ui.label(egui::RichText::new(msg).color(c(u.fg_dim)));
                        if enter {
                            if let Some(n) = g {
                                outcome = Outcome::Accept(Payload::Line(n.saturating_sub(1)), self.input.clone());
                            }
                        }
                        return;
                    }
                    let row_h = 30.0;
                    let n = self.visible.len();
                    let list_h = (n as f32 * row_h).min(row_h * 10.0);
                    let list = self.current_list().clone();
                    if n == 0 {
                        ui.label(egui::RichText::new("No matching results").color(c(u.fg_dim)));
                    }
                    ScrollArea::vertical().max_height(list_h).auto_shrink([false, true]).show_rows(ui, row_h, n, |ui, range| {
                        for vi in range {
                            let it = &list[self.visible[vi]];
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                            let sel = vi == self.selected;
                            if sel {
                                ui.painter().rect_filled(rect, 6.0, c(u.selection));
                            } else if resp.hovered() {
                                ui.painter().rect_filled(rect, 6.0, c(u.bg_hover));
                            }
                            let mut tx = rect.left() + 12.0;
                            if let Payload::File(p) = &it.payload {
                                crate::icons::draw_file(ui.painter(), crate::icons::file_icon_key(p, false, false), egui::pos2(tx + 8.0, rect.center().y), 16.0);
                                tx += 26.0;
                            }
                            let g = ui.painter().text(egui::pos2(tx, rect.center().y), Align2::LEFT_CENTER, &it.label, FontId::proportional(13.5), c(u.fg));
                            if !it.detail.is_empty() {
                                ui.painter().with_clip_rect(rect).text(egui::pos2(g.right() + 10.0, rect.center().y), Align2::LEFT_CENTER, &it.detail, FontId::proportional(12.0), c(u.fg_faint));
                            }
                            let kb = if it.right.is_empty() { if let Payload::Command(id) = &it.payload { keybinding(id).unwrap_or_default() } else { String::new() } } else { it.right.clone() };
                            if !kb.is_empty() {
                                ui.painter().text(egui::pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, kb, FontId::proportional(12.0), c(u.fg_dim));
                            }
                            if sel && self.scroll_to {
                                ui.scroll_to_rect(rect, None);
                            }
                            if resp.clicked() {
                                outcome = Outcome::Accept(it.payload.clone(), self.input.clone());
                            }
                            if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                                self.selected = vi;
                            }
                        }
                    });
                    self.scroll_to = false;
                });
        });
        if enter {
            if let Some(&i) = self.visible.get(self.selected) {
                outcome = Outcome::Accept(self.current_list()[i].payload.clone(), self.input.clone());
            } else if self.kind == Kind::Branch && !self.input.trim().is_empty() {
                outcome = Outcome::Accept(Payload::NewBranch(self.input.trim().to_string()), self.input.clone());
            }
        }
        if matches!(outcome, Outcome::None) && self.kind == Kind::Theme && self.last_previewed != Some(self.selected) {
            self.last_previewed = Some(self.selected);
            if let Some(&i) = self.visible.get(self.selected) {
                outcome = Outcome::Preview(self.items[i].payload.clone());
            }
        }
        outcome
    }
}
