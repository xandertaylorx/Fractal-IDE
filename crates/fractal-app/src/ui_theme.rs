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
    v.panel_fill = c(u.bg_base);
    v.window_fill = c(u.bg_side);
    v.extreme_bg_color = c(u.bg_input);
    v.faint_bg_color = c(u.bg_hover);
    v.window_stroke = Stroke::new(1.0, c(u.border));
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.hyperlink_color = c(u.accent);
    v.selection.bg_fill = c(u.selection);
    v.selection.stroke = Stroke::new(1.0, c(u.accent));
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if t.dark { 140 } else { 50 }) };
    v.popup_shadow = v.window_shadow;
    let r = CornerRadius::same(7);
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
    w.inactive.fg_stroke = Stroke::new(1.0, c(u.fg));
    w.hovered.weak_bg_fill = c(u.bg_hover);
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.0, c(u.fg));
    w.hovered.corner_radius = r;
    w.active.bg_fill = c(u.accent);
    w.active.weak_bg_fill = c(u.selection);
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0, c(u.fg));
    w.active.corner_radius = r;
    w.open.bg_fill = c(u.bg_hover);
    w.open.weak_bg_fill = c(u.bg_hover);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.visuals.slider_trailing_fill = true;
        s.visuals.handle_shape = egui::style::HandleShape::Circle;
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

const GEIST: &[u8] = include_bytes!("../assets/fonts/Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Geist-Medium.ttf");
const GEIST_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Geist-SemiBold.ttf");
const GEIST_MONO: &[u8] = include_bytes!("../assets/fonts/GeistMono-Regular.ttf");

/// Family for headings / emphasis.
pub fn medium() -> FontFamily {
    FontFamily::Name("geist-medium".into())
}
pub fn semibold() -> FontFamily {
    FontFamily::Name("geist-semibold".into())
}

pub fn setup_fonts(ctx: &egui::Context, preferred_mono: &str) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("geist".into(), Arc::new(FontData::from_static(GEIST)));
    fonts.font_data.insert("geist-medium".into(), Arc::new(FontData::from_static(GEIST_MEDIUM)));
    fonts.font_data.insert("geist-semibold".into(), Arc::new(FontData::from_static(GEIST_SEMIBOLD)));
    fonts.font_data.insert("geist-mono".into(), Arc::new(FontData::from_static(GEIST_MONO)));
    // system UI font only as a fallback for symbols (⌘ ⇧ …) that Geist lacks
    if let Some(bytes) = first_existing(&["/System/Library/Fonts/SFNS.ttf", "C:\\Windows\\Fonts\\segoeui.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]) {
        fonts.font_data.insert("sys-ui".into(), Arc::new(FontData::from_owned(bytes)));
    }
    let has_sys = fonts.font_data.contains_key("sys-ui");
    let mut prop = vec!["geist".to_string()];
    if has_sys {
        prop.push("sys-ui".into());
    }
    let defaults_prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    prop.extend(defaults_prop);
    fonts.families.insert(FontFamily::Proportional, prop.clone());
    for (name, key) in [("geist-medium", "geist-medium"), ("geist-semibold", "geist-semibold")] {
        let mut fam = vec![key.to_string()];
        fam.extend(prop.iter().filter(|f| *f != "geist").cloned());
        fonts.families.insert(FontFamily::Name(name.into()), fam);
    }
    // monospace: user's font file first, then Geist Mono
    let mut mono: Vec<String> = vec![];
    if !preferred_mono.is_empty() {
        if let Ok(bytes) = std::fs::read(preferred_mono) {
            fonts.font_data.insert("user-mono".into(), Arc::new(FontData::from_owned(bytes)));
            mono.push("user-mono".into());
        }
    }
    mono.push("geist-mono".into());
    let defaults_mono = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    mono.extend(defaults_mono);
    fonts.families.insert(FontFamily::Monospace, mono);
    ctx.set_fonts(fonts);
}
