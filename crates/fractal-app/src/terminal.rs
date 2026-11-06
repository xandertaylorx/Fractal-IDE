//! Integrated terminal: a PTY, a vt100 emulator and an egui renderer.

use crate::ui_theme::c;
use eframe::egui::{self, text::LayoutJob, Color32, Event, FontId, Key, Modifiers, Rect, Sense, TextFormat, Ui, Vec2};
use fractal_core::theme::Theme;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct Term {
    pub title: String,
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    rows: u16,
    cols: u16,
    pub alive: Arc<AtomicBool>,
    scroll: usize,
    sel: Option<((u16, u16), (u16, u16))>,
    dragging: bool,
}

fn nz(v: u16) -> NonZeroU16 {
    NonZeroU16::new(v.max(1)).unwrap()
}

impl Term {
    pub fn spawn(ctx: &egui::Context, cwd: &Path, title: &str) -> Result<Term, String> {
        let (rows, cols) = (24u16, 80u16);
        let pair = native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())?;
        let shell = fractal_core::shellenv::default_shell();
        let mut cmd = CommandBuilder::new(&shell);
        if !cfg!(windows) {
            cmd.arg("-l");
        }
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Fractal");
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(nz(rows), nz(cols), 5000)));
        let alive = Arc::new(AtomicBool::new(true));
        {
            let parser = parser.clone();
            let alive = alive.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 16384];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Ok(mut p) = parser.lock() {
                                p.process(&buf[..n]);
                            }
                            ctx.request_repaint();
                        }
                    }
                }
                alive.store(false, Ordering::Relaxed);
                ctx.request_repaint();
            });
        }
        Ok(Term { title: title.to_string(), parser, writer, master: pair.master, child, rows, cols, alive, scroll: 0, sel: None, dragging: false })
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
        self.scroll = 0;
    }

    pub fn send_line(&mut self, line: &str) {
        let mut s = line.to_string();
        s.push('\r');
        self.write(s.as_bytes());
    }

    pub fn interrupt(&mut self) {
        self.write(&[3]);
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols || rows < 2 || cols < 2 {
            return;
        }
        self.rows = rows;
        self.cols = cols;
        let _ = self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(nz(rows), nz(cols));
        }
    }

    pub fn selection_text(&self) -> Option<String> {
        let (a, b) = self.sel?;
        let (s, e) = if a <= b { (a, b) } else { (b, a) };
        let p = self.parser.lock().ok()?;
        let t = p.screen().contents_between(s.0, s.1, e.0, e.1 + 1);
        (!t.trim().is_empty()).then_some(t)
    }

    /// Translate an egui event into bytes for the shell. Returns true if handled.
    pub fn handle_event(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Text(t) => {
                self.write(t.as_bytes());
                true
            }
            Event::Paste(t) => {
                let bracketed = self.parser.lock().map(|p| p.screen().bracketed_paste()).unwrap_or(false);
                if bracketed {
                    self.write(b"\x1b[200~");
                    self.write(t.replace("\r\n", "\n").as_bytes());
                    self.write(b"\x1b[201~");
                } else {
                    self.write(t.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
                }
                true
            }
            Event::Key { key, pressed: true, modifiers, .. } => {
                if let Some(b) = key_bytes(*key, *modifiers, self.parser.lock().map(|p| p.screen().application_cursor()).unwrap_or(false)) {
                    if modifiers.shift && matches!(key, Key::PageUp | Key::PageDown) {
                        let page = self.rows as usize;
                        self.scroll = if matches!(key, Key::PageUp) { self.scroll + page } else { self.scroll.saturating_sub(page) };
                    } else {
                        self.write(&b);
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn show(&mut self, ui: &mut Ui, theme: &Theme, font_size: f32, focused: bool) -> bool {
        let font = FontId::monospace(font_size);
        let ctx = ui.ctx().clone();
        let cw = ctx.fonts_mut(|f| f.glyph_width(&font, 'M')).max(1.0);
        let ch = (font_size * 1.35).round();
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let pad = 10.0;
        let cols = (((rect.width() - pad * 2.0) / cw) as u16).max(2);
        let rows = (((rect.height() - 6.0) / ch) as u16).max(2);
        self.resize(rows, cols);
        let painter = ui.painter_at(rect);
        let u = &theme.ui;
        let default_bg = c(u.bg_panel);
        painter.rect_filled(rect, 0.0, default_bg);

        // scrolling
        if resp.hovered() {
            let d = ctx.input(|i| i.smooth_scroll_delta()).y;
            if d != 0.0 {
                let alt = self.parser.lock().map(|p| p.screen().alternate_screen()).unwrap_or(false);
                if alt {
                    let key: &[u8] = if d > 0.0 { b"\x1b[A" } else { b"\x1b[B" };
                    for _ in 0..3 {
                        self.write(key);
                    }
                } else {
                    let lines = (d / ch).round() as isize;
                    self.scroll = (self.scroll as isize + lines).max(0) as usize;
                }
            }
        }
        // mouse selection
        let cell_at = |p: egui::Pos2| -> (u16, u16) {
            let col = (((p.x - rect.left() - pad) / cw).floor().max(0.0) as u16).min(cols - 1);
            let row = (((p.y - rect.top() - 3.0) / ch).floor().max(0.0) as u16).min(rows - 1);
            (row, col)
        };
        let mut pressed = false;
        if let Some(p) = ctx.input(|i| i.pointer.latest_pos()) {
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::Text);
            }
            if ctx.input(|i| i.pointer.primary_pressed()) && resp.hovered() {
                pressed = true;
                self.sel = None;
                self.sel = Some((cell_at(p), cell_at(p)));
                self.dragging = true;
            } else if self.dragging && ctx.input(|i| i.pointer.primary_down()) {
                if let Some((a, _)) = self.sel {
                    self.sel = Some((a, cell_at(p)));
                }
            }
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            if self.dragging {
                if let Some((a, b)) = self.sel {
                    if a == b {
                        self.sel = None;
                    }
                }
            }
            self.dragging = false;
        }

        let Ok(mut parser) = self.parser.lock() else { return pressed };
        let max_scroll = parser.screen().scrollback_capacity();
        self.scroll = self.scroll.min(max_scroll);
        parser.screen_mut().set_scrollback(self.scroll);
        let screen = parser.screen();
        let ansi = ansi_palette(theme);
        let default_fg = c(u.fg);
        let sel = self.sel.map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        let mut job_font = font.clone();
        job_font.size = font_size;
        for row in 0..rows {
            let y = rect.top() + 3.0 + row as f32 * ch;
            let mut job = LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            let mut run = String::new();
            let mut cur: Option<(Color32, bool, bool)> = None;
            let mut bg_start: Option<(u16, Color32)> = None;
            let mut col = 0u16;
            while col < cols {
                let cell = screen.cell(row, col);
                let (mut fg, mut bg, mut bold, mut ul, mut it) = (default_fg, None::<Color32>, false, false, false);
                let mut text = " ".to_string();
                let mut wide = false;
                if let Some(cell) = cell {
                    if cell.is_wide_continuation() {
                        col += 1;
                        continue;
                    }
                    if cell.has_contents() {
                        text = cell.contents().to_string();
                    }
                    wide = cell.is_wide();
                    fg = map_color(cell.fgcolor(), default_fg, &ansi, cell.bold());
                    bg = match cell.bgcolor() {
                        vt100::Color::Default => None,
                        other => Some(map_color(other, default_fg, &ansi, false)),
                    };
                    if cell.inverse() {
                        let b = bg.unwrap_or(default_bg);
                        bg = Some(fg);
                        fg = b;
                    }
                    bold = cell.bold();
                    ul = cell.underline();
                    it = cell.italic();
                }
                let selected = sel.map_or(false, |(a, b)| (row, col) >= a && (row, col) <= b);
                if selected {
                    bg = Some(c(u.selection));
                }
                let _ = bold;
                // background runs
                match (&mut bg_start, bg) {
                    (Some((_, cc)), Some(nb)) if *cc == nb => {}
                    (slot, new) => {
                        if let Some((s, cc)) = slot.take() {
                            painter.rect_filled(Rect::from_min_max(egui::pos2(rect.left() + pad + s as f32 * cw, y), egui::pos2(rect.left() + pad + col as f32 * cw, y + ch)), 0.0, cc);
                        }
                        if let Some(nb) = new {
                            *slot = Some((col, nb));
                        }
                    }
                }
                let fmt = (fg, ul, it);
                if cur != Some(fmt) {
                    if !run.is_empty() {
                        let (fc, fu, fi) = cur.unwrap();
                        job.append(&run, 0.0, TextFormat { font_id: job_font.clone(), color: fc, italics: fi, underline: if fu { egui::Stroke::new(1.0, fc) } else { egui::Stroke::NONE }, ..Default::default() });
                        run.clear();
                    }
                    cur = Some(fmt);
                }
                run.push_str(&text);
                if wide {
                    col += 1;
                }
                col += 1;
            }
            if let Some((s, cc)) = bg_start.take() {
                painter.rect_filled(Rect::from_min_max(egui::pos2(rect.left() + pad + s as f32 * cw, y), egui::pos2(rect.left() + pad + cols as f32 * cw, y + ch)), 0.0, cc);
            }
            if !run.is_empty() {
                let (fc, fu, fi) = cur.unwrap();
                job.append(&run, 0.0, TextFormat { font_id: job_font.clone(), color: fc, italics: fi, underline: if fu { egui::Stroke::new(1.0, fc) } else { egui::Stroke::NONE }, ..Default::default() });
            }
            if !job.sections.is_empty() {
                let g = painter.layout_job(job);
                let gy = y + (ch - g.size().y) / 2.0;
                painter.galley(egui::pos2(rect.left() + pad, gy), g, default_fg);
            }
        }
        // cursor
        if self.scroll == 0 && !screen.hide_cursor() {
            let (cr, cc) = screen.cursor_position();
            let r = Rect::from_min_size(egui::pos2(rect.left() + pad + cc as f32 * cw, rect.top() + 3.0 + cr as f32 * ch), Vec2::new(cw, ch));
            if focused {
                painter.rect_filled(r, 1.0, c(u.cursor).gamma_multiply(0.85));
            } else {
                painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, c(u.fg_dim)), egui::StrokeKind::Inside);
            }
        }
        if self.scroll > 0 {
            painter.text(egui::pos2(rect.right() - 12.0, rect.top() + 8.0), egui::Align2::RIGHT_TOP, format!("↑ {}", self.scroll), FontId::proportional(11.0), c(u.fg_dim));
        }
        parser.screen_mut().set_scrollback(0);
        pressed
    }
}

fn key_bytes(key: Key, m: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let esc = |s: &str| Some(s.as_bytes().to_vec());
    let arrow = |c: char| -> Option<Vec<u8>> {
        if m.alt && matches!(c, 'D' | 'C') {
            return esc(if c == 'D' { "\x1bb" } else { "\x1bf" });
        }
        if m.mac_cmd && matches!(c, 'D' | 'C') {
            return Some(vec![if c == 'D' { 1 } else { 5 }]);
        }
        let mods = 1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8;
        if mods > 1 {
            return Some(format!("\x1b[1;{mods}{c}").into_bytes());
        }
        Some(if app_cursor { format!("\x1bO{c}").into_bytes() } else { format!("\x1b[{c}").into_bytes() })
    };
    match key {
        Key::Enter => esc("\r"),
        Key::Backspace => {
            if m.mac_cmd {
                Some(vec![0x15])
            } else if m.alt {
                Some(vec![0x1b, 0x7f])
            } else {
                Some(vec![0x7f])
            }
        }
        Key::Tab => {
            if m.shift {
                esc("\x1b[Z")
            } else {
                esc("\t")
            }
        }
        Key::Escape => esc("\x1b"),
        Key::ArrowUp => arrow('A'),
        Key::ArrowDown => arrow('B'),
        Key::ArrowRight => arrow('C'),
        Key::ArrowLeft => arrow('D'),
        Key::Home => esc("\x1b[H"),
        Key::End => esc("\x1b[F"),
        Key::PageUp => esc("\x1b[5~"),
        Key::PageDown => esc("\x1b[6~"),
        Key::Delete => esc("\x1b[3~"),
        Key::Insert => esc("\x1b[2~"),
        Key::F1 => esc("\x1bOP"),
        Key::F2 => esc("\x1bOQ"),
        Key::F3 => esc("\x1bOR"),
        Key::F4 => esc("\x1bOS"),
        Key::F5 => esc("\x1b[15~"),
        Key::F6 => esc("\x1b[17~"),
        Key::F7 => esc("\x1b[18~"),
        Key::F8 => esc("\x1b[19~"),
        Key::F9 => esc("\x1b[20~"),
        Key::F10 => esc("\x1b[21~"),
        Key::F11 => esc("\x1b[23~"),
        Key::F12 => esc("\x1b[24~"),
        k if m.ctrl && !m.alt => {
            let name = k.name();
            let ch = name.chars().next()?;
            if name.len() == 1 && ch.is_ascii_alphabetic() {
                Some(vec![(ch.to_ascii_lowercase() as u8) - b'a' + 1])
            } else {
                match k {
                    Key::OpenBracket => Some(vec![0x1b]),
                    Key::Backslash => Some(vec![0x1c]),
                    Key::CloseBracket => Some(vec![0x1d]),
                    _ => None,
                }
            }
        }
        k if m.alt && !m.ctrl && !m.mac_cmd => {
            let name = k.name();
            if name.len() == 1 && name.chars().next()?.is_ascii_alphanumeric() {
                let mut v = vec![0x1b];
                v.extend(name.to_ascii_lowercase().as_bytes());
                Some(v)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn ansi_palette(t: &Theme) -> [Color32; 16] {
    if t.dark {
        [
            "#1e2127", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf", "#5c6370", "#ff7b86", "#b1e18b", "#f2d38e", "#7ec1ff", "#dd8bf5", "#6fd3df", "#ffffff",
        ]
    } else {
        [
            "#383a42", "#e45649", "#50a14f", "#c18401", "#0184bc", "#a626a4", "#0997b3", "#a0a1a7", "#4f525e", "#e06c75", "#6aaf68", "#d19a1a", "#1a8fd1", "#b94cb7", "#1aa9c4", "#ffffff",
        ]
    }
    .map(|h| c(fractal_core::theme::hex(h)))
}

fn map_color(col: vt100::Color, default: Color32, ansi: &[Color32; 16], bold: bool) -> Color32 {
    match col {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        vt100::Color::Idx(i) => {
            let i = if bold && i < 8 { i + 8 } else { i };
            if i < 16 {
                ansi[i as usize]
            } else if i < 232 {
                let n = i - 16;
                let lv = |x: u8| if x == 0 { 0 } else { 55 + x * 40 };
                Color32::from_rgb(lv(n / 36), lv((n / 6) % 6), lv(n % 6))
            } else {
                let g = 8 + (i - 232) * 10;
                Color32::from_rgb(g, g, g)
            }
        }
    }
}
