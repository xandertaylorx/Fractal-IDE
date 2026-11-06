//! Incremental syntax highlighting on top of syntect (TextMate grammars).

use crate::buffer::Buffer;
use crate::theme::Theme;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use syntect::highlighting::{FontStyle, HighlightState, Highlighter, RangedHighlightIterator, Theme as SyntectTheme};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();

pub fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

/// A highlighted run inside one line (byte offsets into the line text).
#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
}

type State = (ParseState, HighlightState);

pub struct Highlighter2 {
    theme: Arc<SyntectTheme>,
    default_fg: [u8; 4],
    syntax_name: String,
    states: Vec<State>,
    lines: Vec<Vec<Span>>,
    disabled: bool,
}

fn pick_syntax(buf: &Buffer) -> &'static SyntaxReference {
    let ss = syntaxes();
    if let Some(p) = &buf.path {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some(s) = ss.find_syntax_by_extension(name) {
            return s;
        }
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if let Some(s) = ss.find_syntax_by_extension(ext) {
                return s;
            }
        }
    }
    let first = buf.line_string(0);
    ss.find_syntax_by_first_line(&first).unwrap_or_else(|| ss.find_syntax_plain_text())
}

impl Highlighter2 {
    pub fn new(buf: &Buffer, theme: &Theme) -> Self {
        let syntax = pick_syntax(buf);
        let mut h = Self {
            theme: theme.syntax.clone(),
            default_fg: theme.default_fg,
            syntax_name: syntax.name.clone(),
            states: vec![],
            lines: vec![],
            disabled: buf.len_chars() > 4_000_000,
        };
        h.reset(syntax);
        h
    }

    fn reset(&mut self, syntax: &SyntaxReference) {
        let hl = Highlighter::new(&self.theme);
        let st = (ParseState::new(syntax), HighlightState::new(&hl, ScopeStack::new()));
        self.states = vec![st];
        self.lines.clear();
    }

    pub fn set_theme(&mut self, buf: &Buffer, theme: &Theme) {
        self.theme = theme.syntax.clone();
        self.default_fg = theme.default_fg;
        let syntax = pick_syntax(buf);
        self.syntax_name = syntax.name.clone();
        self.reset(syntax);
    }

    pub fn syntax_changed(&mut self, buf: &Buffer) {
        let syntax = pick_syntax(buf);
        if syntax.name != self.syntax_name {
            self.syntax_name = syntax.name.clone();
            self.reset(syntax);
        }
    }

    /// Call after edits: drops cached lines at and below `line`.
    pub fn invalidate_from(&mut self, line: usize) {
        self.lines.truncate(line);
        self.states.truncate(line + 1);
        if self.states.is_empty() {
            self.states.push(self.states.first().cloned().unwrap_or_else(|| unreachable!()));
        }
    }

    pub fn is_ready(&self, line: usize) -> bool {
        self.disabled || line < self.lines.len()
    }

    /// Parse forward until `upto` (exclusive) or the time budget runs out.
    /// Returns true when everything requested is available.
    pub fn ensure(&mut self, buf: &Buffer, upto: usize, budget: Duration) -> bool {
        if self.disabled {
            return true;
        }
        let ss = syntaxes();
        let upto = upto.min(buf.line_count());
        let start = Instant::now();
        let hl = Highlighter::new(&self.theme);
        while self.lines.len() < upto {
            let i = self.lines.len();
            let (mut ps, mut hs) = self.states[i].clone();
            let mut text = buf.line_string(i);
            let real_len = text.len();
            text.push('\n');
            let ops = ps.parse_line(&text, ss).unwrap_or_default();
            let mut spans = Vec::new();
            for (style, _s, range) in RangedHighlightIterator::new(&mut hs, &ops, &text, &hl) {
                let end = range.end.min(real_len);
                if range.start >= end {
                    continue;
                }
                let c = style.foreground;
                spans.push(Span {
                    start: range.start as u32,
                    end: end as u32,
                    color: [c.r, c.g, c.b, 255],
                    bold: style.font_style.contains(FontStyle::BOLD),
                    italic: style.font_style.contains(FontStyle::ITALIC),
                });
            }
            self.lines.push(spans);
            self.states.push((ps, hs));
            if i % 32 == 31 && start.elapsed() > budget {
                break;
            }
        }
        self.lines.len() >= upto
    }

    pub fn spans(&self, line: usize) -> Option<&[Span]> {
        self.lines.get(line).map(|v| v.as_slice())
    }

    pub fn default_fg(&self) -> [u8; 4] {
        self.default_fg
    }

    pub fn syntax_name(&self) -> &str {
        &self.syntax_name
    }
}
