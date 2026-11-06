//! The "Run" button brain: figures out what to run for the current file/project.

use crate::jsonc;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum RunKind {
    /// A shell command line to type into the integrated terminal.
    Shell { line: String, cwd: PathBuf },
    /// Serve a folder with live reload and open the browser.
    LiveServer { root: PathBuf, open: PathBuf },
    /// Something is missing (toolchain, etc.).
    Unavailable(String),
}

#[derive(Clone, Debug)]
pub struct RunSpec {
    pub label: String,
    pub detail: String,
    pub kind: RunKind,
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or(".EXE;.CMD;.BAT".into()).split(';').map(|s| s.to_string()).collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for e in &exts {
            let cand = dir.join(format!("{name}{e}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

pub fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-:=@%+,".contains(c)) {
        return s.to_string();
    }
    if cfg!(windows) {
        format!("\"{}\"", s.replace('"', "`\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn find_up(start: &Path, names: &[&str]) -> Option<PathBuf> {
    let mut cur = Some(start);
    for _ in 0..12 {
        let d = cur?;
        if names.iter().any(|n| d.join(n).exists()) {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

fn tmp_out(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("fractal-run");
    let _ = std::fs::create_dir_all(&d);
    d.join(name)
}

fn spec(label: impl Into<String>, detail: impl Into<String>, line: String, cwd: &Path) -> RunSpec {
    RunSpec { label: label.into(), detail: detail.into(), kind: RunKind::Shell { line, cwd: cwd.to_path_buf() } }
}

fn python_for(dirs: &[&Path]) -> String {
    for d in dirs {
        for rel in [".venv/bin/python", "venv/bin/python", "env/bin/python", ".venv/Scripts/python.exe", "venv/Scripts/python.exe"] {
            let p = d.join(rel);
            if p.is_file() {
                return quote(&p.to_string_lossy());
            }
        }
    }
    if which("python3").is_some() { "python3".into() } else { "python".into() }
}

fn java_available() -> bool {
    match std::process::Command::new("java").arg("-version").output() {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

fn file_specs(file: &Path, root: Option<&Path>) -> Vec<RunSpec> {
    let mut out = vec![];
    let dir = file.parent().unwrap_or(Path::new("."));
    let fname = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = file.file_stem().and_then(|n| n.to_str()).unwrap_or("out");
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let f = quote(&file.to_string_lossy());
    let dirs: Vec<&Path> = match root {
        Some(r) => vec![dir, r],
        None => vec![dir],
    };
    let missing = |tool: &str, hint: &str| RunSpec { label: format!("Run {fname}"), detail: format!("{tool} not found"), kind: RunKind::Unavailable(format!("`{tool}` was not found on your PATH. {hint}")) };
    match ext.as_str() {
        "html" | "htm" => {
            let r = root.filter(|r| file.starts_with(r)).unwrap_or(dir);
            out.push(RunSpec { label: format!("Live Server: {fname}"), detail: "serve with live reload".into(), kind: RunKind::LiveServer { root: r.to_path_buf(), open: file.to_path_buf() } });
        }
        "css" | "scss" | "less" => {
            let idx = ["index.html", "index.htm"].iter().map(|n| dir.join(n)).find(|p| p.exists());
            if let Some(idx) = idx {
                out.push(RunSpec { label: "Live Server: index.html".into(), detail: "serve with live reload".into(), kind: RunKind::LiveServer { root: dir.to_path_buf(), open: idx } });
            }
        }
        "py" | "pyw" => {
            let py = python_for(&dirs);
            out.push(spec(format!("Run {fname}"), format!("{py} {fname}"), format!("{py} {f}"), dir));
        }
        "js" | "mjs" | "cjs" => {
            if which("node").is_some() {
                out.push(spec(format!("Run {fname}"), "node", format!("node {f}"), dir));
            } else if which("bun").is_some() {
                out.push(spec(format!("Run {fname}"), "bun", format!("bun run {f}"), dir));
            } else if which("deno").is_some() {
                out.push(spec(format!("Run {fname}"), "deno", format!("deno run -A {f}"), dir));
            } else {
                out.push(missing("node", "Install Node.js from https://nodejs.org"));
            }
        }
        "ts" | "mts" | "tsx" => {
            if which("bun").is_some() {
                out.push(spec(format!("Run {fname}"), "bun", format!("bun run {f}"), dir));
            } else if which("deno").is_some() {
                out.push(spec(format!("Run {fname}"), "deno", format!("deno run -A {f}"), dir));
            } else if which("npx").is_some() {
                out.push(spec(format!("Run {fname}"), "npx tsx", format!("npx --yes tsx {f}"), dir));
            } else {
                out.push(missing("node", "Install Node.js from https://nodejs.org"));
            }
        }
        "rs" => {
            if let Some(cd) = find_up(dir, &["Cargo.toml"]) {
                let rel = file.strip_prefix(&cd).unwrap_or(file);
                let comps: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
                let extra = if comps.len() >= 3 && comps[0] == "src" && comps[1] == "bin" {
                    format!(" --bin {}", quote(stem))
                } else if comps.len() >= 2 && comps[0] == "examples" {
                    format!(" --example {}", quote(stem))
                } else {
                    String::new()
                };
                out.push(spec("cargo run", format!("cargo run{extra}"), format!("cargo run{extra}"), &cd));
            } else if which("rustc").is_some() {
                let o = tmp_out(stem);
                let oq = quote(&o.to_string_lossy());
                out.push(spec(format!("Run {fname}"), "rustc", format!("rustc {f} -o {oq} && {oq}"), dir));
            } else {
                out.push(missing("rustc", "Install Rust from https://rustup.rs"));
            }
        }
        "go" => {
            if which("go").is_some() {
                out.push(spec(format!("Run {fname}"), "go run", format!("go run {f}"), dir));
            } else {
                out.push(missing("go", "Install Go from https://go.dev/dl"));
            }
        }
        "c" => {
            if let Some(cc) = ["cc", "clang", "gcc"].iter().find(|c| which(c).is_some()) {
                let o = quote(&tmp_out(stem).to_string_lossy());
                out.push(spec(format!("Run {fname}"), *cc, format!("{cc} {f} -o {o} && {o}"), dir));
            } else {
                out.push(missing("cc", "Install Xcode command line tools: xcode-select --install"));
            }
        }
        "cpp" | "cc" | "cxx" => {
            if let Some(cc) = ["c++", "clang++", "g++"].iter().find(|c| which(c).is_some()) {
                let o = quote(&tmp_out(stem).to_string_lossy());
                out.push(spec(format!("Run {fname}"), *cc, format!("{cc} -std=c++17 {f} -o {o} && {o}"), dir));
            } else {
                out.push(missing("c++", "Install Xcode command line tools: xcode-select --install"));
            }
        }
        "java" => out.extend(java_specs(file, root)),
        "kt" => {
            if which("kotlinc").is_some() {
                let jar = quote(&tmp_out(&format!("{stem}.jar")).to_string_lossy());
                out.push(spec(format!("Run {fname}"), "kotlinc", format!("kotlinc {f} -include-runtime -d {jar} && java -jar {jar}"), dir));
            } else {
                out.push(missing("kotlinc", "Install Kotlin: brew install kotlin"));
            }
        }
        "cs" => {
            if find_up(dir, &["*.csproj"]).is_some() || dir.read_dir().map_or(false, |mut d| d.any(|e| e.map_or(false, |e| e.path().extension().map_or(false, |x| x == "csproj")))) {
                out.push(spec("dotnet run", "dotnet run", "dotnet run".into(), dir));
            }
        }
        "rb" => out.push(spec(format!("Run {fname}"), "ruby", format!("ruby {f}"), dir)),
        "php" => out.push(spec(format!("Run {fname}"), "php", format!("php {f}"), dir)),
        "lua" => out.push(spec(format!("Run {fname}"), "lua", format!("lua {f}"), dir)),
        "pl" => out.push(spec(format!("Run {fname}"), "perl", format!("perl {f}"), dir)),
        "r" => out.push(spec(format!("Run {fname}"), "Rscript", format!("Rscript {f}"), dir)),
        "jl" => out.push(spec(format!("Run {fname}"), "julia", format!("julia {f}"), dir)),
        "dart" => out.push(spec(format!("Run {fname}"), "dart", format!("dart run {f}"), dir)),
        "swift" => out.push(spec(format!("Run {fname}"), "swift", format!("swift {f}"), dir)),
        "zig" => out.push(spec(format!("Run {fname}"), "zig", format!("zig run {f}"), dir)),
        "hs" => out.push(spec(format!("Run {fname}"), "runghc", format!("runghc {f}"), dir)),
        "exs" | "ex" => out.push(spec(format!("Run {fname}"), "elixir", format!("elixir {f}"), dir)),
        "sh" | "bash" | "zsh" => {
            let sh = if ext == "zsh" { "zsh" } else { "bash" };
            out.push(spec(format!("Run {fname}"), sh, format!("{sh} {f}"), dir));
        }
        "ps1" => out.push(spec(format!("Run {fname}"), "pwsh", format!("pwsh -File {f}"), dir)),
        "bat" | "cmd" => out.push(spec(format!("Run {fname}"), "cmd", f.clone(), dir)),
        _ => {}
    }
    out
}

fn java_specs(file: &Path, root: Option<&Path>) -> Vec<RunSpec> {
    let dir = file.parent().unwrap_or(Path::new("."));
    let fname = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = file.file_stem().and_then(|n| n.to_str()).unwrap_or("Main");
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let pkg = text
        .lines()
        .map(|l| l.trim())
        .find(|l| l.starts_with("package "))
        .map(|l| l.trim_start_matches("package ").trim_end_matches(';').trim().to_string());
    let fqcn = match &pkg {
        Some(p) => format!("{p}.{stem}"),
        None => stem.to_string(),
    };
    let mut out = vec![];
    if !java_available() {
        return vec![RunSpec {
            label: format!("Run {fname}"),
            detail: "JDK not found".into(),
            kind: RunKind::Unavailable("No Java Development Kit was found. Install one (e.g. `brew install openjdk`) and restart Fractal.".into()),
        }];
    }
    // source root = dir minus package path
    let mut src_root = dir.to_path_buf();
    if let Some(p) = &pkg {
        for _ in p.split('.') {
            src_root = src_root.parent().map(|x| x.to_path_buf()).unwrap_or(src_root);
        }
    }
    let has_main = text.contains("static void main") || text.contains("void main(");
    if let Some(pom) = find_up(dir, &["pom.xml"]) {
        let wrapper = if pom.join("mvnw").exists() { "./mvnw" } else { "mvn" };
        out.push(spec(format!("Maven: {stem}"), "mvn compile exec:java", format!("{wrapper} -q compile exec:java -Dexec.mainClass={}", quote(&fqcn)), &pom));
    } else if let Some(g) = find_up(dir, &["build.gradle", "build.gradle.kts"]) {
        let wrapper = if g.join("gradlew").exists() { "./gradlew" } else { "gradle" };
        out.push(spec("Gradle: run", "gradle run", format!("{wrapper} run"), &g));
    }
    if has_main || out.is_empty() {
        let hash = file.to_string_lossy().bytes().fold(5381u64, |h, b| h.wrapping_mul(33) ^ b as u64);
        let outdir = tmp_out(&format!("java-{hash:x}"));
        let o = quote(&outdir.to_string_lossy());
        let line = format!("javac -d {o} -sourcepath {} {} && java -cp {o} {}", quote(&src_root.to_string_lossy()), quote(&file.to_string_lossy()), quote(&fqcn));
        out.insert(0, spec(format!("Run {fname}"), "javac + java", line, root.unwrap_or(&src_root)));
    }
    out
}

fn project_specs(root: &Path) -> Vec<RunSpec> {
    let mut out = vec![];
    if root.join("Cargo.toml").exists() {
        out.push(spec("cargo run", "Rust project", "cargo run".into(), root));
    }
    if let Ok(pkg) = std::fs::read_to_string(root.join("package.json")) {
        if let Some(v) = jsonc::parse(&pkg) {
            let pm = if root.join("pnpm-lock.yaml").exists() {
                "pnpm"
            } else if root.join("yarn.lock").exists() {
                "yarn"
            } else if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
                "bun"
            } else {
                "npm"
            };
            let install = if root.join("node_modules").exists() { String::new() } else { format!("{pm} install && ") };
            if let Some(scripts) = v.get("scripts").and_then(|s| s.as_object()) {
                for name in ["dev", "start", "serve", "develop", "watch"] {
                    if scripts.contains_key(name) {
                        let cmd = if pm == "npm" && name != "start" { format!("npm run {name}") } else if pm == "npm" { "npm start".into() } else { format!("{pm} run {name}") };
                        out.push(spec(format!("{pm} run {name}"), scripts[name].as_str().unwrap_or("").to_string(), format!("{install}{cmd}"), root));
                    }
                }
            } else if let Some(main) = v.get("main").and_then(|m| m.as_str()) {
                out.push(spec(format!("node {main}"), "package.json main", format!("node {}", quote(main)), root));
            }
        }
    }
    if root.join("go.mod").exists() {
        out.push(spec("go run .", "Go module", "go run .".into(), root));
    }
    if root.join("pom.xml").exists() {
        let w = if root.join("mvnw").exists() { "./mvnw" } else { "mvn" };
        let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap_or_default();
        if pom.contains("spring-boot") {
            out.push(spec("Spring Boot", "mvn spring-boot:run", format!("{w} spring-boot:run"), root));
        } else {
            out.push(spec("Maven: package", "mvn package", format!("{w} -q package"), root));
        }
    }
    for g in ["build.gradle", "build.gradle.kts"] {
        if root.join(g).exists() {
            let w = if root.join("gradlew").exists() { "./gradlew" } else { "gradle" };
            let txt = std::fs::read_to_string(root.join(g)).unwrap_or_default();
            let task = if txt.contains("org.springframework.boot") { "bootRun" } else { "run" };
            out.push(spec(format!("Gradle: {task}"), "", format!("{w} {task}"), root));
            break;
        }
    }
    if root.join("manage.py").exists() {
        let py = python_for(&[root]);
        out.push(spec("Django: runserver", "manage.py", format!("{py} manage.py runserver"), root));
    }
    for m in ["main.py", "app.py", "run.py", "__main__.py", "src/main.py"] {
        if root.join(m).exists() {
            let py = python_for(&[root]);
            out.push(spec(format!("Run {m}"), "Python entry", format!("{py} {m}"), root));
            break;
        }
    }
    for h in ["index.html", "public/index.html", "dist/index.html", "src/index.html"] {
        let p = root.join(h);
        if p.exists() {
            let r = p.parent().unwrap().to_path_buf();
            out.push(RunSpec { label: format!("Live Server: {h}"), detail: "serve with live reload".into(), kind: RunKind::LiveServer { root: r, open: p } });
            break;
        }
    }
    if root.join("Makefile").exists() {
        out.push(spec("make", "Makefile", "make".into(), root));
    }
    if root.join("deno.json").exists() || root.join("deno.jsonc").exists() {
        out.push(spec("deno task dev", "", "deno task dev".into(), root));
    }
    out
}

fn vscode_specs(root: &Path, file: Option<&Path>) -> Vec<RunSpec> {
    let mut out = vec![];
    let sub = |s: &str| -> String {
        let mut r = s.replace("${workspaceFolder}", &root.to_string_lossy()).replace("${workspaceRoot}", &root.to_string_lossy());
        if let Some(f) = file {
            r = r.replace("${file}", &f.to_string_lossy());
            r = r.replace("${fileDirname}", &f.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
            r = r.replace("${fileBasenameNoExtension}", &f.file_stem().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
        }
        r
    };
    if let Ok(t) = std::fs::read_to_string(root.join(".vscode/tasks.json")) {
        if let Some(v) = jsonc::parse(&t) {
            for task in v.get("tasks").and_then(|t| t.as_array()).into_iter().flatten() {
                let Some(label) = task.get("label").and_then(|l| l.as_str()) else { continue };
                let Some(cmd) = task.get("command").and_then(|c| c.as_str()) else { continue };
                let args: Vec<String> = task.get("args").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| quote(&sub(s)))).collect()).unwrap_or_default();
                let line = if args.is_empty() { sub(cmd) } else { format!("{} {}", sub(cmd), args.join(" ")) };
                out.push(spec(format!("Task: {label}"), "tasks.json", line, root));
            }
        }
    }
    if let Ok(t) = std::fs::read_to_string(root.join(".vscode/launch.json")) {
        if let Some(v) = jsonc::parse(&t) {
            for c in v.get("configurations").and_then(|c| c.as_array()).into_iter().flatten() {
                let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("launch");
                let ty = c.get("type").and_then(|n| n.as_str()).unwrap_or("");
                let program = c.get("program").and_then(|n| n.as_str()).map(&sub);
                let args: Vec<String> = c.get("args").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| quote(&sub(s)))).collect()).unwrap_or_default();
                let tail = if args.is_empty() { String::new() } else { format!(" {}", args.join(" ")) };
                let cwd = c.get("cwd").and_then(|n| n.as_str()).map(|s| PathBuf::from(sub(s))).unwrap_or_else(|| root.to_path_buf());
                let line = match (ty, program) {
                    ("node", Some(p)) | ("pwa-node", Some(p)) => Some(format!("node {}{tail}", quote(&p))),
                    ("python", Some(p)) | ("debugpy", Some(p)) => Some(format!("{} {}{tail}", python_for(&[root]), quote(&p))),
                    ("go", Some(p)) => Some(format!("go run {}{tail}", quote(&p))),
                    _ => None,
                };
                if let Some(line) = line {
                    out.push(RunSpec { label: format!("Launch: {name}"), detail: "launch.json".into(), kind: RunKind::Shell { line, cwd } });
                }
            }
        }
    }
    out
}

/// Ordered list of things to run; the first entry is the default.
pub fn detect(root: Option<&Path>, file: Option<&Path>) -> Vec<RunSpec> {
    let mut out = vec![];
    if let Some(f) = file {
        out.extend(file_specs(f, root));
    }
    if let Some(r) = root {
        out.extend(project_specs(r));
        out.extend(vscode_specs(r, file));
    } else if let Some(f) = file {
        if let Some(d) = f.parent() {
            out.extend(project_specs(d));
        }
    }
    // De-duplicate identical labels
    let mut seen = std::collections::HashSet::new();
    out.retain(|s| seen.insert(s.label.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_live_server() {
        let specs = detect(None, Some(Path::new("/tmp/site/index.html")));
        assert!(matches!(specs[0].kind, RunKind::LiveServer { .. }));
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("abc/def.py"), "abc/def.py");
    }
}
