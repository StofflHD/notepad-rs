//! Docking panels: Folder as Workspace, Project Panels, Document List, Function List,
//! Document Map, Character Panel, Clipboard History and the Search Results window.

use std::path::{Path, PathBuf};

use eframe::egui::{self, pos2, vec2, Color32, RichText, Sense, Stroke, TextEdit, Ui};

use crate::app::{App, Pick};
use crate::cmd::Cmd;
use crate::doc::EditKind;
use crate::i18n::{tr, trf};
use crate::project::{Node, Workspace};
use crate::tools;

/// Inline editing in the tree panels.
#[derive(Clone, Debug)]
pub enum TreeEdit {
    /// Rename a file/folder on disk.
    Rename(PathBuf, String),
    NewFile(PathBuf, String),
    NewFolder(PathBuf, String),
    /// Rename a project node: (panel, path in tree, name)
    ProjectRename(usize, Vec<usize>, String),
    ProjectNewFolder(usize, Vec<usize>, String),
}

const HTML_NAMES: &[(u32, &str)] = &[
    (34, "&quot;"),
    (38, "&amp;"),
    (60, "&lt;"),
    (62, "&gt;"),
    (160, "&nbsp;"),
    (169, "&copy;"),
    (174, "&reg;"),
    (176, "&deg;"),
    (196, "&Auml;"),
    (214, "&Ouml;"),
    (220, "&Uuml;"),
    (223, "&szlig;"),
    (228, "&auml;"),
    (246, "&ouml;"),
    (252, "&uuml;"),
    (8364, "&euro;"),
];

fn panel_header(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.strong(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = ui.small_button("×").clicked();
        });
    });
    close
}

impl App {
    pub fn side_panels(&mut self, ctx: &egui::Context, cmds: &mut Vec<Cmd>) {
        let left = self.s.show_workspace || self.s.show_project.iter().any(|&b| b) || self.s.show_doc_list;
        if left {
            egui::SidePanel::left("left_panels").resizable(true).default_width(240.0).min_width(140.0).show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                egui::ScrollArea::vertical().id_salt("left_scroll").show(ui, |ui| {
                    if self.s.show_workspace {
                        self.workspace_panel(ui, cmds);
                        ui.separator();
                    }
                    for i in 0..3 {
                        if self.s.show_project[i] {
                            self.project_panel(ui, ctx, i);
                            ui.separator();
                        }
                    }
                    if self.s.show_doc_list {
                        self.doc_list_panel(ui);
                    }
                });
            });
        }
        let right = self.s.show_function_list || self.s.show_doc_map || self.s.show_char_panel || self.s.show_clipboard_history;
        if right {
            egui::SidePanel::right("right_panels").resizable(true).default_width(230.0).min_width(120.0).show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                if self.s.show_doc_map {
                    self.doc_map_panel(ui);
                    ui.separator();
                }
                egui::ScrollArea::vertical().id_salt("right_scroll").show(ui, |ui| {
                    if self.s.show_function_list {
                        self.function_list_panel(ui);
                        ui.separator();
                    }
                    if self.s.show_char_panel {
                        self.char_panel(ui);
                        ui.separator();
                    }
                    if self.s.show_clipboard_history {
                        self.clipboard_panel(ui);
                    }
                });
            });
        }
        self.tree_edit_window(ctx);
    }

    fn insert_text(&mut self, t: &str) {
        let (pane, id) = (self.cur, self.active_id());
        let t = t.to_string();
        self.with_view(pane, id, |app| {
            let d = app.doc_mut();
            let r = d.sel();
            d.replace(r, &t, EditKind::Other);
        });
        self.focus_editor = true;
    }

    // ---- Folder as Workspace -------------------------------------------------------------------

    fn workspace_panel(&mut self, ui: &mut Ui, cmds: &mut Vec<Cmd>) {
        if panel_header(ui, tr("Folder as Workspace")) {
            self.s.show_workspace = false;
        }
        ui.horizontal(|ui| {
            if ui.small_button(tr("Add folder…")).clicked() {
                cmds.push(Cmd::OpenFolderAsWorkspace);
            }
            if ui.small_button(tr("Collapse all")).clicked() {
                self.workspace_open.clear();
            }
        });
        let roots = self.s.workspace_roots.clone();
        if roots.is_empty() {
            ui.label(RichText::new(tr("File → Open Folder as Workspace…")).weak());
        }
        for root in roots {
            let p = PathBuf::from(&root);
            let r = self.ws_entry(ui, &p, 0, true, cmds);
            r.context_menu(|ui| {
                if ui.button(tr("Remove from workspace")).clicked() {
                    self.s.workspace_roots.retain(|x| *x != root);
                    ui.close_menu();
                }
            });
        }
    }

    fn ws_entry(&mut self, ui: &mut Ui, p: &Path, depth: usize, root: bool, cmds: &mut Vec<Cmd>) -> egui::Response {
        let is_dir = p.is_dir();
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
        let open = self.workspace_open.contains(p);
        let icon = if is_dir {
            if open {
                "▾ 🗁"
            } else {
                "▸ 🗀"
            }
        } else {
            "   🗋"
        };
        let active = self.doc().path.as_deref() == Some(p);
        let r = ui
            .horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);
                ui.add(egui::SelectableLabel::new(active, format!("{icon} {name}")))
            })
            .inner;
        if r.clicked() && is_dir {
            if open {
                self.workspace_open.remove(p);
            } else {
                self.workspace_open.insert(p.to_path_buf());
            }
        }
        if (r.double_clicked() || r.clicked()) && !is_dir {
            cmds.push(Cmd::OpenPath(p.display().to_string()));
        }
        let pb = p.to_path_buf();
        let r = r.on_hover_text(p.display().to_string());
        r.context_menu(|ui| {
            let dir = if is_dir { pb.clone() } else { pb.parent().map(Path::to_path_buf).unwrap_or_default() };
            if !is_dir && ui.button(tr("Open")).clicked() {
                cmds.push(Cmd::OpenPath(pb.display().to_string()));
                ui.close_menu();
            }
            if ui.button(tr("New File")).clicked() {
                self.dlg.tree_edit = Some(TreeEdit::NewFile(dir.clone(), String::new()));
                ui.close_menu();
            }
            if ui.button(tr("New Folder")).clicked() {
                self.dlg.tree_edit = Some(TreeEdit::NewFolder(dir.clone(), String::new()));
                ui.close_menu();
            }
            if !root && ui.button(tr("Rename")).clicked() {
                self.dlg.tree_edit = Some(TreeEdit::Rename(pb.clone(), name.clone()));
                ui.close_menu();
            }
            if !root && ui.button(tr("Move to Recycle Bin")).clicked() {
                if let Err(e) = tools::move_to_trash(&pb) {
                    self.message = Some((tr("Move to Recycle Bin").into(), e));
                }
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Find in Files…")).clicked() {
                self.s.fif_dir = dir.display().to_string();
                cmds.push(Cmd::FindInFiles);
                ui.close_menu();
            }
            if ui.button(tr("Open Containing Folder in File Manager")).clicked() {
                let _ = tools::open_with_default(&dir.display().to_string());
                ui.close_menu();
            }
            if ui.button(tr("Open Containing Folder in Terminal")).clicked() {
                let _ = tools::open_terminal(&dir);
                ui.close_menu();
            }
            if ui.button(tr("Copy Path")).clicked() {
                ui.ctx().copy_text(pb.display().to_string());
                ui.close_menu();
            }
            if ui.button(tr("Copy File Name")).clicked() {
                ui.ctx().copy_text(name.clone());
                ui.close_menu();
            }
        });
        if is_dir && open {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(p)
                .map(|rd| rd.flatten().map(|e| e.path()).filter(|e| !e.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect())
                .unwrap_or_default();
            entries.sort_by_key(|e| (!e.is_dir(), e.file_name().map(|n| n.to_string_lossy().to_lowercase())));
            for e in entries.into_iter().take(2000) {
                self.ws_entry(ui, &e, depth + 1, false, cmds);
            }
        }
        r
    }

    // ---- Project panels ------------------------------------------------------------------------

    fn project_panel(&mut self, ui: &mut Ui, ctx: &egui::Context, i: usize) {
        if panel_header(ui, &trf("Project Panel {}", &[&(i + 1)])) {
            self.s.show_project[i] = false;
        }
        ui.horizontal(|ui| {
            ui.menu_button(tr("Workspace"), |ui| {
                if ui.button(tr("New Workspace")).clicked() {
                    self.projects[i] = Some(Workspace { path: None, projects: Vec::new(), dirty: true });
                    ui.close_menu();
                }
                if ui.button(tr("Open Workspace…")).clicked() {
                    self.pick(ctx, Pick::ProjectOpen(i), false, false, None, Some(("Workspace", vec!["xml", "workspace"])));
                    ui.close_menu();
                }
                if ui.button(tr("Save")).clicked() {
                    if let Some(w) = &mut self.projects[i] {
                        if w.path.is_some() {
                            let _ = w.save();
                        } else {
                            self.pick(ctx, Pick::ProjectSaveAs(i), false, true, Some("workspace.xml".into()), None);
                        }
                    }
                    ui.close_menu();
                }
                if ui.button(tr("Save As…")).clicked() {
                    self.pick(ctx, Pick::ProjectSaveAs(i), false, true, Some("workspace.xml".into()), None);
                    ui.close_menu();
                }
                if ui.button(tr("Add New Project")).clicked() {
                    let w = self.projects[i].get_or_insert_with(Default::default);
                    w.projects.push(Node::Folder { name: tr("Project Name").into(), children: Vec::new() });
                    w.dirty = true;
                    ui.close_menu();
                }
            });
            if let Some(w) = &self.projects[i] {
                let name = w.path.as_ref().and_then(|p| p.file_name()).map_or(tr("(unsaved)").to_string(), |n| n.to_string_lossy().into_owned());
                ui.label(RichText::new(format!("{}{name}", if w.dirty { "*" } else { "" })).weak());
            }
        });
        let Some(w) = self.projects[i].clone() else {
            ui.label(RichText::new(tr("No workspace. Use Workspace → New/Open.")).weak());
            return;
        };
        let mut action: Option<(Vec<usize>, &str)> = None;
        for (k, n) in w.projects.iter().enumerate() {
            self.project_node(ui, i, n, vec![k], 0, &mut action);
        }
        if let Some((at, a)) = action {
            match a {
                "open" => {
                    if let Some(Node::File { path }) = self.projects[i].as_mut().and_then(|w| w.node_mut(&at)).cloned() {
                        self.open_path(&path);
                    }
                }
                "toggle" => {
                    let key = (i, at);
                    if !self.project_open.remove(&key) {
                        self.project_open.insert(key);
                    }
                }
                "files" => self.pick(ctx, Pick::ProjectAddFiles(i, at), false, false, None, None),
                "dir" => self.pick(ctx, Pick::ProjectAddDir(i, at), true, false, None, None),
                "folder" => self.dlg.tree_edit = Some(TreeEdit::ProjectNewFolder(i, at, String::new())),
                "rename" => {
                    if let Some(Node::Folder { name, .. }) = self.projects[i].as_mut().and_then(|w| w.node_mut(&at)) {
                        let n = name.clone();
                        self.dlg.tree_edit = Some(TreeEdit::ProjectRename(i, at, n));
                    }
                }
                "remove" => {
                    if let Some(w) = &mut self.projects[i] {
                        w.remove(&at);
                    }
                }
                "up" | "down" => {
                    if let Some(w) = &mut self.projects[i] {
                        w.move_node(&at, if a == "up" { -1 } else { 1 });
                    }
                }
                "active" => {
                    if let Some(p) = self.doc().path.clone() {
                        if let Some(Node::Folder { children, .. }) = self.projects[i].as_mut().and_then(|w| w.node_mut(&at)) {
                            children.push(Node::File { path: p });
                            self.projects[i].as_mut().unwrap().dirty = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn project_node(&self, ui: &mut Ui, panel: usize, n: &Node, at: Vec<usize>, depth: usize, action: &mut Option<(Vec<usize>, &'static str)>) {
        match n {
            Node::Folder { name, children } => {
                let open = self.project_open.contains(&(panel, at.clone()));
                let icon = if depth == 0 { "📦" } else if open { "🗁" } else { "🗀" };
                let r = ui
                    .horizontal(|ui| {
                        ui.add_space(depth as f32 * 12.0);
                        ui.selectable_label(false, format!("{} {icon} {name}", if open { "▾" } else { "▸" }))
                    })
                    .inner;
                if r.clicked() {
                    *action = Some((at.clone(), "toggle"));
                }
                r.context_menu(|ui| {
                    for (l, a) in [
                        ("Rename", "rename"),
                        ("Add Folder", "folder"),
                        ("Add Files…", "files"),
                        ("Add Active Document", "active"),
                        ("Add Files from Directory…", "dir"),
                        ("Move Up", "up"),
                        ("Move Down", "down"),
                        ("Remove", "remove"),
                    ] {
                        if ui.button(tr(l)).clicked() {
                            *action = Some((at.clone(), a));
                            ui.close_menu();
                        }
                    }
                });
                if open {
                    for (k, c) in children.iter().enumerate() {
                        let mut p = at.clone();
                        p.push(k);
                        self.project_node(ui, panel, c, p, depth + 1, action);
                    }
                }
            }
            Node::File { path } => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let exists = path.exists();
                let r = ui
                    .horizontal(|ui| {
                        ui.add_space(depth as f32 * 12.0);
                        let t = if exists { RichText::new(format!("   🗋 {name}")) } else { RichText::new(format!("   🗋 {name}")).color(Color32::GRAY).strikethrough() };
                        ui.selectable_label(self.doc().path.as_deref() == Some(path.as_path()), t)
                    })
                    .inner
                    .on_hover_text(path.display().to_string());
                if r.double_clicked() || r.clicked() {
                    *action = Some((at.clone(), "open"));
                }
                r.context_menu(|ui| {
                    for (l, a) in [("Open", "open"), ("Move Up", "up"), ("Move Down", "down"), ("Remove", "remove")] {
                        if ui.button(tr(l)).clicked() {
                            *action = Some((at.clone(), a));
                            ui.close_menu();
                        }
                    }
                });
            }
        }
    }

    fn tree_edit_window(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.dlg.tree_edit.take() else { return };
        let (title, text) = match &mut e {
            TreeEdit::Rename(_, t) => (tr("Rename"), t),
            TreeEdit::NewFile(_, t) => (tr("New File"), t),
            TreeEdit::NewFolder(_, t) => (tr("New Folder"), t),
            TreeEdit::ProjectRename(_, _, t) => (tr("Rename"), t),
            TreeEdit::ProjectNewFolder(_, _, t) => (tr("Add Folder"), t),
        };
        let mut open = true;
        let mut ok = false;
        egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            let r = ui.add(TextEdit::singleline(text).desired_width(300.0));
            r.request_focus();
            ok = (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button(tr("OK")).clicked();
        });
        if ok {
            let res: Result<(), String> = match &e {
                TreeEdit::Rename(p, n) => std::fs::rename(p, p.with_file_name(n)).map_err(|e| e.to_string()),
                TreeEdit::NewFile(dir, n) => {
                    let p = dir.join(n);
                    std::fs::write(&p, "").map_err(|e| e.to_string()).map(|_| {
                        self.open_path(&p);
                    })
                }
                TreeEdit::NewFolder(dir, n) => std::fs::create_dir(dir.join(n)).map_err(|e| e.to_string()),
                TreeEdit::ProjectRename(i, at, n) => {
                    if let Some(w) = &mut self.projects[*i] {
                        if let Some(Node::Folder { name, .. }) = w.node_mut(at) {
                            *name = n.clone();
                        }
                        w.dirty = true;
                    }
                    Ok(())
                }
                TreeEdit::ProjectNewFolder(i, at, n) => {
                    if let Some(w) = &mut self.projects[*i] {
                        if let Some(Node::Folder { children, .. }) = w.node_mut(at) {
                            children.push(Node::Folder { name: n.clone(), children: Vec::new() });
                        }
                        w.dirty = true;
                    }
                    Ok(())
                }
            };
            if let Err(err) = res {
                self.message = Some((title.into(), err));
            }
        } else if open && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dlg.tree_edit = Some(e);
        }
    }

    // ---- Document list ---------------------------------------------------------------------------

    fn doc_list_panel(&mut self, ui: &mut Ui) {
        if panel_header(ui, tr("Document List")) {
            self.s.show_doc_list = false;
        }
        let mut activate = None;
        egui::Grid::new("doclist").striped(true).num_columns(2).show(ui, |ui| {
            ui.strong(tr("Name"));
            ui.strong(tr("Ext."));
            ui.end_row();
            for p in 0..2 {
                for (i, &id) in self.panes[p].tabs.iter().enumerate() {
                    let Some(d) = self.doc_by_id(id) else { continue };
                    let active = p == self.cur && i == self.panes[p].active;
                    let title = d.title();
                    let (stem, ext) = match title.rsplit_once('.') {
                        Some((a, b)) if !a.is_empty() => (a.to_string(), b.to_string()),
                        _ => (title.clone(), String::new()),
                    };
                    let label = format!("{}{}{stem}", if p == 1 { "² " } else { "" }, if d.is_dirty() { "*" } else { "" });
                    let r = ui.selectable_label(active, label);
                    let r = match &d.path {
                        Some(path) => r.on_hover_text(path.display().to_string()),
                        None => r,
                    };
                    if r.clicked() {
                        activate = Some((p, i));
                    }
                    ui.label(ext);
                    ui.end_row();
                }
            }
        });
        if let Some((p, i)) = activate {
            self.activate_tab(p, i);
        }
    }

    // ---- Function list -------------------------------------------------------------------------

    fn function_list_panel(&mut self, ui: &mut Ui) {
        if panel_header(ui, tr("Function List")) {
            self.s.show_function_list = false;
        }
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut self.func.filter).desired_width(140.0).hint_text(tr("Search")));
            ui.toggle_value(&mut self.func.sorted, "A↓").on_hover_text(tr("Sort"));
            if ui.small_button("⟳").on_hover_text(tr("Reload")).clicked() {
                self.func.version = u64::MAX;
            }
            if self.func.pending.is_some() {
                ui.spinner();
            }
        });
        if self.func.doc_id != self.active_id() {
            return;
        }
        if self.doc().syntax.is_none() {
            ui.label(RichText::new(tr("No language set for this document.")).weak());
            return;
        }
        let f = self.func.filter.to_lowercase();
        let mut syms: Vec<&crate::highlight::Symbol> = self.func.symbols.iter().filter(|s| f.is_empty() || s.name.to_lowercase().contains(&f)).collect();
        if self.func.sorted {
            syms.sort_by_key(|s| s.name.to_lowercase());
        }
        let cur_line = self.doc().line_of(self.doc().cursor);
        let current = self.func.symbols.iter().rev().find(|s| s.line <= cur_line).map(|s| (s.line, s.name.clone()));
        let mut goto = None;
        for s in syms.iter().take(5000) {
            let icon = match s.kind {
                "class" | "struct" | "interface" | "trait" | "enum" | "type" => "◆",
                "impl" | "namespace" | "module" => "▣",
                "section" => "§",
                _ => "ƒ",
            };
            let indent = if self.func.sorted { 0 } else { s.depth };
            let is_cur = current.as_ref().is_some_and(|(l, n)| *l == s.line && *n == s.name);
            let r = ui
                .horizontal(|ui| {
                    ui.add_space(indent as f32 * 12.0);
                    ui.selectable_label(is_cur, format!("{icon} {}", s.name))
                })
                .inner
                .on_hover_text(trf("{} – line {}", &[&s.kind, &(s.line + 1)]));
            if r.clicked() {
                goto = Some(s.line);
            }
        }
        if syms.is_empty() && self.func.pending.is_none() {
            ui.label(RichText::new(tr("(empty)")).weak());
        }
        if let Some(l) = goto {
            let (pane, id) = (self.cur, self.active_id());
            self.with_view(pane, id, |app| app.doc_mut().goto_line(l));
            self.focus_editor = true;
        }
    }

    // ---- Document map ----------------------------------------------------------------------------

    fn doc_map_panel(&mut self, ui: &mut Ui) {
        if panel_header(ui, tr("Document Map")) {
            self.s.show_doc_map = false;
        }
        let (pane, id) = (self.cur, self.active_id());
        let height = (ui.available_height() * 0.6).max(150.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
        let theme = self.hl.theme(self.s.theme()).clone();
        let colors = crate::highlight::theme_colors(&theme);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, colors.bg);
        let line_h = 2.0;
        let char_w = 1.0;
        let max_lines = (rect.height() / line_h) as usize;
        let mut new_first = None;
        self.with_view(pane, id, |app| {
            let d = app.doc_by_id_mut(id).unwrap();
            let n = d.line_count();
            let first = d.view.first_line;
            let vis_est = 40usize;
            // Map window: centred on the view.
            let start = first.saturating_sub(max_lines.saturating_sub(vis_est) / 2).min(n.saturating_sub(max_lines));
            for k in 0..max_lines.min(n - start) {
                let l = start + k;
                let y = rect.top() + k as f32 * line_h;
                let text = d.line_text(l);
                let spans = d.hl.line_spans(l);
                let mut x = rect.left() + 2.0;
                let mut draw_run = |t: &str, c: Color32| {
                    let mut seg_start: Option<f32> = None;
                    for ch in t.chars() {
                        let w = if ch == '\t' { 4.0 * char_w } else { char_w };
                        if ch.is_whitespace() {
                            if let Some(s0) = seg_start.take() {
                                painter.line_segment([pos2(s0, y + 0.5), pos2(x, y + 0.5)], Stroke::new(1.5_f32, c));
                            }
                        } else if seg_start.is_none() {
                            seg_start = Some(x);
                        }
                        x += w;
                        if x > rect.right() {
                            break;
                        }
                    }
                    if let Some(s0) = seg_start {
                        painter.line_segment([pos2(s0, y + 0.5), pos2(x, y + 0.5)], Stroke::new(1.5_f32, c));
                    }
                };
                match spans {
                    Some(sp) => {
                        let mut pos = 0;
                        for (len, st) in sp {
                            let end = (pos + len).min(text.len());
                            if end > pos {
                                draw_run(&text[pos..end], crate::highlight::to_color(st.foreground).gamma_multiply(0.8));
                            }
                            pos += len;
                        }
                    }
                    None => draw_run(text, colors.fg.gamma_multiply(0.6)),
                }
            }
            // Viewport frame
            let vy = rect.top() + (first.saturating_sub(start)) as f32 * line_h;
            let vr = egui::Rect::from_min_size(pos2(rect.left(), vy), vec2(rect.width(), vis_est as f32 * line_h));
            painter.rect_filled(vr, 0.0, Color32::from_rgba_unmultiplied(120, 120, 120, 40));
            painter.rect_stroke(vr, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(120, 120, 120)));
            if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged()) {
                let l = start + ((p.y - rect.top()) / line_h) as usize;
                new_first = Some(l.saturating_sub(vis_est / 2).min(n - 1));
            }
        });
        if let Some(l) = new_first {
            self.with_view(pane, id, |app| {
                let d = app.doc_by_id_mut(id).unwrap();
                d.view.first_line = d.visible_from(l);
                d.view.first_row = 0;
            });
        }
    }

    // ---- Character panel -------------------------------------------------------------------------

    fn char_panel(&mut self, ui: &mut Ui) {
        if panel_header(ui, tr("ASCII Codes Insertion Panel")) {
            self.s.show_char_panel = false;
        }
        let mut insert = None;
        egui::Grid::new("chars").striped(true).num_columns(4).show(ui, |ui| {
            ui.strong(tr("Value"));
            ui.strong("Hex");
            ui.strong(tr("Character"));
            ui.strong("HTML");
            ui.end_row();
            for v in 0u32..256 {
                // Codes 128–159 are shown as Windows-1252 (like Notepad++).
                let ch = if (128..160).contains(&v) {
                    encoding_rs::WINDOWS_1252.decode_without_bom_handling(&[v as u8]).0.chars().next().unwrap_or('?')
                } else {
                    char::from_u32(v).unwrap_or('?')
                };
                let disp = if v < 32 {
                    ["NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "TAB", "LF", "VT", "FF", "CR", "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US"][v as usize].to_string()
                } else if v == 127 {
                    "DEL".into()
                } else {
                    ch.to_string()
                };
                let html = HTML_NAMES.iter().find(|(c, _)| *c == ch as u32).map_or(if v >= 32 { format!("&#{};", ch as u32) } else { String::new() }, |(_, n)| n.to_string());
                if ui.selectable_label(false, v.to_string()).clicked() || ui.selectable_label(false, format!("{v:02X}")).clicked() || ui.selectable_label(false, RichText::new(disp).monospace()).clicked() {
                    insert = Some(ch.to_string());
                }
                if ui.selectable_label(false, &html).on_hover_text(tr("Insert HTML entity")).clicked() && !html.is_empty() {
                    insert = Some(html.clone());
                }
                ui.end_row();
            }
        });
        if let Some(t) = insert {
            self.insert_text(&t);
        }
    }

    // ---- Clipboard history -----------------------------------------------------------------------

    fn clipboard_panel(&mut self, ui: &mut Ui) {
        if panel_header(ui, tr("Clipboard History")) {
            self.s.show_clipboard_history = false;
        }
        if self.clipboard_history.is_empty() {
            ui.label(RichText::new(tr("Copy or cut text in the editor to fill the history.")).weak());
        }
        let mut insert = None;
        let mut remove = None;
        for (i, t) in self.clipboard_history.iter().enumerate() {
            let preview: String = t.chars().take(60).map(|c| if c == '\n' { '⏎' } else { c }).collect();
            let r = ui.selectable_label(false, RichText::new(preview).monospace()).on_hover_text(t.chars().take(1000).collect::<String>());
            if r.double_clicked() || r.clicked() {
                insert = Some(t.clone());
            }
            r.context_menu(|ui| {
                if ui.button(tr("Paste")).clicked() {
                    insert = Some(t.clone());
                    ui.close_menu();
                }
                if ui.button(tr("Remove")).clicked() {
                    remove = Some(i);
                    ui.close_menu();
                }
            });
        }
        if let Some(t) = insert {
            self.insert_text(&t);
        }
        if let Some(i) = remove {
            self.clipboard_history.remove(i);
        }
    }

    // ---- Search results --------------------------------------------------------------------------

    pub fn results_panel(&mut self, ui: &mut Ui) {
        let mut close = false;
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.strong(tr("Search results"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = ui.small_button("×").on_hover_text(tr("Close (F7)")).clicked();
                clear = ui.small_button(tr("Clear all")).clicked();
                if ui.small_button(tr("Collapse all")).clicked() {
                    for b in &mut self.results {
                        b.collapsed = true;
                    }
                }
                if ui.small_button(tr("Expand all")).clicked() {
                    for b in &mut self.results {
                        b.collapsed = false;
                        for f in &mut b.files {
                            f.collapsed = false;
                        }
                    }
                }
            });
        });
        if self.results.is_empty() {
            ui.label(RichText::new(tr("No search results yet. Use \"Find All…\" or \"Find in Files\".")).weak());
        }
        // Flattened rows: (block, file, hit) — None for headers.
        let mut rows: Vec<(usize, Option<usize>, Option<usize>)> = Vec::new();
        for (bi, b) in self.results.iter().enumerate() {
            rows.push((bi, None, None));
            if b.collapsed {
                continue;
            }
            for (fi, f) in b.files.iter().enumerate() {
                rows.push((bi, Some(fi), None));
                if f.collapsed {
                    continue;
                }
                for hi in 0..f.hits.len() {
                    rows.push((bi, Some(fi), Some(hi)));
                }
            }
        }
        let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 3.0;
        let mut open: Option<(usize, usize, usize)> = None;
        let mut toggle_block = None;
        let mut toggle_file = None;
        let mut copy_text: Option<String> = None;
        egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
            for &(bi, fi, hi) in &rows[range] {
                let b = &self.results[bi];
                match (fi, hi) {
                    (None, _) => {
                        let r = ui.selectable_label(false, RichText::new(format!("{} {}", if b.collapsed { "▸" } else { "▾" }, b.title)).strong().color(Color32::from_rgb(60, 110, 200)));
                        if r.clicked() {
                            toggle_block = Some(bi);
                        }
                    }
                    (Some(fi), None) => {
                        let f = &b.files[fi];
                        let name = f.path.as_ref().map(|p| p.display().to_string()).or_else(|| f.doc_id.and_then(|id| self.doc_by_id(id)).map(|d| d.title())).unwrap_or_default();
                        let r = ui.selectable_label(false, RichText::new(format!("  {} {name} ({})", if f.collapsed { "▸" } else { "▾" }, trf("{} hits", &[&f.hits.len()]))).strong().color(Color32::from_rgb(40, 140, 60)));
                        if r.clicked() {
                            toggle_file = Some((bi, fi));
                        }
                    }
                    (Some(fi), Some(hi)) => {
                        let h = &b.files[fi].hits[hi];
                        let cur = self.result_pos == Some((bi, fi, hi));
                        let text = format!("      {} {:>6}: {}", tr("Line"), h.line + 1, h.line_text);
                        let r = ui.add(egui::SelectableLabel::new(cur, RichText::new(text).monospace()));
                        if r.double_clicked() || r.clicked() {
                            open = Some((bi, fi, hi));
                        }
                        r.context_menu(|ui| {
                            if ui.button(tr("Copy")).clicked() {
                                copy_text = Some(h.line_text.clone());
                                ui.close_menu();
                            }
                            if ui.button(tr("Copy Pathname(s)")).clicked() {
                                copy_text = Some(b.files.iter().filter_map(|f| f.path.as_ref().map(|p| p.display().to_string())).collect::<Vec<_>>().join("\n"));
                                ui.close_menu();
                            }
                            if ui.button(tr("Copy all lines")).clicked() {
                                copy_text = Some(b.files.iter().flat_map(|f| f.hits.iter().map(|h| h.line_text.clone())).collect::<Vec<_>>().join("\n"));
                                ui.close_menu();
                            }
                        });
                    }
                }
            }
        });
        if let Some(t) = copy_text {
            ui.ctx().copy_text(t);
        }
        if let Some(bi) = toggle_block {
            self.results[bi].collapsed = !self.results[bi].collapsed;
        }
        if let Some((bi, fi)) = toggle_file {
            let f = &mut self.results[bi].files[fi];
            f.collapsed = !f.collapsed;
        }
        if let Some((bi, fi, hi)) = open {
            self.result_pos = Some((bi, fi, hi));
            self.open_result(bi, fi, hi);
        }
        if clear {
            self.results.clear();
            self.result_pos = None;
        }
        if close {
            self.show_results = false;
        }
    }
}
