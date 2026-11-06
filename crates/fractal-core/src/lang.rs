//! Language metadata derived from file names.

use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LangInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub line_comment: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,
    pub colon_indent: bool,
}

const fn l(id: &'static str, name: &'static str, lc: Option<&'static str>, bc: Option<(&'static str, &'static str)>) -> LangInfo {
    LangInfo { id, name, line_comment: lc, block_comment: bc, colon_indent: false }
}

const C_LIKE: Option<&str> = Some("//");

impl LangInfo {
    pub fn plain() -> Self {
        l("plaintext", "Plain Text", None, None)
    }

    pub fn for_path(path: Option<&Path>) -> Self {
        let Some(path) = path else { return Self::plain() };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        match name {
            "Makefile" | "makefile" | "GNUmakefile" => return l("makefile", "Makefile", Some("#"), None),
            "Dockerfile" => return l("dockerfile", "Dockerfile", Some("#"), None),
            "Cargo.lock" => return l("toml", "TOML", Some("#"), None),
            _ => {}
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "rs" => l("rust", "Rust", C_LIKE, Some(("/*", "*/"))),
            "py" | "pyw" | "pyi" => LangInfo { colon_indent: true, ..l("python", "Python", Some("#"), None) },
            "js" | "mjs" | "cjs" => l("javascript", "JavaScript", C_LIKE, Some(("/*", "*/"))),
            "jsx" => l("javascriptreact", "JavaScript React", C_LIKE, Some(("/*", "*/"))),
            "ts" | "mts" | "cts" => l("typescript", "TypeScript", C_LIKE, Some(("/*", "*/"))),
            "tsx" => l("typescriptreact", "TypeScript React", C_LIKE, Some(("/*", "*/"))),
            "java" => l("java", "Java", C_LIKE, Some(("/*", "*/"))),
            "kt" | "kts" => l("kotlin", "Kotlin", C_LIKE, Some(("/*", "*/"))),
            "scala" => l("scala", "Scala", C_LIKE, None),
            "c" | "h" => l("c", "C", C_LIKE, Some(("/*", "*/"))),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => l("cpp", "C++", C_LIKE, Some(("/*", "*/"))),
            "cs" => l("csharp", "C#", C_LIKE, Some(("/*", "*/"))),
            "go" => l("go", "Go", C_LIKE, Some(("/*", "*/"))),
            "swift" => l("swift", "Swift", C_LIKE, Some(("/*", "*/"))),
            "dart" => l("dart", "Dart", C_LIKE, None),
            "php" => l("php", "PHP", C_LIKE, Some(("/*", "*/"))),
            "rb" => l("ruby", "Ruby", Some("#"), None),
            "lua" => l("lua", "Lua", Some("--"), None),
            "pl" | "pm" => l("perl", "Perl", Some("#"), None),
            "r" => l("r", "R", Some("#"), None),
            "jl" => l("julia", "Julia", Some("#"), None),
            "sh" | "bash" | "zsh" | "fish" => l("shellscript", "Shell Script", Some("#"), None),
            "ps1" => l("powershell", "PowerShell", Some("#"), None),
            "bat" | "cmd" => l("bat", "Batch", Some("REM"), None),
            "html" | "htm" => l("html", "HTML", None, Some(("<!--", "-->"))),
            "xml" | "svg" | "xsl" => l("xml", "XML", None, Some(("<!--", "-->"))),
            "css" => l("css", "CSS", None, Some(("/*", "*/"))),
            "scss" => l("scss", "SCSS", C_LIKE, Some(("/*", "*/"))),
            "less" => l("less", "Less", C_LIKE, Some(("/*", "*/"))),
            "json" | "jsonc" => l("json", "JSON", None, None),
            "toml" => l("toml", "TOML", Some("#"), None),
            "yaml" | "yml" => l("yaml", "YAML", Some("#"), None),
            "md" | "markdown" => l("markdown", "Markdown", None, Some(("<!--", "-->"))),
            "sql" => l("sql", "SQL", Some("--"), Some(("/*", "*/"))),
            "ini" | "cfg" | "conf" => l("ini", "INI", Some(";"), None),
            "hs" => l("haskell", "Haskell", Some("--"), None),
            "ex" | "exs" => l("elixir", "Elixir", Some("#"), None),
            "clj" => l("clojure", "Clojure", Some(";;"), None),
            "zig" => l("zig", "Zig", C_LIKE, None),
            "vue" | "svelte" => l("html", "HTML", None, Some(("<!--", "-->"))),
            "txt" | "log" => Self::plain(),
            _ => Self::plain(),
        }
    }
}
