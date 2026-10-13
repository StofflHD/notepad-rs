//! Main window state: documents, the two views, session handling and the frame loop.
//! Menus, dialogs, panels and command handling live in the `ui_*` / `commands` modules.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Event, FontId, Key, Modifiers, ViewportCommand};

use crate::cmd::{self, Cmd};
use crate::doc::{Document, ScrollTo, TextEncoding};
use crate::editor::{self, Completion, Palette};
use crate::highlight::{Highlighting, Symbol};
use crate::i18n::{tr, trf};
use crate::project::Workspace;
use crate::search::Hit;
use crate::settings::{BackupMode, Session, SessionDoc, Settings};
use crate::udl::UserLang;
use crate::{fonts, ipc};

pub const APP_NAME: &str = "NotepadRS";

/// Options from the command line (Notepad++ compatible where possible).
#[derive(Clone, Debug, Default)]
pub struct CliOptions {
    pub files: Vec<PathBuf>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub lang: Option<String>,
    pub udl: Option<String>,
    pub no_session: bool,
    pub read_only: bool,
    pub always_on_top: bool,
    pub system_tray: bool,
    pub no_tab_bar: bool,
    pub open_session: Option<PathBuf>,
    pub monitor: bool,
}

pub enum DialogResult {
    Open(Vec<PathBuf>),
    SaveAs { id: u64, path: Option<PathBuf>, copy: bool },
    /// Generic path picked for a purpose.
    Picked(Pick, Vec<PathBuf>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    WorkspaceFolder,
    LoadSession,
    SaveSession,
    HashFiles(cmd::Hash),
    FifDir,
    ImportTheme,
    ImportSyntax,
    UdlImport,
    UdlExport(usize),
    ProjectOpen(usize),
    ProjectSaveAs(usize),
    ProjectAddFiles(usize, Vec<usize>),
    ProjectAddDir(usize, Vec<usize>),
    PrintToFile,
    ExportHtml,
    ExportRtf,
    BackupDir,
    DefaultDir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    SaveBeforeClose(u64),
    ReloadChanged(u64),
    KeepMissing(u64),
    ReloadDiscard(u64),
    TrashFile(u64),
    ReplaceAllDocs,
    ReplaceInFiles,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FindTab {
    #[default]
    Find,
    Replace,
    FindInFiles,
    Mark,
}

#[derive(Default)]
pub struct FindState {
    pub open: bool,
    pub tab: FindTab,
    pub what: String,
    pub with: String,
    pub in_selection: bool,
    pub backward: bool,
    pub bookmark_lines: bool,
    pub purge: bool,
    pub status: Option<(String, bool)>,
    pub focus: bool,
}

/// One block in the search results panel.
pub struct ResultBlock {
    pub title: String,
    pub files: Vec<ResultFile>,
    pub collapsed: bool,
}

pub struct ResultFile {
    pub path: Option<PathBuf>,
    pub doc_id: Option<u64>,
    pub hits: Vec<Hit>,
    pub collapsed: bool,
}

pub struct FifJob {
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<crate::findfiles::Msg>,
    pub scanned: usize,
}

#[derive(Default)]
pub struct Macro {
    pub recording: Option<Vec<Event>>,
    pub last: Vec<Event>,
    pub queue: VecDeque<Vec<Event>>,
    /// "Run until the end of file": the macro chunks and the caret position of the last run.
    pub until_end: Option<(Vec<Vec<Event>>, usize)>,
}

pub struct Pane {
    pub tabs: Vec<u64>,
    pub active: usize,
    pub last_first_line: usize,
    pub last_x: f32,
}

impl Pane {
    fn new() -> Self {
        Self { tabs: Vec::new(), active: 0, last_first_line: 0, last_x: 0.0 }
    }
    pub fn active_id(&self) -> Option<u64> {
        self.tabs.get(self.active).copied()
    }
}

/// (doc id, version, cancel flag, result channel)
pub type PendingSymbols = (u64, u64, Arc<AtomicBool>, Receiver<Vec<Symbol>>);

/// Function list computed on a background thread.
#[derive(Default)]
pub struct FuncList {
    pub doc_id: u64,
    pub version: u64,
    pub symbols: Vec<Symbol>,
    pub filter: String,
    pub sorted: bool,
    pub pending: Option<PendingSymbols>,
    pub requested_at: Option<Instant>,
}

pub struct App {
    pub hl: Highlighting,
    pub udls: Vec<UserLang>,
    pub syntax_groups: BTreeMap<char, Vec<String>>,
    pub mono_fonts: Option<Vec<(String, String)>>,
    pub s: Settings,
    pub docs: Vec<Document>,
    pub panes: [Pane; 2],
    /// Active view (0 = main, 1 = second)
    pub cur: usize,
    pub split_ratio: f32,
    pub next_id: u64,
    pub overwrite: bool,
    pub editor_focused: bool,
    pub focus_editor: bool,
    pub completion: Option<Completion>,
    pub calltip: Option<crate::api::CallTip>,

    pub find: FindState,
    pub fif: Option<FifJob>,
    pub results: Vec<ResultBlock>,
    pub show_results: bool,
    /// Current result for F4 navigation: (block, file, hit)
    pub result_pos: Option<(usize, usize, usize)>,
    pub inc_search: Option<crate::ui_dialogs::IncSearch>,
    pub dlg: crate::ui_dialogs::Dialogs,
    pub prefs: crate::ui_prefs::PrefsState,
    pub confirms: VecDeque<Confirm>,
    pub message: Option<(String, String)>,
    pub macro_: Macro,
    pub begin_select: Option<(u64, usize, bool)>,

    pub close_queue: VecDeque<(u64, usize)>,
    pub exit_after: bool,
    /// Updater: running check/download and its dialog.
    pub update_rx: Option<Receiver<crate::update::Msg>>,
    pub update: Option<crate::ui_dialogs::UpdateDlg>,
    /// Program to start after the window has closed (restart after an update).
    pub restart_exe: Option<PathBuf>,
    pub icons: crate::icons::Icons,
    /// System tray: icon while the window is hidden, its events, state.
    pub tray: Option<crate::tray::Handle>,
    pub tray_rx: Option<Receiver<crate::tray::Event>>,
    pub in_tray: bool,
    /// Restored from the tray; the viewport may still report "minimized" for a few frames.
    pub tray_restoring: bool,
    /// Start hidden in the tray (-systemtray), applied in the first frame.
    pub start_in_tray: bool,
    /// File → Exit / tray Exit: close even with "close to tray".
    pub explicit_exit: bool,
    pub allow_close: bool,
    pub close_after_save: Option<u64>,
    pub closed_paths: Vec<String>,

    pub dlg_tx: Sender<DialogResult>,
    pub dlg_rx: Receiver<DialogResult>,
    pub dialog_busy: bool,
    pub ipc_rx: Option<Receiver<Vec<PathBuf>>>,
    pub owns_socket: bool,

    pub title: String,
    pub last_disk_check: Instant,
    pub last_persist: Instant,
    pub last_monitor: Instant,
    pub persisted_session: u64,
    pub persisted_settings: String,
    pub fullscreen: bool,
    pub always_on_top: bool,
    pub postit: bool,
    pub distraction_free: bool,
    pub sync_v: bool,
    pub sync_h: bool,
    pub clipboard_history: Vec<String>,
    pub workspace_open: std::collections::HashSet<PathBuf>,
    pub projects: [Option<Workspace>; 3],
    pub project_open: std::collections::HashSet<(usize, Vec<usize>)>,
    pub func: FuncList,
    pub keywords_cache: (String, std::rc::Rc<Vec<String>>),
    pub no_session: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: CliOptions) -> Self {
        Self::init(&cc.egui_ctx, cli)
    }

    pub(crate) fn init(ctx: &egui::Context, cli: CliOptions) -> Self {
        let s = Settings::load();
        crate::i18n::set_language(&s.ui_lang);
        fonts::apply(ctx, s.font_path.as_deref());
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        crate::theme::apply(ctx, s.dark_mode);
        ctx.set_zoom_factor(s.ui_scale.clamp(0.5, 3.0));

        let hl = Highlighting::new();
        let mut syntax_groups: BTreeMap<char, Vec<String>> = BTreeMap::new();
        for name in hl.syntax_names() {
            let k = name.chars().next().map_or('#', |c| c.to_ascii_uppercase());
            let k = if k.is_ascii_alphabetic() { k } else { '#' };
            syntax_groups.entry(k).or_default().push(name);
        }
        let (dlg_tx, dlg_rx) = channel();
        let projects = [0, 1, 2].map(|i| {
            let p = &s.project_files[i];
            (!p.is_empty()).then(|| Workspace::load(Path::new(p)).ok()).flatten()
        });
        let mut app = Self {
            hl,
            udls: crate::udl::load_all(),
            syntax_groups,
            mono_fonts: None,
            s,
            docs: Vec::new(),
            panes: [Pane::new(), Pane::new()],
            cur: 0,
            split_ratio: 0.5,
            next_id: 1,
            overwrite: false,
            editor_focused: false,
            focus_editor: true,
            completion: None,
            calltip: None,
            find: FindState::default(),
            fif: None,
            results: Vec::new(),
            show_results: false,
            result_pos: None,
            inc_search: None,
            dlg: Default::default(),
            prefs: Default::default(),
            confirms: VecDeque::new(),
            message: None,
            macro_: Macro::default(),
            begin_select: None,
            close_queue: VecDeque::new(),
            exit_after: false,
            update_rx: None,
            update: None,
            restart_exe: None,
            icons: Default::default(),
            tray: None,
            tray_rx: None,
            in_tray: false,
            tray_restoring: false,
            start_in_tray: false,
            explicit_exit: false,
            allow_close: false,
            close_after_save: None,
            closed_paths: Vec::new(),
            dlg_tx,
            dlg_rx,
            dialog_busy: false,
            ipc_rx: None,
            owns_socket: false,
            title: String::new(),
            last_disk_check: Instant::now(),
            last_persist: Instant::now(),
            last_monitor: Instant::now(),
            persisted_session: 0,
            persisted_settings: String::new(),
            fullscreen: false,
            always_on_top: false,
            postit: false,
            distraction_free: false,
            sync_v: false,
            sync_h: false,
            clipboard_history: Vec::new(),
            workspace_open: Default::default(),
            projects,
            project_open: Default::default(),
            func: FuncList::default(),
            keywords_cache: (String::new(), Default::default()),
            no_session: cli.no_session,
        };
        app.persisted_settings = serde_json::to_string(&app.s).unwrap_or_default();

        if let Some(sess) = &cli.open_session {
            app.load_session_file(sess);
        } else if app.s.remember_session && !cli.no_session {
            app.restore_session(&Session::load());
        }
        let mut opened = Vec::new();
        for f in &cli.files {
            if let Some(id) = app.open_path(f) {
                opened.push(id);
            }
        }
        for id in &opened {
            let i = app.idx(*id);
            let d = &mut app.docs[i];
            if cli.read_only {
                d.read_only = true;
            }
            if let Some(l) = &cli.lang {
                d.syntax = app.hl.syntax_names().into_iter().find(|n| n.eq_ignore_ascii_case(l));
            }
            if let Some(u) = &cli.udl {
                d.user_lang = Some(u.clone());
            }
            if cli.monitor {
                d.monitoring = true;
                d.read_only = true;
            }
        }
        if let (Some(&id), Some(line)) = (opened.last(), cli.line) {
            let i = app.idx(id);
            let d = &mut app.docs[i];
            let l = line.saturating_sub(1).min(d.line_count() - 1);
            let p = d.pos_at_col(l, cli.column.unwrap_or(1).saturating_sub(1));
            d.select(p..p);
        }
        if app.docs.is_empty() {
            app.new_doc();
        }
        app.start_in_tray = cli.system_tray;
        if cli.always_on_top {
            app.always_on_top = true;
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        }
        if cli.no_tab_bar {
            app.s.show_tab_bar = false;
        }
        // Debug aid for screenshots: NRS_OPEN=find,replace,fif,prefs,style,shortcuts,udl,split
        if let Ok(list) = std::env::var("NRS_OPEN") {
            for w in list.split(',') {
                match w {
                    "find" => app.open_find(FindTab::Find),
                    "replace" => app.open_find(FindTab::Replace),
                    "fif" => app.open_find(FindTab::FindInFiles),
                    "prefs" => app.prefs.open = true,
                    "assoc" => app.prefs.open_at(crate::ui_prefs::PrefPage::FileAssociation),
                    "style" => app.prefs.style_open = true,
                    "shortcuts" => app.prefs.shortcuts_open = true,
                    "udl" => app.prefs.udl_open = true,
                    "results" => app.show_results = true,
                    "split" => app.run(ctx, Cmd::CloneToOtherView),
                    "column" => app.dlg.column_editor = Some(Default::default()),
                    "about" => app.dlg.about = true,
                    "update" => app.start_update_check(ctx, true),
                    _ => {}
                }
            }
        }
        if app.s.single_instance {
            let (tx, rx) = channel();
            let ctx2 = ctx.clone();
            app.owns_socket = ipc::listen(tx, move || {
                ctx2.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx2.send_viewport_cmd(ViewportCommand::Focus);
                ctx2.request_repaint();
            });
            app.ipc_rx = Some(rx);
        }
        // Automatic update check (at most once a day, in the background).
        let now = chrono::Utc::now().timestamp();
        if !cfg!(test) && app.s.update_auto_check && std::env::var_os("NRS_NO_UPDATE").is_none() && now - app.s.update_last_check >= 86_400 {
            app.s.update_last_check = now;
            app.start_update_check(ctx, false);
        }
        app
    }

    pub fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.update_rx.is_some() {
            if manual {
                self.update.get_or_insert(crate::ui_dialogs::UpdateDlg { state: crate::ui_dialogs::UpdateState::Checking, manual });
            }
            return;
        }
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        crate::update::spawn_check(self.s.update_proxy.clone(), tx, move || ctx2.request_repaint());
        self.update_rx = Some(rx);
        if manual {
            self.update = Some(crate::ui_dialogs::UpdateDlg { state: crate::ui_dialogs::UpdateState::Checking, manual });
        }
    }

    pub fn start_update_install(&mut self, ctx: &egui::Context, rel: crate::update::Release) {
        let Some(target) = crate::update::running_appimage() else { return };
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        crate::update::spawn_install(rel, target, self.s.update_proxy.clone(), tx, move || ctx2.request_repaint());
        self.update_rx = Some(rx);
        self.update = Some(crate::ui_dialogs::UpdateDlg { state: crate::ui_dialogs::UpdateState::Installing, manual: true });
    }

    /// Hides the window and shows the tray icon. Returns false when there is no tray.
    pub fn hide_to_tray(&mut self, ctx: &egui::Context) -> bool {
        if self.tray.is_none() {
            let (tx, rx) = channel();
            let tip = self.doc().title();
            match crate::tray::show(ctx, tx, tip) {
                Ok(h) => {
                    self.tray = Some(h);
                    self.tray_rx = Some(rx);
                }
                Err(e) => {
                    self.message = Some((tr("System tray").into(), trf("No system tray available: {}", &[&e])));
                    return false;
                }
            }
        }
        self.persist(false);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        self.in_tray = true;
        true
    }

    pub fn show_from_tray(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.in_tray = false;
        self.tray_restoring = true;
        self.tray_rx = None;
        if let Some(h) = self.tray.take() {
            std::thread::spawn(move || h.remove());
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        use crate::settings::TrayMode;
        if std::mem::take(&mut self.start_in_tray) {
            self.hide_to_tray(ctx);
            return;
        }
        let events: Vec<crate::tray::Event> = self.tray_rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for ev in events {
            self.show_from_tray(ctx);
            match ev {
                crate::tray::Event::Show => {}
                crate::tray::Event::New => self.run(ctx, Cmd::New),
                crate::tray::Event::Open => self.run(ctx, Cmd::Open),
                crate::tray::Event::Exit => self.run(ctx, Cmd::Exit),
            }
        }
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if !minimized {
            self.tray_restoring = false;
        }
        if minimized && !self.in_tray && !self.tray_restoring && matches!(self.s.tray, TrayMode::Minimize | TrayMode::MinimizeAndClose) {
            self.hide_to_tray(ctx);
        }
    }

    fn poll_update(&mut self) {
        use crate::ui_dialogs::{UpdateDlg, UpdateState};
        let Some(rx) = &self.update_rx else { return };
        let Ok(msg) = rx.try_recv() else { return };
        self.update_rx = None;
        let manual = self.update.as_ref().is_some_and(|u| u.manual);
        let state = match msg {
            crate::update::Msg::Checked(Ok(Some(rel))) if manual || rel.version != self.s.update_skip => Some(UpdateState::Available(rel)),
            crate::update::Msg::Checked(Ok(_)) => manual.then_some(UpdateState::UpToDate),
            crate::update::Msg::Checked(Err(e)) => manual.then_some(UpdateState::Error(e)),
            crate::update::Msg::Installed(Ok(p)) => Some(UpdateState::Installed(p)),
            crate::update::Msg::Installed(Err(e)) => Some(UpdateState::Error(e)),
        };
        self.update = state.map(|state| UpdateDlg { state, manual });
    }

    // ---- documents & views ---------------------------------------------------------------------

    pub fn idx(&self, id: u64) -> usize {
        self.docs.iter().position(|d| d.id == id).expect("document id")
    }

    pub fn try_idx(&self, id: u64) -> Option<usize> {
        self.docs.iter().position(|d| d.id == id)
    }

    pub fn doc_by_id(&self, id: u64) -> Option<&Document> {
        self.docs.iter().find(|d| d.id == id)
    }

    pub fn doc_by_id_mut(&mut self, id: u64) -> Option<&mut Document> {
        self.docs.iter_mut().find(|d| d.id == id)
    }

    pub fn active_id(&self) -> u64 {
        self.panes[self.cur].active_id().or_else(|| self.panes[0].active_id()).unwrap_or(self.docs[0].id)
    }

    pub fn doc(&self) -> &Document {
        let id = self.active_id();
        &self.docs[self.idx(id)]
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        let id = self.active_id();
        let i = self.idx(id);
        &mut self.docs[i]
    }

    /// The second view shows a clone: its state lives in `Document::alt`.
    pub fn is_clone_view(&self, pane: usize, id: u64) -> bool {
        pane == 1 && self.panes[0].tabs.contains(&id)
    }

    /// Runs `f` with the document's selection/scroll state of `pane` in place.
    pub fn with_view<R>(&mut self, pane: usize, id: u64, f: impl FnOnce(&mut Self) -> R) -> R {
        let swap = self.is_clone_view(pane, id);
        if swap {
            if let Some(d) = self.doc_by_id_mut(id) {
                d.swap_alt();
            }
        }
        let r = f(self);
        if swap && self.is_clone_view(pane, id) {
            if let Some(d) = self.doc_by_id_mut(id) {
                d.swap_alt();
            }
        }
        r
    }

    pub fn alloc_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn next_untitled_no(&self) -> usize {
        (1..).find(|n| !self.docs.iter().any(|d| d.path.is_none() && d.untitled_no == *n)).unwrap()
    }

    pub fn new_doc(&mut self) -> u64 {
        let id = self.alloc_id();
        let mut doc = Document::new_untitled(id, self.next_untitled_no(), self.s.default_eol);
        doc.encoding = self.s.default_encoding.clone();
        doc.syntax = self.s.default_lang.clone();
        self.docs.push(doc);
        self.add_tab(self.cur, id);
        id
    }

    pub fn add_tab(&mut self, pane: usize, id: u64) {
        let p = &mut self.panes[pane];
        let at = (p.active + 1).min(p.tabs.len());
        p.tabs.insert(at, id);
        p.active = at;
        self.cur = pane;
        self.focus_editor = true;
    }

    pub fn activate_id(&mut self, id: u64) {
        for (pi, p) in self.panes.iter_mut().enumerate() {
            if let Some(i) = p.tabs.iter().position(|&t| t == id) {
                p.active = i;
                self.cur = pi;
                self.focus_editor = true;
                return;
            }
        }
    }

    pub fn activate_tab(&mut self, pane: usize, i: usize) {
        if i < self.panes[pane].tabs.len() {
            self.panes[pane].active = i;
            self.cur = pane;
            self.focus_editor = true;
        }
    }

    /// A single untouched "new 1" is replaced when a file is opened, like in Notepad++.
    fn is_pristine(&self, id: u64) -> bool {
        let Some(d) = self.doc_by_id(id) else { return false };
        d.path.is_none() && d.text().is_empty() && !d.is_dirty() && !d.can_undo()
    }

    pub fn find_udl_for(&self, path: &Path) -> Option<String> {
        self.udls.iter().find(|u| u.matches_path(path)).map(|u| u.name.clone())
    }

    /// Opens a file (or activates it). Returns the document id.
    pub fn open_path(&mut self, path: &Path) -> Option<u64> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if path.is_dir() {
            // Opening a folder: show it in "Folder as Workspace".
            let p = path.display().to_string();
            if !self.s.workspace_roots.contains(&p) {
                self.s.workspace_roots.push(p);
            }
            self.s.show_workspace = true;
            return None;
        }
        if let Some(d) = self.docs.iter().find(|d| d.path.as_deref() == Some(path.as_path())) {
            let id = d.id;
            self.activate_id(id);
            return Some(id);
        }
        let id = self.alloc_id();
        let doc = if !path.exists() {
            // Like Notepad++: a missing file passed on the command line becomes a new document.
            let mut doc = Document::new_untitled(id, 0, self.s.default_eol);
            doc.path = Some(path.clone());
            doc.disk_missing = true;
            doc.syntax = self.hl.detect(Some(&path), "");
            doc
        } else {
            match Document::open(id, &path, None, self.s.default_eol) {
                Ok(mut doc) => {
                    doc.user_lang = self.find_udl_for(&path);
                    if doc.user_lang.is_none() {
                        doc.syntax = self.hl.detect(Some(&path), &doc.text()[..doc.text().len().min(4096)]);
                    }
                    self.s.add_recent(&path.display().to_string());
                    doc
                }
                Err(e) => {
                    self.message = Some((tr("Open failed").into(), e));
                    return None;
                }
            }
        };
        if let Some(dir) = path.parent() {
            self.s.last_dir = dir.display().to_string();
        }
        self.push_doc(doc);
        Some(id)
    }

    pub fn push_doc(&mut self, doc: Document) {
        let id = doc.id;
        let pane = self.cur;
        let replace = self.panes[pane].tabs.len() == 1 && self.panes[pane].active_id().is_some_and(|t| self.is_pristine(t)) && self.docs.len() == 1;
        if replace {
            let old = self.panes[pane].tabs[0];
            self.docs.retain(|d| d.id != old);
            self.panes[pane].tabs.clear();
            self.panes[pane].active = 0;
        }
        self.docs.push(doc);
        self.add_tab(pane, id);
    }

    /// Removes a tab from a view; the document is dropped when no view shows it any more.
    pub fn close_tab_now(&mut self, id: u64, pane: usize) {
        let in_other = self.panes[1 - pane].tabs.contains(&id);
        if pane == 0 && in_other {
            // The second view keeps the document: its state becomes the main state.
            if let Some(d) = self.doc_by_id_mut(id) {
                d.swap_alt();
                d.alt = None;
            }
        } else if pane == 1 && in_other {
            if let Some(d) = self.doc_by_id_mut(id) {
                d.alt = None;
            }
        }
        let p = &mut self.panes[pane];
        if let Some(i) = p.tabs.iter().position(|&t| t == id) {
            p.tabs.remove(i);
            if p.active > i || p.active >= p.tabs.len() {
                p.active = p.active.saturating_sub(1).min(p.tabs.len().saturating_sub(1));
            }
        }
        if !in_other {
            if let Some(i) = self.try_idx(id) {
                let d = self.docs.remove(i);
                if let Some(p) = d.path_string() {
                    self.closed_paths.push(p);
                }
            }
        }
        self.fix_panes();
        self.focus_editor = true;
    }

    /// Keeps the views consistent: the main view is never empty while the second has tabs.
    pub fn fix_panes(&mut self) {
        if self.panes[0].tabs.is_empty() && !self.panes[1].tabs.is_empty() {
            self.panes.swap(0, 1);
            for d in &mut self.docs {
                d.alt = None;
            }
        }
        if self.panes[0].tabs.is_empty() {
            self.cur = 0;
            self.new_doc();
        }
        if self.panes[1].tabs.is_empty() {
            self.cur = 0;
        }
    }

    pub fn request_close(&mut self, ids: Vec<u64>) {
        let pane = self.cur;
        self.close_queue.extend(ids.into_iter().map(|id| (id, pane)));
    }

    pub fn process_close_queue(&mut self, ctx: &egui::Context) {
        while self.confirms.is_empty() && !self.dialog_busy {
            let Some(&(id, pane)) = self.close_queue.front() else {
                if self.exit_after {
                    self.exit_after = false;
                    self.allow_close = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                return;
            };
            let Some(i) = self.try_idx(id) else {
                self.close_queue.pop_front();
                continue;
            };
            let pane = if self.panes[pane].tabs.contains(&id) { pane } else { 1 - pane };
            let last_view = !self.panes[1 - pane].tabs.contains(&id);
            if self.docs[i].is_dirty() && last_view && !(self.docs[i].path.is_none() && self.docs[i].text().is_empty()) {
                self.activate_id(id);
                self.confirms.push_back(Confirm::SaveBeforeClose(id));
                return;
            }
            self.close_queue.pop_front();
            self.close_tab_now(id, pane);
        }
    }

    pub fn cancel_close(&mut self) {
        self.close_queue.clear();
        self.exit_after = false;
    }

    /// Saves a document. Returns false when a "save as" dialog was started instead.
    pub fn save(&mut self, id: u64) -> bool {
        let i = self.idx(id);
        let Some(path) = self.docs[i].path.clone() else {
            self.save_as_dialog(id, false);
            return false;
        };
        self.backup_before_save(&path);
        match self.docs[i].save_as(&path) {
            Ok(warn) => {
                if let Some(w) = warn {
                    self.message = Some((tr("Save").into(), w));
                }
                true
            }
            Err(e) => {
                self.message = Some((tr("Save failed").into(), e));
                false
            }
        }
    }

    fn backup_before_save(&self, path: &Path) {
        if !path.exists() {
            return;
        }
        match self.s.backup_on_save {
            BackupMode::None => {}
            BackupMode::Simple => {
                let mut b = path.as_os_str().to_owned();
                b.push(".bak");
                let _ = std::fs::copy(path, PathBuf::from(b));
            }
            BackupMode::Verbose => {
                let dir = if self.s.backup_dir.is_empty() {
                    crate::settings::config_dir().join("backup")
                } else {
                    PathBuf::from(&self.s.backup_dir)
                };
                let _ = std::fs::create_dir_all(&dir);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
                let _ = std::fs::copy(path, dir.join(format!("{name}.{stamp}.bak")));
            }
        }
    }

    pub fn start_dir(&self) -> Option<PathBuf> {
        use crate::settings::OpenDir;
        match self.s.open_dir {
            OpenDir::Custom if !self.s.custom_dir.is_empty() => return Some(PathBuf::from(&self.s.custom_dir)),
            OpenDir::RememberLast if !self.s.last_dir.is_empty() => return Some(PathBuf::from(&self.s.last_dir)),
            _ => {}
        }
        self.doc()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| self.s.recent.first().and_then(|p| Path::new(p).parent().map(Path::to_path_buf)))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    }

    pub fn open_dialog(&mut self, ctx: &egui::Context) {
        if self.dialog_busy {
            return;
        }
        self.dialog_busy = true;
        let tx = self.dlg_tx.clone();
        let dir = self.start_dir();
        let ctx = ctx.clone();
        let title = tr("Open").to_string();
        std::thread::spawn(move || {
            let mut d = rfd::FileDialog::new().set_title(title);
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            let files = d.pick_files().unwrap_or_default();
            let _ = tx.send(DialogResult::Open(files));
            ctx.request_repaint();
        });
    }

    /// Generic file/folder dialog; the result arrives as `DialogResult::Picked`.
    pub fn pick(&mut self, ctx: &egui::Context, what: Pick, folder: bool, save: bool, name: Option<String>, filter: Option<(&str, Vec<&str>)>) {
        if self.dialog_busy {
            return;
        }
        self.dialog_busy = true;
        let tx = self.dlg_tx.clone();
        let dir = self.start_dir();
        let ctx = ctx.clone();
        let filter = filter.map(|(n, e)| (n.to_string(), e.into_iter().map(str::to_string).collect::<Vec<_>>()));
        std::thread::spawn(move || {
            let mut d = rfd::FileDialog::new();
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            if let Some(n) = name {
                d = d.set_file_name(n);
            }
            if let Some((n, exts)) = &filter {
                d = d.add_filter(n, exts);
            }
            let paths = if folder {
                d.pick_folder().into_iter().collect()
            } else if save {
                d.save_file().into_iter().collect()
            } else {
                d.pick_files().unwrap_or_default()
            };
            let _ = tx.send(DialogResult::Picked(what, paths));
            ctx.request_repaint();
        });
    }

    pub fn save_as_dialog(&mut self, id: u64, copy: bool) {
        if self.dialog_busy {
            return;
        }
        self.dialog_busy = true;
        let tx = self.dlg_tx.clone();
        let Some(doc) = self.doc_by_id(id) else { return };
        let name = doc.title();
        let dir = doc.path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf)).or_else(|| self.start_dir());
        let title = tr(if copy { "Save a Copy As" } else { "Save As" }).to_string();
        std::thread::spawn(move || {
            let mut d = rfd::FileDialog::new().set_title(title).set_file_name(name);
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            let path = d.save_file();
            let _ = tx.send(DialogResult::SaveAs { id, path, copy });
        });
    }

    fn handle_dialog_results(&mut self, ctx: &egui::Context) {
        while let Ok(r) = self.dlg_rx.try_recv() {
            self.dialog_busy = false;
            match r {
                DialogResult::Open(files) => {
                    for f in files {
                        self.open_path(&f);
                    }
                }
                DialogResult::SaveAs { id, path, copy } => {
                    let Some(i) = self.try_idx(id) else { continue };
                    let Some(path) = path else {
                        if self.close_after_save == Some(id) {
                            self.close_after_save = None;
                            self.cancel_close();
                        }
                        continue;
                    };
                    self.backup_before_save(&path);
                    let res = if copy { self.docs[i].write_to(&path) } else { self.docs[i].save_as(&path) };
                    match res {
                        Ok(warn) => {
                            if let Some(w) = warn {
                                self.message = Some((tr("Save").into(), w));
                            }
                            if !copy {
                                let udl = self.find_udl_for(&path);
                                let d = &mut self.docs[i];
                                if d.syntax.is_none() && d.user_lang.is_none() {
                                    d.user_lang = udl;
                                    if d.user_lang.is_none() {
                                        d.syntax = self.hl.detect(Some(&path), d.text());
                                    }
                                }
                                self.s.add_recent(&path.display().to_string());
                                if let Some(dir) = path.parent() {
                                    self.s.last_dir = dir.display().to_string();
                                }
                            }
                            if self.close_after_save == Some(id) {
                                self.close_after_save = None;
                                let pane = self.close_queue.iter().find(|(x, _)| *x == id).map_or(self.cur, |(_, p)| *p);
                                self.close_queue.retain(|(x, _)| *x != id);
                                self.close_tab_now(id, pane);
                            }
                        }
                        Err(e) => {
                            self.message = Some((tr("Save failed").into(), e));
                            if self.close_after_save == Some(id) {
                                self.close_after_save = None;
                                self.cancel_close();
                            }
                        }
                    }
                }
                DialogResult::Picked(what, paths) => self.handle_pick(ctx, what, paths),
            }
        }
    }

    // ---- session -------------------------------------------------------------------------------

    pub fn restore_session(&mut self, session: &Session) {
        let mut ids = Vec::new();
        for sd in &session.docs {
            let id = self.alloc_id();
            let doc = match (&sd.path, &sd.text) {
                (Some(p), None) => {
                    let p = PathBuf::from(p);
                    let Ok(mut d) = Document::open(id, &p, Some(sd.encoding.clone()), self.s.default_eol) else {
                        ids.push(None);
                        continue;
                    };
                    d.syntax = sd.syntax.clone();
                    Some(d)
                }
                (path, Some(text)) => {
                    let mut d = Document::new_untitled(id, sd.untitled_no.max(1), sd.eol.unwrap_or(self.s.default_eol));
                    if let Some(p) = path {
                        let p = PathBuf::from(p);
                        d.disk_mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
                        d.disk_missing = !p.exists();
                        d.path = Some(p);
                    }
                    d.encoding = sd.encoding.clone();
                    d.syntax = sd.syntax.clone();
                    d.replace_text_raw(text.clone());
                    d.force_dirty();
                    Some(d)
                }
                (None, None) => None,
            };
            let Some(mut d) = doc else {
                ids.push(None);
                continue;
            };
            if let Some(eol) = sd.eol {
                d.eol = eol;
            }
            d.user_lang = sd.user_lang.clone();
            d.read_only = sd.read_only;
            d.pinned = sd.pinned;
            d.tab_color = sd.tab_color;
            d.bookmarks = sd.bookmarks.iter().copied().filter(|&b| b < d.line_count()).collect();
            d.folded = sd.folded.iter().copied().filter(|&b| b < d.line_count()).collect();
            d.set_sel(sd.sel.0, sd.sel.1);
            d.view.first_line = sd.first_line.min(d.line_count() - 1);
            ids.push(Some(d.id));
            let view = sd.view;
            let id = d.id;
            self.docs.push(d);
            if view != 1 {
                self.panes[0].tabs.push(id);
            }
            if view >= 1 {
                self.panes[1].tabs.push(id);
            }
        }
        let pos = |pane: &Pane, k: Option<usize>| -> usize {
            k.and_then(|k| ids.get(k).copied().flatten()).and_then(|id| pane.tabs.iter().position(|&t| t == id)).unwrap_or(0)
        };
        self.panes[0].active = pos(&self.panes[0], Some(session.active));
        self.panes[1].active = pos(&self.panes[1], session.active2);
        self.cur = 0;
        self.fix_panes_initial();
    }

    fn fix_panes_initial(&mut self) {
        if self.panes[0].tabs.is_empty() && !self.panes[1].tabs.is_empty() {
            self.panes.swap(0, 1);
        }
    }

    pub fn session(&self) -> Session {
        let keep = |d: &Document| !(d.path.is_none() && d.text().is_empty());
        let mut docs = Vec::new();
        let mut order: Vec<u64> = self.panes[0].tabs.clone();
        order.extend(self.panes[1].tabs.iter().filter(|t| !self.panes[0].tabs.contains(t)));
        let mut index_of = BTreeMap::new();
        for id in order {
            let Some(d) = self.doc_by_id(id) else { continue };
            if !keep(d) {
                continue;
            }
            let in0 = self.panes[0].tabs.contains(&id);
            let in1 = self.panes[1].tabs.contains(&id);
            index_of.insert(id, docs.len());
            docs.push(SessionDoc {
                path: d.path_string(),
                untitled_no: d.untitled_no,
                text: (d.is_dirty() || d.path.is_none()).then(|| d.text().to_owned()),
                encoding: d.encoding.clone(),
                eol: Some(d.eol),
                syntax: d.syntax.clone(),
                user_lang: d.user_lang.clone(),
                sel: (d.anchor, d.cursor),
                first_line: d.view.first_line,
                bookmarks: d.bookmarks.iter().copied().collect(),
                folded: d.folded.iter().copied().collect(),
                read_only: d.read_only,
                pinned: d.pinned,
                tab_color: d.tab_color,
                view: match (in0, in1) {
                    (true, true) => 2,
                    (false, true) => 1,
                    _ => 0,
                },
            });
        }
        Session {
            docs,
            active: self.panes[0].active_id().and_then(|id| index_of.get(&id).copied()).unwrap_or(0),
            active2: self.panes[1].active_id().and_then(|id| index_of.get(&id).copied()),
        }
    }

    pub fn load_session_file(&mut self, path: &Path) {
        match Session::load_from(path) {
            Some(s) => self.restore_session(&s),
            None => self.message = Some((tr("Load Session").into(), trf("Cannot read session file {}", &[&path.display()]))),
        }
    }

    fn session_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.cur, &self.panes[0].tabs, &self.panes[1].tabs, self.panes[0].active, self.panes[1].active).hash(&mut h);
        for d in &self.docs {
            (d.id, d.version, d.cursor, d.anchor, d.is_dirty(), d.read_only, d.bookmarks.len(), d.folded.len(), d.view.first_line).hash(&mut h);
            (d.pinned, d.tab_color).hash(&mut h);
            d.path.hash(&mut h);
            d.syntax.hash(&mut h);
            d.encoding.name.hash(&mut h);
        }
        h.finish()
    }

    pub fn persist(&mut self, force: bool) {
        if self.s.remember_session && !self.no_session {
            let sig = self.session_signature();
            if force || sig != self.persisted_session {
                let _ = self.session().save();
                self.persisted_session = sig;
            }
        }
        let json = serde_json::to_string(&self.s).unwrap_or_default();
        if force || json != self.persisted_settings {
            let _ = self.s.save();
            self.persisted_settings = json;
        }
        for p in self.projects.iter_mut().flatten() {
            if p.dirty && p.path.is_some() {
                let _ = p.save();
            }
        }
    }

    fn check_disk(&mut self) {
        for d in &mut self.docs {
            if d.monitoring {
                continue;
            }
            let Some(p) = d.path.clone() else { continue };
            match std::fs::metadata(&p).and_then(|m| m.modified()) {
                Err(_) => {
                    if !d.disk_missing {
                        d.disk_missing = true;
                        if d.disk_mtime.is_some() {
                            self.confirms.push_back(Confirm::KeepMissing(d.id));
                        }
                    }
                }
                Ok(t) => {
                    d.disk_missing = false;
                    if d.disk_mtime.is_some_and(|old| old != t) {
                        d.disk_mtime = Some(t);
                        if self.s.auto_reload && !d.is_dirty() {
                            let _ = d.load_from_disk(Some(d.encoding.clone()), self.s.default_eol);
                            if self.s.autodetect_scroll_end {
                                let n = d.text().len();
                                d.select(n..n);
                            }
                        } else if !self.confirms.contains(&Confirm::ReloadChanged(d.id)) {
                            self.confirms.push_back(Confirm::ReloadChanged(d.id));
                        }
                    } else if d.disk_mtime.is_none() {
                        d.disk_mtime = Some(t);
                    }
                }
            }
        }
    }

    /// Monitoring (tail -f): reload changed files and scroll to the end.
    fn monitor_files(&mut self) {
        for d in self.docs.iter_mut().filter(|d| d.monitoring) {
            let Some(p) = d.path.clone() else { continue };
            if let Ok(t) = std::fs::metadata(&p).and_then(|m| m.modified()) {
                if d.disk_mtime != Some(t) {
                    let enc = d.encoding.clone();
                    d.read_only = false;
                    let _ = d.load_from_disk(Some(enc), self.s.default_eol);
                    d.read_only = true;
                    let n = d.text().len();
                    d.set_sel(n, n);
                    d.view.scroll = Some(ScrollTo::Cursor);
                }
            }
        }
    }

    // ---- input ---------------------------------------------------------------------------------

    fn shortcuts(&self, ctx: &egui::Context) -> Vec<Cmd> {
        let mut table: Vec<(Modifiers, Key, Cmd, bool)> = Vec::new();
        for e in cmd::registry() {
            let sc = cmd::shortcut_of(e, &self.s.shortcuts);
            // Keys handled by the editor itself.
            if matches!(sc, "Tab" | "Shift+Tab" | "Del" | "Alt+F4" | "Ctrl+Z" | "Ctrl+Y" | "Ctrl+A" | "Ctrl+X" | "Ctrl+C" | "Ctrl+V") {
                continue;
            }
            if let Some((m, k)) = cmd::parse_shortcut(sc) {
                table.push((m, k, e.cmd.clone(), e.editor_only));
            }
        }
        table.push((Modifiers::CTRL, Key::Equals, Cmd::ZoomIn, false));
        table.push((Modifiers::CTRL, Key::Tab, Cmd::NextTab, false));
        table.push((Modifiers { ctrl: true, shift: true, ..Modifiers::NONE }, Key::Tab, Cmd::PrevTab, false));
        for (i, m) in self.s.macros.iter().enumerate() {
            if let Some((mo, k)) = cmd::parse_shortcut(&m.shortcut) {
                table.push((mo, k, Cmd::MacroRun(i), true));
            }
        }
        for (i, r) in self.s.run_commands.iter().enumerate() {
            if let Some((mo, k)) = cmd::parse_shortcut(&r.shortcut) {
                table.push((mo, k, Cmd::RunCommand(i), false));
            }
        }
        // Text fields (find dialog …) keep their own editing keys.
        let text_field_focus = !self.editor_focused && ctx.memory(|m| m.focused().is_some());
        let mut out = Vec::new();
        ctx.input_mut(|inp| {
            inp.events.retain(|e| {
                let Event::Key { key, pressed: true, modifiers: m, .. } = e else { return true };
                for (mods, k, cmd, needs_editor) in &table {
                    if k == key && m.ctrl == mods.ctrl && m.shift == mods.shift && m.alt == mods.alt {
                        if *needs_editor && !self.editor_focused {
                            return true;
                        }
                        if text_field_focus && !m.ctrl && !m.alt && !matches!(key, Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12) {
                            return true;
                        }
                        out.push(cmd.clone());
                        return false;
                    }
                }
                true
            })
        });
        out
    }

    fn record_and_replay_macro(&mut self, ctx: &egui::Context) {
        if self.macro_.queue.is_empty() {
            if let Some((chunks, last)) = self.macro_.until_end.take() {
                // Repeat while the caret moves forward and the end is not reached.
                let d = self.doc();
                let pos = d.cursor;
                if pos < d.text().len() && (last == usize::MAX || pos > last) {
                    self.macro_.queue.extend(chunks.iter().cloned());
                    self.macro_.until_end = Some((chunks, pos));
                }
            }
        }
        if let Some(chunk) = self.macro_.queue.pop_front() {
            let id = editor::editor_id(self.active_id(), self.cur);
            ctx.memory_mut(|m| m.request_focus(id));
            self.editor_focused = true;
            ctx.input_mut(|i| i.events.extend(chunk));
            ctx.request_repaint();
            return;
        }
        if let Some(rec) = &mut self.macro_.recording {
            if self.editor_focused {
                ctx.input(|i| {
                    for e in &i.events {
                        let keep = match e {
                            Event::Key { key, pressed: true, modifiers, .. } => {
                                !(modifiers.ctrl && modifiers.shift && matches!(key, Key::R | Key::P))
                            }
                            Event::Text(_) | Event::Paste(_) | Event::Copy | Event::Cut => true,
                            Event::Ime(egui::ImeEvent::Commit(_)) => true,
                            _ => false,
                        };
                        if keep {
                            rec.push(e.clone());
                        }
                    }
                });
            }
        }
    }

    pub fn keywords_for(&mut self, syntax: Option<&str>) -> std::rc::Rc<Vec<String>> {
        let key = syntax.unwrap_or("");
        if self.keywords_cache.0 != key {
            let mut kw = if key.is_empty() { Default::default() } else { self.hl.keywords(key) };
            if let Some(api) = crate::api::for_lang(key) {
                let mut v: Vec<String> = (*kw).clone();
                v.extend(api.words.iter().cloned());
                v.sort();
                v.dedup();
                kw = std::rc::Rc::new(v);
            }
            self.keywords_cache = (key.to_string(), kw);
        }
        self.keywords_cache.1.clone()
    }

    /// Updates the window title.
    fn update_title(&mut self, ctx: &egui::Context) {
        let d = self.doc();
        let name = d.path_string().unwrap_or_else(|| d.title());
        let t = format!("{}{name} - {APP_NAME}", if d.is_dirty() { "*" } else { "" });
        if t != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(t.clone()));
            self.title = t;
        }
    }

    /// Recomputes the function list in the background when the active document changed.
    fn update_function_list(&mut self, ctx: &egui::Context) {
        if let Some((doc_id, version, _, rx)) = &self.func.pending {
            if let Ok(sym) = rx.try_recv() {
                self.func.doc_id = *doc_id;
                self.func.version = *version;
                self.func.symbols = sym;
                self.func.pending = None;
            }
        }
        if !self.s.show_function_list {
            return;
        }
        let (id, version, syntax) = {
            let d = self.doc();
            (d.id, d.version, d.syntax.clone())
        };
        if self.func.doc_id == id && self.func.version == version {
            return;
        }
        if let Some((pid, pv, cancel, _)) = &self.func.pending {
            if *pid == id && *pv == version {
                return;
            }
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        // Debounce while typing.
        let since = self.func.requested_at.map_or(Duration::MAX, |t| t.elapsed());
        if self.func.doc_id == id && since < Duration::from_millis(700) {
            ctx.request_repaint_after(Duration::from_millis(700));
            return;
        }
        self.func.requested_at = Some(Instant::now());
        let Some(syntax) = syntax else {
            self.func.doc_id = id;
            self.func.version = version;
            self.func.symbols.clear();
            return;
        };
        let text = self.doc().text().to_owned();
        let ss = self.hl.syntaxes.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let s = crate::highlight::symbols(&ss, &syntax, &text, &c2);
            let _ = tx.send(s);
            ctx2.request_repaint();
        });
        self.func.pending = Some((id, version, cancel, rx));
    }

    fn poll_find_in_files(&mut self) {
        let Some(job) = &mut self.fif else { return };
        let mut done = None;
        let mut new_files = Vec::new();
        while let Ok(m) = job.rx.try_recv() {
            match m {
                crate::findfiles::Msg::File { path, hits } => new_files.push((path, hits)),
                crate::findfiles::Msg::Progress { scanned } => job.scanned = scanned,
                crate::findfiles::Msg::Done { scanned, cancelled } => done = Some((scanned, cancelled)),
            }
        }
        if let Some(block) = self.results.first_mut() {
            for (path, hits) in new_files {
                block.files.push(ResultFile { path: Some(path), doc_id: None, hits, collapsed: false });
            }
            let total: usize = block.files.iter().map(|f| f.hits.len()).sum();
            let base = block.title.split(" (").next().unwrap_or("").to_string();
            block.title = format!("{base} ({})", trf("{} hits in {} files", &[&total, &block.files.len()]));
            if let Some((scanned, cancelled)) = done {
                self.find.status = Some((
                    trf("Find in Files: {} hits in {} of {} searched files.", &[&total, &block.files.len(), &scanned]) + if cancelled { tr(" (cancelled)") } else { "" },
                    total == 0,
                ));
            }
        }
        if done.is_some() {
            self.fif = None;
        }
    }

    // ---- frame ---------------------------------------------------------------------------------

    pub(crate) fn frame(&mut self, ctx: &egui::Context) {
        self.handle_dialog_results(ctx);
        if let Some(rx) = &self.ipc_rx {
            let batches: Vec<Vec<PathBuf>> = rx.try_iter().collect();
            for files in batches {
                for f in files {
                    self.open_path(&f);
                }
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for f in dropped {
            self.open_path(&f);
        }
        self.poll_find_in_files();
        self.poll_update();
        self.poll_tray(ctx);
        self.update_function_list(ctx);

        // Close request (window manager, Alt+F4, File → Exit)
        let close_to_tray = matches!(self.s.tray, crate::settings::TrayMode::Close | crate::settings::TrayMode::MinimizeAndClose);
        if ctx.input(|i| i.viewport().close_requested()) && close_to_tray && !self.explicit_exit && !self.allow_close && self.hide_to_tray(ctx) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        } else if ctx.input(|i| i.viewport().close_requested()) {
            self.explicit_exit = false;
            if self.allow_close || (self.s.remember_session && !self.no_session) {
                self.persist(true);
                if self.owns_socket {
                    ipc::cleanup();
                }
                if let Some(exe) = self.restart_exe.take() {
                    crate::update::restart_when_exited(&exe);
                }
            } else {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                let mut ids = Vec::new();
                for p in 0..2 {
                    for &id in &self.panes[p].tabs {
                        ids.push((id, p));
                    }
                }
                self.close_queue.extend(ids);
                self.exit_after = true;
            }
        }

        let modal = !self.confirms.is_empty() || self.message.is_some();
        self.record_and_replay_macro(ctx);
        let mut cmds = if modal { Vec::new() } else { self.shortcuts(ctx) };

        // Ctrl+mouse wheel zooms the editor font.
        let zoom = ctx.input(|i| i.zoom_delta());
        if zoom != 1.0 && !modal {
            self.s.font_size = (self.s.font_size * zoom).clamp(6.0, 72.0);
        }

        let chrome = !self.postit && !self.distraction_free;
        if chrome && self.s.show_menu_bar {
            egui::TopBottomPanel::top("menu").show(ctx, |ui| {
                self.menu_bar(ui, &mut cmds);
                if self.s.show_toolbar {
                    ui.add_space(1.0);
                    self.toolbar(ui, &mut cmds);
                }
            });
        }
        if chrome && self.s.show_status_bar {
            egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui, &mut cmds));
        }
        if self.inc_search.is_some() {
            egui::TopBottomPanel::bottom("incsearch").show(ctx, |ui| self.inc_search_bar(ui));
        }
        if self.show_results && chrome {
            egui::TopBottomPanel::bottom("results").resizable(true).default_height(180.0).min_height(60.0).show(ctx, |ui| {
                self.results_panel(ui);
            });
        }
        if chrome {
            self.side_panels(ctx, &mut cmds);
        }

        for c in cmds.drain(..) {
            self.run(ctx, c);
        }
        self.process_close_queue(ctx);

        // Editor views
        let theme_name = self.s.theme().to_owned();
        crate::udl::set_dark(self.s.dark_mode);
        let pal = Palette::new(self.hl.theme(&theme_name), &self.s);
        let tab_bg = if ctx.style().visuals.dark_mode { Color32::from_gray(20) } else { Color32::from_gray(218) };
        egui::CentralPanel::default().frame(egui::Frame::none().fill(tab_bg)).show(ctx, |ui| {
            let full = ui.available_rect_before_wrap();
            let split = !self.panes[1].tabs.is_empty();
            if split {
                let w = full.width();
                let x = full.left() + (w * self.split_ratio).round();
                let left = egui::Rect::from_min_max(full.min, egui::pos2(x - 2.0, full.max.y));
                let right = egui::Rect::from_min_max(egui::pos2(x + 2.0, full.min.y), full.max);
                let handle = egui::Rect::from_min_max(egui::pos2(x - 3.0, full.min.y), egui::pos2(x + 3.0, full.max.y));
                let r = ui.interact(handle, ui.id().with("splitter"), egui::Sense::drag());
                if r.hovered() || r.dragged() {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if let Some(p) = r.interact_pointer_pos().filter(|_| r.dragged()) {
                    self.split_ratio = ((p.x - full.left()) / w).clamp(0.15, 0.85);
                }
                self.pane_ui(ui, 0, left, &pal, &mut cmds, chrome);
                self.pane_ui(ui, 1, right, &pal, &mut cmds, chrome);
            } else {
                self.pane_ui(ui, 0, full, &pal, &mut cmds, chrome);
            }
            if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
                ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(90));
                ui.painter().text(full.center(), egui::Align2::CENTER_CENTER, tr("Drop files here to open them"), FontId::proportional(22.0), Color32::WHITE);
            }
        });
        self.focus_editor = false;
        self.sync_scrolling();
        for c in cmds.drain(..) {
            self.run(ctx, c);
        }

        self.dialogs_ui(ctx);
        self.prefs_ui(ctx);
        self.update_title(ctx);

        if self.last_disk_check.elapsed() > Duration::from_secs(2) {
            self.last_disk_check = Instant::now();
            if self.s.file_autodetect && ctx.input(|i| i.focused) {
                self.check_disk();
            }
        }
        if self.docs.iter().any(|d| d.monitoring) {
            if self.last_monitor.elapsed() > Duration::from_millis(500) {
                self.last_monitor = Instant::now();
                self.monitor_files();
            }
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if self.last_persist.elapsed() > Duration::from_secs(self.s.backup_interval_secs.max(1)) {
            self.last_persist = Instant::now();
            self.persist(false);
        }
        ctx.request_repaint_after(Duration::from_secs(2));
    }

    /// Tab bar and editor of one view.
    fn pane_ui(&mut self, ui: &mut egui::Ui, pane: usize, rect: egui::Rect, pal: &Palette, cmds: &mut Vec<Cmd>, chrome: bool) {
        let Some(id) = self.panes[pane].active_id() else { return };
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(("pane", pane)));
        if chrome && self.s.show_tab_bar && self.s.tabs_vertical {
            let w = self.s.tab_max_width.clamp(80.0, 400.0).min(rect.width() * 0.4);
            let bar = egui::Rect::from_min_size(rect.min, egui::vec2(w, rect.height()));
            let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(egui::vec2(2.0, 4.0))).id_salt(("vtab_ui", pane)));
            self.tab_bar(&mut bar_ui, pane, cmds);
            ui.painter().vline(bar.right(), bar.y_range(), ui.visuals().widgets.noninteractive.bg_stroke);
            ui = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(egui::pos2(bar.right() + 1.0, rect.top()), rect.max)).id_salt(("pane_ed", pane)));
        } else if chrome && self.s.show_tab_bar {
            egui::Frame::none().inner_margin(egui::Margin { left: 4.0, right: 4.0, top: 4.0, bottom: 0.0 }).show(&mut ui, |ui| {
                self.tab_bar(ui, pane, cmds);
            });
        }
        let editor_rect = ui.available_rect_before_wrap();
        // Active view marker
        if !self.panes[1].tabs.is_empty() && pane == self.cur {
            ui.painter().line_segment([editor_rect.left_top(), editor_rect.right_top()], egui::Stroke::new(2.0_f32, crate::theme::accent(self.s.dark_mode)));
        }
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(editor_rect));
        if self.focus_editor && pane == self.cur && !self.find.open && !self.dlg.any_text_dialog() && self.inc_search.is_none() {
            ui.memory_mut(|m| m.request_focus(editor::editor_id(id, pane)));
        }
        let limit = (self.s.highlight_limit_mb * 1_048_576.0) as usize;
        let large_limit = (self.s.large_file_mb * 1_048_576.0) as usize;
        let (syntax, udl_name, len) = {
            let d = self.doc_by_id(id).unwrap();
            (d.syntax.clone(), d.user_lang.clone(), d.text().len())
        };
        let syntax = syntax.filter(|_| len <= limit);
        let keywords = self.keywords_for(syntax.as_deref());
        let tabs = self.s.tabs_for(syntax.as_deref());
        let udl_idx = udl_name.and_then(|n| self.udls.iter().position(|u| u.name == n)).filter(|_| len <= limit);
        let mut copied = None;
        let out = self.with_view(pane, id, |app| {
            let i = app.idx(id);
            let udl = udl_idx.map(|k| &app.udls[k]);
            editor::show(
                &mut ui,
                &mut app.docs[i],
                editor::Cfg {
                    hl: &app.hl,
                    s: &app.s,
                    pal,
                    syntax: if udl.is_some() { None } else { syntax.as_deref() },
                    udl,
                    overwrite: &mut app.overwrite,
                    cmds,
                    completion: &mut app.completion,
                    calltip: &mut app.calltip,
                    api: udl.map_or(syntax.as_deref(), |u| Some(u.name.as_str())).and_then(crate::api::for_lang),
                    keywords: &keywords,
                    tabs,
                    large: len > large_limit,
                    view: pane,
                    copied: &mut copied,
                },
            )
        });
        if let Some(c) = copied {
            self.push_clipboard_history(c);
        }
        if out.clicked {
            self.cur = pane;
        }
        if pane == self.cur {
            self.editor_focused = out.focused;
        }
    }

    pub fn push_clipboard_history(&mut self, c: String) {
        if c.is_empty() {
            return;
        }
        self.clipboard_history.retain(|x| *x != c);
        self.clipboard_history.insert(0, c);
        self.clipboard_history.truncate(50);
    }

    /// Synchronized scrolling of the two views.
    fn sync_scrolling(&mut self) {
        let (Some(a), Some(b)) = (self.panes[0].active_id(), self.panes[1].active_id()) else { return };
        let fl = |app: &mut Self, pane: usize, id: u64| app.with_view(pane, id, |app| app.doc_by_id(id).map_or((0, 0.0), |d| (d.view.first_line, d.view.x_offset)));
        let (l0, x0) = fl(self, 0, a);
        let (l1, x1) = fl(self, 1, b);
        if self.sync_v {
            let d0 = l0 as isize - self.panes[0].last_first_line as isize;
            let d1 = l1 as isize - self.panes[1].last_first_line as isize;
            if d0 != 0 && d1 == 0 {
                self.with_view(1, b, |app| app.doc_by_id_mut(b).map(|d| editor::scroll_lines(d, d0)));
            } else if d1 != 0 && d0 == 0 {
                self.with_view(0, a, |app| app.doc_by_id_mut(a).map(|d| editor::scroll_lines(d, d1)));
            }
        }
        if self.sync_h {
            if x0 != self.panes[0].last_x {
                self.with_view(1, b, |app| app.doc_by_id_mut(b).map(|d| d.view.x_offset = x0));
            } else if x1 != self.panes[1].last_x {
                self.with_view(0, a, |app| app.doc_by_id_mut(a).map(|d| d.view.x_offset = x1));
            }
        }
        let (l0, x0) = fl(self, 0, a);
        let (l1, x1) = fl(self, 1, b);
        self.panes[0].last_first_line = l0;
        self.panes[1].last_first_line = l1;
        self.panes[0].last_x = x0;
        self.panes[1].last_x = x1;
    }

    pub fn encodings_main() -> Vec<(&'static str, TextEncoding)> {
        vec![
            ("ANSI", TextEncoding::new(encoding_rs::WINDOWS_1252, false)),
            ("UTF-8", TextEncoding::utf8()),
            ("UTF-8-BOM", TextEncoding::new(encoding_rs::UTF_8, true)),
            ("UTF-16 BE BOM", TextEncoding::new(encoding_rs::UTF_16BE, true)),
            ("UTF-16 LE BOM", TextEncoding::new(encoding_rs::UTF_16LE, true)),
        ]
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }
}

/// Splits a recorded macro so that commands handled before the editor run in their own frame
/// and keep their order relative to typed text.
pub fn macro_chunks(events: &[Event]) -> Vec<Vec<Event>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for e in events {
        let is_cmd = matches!(e, Event::Key { key, modifiers, .. }
            if modifiers.ctrl || modifiers.alt || matches!(key, Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12));
        if is_cmd {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(vec![e.clone()]);
        } else {
            cur.push(e.clone());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let sep = if tr("decimal_sep") == "," { '.' } else { ',' };
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Pos2, RawInput, Rect};

    fn setup() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notepad-rs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        dir
    }

    struct H {
        ctx: Context,
        app: App,
    }

    impl H {
        fn new(files: Vec<PathBuf>) -> Self {
            let ctx = Context::default();
            let app = App::init(&ctx, CliOptions { files, ..Default::default() });
            let mut h = Self { ctx, app };
            h.frame(vec![]);
            h
        }
        fn frame(&mut self, events: Vec<Event>) {
            let raw = RawInput { events, screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))), focused: true, ..Default::default() };
            let app = &mut self.app;
            let _ = self.ctx.run(raw, |ctx| app.frame(ctx));
        }
        fn key(&mut self, key: Key, modifiers: Modifiers) {
            self.frame(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
            self.frame(vec![]);
        }
        fn typ(&mut self, t: &str) {
            self.frame(vec![Event::Text(t.into())]);
        }
        fn text(&self) -> String {
            self.app.doc().text().to_string()
        }
        fn cmd(&mut self, c: Cmd) {
            let ctx = self.ctx.clone();
            self.app.run(&ctx, c);
            self.frame(vec![]);
        }
    }

    const CS: Modifiers = Modifiers { ctrl: true, shift: true, ..Modifiers::NONE };

    // One test function: the tests share environment variables (config dir).
    #[test]
    fn app_flows() {
        let dir = setup();

        let mut h = H::new(vec![]);
        h.app.s.remember_session = false;
        h.typ("foo bar foo");
        assert_eq!(h.text(), "foo bar foo");
        h.key(Key::D, Modifiers::CTRL);
        assert_eq!(h.text(), "foo bar foo\nfoo bar foo");

        // Find dialog: Ctrl+F, type into the field, Enter = find next.
        h.key(Key::Home, Modifiers::CTRL);
        h.key(Key::F, Modifiers::CTRL);
        assert!(h.app.find.open);
        h.app.find.what.clear();
        h.frame(vec![]);
        h.typ("bar");
        h.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
        h.frame(vec![]);
        assert_eq!(h.app.find.what, "bar");
        assert_eq!(h.app.doc().sel(), 4..7);
        h.key(Key::F3, Modifiers::NONE);
        assert_eq!(h.app.doc().sel(), 16..19);
        h.app.find.with = "baz".into();
        h.app.replace_all(false);
        assert_eq!(h.text(), "foo baz foo\nfoo baz foo");
        h.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
        h.frame(vec![]);
        assert!(!h.app.find.open);
        h.key(Key::Z, Modifiers::CTRL);
        assert_eq!(h.text(), "foo bar foo\nfoo bar foo", "undo of replace-all");

        // Find All → results panel, F4 navigation
        h.app.find.what = "foo".into();
        h.app.find_all(false);
        assert_eq!(h.app.results[0].files[0].hits.len(), 4);
        h.key(Key::F4, Modifiers::NONE);
        h.key(Key::F4, Modifiers::NONE);
        assert_eq!(h.app.doc().sel(), 8..11);

        // Styles, bookmarks, multi-select
        h.cmd(Cmd::StyleAll(1));
        assert_eq!(h.app.doc().marks.iter().filter(|m| m.1 == 1).count(), 4);
        h.cmd(Cmd::ClearStyle(0));
        assert!(h.app.doc().marks.iter().all(|m| m.1 == 0));
        h.cmd(Cmd::MultiSelectAll(crate::cmd::Match::CaseAndWholeWord));
        assert_eq!(h.app.doc().all_sels().len(), 4);
        h.typ("X");
        assert_eq!(h.text(), "X bar X\nX bar X");
        h.key(Key::Escape, Modifiers::NONE);

        // Closing a modified document asks; Escape cancels.
        h.key(Key::W, Modifiers::CTRL);
        assert_eq!(h.app.confirms.front(), Some(&Confirm::SaveBeforeClose(h.app.active_id())));
        h.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
        assert!(h.app.confirms.is_empty());
        assert_eq!(h.app.docs.len(), 1);

        // Macro: record "x" + Enter, play it back twice.
        h.key(Key::End, Modifiers::CTRL);
        h.key(Key::R, CS);
        assert!(h.app.macro_.recording.is_some());
        h.typ("x");
        h.key(Key::Enter, Modifiers::NONE);
        h.key(Key::R, CS);
        assert_eq!(h.app.macro_.last.len(), 2);
        h.key(Key::P, CS);
        h.frame(vec![]);
        h.key(Key::P, CS);
        h.frame(vec![]);
        assert_eq!(h.text(), "X bar X\nX bar Xx\nx\nx\n");

        // Split view: clone, edit in one view, both see it; independent carets.
        let id = h.app.active_id();
        h.cmd(Cmd::CloneToOtherView);
        assert_eq!(h.app.panes[1].tabs, vec![id]);
        assert_eq!(h.app.cur, 1);
        h.app.with_view(1, id, |app| app.doc_mut().set_sel(0, 0));
        h.typ("V2 ");
        assert!(h.text().starts_with("V2 X bar"));
        let main_cursor = h.app.doc_by_id(id).unwrap().cursor;
        assert_eq!(main_cursor, h.text().len(), "main view caret stays at the end (shifted)");
        h.cmd(Cmd::FocusOtherView);
        assert_eq!(h.app.cur, 0);
        h.app.cur = 1;
        h.key(Key::W, Modifiers::CTRL);
        assert!(h.app.panes[1].tabs.is_empty(), "closing the clone keeps the document");
        assert_eq!(h.app.docs.len(), 1);

        // Folding commands
        h.cmd(Cmd::New);
        h.cmd(Cmd::SetSyntax(Some("Rust".into())));
        h.app.doc_mut().set_text("fn a() {\n    x;\n}\n".into(), 0..0);
        h.cmd(Cmd::FoldAll);
        assert_eq!(h.app.doc().folded.len(), 1);
        h.cmd(Cmd::UnfoldAll);
        assert!(h.app.doc().folded.is_empty());

        // Files: open, edit, save (with backup), session restore of unsaved changes.
        let file = dir.join("a.txt");
        std::fs::write(&file, "line1\r\nline2\r\n").unwrap();
        let mut h = H::new(vec![file.clone()]);
        h.app.s.remember_session = true;
        h.app.s.backup_on_save = BackupMode::Simple;
        assert_eq!(h.app.doc().eol, crate::doc::Eol::Crlf);
        h.typ("X");
        h.key(Key::S, Modifiers::CTRL);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "Xline1\r\nline2\r\n");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt.bak")).unwrap(), "line1\r\nline2\r\n");
        assert!(!h.app.doc().is_dirty());
        h.typ("Y");
        h.key(Key::N, Modifiers::CTRL);
        h.typ("unsaved");
        h.app.persist(true);
        drop(h);

        let h = H::new(vec![]);
        let texts: Vec<String> = h.app.panes[0].tabs.iter().map(|&id| h.app.doc_by_id(id).unwrap().text().to_string()).collect();
        assert!(texts.contains(&"XYline1\nline2\n".to_string()), "{texts:?}");
        assert!(texts.contains(&"unsaved".to_string()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "Xline1\r\nline2\r\n", "file untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
