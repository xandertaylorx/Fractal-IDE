//! Talks to REAL language servers if they are installed (skips otherwise).

use fractal_core::lsp::*;
use std::time::Duration;

fn complete_at(group: &str, file: &str, lang: &str, text: &str, line: usize, col: usize) -> Option<Vec<CompletionItem>> {
    let dir = std::env::temp_dir().join(format!("fractal-lsp-test-{group}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(file);
    std::fs::write(&path, text).unwrap();
    let spec = match find_server(group, Some(&dir)) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("skipping {group}: server not installed");
            return None;
        }
    };
    let mut c = LspClient::start(&spec.name, &spec.program, &spec.args, &dir, || {}).expect("start");
    c.did_open(&path, lang, text);
    // give the server a moment to index, then ask (requests queue until initialized)
    std::thread::sleep(Duration::from_millis(1500));
    let rx = c.completion(&path, line, col, None);
    let v = rx.recv_timeout(Duration::from_secs(40)).expect("completion timed out").expect("completion error");
    Some(parse_completion(&v))
}

#[test]
fn pyright_knows_os_members() {
    let Some(items) = complete_at("python", "a.py", "python", "import os\nos.pa", 1, 5) else { return };
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"path"), "pyright items: {:?}", &labels[..labels.len().min(20)]);
}

#[test]
fn typescript_knows_string_methods() {
    let Some(items) = complete_at("typescript", "a.ts", "typescript", "const s: string = 'abc';\ns.toUp", 1, 6) else { return };
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"toUpperCase"), "ts items: {:?}", &labels[..labels.len().min(20)]);
}

#[test]
fn rust_analyzer_knows_vec_methods() {
    let dir = std::env::temp_dir().join("fractal-lsp-test-rust");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
    let text = "fn main() {\n    let v: Vec<i32> = Vec::new();\n    v.pus\n}\n";
    let path = dir.join("src/main.rs");
    std::fs::write(&path, text).unwrap();
    let Ok(spec) = find_server("rust", Some(&dir)) else { return };
    let mut c = LspClient::start(&spec.name, &spec.program, &spec.args, &dir, || {}).unwrap();
    c.did_open(&path, "rust", text);
    // rust-analyzer needs time to load the crate
    let start = std::time::Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(1500));
        let rx = c.completion(&path, 2, 9, None);
        if let Ok(Ok(v)) = rx.recv_timeout(Duration::from_secs(30)) {
            let items = parse_completion(&v);
            if items.iter().any(|i| i.label == "push") {
                return;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(60), "rust-analyzer never offered `push`");
    }
}
