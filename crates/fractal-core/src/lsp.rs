//! A small Language Server Protocol client (JSON-RPC over stdio).
//!
//! UI-independent: the app polls channels each frame. This is the same
//! mechanism VS Code uses for IntelliSense (completion, hover, diagnostics,
//! go-to-definition), so any language server works.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

// ----------------------------------------------------------------------
// Plain data types the UI uses
// ----------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    /// 1 error, 2 warning, 3 info, 4 hint
    pub severity: u8,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct TextEdit {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub new_text: String,
}

#[derive(Clone, Debug)]
pub struct CompletionItem {
    pub label: String,
    /// LSP CompletionItemKind (1 text, 2 method, 3 function, ... 14 keyword, 15 snippet)
    pub kind: u32,
    pub detail: String,
    pub insert_text: String,
    pub is_snippet: bool,
    pub edit: Option<TextEdit>,
    pub sort_text: String,
    pub filter_text: String,
    pub deprecated: bool,
}

#[derive(Clone, Debug)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub enum LspEvent {
    Diagnostics { uri: String, items: Vec<Diagnostic> },
    Log(String),
    Exited,
}

pub type Reply = Receiver<Result<Value, String>>;

// ----------------------------------------------------------------------
// URI + position helpers
// ----------------------------------------------------------------------

pub fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy();
    let mut out = String::from("file://");
    let s = if cfg!(windows) { format!("/{}", s.replace('\\', "/")) } else { s.to_string() };
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 0 && i + 2 <= bytes.len() - 1 {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).to_string();
    let s = if cfg!(windows) { s.trim_start_matches('/').replace('/', "\\") } else { s };
    Some(PathBuf::from(s))
}

/// LSP columns are UTF-16 code units.
pub fn char_to_utf16(line: &str, col: usize) -> usize {
    line.chars().take(col).map(|c| c.len_utf16()).sum()
}

pub fn utf16_to_char(line: &str, u16col: usize) -> usize {
    let mut n = 0;
    for (i, c) in line.chars().enumerate() {
        if n >= u16col {
            return i;
        }
        n += c.len_utf16();
    }
    line.chars().count()
}

// ----------------------------------------------------------------------
// Client
// ----------------------------------------------------------------------

struct State {
    ready: bool,
    queue: Vec<String>,
    trigger_chars: Vec<char>,
    caps: Value,
}

pub struct LspClient {
    pub name: String,
    pub root: PathBuf,
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicI64,
    pending: Arc<Mutex<HashMap<i64, Sender<Result<Value, String>>>>>,
    pub events: Receiver<LspEvent>,
    state: Arc<Mutex<State>>,
    pub dead: Arc<AtomicBool>,
    versions: HashMap<String, i32>,
}

fn write_msg(stdin: &Arc<Mutex<ChildStdin>>, v: &Value) {
    let body = v.to_string();
    if let Ok(mut s) = stdin.lock() {
        let _ = write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body);
        let _ = s.flush();
    }
}

fn read_frame(r: &mut BufReader<impl Read>) -> Option<Value> {
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let l = line.trim();
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.strip_prefix("Content-Length:") {
            len = v.trim().parse().ok()?;
        }
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

impl LspClient {
    pub fn start(name: &str, program: &Path, args: &[String], root: &Path, notify: impl Fn() + Send + 'static) -> Result<LspClient, String> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not start {name}: {e}"))?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or("no stdin")?));
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;
        let (ev_tx, ev_rx) = channel();
        let pending: Arc<Mutex<HashMap<i64, Sender<Result<Value, String>>>>> = Arc::new(Mutex::new(HashMap::new()));
        let state = Arc::new(Mutex::new(State { ready: false, queue: vec![], trigger_chars: vec![], caps: Value::Null }));
        let dead = Arc::new(AtomicBool::new(false));

        // stderr: keep the pipe drained
        {
            let tx = ev_tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _ = tx.send(LspEvent::Log(line));
                }
            });
        }
        // stdout: frames
        {
            let stdin = stdin.clone();
            let pending = pending.clone();
            let state = state.clone();
            let dead = dead.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(stdout);
                let trace = std::env::var("FRACTAL_LSP_TRACE").is_ok();
                while let Some(msg) = read_frame(&mut r) {
                    if trace {
                        let _ = ev_tx.send(LspEvent::Log(format!("<- {}", msg.to_string().chars().take(220).collect::<String>())));
                    }
                    let id = msg.get("id").and_then(|i| i.as_i64());
                    let method = msg.get("method").and_then(|m| m.as_str()).map(String::from);
                    match (id, method) {
                        // response to one of our requests
                        (Some(id), None) => {
                            let res = if let Some(e) = msg.get("error") { Err(e.get("message").and_then(|m| m.as_str()).unwrap_or("error").to_string()) } else { Ok(msg.get("result").cloned().unwrap_or(Value::Null)) };
                            if id == 0 {
                                // initialize finished
                                if let Ok(v) = &res {
                                    let tc: Vec<char> = v
                                        .pointer("/capabilities/completionProvider/triggerCharacters")
                                        .and_then(|t| t.as_array())
                                        .map(|a| a.iter().filter_map(|x| x.as_str().and_then(|s| s.chars().next())).collect())
                                        .unwrap_or_default();
                                    let mut st = state.lock().unwrap();
                                    st.trigger_chars = tc;
                                    st.caps = v.get("capabilities").cloned().unwrap_or(Value::Null);
                                }
                                write_msg(&stdin, &json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
                                let queued = {
                                    let mut st = state.lock().unwrap();
                                    st.ready = true;
                                    std::mem::take(&mut st.queue)
                                };
                                for q in queued {
                                    if let Ok(mut s) = stdin.lock() {
                                        let _ = write!(s, "Content-Length: {}\r\n\r\n{}", q.len(), q);
                                        let _ = s.flush();
                                    }
                                }
                            } else if let Some(tx) = pending.lock().unwrap().remove(&id) {
                                let _ = tx.send(res);
                            }
                        }
                        // request from the server: answer so it doesn't stall
                        (Some(id), Some(m)) => {
                            let result = match m.as_str() {
                                "workspace/configuration" => {
                                    let n = msg.pointer("/params/items").and_then(|i| i.as_array()).map(|a| a.len()).unwrap_or(0);
                                    Value::Array(vec![Value::Null; n])
                                }
                                "workspace/workspaceFolders" => json!([]),
                                _ => Value::Null,
                            };
                            write_msg(&stdin, &json!({"jsonrpc":"2.0","id":id,"result":result}));
                        }
                        // notification
                        (None, Some(m)) => {
                            if m == "textDocument/publishDiagnostics" {
                                if let Some(p) = msg.get("params") {
                                    let uri = p.get("uri").and_then(|u| u.as_str()).unwrap_or("").to_string();
                                    let _ = ev_tx.send(LspEvent::Diagnostics { uri, items: parse_diagnostics(p) });
                                }
                            }
                        }
                        _ => {}
                    }
                    notify();
                }
                dead.store(true, Ordering::Relaxed);
                let _ = ev_tx.send(LspEvent::Exited);
                notify();
            });
        }

        let client = LspClient { name: name.to_string(), root: root.to_path_buf(), child, stdin, next_id: AtomicI64::new(1), pending, events: ev_rx, state, dead, versions: HashMap::new() };
        let root_uri = path_to_uri(root);
        let init = json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "clientInfo": {"name": "Fractal", "version": env!("CARGO_PKG_VERSION")},
                "rootUri": root_uri,
                "rootPath": root.to_string_lossy(),
                "capabilities": {
                    "general": {"positionEncodings": ["utf-16"]},
                    "workspace": {"configuration": true},
                    "window": {"workDoneProgress": true},
                    "textDocument": {
                        "synchronization": {"didSave": true, "dynamicRegistration": false},
                        "completion": {
                            "contextSupport": true,
                            "completionItem": {"snippetSupport": false, "documentationFormat": ["plaintext", "markdown"], "deprecatedSupport": true, "labelDetailsSupport": true},
                            "completionItemKind": {}
                        },
                        "hover": {"contentFormat": ["plaintext", "markdown"]},
                        "definition": {"linkSupport": true},
                        "publishDiagnostics": {"relatedInformation": false}
                    }
                }
            }
        });
        write_msg(&client.stdin, &init);
        Ok(client)
    }

    pub fn is_ready(&self) -> bool {
        self.state.lock().map(|s| s.ready).unwrap_or(false)
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Relaxed)
    }

    pub fn trigger_chars(&self) -> Vec<char> {
        self.state.lock().map(|s| s.trigger_chars.clone()).unwrap_or_default()
    }

    fn send_or_queue(&self, v: &Value) {
        let body = v.to_string();
        let mut st = self.state.lock().unwrap();
        if st.ready {
            drop(st);
            write_msg(&self.stdin, v);
        } else {
            st.queue.push(body);
        }
    }

    pub fn notify(&self, method: &str, params: Value) {
        self.send_or_queue(&json!({"jsonrpc":"2.0","method":method,"params":params}));
    }

    pub fn request(&self, method: &str, params: Value) -> Reply {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.send_or_queue(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        rx
    }

    // ---- documents ----

    pub fn did_open(&mut self, path: &Path, language_id: &str, text: &str) {
        let uri = path_to_uri(path);
        self.versions.insert(uri.clone(), 1);
        self.notify("textDocument/didOpen", json!({"textDocument": {"uri": uri, "languageId": language_id, "version": 1, "text": text}}));
    }

    pub fn did_change(&mut self, path: &Path, text: &str) {
        let uri = path_to_uri(path);
        let v = self.versions.entry(uri.clone()).or_insert(1);
        *v += 1;
        let version = *v;
        self.notify("textDocument/didChange", json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}));
    }

    pub fn did_save(&mut self, path: &Path) {
        self.notify("textDocument/didSave", json!({"textDocument": {"uri": path_to_uri(path)}}));
    }

    pub fn did_close(&mut self, path: &Path) {
        let uri = path_to_uri(path);
        self.versions.remove(&uri);
        self.notify("textDocument/didClose", json!({"textDocument": {"uri": uri}}));
    }

    pub fn is_open(&self, path: &Path) -> bool {
        self.versions.contains_key(&path_to_uri(path))
    }

    // ---- requests ----

    fn pos(path: &Path, line: usize, col16: usize) -> Value {
        json!({"textDocument": {"uri": path_to_uri(path)}, "position": {"line": line, "character": col16}})
    }

    pub fn completion(&self, path: &Path, line: usize, col16: usize, trigger_char: Option<char>) -> Reply {
        let mut p = Self::pos(path, line, col16);
        p["context"] = match trigger_char {
            Some(c) => json!({"triggerKind": 2, "triggerCharacter": c.to_string()}),
            None => json!({"triggerKind": 1}),
        };
        self.request("textDocument/completion", p)
    }

    pub fn hover(&self, path: &Path, line: usize, col16: usize) -> Reply {
        self.request("textDocument/hover", Self::pos(path, line, col16))
    }

    pub fn definition(&self, path: &Path, line: usize, col16: usize) -> Reply {
        self.request("textDocument/definition", Self::pos(path, line, col16))
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if !self.is_dead() {
            write_msg(&self.stdin, &json!({"jsonrpc":"2.0","id":999999,"method":"shutdown"}));
            write_msg(&self.stdin, &json!({"jsonrpc":"2.0","method":"exit"}));
        }
        let _ = self.child.kill();
    }
}

// ----------------------------------------------------------------------
// Response parsing
// ----------------------------------------------------------------------

fn range_of(v: &Value) -> Option<(usize, usize, usize, usize)> {
    let g = |p: &str| v.pointer(p).and_then(|x| x.as_u64()).map(|x| x as usize);
    Some((g("/start/line")?, g("/start/character")?, g("/end/line")?, g("/end/character")?))
}

pub fn parse_diagnostics(params: &Value) -> Vec<Diagnostic> {
    params
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|d| {
                    let (l, c, el, ec) = range_of(d.get("range")?)?;
                    Some(Diagnostic { line: l, col: c, end_line: el, end_col: ec, severity: d.get("severity").and_then(|s| s.as_u64()).unwrap_or(1) as u8, message: d.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string() })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_completion(v: &Value) -> Vec<CompletionItem> {
    let arr = match v {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default(),
        _ => vec![],
    };
    arr.iter()
        .filter_map(|it| {
            let label = it.get("label")?.as_str()?.to_string();
            let edit = it.get("textEdit").and_then(|te| {
                let range = te.get("range").or_else(|| te.get("replace")).or_else(|| te.get("insert"))?;
                let (l, c, el, ec) = range_of(range)?;
                Some(TextEdit { line: l, col: c, end_line: el, end_col: ec, new_text: te.get("newText")?.as_str()?.to_string() })
            });
            let detail = it
                .pointer("/labelDetails/detail")
                .and_then(|d| d.as_str())
                .map(|d| d.to_string())
                .or_else(|| it.get("detail").and_then(|d| d.as_str()).map(String::from))
                .unwrap_or_default();
            let insert_text = it.get("insertText").and_then(|t| t.as_str()).map(String::from).or_else(|| edit.as_ref().map(|e| e.new_text.clone())).unwrap_or_else(|| label.clone());
            Some(CompletionItem {
                kind: it.get("kind").and_then(|k| k.as_u64()).unwrap_or(1) as u32,
                detail,
                is_snippet: it.get("insertTextFormat").and_then(|f| f.as_u64()) == Some(2),
                sort_text: it.get("sortText").and_then(|s| s.as_str()).unwrap_or(&label).to_string(),
                filter_text: it.get("filterText").and_then(|s| s.as_str()).unwrap_or(&label).to_string(),
                deprecated: it.get("deprecated").and_then(|d| d.as_bool()).unwrap_or(false) || it.get("tags").and_then(|t| t.as_array()).map_or(false, |t| t.iter().any(|x| x.as_u64() == Some(1))),
                label,
                insert_text,
                edit,
            })
        })
        .collect()
}

/// Flatten hover contents (string | MarkupContent | MarkedString[]) to plain text.
pub fn parse_hover(v: &Value) -> Option<String> {
    fn one(c: &Value) -> String {
        match c {
            Value::String(s) => s.clone(),
            Value::Object(o) => o.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            _ => String::new(),
        }
    }
    let contents = v.get("contents")?;
    let raw = match contents {
        Value::Array(a) => a.iter().map(one).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n"),
        other => one(other),
    };
    // strip markdown code fences, keep content
    let cleaned: Vec<&str> = raw.lines().filter(|l| !l.trim_start().starts_with("```")).collect();
    let text = cleaned.join("\n").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn parse_definition(v: &Value) -> Vec<Location> {
    let items: Vec<Value> = match v {
        Value::Array(a) => a.clone(),
        Value::Object(_) => vec![v.clone()],
        _ => vec![],
    };
    items
        .iter()
        .filter_map(|l| {
            let uri = l.get("uri").or_else(|| l.get("targetUri"))?.as_str()?;
            let range = l.get("range").or_else(|| l.get("targetSelectionRange")).or_else(|| l.get("targetRange"))?;
            let (line, col, _, _) = range_of(range)?;
            Some(Location { path: uri_to_path(uri)?, line, col })
        })
        .collect()
}

/// Turn an LSP snippet into plain text; returns the text and the caret char offset.
pub fn snippet_to_text(s: &str) -> (String, Option<usize>) {
    let mut out = String::new();
    let mut caret: Option<usize> = None;
    let mut first_stop: Option<usize> = None;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            out.push(chars[i + 1]);
            i += 2;
        } else if c == '$' {
            let mut j = i + 1;
            if j < chars.len() && chars[j] == '{' {
                j += 1;
                let mut num = String::new();
                while j < chars.len() && chars[j].is_ascii_digit() {
                    num.push(chars[j]);
                    j += 1;
                }
                let mut default = String::new();
                if j < chars.len() && (chars[j] == ':' || chars[j] == '|') {
                    j += 1;
                    let mut depth = 1;
                    while j < chars.len() {
                        if chars[j] == '{' {
                            depth += 1;
                        } else if chars[j] == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        default.push(chars[j]);
                        j += 1;
                    }
                } else {
                    while j < chars.len() && chars[j] != '}' {
                        j += 1;
                    }
                }
                let default = default.split(',').next().unwrap_or("").to_string();
                if num == "0" {
                    caret = Some(out.chars().count());
                } else if first_stop.is_none() {
                    first_stop = Some(out.chars().count());
                }
                out.push_str(&default);
                i = j + 1;
            } else {
                let mut num = String::new();
                while j < chars.len() && chars[j].is_ascii_digit() {
                    num.push(chars[j]);
                    j += 1;
                }
                if num.is_empty() {
                    out.push('$');
                    i += 1;
                } else {
                    if num == "0" {
                        caret = Some(out.chars().count());
                    } else if first_stop.is_none() {
                        first_stop = Some(out.chars().count());
                    }
                    i = j;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    (out, first_stop.or(caret))
}

// ----------------------------------------------------------------------
// Server registry
// ----------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ServerSpec {
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Missing {
    pub name: &'static str,
    pub install: &'static str,
}

fn xcrun_find(tool: &str) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let out = Command::new("xcrun").args(["--find", tool]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.exists().then_some(p)
}

fn find_bin(name: &str, root: Option<&Path>) -> Option<PathBuf> {
    if let Some(r) = root {
        let local = r.join("node_modules/.bin").join(name);
        if local.exists() {
            return Some(local);
        }
    }
    crate::run::which(name)
}

/// Which servers serve this language id, in preference order.
pub fn server_group(lang: &str) -> &'static str {
    match lang {
        "javascript" | "javascriptreact" | "typescript" | "typescriptreact" => "typescript",
        "c" | "cpp" => "clangd",
        "css" | "scss" | "less" => "css",
        "plaintext" | "markdown" | "diff" | "toml" | "ini" | "makefile" | "dockerfile" => "",
        other => match other {
            "rust" => "rust",
            "python" => "python",
            "go" => "go",
            "swift" => "swift",
            "java" => "java",
            "lua" => "lua",
            "shellscript" => "bash",
            "html" => "html",
            "json" => "json",
            "yaml" => "yaml",
            "php" => "php",
            "ruby" => "ruby",
            "kotlin" => "kotlin",
            "zig" => "zig",
            "dart" => "dart",
            _ => "",
        },
    }
}

pub fn find_server(group: &str, root: Option<&Path>) -> Result<ServerSpec, Option<Missing>> {
    let spec = |name: &str, p: PathBuf, args: &[&str]| ServerSpec { name: name.to_string(), program: p, args: args.iter().map(|s| s.to_string()).collect() };
    let try_list = |cands: &[(&str, &[&str])]| -> Option<ServerSpec> {
        for (bin, args) in cands {
            if let Some(p) = find_bin(bin, root) {
                // rustup installs a stub even when the component is missing: make sure it runs
                if *bin == "rust-analyzer" && !Command::new(&p).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().map_or(false, |s| s.success()) {
                    continue;
                }
                return Some(spec(bin, p, args));
            }
        }
        None
    };
    let found = match group {
        "rust" => try_list(&[("rust-analyzer", &[])]),
        "python" => try_list(&[("pyright-langserver", &["--stdio"]), ("basedpyright-langserver", &["--stdio"]), ("pylsp", &[])]),
        "typescript" => try_list(&[("typescript-language-server", &["--stdio"])]),
        "go" => try_list(&[("gopls", &[])]),
        "clangd" => try_list(&[("clangd", &[])]).or_else(|| xcrun_find("clangd").map(|p| spec("clangd", p, &[]))),
        "swift" => try_list(&[("sourcekit-lsp", &[])]).or_else(|| xcrun_find("sourcekit-lsp").map(|p| spec("sourcekit-lsp", p, &[]))),
        "java" => try_list(&[("jdtls", &[])]),
        "lua" => try_list(&[("lua-language-server", &[])]),
        "bash" => try_list(&[("bash-language-server", &["start"])]),
        "html" => try_list(&[("vscode-html-language-server", &["--stdio"])]),
        "css" => try_list(&[("vscode-css-language-server", &["--stdio"])]),
        "json" => try_list(&[("vscode-json-language-server", &["--stdio"])]),
        "yaml" => try_list(&[("yaml-language-server", &["--stdio"])]),
        "php" => try_list(&[("intelephense", &["--stdio"])]),
        "ruby" => try_list(&[("ruby-lsp", &[]), ("solargraph", &["stdio"])]),
        "kotlin" => try_list(&[("kotlin-language-server", &[])]),
        "zig" => try_list(&[("zls", &[])]),
        "dart" => try_list(&[("dart", &["language-server"])]),
        _ => return Err(None),
    };
    found.ok_or_else(|| {
        Some(match group {
            "rust" => Missing { name: "rust-analyzer", install: "rustup component add rust-analyzer" },
            "python" => Missing { name: "pyright", install: "npm i -g pyright" },
            "typescript" => Missing { name: "typescript-language-server", install: "npm i -g typescript typescript-language-server" },
            "go" => Missing { name: "gopls", install: "go install golang.org/x/tools/gopls@latest" },
            "clangd" => Missing { name: "clangd", install: "xcode-select --install" },
            "swift" => Missing { name: "sourcekit-lsp", install: "xcode-select --install" },
            "java" => Missing { name: "jdtls", install: "brew install jdtls" },
            "lua" => Missing { name: "lua-language-server", install: "brew install lua-language-server" },
            "bash" => Missing { name: "bash-language-server", install: "npm i -g bash-language-server" },
            "html" | "css" | "json" => Missing { name: "vscode-langservers-extracted", install: "npm i -g vscode-langservers-extracted" },
            "yaml" => Missing { name: "yaml-language-server", install: "npm i -g yaml-language-server" },
            "php" => Missing { name: "intelephense", install: "npm i -g intelephense" },
            "ruby" => Missing { name: "ruby-lsp", install: "gem install ruby-lsp" },
            "kotlin" => Missing { name: "kotlin-language-server", install: "brew install kotlin-language-server" },
            "zig" => Missing { name: "zls", install: "brew install zls" },
            _ => Missing { name: "language server", install: "see docs" },
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_roundtrip() {
        let p = PathBuf::from("/tmp/my dir/ü.rs");
        let u = path_to_uri(&p);
        assert!(u.starts_with("file:///tmp/my%20dir/"));
        assert_eq!(uri_to_path(&u).unwrap(), p);
    }

    #[test]
    fn utf16_columns() {
        assert_eq!(char_to_utf16("a😀b", 2), 3);
        assert_eq!(utf16_to_char("a😀b", 3), 2);
    }

    #[test]
    fn snippets_flatten() {
        let (t, c) = snippet_to_text("foo(${1:arg}, $2)$0");
        assert_eq!(t, "foo(arg, )");
        assert_eq!(c, Some(4));
    }

    #[test]
    fn completion_parsing() {
        let v = json!({"items": [{"label": "path", "kind": 9, "detail": "module", "textEdit": {"range": {"start": {"line":0,"character":3},"end": {"line":0,"character":5}}, "newText": "path"}}]});
        let items = parse_completion(&v);
        assert_eq!(items[0].label, "path");
        assert_eq!(items[0].edit.as_ref().unwrap().col, 3);
    }
}
