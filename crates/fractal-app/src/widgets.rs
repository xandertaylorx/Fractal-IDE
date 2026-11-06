//! Small reusable widgets.

use crate::icons::{self, Icon};
use crate::ui_theme::c;
use eframe::egui::{self, Align2, FontId, Response, Sense, Ui, Vec2};
use fractal_core::theme::Theme;

pub fn icon_button(ui: &mut Ui, theme: &Theme, icon: Icon, tip: &str, active: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if resp.hovered() || active {
        ui.painter().rect_filled(rect.shrink(1.0), 6.0, if active { c(theme.ui.bg_hover) } else { c(theme.ui.bg_hover).gamma_multiply(0.8) });
    }
    let col = if active || resp.hovered() { c(theme.ui.fg) } else { c(theme.ui.fg_dim) };
    icons::draw(ui.painter(), icon, rect.center(), size * 0.55, col);
    resp.on_hover_text(tip)
}

pub fn small_toggle(ui: &mut Ui, theme: &Theme, icon: Icon, tip: &str, on: &mut bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    if *on {
        ui.painter().rect_filled(rect, 4.0, c(theme.ui.accent).gamma_multiply(0.35));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, c(theme.ui.bg_hover));
    }
    icons::draw(ui.painter(), icon, rect.center(), 16.0, if *on { c(theme.ui.fg) } else { c(theme.ui.fg_dim) });
    resp.on_hover_text(tip)
}

pub fn section_header(ui: &mut Ui, _theme: &Theme, _title: &str) {
    // the sidebar's text tabs already name the view
    ui.add_space(2.0);
}

/// A filled accent button.
pub fn accent_button(ui: &mut Ui, theme: &Theme, text: &str, enabled: bool) -> Response {
    let font = FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, c(theme.ui.accent_fg));
    let size = Vec2::new(ui.available_width().max(galley.size().x + 24.0), 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let mut col = c(theme.ui.accent);
    if !enabled {
        col = col.gamma_multiply(0.4);
    } else if resp.hovered() {
        col = col.gamma_multiply(1.12);
    }
    ui.painter().rect_filled(rect, 6.0, col);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, c(theme.ui.accent_fg));
    resp
}

pub fn ghost_button(ui: &mut Ui, theme: &Theme, text: &str) -> Response {
    let font = FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, c(theme.ui.fg));
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 20.0, 26.0), Sense::click());
    let bg = if resp.hovered() { c(theme.ui.bg_hover) } else { c(theme.ui.bg_input) };
    ui.painter().rect_filled(rect, 6.0, bg);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, c(theme.ui.fg));
    resp
}
