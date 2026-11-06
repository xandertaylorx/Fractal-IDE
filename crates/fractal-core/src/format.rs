//! Document formatting: prefers the language's real formatter when installed,
//! otherwise falls back to a built-in "easy format" (indentation, whitespace).

use crate::buffer::IndentStyle;
use crate::run::which;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Formatted {
    pub text: String,
    pub used: String,
}

fn run_stdin(mut cmd: Command, input: &str, cwd: Option<&Path>) -> Result<String, String> {
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let data = input.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(data.as_bytes());
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(10) {
                    let _ = child.kill();
                    return Err("formatter timed out".into());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = writer.join();
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("formatter failed").to_string())
    }
}

fn local_bin(project: Option<&Path>, name: &str) -> Option<PathBuf> {
    if let Some(p) = project {
        let b = p.join("node_modules/.bin").join(name);
        if b.exists() {
            return Some(b);
        }
    }
    which(name)
}

fn external(lang: &str, text: &str, path: Option<&Path>, project: Option<&Path>) -> Option<Result<Formatted, String>> {
    let dir = path.and_then(|p| p.parent());
    let fname = path.and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("file.txt").to_string();
    let wrap = |name: &str, r: Result<String, String>| Some(r.map(|t| Formatted { text: t, used: name.to_string() }));
    match lang {
        "rust" => {
            let bin = which("rustfmt")?;
            let mut c = Command::new(bin);
            c.args(["--edition", "2021", "--emit", "stdout"]);
            wrap("rustfmt", run_stdin(c, text, dir))
        }
        "go" => {
            let bin = which("gofmt")?;
            wrap("gofmt", run_stdin(Command::new(bin), text, dir))
        }
        "python" => {
            if let Some(bin) = which("ruff") {
                let mut c = Command::new(bin);
                c.args(["format", "--stdin-filename", &fname, "-"]);
                return wrap("ruff", run_stdin(c, text, dir));
            }
            let bin = which("black")?;
            let mut c = Command::new(bin);
            c.args(["-q", "-"]);
            wrap("black", run_stdin(c, text, dir))
        }
        "javascript" | "javascriptreact" | "typescript" | "typescriptreact" | "css" | "scss" | "less" | "html" | "json" | "markdown" | "yaml" => {
            let bin = local_bin(project, "prettier")?;
            let mut c = Command::new(bin);
            c.args(["--stdin-filepath", &fname]);
            wrap("prettier", run_stdin(c, text, dir))
        }
        "c" | "cpp" | "java" | "csharp" => {
            let bin = which("clang-format")?;
            let mut c = Command::new(bin);
            c.arg(format!("--assume-filename={fname}"));
            wrap("clang-format", run_stdin(c, text, dir))
        }
        "lua" => {
            let bin = which("stylua")?;
            let mut c = Command::new(bin);
            c.args(["-"]);
            wrap("stylua", run_stdin(c, text, dir))
        }
        "shellscript" => {
            let bin = which("shfmt")?;
            wrap("shfmt", run_stdin(Command::new(bin), text, dir))
        }
        _ => None,
    }
}

/// Format `text`. Tries the real formatter first, then the built-in one.
pub fn format_document(lang: &str, text: &str, path: Option<&Path>, project: Option<&Path>, indent: IndentStyle) -> Result<Formatted, String> {
    if let Some(r) = external(lang, text, path, project) {
        match r {
            Ok(f) if !f.text.trim().is_empty() || text.trim().is_empty() => return Ok(f),
            _ => {} // fall through to built-in on failure (e.g. syntax errors)
        }
    }
    builtin(lang, text, indent).map(|t| Formatted { text: t, used: "built-in".into() })
}

fn unit(ind: IndentStyle) -> String {
    if ind.use_spaces {
        " ".repeat(ind.size)
    } else {
        "\t".to_string()
    }
}

/// Whitespace hygiene shared by all languages.
fn tidy(text: &str, trim_trailing: bool) -> String {
    let mut lines: Vec<String> = text.split('\n').map(|l| if trim_trailing { l.trim_end_matches([' ', '\t', '\r']).to_string() } else { l.trim_end_matches('\r').to_string() }).collect();
    while lines.last().map_or(false, |l| l.is_empty()) {
        lines.pop();
    }
    // collapse 3+ blank lines into 2
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut blanks = 0;
    for l in lines {
        if l.is_empty() {
            blanks += 1;
            if blanks > 2 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push(l);
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

fn builtin(lang: &str, text: &str, indent: IndentStyle) -> Result<String, String> {
    match lang {
        "json" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                let mut buf = Vec::new();
                let ind = unit(indent);
                let fmt = serde_json::ser::PrettyFormatter::with_indent(ind.as_bytes());
                let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
                serde::Serialize::serialize(&v, &mut ser).map_err(|e| e.to_string())?;
                let mut s = String::from_utf8_lossy(&buf).to_string();
                s.push('\n');
                return Ok(s);
            }
            Ok(reindent_braces(text, &unit(indent)))
        }
        "rust" | "javascript" | "javascriptreact" | "typescript" | "typescriptreact" | "java" | "kotlin" | "scala" | "c" | "cpp" | "csharp" | "go" | "php" | "swift" | "dart" | "css" | "scss" | "less" | "zig" => Ok(reindent_braces(text, &unit(indent))),
        "html" | "xml" => Ok(reindent_markup(text, &unit(indent))),
        "python" => {
            // Indentation is semantic in Python: only normalize tabs and whitespace.
            let u = " ".repeat(indent.size);
            let conv: Vec<String> = text
                .split('\n')
                .map(|l| {
                    let lead: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                    let rest = &l[lead.len()..];
                    format!("{}{}", lead.replace('\t', &u), rest)
                })
                .collect();
            Ok(tidy(&conv.join("\n"), true))
        }
        "markdown" => Ok(tidy(text, false)),
        _ => Ok(tidy(text, true)),
    }
}

#[derive(Default)]
struct Lex {
    in_block_comment: bool,
    in_string: Option<char>,
    in_template: bool,
}

/// Net bracket change and count of leading closers on a line, ignoring strings and comments.
fn scan_line(line: &str, st: &mut Lex, line_comment: &str) -> (i32, usize, bool) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut net = 0;
    let mut leading_closers = 0;
    let mut seen_code = false;
    let mut started_in_literal = st.in_block_comment || st.in_template;
    let lc: Vec<char> = line_comment.chars().collect();
    while i < chars.len() {
        let c = chars[i];
        if st.in_block_comment {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                st.in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if st.in_template {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == '`' {
                st.in_template = false;
            }
            i += 1;
            continue;
        }
        if let Some(q) = st.in_string {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                st.in_string = None;
            }
            i += 1;
            continue;
        }
        if !lc.is_empty() && chars[i..].starts_with(&lc) {
            break;
        }
        match c {
            '/' if chars.get(i + 1) == Some(&'*') => {
                st.in_block_comment = true;
                i += 2;
                continue;
            }
            '"' => st.in_string = Some('"'),
            '\'' => {
                // lifetimes ('a) and char literals: only treat as string if a closing quote follows soon
                let close = chars[i + 1..].iter().take(4).position(|&x| x == '\'');
                if close.is_some() && chars.get(i + 1) != Some(&' ') {
                    let j = i + 1 + close.unwrap();
                    i = j + 1;
                    seen_code = true;
                    continue;
                }
            }
            '`' => st.in_template = true,
            '{' | '(' | '[' => {
                net += 1;
                seen_code = true;
            }
            '}' | ')' | ']' => {
                net -= 1;
                if !seen_code {
                    leading_closers += 1;
                }
            }
            c if !c.is_whitespace() => seen_code = true,
            _ => {}
        }
        i += 1;
    }
    // unterminated plain strings end at EOL
    if let Some(q) = st.in_string {
        if q != '`' {
            st.in_string = None;
        }
    }
    started_in_literal |= false;
    (net, leading_closers, started_in_literal)
}

fn reindent_braces(text: &str, unit: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut depth: i32 = 0;
    let mut st = Lex::default();
    let mut prev_open_literal = false;
    let line_comment = "//";
    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        let was_in_literal = st.in_block_comment || st.in_template;
        if was_in_literal {
            // inside a block comment / template literal: keep as-is
            let _ = scan_line(line, &mut st, line_comment);
            out.push(line.trim_end().to_string());
            prev_open_literal = true;
            continue;
        }
        let _ = prev_open_literal;
        prev_open_literal = false;
        if trimmed.is_empty() {
            out.push(String::new());
            continue;
        }
        let (net, lead, _) = scan_line(trimmed, &mut st, line_comment);
        let d = (depth - lead as i32).max(0);
        // preprocessor / labels stay flush-left in C-likes
        let indent_level = if trimmed.starts_with('#') && !trimmed.starts_with("#[") && !trimmed.starts_with("#!") { 0 } else { d };
        // continuation of block comment lines starting with '*'
        let extra = if trimmed.starts_with('*') && !trimmed.starts_with("*/") && depth > 0 { 0 } else { 0 };
        out.push(format!("{}{}", unit.repeat((indent_level + extra) as usize), trimmed));
        depth = (depth + net).max(0);
    }
    tidy(&out.join("\n"), true)
}

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr", "!doctype"];

fn reindent_markup(text: &str, unit: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut depth: i32 = 0;
    let mut raw_until: Option<&str> = None;
    for raw in text.split('\n') {
        let t = raw.trim();
        if let Some(tag) = raw_until {
            // inside <script>/<style>/<pre>: leave content untouched
            if t.to_ascii_lowercase().starts_with(&format!("</{tag}")) {
                raw_until = None;
                depth = (depth - 1).max(0);
                out.push(format!("{}{}", unit.repeat(depth as usize), t));
            } else {
                out.push(raw.trim_end().to_string());
            }
            continue;
        }
        if t.is_empty() {
            out.push(String::new());
            continue;
        }
        // scan tags in this line
        let lower = t.to_ascii_lowercase();
        let mut opens = 0i32;
        let mut closes = 0i32;
        let mut first_is_close = lower.starts_with("</");
        let bytes = t.as_bytes();
        let mut i = 0;
        let mut pending_raw: Option<&str> = None;
        while i < bytes.len() {
            if bytes[i] == b'<' {
                let rest = &lower[i..];
                if rest.starts_with("<!--") {
                    if let Some(e) = rest.find("-->") {
                        i += e + 3;
                        continue;
                    }
                    break;
                }
                let is_close = rest.starts_with("</");
                let name_start = if is_close { 2 } else { 1 };
                let name: String = rest[name_start..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '!' || *c == ':').collect();
                let end = rest.find('>').map(|e| i + e);
                if name.is_empty() {
                    i += 1;
                    continue;
                }
                let self_closing = end.map_or(false, |e| t[..e].ends_with('/'));
                if is_close {
                    closes += 1;
                } else if !VOID.contains(&name.as_str()) && !self_closing {
                    opens += 1;
                    if matches!(name.as_str(), "script" | "style" | "pre" | "textarea") {
                        // raw if the closing tag isn't on this line
                        let close_tag = format!("</{name}");
                        if !lower[i..].contains(&close_tag) {
                            pending_raw = Some(match name.as_str() {
                                "script" => "script",
                                "style" => "style",
                                "pre" => "pre",
                                _ => "textarea",
                            });
                        }
                    }
                }
                i = end.map_or(bytes.len(), |e| e + 1);
                continue;
            }
            i += 1;
        }
        if first_is_close && closes == 0 {
            first_is_close = false;
        }
        let d = if first_is_close { (depth - 1).max(0) } else { depth };
        out.push(format!("{}{}", unit.repeat(d as usize), t));
        depth = (depth + opens - closes).max(0);
        if let Some(tag) = pending_raw {
            raw_until = Some(tag);
        }
    }
    tidy(&out.join("\n"), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ind() -> IndentStyle {
        IndentStyle { use_spaces: true, size: 4 }
    }

    #[test]
    fn braces_reindent() {
        let src = "fn main() {\nlet x = 1;\n   if x > 0 {\n  println!(\"hi {\");\n}\n}\n";
        let out = builtin("rust", src, ind()).unwrap();
        assert_eq!(out, "fn main() {\n    let x = 1;\n    if x > 0 {\n        println!(\"hi {\");\n    }\n}\n");
    }

    #[test]
    fn json_pretty_keeps_order() {
        let out = builtin("json", "{\"b\":1,\"a\":[1,2]}", IndentStyle { use_spaces: true, size: 2 }).unwrap();
        assert_eq!(out, "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ]\n}\n");
    }

    #[test]
    fn html_indents_and_keeps_script() {
        let src = "<div>\n<p>hi</p>\n<script>\nlet a = {  };\n</script>\n</div>\n";
        let out = builtin("html", src, ind()).unwrap();
        assert_eq!(out, "<div>\n    <p>hi</p>\n    <script>\nlet a = {  };\n    </script>\n</div>\n");
    }

    #[test]
    fn python_whitespace_only() {
        let out = builtin("python", "def f():\n\treturn 1   \n\n\n\n\nx = 2", ind()).unwrap();
        assert_eq!(out, "def f():\n    return 1\n\n\nx = 2\n");
    }

    #[test]
    fn block_comments_untouched() {
        let src = "/*\n   keep   \n*/\nint a() {\nreturn 1;\n}\n";
        let out = builtin("c", src, ind()).unwrap();
        assert_eq!(out, "/*\n   keep\n*/\nint a() {\n    return 1;\n}\n");
    }
}
