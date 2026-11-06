//! GUI apps on macOS don't inherit the user's shell PATH; recover it.

use std::process::Command;

pub fn default_shell() -> String {
    if cfg!(windows) {
        return "powershell.exe".into();
    }
    std::env::var("SHELL").unwrap_or_else(|_| if cfg!(target_os = "macos") { "/bin/zsh".into() } else { "/bin/bash".into() })
}

/// Run the login shell once to read its PATH and merge it into ours.
pub fn import_login_path() {
    if cfg!(windows) {
        return;
    }
    let shell = default_shell();
    let out = Command::new(&shell).args(["-ilc", "printf '__FRACTAL_PATH__%s__FRACTAL_PATH__' \"$PATH\""]).stdin(std::process::Stdio::null()).output();
    let Ok(out) = out else { return };
    let s = String::from_utf8_lossy(&out.stdout);
    let Some(a) = s.find("__FRACTAL_PATH__") else { return };
    let rest = &s[a + 16..];
    let Some(b) = rest.find("__FRACTAL_PATH__") else { return };
    let login = &rest[..b];
    if login.is_empty() {
        return;
    }
    let mut parts: Vec<std::path::PathBuf> = std::env::split_paths(login).collect();
    if let Some(cur) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&cur) {
            if !parts.contains(&p) {
                parts.push(p);
            }
        }
    }
    if let Ok(joined) = std::env::join_paths(parts) {
        std::env::set_var("PATH", joined);
    }
}
