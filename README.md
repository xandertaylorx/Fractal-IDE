# Fractal

A fast, minimal, modern code editor written in Rust, with VS Code muscle memory.

- **One-button Run** (top right, `F5`): detects what to run — Python (uses your `.venv`), Node/TS (node/bun/deno/tsx), Rust (`cargo run`), Go, Java (Maven / Gradle / plain `javac`+`java` with package-aware source roots), C/C++, Ruby, PHP, shell and more. HTML/CSS/JS starts a built-in **live server with auto-reload**. Also reads `.vscode/tasks.json` and `launch.json`, and `package.json` scripts.
- **VS Code keybindings** — built-in default table uses real VS Code command IDs; override with a VS Code-format `keybindings.json`. Chords (`⌘K ⌘T`), `when` clauses, multi-cursor, move/copy lines, comment toggle, find/replace (regex, case, word), go to line/file, command palette.
- **Smart editing:** Tab-to-complete suggestions with inline ghost text (identifiers from your code and open files, language keywords, snippets), auto-closing brackets/quotes/HTML tags, select text and type `"` `(` `[` `{` `` ` `` to wrap it, `⌘/` line comment, `⇧⌥A` block comment, `⌘⇧F` **Format Document** (uses rustfmt/prettier/black/ruff/gofmt/clang-format when installed, otherwise a built-in formatter).
- **Settings:** click the gear for a GUI settings page, right-click it for `settings.json`. Edits to either apply live.
- **Integrated terminal** (PTY + VT emulation), **Git** (status, stage/unstage/discard, commit, push/pull, branches, gutter marks, diffs), **project search & replace**.
- **Looks like VS Code, runs like Zed:** ships the official **Dark Modern / Light Modern / Dark+ / Light+** themes plus 20 more, and imports any **VS Code color theme** JSON. Icons are [Lucide](https://lucide.dev) (UI) and [Material Icon Theme](https://github.com/PKief/vscode-material-icon-theme) (files), rendered from SVG. TextMate grammars for ~100 languages.
- GPU rendered (wgpu/Metal), virtualized editor, minimap, indent guides.

## Build & run (macOS)

```bash
cargo run --release -p fractal-app -- path/to/project   # or ./scripts/bundle-macos.sh for Fractal.app
```

Requires Rust stable. Settings live in `~/Library/Application Support/Fractal/` (`settings.json`, `keybindings.json`, `themes/`).

## Status — honest

Early (0.1). Working and tested: the editing engine (`cargo test -p fractal-core`), themes, keymap, run detection. The GUI is verified on macOS only; **Windows/Linux build in CI but are untested** — the code is structured for it (core is UI/platform independent; ConPTY via `portable-pty`; `Ctrl` maps for `mod`).

Not yet implemented: **LSP** (completion, diagnostics, go-to-definition, hover), **debugger (DAP)**, word wrap, split editors, snippets, bracket-pair colorization, Windows custom title-bar buttons, running VS Code *extensions*. Extensions are intentionally not a goal (the VS Code extension API needs a Node host); the plan is native LSP/DAP + auto-installed language servers + Open VSX themes/snippets/grammars.

## License
MIT. Bundled third-party assets (Lucide ISC, Material Icon Theme MIT, VS Code themes MIT) are listed in `crates/fractal-app/assets/LICENSES.md`.
