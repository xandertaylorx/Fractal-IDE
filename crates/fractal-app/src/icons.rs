//! Icons: Lucide (UI) and Material Icon Theme (file types), rasterized from
//! embedded SVG with resvg and cached as GPU textures.

use crate::assets;
use eframe::egui::{self, pos2, vec2, Color32, ColorImage, Painter, Pos2, Rect, TextureHandle, TextureOptions};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Files,
    Search,
    Git,
    Play,
    Stop,
    Gear,
    Terminal,
    Palette,
    ChevronRight,
    ChevronDown,
    Close,
    Plus,
    Refresh,
    Check,
    Folder,
    FolderOpen,
    File,
    FilePlus,
    FolderPlus,
    Sidebar,
    Panel,
    Minus,
    Undo,
    Branch,
    Up,
    Down,
    CaseSensitive,
    WholeWord,
    Regex,
    Trash,
    Globe,
    Command,
    Commit,
    Ellipsis,
    Error,
    Warning,
    Info,
    KindMethod,
    KindVariable,
    KindClass,
    KindKeyword,
    KindModule,
    KindConstant,
    KindText,
    KindSnippet,
}

impl Icon {
    fn name(self) -> &'static str {
        match self {
            Icon::Files => "files",
            Icon::Search => "search",
            Icon::Git | Icon::Branch => "git-branch",
            Icon::Play => "play",
            Icon::Stop => "square",
            Icon::Gear => "settings",
            Icon::Terminal => "terminal",
            Icon::Palette => "palette",
            Icon::ChevronRight => "chevron-right",
            Icon::ChevronDown | Icon::Down => "chevron-down",
            Icon::Up => "chevron-up",
            Icon::Close => "x",
            Icon::Plus => "plus",
            Icon::Refresh => "refresh-cw",
            Icon::Check => "check",
            Icon::Folder => "folder",
            Icon::FolderOpen => "folder-open",
            Icon::File => "file",
            Icon::FilePlus => "file-plus",
            Icon::FolderPlus => "folder-plus",
            Icon::Sidebar => "panel-left",
            Icon::Panel => "panel-bottom",
            Icon::Minus => "minus",
            Icon::Undo => "undo-2",
            Icon::CaseSensitive => "case-sensitive",
            Icon::WholeWord => "whole-word",
            Icon::Regex => "regex",
            Icon::Trash => "trash-2",
            Icon::Globe => "globe",
            Icon::Command => "command",
            Icon::Commit => "git-commit-horizontal",
            Icon::Ellipsis => "ellipsis",
            Icon::Error => "circle-x",
            Icon::Warning => "triangle-alert",
            Icon::Info => "info",
            Icon::KindMethod => "box",
            Icon::KindVariable => "variable",
            Icon::KindClass => "braces",
            Icon::KindKeyword => "key-round",
            Icon::KindModule => "package",
            Icon::KindConstant => "hash",
            Icon::KindText => "text",
            Icon::KindSnippet => "square-dashed",
        }
    }

    fn filled(self) -> bool {
        matches!(self, Icon::Play | Icon::Stop)
    }
}

thread_local! {
    static CACHE: RefCell<HashMap<(String, u32), TextureHandle>> = RefCell::new(HashMap::new());
}

fn rasterize(svg: &str, px: u32) -> Option<ColorImage> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let scale = px as f32 / tree.size().width().max(1.0);
    let mut pm = tiny_skia::Pixmap::new(px, px)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
    Some(ColorImage::from_rgba_premultiplied([px as usize, px as usize], pm.data()))
}

fn texture(ctx: &egui::Context, key: &str, px: u32, make_svg: impl FnOnce() -> Option<String>) -> Option<egui::TextureId> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(t) = c.get(&(key.to_string(), px)) {
            return Some(t.id());
        }
        let svg = make_svg()?;
        let img = rasterize(&svg, px)?;
        let h = ctx.load_texture(format!("{key}@{px}"), img, TextureOptions::LINEAR);
        let id = h.id();
        c.insert((key.to_string(), px), h);
        Some(id)
    })
}

fn paint(p: &Painter, tex: egui::TextureId, center: Pos2, size: f32, tint: Color32) {
    p.image(tex, Rect::from_center_size(center, vec2(size, size)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
}

/// Draw a monochrome UI icon tinted with `color`.
pub fn draw(p: &Painter, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let ppp = p.ctx().pixels_per_point();
    let px = ((size * ppp).round() as u32).clamp(8, 256);
    let name = icon.name();
    let filled = icon.filled();
    let tex = texture(p.ctx(), &format!("ui:{name}:{filled}"), px, || {
        let mut svg = assets::ui_svg(name)?.replace("currentColor", "#ffffff").replace("stroke-width=\"2\"", "stroke-width=\"1.8\"");
        if filled {
            svg = svg.replace("fill=\"none\"", "fill=\"#ffffff\"");
        }
        Some(svg)
    });
    if let Some(t) = tex {
        paint(p, t, center, size, color);
    }
}

/// Draw a colored file-type (or folder) icon.
pub fn draw_file(p: &Painter, key: &str, center: Pos2, size: f32) {
    let ppp = p.ctx().pixels_per_point();
    let px = ((size * ppp).round() as u32).clamp(8, 256);
    let tex = texture(p.ctx(), &format!("file:{key}"), px, || assets::file_svg(key).or_else(|| assets::file_svg("document")).map(String::from));
    if let Some(t) = tex {
        paint(p, t, center, size, Color32::WHITE);
    }
}

/// Pick the Material icon for a path.
pub fn file_icon_key(path: &Path, is_dir: bool, open: bool) -> &'static str {
    if is_dir {
        return if open { "folder-base-open" } else { "folder-base" };
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" | "docker-compose.yml" | "docker-compose.yaml" | ".dockerignore" => return "docker",
        "makefile" | "gnumakefile" => return "makefile",
        "package.json" => return "nodejs",
        "package-lock.json" => return "npm",
        "yarn.lock" => return "yarn",
        "pnpm-lock.yaml" => return "pnpm",
        "cargo.lock" | "poetry.lock" | "gemfile.lock" | "composer.lock" => return "lock",
        "tsconfig.json" | "jsconfig.json" => return "tsconfig",
        "pom.xml" => return "maven",
        "build.gradle" | "build.gradle.kts" | "settings.gradle" | "gradlew" => return "gradle",
        "cmakelists.txt" => return "cmake",
        ".gitignore" | ".gitattributes" | ".gitmodules" => return "git",
        "license" | "license.md" | "license.txt" | "copying" => return "license",
        ".prettierrc" | ".prettierignore" => return "prettier",
        _ => {}
    }
    if name.starts_with("readme") {
        return "readme";
    }
    if name.starts_with(".eslintrc") || name.starts_with("eslint.config") {
        return "eslint";
    }
    if name.starts_with("vite.config") {
        return "vite";
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "py" | "pyw" | "pyi" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "react",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "react_ts",
        "java" | "jar" | "class" => "java",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "sass",
        "less" => "less",
        "json" | "jsonc" | "json5" => "json",
        "md" | "markdown" | "mdx" => "markdown",
        "rs" => "rust",
        "go" => "go",
        "c" => "c",
        "h" => "h",
        "cpp" | "cc" | "cxx" => "cpp",
        "hpp" | "hh" | "hxx" => "hpp",
        "cs" => "csharp",
        "php" => "php",
        "rb" => "ruby",
        "swift" => "swift",
        "yaml" | "yml" => "yaml",
        "toml" | "ini" | "cfg" | "conf" | "env" => "settings",
        "sh" | "bash" | "zsh" | "fish" => "console",
        "ps1" => "powershell",
        "bat" | "cmd" => "console",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "avif" => "image",
        "svg" => "svg",
        "lock" => "lock",
        "xml" | "xsl" | "plist" => "xml",
        "lua" => "lua",
        "dart" => "dart",
        "vue" => "vue",
        "svelte" => "svelte",
        "sql" | "db" | "sqlite" => "database",
        "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" => "zip",
        "pdf" => "pdf",
        "hs" => "haskell",
        "ex" | "exs" => "elixir",
        "r" => "r",
        "jl" => "julia",
        "zig" => "zig",
        "pl" | "pm" => "perl",
        "gradle" => "gradle",
        "diff" | "patch" => "git",
        _ => "document",
    }
}

pub fn v(x: f32, y: f32) -> egui::Vec2 {
    vec2(x, y)
}
