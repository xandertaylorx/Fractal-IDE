//! User settings and session persistence.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub font_size: f32,
    pub ui_scale: f32,
    pub tab_size: usize,
    pub insert_spaces: bool,
    pub auto_save: bool,
    pub minimap: bool,
    pub line_numbers: bool,
    pub cursor_blink: bool,
    pub terminal_font_size: f32,
    pub font_family: String,
    pub format_on_save: bool,
    pub suggestions: bool,
    pub ghost_text: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: crate::theme::DEFAULT_THEME.into(),
            font_size: 13.5,
            ui_scale: 1.0,
            tab_size: 4,
            insert_spaces: true,
            auto_save: false,
            minimap: true,
            line_numbers: true,
            cursor_blink: true,
            terminal_font_size: 13.0,
            font_family: String::new(),
            format_on_save: false,
            suggestions: true,
            ghost_text: true,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub folder: Option<PathBuf>,
    pub open_files: Vec<PathBuf>,
    pub active: usize,
    pub recent_folders: Vec<PathBuf>,
    pub sidebar_width: f32,
    pub panel_height: f32,
    pub sidebar_visible: Option<bool>,
}

pub fn config_dir() -> PathBuf {
    let d = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Fractal");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn keybindings_path() -> PathBuf {
    config_dir().join("keybindings.json")
}

pub fn themes_dir() -> PathBuf {
    let d = config_dir().join("themes");
    let _ = std::fs::create_dir_all(&d);
    d
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|s| crate::jsonc::parse(&s))
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(settings_path(), s);
        }
    }
}

impl Session {
    fn path() -> PathBuf {
        config_dir().join("session.json")
    }
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), s);
        }
    }
}

pub const DEFAULT_KEYBINDINGS: &str = "// Place your key bindings here. Same format as VS Code's keybindings.json.\n// Example: { \"key\": \"cmd+shift+d\", \"command\": \"editor.action.deleteLines\", \"when\": \"editorFocus\" }\n[\n]\n";
