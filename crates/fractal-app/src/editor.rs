//! The code editor view: rendering and mouse handling for one buffer.

use crate::ui_theme::c;
use eframe::egui::{self, text::LayoutJob, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, TextFormat, Ui, Vec2};
use fractal_core::buffer::{Buffer, Selection};
use fractal_core::git::LineMark;
use fractal_core::syntax::Highlighter2;
use fractal_core::theme::Theme;
use std::time::{Duration, Instant};

pub struct Tab {
    pub buf: Buffer,
    pub hl: Highlighter2,
    pub scroll_y: f32,
    pub scroll_x: f32,
    pub marks: Vec<(usize, LineMark)>,
    pub reveal: bool,
    pub title_override: Option<String>,
    pub drag_anchor: Option<usize>,
    pub last_edit: Instant,
    pub last_version_seen: u64,
    pub viewport_lines: usize,
    pub untitled_n: usize,
    pub pending_goto: Option<(usize, usize, usize)>,
}

impl Tab {
    pub fn new(buf: Buffer, theme: &Theme) -> Self {
        let hl = Highlighter2::new(&buf, theme);
        let v = buf.version;
        Self {
            buf,
            hl,
            scroll_y: 0.0,
            scroll_x: 0.0,
            marks: vec![],
            reveal: false,
            title_override: None,
            drag_anchor: None,
            last_edit: Instant::now(),
            last_version_seen: v,
            viewport_lines: 40,
            untitled_n: 0,
            pending_goto: None,
        }
    }

    pub fn title(&self) -> String {
        if let Some(t) = &self.title_override {
            return t.clone();
        }
        match &self.buf.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            None => format!("Untitled-{}", self.untitled_n),
        }
    }

    /// Keep the highlighter consistent with edits.
    pub fn sync(&mut self) {
        if let Some(l) = self.buf.take_dirty_line() {
            self.hl.invalidate_from(l);
            self.last_edit = Instant::now();
        }
        if self.buf.version != self.last_version_seen {
            self.last_version_seen = self.buf.version;
            self.reveal = true;
        }
    }
}

pub struct Metrics {
    pub char_w: f32,
    pub line_h: f32,
    pub font: FontId,
}

pub fn metrics(ctx: &egui::Context, size: f32) -> Metrics {
    let font = FontId::monospace(size);
    let char_w = ctx.fonts_mut(|f| f.glyph_width(&font, 'M')).max(1.0);
    Metrics { char_w, line_h: (size * 1.6).round(), font }
}

pub fn char_col_to_vis(line: &str, col: usize, tab: usize) -> usize {
    if !line.contains('\t') {
        return col;
    }
    let mut v = 0;
    for (i, ch) in line.chars().enumerate() {
        if i >= col {
            break;
        }
        v += if ch == '\t' { tab - v % tab } else { 1 };
    }
    v
}

pub fn vis_to_char_col(line: &str, vis: usize, tab: usize) -> usize {
    if !line.contains('\t') {
        return vis.min(line.chars().count());
    }
    let mut v = 0;
    let mut n = 0;
    for ch in line.chars() {
        let w = if ch == '\t' { tab - v % tab } else { 1 };
        if vis < v + w.div_ceil(2) {
            return n;
        }
        v += w;
        n += 1;
    }
    n
}

pub struct EditorEnv<'a> {
    pub theme: &'a Theme,
    pub metrics: &'a Metrics,
    pub minimap: bool,
    pub line_numbers: bool,
    pub focused: bool,
    pub caret_visible: bool,
    pub find_matches: &'a [(usize, usize)],
    pub find_current: Option<usize>,
}

#[derive(Default)]
pub struct EditorResult {
    pub pressed: bool,
}

const GUTTER_PAD: f32 = 12.0;

pub fn show(ui: &mut Ui, tab: &mut Tab, env: &EditorEnv) -> EditorResult {
    let mut result = EditorResult::default();
    let ctx = ui.ctx().clone();
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
    let m = env.metrics;
    let ui_c = &env.theme.ui;
    let tab_w = tab.buf.indent.size.max(1);
    let n_lines = tab.buf.line_count();
    let painter = ui.painter_at(rect);

    // --- layout -------------------------------------------------------
    let digits = n_lines.to_string().len().max(3);
    let gutter_w = if env.line_numbers { digits as f32 * m.char_w + GUTTER_PAD * 2.0 + 4.0 } else { 14.0 };
    let minimap_w = if env.minimap && rect.width() > 520.0 { 96.0 } else { 0.0 };
    let sb_w = 10.0;
    let text_left = rect.left() + gutter_w;
    let text_right = rect.right() - minimap_w - sb_w;
    let text_w = (text_right - text_left).max(10.0);
    let view_h = rect.height();
    tab.viewport_lines = ((view_h / m.line_h) as usize).max(1);
    let max_scroll_y = ((n_lines.saturating_sub(1)) as f32 * m.line_h).max(0.0);

    // --- scroll input -------------------------------------------------
    if resp.hovered() || tab.drag_anchor.is_some() {
        let d = ctx.input(|i| i.smooth_scroll_delta());
        tab.scroll_y -= d.y;
        tab.scroll_x -= d.x;
    }
    // --- reveal caret -------------------------------------------------
    if tab.reveal {
        let head = tab.buf.primary().head;
        let (l, col) = tab.buf.char_to_pos(head);
        let line_txt = tab.buf.line_string(l);
        let vis = char_col_to_vis(&line_txt, col, tab_w);
        let y = l as f32 * m.line_h;
        let margin = m.line_h * 2.0;
        if y < tab.scroll_y + margin {
            tab.scroll_y = (y - margin).max(0.0);
        } else if y + m.line_h > tab.scroll_y + view_h - margin {
            tab.scroll_y = y + m.line_h - view_h + margin;
        }
        let x = vis as f32 * m.char_w;
        if x < tab.scroll_x + m.char_w * 3.0 {
            tab.scroll_x = (x - m.char_w * 6.0).max(0.0);
        } else if x + m.char_w > tab.scroll_x + text_w - m.char_w * 3.0 {
            tab.scroll_x = x + m.char_w * 6.0 - text_w;
        }
        tab.reveal = false;
    }
    tab.scroll_y = tab.scroll_y.clamp(0.0, max_scroll_y);
    tab.scroll_x = tab.scroll_x.max(0.0);

    let first_line = (tab.scroll_y / m.line_h).floor() as usize;
    let last_line = (((tab.scroll_y + view_h) / m.line_h).ceil() as usize + 1).min(n_lines);

    // --- highlighting (time-boxed) -------------------------------------
    tab.sync();
    let done = tab.hl.ensure(&tab.buf, last_line, Duration::from_millis(6));
    if !done {
        ctx.request_repaint();
    }

    // --- mouse ---------------------------------------------------------
    let pos_to_char = |buf: &Buffer, p: Pos2, scroll_y: f32, scroll_x: f32| -> usize {
        let ly = ((p.y - rect.top() + scroll_y) / m.line_h).floor().max(0.0) as usize;
        let line = ly.min(buf.line_count().saturating_sub(1));
        let vis = ((p.x - text_left + scroll_x) / m.char_w + 0.5).floor().max(0.0) as usize;
        let text = buf.line_string(line);
        let col = vis_to_char_col(&text, vis, tab_w);
        buf.pos_to_char(line, col)
    };
    let text_rect = Rect::from_min_max(egui::pos2(rect.left(), rect.top()), egui::pos2(text_right, rect.bottom()));
    let (pressed, down, pointer, mods, dbl, tpl) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.latest_pos(),
            i.modifiers,
            i.pointer.button_double_clicked(egui::PointerButton::Primary),
            i.pointer.button_triple_clicked(egui::PointerButton::Primary),
        )
    });
    if let Some(p) = pointer {
        if text_rect.contains(p) && resp.hovered() {
            ctx.set_cursor_icon(if p.x < text_left { CursorIcon::Default } else { CursorIcon::Text });
        }
        if pressed && text_rect.contains(p) && resp.hovered() {
            result.pressed = true;
            let idx = pos_to_char(&tab.buf, p, tab.scroll_y, tab.scroll_x);
            if tpl {
                tab.buf.select_line_at(idx);
                tab.drag_anchor = None;
            } else if dbl {
                let (a, b) = tab.buf.word_range_at(idx);
                tab.buf.set_selection(a, b);
                tab.drag_anchor = Some(a);
            } else if mods.alt {
                tab.buf.add_caret(idx);
                tab.drag_anchor = Some(idx);
            } else if mods.shift {
                let a = tab.buf.primary().anchor;
                tab.buf.set_selection(a, idx);
                tab.drag_anchor = Some(a);
            } else if p.x < text_left && env.line_numbers {
                // click in gutter selects the whole line
                tab.buf.select_line_at(idx);
                tab.drag_anchor = None;
            } else {
                tab.buf.set_single_caret(idx);
                tab.drag_anchor = Some(idx);
            }
            tab.reveal = false;
        } else if down && tab.drag_anchor.is_some() {
            let anchor = tab.drag_anchor.unwrap();
            // autoscroll while dragging outside
            if p.y < rect.top() {
                tab.scroll_y = (tab.scroll_y - 12.0).max(0.0);
                ctx.request_repaint();
            } else if p.y > rect.bottom() {
                tab.scroll_y = (tab.scroll_y + 12.0).min(max_scroll_y);
                ctx.request_repaint();
            }
            let idx = pos_to_char(&tab.buf, p, tab.scroll_y, tab.scroll_x);
            let n = tab.buf.sels.len();
            if n > 1 && mods.alt {
                tab.buf.sels[n - 1] = Selection { anchor, head: idx };
            } else {
                tab.buf.set_selection(anchor, idx);
            }
        }
    }
    if !down {
        tab.drag_anchor = None;
    }

    // --- painting ------------------------------------------------------
    painter.rect_filled(rect, 0.0, c(ui_c.bg));
    let text_clip = Rect::from_min_max(egui::pos2(text_left, rect.top()), egui::pos2(text_right, rect.bottom()));
    let tp = painter.with_clip_rect(text_clip);
    let y_of = |line: usize| rect.top() + line as f32 * m.line_h - tab.scroll_y;
    let x_of = |vis: usize| text_left + vis as f32 * m.char_w - tab.scroll_x;

    // current line highlight
    let primary = tab.buf.primary();
    let (pl, pc) = tab.buf.char_to_pos(primary.head);
    if primary.is_empty() {
        let y = y_of(pl);
        painter.rect_filled(Rect::from_min_size(egui::pos2(text_left, y), Vec2::new(text_w + sb_w, m.line_h)), 0.0, c(ui_c.line_highlight));
    }

    // char range visible
    let vis_start = tab.buf.line_start(first_line);
    let vis_end = if last_line < n_lines { tab.buf.line_start(last_line) } else { tab.buf.len_chars() };

    let paint_range = |a: usize, b: usize, color: Color32, extend_eol: bool| {
        let (a, b) = (a.max(vis_start), b.min(vis_end));
        if a > b {
            return;
        }
        let (l1, c1) = tab.buf.char_to_pos(a);
        let (l2, c2) = tab.buf.char_to_pos(b);
        for l in l1..=l2.min(last_line.saturating_sub(1)) {
            let text = tab.buf.line_string(l);
            let s = if l == l1 { char_col_to_vis(&text, c1, tab_w) } else { 0 };
            let e_char = if l == l2 { c2 } else { text.chars().count() };
            let mut e = char_col_to_vis(&text, e_char, tab_w);
            if l != l2 && extend_eol {
                e += 1;
            }
            if e > s {
                let r = Rect::from_min_max(egui::pos2(x_of(s), y_of(l)), egui::pos2(x_of(e), y_of(l) + m.line_h));
                tp.rect_filled(r, 0.0, color);
            }
        }
    };

    // find matches
    if !env.find_matches.is_empty() {
        let start_i = env.find_matches.partition_point(|&(_, b)| b < vis_start);
        for (i, &(a, b)) in env.find_matches.iter().enumerate().skip(start_i) {
            if a > vis_end {
                break;
            }
            let cur = env.find_current == Some(i);
            paint_range(a, b, if cur { c(ui_c.find_match).gamma_multiply(2.6) } else { c(ui_c.find_match) }, false);
        }
    }
    // selections
    for s in &tab.buf.sels {
        if !s.is_empty() {
            paint_range(s.start(), s.end(), c(ui_c.selection), true);
        }
    }

    // indent guides + text
    let first_vis_col = (tab.scroll_x / m.char_w).floor().max(0.0) as usize;
    let last_vis_col = first_vis_col + (text_w / m.char_w).ceil() as usize + 2;
    let default_fg = c(env.theme.default_fg);
    let guide_col = c(ui_c.border);
    for line in first_line..last_line {
        let y = y_of(line);
        let text = tab.buf.line_string(line);
        // indent guides
        let lead = text.chars().take_while(|ch| *ch == ' ' || *ch == '\t').count();
        let lead_vis = char_col_to_vis(&text, lead, tab_w);
        if lead_vis > 0 && !text.trim().is_empty() {
            let mut g = tab_w;
            while g < lead_vis + 0 {
                let x = x_of(g.saturating_sub(0)) - 0.5;
                tp.line_segment([egui::pos2(x, y), egui::pos2(x, y + m.line_h)], egui::Stroke::new(1.0, guide_col));
                g += tab_w;
            }
        }
        if text.is_empty() {
            continue;
        }
        let spans = tab.hl.spans(line);
        let mut job = LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        let mut sp_i = 0usize;
        let mut vis = 0usize;
        let mut run = String::new();
        let mut run_fmt: Option<(Color32, bool)> = None;
        let mut started = false;
        let mut start_vis = 0usize;
        let flush = |job: &mut LayoutJob, run: &mut String, fmt: Option<(Color32, bool)>, font: &FontId| {
            if run.is_empty() {
                return;
            }
            let (color, italics) = fmt.unwrap_or((default_fg, false));
            job.append(run, 0.0, TextFormat { font_id: font.clone(), color, italics, ..Default::default() });
            run.clear();
        };
        for (byte, ch) in text.char_indices() {
            let w = if ch == '\t' { tab_w - vis % tab_w } else { 1 };
            if vis >= last_vis_col {
                break;
            }
            if vis + w > first_vis_col {
                if !started {
                    started = true;
                    start_vis = vis;
                }
                let fmt = match spans {
                    Some(sp) => {
                        while sp_i < sp.len() && (sp[sp_i].end as usize) <= byte {
                            sp_i += 1;
                        }
                        match sp.get(sp_i) {
                            Some(s) if (s.start as usize) <= byte => Some((Color32::from_rgb(s.color[0], s.color[1], s.color[2]), s.italic)),
                            _ => Some((default_fg, false)),
                        }
                    }
                    None => Some((default_fg, false)),
                };
                if fmt != run_fmt {
                    flush(&mut job, &mut run, run_fmt, &m.font);
                    run_fmt = fmt;
                }
                if ch == '\t' {
                    for _ in 0..w {
                        run.push(' ');
                    }
                } else {
                    run.push(ch);
                }
            } else if spans.is_some() {
                while sp_i < spans.unwrap().len() && (spans.unwrap()[sp_i].end as usize) <= byte {
                    sp_i += 1;
                }
            }
            vis += w;
        }
        flush(&mut job, &mut run, run_fmt, &m.font);
        if !job.sections.is_empty() {
            let galley = tp.layout_job(job);
            let gy = y + (m.line_h - galley.size().y) / 2.0;
            tp.galley(egui::pos2(x_of(start_vis), gy), galley, default_fg);
        }
    }

    // carets
    if env.focused && env.caret_visible {
        for s in &tab.buf.sels {
            let (l, col) = tab.buf.char_to_pos(s.head);
            if l >= first_line && l < last_line {
                let text = tab.buf.line_string(l);
                let vis = char_col_to_vis(&text, col, tab_w);
                let x = x_of(vis);
                tp.rect_filled(Rect::from_min_size(egui::pos2(x, y_of(l) + 1.0), Vec2::new(2.0, m.line_h - 2.0)), 1.0, c(ui_c.cursor));
            }
        }
    }
    let _ = pc;

    // gutter
    painter.rect_filled(Rect::from_min_max(rect.min, egui::pos2(text_left, rect.bottom())), 0.0, c(ui_c.bg));
    let gp = painter.with_clip_rect(Rect::from_min_max(rect.min, egui::pos2(text_left, rect.bottom())));
    let small = FontId::monospace((m.font.size - 1.5).max(9.0));
    if env.line_numbers {
        for line in first_line..last_line {
            let active = tab.buf.sels.iter().any(|s| tab.buf.char_to_pos(s.head).0 == line);
            let col = if active { c(ui_c.fg) } else { c(ui_c.fg_faint) };
            gp.text(egui::pos2(text_left - GUTTER_PAD, y_of(line) + m.line_h / 2.0), Align2::RIGHT_CENTER, (line + 1).to_string(), small.clone(), col);
        }
    }
    // git marks
    for &(line, mark) in &tab.marks {
        if line < first_line || line >= last_line {
            continue;
        }
        let y = y_of(line);
        match mark {
            LineMark::Added => { gp.rect_filled(Rect::from_min_size(egui::pos2(text_left - 6.0, y), Vec2::new(3.0, m.line_h)), 1.0, c(ui_c.git_added)); }
            LineMark::Modified => { gp.rect_filled(Rect::from_min_size(egui::pos2(text_left - 6.0, y), Vec2::new(3.0, m.line_h)), 1.0, c(ui_c.git_modified)); }
            LineMark::Deleted => { gp.add(egui::Shape::convex_polygon(vec![egui::pos2(text_left - 7.0, y - 3.0), egui::pos2(text_left - 2.0, y), egui::pos2(text_left - 7.0, y + 3.0)], c(ui_c.git_deleted), egui::Stroke::NONE)); }

        }
    }

    // scrollbar
    let total_h = (n_lines as f32 * m.line_h).max(view_h) + view_h * 0.5;
    let sb_rect = Rect::from_min_max(egui::pos2(rect.right() - minimap_w - sb_w, rect.top()), egui::pos2(rect.right() - minimap_w, rect.bottom()));
    let thumb_h = (view_h / total_h * view_h).clamp(24.0, view_h);
    let thumb_y = if total_h - view_h > 0.0 { tab.scroll_y / (total_h - view_h).max(1.0) * (view_h - thumb_h) } else { 0.0 };
    let thumb = Rect::from_min_size(egui::pos2(sb_rect.left() + 2.0, rect.top() + thumb_y), Vec2::new(sb_w - 4.0, thumb_h));
    let sb_id = ui.id().with("vsb");
    let sb_resp = ui.interact(sb_rect, sb_id, Sense::click_and_drag());
    if sb_resp.dragged() || sb_resp.is_pointer_button_down_on() {
        if let Some(p) = pointer {
            let frac = ((p.y - rect.top() - thumb_h / 2.0) / (view_h - thumb_h).max(1.0)).clamp(0.0, 1.0);
            tab.scroll_y = (frac * (total_h - view_h)).clamp(0.0, max_scroll_y);
        }
    }
    if n_lines as f32 * m.line_h > view_h * 0.9 {
        let hot = sb_resp.hovered() || sb_resp.dragged();
        let alpha = if hot { 150 } else { 70 };
        painter.rect_filled(thumb, 3.0, Color32::from_rgba_unmultiplied(ui_c.fg[0], ui_c.fg[1], ui_c.fg[2], alpha));
    }

    // minimap
    if minimap_w > 0.0 {
        let mm = Rect::from_min_max(egui::pos2(rect.right() - minimap_w, rect.top()), rect.max);
        let mp = painter.with_clip_rect(mm);
        mp.rect_filled(mm, 0.0, c(ui_c.bg));
        mp.line_segment([mm.left_top(), mm.left_bottom()], egui::Stroke::new(1.0, c(ui_c.border)));
        let lh = 2.0;
        let cap = (mm.height() / lh) as usize;
        let mm_first = if n_lines > cap {
            let frac = tab.scroll_y / max_scroll_y.max(1.0);
            ((n_lines - cap) as f32 * frac) as usize
        } else {
            0
        };
        let mm_last = (mm_first + cap).min(n_lines);
        tab.hl.ensure(&tab.buf, mm_last.min(first_line + 1500), Duration::from_millis(2));
        let cw = 1.0;
        for line in mm_first..mm_last {
            let y = mm.top() + (line - mm_first) as f32 * lh;
            let text = tab.buf.line_string(line);
            if text.trim().is_empty() {
                continue;
            }
            match tab.hl.spans(line) {
                Some(sp) => {
                    for s in sp {
                        let seg = &text.as_bytes()[(s.start as usize).min(text.len())..(s.end as usize).min(text.len())];
                        if seg.iter().all(|b| b.is_ascii_whitespace()) {
                            continue;
                        }
                        let x = mm.left() + 6.0 + s.start as f32 * cw;
                        if x > mm.right() - 2.0 {
                            break;
                        }
                        let w = ((s.end - s.start) as f32 * cw).min(mm.right() - 2.0 - x);
                        mp.rect_filled(Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, lh - 0.6)), 0.0, Color32::from_rgba_unmultiplied(s.color[0], s.color[1], s.color[2], 150));
                    }
                }
                None => {
                    let lead = text.len() - text.trim_start().len();
                    let w = (text.trim().len() as f32 * cw).min(mm.width() - 10.0);
                    mp.rect_filled(Rect::from_min_size(egui::pos2(mm.left() + 6.0 + lead as f32, y), Vec2::new(w, lh - 0.6)), 0.0, Color32::from_rgba_unmultiplied(ui_c.fg[0], ui_c.fg[1], ui_c.fg[2], 50));
                }
            }
        }
        // viewport slider
        let vy = mm.top() + (first_line.saturating_sub(mm_first)) as f32 * lh;
        let vh = (tab.viewport_lines as f32 * lh).min(mm.height());
        mp.rect_filled(Rect::from_min_size(egui::pos2(mm.left(), vy), Vec2::new(mm.width(), vh)), 0.0, Color32::from_rgba_unmultiplied(ui_c.fg[0], ui_c.fg[1], ui_c.fg[2], 22));
        let mm_resp = ui.interact(mm, ui.id().with("minimap"), Sense::click_and_drag());
        if mm_resp.is_pointer_button_down_on() {
            if let Some(p) = pointer {
                let target_line = mm_first + ((p.y - mm.top()) / lh) as usize;
                tab.scroll_y = ((target_line as f32 - tab.viewport_lines as f32 / 2.0) * m.line_h).clamp(0.0, max_scroll_y);
            }
        }
    }

    // 'goto' request from search results etc.
    if let Some((line, c0, c1)) = tab.pending_goto.take() {
        let a = tab.buf.pos_to_char(line, c0);
        let b = tab.buf.pos_to_char(line, c1);
        tab.buf.set_selection(a, b);
        tab.reveal = true;
        let y = line as f32 * m.line_h;
        tab.scroll_y = (y - view_h / 3.0).clamp(0.0, max_scroll_y);
    }
    result
}
