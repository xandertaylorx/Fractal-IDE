//! Application state, input routing and command execution.

use crate::commands::{self, COMMANDS};
use crate::editor::{self, Tab};
use crate::explorer::{Explorer, ExplorerAction};
use crate::job::Job;
use crate::overlay::{Item, Kind, Outcome, Overlay, Payload};
use crate::scm::{Scm, ScmAction};
use crate::search_panel::{SearchAction, SearchPanel};
use crate::terminal::Term;
use crate::ui_theme;
use eframe::egui::{self, Event, Key};
use fractal_core::buffer::{Buffer, EditKind, Motion};
use fractal_core::complete::{self, Suggestion};
use fractal_core::git;
use fractal_core::keymap::{KeyCombo, Keymap, Resolve};
use fractal_core::liveserver::LiveServer;
use fractal_core::run::{self, RunKind, RunSpec};
use fractal_core::search::SearchQuery;
use fractal_core::settings::{self, Session, Settings};
use fractal_core::theme::{self, Theme};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Editor,
    Terminal,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SideView {
    Explorer,
    Search,
    Scm,
}

pub struct Toast {
    pub text: String,
    pub born: Instant,
}

#[derive(Default)]
pub struct FindState {
    pub visible: bool,
    pub replace_visible: bool,
    pub query: SearchQuery,
    pub replace: String,
    pub matches: Vec<(usize, usize)>,
    pub current: Option<usize>,
    pub focus_input: bool,
    pub computed_for: Option<(u64, SearchQuery, usize)>,
    pub input_id: Option<egui::Id>,
    pub tab_id: usize,
}

pub struct Suggest {
    pub items: Vec<Suggestion>,
    pub selected: usize,
    pub start: usize,
    pub prefix: String,
}

pub struct App {
    pub lsp: crate::lsp_app::LspHub,
    pub last_typed: Option<char>,
    pub suggest: Option<Suggest>,
    pub suggest_dirty: bool,
    pub settings_reload: bool,
    pub keybindings_reload: bool,
    pub settings_filter: String,
    pub settings: Settings,
    pub session: Session,
    pub theme: Theme,
    pub themes: Vec<Theme>,
    pub keymap: Keymap,
    pub chord: Vec<KeyCombo>,
    pub mac: bool,
    pub folder: Option<PathBuf>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub closed_stack: Vec<PathBuf>,
    pub focus: Focus,
    pub side_view: SideView,
    pub sidebar_visible: bool,
    pub panel_visible: bool,
    pub explorer: Explorer,
    pub search: SearchPanel,
    pub scm: Scm,
    pub terms: Vec<Term>,
    pub active_term: usize,
    pub term_counter: usize,
    pub run_term: Option<usize>,
    pub overlay: Option<Overlay>,
    pub find: FindState,
    pub toasts: Vec<Toast>,
    pub file_index: Arc<Mutex<Vec<PathBuf>>>,
    pub index_job: Option<Job<Vec<PathBuf>>>,
    pub run_specs: Vec<RunSpec>,
    pub run_job: Option<Job<Vec<RunSpec>>>,
    pub run_key: Option<(Option<PathBuf>, Option<PathBuf>)>,
    pub run_choice: HashMap<String, usize>,
    pub live: Option<LiveServer>,
    pub git_marks_job: Option<(usize, Job<Vec<(usize, git::LineMark)>>)>,
    pub git_marks_for: HashMap<usize, u64>,
    pub tab_ids: Vec<usize>,
    pub next_tab_id: usize,
    pub last_activity: Instant,
    pub last_poll: Instant,
    pub untitled_counter: usize,
    pub editor_rect: egui::Rect,
    pub fonts_loaded: bool,
    pub applied_zoom: f32,
    pub last_title: String,
    pub quit_confirmed: bool,
    pub git_status_map: HashMap<PathBuf, char>,
    pub needs_session_save: bool,
    pub autorun: bool,
    pub autorun_settings: bool,
    pub autocmd: Option<String>,
    pub autotype: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mac = cfg!(target_os = "macos");
        let settings = Settings::load();
        let session = Session::load();
        let mut themes: Vec<Theme> = theme::vscode_defaults();
        themes.extend(theme::builtin_names().iter().filter_map(|n| theme::builtin(n)));
        if let Ok(rd) = std::fs::read_dir(settings::themes_dir()) {
            for e in rd.flatten() {
                if e.path().extension().map_or(false, |x| x == "json") {
                    if let Ok(s) = std::fs::read_to_string(e.path()) {
                        if let Some(t) = theme::from_vscode_json(&s, &e.file_name().to_string_lossy()) {
                            themes.push(t);
                        }
                    }
                }
            }
        }
        let theme = themes.iter().find(|t| t.name == settings.theme).cloned().unwrap_or_else(|| themes[0].clone());
        ui_theme::setup_fonts(&cc.egui_ctx, &settings.font_family);
        ui_theme::apply(&cc.egui_ctx, &theme);
        cc.egui_ctx.set_zoom_factor(settings.ui_scale);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut keymap = Keymap::new(mac);
        if let Ok(s) = std::fs::read_to_string(settings::keybindings_path()) {
            keymap.load_user(&s);
        }
        let mut app = App {
            lsp: Default::default(),
            last_typed: None,
            suggest: None,
            suggest_dirty: false,
            settings_reload: false,
            keybindings_reload: false,
            settings_filter: String::new(),
            applied_zoom: settings.ui_scale,
            settings,
            session: session.clone(),
            theme,
            themes,
            keymap,
            chord: vec![],
            mac,
            folder: None,
            tabs: vec![],
            active: 0,
            closed_stack: vec![],
            focus: Focus::Editor,
            side_view: SideView::Explorer,
            sidebar_visible: session.sidebar_visible.unwrap_or(true),
            panel_visible: false,
            explorer: Explorer::new(),
            search: SearchPanel::new(),
            scm: Scm::new(),
            terms: vec![],
            active_term: 0,
            term_counter: 0,
            run_term: None,
            overlay: None,
            find: FindState::default(),
            toasts: vec![],
            file_index: Arc::new(Mutex::new(vec![])),
            index_job: None,
            run_specs: vec![],
            run_job: None,
            run_key: None,
            run_choice: HashMap::new(),
            live: None,
            git_marks_job: None,
            git_marks_for: HashMap::new(),
            tab_ids: vec![],
            next_tab_id: 1,
            last_activity: Instant::now(),
            last_poll: Instant::now(),
            untitled_counter: 0,
            editor_rect: egui::Rect::NOTHING,
            fonts_loaded: true,
            last_title: String::new(),
            quit_confirmed: false,
            git_status_map: HashMap::new(),
            needs_session_save: false,
            autorun: std::env::var("FRACTAL_AUTORUN").is_ok(),
            autorun_settings: std::env::var("FRACTAL_SETTINGS").is_ok(),
            autocmd: std::env::var("FRACTAL_CMD").ok(),
            autotype: std::env::var("FRACTAL_TYPE").ok(),
        };
        // CLI args: fractal <folder|file>...
        let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).filter(|p| p.exists()).collect();
        if !args.is_empty() {
            for a in args {
                let a = std::fs::canonicalize(&a).unwrap_or(a);
                if a.is_dir() {
                    app.set_folder(&cc.egui_ctx, Some(a));
                } else {
                    if app.folder.is_none() {
                        if let Some(p) = a.parent() {
                            app.set_folder(&cc.egui_ctx, Some(p.to_path_buf()));
                        }
                    }
                    app.open_file(&a, None);
                }
            }
        } else if let Some(f) = session.folder.clone().filter(|f| f.exists()) {
            app.set_folder(&cc.egui_ctx, Some(f));
            for p in session.open_files.iter().filter(|p| p.exists()) {
                app.open_file(p, None);
            }
            app.active = session.active.min(app.tabs.len().saturating_sub(1));
        }
        app
    }

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast { text: text.into(), born: Instant::now() });
    }

    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn active_path(&self) -> Option<PathBuf> {
        self.tab().and_then(|t| t.buf.path.clone())
    }

    pub fn set_folder(&mut self, ctx: &egui::Context, folder: Option<PathBuf>) {
        self.folder = folder.clone();
        self.explorer.set_root(folder.clone(), ctx);
        self.scm.set_folder(folder.as_deref());
        self.run_key = None;
        self.live = None;
        self.index_job = None;
        if let Some(f) = &folder {
            self.session.folder = Some(f.clone());
            self.session.recent_folders.retain(|r| r != f);
            self.session.recent_folders.insert(0, f.clone());
            self.session.recent_folders.truncate(12);
            let f = f.clone();
            let idx = self.file_index.clone();
            self.index_job = Some(Job::spawn(ctx, move || {
                let files = fractal_core::fsutil::list_files(&f, 200_000);
                *idx.lock().unwrap() = files.clone();
                files
            }));
        } else {
            *self.file_index.lock().unwrap() = vec![];
        }
        self.search.hits.clear();
        self.needs_session_save = true;
    }

    pub fn set_theme(&mut self, ctx: &egui::Context, name: &str) {
        if let Some(t) = self.themes.iter().find(|t| t.name == name).cloned() {
            ui_theme::apply(ctx, &t);
            for tab in &mut self.tabs {
                tab.hl.set_theme(&tab.buf, &t);
            }
            self.theme = t;
        }
    }

    pub fn open_file(&mut self, path: &Path, goto: Option<(usize, usize, usize)>) {
        if let Some(i) = self.tabs.iter().position(|t| t.buf.path.as_deref() == Some(path)) {
            self.active = i;
            if let Some(g) = goto {
                self.tabs[i].pending_goto = Some(g);
            }
            self.focus = Focus::Editor;
            return;
        }
        match std::fs::metadata(path) {
            Ok(m) if m.len() > 64_000_000 => {
                self.toast("File is too large to open (over 64 MB).");
                return;
            }
            _ => {}
        }
        match std::fs::read(path) {
            Ok(bytes) => {
                if fractal_core::fsutil::is_binary(&bytes) {
                    let _ = open::that(path);
                    self.toast(format!("{} is a binary file — opened with the system viewer.", path.file_name().unwrap_or_default().to_string_lossy()));
                    return;
                }
                let text = String::from_utf8_lossy(&bytes);
                let mut buf = Buffer::from_text(&text, Some(path.to_path_buf()));
                buf.disk_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
                let mut tab = Tab::new(buf, &self.theme);
                tab.pending_goto = goto;
                let insert_at = (self.active + 1).min(self.tabs.len());
                let insert_at = if self.tabs.is_empty() { 0 } else { insert_at };
                self.tabs.insert(insert_at, tab);
                self.active = insert_at;
                self.focus = Focus::Editor;
                self.explorer.reveal(path);
                self.refresh_marks(self.active);
                self.needs_session_save = true;
            }
            Err(e) => self.toast(format!("Could not open file: {e}")),
        }
    }

    pub fn new_untitled(&mut self) {
        self.untitled_counter += 1;
        let mut buf = Buffer::new();
        buf.indent.size = self.settings.tab_size;
        buf.indent.use_spaces = self.settings.insert_spaces;
        let mut tab = Tab::new(buf, &self.theme);
        tab.untitled_n = self.untitled_counter;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.focus = Focus::Editor;
    }

    pub fn open_virtual(&mut self, title: &str, ext: &str, text: &str) {
        let buf = Buffer::from_text(text, Some(PathBuf::from(format!("{title}.{ext}"))));
        let mut buf = buf;
        buf.path = None;
        let mut tab = Tab::new(buf, &self.theme);
        tab.title_override = Some(title.to_string());
        // highlight using a fake path for syntax selection
        tab.buf.lang = fractal_core::lang::LangInfo::for_path(Some(Path::new(&format!("x.{ext}"))));
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    pub fn refresh_marks(&mut self, idx: usize) {
        // computed lazily in tick(); here we just invalidate
        self.git_marks_for.remove(&idx);
    }

    pub fn save_tab(&mut self, idx: usize) -> bool {
        if self.settings.format_on_save {
            let project = self.folder.clone();
            if let Some(t) = self.tabs.get_mut(idx) {
                if !t.settings_page && t.buf.path.is_some() && !matches!(t.buf.lang.id, "plaintext" | "markdown") {
                    let text = t.buf.text();
                    if let Ok(f) = fractal_core::format::format_document(t.buf.lang.id, &text, t.buf.path.as_deref(), project.as_deref(), t.buf.indent) {
                        if f.text != text {
                            t.buf.replace_all_text(&f.text);
                        }
                    }
                }
            }
        }
        let Some(tab) = self.tabs.get_mut(idx) else { return false };
        if tab.settings_page {
            return true;
        }
        if tab.buf.path.is_none() {
            let Some(p) = rfd::FileDialog::new().set_title("Save As").set_directory(self.folder.clone().unwrap_or_else(|| dirs::home_dir().unwrap_or_default())).save_file() else { return false };
            tab.title_override = None;
            if let Err(e) = tab.buf.save_as(&p) {
                self.toast(format!("Save failed: {e}"));
                return false;
            }
            tab.hl.syntax_changed(&tab.buf);
            tab.buf.mark_all_dirty();
        } else if let Err(e) = tab.buf.save() {
            let msg = format!("Save failed: {e}");
            self.toast(msg);
            return false;
        }
        self.scm.want_refresh = true;
        self.git_marks_for.remove(&idx);
        self.needs_session_save = true;
        if let Some(p) = self.tabs.get(idx).and_then(|t| t.buf.path.clone()) {
            self.lsp_sync_active(true);
            self.lsp_did_save(&p);
            if p == settings::settings_path() {
                self.settings_reload = true;
            } else if p == settings::keybindings_path() {
                self.keybindings_reload = true;
            }
        }
        true
    }

    pub fn save_as(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        let mut dlg = rfd::FileDialog::new().set_title("Save As");
        if let Some(p) = &tab.buf.path {
            if let Some(d) = p.parent() {
                dlg = dlg.set_directory(d);
            }
            if let Some(n) = p.file_name() {
                dlg = dlg.set_file_name(n.to_string_lossy());
            }
        }
        if let Some(p) = dlg.save_file() {
            match tab.buf.save_as(&p) {
                Ok(()) => {
                    tab.title_override = None;
                    tab.hl.syntax_changed(&tab.buf);
                    tab.buf.mark_all_dirty();
                }
                Err(e) => {
                    let m = format!("Save failed: {e}");
                    self.toast(m);
                }
            }
        }
    }

    /// Returns false if the user cancelled.
    pub fn close_tab(&mut self, idx: usize, ask: bool) -> bool {
        if idx >= self.tabs.len() {
            return true;
        }
        if ask && self.tabs[idx].buf.is_dirty() {
            let name = self.tabs[idx].title();
            let r = rfd::MessageDialog::new()
                .set_title("Unsaved changes")
                .set_description(format!("Do you want to save the changes you made to {name}?"))
                .set_buttons(rfd::MessageButtons::YesNoCancelCustom("Save".into(), "Don't Save".into(), "Cancel".into()))
                .show();
            match r {
                rfd::MessageDialogResult::Custom(s) if s == "Save" => {
                    if !self.save_tab(idx) {
                        return false;
                    }
                }
                rfd::MessageDialogResult::Yes => {
                    if !self.save_tab(idx) {
                        return false;
                    }
                }
                rfd::MessageDialogResult::Custom(s) if s == "Don't Save" => {}
                rfd::MessageDialogResult::No => {}
                _ => return false,
            }
        }
        if let Some(p) = self.tabs[idx].buf.path.clone() {
            self.lsp_did_close(&p);
            self.closed_stack.push(p);
        }
        self.tabs.remove(idx);
        self.git_marks_for.clear();
        if self.active > idx || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len().saturating_sub(1));
        }
        self.needs_session_save = true;
        true
    }

    pub fn has_dirty(&self) -> bool {
        self.tabs.iter().any(|t| t.buf.is_dirty())
    }

    // ------------------------------------------------------------------
    // terminals
    // ------------------------------------------------------------------

    pub fn new_terminal(&mut self, ctx: &egui::Context, cwd: Option<&Path>) -> Option<usize> {
        let dir = cwd.map(|p| p.to_path_buf()).or_else(|| self.folder.clone()).or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
        self.term_counter += 1;
        let title = format!("{} {}", fractal_core::shellenv::default_shell().rsplit(['/', '\\']).next().unwrap_or("shell"), self.term_counter);
        match Term::spawn(ctx, &dir, &title) {
            Ok(t) => {
                self.terms.push(t);
                self.active_term = self.terms.len() - 1;
                self.panel_visible = true;
                self.focus = Focus::Terminal;
                Some(self.active_term)
            }
            Err(e) => {
                self.toast(format!("Could not start terminal: {e}"));
                None
            }
        }
    }

    pub fn toggle_terminal(&mut self, ctx: &egui::Context) {
        if self.panel_visible && self.focus == Focus::Terminal {
            self.panel_visible = false;
            self.focus = Focus::Editor;
        } else {
            if self.terms.is_empty() {
                self.new_terminal(ctx, None);
            }
            self.panel_visible = true;
            self.focus = Focus::Terminal;
        }
    }

    // ------------------------------------------------------------------
    // run
    // ------------------------------------------------------------------

    fn run_key_now(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        (self.active_path(), self.folder.clone())
    }

    pub fn tick_run_specs(&mut self, ctx: &egui::Context) {
        if let Some(j) = &self.run_job {
            if let Some(specs) = j.poll() {
                self.run_specs = specs;
                self.run_job = None;
            }
        }
        let key = self.run_key_now();
        if self.run_key.as_ref() != Some(&key) && self.run_job.is_none() {
            self.run_key = Some(key.clone());
            let (file, folder) = key;
            self.run_job = Some(Job::spawn(ctx, move || run::detect(folder.as_deref(), file.as_deref())));
        }
    }

    pub fn preferred_run(&self) -> Option<usize> {
        if self.run_specs.is_empty() {
            return None;
        }
        let key = self.active_path().map(|p| p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()).unwrap_or_default();
        Some(self.run_choice.get(&key).copied().filter(|i| *i < self.run_specs.len()).unwrap_or(0))
    }

    pub fn run_current(&mut self, ctx: &egui::Context, which: Option<usize>) {
        // make sure specs match the current file
        let key = self.run_key_now();
        if self.run_key.as_ref() != Some(&key) || self.run_job.is_some() {
            let specs = run::detect(self.folder.as_deref(), self.active_path().as_deref());
            self.run_specs = specs;
            self.run_key = Some(key);
            self.run_job = None;
        }
        let Some(idx) = which.or_else(|| self.preferred_run()) else {
            self.toast("Nothing to run here. Open a source file or a project folder.");
            return;
        };
        let spec = self.run_specs[idx].clone();
        // remember the choice per file type
        let ext = self.active_path().and_then(|p| p.extension().map(|e| e.to_string_lossy().to_string())).unwrap_or_default();
        self.run_choice.insert(ext, idx);
        // save dirty files first
        let dirty: Vec<usize> = (0..self.tabs.len()).filter(|&i| self.tabs[i].buf.is_dirty() && self.tabs[i].buf.path.is_some()).collect();
        for i in dirty {
            self.save_tab(i);
        }
        match spec.kind {
            RunKind::Unavailable(msg) => self.toast(msg),
            RunKind::LiveServer { root, open } => {
                let restart = self.live.as_ref().map_or(true, |l| l.root != root);
                if restart {
                    self.live = None;
                    match LiveServer::start(root.clone()) {
                        Ok(s) => self.live = Some(s),
                        Err(e) => {
                            self.toast(format!("Could not start live server: {e}"));
                            return;
                        }
                    }
                }
                let url = self.live.as_ref().unwrap().url_for(&open);
                let _ = open::that(&url);
                self.toast(format!("Live Server running at {url}"));
            }
            RunKind::Shell { line, cwd } => {
                let full = if cfg!(windows) { format!("cd '{}'; {}", cwd.display(), line) } else { format!("cd {} && {}", run::quote(&cwd.to_string_lossy()), line) };
                let reuse = self.run_term.filter(|&i| i < self.terms.len() && self.terms[i].alive.load(std::sync::atomic::Ordering::Relaxed));
                let ti = match reuse {
                    Some(i) => {
                        self.terms[i].interrupt();
                        i
                    }
                    None => {
                        let Some(i) = self.new_terminal(ctx, Some(&cwd)) else { return };
                        self.terms[i].title = "Run".into();
                        self.run_term = Some(i);
                        i
                    }
                };
                self.active_term = ti;
                self.panel_visible = true;
                let clear = if cfg!(windows) { "cls; " } else { "clear; " };
                self.terms[ti].send_line(&format!("{clear}{full}"));
                self.focus = Focus::Terminal;
            }
        }
    }

    pub fn stop_run(&mut self) {
        if let Some(i) = self.run_term {
            if let Some(t) = self.terms.get_mut(i) {
                t.interrupt();
            }
        }
        if self.live.take().is_some() {
            self.toast("Live server stopped");
        }
    }

    // ------------------------------------------------------------------
    // find
    // ------------------------------------------------------------------

    pub fn open_find(&mut self, replace: bool) {
        self.find.visible = true;
        self.find.replace_visible = replace || self.find.replace_visible && replace;
        if replace {
            self.find.replace_visible = true;
        }
        self.find.focus_input = true;
        if let Some(t) = self.tab() {
            let s = t.buf.primary();
            if !s.is_empty() {
                let text = t.buf.slice_string(s.start(), s.end());
                if !text.contains('\n') && text.len() < 200 {
                    self.find.query.text = text;
                }
            }
        }
    }

    pub fn refresh_find(&mut self) {
        if !self.find.visible {
            return;
        }
        let Some(t) = self.tabs.get(self.active) else { return };
        let key = (t.buf.version, self.find.query.clone(), self.active);
        if self.find.computed_for.as_ref() == Some(&key) {
            return;
        }
        let query_changed = self.find.computed_for.as_ref().map_or(true, |k| k.1 != key.1 || k.2 != key.2);
        self.find.matches = t.buf.find_all(&self.find.query);
        self.find.computed_for = Some(key);
        let head = t.buf.primary().start();
        self.find.current = self.find.matches.iter().position(|m| m.0 >= head).or(if self.find.matches.is_empty() { None } else { Some(0) });
        if query_changed && self.find.current.is_some() && !self.find.query.text.is_empty() {
            self.find_select_current();
        }
    }

    fn find_select_current(&mut self) {
        if let (Some(i), Some(t)) = (self.find.current, self.tabs.get_mut(self.active)) {
            if let Some(&(a, b)) = self.find.matches.get(i) {
                t.buf.set_selection(a, b);
                t.reveal = true;
            }
        }
    }

    pub fn find_step(&mut self, forward: bool) {
        if !self.find.visible || self.find.query.text.is_empty() {
            // reuse previous query if available
            if self.find.query.text.is_empty() {
                self.open_find(false);
                return;
            }
        }
        self.refresh_find();
        if self.find.matches.is_empty() {
            return;
        }
        let n = self.find.matches.len();
        let Some(t) = self.tab() else { return };
        let sel = t.buf.primary();
        let next = if forward {
            self.find.matches.iter().position(|m| m.0 >= sel.end() && !(m.0 == sel.start() && m.1 == sel.end())).unwrap_or(0)
        } else {
            self.find.matches.iter().rposition(|m| m.1 <= sel.start()).unwrap_or(n - 1)
        };
        self.find.current = Some(next);
        self.find_select_current();
    }

    pub fn find_replace(&mut self, all: bool) {
        let q = self.find.query.clone();
        let rep = self.find.replace.clone();
        let cur = self.find.current.and_then(|i| self.find.matches.get(i).copied());
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        if all {
            let n = t.buf.replace_matches(&q, &rep, None);
            self.toast(format!("Replaced {n} occurrences"));
        } else if let Some(m) = cur {
            t.buf.replace_matches(&q, &rep, Some(m));
            t.reveal = true;
            self.find.computed_for = None;
            self.refresh_find();
            self.find_step(true);
        }
    }

    // ------------------------------------------------------------------
    // overlays
    // ------------------------------------------------------------------

    fn command_items(&self) -> Vec<Item> {
        COMMANDS.iter().map(|(id, title)| Item { label: title.to_string(), detail: String::new(), right: String::new(), payload: Payload::Command(id.to_string()) }).collect()
    }

    pub fn open_overlay(&mut self, kind: Kind, input: &str) {
        let commands = self.command_items();
        let (placeholder, items) = match kind {
            Kind::QuickOpen => {
                let mut items: Vec<Item> = vec![];
                let root = self.folder.clone();
                let mut seen = HashSet::new();
                for t in self.tabs.iter().rev() {
                    if let Some(p) = &t.buf.path {
                        if seen.insert(p.clone()) {
                            items.push(file_item(p, root.as_deref(), true));
                        }
                    }
                }
                for p in self.file_index.lock().unwrap().iter() {
                    if !seen.contains(p) {
                        items.push(file_item(p, root.as_deref(), false));
                    }
                }
                ("Go to file…  (type > for commands, : for line)".to_string(), items)
            }
            Kind::Theme => {
                let items = self.themes.iter().map(|t| Item { label: t.name.clone(), detail: if t.dark { "dark".into() } else { "light".into() }, right: if t.name == self.theme.name { "current".into() } else { String::new() }, payload: Payload::Theme(t.name.clone()) }).collect();
                ("Select Color Theme (↑↓ to preview)".to_string(), items)
            }
            Kind::Run => {
                let items = self.run_specs.iter().enumerate().map(|(i, s)| Item { label: s.label.clone(), detail: s.detail.clone(), right: String::new(), payload: Payload::Run(i) }).collect();
                ("Choose what to run".to_string(), items)
            }
            Kind::Branch => {
                let items = self.scm.repo.as_ref().map(|r| git::branches(r)).unwrap_or_default().into_iter().map(|b| Item { label: b.clone(), detail: String::new(), right: String::new(), payload: Payload::Branch(b) }).collect();
                ("Select a branch — or type a new name and press Enter to create it".to_string(), items)
            }
            Kind::Recent => {
                let items = self.session.recent_folders.iter().map(|f| Item { label: f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), detail: f.to_string_lossy().to_string(), right: String::new(), payload: Payload::Folder(f.clone()) }).collect();
                ("Open recent folder".to_string(), items)
            }
        };
        let mut ov = Overlay::new(kind, input, &placeholder, items, commands);
        if kind == Kind::Theme {
            ov.original_theme = Some(self.theme.name.clone());
            ov.selected = self.themes.iter().position(|t| t.name == self.theme.name).unwrap_or(0);
        }
        self.overlay = Some(ov);
    }

    pub fn close_overlay(&mut self, ctx: &egui::Context) {
        if let Some(ov) = self.overlay.take() {
            if let Some(orig) = ov.original_theme {
                if orig != self.theme.name {
                    self.set_theme(ctx, &orig);
                }
            }
        }
    }

    pub fn show_overlay(&mut self, ctx: &egui::Context) {
        let Some(mut ov) = self.overlay.take() else { return };
        let km = &self.keymap;
        let outcome = ov.show(ctx, &self.theme, &|id| km.label_for(id));
        match outcome {
            Outcome::None => self.overlay = Some(ov),
            Outcome::Preview(Payload::Theme(n)) => {
                self.set_theme(ctx, &n);
                self.overlay = Some(ov);
            }
            Outcome::Preview(_) => self.overlay = Some(ov),
            Outcome::Close => {
                self.overlay = Some(ov);
                self.close_overlay(ctx);
            }
            Outcome::Accept(p, _) => {
                let keep_theme = matches!(p, Payload::Theme(_));
                if keep_theme {
                    ov.original_theme = None;
                }
                self.overlay = Some(ov);
                self.close_overlay(ctx);
                self.accept(ctx, p);
            }
        }
    }

    fn accept(&mut self, ctx: &egui::Context, p: Payload) {
        match p {
            Payload::Command(id) => self.exec(ctx, &id),
            Payload::File(path) => self.open_file(&path, None),
            Payload::Theme(n) => {
                self.set_theme(ctx, &n);
                self.settings.theme = n;
                self.settings.save();
            }
            Payload::Run(i) => self.run_current(ctx, Some(i)),
            Payload::Branch(b) => {
                self.scm.run_op(ctx, "Checkout", move |r| git::checkout(r, &b));
            }
            Payload::NewBranch(b) => {
                self.scm.run_op(ctx, "Checkout", move |r| git::create_branch(r, &b));
            }
            Payload::Folder(f) => self.set_folder(ctx, Some(f)),
            Payload::Line(l) => {
                if let Some(t) = self.tab_mut() {
                    let pos = t.buf.pos_to_char(l, 0);
                    t.buf.set_single_caret(pos);
                    t.reveal = true;
                }
                self.focus = Focus::Editor;
            }
            Payload::OpenTab(i) => self.active = i.min(self.tabs.len().saturating_sub(1)),
            Payload::Nothing => {}
        }
    }

    // ------------------------------------------------------------------
    // input
    // ------------------------------------------------------------------

    fn context_flags(&self, ctx: &egui::Context) -> HashSet<&'static str> {
        let mut f = HashSet::new();
        let text_focus = ctx.text_edit_focused();
        if self.overlay.is_some() {
            f.insert("overlayVisible");
        }
        if self.focus == Focus::Editor && !text_focus && self.overlay.is_none() && self.tabs.get(self.active).map_or(false, |t| !t.settings_page) {
            f.insert("editorFocus");
            if self.suggest.is_some() {
                f.insert("suggestVisible");
            }
        }
        if self.focus == Focus::Terminal && !text_focus && self.overlay.is_none() {
            f.insert("terminalFocus");
        }
        if self.find.visible {
            f.insert("findWidgetVisible");
            if let Some(id) = self.find.input_id {
                if ctx.memory(|m| m.has_focus(id)) {
                    f.insert("findInputFocus");
                }
            }
        }
        if self.tab().map_or(false, |t| t.buf.sels.len() > 1) {
            f.insert("multiCursor");
        }
        if self.folder.is_some() {
            f.insert("hasFolder");
        }
        f
    }

    pub fn handle_input(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        if events.is_empty() {
            return;
        }
        self.handle_events(ctx, events);
        if self.suggest_dirty {
            self.suggest_dirty = false;
            let c = self.last_typed.take();
            self.after_typing(c);
        }
    }

    pub fn compute_suggest(&mut self, min_prefix: usize) {
        self.suggest = None;
        if !self.settings.suggestions || self.focus != Focus::Editor {
            return;
        }
        let active = self.active;
        // refresh word caches of the other tabs
        for (i, t) in self.tabs.iter_mut().enumerate() {
            if i == active || t.settings_page || t.buf.len_chars() > 600_000 {
                continue;
            }
            if t.words_cache.as_ref().map_or(true, |(v, _)| *v != t.buf.version) {
                let mut m = std::collections::HashMap::new();
                complete::index_words(&t.buf.text(), &mut m);
                t.words_cache = Some((t.buf.version, m));
            }
        }
        let mut other: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for (i, t) in self.tabs.iter().enumerate() {
            if i == active {
                continue;
            }
            if let Some((_, m)) = &t.words_cache {
                for (k, v) in m {
                    *other.entry(k.clone()).or_insert(0) += v;
                }
            }
        }
        let Some(t) = self.tabs.get(active) else { return };
        if t.settings_page || t.buf.sels.len() != 1 || !t.buf.primary().is_empty() {
            return;
        }
        let (prefix, start) = t.buf.prefix_at_caret();
        if prefix.chars().count() < min_prefix.max(1) || prefix.chars().next().map_or(true, |c| c.is_ascii_digit()) {
            return;
        }
        let local = t.buf.nearby_words(400);
        let items = complete::suggest(&complete::Inputs { prefix: &prefix, lang: t.buf.lang.id, local_words: &local, other_words: &other, max: 8 });
        if !items.is_empty() {
            self.suggest = Some(Suggest { items, selected: 0, start, prefix });
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context, events: Vec<Event>) {
        for ev in events {
            let flags = self.context_flags(ctx);
            let editor_focus = flags.contains("editorFocus");
            let term_focus = flags.contains("terminalFocus");
            match &ev {
                Event::Key { key, pressed: true, modifiers, .. } => {
                    self.last_activity = Instant::now();
                    let Some(name) = key_name(*key) else {
                        if term_focus {
                            if let Some(t) = self.terms.get_mut(self.active_term) {
                                t.handle_event(&ev);
                            }
                        }
                        continue;
                    };
                    let combo = KeyCombo { key: name.to_string(), ctrl: modifiers.ctrl, alt: modifiers.alt, shift: modifiers.shift, cmd: modifiers.mac_cmd };
                    let chord = std::mem::take(&mut self.chord);
                    match self.keymap.resolve(&chord, &combo, &|n| flags.contains(n)) {
                        Resolve::Command(id, _) => self.exec(ctx, &id),
                        Resolve::Pending => self.chord = {
                            let mut c = chord;
                            c.push(combo);
                            c
                        },
                        Resolve::None => {
                            if !chord.is_empty() {
                                // swallowed an aborted chord
                            } else if term_focus {
                                if let Some(t) = self.terms.get_mut(self.active_term) {
                                    t.handle_event(&ev);
                                }
                            }
                        }
                    }
                }
                Event::Text(t) => {
                    self.last_activity = Instant::now();
                    if editor_focus {
                        if let Some(tab) = self.tabs.get_mut(self.active) {
                            let chars: Vec<char> = t.chars().filter(|c| !c.is_control()).collect();
                            let single = if chars.len() == 1 { Some(chars[0]) } else { None };
                            if chars.len() == 1 {
                                tab.buf.type_char(chars[0]);
                            } else if !chars.is_empty() {
                                tab.buf.insert_text(&chars.iter().collect::<String>(), EditKind::Other);
                            }
                            tab.reveal = true;
                            match single {
                                Some(c) => {
                                    self.suggest_dirty = true;
                                    self.last_typed = Some(c);
                                }
                                None => self.suggest = None,
                            }
                        }
                    } else if term_focus {
                        if let Some(tm) = self.terms.get_mut(self.active_term) {
                            tm.handle_event(&ev);
                        }
                    }
                }
                Event::Ime(egui::ImeEvent::Commit(t)) => {
                    if editor_focus {
                        if let Some(tab) = self.tabs.get_mut(self.active) {
                            tab.buf.insert_text(t, EditKind::Other);
                            tab.reveal = true;
                        }
                    }
                }
                Event::Paste(s) => {
                    if editor_focus {
                        if let Some(tab) = self.tabs.get_mut(self.active) {
                            tab.buf.paste(s);
                            tab.reveal = true;
                        }
                    } else if term_focus {
                        if let Some(tm) = self.terms.get_mut(self.active_term) {
                            tm.handle_event(&ev);
                        }
                    }
                }
                Event::Copy | Event::Cut => {
                    if editor_focus {
                        let cut = matches!(ev, Event::Cut);
                        if let Some(tab) = self.tabs.get_mut(self.active) {
                            let text = tab.buf.copy_text();
                            ctx.copy_text(text);
                            if cut {
                                if tab.buf.has_selection() {
                                    tab.buf.insert_text("", EditKind::Other);
                                } else {
                                    tab.buf.delete_lines();
                                }
                                tab.reveal = true;
                            }
                        }
                    } else if term_focus {
                        if let Some(tm) = self.terms.get_mut(self.active_term) {
                            match tm.selection_text() {
                                Some(t) => ctx.copy_text(t),
                                None => {
                                    if !self.mac {
                                        tm.write(&[3]);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // commands
    // ------------------------------------------------------------------

    fn motion(&mut self, m: Motion, extend: bool) {
        if let Some(t) = self.tab_mut() {
            let m = match m {
                Motion::PageUp(_) => Motion::PageUp(t.viewport_lines.saturating_sub(2)),
                Motion::PageDown(_) => Motion::PageDown(t.viewport_lines.saturating_sub(2)),
                o => o,
            };
            t.buf.move_cursors(m, extend);
            t.reveal = true;
        }
    }

    fn edit(&mut self, f: impl FnOnce(&mut Buffer)) {
        if let Some(t) = self.tab_mut() {
            f(&mut t.buf);
            t.reveal = true;
        }
    }

    pub fn exec(&mut self, ctx: &egui::Context, id: &str) {
        self.last_activity = Instant::now();
        if !matches!(id, "acceptSuggestion" | "selectNextSuggestion" | "selectPrevSuggestion" | "deleteLeft") {
            self.suggest = None;
        }
        match id {
            "acceptSuggestion" => {
                if let (Some(sg), Some(t)) = (self.suggest.take(), self.tabs.get_mut(self.active)) {
                    if let Some(item) = sg.items.get(sg.selected) {
                        t.buf.accept_completion(item.edit_start.unwrap_or(sg.start), &item.insert);
                        t.reveal = true;
                    }
                }
            }
            "hideSuggestWidget" => self.suggest = None,
            "selectNextSuggestion" => {
                if let Some(sg) = &mut self.suggest {
                    sg.selected = (sg.selected + 1) % sg.items.len();
                }
            }
            "selectPrevSuggestion" => {
                if let Some(sg) = &mut self.suggest {
                    sg.selected = (sg.selected + sg.items.len() - 1) % sg.items.len();
                }
            }
            "editor.action.triggerSuggest" => {
                if self.lsp_status().map_or(false, |s| s.1) {
                    self.lsp_request_completion(None);
                } else {
                    self.compute_suggest(1);
                }
            }
            "editor.action.revealDefinition" => {
                if let Some(t) = self.tab() {
                    let h = t.buf.primary().head;
                    self.lsp_definition(h);
                }
            }
            "editor.action.formatDocument" => self.format_active(),
            "editor.action.blockComment" => self.edit(|b| b.toggle_block_comment()),
            "workbench.action.openSettings" => self.open_settings_page(),
            "workbench.action.openSettingsJson" => {
                self.settings.save();
                let p = settings::settings_path();
                self.open_file(&p, None);
            }
            // --- workbench ---
            "workbench.action.showCommands" => {
                if self.overlay.is_some() {
                    self.close_overlay(ctx);
                } else {
                    self.open_overlay(Kind::QuickOpen, ">")
                }
            }
            "workbench.action.quickOpen" => {
                if self.overlay.is_some() {
                    self.close_overlay(ctx);
                } else {
                    self.open_overlay(Kind::QuickOpen, "")
                }
            }
            "workbench.action.gotoLine" => self.open_overlay(Kind::QuickOpen, ":"),
            "workbench.action.closeQuickOpen" => self.close_overlay(ctx),
            "workbench.action.selectTheme" => self.open_overlay(Kind::Theme, ""),
            "workbench.action.openRecent" => self.open_overlay(Kind::Recent, ""),
            "fractal.runPicker" => {
                self.run_current_prepare();
                self.open_overlay(Kind::Run, "");
            }
            "git.checkout" | "git.createBranch" => self.open_overlay(Kind::Branch, ""),
            "workbench.action.files.save" => {
                let a = self.active;
                self.save_tab(a);
            }
            "workbench.action.files.saveAs" => self.save_as(),
            "workbench.action.files.saveAll" => {
                for i in 0..self.tabs.len() {
                    if self.tabs[i].buf.is_dirty() {
                        self.save_tab(i);
                    }
                }
            }
            "workbench.action.files.newUntitledFile" => self.new_untitled(),
            "workbench.action.files.openFile" => {
                if let Some(files) = rfd::FileDialog::new().set_directory(self.folder.clone().unwrap_or_else(|| dirs::home_dir().unwrap_or_default())).pick_files() {
                    for f in files {
                        self.open_file(&f, None);
                    }
                }
            }
            "workbench.action.files.openFolder" => {
                if let Some(f) = rfd::FileDialog::new().set_title("Open Folder").pick_folder() {
                    self.set_folder(ctx, Some(f));
                    self.sidebar_visible = true;
                    self.side_view = SideView::Explorer;
                }
            }
            "workbench.action.closeFolder" => {
                self.set_folder(ctx, None);
            }
            "workbench.action.closeActiveEditor" => {
                let a = self.active;
                self.close_tab(a, true);
            }
            "workbench.action.reopenClosedEditor" => {
                if let Some(p) = self.closed_stack.pop() {
                    self.open_file(&p, None);
                }
            }
            "workbench.action.closeWindow" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "workbench.action.nextEditor" => {
                if !self.tabs.is_empty() {
                    self.active = (self.active + 1) % self.tabs.len();
                }
            }
            "workbench.action.previousEditor" => {
                if !self.tabs.is_empty() {
                    self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
                }
            }
            s if s.starts_with("workbench.action.openEditorAtIndex") => {
                if let Ok(n) = s.trim_start_matches("workbench.action.openEditorAtIndex").parse::<usize>() {
                    if n >= 1 && n <= self.tabs.len() {
                        self.active = n - 1;
                    }
                }
            }
            "workbench.action.toggleSidebarVisibility" => self.sidebar_visible = !self.sidebar_visible,
            "workbench.action.togglePanel" => {
                self.panel_visible = !self.panel_visible;
                if self.panel_visible && self.terms.is_empty() {
                    self.new_terminal(ctx, None);
                }
            }
            "workbench.view.explorer" => self.show_side(SideView::Explorer),
            "workbench.view.search" | "workbench.action.findInFiles" => {
                self.show_side(SideView::Search);
                self.search.focus_input = true;
                if let Some(t) = self.tab() {
                    let s = t.buf.primary();
                    if !s.is_empty() {
                        let text = t.buf.slice_string(s.start(), s.end());
                        if !text.contains('\n') {
                            self.search.query.text = text;
                        }
                    }
                }
            }
            "workbench.view.scm" => self.show_side(SideView::Scm),
            "workbench.action.terminal.toggleTerminal" => self.toggle_terminal(ctx),
            "workbench.action.terminal.new" => {
                self.new_terminal(ctx, None);
            }
            "workbench.action.terminal.kill" => {
                if self.active_term < self.terms.len() {
                    self.terms[self.active_term].kill();
                    self.terms.remove(self.active_term);
                    self.run_term = None;
                    self.active_term = self.active_term.saturating_sub(1);
                    if self.terms.is_empty() {
                        self.panel_visible = false;
                        self.focus = Focus::Editor;
                    }
                }
            }
            "workbench.action.openGlobalKeybindings" => {
                let p = settings::keybindings_path();
                if !p.exists() {
                    let _ = std::fs::write(&p, settings::DEFAULT_KEYBINDINGS);
                }
                self.open_file(&p, None);
            }
            "workbench.action.zoomIn" => {
                self.settings.ui_scale = (self.settings.ui_scale + 0.1).min(2.5);
                self.settings.save();
            }
            "workbench.action.zoomOut" => {
                self.settings.ui_scale = (self.settings.ui_scale - 0.1).max(0.6);
                self.settings.save();
            }
            "workbench.action.zoomReset" => {
                self.settings.ui_scale = 1.0;
                self.settings.save();
            }
            "fractal.toggleMinimap" => {
                self.settings.minimap = !self.settings.minimap;
                self.settings.save();
            }
            "fractal.toggleAutoSave" => {
                self.settings.auto_save = !self.settings.auto_save;
                self.settings.save();
                let on = self.settings.auto_save;
                self.toast(format!("Auto Save {}", if on { "on" } else { "off" }));
            }
            "fractal.importTheme" => {
                if let Some(p) = rfd::FileDialog::new().add_filter("VS Code theme", &["json"]).pick_file() {
                    match std::fs::read_to_string(&p).ok().and_then(|s| theme::from_vscode_json(&s, &p.file_stem().unwrap_or_default().to_string_lossy())) {
                        Some(t) => {
                            let dest = settings::themes_dir().join(p.file_name().unwrap());
                            let _ = std::fs::copy(&p, dest);
                            let name = t.name.clone();
                            self.themes.retain(|x| x.name != name);
                            self.themes.push(t);
                            self.set_theme(ctx, &name);
                            self.settings.theme = name.clone();
                            self.settings.save();
                            self.toast(format!("Imported theme “{name}”"));
                        }
                        None => self.toast("That file is not a valid VS Code color theme."),
                    }
                }
            }
            "fractal.revealInFinder" => {
                if let Some(p) = self.active_path() {
                    reveal(&p);
                }
            }
            "fractal.copyPath" => {
                if let Some(p) = self.active_path() {
                    ctx.copy_text(p.to_string_lossy().to_string());
                }
            }
            "fractal.about" => self.toast(format!("Fractal IDE {} — built in Rust", env!("CARGO_PKG_VERSION"))),
            // --- run ---
            "fractal.run" => self.run_current(ctx, None),
            "fractal.stop" => self.stop_run(),
            // --- git ---
            "git.refresh" => self.scm.want_refresh = true,
            "git.push" => self.scm.run_op(ctx, "Push", git::push),
            "git.pull" => self.scm.run_op(ctx, "Pull", git::pull),
            "git.stageAll" => self.scm.run_op(ctx, "Stage", |r| git::stage_all(r).map(|_| String::new())),
            "git.init" => {
                if let Some(f) = self.folder.clone() {
                    let _ = git::init(&f);
                    self.scm.set_folder(Some(&f));
                }
            }
            // --- find ---
            "actions.find" => self.open_find(false),
            "editor.action.startFindReplace" => self.open_find(true),
            "editor.action.nextMatchFindAction" => self.find_step(true),
            "editor.action.previousMatchFindAction" => self.find_step(false),
            "closeFindWidget" => {
                self.find.visible = false;
                self.focus = Focus::Editor;
                ctx.memory_mut(|m| {
                    if let Some(f) = m.focused() {
                        m.surrender_focus(f);
                    }
                });
            }
            // --- editor ---
            "undo" => self.edit(|b| b.undo()),
            "redo" => self.edit(|b| b.redo()),
            "editor.action.selectAll" => self.edit(|b| b.select_all()),
            "type.newline" => self.edit(|b| b.newline()),
            "deleteLeft" => {
                self.edit(|b| b.backspace());
                if self.suggest.is_some() {
                    self.suggest_dirty = true;
                    self.last_typed = None;
                }
            }
            "deleteRight" => self.edit(|b| b.delete_forward()),
            "deleteWordLeft" => self.edit(|b| b.delete_word_left()),
            "deleteWordRight" => self.edit(|b| b.delete_word_right()),
            "deleteAllLeft" => self.edit(|b| b.delete_to_line_start()),
            "deleteAllRight" => self.edit(|b| b.delete_to_line_end()),
            "tab" => self.edit(|b| b.tab()),
            "outdent" => self.edit(|b| b.shift_tab()),
            "editor.action.indentLines" => self.edit(|b| b.indent_lines()),
            "editor.action.outdentLines" => self.edit(|b| b.outdent_lines()),
            "editor.action.commentLine" => self.edit(|b| b.toggle_comment()),
            "editor.action.moveLinesUpAction" => self.edit(|b| b.move_lines(true)),
            "editor.action.moveLinesDownAction" => self.edit(|b| b.move_lines(false)),
            "editor.action.copyLinesUpAction" => self.edit(|b| b.copy_lines(true)),
            "editor.action.copyLinesDownAction" => self.edit(|b| b.copy_lines(false)),
            "editor.action.deleteLines" => self.edit(|b| b.delete_lines()),
            "editor.action.insertLineAfter" => self.edit(|b| b.insert_line_below()),
            "editor.action.insertLineBefore" => self.edit(|b| b.insert_line_above()),
            "editor.action.addSelectionToNextFindMatch" => self.edit(|b| b.add_next_occurrence()),
            "editor.action.selectHighlights" => self.edit(|b| b.select_all_occurrences()),
            "editor.action.insertCursorAbove" => self.edit(|b| b.add_cursor_vertical(true)),
            "editor.action.insertCursorBelow" => self.edit(|b| b.add_cursor_vertical(false)),
            "expandLineSelection" => self.edit(|b| b.expand_line_selection()),
            "removeSecondaryCursors" => self.edit(|b| b.collapse_to_primary()),
            "editor.action.transformToUppercase" => self.transform_selection(|s| s.to_uppercase()),
            "editor.action.transformToLowercase" => self.transform_selection(|s| s.to_lowercase()),
            "editor.action.trimTrailingWhitespace" => self.edit(|b| {
                let t = b.text();
                let new: String = t.split('\n').map(|l| l.trim_end_matches([' ', '\t'])).collect::<Vec<_>>().join("\n");
                if new != t {
                    let n = b.len_chars();
                    let head = b.primary().head.min(new.chars().count());
                    b.apply(vec![fractal_core::buffer::Change { start: 0, end: n, text: new, sel: (head, head) }], EditKind::Other);
                }
            }),
            "editor.action.sortLinesAscending" => self.edit(|b| {
                let (l1, l2) = {
                    let s = b.primary();
                    (b.char_to_pos(s.start()).0, b.char_to_pos(s.end()).0)
                };
                let (l1, l2) = if l1 == l2 { (0, b.line_count().saturating_sub(1)) } else { (l1, l2) };
                let mut lines: Vec<String> = (l1..=l2).map(|l| b.line_string(l)).collect();
                lines.sort();
                let (a, e) = (b.line_start(l1), b.line_end(l2));
                let txt = lines.join("\n");
                b.apply(vec![fractal_core::buffer::Change::replace(a, e, txt)], EditKind::Other);
            }),
            "editor.action.joinLines" => self.edit(|b| {
                let l = b.char_to_pos(b.primary().head).0;
                if l + 1 < b.line_count() {
                    let e = b.line_end(l);
                    let next_start = b.line_start(l + 1);
                    let next_trim = b.line_string(l + 1).chars().take_while(|c| c.is_whitespace()).count();
                    b.apply(vec![fractal_core::buffer::Change::replace(e, next_start + next_trim, " ")], EditKind::Other);
                }
            }),
            "fractal.indentSpaces2" => self.set_indent(true, 2),
            "fractal.indentSpaces4" => self.set_indent(true, 4),
            "fractal.indentTabs" => self.set_indent(false, 4),
            "cursorLeft" => self.motion(Motion::Left, false),
            "cursorRight" => self.motion(Motion::Right, false),
            "cursorUp" => self.motion(Motion::Up, false),
            "cursorDown" => self.motion(Motion::Down, false),
            "cursorLeftSelect" => self.motion(Motion::Left, true),
            "cursorRightSelect" => self.motion(Motion::Right, true),
            "cursorUpSelect" => self.motion(Motion::Up, true),
            "cursorDownSelect" => self.motion(Motion::Down, true),
            "cursorWordLeft" => self.motion(Motion::WordLeft, false),
            "cursorWordRight" => self.motion(Motion::WordRight, false),
            "cursorWordLeftSelect" => self.motion(Motion::WordLeft, true),
            "cursorWordRightSelect" => self.motion(Motion::WordRight, true),
            "cursorHome" => self.motion(Motion::LineStart, false),
            "cursorEnd" => self.motion(Motion::LineEnd, false),
            "cursorHomeSelect" => self.motion(Motion::LineStart, true),
            "cursorEndSelect" => self.motion(Motion::LineEnd, true),
            "cursorTop" => self.motion(Motion::DocStart, false),
            "cursorBottom" => self.motion(Motion::DocEnd, false),
            "cursorTopSelect" => self.motion(Motion::DocStart, true),
            "cursorBottomSelect" => self.motion(Motion::DocEnd, true),
            "cursorPageUp" => self.motion(Motion::PageUp(0), false),
            "cursorPageDown" => self.motion(Motion::PageDown(0), false),
            "cursorPageUpSelect" => self.motion(Motion::PageUp(0), true),
            "cursorPageDownSelect" => self.motion(Motion::PageDown(0), true),
            other => {
                if commands::title_of(other).is_none() {
                    self.toast(format!("Unknown command: {other}"));
                }
            }
        }
    }

    fn run_current_prepare(&mut self) {
        let specs = run::detect(self.folder.as_deref(), self.active_path().as_deref());
        self.run_specs = specs;
        self.run_key = Some(self.run_key_now());
        self.run_job = None;
    }

    fn transform_selection(&mut self, f: impl Fn(&str) -> String) {
        self.edit(|b| {
            let mut sels = b.sels.clone();
            sels.sort_by_key(|s| s.start());
            let changes: Vec<_> = sels
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let t = f(&b.slice_string(s.start(), s.end()));
                    let n = t.chars().count();
                    fractal_core::buffer::Change { start: s.start(), end: s.end(), text: t, sel: (0, n) }
                })
                .collect();
            b.apply(changes, EditKind::Other);
        });
    }

    fn set_indent(&mut self, spaces: bool, size: usize) {
        self.edit(|b| {
            b.indent.use_spaces = spaces;
            b.indent.size = size;
        });
    }

    pub fn open_settings_page(&mut self) {
        if let Some(i) = self.tabs.iter().position(|t| t.settings_page) {
            self.active = i;
        } else {
            let mut tab = Tab::new(Buffer::new(), &self.theme);
            tab.settings_page = true;
            tab.title_override = Some("Settings".into());
            let at = (self.active + 1).min(self.tabs.len());
            self.tabs.insert(if self.tabs.is_empty() { 0 } else { at }, tab);
            self.active = if self.tabs.len() == 1 { 0 } else { at };
        }
        self.focus = Focus::Other;
    }

    /// Format the active document with the language's formatter (or the built-in one).
    pub fn format_active(&mut self) {
        let project = self.folder.clone();
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        if t.settings_page {
            return;
        }
        let text = t.buf.text();
        let path = t.buf.path.clone();
        match fractal_core::format::format_document(t.buf.lang.id, &text, path.as_deref(), project.as_deref(), t.buf.indent) {
            Ok(f) => {
                if f.text == text {
                    self.toast("Already formatted");
                } else {
                    t.buf.replace_all_text(&f.text);
                    t.reveal = true;
                    let msg = if f.used == "built-in" { "Formatted".to_string() } else { format!("Formatted with {}", f.used) };
                    self.toast(msg);
                }
            }
            Err(e) => self.toast(format!("Format failed: {e}")),
        }
    }

    pub fn show_side(&mut self, v: SideView) {
        self.sidebar_visible = true;
        self.side_view = v;
        self.focus = Focus::Other;
    }

    // ------------------------------------------------------------------
    // periodic work
    // ------------------------------------------------------------------

    pub fn tick(&mut self, ctx: &egui::Context) {
        self.lsp_tick(ctx);
        if self.settings_reload {
            self.settings_reload = false;
            let new = Settings::load();
            let theme_changed = new.theme != self.settings.theme;
            let font_changed = new.font_family != self.settings.font_family;
            self.settings = new;
            if theme_changed {
                let n = self.settings.theme.clone();
                self.set_theme(ctx, &n);
            }
            if font_changed {
                ui_theme::setup_fonts(ctx, &self.settings.font_family);
            }
            self.toast("Settings reloaded");
        }
        if self.keybindings_reload {
            self.keybindings_reload = false;
            let mut km = Keymap::new(self.mac);
            if let Ok(s) = std::fs::read_to_string(settings::keybindings_path()) {
                km.load_user(&s);
            }
            self.keymap = km;
            self.toast("Keybindings reloaded");
        }
        self.tick_run_specs(ctx);
        // scm
        for a in self.scm.tick(ctx) {
            match a {
                ScmAction::Toast(t) => self.toast(t),
                ScmAction::Refreshed => {
                    self.git_status_map.clear();
                    if let Some(st) = &self.scm.status {
                        for e in &st.entries {
                            let ch = if e.untracked() { '?' } else if e.worktree != ' ' { e.worktree } else { e.index };
                            self.git_status_map.insert(st.root.join(&e.path), ch);
                        }
                    }
                    self.git_marks_for.clear();
                }
                _ => {}
            }
        }
        // git gutter marks for active tab
        if let Some((idx, job)) = &self.git_marks_job {
            if let Some(marks) = job.poll() {
                let idx = *idx;
                if let Some(t) = self.tabs.get_mut(idx) {
                    t.marks = marks;
                }
                self.git_marks_job = None;
            }
        } else if let (Some(repo), Some(t)) = (self.scm.repo.clone(), self.tabs.get(self.active)) {
            let v = t.buf.saved_version;
            if self.git_marks_for.get(&self.active) != Some(&v) {
                if let Some(path) = t.buf.path.clone() {
                    let untracked = self.git_status_map.get(&path) == Some(&'?');
                    self.git_marks_for.insert(self.active, v);
                    let idx = self.active;
                    self.git_marks_job = Some((idx, Job::spawn(ctx, move || git::line_marks(&repo, &path, untracked))));
                }
            }
        }
        // file index done
        if let Some(j) = &self.index_job {
            if j.poll().is_some() {
                self.index_job = None;
            }
        }
        // toasts
        self.toasts.retain(|t| t.born.elapsed() < Duration::from_secs(5));
        // 1-second housekeeping: external changes + autosave
        if self.last_poll.elapsed() > Duration::from_millis(1000) {
            self.last_poll = Instant::now();
            let mut msgs = vec![];
            for t in &mut self.tabs {
                if let Some(p) = t.buf.path.clone() {
                    if let Ok(mt) = std::fs::metadata(&p).and_then(|m| m.modified()) {
                        if t.buf.disk_mtime != Some(mt) {
                            if !t.buf.is_dirty() {
                                if let Ok(bytes) = std::fs::read(&p) {
                                    t.buf.reload(&String::from_utf8_lossy(&bytes));
                                    t.buf.disk_mtime = Some(mt);
                                }
                            } else if t.buf.disk_mtime.is_some() {
                                msgs.push(format!("{} changed on disk. Save to overwrite or close to reload.", t.title()));
                                t.buf.disk_mtime = Some(mt);
                            }
                        }
                    }
                }
            }
            for m in msgs {
                self.toast(m);
            }
            if self.settings.auto_save {
                for i in 0..self.tabs.len() {
                    let t = &self.tabs[i];
                    if t.buf.is_dirty() && t.buf.path.is_some() && t.last_edit.elapsed() > Duration::from_millis(900) {
                        self.save_tab(i);
                    }
                }
            }
            if self.needs_session_save {
                self.save_session();
            }
        }
    }

    pub fn save_session(&mut self) {
        self.needs_session_save = false;
        self.session.folder = self.folder.clone();
        self.session.open_files = self.tabs.iter().filter_map(|t| t.buf.path.clone()).collect();
        self.session.active = self.active;
        self.session.sidebar_visible = Some(self.sidebar_visible);
        self.session.save();
    }

    pub fn handle_explorer_actions(&mut self, ctx: &egui::Context, actions: Vec<ExplorerAction>) {
        for a in actions {
            match a {
                ExplorerAction::Open(p) | ExplorerAction::FileCreated(p) => self.open_file(&p, None),
                ExplorerAction::Deleted(p) => {
                    let idxs: Vec<usize> = self.tabs.iter().enumerate().filter(|(_, t)| t.buf.path.as_ref().map_or(false, |x| x.starts_with(&p))).map(|(i, _)| i).collect();
                    for i in idxs.into_iter().rev() {
                        self.close_tab(i, false);
                    }
                    self.scm.want_refresh = true;
                }
                ExplorerAction::Renamed(from, to) => {
                    for t in &mut self.tabs {
                        if t.buf.path.as_deref() == Some(from.as_path()) {
                            t.buf.path = Some(to.clone());
                            t.buf.lang = fractal_core::lang::LangInfo::for_path(Some(&to));
                            t.hl.syntax_changed(&t.buf);
                        }
                    }
                    self.scm.want_refresh = true;
                }
                ExplorerAction::Reveal(p) => reveal(&p),
                ExplorerAction::CopyPath(p) => ctx.copy_text(p.to_string_lossy().to_string()),
                ExplorerAction::OpenInTerminal(p) => {
                    self.new_terminal(ctx, Some(&p));
                }
                ExplorerAction::Toast(t) => self.toast(t),
            }
        }
    }

    pub fn handle_search_action(&mut self, a: SearchAction) {
        match a {
            SearchAction::Open { path, line, c0, c1 } => self.open_file(&path, Some((line, c0, c1))),
            SearchAction::ReplaceAll => {
                if let Some(root) = self.folder.clone() {
                    let n = self.search.replace_all(&root);
                    self.toast(format!("Replaced matches in {n} files"));
                    let q = self.search.query.clone();
                    let _ = q;
                }
            }
            SearchAction::None => {}
        }
    }

    pub fn handle_scm_action(&mut self, a: ScmAction) {
        match a {
            ScmAction::Open(p) => self.open_file(&p, None),
            ScmAction::Diff { path, staged } => {
                if let Some(r) = self.scm.repo.clone() {
                    let text = git::diff_text(&r, &path, staged);
                    let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.open_virtual(&format!("{name} (Diff)"), "diff", &text);
                }
            }
            ScmAction::Toast(t) => self.toast(t),
            ScmAction::BranchPicker => self.open_overlay(Kind::Branch, ""),
            ScmAction::Refreshed => {}
        }
    }
}

fn file_item(p: &Path, root: Option<&Path>, recent: bool) -> Item {
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let rel = root.and_then(|r| p.strip_prefix(r).ok()).unwrap_or(p);
    let dir = rel.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    Item { label: name, detail: dir, right: if recent { "recently opened".into() } else { String::new() }, payload: Payload::File(p.to_path_buf()) }
}

pub fn reveal(p: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = open::that(p.parent().unwrap_or(p));
    }
}

pub fn key_name(k: Key) -> Option<&'static str> {
    use Key::*;
    Some(match k {
        ArrowDown => "down",
        ArrowLeft => "left",
        ArrowRight => "right",
        ArrowUp => "up",
        Escape => "escape",
        Tab => "tab",
        Backspace => "backspace",
        Enter => "enter",
        Space => "space",
        Delete => "delete",
        Home => "home",
        End => "end",
        PageUp => "pageup",
        PageDown => "pagedown",
        Comma => ",",
        Backslash => "\\",
        Slash => "/",
        OpenBracket | OpenCurlyBracket => "[",
        CloseBracket | CloseCurlyBracket => "]",
        Backtick => "`",
        Minus => "-",
        Period => ".",
        Plus | Equals => "=",
        Semicolon => ";",
        Quote => "'",
        Num0 => "0",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Num4 => "4",
        Num5 => "5",
        Num6 => "6",
        Num7 => "7",
        Num8 => "8",
        Num9 => "9",
        A => "a",
        B => "b",
        C => "c",
        D => "d",
        E => "e",
        F => "f",
        G => "g",
        H => "h",
        I => "i",
        J => "j",
        K => "k",
        L => "l",
        M => "m",
        N => "n",
        O => "o",
        P => "p",
        Q => "q",
        R => "r",
        S => "s",
        T => "t",
        U => "u",
        V => "v",
        W => "w",
        X => "x",
        Y => "y",
        Z => "z",
        F1 => "f1",
        F2 => "f2",
        F3 => "f3",
        F4 => "f4",
        F5 => "f5",
        F6 => "f6",
        F7 => "f7",
        F8 => "f8",
        F9 => "f9",
        F10 => "f10",
        F11 => "f11",
        F12 => "f12",
        _ => return None,
    })
}
