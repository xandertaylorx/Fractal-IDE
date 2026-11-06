//! Bridge from the core `Theme` to egui visuals, plus font setup.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};
use fractal_core::theme::{Rgba, Theme};
use std::sync::Arc;

pub fn c(x: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(x[0], x[1], x[2], x[3])
}

pub fn apply(ctx: &egui::Context, t: &Theme) {
    let u = &t.ui;
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(c(u.fg));
    v.panel_fill = c(u.bg);
    v.window_fill = c(u.bg_side);
    v.extreme_bg_color = c(u.bg_input);
    v.faint_bg_color = c(u.bg_hover);
    v.window_stroke = Stroke::new(1.0, c(u.border));
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.hyperlink_color = c(u.accent);
    v.selection.bg_fill = c(u.selection);
    v.selection.stroke = Stroke::new(1.0, c(u.accent));
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if t.dark { 140 } else { 50 }) };
    v.popup_shadow = v.window_shadow;
    let r = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c(u.bg_side);
    w.noninteractive.bg_stroke = Stroke::new(1.0, c(u.border));
    w.noninteractive.fg_stroke = Stroke::new(1.0, c(u.fg));
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = c(u.bg_input);
    w.inactive.weak_bg_fill = c(u.bg_input);
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, c(u.fg));
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = c(u.bg_hover);
    w.hovered.weak_bg_fill = c(u.bg_hover);
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.0, c(u.fg));
    w.hovered.corner_radius = r;
    w.active.bg_fill = c(u.selection);
    w.active.weak_bg_fill = c(u.selection);
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0, c(u.fg));
    w.active.corner_radius = r;
    w.open.bg_fill = c(u.bg_hover);
    w.open.weak_bg_fill = c(u.bg_hover);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;
        s.spacing.scroll.floating_width = 4.0;
        s.spacing.scroll.foreground_color = false;
        s.interaction.selectable_labels = false;
    });
}

fn first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    for p in paths {
        if let Ok(b) = std::fs::read(p) {
            return Some(b);
        }
    }
    None
}

pub fn setup_fonts(ctx: &egui::Context, preferred_mono: &str) {
    let mut fonts = FontDefinitions::default();
    let home = std::env::var("HOME").unwrap_or_default();
    let mut mono_candidates: Vec<String> = vec![];
    if !preferred_mono.is_empty() {
        mono_candidates.push(preferred_mono.to_string());
    }
    for n in ["JetBrainsMono-Regular.ttf", "JetBrainsMonoNL-Regular.ttf", "FiraCode-Regular.ttf", "CascadiaCode.ttf", "SourceCodePro-Regular.ttf", "Hack-Regular.ttf"] {
        mono_candidates.push(format!("{home}/Library/Fonts/{n}"));
        mono_candidates.push(format!("/Library/Fonts/{n}"));
        mono_candidates.push(format!("/usr/share/fonts/truetype/{n}"));
    }
    mono_candidates.extend(
        [
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/SFNSMono.ttf",
            "C:\\Windows\\Fonts\\consola.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        ]
        .map(String::from),
    );
    let refs: Vec<&str> = mono_candidates.iter().map(|s| s.as_str()).collect();
    if let Some(bytes) = first_existing(&refs) {
        fonts.font_data.insert("fractal-mono".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "fractal-mono".into());
    }
    let ui_candidates = ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc", "C:\\Windows\\Fonts\\segoeui.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];
    // The UI font is optional: keep egui's default if it can't be loaded.
    if let Some(bytes) = first_existing(&ui_candidates) {
        fonts.font_data.insert("fractal-ui".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "fractal-ui".into());
    }
    ctx.set_fonts(fonts);
}
