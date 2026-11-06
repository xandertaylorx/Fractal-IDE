//! VS Code compatible keybindings: parsing, chords, `when` clauses and a
//! built-in default table that uses VS Code command IDs.

use crate::jsonc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
}

impl KeyCombo {
    pub fn parse(s: &str, mac: bool) -> Option<KeyCombo> {
        let mut c = KeyCombo { key: String::new(), ctrl: false, alt: false, shift: false, cmd: false };
        let lower = s.trim().to_lowercase();
        let mut rest = lower.as_str();
        loop {
            let mut matched = false;
            for (p, f) in [("ctrl+", 0), ("control+", 0), ("alt+", 1), ("option+", 1), ("shift+", 2), ("cmd+", 3), ("meta+", 3), ("win+", 3), ("super+", 3), ("mod+", 4)] {
                if let Some(r) = rest.strip_prefix(p) {
                    if r.is_empty() {
                        break;
                    }
                    match f {
                        0 => c.ctrl = true,
                        1 => c.alt = true,
                        2 => c.shift = true,
                        3 => {
                            if mac {
                                c.cmd = true
                            } else {
                                c.ctrl = true
                            }
                        }
                        _ => {
                            if mac {
                                c.cmd = true
                            } else {
                                c.ctrl = true
                            }
                        }
                    }
                    rest = r;
                    matched = true;
                    break;
                }
            }
            if !matched {
                break;
            }
        }
        if rest.is_empty() {
            return None;
        }
        c.key = match rest {
            "esc" => "escape".into(),
            "return" => "enter".into(),
            "del" => "delete".into(),
            "arrowleft" => "left".into(),
            "arrowright" => "right".into(),
            "arrowup" => "up".into(),
            "arrowdown" => "down".into(),
            "backquote" => "`".into(),
            "bracketleft" => "[".into(),
            "bracketright" => "]".into(),
            "backslash" => "\\".into(),
            "equal" => "=".into(),
            "minus" => "-".into(),
            "slash" => "/".into(),
            "comma" => ",".into(),
            "period" => ".".into(),
            other => other.to_string(),
        };
        Some(c)
    }

    pub fn label(&self, mac: bool) -> String {
        let mut s = String::new();
        let key = match self.key.as_str() {
            "enter" => "↩".to_string(),
            "escape" => "⎋".to_string(),
            "backspace" => "⌫".to_string(),
            "delete" => "⌦".to_string(),
            "tab" => "⇥".to_string(),
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            "space" => "Space".to_string(),
            k if k.len() == 1 => k.to_uppercase(),
            k => {
                let mut cs = k.chars();
                cs.next().map(|f| f.to_uppercase().collect::<String>() + cs.as_str()).unwrap_or_default()
            }
        };
        if mac {
            if self.ctrl {
                s.push('⌃');
            }
            if self.alt {
                s.push('⌥');
            }
            if self.shift {
                s.push('⇧');
            }
            if self.cmd {
                s.push('⌘');
            }
            s.push_str(&key);
        } else {
            if self.ctrl {
                s.push_str("Ctrl+");
            }
            if self.alt {
                s.push_str("Alt+");
            }
            if self.shift {
                s.push_str("Shift+");
            }
            s.push_str(&key);
        }
        s
    }
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub keys: Vec<KeyCombo>,
    pub command: String,
    pub when: Option<String>,
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, PartialEq)]
pub enum Resolve {
    Command(String, Option<serde_json::Value>),
    /// First part of a chord was typed; wait for the next key.
    Pending,
    None,
}

pub struct Keymap {
    pub bindings: Vec<Binding>,
    pub mac: bool,
}

/// `keys | command | when | platform(mac/win/empty)`
const DEFAULTS: &str = r#"
mod+shift+p | workbench.action.showCommands
f1 | workbench.action.showCommands
mod+p | workbench.action.quickOpen
ctrl+g | workbench.action.gotoLine | | mac
ctrl+g | workbench.action.gotoLine | | win
mod+s | workbench.action.files.save
mod+alt+s | workbench.action.files.saveAll | | mac
ctrl+k s | workbench.action.files.saveAll | | win
mod+shift+s | workbench.action.files.saveAs
mod+n | workbench.action.files.newUntitledFile
mod+o | workbench.action.files.openFile
mod+w | workbench.action.closeActiveEditor
mod+shift+w | workbench.action.closeWindow
mod+b | workbench.action.toggleSidebarVisibility
mod+j | workbench.action.togglePanel
ctrl+` | workbench.action.terminal.toggleTerminal
ctrl+shift+` | workbench.action.terminal.new
mod+shift+e | workbench.view.explorer
mod+shift+h | workbench.view.search
ctrl+shift+g | workbench.view.scm
mod+, | workbench.action.openSettings
mod+k mod+s | workbench.action.openGlobalKeybindings
mod+k mod+t | workbench.action.selectTheme
mod+k m | workbench.action.editor.changeLanguageMode
mod+= | workbench.action.zoomIn
mod+shift+= | workbench.action.zoomIn
mod+- | workbench.action.zoomOut
mod+0 | workbench.action.zoomReset
ctrl+tab | workbench.action.nextEditor
ctrl+shift+tab | workbench.action.previousEditor
mod+alt+right | workbench.action.nextEditor | | mac
mod+alt+left | workbench.action.previousEditor | | mac
mod+shift+] | workbench.action.nextEditor | | mac
mod+shift+[ | workbench.action.previousEditor | | mac
ctrl+1 | workbench.action.openEditorAtIndex1
ctrl+2 | workbench.action.openEditorAtIndex2
ctrl+3 | workbench.action.openEditorAtIndex3
ctrl+4 | workbench.action.openEditorAtIndex4
ctrl+5 | workbench.action.openEditorAtIndex5
ctrl+6 | workbench.action.openEditorAtIndex6
ctrl+7 | workbench.action.openEditorAtIndex7
ctrl+8 | workbench.action.openEditorAtIndex8
ctrl+9 | workbench.action.openEditorAtIndex9
f5 | fractal.run
ctrl+f5 | fractal.run
mod+shift+r | fractal.stop
mod+r | fractal.run | | win
mod+shift+b | fractal.runPicker

mod+f | actions.find | editorFocus
mod+alt+f | editor.action.startFindReplace | editorFocus | mac
ctrl+h | editor.action.startFindReplace | editorFocus | win
f3 | editor.action.nextMatchFindAction | editorFocus
shift+f3 | editor.action.previousMatchFindAction | editorFocus
mod+g | editor.action.nextMatchFindAction | editorFocus | mac
mod+shift+g | editor.action.previousMatchFindAction | editorFocus | mac
enter | editor.action.nextMatchFindAction | findInputFocus
shift+enter | editor.action.previousMatchFindAction | findInputFocus
escape | closeFindWidget | findWidgetVisible
escape | removeSecondaryCursors | editorFocus && multiCursor
escape | workbench.action.closeQuickOpen | overlayVisible

mod+z | undo | editorFocus
mod+shift+z | redo | editorFocus
ctrl+y | redo | editorFocus | win
mod+a | editor.action.selectAll | editorFocus
mod+/ | editor.action.commentLine | editorFocus
alt+up | editor.action.moveLinesUpAction | editorFocus
alt+down | editor.action.moveLinesDownAction | editorFocus
shift+alt+up | editor.action.copyLinesUpAction | editorFocus
shift+alt+down | editor.action.copyLinesDownAction | editorFocus
mod+shift+k | editor.action.deleteLines | editorFocus
mod+enter | editor.action.insertLineAfter | editorFocus
mod+shift+enter | editor.action.insertLineBefore | editorFocus
mod+d | editor.action.addSelectionToNextFindMatch | editorFocus
mod+shift+l | editor.action.selectHighlights | editorFocus
mod+l | expandLineSelection | editorFocus
mod+] | editor.action.indentLines | editorFocus
mod+[ | editor.action.outdentLines | editorFocus
mod+alt+up | editor.action.insertCursorAbove | editorFocus | mac
mod+alt+down | editor.action.insertCursorBelow | editorFocus | mac
ctrl+alt+up | editor.action.insertCursorAbove | editorFocus | win
ctrl+alt+down | editor.action.insertCursorBelow | editorFocus | win

enter | type.newline | editorFocus
backspace | deleteLeft | editorFocus
shift+backspace | deleteLeft | editorFocus
delete | deleteRight | editorFocus
ctrl+d | deleteRight | editorFocus | mac
alt+backspace | deleteWordLeft | editorFocus
alt+delete | deleteWordRight | editorFocus
ctrl+backspace | deleteWordLeft | editorFocus | win
ctrl+delete | deleteWordRight | editorFocus | win
mod+backspace | deleteAllLeft | editorFocus | mac
ctrl+k | deleteAllRight | editorFocus | mac
tab | tab | editorFocus
shift+tab | outdent | editorFocus
left | cursorLeft | editorFocus
right | cursorRight | editorFocus
up | cursorUp | editorFocus
down | cursorDown | editorFocus
shift+left | cursorLeftSelect | editorFocus
shift+right | cursorRightSelect | editorFocus
shift+up | cursorUpSelect | editorFocus
shift+down | cursorDownSelect | editorFocus
alt+left | cursorWordLeft | editorFocus
alt+right | cursorWordRight | editorFocus
shift+alt+left | cursorWordLeftSelect | editorFocus
shift+alt+right | cursorWordRightSelect | editorFocus
ctrl+left | cursorWordLeft | editorFocus | win
ctrl+right | cursorWordRight | editorFocus | win
ctrl+shift+left | cursorWordLeftSelect | editorFocus | win
ctrl+shift+right | cursorWordRightSelect | editorFocus | win
home | cursorHome | editorFocus
end | cursorEnd | editorFocus
shift+home | cursorHomeSelect | editorFocus
shift+end | cursorEndSelect | editorFocus
mod+left | cursorHome | editorFocus | mac
mod+right | cursorEnd | editorFocus | mac
mod+shift+left | cursorHomeSelect | editorFocus | mac
mod+shift+right | cursorEndSelect | editorFocus | mac
mod+up | cursorTop | editorFocus | mac
mod+down | cursorBottom | editorFocus | mac
mod+shift+up | cursorTopSelect | editorFocus | mac
mod+shift+down | cursorBottomSelect | editorFocus | mac
ctrl+home | cursorTop | editorFocus | win
ctrl+end | cursorBottom | editorFocus | win
ctrl+shift+home | cursorTopSelect | editorFocus | win
ctrl+shift+end | cursorBottomSelect | editorFocus | win
pageup | cursorPageUp | editorFocus
pagedown | cursorPageDown | editorFocus
shift+pageup | cursorPageUpSelect | editorFocus
shift+pagedown | cursorPageDownSelect | editorFocus
ctrl+a | cursorHome | editorFocus | mac
ctrl+e | cursorEnd | editorFocus | mac
ctrl+p | cursorUp | editorFocus | mac
ctrl+n | cursorDown | editorFocus | mac
ctrl+b | cursorLeft | editorFocus | mac
ctrl+f | cursorRight | editorFocus | mac
mod+shift+f | editor.action.formatDocument | editorFocus
shift+alt+f | editor.action.formatDocument | editorFocus
shift+alt+a | editor.action.blockComment | editorFocus
ctrl+space | editor.action.triggerSuggest | editorFocus
tab | acceptSuggestion | editorFocus && suggestVisible
enter | acceptSuggestion | editorFocus && suggestVisible
escape | hideSuggestWidget | editorFocus && suggestVisible
up | selectPrevSuggestion | editorFocus && suggestVisible
down | selectNextSuggestion | editorFocus && suggestVisible
"#;

pub fn eval_when(when: &str, ctx: &dyn Fn(&str) -> bool) -> bool {
    when.split("||").any(|alt| {
        alt.split("&&").all(|term| {
            let t = term.trim();
            if t.is_empty() {
                return true;
            }
            if let Some(n) = t.strip_prefix('!') {
                !ctx(n.trim())
            } else {
                ctx(t)
            }
        })
    })
}

impl Keymap {
    pub fn new(mac: bool) -> Self {
        let mut km = Keymap { bindings: vec![], mac };
        for line in DEFAULTS.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('|').map(|p| p.trim()).collect();
            if parts.len() < 2 {
                continue;
            }
            let plat = parts.get(3).copied().unwrap_or("");
            if (plat == "mac" && !mac) || (plat == "win" && mac) {
                continue;
            }
            let keys: Vec<KeyCombo> = parts[0].split_whitespace().filter_map(|k| KeyCombo::parse(k, mac)).collect();
            if keys.is_empty() {
                continue;
            }
            let when = parts.get(2).copied().filter(|w| !w.is_empty()).map(String::from);
            km.bindings.push(Binding { keys, command: parts[1].to_string(), when, args: None });
        }
        km
    }

    /// Merge a user keybindings.json (VS Code format). `-command` removes bindings.
    pub fn load_user(&mut self, src: &str) {
        let Some(serde_json::Value::Array(arr)) = jsonc::parse(src) else { return };
        for e in arr {
            let (Some(key), Some(cmd)) = (e.get("key").and_then(|k| k.as_str()), e.get("command").and_then(|c| c.as_str())) else { continue };
            let keys: Vec<KeyCombo> = key.split_whitespace().filter_map(|k| KeyCombo::parse(k, self.mac)).collect();
            if keys.is_empty() {
                continue;
            }
            let when = e.get("when").and_then(|w| w.as_str()).map(String::from);
            if let Some(rm) = cmd.strip_prefix('-') {
                self.bindings.retain(|b| !(b.command == rm && b.keys == keys));
            } else {
                self.bindings.push(Binding { keys, command: cmd.to_string(), when, args: e.get("args").cloned() });
            }
        }
    }

    pub fn resolve(&self, prefix: &[KeyCombo], combo: &KeyCombo, ctx: &dyn Fn(&str) -> bool) -> Resolve {
        let mut seq: Vec<KeyCombo> = prefix.to_vec();
        seq.push(combo.clone());
        let mut pending = false;
        for b in self.bindings.iter().rev() {
            if b.keys.len() < seq.len() || b.keys[..seq.len()] != seq[..] {
                continue;
            }
            if let Some(w) = &b.when {
                if !eval_when(w, ctx) {
                    continue;
                }
            }
            if b.keys.len() == seq.len() {
                return Resolve::Command(b.command.clone(), b.args.clone());
            }
            pending = true;
        }
        if pending {
            Resolve::Pending
        } else {
            Resolve::None
        }
    }

    pub fn label_for(&self, command: &str) -> Option<String> {
        let b = self.bindings.iter().rev().find(|b| b.command == command)?;
        Some(b.keys.iter().map(|k| k.label(self.mac)).collect::<Vec<_>>().join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_resolve() {
        let km = Keymap::new(true);
        let all = |_: &str| true;
        let c = KeyCombo::parse("cmd+shift+p", true).unwrap();
        assert_eq!(km.resolve(&[], &c, &all), Resolve::Command("workbench.action.showCommands".into(), None));
        let k = KeyCombo::parse("cmd+k", true).unwrap();
        assert_eq!(km.resolve(&[], &k, &all), Resolve::Pending);
        let t = KeyCombo::parse("cmd+t", true).unwrap();
        assert_eq!(km.resolve(&[k], &t, &all), Resolve::Command("workbench.action.selectTheme".into(), None));
    }

    #[test]
    fn when_clauses() {
        let ctx = |n: &str| n == "a";
        assert!(eval_when("a && !b", &ctx));
        assert!(!eval_when("a && b", &ctx));
        assert!(eval_when("b || a", &ctx));
    }
}
