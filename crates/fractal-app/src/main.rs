//! Fractal IDE — entry point.
#![allow(deprecated)]

mod app;
mod commands;
mod editor;
mod explorer;
mod icons;
mod job;
mod overlay;
mod scm;
mod search_panel;
mod terminal;
mod ui;
mod ui_theme;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // GUI launches on macOS don't inherit the shell PATH; recover it before anything spawns.
    fractal_core::shellenv::import_login_path();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([760.0, 480.0])
        .with_title("Fractal")
        .with_fullsize_content_view(cfg!(target_os = "macos"))
        .with_title_shown(!cfg!(target_os = "macos"))
        .with_titlebar_shown(!cfg!(target_os = "macos"))
        .with_drag_and_drop(true);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Fractal", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
