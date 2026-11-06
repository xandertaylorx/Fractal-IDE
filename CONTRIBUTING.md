# Contributing

- `crates/fractal-core` — UI-independent logic (buffer, keymap, themes, run detection, git, live server). Keep it free of GUI and platform-specific code; add tests in `crates/fractal-core/tests`.
- `crates/fractal-app` — egui/wgpu front end. Platform-specific bits are behind `cfg` (see `reveal()` in `app.rs`, title bar in `main.rs`/`ui.rs`).

```bash
cargo test -p fractal-core
cargo run -p fractal-app -- path/to/project
```

Good first issues: LSP client, debugger (DAP), word wrap, split editors, Windows title-bar controls, more run-detection rules in `fractal-core/src/run.rs`.
