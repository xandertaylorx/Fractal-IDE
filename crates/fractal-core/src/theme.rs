//! Themes: built-in palettes plus import of VS Code color themes.

use std::str::FromStr;
use std::sync::Arc;
use syntect::highlighting::{Color as SColor, FontStyle, ScopeSelectors, StyleModifier, Theme as SyntectTheme, ThemeItem, ThemeSettings};

pub type Rgba = [u8; 4];

#[derive(Clone, Debug)]
pub struct UiColors {
    pub bg: Rgba,
    pub bg_side: Rgba,
    pub bg_panel: Rgba,
    pub bg_title: Rgba,
    pub bg_tab: Rgba,
    pub bg_tab_active: Rgba,
    pub bg_hover: Rgba,
    pub bg_input: Rgba,
    pub bg_status: Rgba,
    pub border: Rgba,
    pub fg: Rgba,
    pub fg_dim: Rgba,
    pub fg_faint: Rgba,
    pub fg_status: Rgba,
    pub accent: Rgba,
    pub accent_fg: Rgba,
    pub selection: Rgba,
    pub line_highlight: Rgba,
    pub cursor: Rgba,
    pub find_match: Rgba,
    pub error: Rgba,
    pub warning: Rgba,
    pub git_added: Rgba,
    pub git_modified: Rgba,
    pub git_deleted: Rgba,
}

#[derive(Clone)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    pub ui: UiColors,
    pub syntax: Arc<SyntectTheme>,
    pub default_fg: Rgba,
}

pub fn hex(s: &str) -> Rgba {
    parse_hex(s).unwrap_or([255, 0, 255, 255])
}

pub fn parse_hex(s: &str) -> Option<Rgba> {
    let s = s.trim().trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    match s.len() {
        3 => {
            let q = |i: usize| u8::from_str_radix(&s[i..i + 1].repeat(2), 16).ok();
            Some([q(0)?, q(1)?, q(2)?, 255])
        }
        6 => Some([p(0)?, p(2)?, p(4)?, 255]),
        8 => Some([p(0)?, p(2)?, p(4)?, p(6)?]),
        _ => None,
    }
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), 255]
}

fn with_alpha(c: Rgba, a: u8) -> Rgba {
    [c[0], c[1], c[2], a]
}

fn sc(c: Rgba) -> SColor {
    SColor { r: c[0], g: c[1], b: c[2], a: c[3] }
}

struct Palette {
    name: &'static str,
    dark: bool,
    bg: &'static str,
    side: &'static str,
    fg: &'static str,
    comment: &'static str,
    string: &'static str,
    number: &'static str,
    keyword: &'static str,
    func: &'static str,
    ty: &'static str,
    var: &'static str,
    op: &'static str,
    tag: &'static str,
    attr: &'static str,
    accent: &'static str,
    sel: &'static str,
}

#[rustfmt::skip]
const PALETTES: &[Palette] = &[
    Palette { name: "Fractal Dark", dark: true, bg: "#14161b", side: "#101216", fg: "#d4d7de", comment: "#5f6877", string: "#8fd694", number: "#f2a65a", keyword: "#b79cff", func: "#6cb6ff", ty: "#4fd1c5", var: "#d4d7de", op: "#89a0b8", tag: "#ff7b9c", attr: "#f2a65a", accent: "#8b7cff", sel: "#33306b" },
    Palette { name: "Fractal Light", dark: false, bg: "#fbfbfd", side: "#f1f2f6", fg: "#2b2f3a", comment: "#9aa1b0", string: "#2f8f4e", number: "#c2641b", keyword: "#6d4fd6", func: "#1f6fd1", ty: "#0e8a7f", var: "#2b2f3a", op: "#5c6b80", tag: "#d1365f", attr: "#b8601a", accent: "#6d4fd6", sel: "#d9d2ff" },
    Palette { name: "Midnight", dark: true, bg: "#000000", side: "#050507", fg: "#d0d0d0", comment: "#555b66", string: "#7fdc8a", number: "#ffb86b", keyword: "#c792ff", func: "#64b5ff", ty: "#5de4c7", var: "#d0d0d0", op: "#89a0b8", tag: "#ff6b9d", attr: "#ffb86b", accent: "#00d4ff", sel: "#1d2b4a" },
    Palette { name: "Tokyo Night", dark: true, bg: "#1a1b26", side: "#16161e", fg: "#a9b1d6", comment: "#565f89", string: "#9ece6a", number: "#ff9e64", keyword: "#bb9af7", func: "#7aa2f7", ty: "#2ac3de", var: "#c0caf5", op: "#89ddff", tag: "#f7768e", attr: "#e0af68", accent: "#7aa2f7", sel: "#283457" },
    Palette { name: "Catppuccin Mocha", dark: true, bg: "#1e1e2e", side: "#181825", fg: "#cdd6f4", comment: "#6c7086", string: "#a6e3a1", number: "#fab387", keyword: "#cba6f7", func: "#89b4fa", ty: "#f9e2af", var: "#cdd6f4", op: "#89dceb", tag: "#f38ba8", attr: "#f9e2af", accent: "#cba6f7", sel: "#45475a" },
    Palette { name: "Catppuccin Latte", dark: false, bg: "#eff1f5", side: "#e6e9ef", fg: "#4c4f69", comment: "#9ca0b0", string: "#40a02b", number: "#fe640b", keyword: "#8839ef", func: "#1e66f5", ty: "#df8e1d", var: "#4c4f69", op: "#04a5e5", tag: "#d20f39", attr: "#df8e1d", accent: "#8839ef", sel: "#ccd0da" },
    Palette { name: "Dracula", dark: true, bg: "#282a36", side: "#21222c", fg: "#f8f8f2", comment: "#6272a4", string: "#f1fa8c", number: "#bd93f9", keyword: "#ff79c6", func: "#50fa7b", ty: "#8be9fd", var: "#f8f8f2", op: "#ff79c6", tag: "#ff79c6", attr: "#50fa7b", accent: "#bd93f9", sel: "#44475a" },
    Palette { name: "Nord", dark: true, bg: "#2e3440", side: "#292e39", fg: "#d8dee9", comment: "#616e88", string: "#a3be8c", number: "#b48ead", keyword: "#81a1c1", func: "#88c0d0", ty: "#8fbcbb", var: "#d8dee9", op: "#81a1c1", tag: "#81a1c1", attr: "#8fbcbb", accent: "#88c0d0", sel: "#434c5e" },
    Palette { name: "One Dark", dark: true, bg: "#282c34", side: "#21252b", fg: "#abb2bf", comment: "#5c6370", string: "#98c379", number: "#d19a66", keyword: "#c678dd", func: "#61afef", ty: "#e5c07b", var: "#e06c75", op: "#56b6c2", tag: "#e06c75", attr: "#d19a66", accent: "#61afef", sel: "#3e4451" },
    Palette { name: "Gruvbox Dark", dark: true, bg: "#282828", side: "#1d2021", fg: "#ebdbb2", comment: "#928374", string: "#b8bb26", number: "#d3869b", keyword: "#fb4934", func: "#8ec07c", ty: "#fabd2f", var: "#83a598", op: "#fe8019", tag: "#fb4934", attr: "#fabd2f", accent: "#fe8019", sel: "#504945" },
    Palette { name: "Gruvbox Light", dark: false, bg: "#fbf1c7", side: "#ebdbb2", fg: "#3c3836", comment: "#928374", string: "#79740e", number: "#8f3f71", keyword: "#9d0006", func: "#427b58", ty: "#b57614", var: "#076678", op: "#af3a03", tag: "#9d0006", attr: "#b57614", accent: "#af3a03", sel: "#d5c4a1" },
    Palette { name: "Solarized Dark", dark: true, bg: "#002b36", side: "#00252e", fg: "#93a1a1", comment: "#586e75", string: "#2aa198", number: "#d33682", keyword: "#859900", func: "#268bd2", ty: "#b58900", var: "#839496", op: "#859900", tag: "#268bd2", attr: "#93a1a1", accent: "#268bd2", sel: "#073642" },
    Palette { name: "Solarized Light", dark: false, bg: "#fdf6e3", side: "#eee8d5", fg: "#586e75", comment: "#93a1a1", string: "#2aa198", number: "#d33682", keyword: "#859900", func: "#268bd2", ty: "#b58900", var: "#657b83", op: "#859900", tag: "#268bd2", attr: "#93a1a1", accent: "#268bd2", sel: "#eee8d5" },
    Palette { name: "GitHub Dark", dark: true, bg: "#0d1117", side: "#010409", fg: "#c9d1d9", comment: "#8b949e", string: "#a5d6ff", number: "#79c0ff", keyword: "#ff7b72", func: "#d2a8ff", ty: "#ffa657", var: "#c9d1d9", op: "#ff7b72", tag: "#7ee787", attr: "#79c0ff", accent: "#58a6ff", sel: "#1f3a5f" },
    Palette { name: "GitHub Light", dark: false, bg: "#ffffff", side: "#f6f8fa", fg: "#24292f", comment: "#6e7781", string: "#0a3069", number: "#0550ae", keyword: "#cf222e", func: "#8250df", ty: "#953800", var: "#24292f", op: "#cf222e", tag: "#116329", attr: "#0550ae", accent: "#0969da", sel: "#b6e3ff" },
    Palette { name: "Rosé Pine", dark: true, bg: "#191724", side: "#16141f", fg: "#e0def4", comment: "#6e6a86", string: "#f6c177", number: "#ebbcba", keyword: "#31748f", func: "#ebbcba", ty: "#9ccfd8", var: "#e0def4", op: "#908caa", tag: "#eb6f92", attr: "#c4a7e7", accent: "#c4a7e7", sel: "#403d52" },
    Palette { name: "Monokai", dark: true, bg: "#272822", side: "#1e1f1c", fg: "#f8f8f2", comment: "#75715e", string: "#e6db74", number: "#ae81ff", keyword: "#f92672", func: "#a6e22e", ty: "#66d9ef", var: "#f8f8f2", op: "#f92672", tag: "#f92672", attr: "#a6e22e", accent: "#a6e22e", sel: "#49483e" },
    Palette { name: "Ayu Mirage", dark: true, bg: "#1f2430", side: "#1a1f29", fg: "#cccac2", comment: "#707a8c", string: "#d5ff80", number: "#dfbfff", keyword: "#ffad66", func: "#ffd173", ty: "#73d0ff", var: "#cccac2", op: "#f29e74", tag: "#5ccfe6", attr: "#ffd173", accent: "#ffcc66", sel: "#33415e" },
    Palette { name: "Kanagawa", dark: true, bg: "#1f1f28", side: "#16161d", fg: "#dcd7ba", comment: "#727169", string: "#98bb6c", number: "#d27e99", keyword: "#957fb8", func: "#7e9cd8", ty: "#7aa89f", var: "#dcd7ba", op: "#c0a36e", tag: "#e6c384", attr: "#e6c384", accent: "#7e9cd8", sel: "#2d4f67" },
    Palette { name: "Synthwave '84", dark: true, bg: "#262335", side: "#1e1b2b", fg: "#f0eff1", comment: "#848bbd", string: "#ff8b39", number: "#f97e72", keyword: "#fede5d", func: "#36f9f6", ty: "#fe4450", var: "#ff7edb", op: "#fede5d", tag: "#72f1b8", attr: "#fede5d", accent: "#ff7edb", sel: "#463465" },
];

pub const DEFAULT_THEME: &str = "Fractal Dark";

pub fn builtin_names() -> Vec<&'static str> {
    PALETTES.iter().map(|p| p.name).collect()
}

fn item(scopes: &str, fg: Option<Rgba>, style: FontStyle) -> ThemeItem {
    ThemeItem {
        scope: ScopeSelectors::from_str(scopes).unwrap_or_default(),
        style: StyleModifier { foreground: fg.map(sc), background: None, font_style: if style.is_empty() { None } else { Some(style) } },
    }
}

fn build_syntect(name: &str, fg: Rgba, bg: Rgba, items: Vec<ThemeItem>) -> SyntectTheme {
    SyntectTheme {
        name: Some(name.to_string()),
        author: None,
        settings: ThemeSettings { foreground: Some(sc(fg)), background: Some(sc(bg)), ..Default::default() },
        scopes: items,
    }
}

pub fn builtin(name: &str) -> Option<Theme> {
    let p = PALETTES.iter().find(|p| p.name.eq_ignore_ascii_case(name))?;
    let (bg, side, fg) = (hex(p.bg), hex(p.side), hex(p.fg));
    let accent = hex(p.accent);
    let toward = if p.dark { [255, 255, 255, 255] } else { [0, 0, 0, 255] };
    let (comment, string, number, keyword, func, ty, var, op, tag, attr) =
        (hex(p.comment), hex(p.string), hex(p.number), hex(p.keyword), hex(p.func), hex(p.ty), hex(p.var), hex(p.op), hex(p.tag), hex(p.attr));
    let n = FontStyle::empty();
    let items = vec![
        item("comment, punctuation.definition.comment", Some(comment), FontStyle::ITALIC),
        item("string, punctuation.definition.string", Some(string), n),
        item("constant.numeric, constant.language, constant.character, constant.other", Some(number), n),
        item("constant.character.escape, string.regexp", Some(op), n),
        item("keyword, storage, keyword.control", Some(keyword), n),
        item("keyword.operator, punctuation.separator, punctuation.accessor", Some(op), n),
        item("entity.name.function, support.function, variable.function, meta.function-call", Some(func), n),
        item("entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, support.class, support.type, entity.other.inherited-class, storage.type.class", Some(ty), n),
        item("variable, meta.path", Some(var), n),
        item("variable.parameter", Some(var), FontStyle::ITALIC),
        item("variable.language, variable.other.constant", Some(keyword), n),
        item("entity.name.tag, meta.tag", Some(tag), n),
        item("entity.other.attribute-name, support.type.property-name, meta.object-literal.key", Some(attr), n),
        item("entity.name.macro, support.macro, meta.macro", Some(func), n),
        item("punctuation", Some(mix(fg, bg, 0.35)), n),
        item("invalid", Some(hex("#ff5370")), n),
        item("markup.heading, entity.name.section", Some(func), FontStyle::BOLD),
        item("markup.bold", Some(number), FontStyle::BOLD),
        item("markup.italic", Some(keyword), FontStyle::ITALIC),
        item("markup.underline.link, markup.raw", Some(string), n),
        item("markup.inserted", Some(hex("#7fd88a")), n),
        item("markup.deleted", Some(hex("#ff6b7a")), n),
    ];
    let ui = UiColors {
        bg,
        bg_side: side,
        bg_panel: mix(bg, side, 0.5),
        bg_title: side,
        bg_tab: mix(side, bg, 0.25),
        bg_tab_active: bg,
        bg_hover: mix(side, toward, 0.07),
        bg_input: mix(bg, toward, 0.06),
        bg_status: side,
        border: mix(bg, toward, 0.10),
        fg,
        fg_dim: mix(fg, bg, 0.38),
        fg_faint: mix(fg, bg, 0.62),
        fg_status: mix(fg, bg, 0.2),
        accent,
        accent_fg: if p.dark { mix(bg, [0, 0, 0, 255], 0.5) } else { [255, 255, 255, 255] },
        selection: hex(p.sel),
        line_highlight: mix(bg, toward, 0.045),
        cursor: accent,
        find_match: with_alpha(mix(accent, [255, 200, 0, 255], 0.5), 90),
        error: hex(if p.dark { "#f7768e" } else { "#d1365f" }),
        warning: hex(if p.dark { "#e0af68" } else { "#b8860b" }),
        git_added: hex(if p.dark { "#73c991" } else { "#2e8b57" }),
        git_modified: hex(if p.dark { "#e2c08d" } else { "#b8860b" }),
        git_deleted: hex(if p.dark { "#f0716f" } else { "#c0392b" }),
    };
    Some(Theme { name: p.name.to_string(), dark: p.dark, ui, syntax: Arc::new(build_syntect(p.name, fg, bg, items)), default_fg: fg })
}

/// Import a VS Code color theme (`.json`, comments allowed).
pub fn from_vscode_json(src: &str, fallback_name: &str) -> Option<Theme> {
    let v = crate::jsonc::parse(src)?;
    let name = v.get("name").and_then(|n| n.as_str()).unwrap_or(fallback_name).to_string();
    let dark = match v.get("type").and_then(|t| t.as_str()) {
        Some("light") | Some("hc-light") => false,
        _ => true,
    };
    let base = builtin(if dark { "Fractal Dark" } else { "Fractal Light" })?;
    let colors = v.get("colors").and_then(|c| c.as_object());
    let get = |keys: &[&str]| -> Option<Rgba> {
        let c = colors?;
        keys.iter().find_map(|k| c.get(*k).and_then(|x| x.as_str()).and_then(parse_hex))
    };
    let mut ui = base.ui.clone();
    let opaque = |c: Rgba, over: Rgba| -> Rgba {
        if c[3] == 255 {
            c
        } else {
            let a = c[3] as f32 / 255.0;
            mix(over, c, a)
        }
    };
    if let Some(c) = get(&["editor.background"]) {
        ui.bg = c;
        ui.bg_tab_active = c;
    }
    let bg = ui.bg;
    if let Some(c) = get(&["sideBar.background", "activityBar.background"]) {
        ui.bg_side = c;
        ui.bg_title = c;
        ui.bg_status = c;
    }
    if let Some(c) = get(&["titleBar.activeBackground"]) {
        ui.bg_title = c;
    }
    if let Some(c) = get(&["statusBar.background"]) {
        ui.bg_status = c;
    }
    if let Some(c) = get(&["panel.background"]) {
        ui.bg_panel = c;
    } else {
        ui.bg_panel = bg;
    }
    if let Some(c) = get(&["editorGroupHeader.tabsBackground", "tab.inactiveBackground"]) {
        ui.bg_tab = c;
    } else {
        ui.bg_tab = mix(ui.bg_side, bg, 0.3);
    }
    if let Some(c) = get(&["tab.activeBackground"]) {
        ui.bg_tab_active = c;
    }
    if let Some(c) = get(&["input.background"]) {
        ui.bg_input = c;
    }
    if let Some(c) = get(&["editor.foreground", "foreground"]) {
        ui.fg = c;
    }
    ui.fg_dim = mix(ui.fg, bg, 0.38);
    ui.fg_faint = mix(ui.fg, bg, 0.62);
    ui.fg_status = get(&["statusBar.foreground"]).unwrap_or(mix(ui.fg, bg, 0.2));
    if let Some(c) = get(&["focusBorder", "button.background", "activityBarBadge.background", "textLink.foreground"]) {
        ui.accent = c;
        ui.cursor = c;
    }
    if let Some(c) = get(&["editorCursor.foreground"]) {
        ui.cursor = c;
    }
    if let Some(c) = get(&["editor.selectionBackground"]) {
        ui.selection = opaque(c, bg);
    }
    if let Some(c) = get(&["editor.lineHighlightBackground"]) {
        ui.line_highlight = opaque(c, bg);
    }
    if let Some(c) = get(&["editorGroup.border", "panel.border", "sideBar.border"]) {
        ui.border = opaque(c, bg);
    }
    if let Some(c) = get(&["list.hoverBackground"]) {
        ui.bg_hover = opaque(c, ui.bg_side);
    }
    if let Some(c) = get(&["editorError.foreground"]) {
        ui.error = c;
    }
    if let Some(c) = get(&["editorWarning.foreground"]) {
        ui.warning = c;
    }
    if let Some(c) = get(&["gitDecoration.addedResourceForeground", "editorGutter.addedBackground"]) {
        ui.git_added = c;
    }
    if let Some(c) = get(&["gitDecoration.modifiedResourceForeground", "editorGutter.modifiedBackground"]) {
        ui.git_modified = c;
    }
    if let Some(c) = get(&["gitDecoration.deletedResourceForeground", "editorGutter.deletedBackground"]) {
        ui.git_deleted = c;
    }
    ui.accent_fg = if dark { mix(bg, [0, 0, 0, 255], 0.5) } else { [255, 255, 255, 255] };

    let mut items = vec![];
    if let Some(arr) = v.get("tokenColors").and_then(|t| t.as_array()) {
        for tc in arr {
            let scopes: Vec<String> = match tc.get("scope") {
                Some(serde_json::Value::String(s)) => vec![s.clone()],
                Some(serde_json::Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                _ => continue,
            };
            let settings = tc.get("settings");
            let fg = settings.and_then(|s| s.get("foreground")).and_then(|f| f.as_str()).and_then(parse_hex);
            let mut fs = FontStyle::empty();
            if let Some(f) = settings.and_then(|s| s.get("fontStyle")).and_then(|f| f.as_str()) {
                if f.contains("bold") {
                    fs |= FontStyle::BOLD;
                }
                if f.contains("italic") {
                    fs |= FontStyle::ITALIC;
                }
                if f.contains("underline") {
                    fs |= FontStyle::UNDERLINE;
                }
            }
            if fg.is_none() && fs.is_empty() {
                continue;
            }
            let joined = scopes.iter().flat_map(|s| s.split(',')).map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
            if joined.is_empty() {
                continue;
            }
            items.push(item(&joined, fg, fs));
        }
    }
    // Fall back to our own palette scopes when a theme has no token colors at all.
    let syntax = if items.is_empty() { base.syntax.clone() } else { Arc::new(build_syntect(&name, ui.fg, ui.bg, items)) };
    let default_fg = ui.fg;
    Some(Theme { name, dark, ui, syntax, default_fg })
}
