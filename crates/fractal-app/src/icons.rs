//! Vector icons drawn with the egui painter (no font or SVG dependency).

use eframe::egui::{epaint::PathShape, pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

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
}

fn line(p: &Painter, a: Pos2, b: Pos2, s: Stroke) {
    p.line_segment([a, b], s);
}

pub fn draw(p: &Painter, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let u = size / 16.0;
    let s = Stroke::new((1.4 * u).max(1.0), color);
    let c = center;
    let pt = |x: f32, y: f32| pos2(c.x + (x - 8.0) * u, c.y + (y - 8.0) * u);
    match icon {
        Icon::Files => {
            p.rect_stroke(Rect::from_two_pos(pt(4.0, 3.0), pt(11.0, 12.5)), 1.5 * u, s, eframe::egui::StrokeKind::Middle);
            line(p, pt(6.0, 1.5), pt(13.0, 1.5), s);
            line(p, pt(13.0, 1.5), pt(13.0, 10.0), s);
        }
        Icon::Search => {
            p.circle_stroke(pt(7.0, 7.0), 4.2 * u, s);
            line(p, pt(10.2, 10.2), pt(14.0, 14.0), s);
        }
        Icon::Git | Icon::Branch => {
            line(p, pt(5.0, 3.5), pt(5.0, 12.5), s);
            p.circle_stroke(pt(5.0, 3.0), 1.7 * u, s);
            p.circle_stroke(pt(5.0, 13.0), 1.7 * u, s);
            p.circle_stroke(pt(11.5, 5.5), 1.7 * u, s);
            p.add(Shape::Path(PathShape::line(vec![pt(11.5, 7.2), pt(11.5, 8.0), pt(5.0, 10.0)], s)));
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![pt(4.5, 2.5), pt(13.0, 8.0), pt(4.5, 13.5)], color, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_two_pos(pt(3.5, 3.5), pt(12.5, 12.5)), 2.0 * u, color);
        }
        Icon::Gear => {
            p.circle_stroke(pt(8.0, 8.0), 2.3 * u, s);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (sn, cs) = a.sin_cos();
                line(p, pt(8.0 + cs * 4.6, 8.0 + sn * 4.6), pt(8.0 + cs * 6.4, 8.0 + sn * 6.4), Stroke::new((2.0 * u).max(1.2), color));
            }
            p.circle_stroke(pt(8.0, 8.0), 4.6 * u, s);
        }
        Icon::Terminal => {
            p.rect_stroke(Rect::from_two_pos(pt(1.5, 2.5), pt(14.5, 13.5)), 2.0 * u, s, eframe::egui::StrokeKind::Middle);
            p.add(Shape::Path(PathShape::line(vec![pt(4.5, 6.0), pt(7.0, 8.0), pt(4.5, 10.0)], s)));
            line(p, pt(8.5, 10.5), pt(11.5, 10.5), s);
        }
        Icon::Palette => {
            p.circle_stroke(pt(8.0, 8.0), 6.2 * u, s);
            for (x, y) in [(5.0, 6.5), (8.0, 4.5), (11.0, 6.5), (6.0, 10.5)] {
                p.circle_filled(pt(x, y), 1.1 * u, color);
            }
        }
        Icon::ChevronRight => {
            p.add(Shape::Path(PathShape::line(vec![pt(6.0, 3.5), pt(10.5, 8.0), pt(6.0, 12.5)], s)));
        }
        Icon::ChevronDown => {
            p.add(Shape::Path(PathShape::line(vec![pt(3.5, 6.0), pt(8.0, 10.5), pt(12.5, 6.0)], s)));
        }
        Icon::Up => {
            p.add(Shape::Path(PathShape::line(vec![pt(3.5, 10.0), pt(8.0, 5.5), pt(12.5, 10.0)], s)));
        }
        Icon::Down => {
            p.add(Shape::Path(PathShape::line(vec![pt(3.5, 6.0), pt(8.0, 10.5), pt(12.5, 6.0)], s)));
        }
        Icon::Close => {
            line(p, pt(4.0, 4.0), pt(12.0, 12.0), s);
            line(p, pt(12.0, 4.0), pt(4.0, 12.0), s);
        }
        Icon::Plus => {
            line(p, pt(8.0, 3.0), pt(8.0, 13.0), s);
            line(p, pt(3.0, 8.0), pt(13.0, 8.0), s);
        }
        Icon::Minus => line(p, pt(3.0, 8.0), pt(13.0, 8.0), s),
        Icon::Refresh => {
            let mut pts = vec![];
            for i in 0..=18 {
                let a = (i as f32 / 18.0) * 4.9 - 0.6;
                pts.push(pt(8.0 + a.cos() * 5.0, 8.0 + a.sin() * 5.0));
            }
            let last = *pts.last().unwrap();
            p.add(Shape::Path(PathShape::line(pts, s)));
            line(p, last, pt(last_x(last, c, u) + 0.0, 4.0), s);
            line(p, last, pos2(last.x + 3.0 * u, last.y - 0.5 * u), s);
        }
        Icon::Check => {
            p.add(Shape::Path(PathShape::line(vec![pt(3.0, 8.5), pt(6.5, 12.0), pt(13.0, 4.5)], s)));
        }
        Icon::Folder | Icon::FolderOpen => {
            let pts = vec![pt(1.5, 4.0), pt(6.0, 4.0), pt(7.5, 5.5), pt(14.5, 5.5), pt(14.5, 12.5), pt(1.5, 12.5)];
            p.add(Shape::closed_line(pts, s));
        }
        Icon::File => {
            p.add(Shape::closed_line(vec![pt(3.5, 1.5), pt(9.5, 1.5), pt(12.5, 4.5), pt(12.5, 14.5), pt(3.5, 14.5)], s));
            p.add(Shape::Path(PathShape::line(vec![pt(9.5, 1.5), pt(9.5, 4.5), pt(12.5, 4.5)], s)));
        }
        Icon::Sidebar => {
            p.rect_stroke(Rect::from_two_pos(pt(1.5, 2.5), pt(14.5, 13.5)), 2.0 * u, s, eframe::egui::StrokeKind::Middle);
            line(p, pt(6.0, 2.5), pt(6.0, 13.5), s);
        }
        Icon::Panel => {
            p.rect_stroke(Rect::from_two_pos(pt(1.5, 2.5), pt(14.5, 13.5)), 2.0 * u, s, eframe::egui::StrokeKind::Middle);
            line(p, pt(1.5, 9.5), pt(14.5, 9.5), s);
        }
        Icon::Undo => {
            p.add(Shape::Path(PathShape::line(vec![pt(5.5, 3.5), pt(2.5, 6.5), pt(5.5, 9.5)], s)));
            p.add(Shape::Path(PathShape::line(vec![pt(2.5, 6.5), pt(9.5, 6.5), pt(13.0, 9.0), pt(9.5, 12.5), pt(5.0, 12.5)], s)));
        }
        Icon::CaseSensitive => {
            p.add(Shape::Path(PathShape::line(vec![pt(1.5, 12.0), pt(5.0, 3.5), pt(8.5, 12.0)], s)));
            line(p, pt(2.8, 9.0), pt(7.2, 9.0), s);
            p.circle_stroke(pt(11.7, 9.8), 2.0 * u, s);
            line(p, pt(13.7, 7.5), pt(13.7, 12.0), s);
        }
        Icon::WholeWord => {
            line(p, pt(1.5, 13.0), pt(14.5, 13.0), s);
            line(p, pt(1.5, 13.0), pt(1.5, 11.0), s);
            line(p, pt(14.5, 13.0), pt(14.5, 11.0), s);
            p.circle_stroke(pt(5.2, 7.0), 2.0 * u, s);
            line(p, pt(7.2, 5.0), pt(7.2, 9.0), s);
            line(p, pt(10.0, 3.5), pt(10.0, 9.0), s);
        }
        Icon::Regex => {
            line(p, pt(10.0, 3.0), pt(10.0, 9.0), s);
            line(p, pt(7.0, 4.5), pt(13.0, 7.5), s);
            line(p, pt(13.0, 4.5), pt(7.0, 7.5), s);
            p.circle_filled(pt(4.0, 11.5), 1.3 * u, color);
        }
        Icon::Trash => {
            line(p, pt(2.5, 4.0), pt(13.5, 4.0), s);
            p.add(Shape::Path(PathShape::line(vec![pt(4.0, 4.0), pt(4.8, 13.5), pt(11.2, 13.5), pt(12.0, 4.0)], s)));
            line(p, pt(6.5, 2.0), pt(9.5, 2.0), s);
        }
        Icon::Globe => {
            p.circle_stroke(pt(8.0, 8.0), 6.2 * u, s);
            p.add(Shape::Path(PathShape::line((0..=12).map(|i| { let a = i as f32 / 12.0 * std::f32::consts::PI; pt(8.0 + 3.0 * a.sin() * 0.0 + 0.0, 1.8 + a.cos().mul_add(-6.2, 6.2)) }).collect(), Stroke::NONE)));
            line(p, pt(1.8, 8.0), pt(14.2, 8.0), s);
            p.add(Shape::Path(PathShape::line(vec![pt(8.0, 1.8), pt(5.5, 5.0), pt(5.5, 11.0), pt(8.0, 14.2)], s)));
            p.add(Shape::Path(PathShape::line(vec![pt(8.0, 1.8), pt(10.5, 5.0), pt(10.5, 11.0), pt(8.0, 14.2)], s)));
        }
        Icon::Command => {
            p.rect_stroke(Rect::from_two_pos(pt(5.0, 5.0), pt(11.0, 11.0)), 0.5 * u, s, eframe::egui::StrokeKind::Middle);
            for (x, y) in [(3.5, 3.5), (12.5, 3.5), (3.5, 12.5), (12.5, 12.5)] {
                p.circle_stroke(pt(x, y), 1.8 * u, s);
            }
        }
    }
}

fn last_x(last: Pos2, _c: Pos2, _u: f32) -> f32 {
    last.x
}

/// A small file-type color used for tree/tab dots.
pub fn file_color(ext: &str) -> Color32 {
    match ext {
        "rs" => Color32::from_rgb(0xde, 0xa5, 0x84),
        "py" => Color32::from_rgb(0x4b, 0x8b, 0xbe),
        "js" | "mjs" | "cjs" | "jsx" => Color32::from_rgb(0xf1, 0xe0, 0x5a),
        "ts" | "tsx" => Color32::from_rgb(0x31, 0x78, 0xc6),
        "java" | "jar" => Color32::from_rgb(0xe7, 0x6f, 0x00),
        "html" | "htm" => Color32::from_rgb(0xe3, 0x4c, 0x26),
        "css" | "scss" | "less" => Color32::from_rgb(0x56, 0x3d, 0x7c),
        "json" | "jsonc" => Color32::from_rgb(0xcb, 0xcb, 0x41),
        "md" | "markdown" => Color32::from_rgb(0x51, 0x9a, 0xba),
        "go" => Color32::from_rgb(0x00, 0xad, 0xd8),
        "c" | "h" | "cpp" | "cc" | "hpp" => Color32::from_rgb(0x59, 0x9e, 0xff),
        "toml" | "yaml" | "yml" | "ini" => Color32::from_rgb(0x9c, 0x4a, 0x4a),
        "sh" | "bash" | "zsh" => Color32::from_rgb(0x89, 0xe0, 0x51),
        "rb" => Color32::from_rgb(0xcc, 0x34, 0x2d),
        "php" => Color32::from_rgb(0x77, 0x7b, 0xb4),
        "swift" => Color32::from_rgb(0xf0, 0x51, 0x38),
        "kt" => Color32::from_rgb(0xa9, 0x7b, 0xff),
        "svg" | "png" | "jpg" | "jpeg" | "gif" | "webp" => Color32::from_rgb(0xa0, 0x74, 0xc4),
        _ => Color32::from_gray(140),
    }
}

pub fn v(x: f32, y: f32) -> eframe::egui::Vec2 {
    vec2(x, y)
}
