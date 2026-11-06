//! Static file server with live reload (for HTML/CSS/JS projects).

use notify::{RecursiveMode, Watcher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Response, Server, StatusCode};

pub struct LiveServer {
    pub port: u16,
    pub root: PathBuf,
    stop: Arc<AtomicBool>,
    _watcher: Option<notify::RecommendedWatcher>,
}

const RELOAD_JS: &str = "<script>(function(){var v=null;function p(){fetch('/__fractal_reload',{cache:'no-store'}).then(function(r){return r.text()}).then(function(t){if(v===null){v=t}else if(v!==t){location.reload();return}setTimeout(p,400)}).catch(function(){setTimeout(p,1200)})}p()})();</script>";

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn respond(req: tiny_http::Request, code: u16, ctype: &str, body: Vec<u8>) {
    let mut r = Response::new(StatusCode(code), vec![], Cursor::new(body.clone()), Some(body.len()), None);
    if let Ok(h) = Header::from_bytes("Content-Type", ctype) {
        r.add_header(h);
    }
    if let Ok(h) = Header::from_bytes("Cache-Control", "no-store") {
        r.add_header(h);
    }
    let _ = req.respond(r);
}

impl LiveServer {
    pub fn start(root: PathBuf) -> Result<LiveServer, String> {
        let server = Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = server.server_addr().to_ip().map(|a| a.port()).ok_or("no port")?;
        let server = Arc::new(server);
        let stop = Arc::new(AtomicBool::new(false));
        let version = Arc::new(AtomicU64::new(1));

        let v2 = version.clone();
        let root_w = root.clone();
        let watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(ev) = res {
                let noisy = ev.paths.iter().all(|p| p.components().any(|c| matches!(c.as_os_str().to_str(), Some(".git") | Some("node_modules") | Some("target"))));
                if !noisy {
                    v2.fetch_add(1, Ordering::SeqCst);
                }
            }
        })
        .ok()
        .and_then(|mut w| {
            w.watch(&root_w, RecursiveMode::Recursive).ok()?;
            Some(w)
        });

        for _ in 0..4 {
            let server = server.clone();
            let stop = stop.clone();
            let version = version.clone();
            let root = root.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(200)) else { continue };
                    let url = req.url().to_string();
                    let path_part = url.split('?').next().unwrap_or("/");
                    if path_part == "/__fractal_reload" {
                        respond(req, 200, "text/plain", version.load(Ordering::SeqCst).to_string().into_bytes());
                        continue;
                    }
                    let rel = percent_decode(path_part);
                    let mut target = root.clone();
                    let mut ok = true;
                    for comp in rel.split('/') {
                        match comp {
                            "" | "." => {}
                            ".." => ok = false,
                            c => target.push(c),
                        }
                    }
                    if !ok {
                        respond(req, 403, "text/plain", b"forbidden".to_vec());
                        continue;
                    }
                    if target.is_dir() {
                        let idx = target.join("index.html");
                        if idx.exists() {
                            target = idx;
                        } else {
                            let mut html = String::from("<!doctype html><meta charset=utf-8><title>Index</title><body style='font-family:system-ui;padding:2rem'><h2>Index of ");
                            html.push_str(path_part);
                            html.push_str("</h2><ul>");
                            if let Ok(rd) = std::fs::read_dir(&target) {
                                let mut names: Vec<String> = rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string() + if e.path().is_dir() { "/" } else { "" }).collect();
                                names.sort();
                                for n in names {
                                    html.push_str(&format!("<li><a href=\"{}{}\">{}</a></li>", if path_part.ends_with('/') { path_part.to_string() } else { format!("{path_part}/") }, n, n));
                                }
                            }
                            html.push_str("</ul>");
                            html.push_str(RELOAD_JS);
                            respond(req, 200, "text/html; charset=utf-8", html.into_bytes());
                            continue;
                        }
                    }
                    match std::fs::read(&target) {
                        Ok(mut bytes) => {
                            let m = mime(&target);
                            if m.starts_with("text/html") {
                                let mut s = String::from_utf8_lossy(&bytes).to_string();
                                match s.rfind("</body>") {
                                    Some(i) => s.insert_str(i, RELOAD_JS),
                                    None => s.push_str(RELOAD_JS),
                                }
                                bytes = s.into_bytes();
                            }
                            respond(req, 200, m, bytes);
                        }
                        Err(_) => respond(req, 404, "text/html; charset=utf-8", format!("<h1>404</h1><p>{}</p>{}", path_part, RELOAD_JS).into_bytes()),
                    }
                }
            });
        }
        Ok(LiveServer { port, root, stop, _watcher: watcher })
    }

    pub fn url_for(&self, file: &Path) -> String {
        let rel = file.strip_prefix(&self.root).unwrap_or(file);
        let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().replace(' ', "%20")).collect();
        format!("http://127.0.0.1:{}/{}", self.port, parts.join("/"))
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
