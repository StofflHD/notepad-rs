//! Command handling (`App::run`) and the search operations behind the Find dialog.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use eframe::egui::{self, Event, ViewportCommand};
use regex::Regex;

use crate::app::{App, Confirm, FifJob, FindTab, Pick, ResultBlock, ResultFile};
use crate::cmd::{Cmd, Hash, Match};
use crate::doc::{EditKind, ScrollTo};
use crate::editor;
use crate::highlight;
use crate::i18n::{tr, trf};
use crate::search::{self, Mode};
use crate::settings::push_history;
use crate::textops::{self, LineOp};
use crate::tools;

/// Commands that change tabs/views and must not run with a swapped clone state.
fn structural(c: &Cmd) -> bool {
    matches!(
        c,
        Cmd::New
            | Cmd::Open
            | Cmd::OpenPath(_)
            | Cmd::OpenAllRecent
            | Cmd::ReopenClosed
            | Cmd::Close
            | Cmd::CloseDoc(_)
            | Cmd::CloseAll
            | Cmd::CloseAllButCurrent
            | Cmd::CloseAllButPinned
            | Cmd::CloseLeft
            | Cmd::CloseRight
            | Cmd::CloseUnchanged
            | Cmd::MoveToTrash
            | Cmd::LoadSession
            | Cmd::MoveToOtherView
            | Cmd::CloneToOtherView
            | Cmd::FocusOtherView
            | Cmd::TabN(_)
            | Cmd::FirstTab
            | Cmd::LastTab
            | Cmd::NextTab
            | Cmd::PrevTab
            | Cmd::MoveTabForward
            | Cmd::MoveTabBackward
            | Cmd::Compare
            | Cmd::CompareClear
            | Cmd::NextResult
            | Cmd::PrevResult
            | Cmd::Exit
            | Cmd::Rename
    )
}

impl App {
    pub fn run(&mut self, ctx: &egui::Context, cmd: Cmd) {
        if structural(&cmd) {
            self.run_inner(ctx, cmd);
        } else {
            let (pane, id) = (self.cur, self.active_id());
            self.with_view(pane, id, |app| app.run_inner(ctx, cmd));
        }
    }

    pub fn transform(&mut self, f: impl FnOnce(&str, &std::ops::Range<usize>) -> textops::Out) {
        let doc = self.doc_mut();
        if doc.read_only {
            return;
        }
        if let Some((t, sel)) = f(doc.text(), &doc.sel()) {
            doc.set_text(t, sel);
        }
    }

    /// Replaces the selection (or the whole text) with `f(text)`.
    fn convert_selection(&mut self, f: impl Fn(&str) -> Result<String, String>) {
        let doc = self.doc_mut();
        if doc.read_only {
            return;
        }
        let r = if doc.has_sel() { doc.sel() } else { 0..doc.text().len() };
        match f(&doc.text()[r.clone()]) {
            Ok(new) => {
                let start = r.start;
                let mut t = doc.text().to_string();
                t.replace_range(r, &new);
                doc.set_text(t, start..start + new.len());
            }
            Err(e) => self.message = Some((tr("Error").into(), e)),
        }
    }

    fn inject(&mut self, ctx: &egui::Context, ev: Event) {
        let id = editor::editor_id(self.active_id(), self.cur);
        ctx.memory_mut(|m| m.request_focus(id));
        ctx.input_mut(|i| i.events.push(ev));
    }

    pub fn run_vars(&self) -> tools::RunVars {
        let d = self.doc();
        let (line, col) = d.line_col(d.cursor);
        tools::RunVars {
            full_path: d.path_string().unwrap_or_default(),
            word: if d.has_sel() {
                d.selected_text().to_string()
            } else {
                editor::word_at(d.text(), d.cursor, &self.s.word_chars).map(|r| d.text()[r].to_string()).unwrap_or_default()
            },
            line: line + 1,
            column: col + 1,
            line_text: d.line_text(line).to_string(),
            selection: d.selected_text().to_string(),
        }
    }

    fn all_tab_ids(&self) -> Vec<u64> {
        let mut v = self.panes[0].tabs.clone();
        v.extend(self.panes[1].tabs.iter().filter(|t| !self.panes[0].tabs.contains(t)));
        v
    }

    fn run_inner(&mut self, ctx: &egui::Context, cmd: Cmd) {
        let pane = self.cur;
        let id = self.active_id();
        match cmd {
            // ---- File ------------------------------------------------------------------------
            Cmd::New => {
                self.new_doc();
            }
            Cmd::Open => self.open_dialog(ctx),
            Cmd::OpenPath(p) => {
                self.open_path(Path::new(&p));
            }
            Cmd::OpenFolderAsWorkspace => self.pick(ctx, Pick::WorkspaceFolder, true, false, None, None),
            Cmd::OpenInDefaultViewer => {
                if let Some(p) = self.doc().path_string() {
                    let _ = tools::open_with_default(&p);
                }
            }
            Cmd::OpenContainingFolder => {
                if let Some(dir) = self.doc().path.as_ref().and_then(|p| p.parent()) {
                    let _ = tools::open_with_default(&dir.display().to_string());
                }
            }
            Cmd::OpenTerminal => {
                let dir = self.doc().path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf)).or_else(|| self.start_dir());
                if let Some(d) = dir {
                    if let Err(e) = tools::open_terminal(&d) {
                        self.message = Some((tr("Terminal").into(), e));
                    }
                }
            }
            Cmd::Reload => {
                if self.doc().path.is_some() {
                    if self.doc().is_dirty() {
                        self.confirms.push_back(Confirm::ReloadDiscard(id));
                    } else {
                        let eol = self.s.default_eol;
                        let enc = self.doc().encoding.clone();
                        if let Err(e) = self.doc_mut().load_from_disk(Some(enc), eol) {
                            self.message = Some((tr("Reload from Disk").into(), e));
                        }
                    }
                }
            }
            Cmd::Save => {
                self.save(id);
            }
            Cmd::SaveAs => self.save_as_dialog(id, false),
            Cmd::SaveCopyAs => self.save_as_dialog(id, true),
            Cmd::SaveAll => {
                let ids: Vec<u64> = self.docs.iter().filter(|d| d.is_dirty() && d.path.is_some()).map(|d| d.id).collect();
                for i in ids {
                    self.save(i);
                }
                if let Some(d) = self.docs.iter().find(|d| d.is_dirty() && d.path.is_none()) {
                    let i = d.id;
                    self.activate_id(i);
                    self.save_as_dialog(i, false);
                }
            }
            Cmd::Rename => {
                if let Some(p) = self.doc().path.as_ref() {
                    self.dlg.rename = Some((id, p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                } else {
                    self.save_as_dialog(id, false);
                }
            }
            Cmd::Close => self.request_close(vec![id]),
            Cmd::CloseDoc(x) => self.request_close(vec![x]),
            Cmd::CloseAll => {
                let mut v = Vec::new();
                for p in 0..2 {
                    v.extend(self.panes[p].tabs.iter().map(|&t| (t, p)));
                }
                self.close_queue.extend(v);
            }
            Cmd::CloseAllButCurrent => {
                let v: Vec<u64> = self.panes[pane].tabs.iter().copied().filter(|&t| t != id).collect();
                self.request_close(v);
            }
            Cmd::CloseAllButPinned => {
                let v: Vec<u64> = self.panes[pane].tabs.iter().copied().filter(|&t| !self.doc_by_id(t).is_some_and(|d| d.pinned)).collect();
                self.request_close(v);
            }
            Cmd::CloseLeft => {
                let i = self.panes[pane].active;
                let v = self.panes[pane].tabs[..i].to_vec();
                self.request_close(v);
            }
            Cmd::CloseRight => {
                let i = self.panes[pane].active;
                let v = self.panes[pane].tabs[i + 1..].to_vec();
                self.request_close(v);
            }
            Cmd::CloseUnchanged => {
                let v: Vec<u64> = self.panes[pane].tabs.iter().copied().filter(|&t| self.doc_by_id(t).is_some_and(|d| !d.is_dirty())).collect();
                self.request_close(v);
            }
            Cmd::MoveToTrash => {
                if self.doc().path.as_ref().is_some_and(|p| p.exists()) {
                    self.confirms.push_back(Confirm::TrashFile(id));
                }
            }
            Cmd::LoadSession => self.pick(ctx, Pick::LoadSession, false, false, None, None),
            Cmd::SaveSession => self.pick(ctx, Pick::SaveSession, false, true, Some("session.json".into()), None),
            Cmd::Print => self.dlg.print = Some(Default::default()),
            Cmd::PrintNow => {
                let ps = self.print_postscript();
                let cmd = self.s.print_command.clone();
                if let Err(e) = tools::send_to_printer(&cmd, None, 1, &ps) {
                    self.message = Some((tr("Print").into(), e));
                }
            }
            Cmd::OpenAllRecent => {
                for p in self.s.recent.clone() {
                    self.open_path(Path::new(&p));
                }
            }
            Cmd::ClearRecent => self.s.recent.clear(),
            Cmd::ReopenClosed => {
                if let Some(p) = self.closed_paths.pop() {
                    self.open_path(Path::new(&p));
                }
            }
            Cmd::Exit => ctx.send_viewport_cmd(ViewportCommand::Close),

            // ---- Edit ------------------------------------------------------------------------
            Cmd::Undo => self.doc_mut().undo(),
            Cmd::Redo => self.doc_mut().redo(),
            Cmd::Cut => self.inject(ctx, Event::Cut),
            Cmd::Copy => self.inject(ctx, Event::Copy),
            Cmd::Paste => {
                if let Ok(t) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                    self.inject(ctx, Event::Paste(t));
                }
            }
            Cmd::Delete => {
                let d = self.doc_mut();
                let edits: Vec<(std::ops::Range<usize>, String)> =
                    d.all_sels().into_iter().filter(|(a, c)| a != c).map(|(a, c)| (a.min(c)..a.max(c), String::new())).collect();
                d.replace_multi(edits, false);
            }
            Cmd::SelectAll => {
                let d = self.doc_mut();
                let n = d.text().len();
                d.set_sel(0, n);
            }
            Cmd::BeginEndSelect | Cmd::BeginEndSelectColumn => {
                let column = cmd == Cmd::BeginEndSelectColumn;
                let begin = self.begin_select.take();
                let d = self.doc_mut();
                match begin {
                    Some((bid, pos, col)) if bid == d.id && col == column => {
                        if column {
                            let (l1, c1) = d.line_col(pos.min(d.text().len()));
                            let (l2, c2) = d.line_col(d.cursor);
                            let mut sels = Vec::new();
                            for l in l1.min(l2)..=l1.max(l2) {
                                let s = (d.pos_at_col(l, c1), d.pos_at_col(l, c2));
                                if l == l2 {
                                    sels.insert(0, s);
                                } else {
                                    sels.push(s);
                                }
                            }
                            d.set_all_sels(sels);
                        } else {
                            let c = d.cursor;
                            d.set_sel(pos.min(d.text().len()), c);
                        }
                    }
                    _ => {
                        let b = (d.id, d.cursor, column);
                        self.begin_select = Some(b);
                    }
                }
            }
            Cmd::InsertDateShort | Cmd::InsertDateLong | Cmd::InsertDateCustom => {
                let now = chrono::Local::now();
                let s = match cmd {
                    Cmd::InsertDateShort => now.format("%H:%M %d.%m.%Y").to_string(),
                    Cmd::InsertDateLong => now.format("%H:%M:%S %A, %e. %B %Y").to_string(),
                    _ => format_custom_date(&self.s.date_format, now),
                };
                let d = self.doc_mut();
                let r = d.sel();
                d.replace(r, &s, EditKind::Other);
            }
            Cmd::CopyPath => {
                if let Some(p) = self.doc().path_string() {
                    ctx.copy_text(p);
                }
            }
            Cmd::CopyName => ctx.copy_text(self.doc().title()),
            Cmd::CopyDir => {
                if let Some(d) = self.doc().path.as_ref().and_then(|p| p.parent()) {
                    ctx.copy_text(d.display().to_string());
                }
            }
            Cmd::CopyAllNames | Cmd::CopyAllPaths => {
                let names: Vec<String> = self
                    .all_tab_ids()
                    .iter()
                    .filter_map(|&i| self.doc_by_id(i))
                    .map(|d| if cmd == Cmd::CopyAllPaths { d.path_string().unwrap_or_else(|| d.title()) } else { d.title() })
                    .collect();
                ctx.copy_text(names.join("\n"));
            }
            Cmd::Indent => {
                let u = editor::indent_unit_for(&self.s.tabs_for(self.doc().syntax.as_deref()));
                self.transform(|t, s| textops::indent(t, s, &u))
            }
            Cmd::Unindent => {
                let w = self.s.tabs_for(self.doc().syntax.as_deref()).width;
                self.transform(|t, s| textops::unindent(t, s, w))
            }
            Cmd::Case(c) => {
                let d = self.doc_mut();
                if d.is_multi() {
                    let sels: Vec<_> = d.all_sels().iter().map(|&(a, c)| a.min(c)..a.max(c)).collect();
                    let edits = textops::case_multi(d.text(), &sels, c);
                    d.replace_multi(edits, true);
                } else {
                    self.transform(|t, s| textops::change_case(t, s, c));
                }
            }
            Cmd::LineOp(op) => {
                let w = self.s.tab_width;
                self.transform(|t, s| textops::line_op(t, s, op, w))
            }
            Cmd::Duplicate => self.transform(textops::duplicate),
            Cmd::DeleteLine => self.transform(textops::delete_lines),
            Cmd::SplitLines => {
                let w = if self.s.edge_column > 0 { self.s.edge_column } else { 80 };
                self.transform(|t, s| textops::split_lines(t, s, w))
            }
            Cmd::MoveUp => self.transform(|t, s| textops::move_lines(t, s, true)),
            Cmd::MoveDown => self.transform(|t, s| textops::move_lines(t, s, false)),
            Cmd::BlankAbove => self.transform(|t, s| textops::insert_blank_line(t, s, true)),
            Cmd::BlankBelow => self.transform(|t, s| textops::insert_blank_line(t, s, false)),
            Cmd::ToggleComment | Cmd::SingleComment | Cmd::SingleUncomment | Cmd::BlockComment | Cmd::BlockUncomment => {
                self.comment(cmd);
            }
            Cmd::TabsToSpaces => {
                let w = self.s.tabs_for(self.doc().syntax.as_deref()).width;
                self.transform(|t, s| textops::convert_tabs(t, s, true, w))
            }
            Cmd::SpacesToTabsLeading => {
                let w = self.s.tabs_for(self.doc().syntax.as_deref()).width;
                self.transform(|t, s| textops::convert_tabs(t, s, false, w))
            }
            Cmd::SpacesToTabsAll => {
                let w = self.s.tabs_for(self.doc().syntax.as_deref()).width.max(1);
                // Runs of spaces reaching a tab stop become tabs.
                self.transform(|t, s| {
                    let r = if s.is_empty() { 0..t.len() } else { textops::line_block(t, s) };
                    let mut out = String::new();
                    for (i, line) in t[r.clone()].split('\n').enumerate() {
                        if i > 0 {
                            out.push('\n');
                        }
                        let mut col = 0;
                        let mut spaces = 0;
                        for c in line.chars() {
                            if c == ' ' {
                                spaces += 1;
                                col += 1;
                                if col % w == 0 {
                                    out.push(if spaces > 1 { '\t' } else { ' ' });
                                    spaces = 0;
                                }
                            } else {
                                out.extend(std::iter::repeat_n(' ', spaces));
                                spaces = 0;
                                out.push(c);
                                col = if c == '\t' { (col / w + 1) * w } else { col + 1 };
                            }
                        }
                        out.extend(std::iter::repeat_n(' ', spaces));
                    }
                    let mut nt = t.to_string();
                    nt.replace_range(r.clone(), &out);
                    (nt != t).then(|| (nt, r.start..r.start + out.len()))
                })
            }
            Cmd::MultiSelectAll(m) | Cmd::MultiSelectNext(m) => self.multi_select(m, matches!(cmd, Cmd::MultiSelectAll(_))),
            Cmd::MultiSelectUndo => {
                let d = self.doc_mut();
                if d.is_multi() {
                    let mut s = d.all_sels();
                    s.remove(0);
                    d.set_all_sels(s);
                    d.view.scroll = Some(ScrollTo::Cursor);
                }
            }
            Cmd::MultiSelectSkip => {
                let d = self.doc_mut();
                if d.is_multi() {
                    let mut s = d.all_sels();
                    s.remove(0);
                    d.set_all_sels(s);
                }
                self.multi_select(Match::CaseAndWholeWord, false);
            }
            Cmd::ColumnMode => {
                self.message = Some((
                    tr("Column Mode").into(),
                    tr("Column selection: hold Alt and drag with the mouse, or use Alt+Shift+Arrow keys.\nMultiple carets: Ctrl+click.\nTyping, Backspace, Delete and Paste then act on every line.").into(),
                ))
            }
            Cmd::ColumnEditor => self.dlg.column_editor = Some(Default::default()),
            Cmd::CharPanel => self.s.show_char_panel = !self.s.show_char_panel,
            Cmd::ClipboardHistory => self.s.show_clipboard_history = !self.s.show_clipboard_history,
            Cmd::ToggleReadOnly => {
                let d = self.doc_mut();
                d.read_only = !d.read_only;
            }
            Cmd::ReadOnlyAll | Cmd::ClearReadOnlyAll => {
                for d in &mut self.docs {
                    d.read_only = cmd == Cmd::ReadOnlyAll;
                }
            }
            Cmd::ToggleFileReadOnly => {
                if let Some(p) = self.doc().path.clone() {
                    if let Ok(meta) = std::fs::metadata(&p) {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perm = meta.permissions();
                        let mode = perm.mode();
                        let ro = mode & 0o222 == 0;
                        perm.set_mode(if ro { mode | 0o200 } else { mode & !0o222 });
                        match std::fs::set_permissions(&p, perm) {
                            Ok(()) => self.doc_mut().read_only = !ro,
                            Err(e) => self.message = Some((tr("Read-Only Attribute").into(), e.to_string())),
                        }
                    }
                }
            }
            Cmd::OpenSelectedFile | Cmd::OpenSelectedFolder => {
                let d = self.doc();
                let sel = if d.has_sel() {
                    d.selected_text().trim().to_string()
                } else {
                    // Path-like token under the caret
                    let line = d.line_text(d.line_of(d.cursor));
                    let col = d.cursor - d.line_start(d.line_of(d.cursor));
                    let is_p = |c: char| !c.is_whitespace() && !"\"'<>|()[]{}".contains(c);
                    let s = line[..col].char_indices().rev().take_while(|(_, c)| is_p(*c)).last().map_or(col, |(i, _)| i);
                    let e = line[col..].char_indices().find(|(_, c)| !is_p(*c)).map_or(line.len(), |(i, _)| col + i);
                    line[s..e].to_string()
                };
                let base = d.path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
                let p = match sel.strip_prefix('~') {
                    Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default() + rest),
                    None => base.join(&sel),
                };
                if cmd == Cmd::OpenSelectedFile {
                    if p.is_file() {
                        self.open_path(&p);
                    } else {
                        self.message = Some((tr("Open File").into(), trf("File not found: {}", &[&p.display()])));
                    }
                } else {
                    let dir = if p.is_dir() { p } else { p.parent().map(Path::to_path_buf).unwrap_or(base) };
                    let _ = tools::open_with_default(&dir.display().to_string());
                }
            }
            Cmd::Redact(dot) => {
                let d = self.doc_mut();
                if d.has_sel() {
                    let sels: Vec<_> = d.all_sels().iter().map(|&(a, c)| a.min(c)..a.max(c)).filter(|r| !r.is_empty()).collect();
                    let edits = sels.into_iter().map(|r| (r.clone(), textops::redact(&d.text()[r], dot))).collect();
                    d.replace_multi(edits, true);
                }
            }
            Cmd::SearchInternet => {
                let v = self.run_vars();
                let q = tools::url_encode(&v.word);
                let url = self.s.search_engine.replace("$(CURRENT_WORD)", &q);
                let _ = tools::open_with_default(&url);
            }
            Cmd::ChangeSearchEngine => self.prefs.open_at(crate::ui_prefs::PrefPage::SearchEngine),

            // ---- Search ----------------------------------------------------------------------
            Cmd::Find => self.open_find(FindTab::Find),
            Cmd::Replace => self.open_find(FindTab::Replace),
            Cmd::Mark => self.open_find(FindTab::Mark),
            Cmd::FindInFiles => {
                if self.s.fif_dir.is_empty() {
                    if let Some(d) = self.doc().path.as_ref().and_then(|p| p.parent()) {
                        self.s.fif_dir = d.display().to_string();
                    }
                }
                self.open_find(FindTab::FindInFiles)
            }
            Cmd::FindNext | Cmd::FindPrev => {
                if self.find.what.is_empty() {
                    self.open_find(FindTab::Find);
                } else {
                    self.find_next(cmd == Cmd::FindPrev);
                }
            }
            Cmd::FindSelNext | Cmd::FindSelPrev | Cmd::FindVolatileNext | Cmd::FindVolatilePrev => {
                let back = matches!(cmd, Cmd::FindSelPrev | Cmd::FindVolatilePrev);
                let volatile = matches!(cmd, Cmd::FindVolatileNext | Cmd::FindVolatilePrev);
                let d = self.doc();
                let word = if d.has_sel() {
                    Some(d.selected_text().to_owned())
                } else {
                    editor::word_at(d.text(), d.cursor, &self.s.word_chars).map(|r| d.text()[r].to_owned())
                };
                if let Some(w) = word.filter(|w| !w.contains('\n')) {
                    if !self.doc().has_sel() {
                        if let Some(r) = editor::word_at(self.doc().text(), self.doc().cursor, &self.s.word_chars) {
                            self.doc_mut().set_sel(r.start, r.end);
                        }
                    }
                    let saved_opts = self.s.search.clone();
                    let saved_what = self.find.what.clone();
                    self.s.search.mode = Mode::Normal;
                    self.find.what = w;
                    self.find_next(back);
                    self.s.search = saved_opts;
                    if volatile {
                        self.find.what = saved_what;
                    }
                }
            }
            Cmd::IncrementalSearch => self.inc_search = Some(Default::default()),
            Cmd::ToggleResults => self.show_results = !self.show_results,
            Cmd::NextResult | Cmd::PrevResult => self.goto_result(cmd == Cmd::NextResult),
            Cmd::GotoLine => {
                self.dlg.goto_open = true;
                self.dlg.goto_focus = true;
                self.dlg.goto_input.clear();
            }
            Cmd::GotoBrace => {
                let d = self.doc_mut();
                if let Some((_, m)) = textops::matching_bracket(d.text(), d.cursor) {
                    d.set_sel(m, m);
                    d.view.scroll = Some(ScrollTo::Cursor);
                }
            }
            Cmd::SelectBetweenBraces => {
                let d = self.doc_mut();
                if let Some(r) = textops::between_braces(d.text(), d.cursor) {
                    d.set_sel(r.start, r.end);
                }
            }
            Cmd::StyleAll(n) | Cmd::StyleOne(n) => {
                let extra = self.s.word_chars.clone();
                let d = self.doc_mut();
                let token = if d.has_sel() {
                    Some(d.sel())
                } else {
                    editor::word_at(d.text(), d.cursor, &extra)
                };
                if let Some(r) = token {
                    if matches!(cmd, Cmd::StyleOne(_)) {
                        d.marks.push((r, n));
                    } else {
                        let w = d.text()[r].to_string();
                        let re = Regex::new(&format!(r"\b{}\b", regex::escape(&w))).or_else(|_| Regex::new(&regex::escape(&w)));
                        if let Ok(re) = re {
                            let found: Vec<_> = re.find_iter(d.text()).take(200_000).map(|m| (m.range(), n)).collect();
                            d.marks.extend(found);
                        }
                    }
                }
            }
            Cmd::ClearStyle(n) => {
                let d = self.doc_mut();
                if n == 0 {
                    d.marks.retain(|(_, s)| *s == 0);
                } else {
                    d.marks.retain(|(_, s)| *s != n);
                }
            }
            Cmd::ClearMarks => self.doc_mut().marks.clear(),
            Cmd::JumpStyle(n, down) => {
                let d = self.doc_mut();
                let c = d.cursor;
                let mut ranges: Vec<_> = d.marks.iter().filter(|(_, s)| *s == n).map(|(r, _)| r.clone()).collect();
                ranges.sort_by_key(|r| r.start);
                let target = if down {
                    ranges.iter().find(|r| r.start > c).or(ranges.first())
                } else {
                    ranges.iter().rev().find(|r| r.end < c || (r.start < c && r.end <= d.anchor.min(c))).or(ranges.last())
                };
                if let Some(r) = target.cloned() {
                    d.select(r);
                }
            }
            Cmd::CopyStyled(n) => {
                let d = self.doc();
                let mut ranges: Vec<_> = d
                    .marks
                    .iter()
                    .filter(|(_, s)| match n {
                        0 => *s >= 1,
                        6 => *s == 0,
                        k => *s == k,
                    })
                    .map(|(r, _)| r.clone())
                    .collect();
                ranges.sort_by_key(|r| r.start);
                let text: Vec<&str> = ranges.iter().map(|r| &d.text()[r.clone()]).collect();
                ctx.copy_text(text.join("\n"));
            }
            Cmd::ToggleBookmark => {
                let d = self.doc_mut();
                let l = d.line_of(d.cursor);
                if !d.bookmarks.remove(&l) {
                    d.bookmarks.insert(l);
                }
            }
            Cmd::NextBookmark | Cmd::PrevBookmark => {
                let d = self.doc_mut();
                let l = d.line_of(d.cursor);
                let target = if cmd == Cmd::NextBookmark {
                    d.bookmarks.range(l + 1..).next().or_else(|| d.bookmarks.iter().next())
                } else {
                    d.bookmarks.range(..l).next_back().or_else(|| d.bookmarks.iter().next_back())
                };
                if let Some(&t) = target {
                    d.goto_line(t);
                }
            }
            Cmd::ClearBookmarks => self.doc_mut().bookmarks.clear(),
            Cmd::InverseBookmarks => {
                let d = self.doc_mut();
                let all: std::collections::BTreeSet<usize> = (0..d.line_count()).filter(|l| !d.bookmarks.contains(l)).collect();
                d.bookmarks = all;
            }
            Cmd::CopyBookmarked | Cmd::CutBookmarked | Cmd::RemoveBookmarked | Cmd::RemoveUnbookmarked | Cmd::PasteBookmarked => {
                let clip = if cmd == Cmd::PasteBookmarked { arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok() } else { None };
                let d = self.doc_mut();
                let lines: Vec<&str> = d.text().split('\n').collect();
                let marked: Vec<&str> = lines.iter().enumerate().filter(|(n, _)| d.bookmarks.contains(n)).map(|(_, l)| *l).collect();
                if matches!(cmd, Cmd::CopyBookmarked | Cmd::CutBookmarked) && !marked.is_empty() {
                    ctx.copy_text(marked.join("\n") + "\n");
                }
                if cmd == Cmd::PasteBookmarked {
                    if let Some(c) = clip {
                        let c = c.replace("\r\n", "\n");
                        let new: Vec<String> = lines.iter().enumerate().map(|(n, l)| if d.bookmarks.contains(&n) { c.trim_end_matches('\n').to_string() } else { l.to_string() }).collect();
                        let t = new.join("\n");
                        let cur = d.cursor.min(t.len());
                        d.set_text(t, cur..cur);
                    }
                } else if cmd != Cmd::CopyBookmarked && !d.read_only {
                    let keep_marked = cmd == Cmd::RemoveUnbookmarked;
                    let kept: Vec<&str> = lines.iter().enumerate().filter(|(n, _)| d.bookmarks.contains(n) == keep_marked).map(|(_, l)| *l).collect();
                    let new = kept.join("\n");
                    d.set_text(new, 0..0);
                    d.bookmarks.clear();
                }
            }
            Cmd::FindCharsInRange => self.dlg.char_range = Some(Default::default()),
            Cmd::NextChange | Cmd::PrevChange => {
                let d = self.doc_mut();
                let l = d.line_of(d.cursor);
                let n = d.line_count();
                let changed = |d: &crate::doc::Document, x: usize| d.line_change(x) != crate::doc::CHANGE_NONE;
                // Skip the block the caret is in.
                let target = if cmd == Cmd::NextChange {
                    let mut x = l;
                    while x < n && changed(d, x) {
                        x += 1;
                    }
                    (x..n).find(|&x| changed(d, x))
                } else {
                    let mut x = l as isize;
                    while x >= 0 && changed(d, x as usize) {
                        x -= 1;
                    }
                    (0..(x.max(0) as usize)).rev().find(|&x| changed(d, x)).map(|mut x| {
                        while x > 0 && changed(d, x - 1) {
                            x -= 1;
                        }
                        x
                    })
                };
                if let Some(t) = target {
                    d.goto_line(t);
                }
            }
            Cmd::ClearChangeHistory => self.doc_mut().clear_change_history(),

            // ---- View ------------------------------------------------------------------------
            Cmd::ToggleAlwaysOnTop => {
                self.always_on_top = !self.always_on_top;
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal }));
            }
            Cmd::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
            }
            Cmd::PostIt => {
                self.postit = !self.postit;
                ctx.send_viewport_cmd(ViewportCommand::Decorations(!self.postit));
            }
            Cmd::DistractionFree => {
                self.distraction_free = !self.distraction_free;
                self.fullscreen = self.distraction_free;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
            }
            Cmd::ToggleWhitespace => self.s.show_whitespace = !self.s.show_whitespace,
            Cmd::ToggleEol => self.s.show_eol = !self.s.show_eol,
            Cmd::ToggleShowAll => {
                let on = !(self.s.show_whitespace && self.s.show_eol);
                self.s.show_whitespace = on;
                self.s.show_eol = on;
            }
            Cmd::ToggleNonPrinting => self.s.show_non_printing = !self.s.show_non_printing,
            Cmd::ToggleCtrlChars => self.s.show_ctrl_chars = !self.s.show_ctrl_chars,
            Cmd::ToggleIndentGuides => self.s.show_indent_guides = !self.s.show_indent_guides,
            Cmd::ToggleWrapSymbol => self.s.show_wrap_symbol = !self.s.show_wrap_symbol,
            Cmd::ToggleWrap => self.s.word_wrap = !self.s.word_wrap,
            Cmd::ZoomIn => self.s.font_size = (self.s.font_size + 1.0).min(72.0),
            Cmd::ZoomOut => self.s.font_size = (self.s.font_size - 1.0).max(6.0),
            Cmd::ZoomReset => self.s.font_size = 14.0,
            Cmd::MoveToOtherView | Cmd::CloneToOtherView => {
                let other = 1 - pane;
                if self.panes[other].tabs.contains(&id) {
                    self.activate_id(id);
                    self.cur = other;
                    self.panes[other].active = self.panes[other].tabs.iter().position(|&t| t == id).unwrap();
                    return;
                }
                if cmd == Cmd::CloneToOtherView {
                    // The clone gets its own selection/scroll state.
                    let d = self.doc_mut();
                    let (c, a, v) = (d.cursor, d.anchor, d.view.clone());
                    d.alt = Some(Box::new(crate::doc::ViewState { cursor: c, anchor: a, extra: Vec::new(), rect: None, view: v }));
                    self.add_tab(other, id);
                } else {
                    if self.panes[pane].tabs.len() == 1 && pane == 0 && self.panes[1].tabs.is_empty() {
                        // Moving the only document: keep a new empty one in the main view.
                        self.panes[0].tabs.clear();
                        self.add_tab(1, id);
                        self.cur = 0;
                        self.new_doc();
                        self.cur = 1;
                        return;
                    }
                    let p = &mut self.panes[pane];
                    let i = p.active;
                    p.tabs.remove(i);
                    p.active = p.active.min(p.tabs.len().saturating_sub(1));
                    self.add_tab(other, id);
                    self.fix_panes();
                }
            }
            Cmd::OpenInNewInstance | Cmd::MoveToNewInstance => {
                let Some(p) = self.doc().path_string() else { return };
                if cmd == Cmd::MoveToNewInstance && self.doc().is_dirty() {
                    self.save(id);
                }
                let exe = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from).or_else(|| std::env::current_exe().ok());
                if let Some(exe) = exe {
                    let _ = std::process::Command::new(exe).arg("--new-instance").arg("-nosession").arg(&p).spawn();
                }
                if cmd == Cmd::MoveToNewInstance {
                    self.request_close(vec![id]);
                }
            }
            Cmd::FocusOtherView => {
                if !self.panes[1 - pane].tabs.is_empty() {
                    self.cur = 1 - pane;
                    self.focus_editor = true;
                }
            }
            Cmd::SyncVertical => self.sync_v = !self.sync_v,
            Cmd::SyncHorizontal => self.sync_h = !self.sync_h,
            Cmd::TabN(n) => self.activate_tab(pane, n as usize),
            Cmd::FirstTab => self.activate_tab(pane, 0),
            Cmd::LastTab => {
                let n = self.panes[pane].tabs.len();
                self.activate_tab(pane, n - 1);
            }
            Cmd::NextTab | Cmd::PrevTab => {
                let n = self.panes[pane].tabs.len();
                let i = self.panes[pane].active;
                self.activate_tab(pane, if cmd == Cmd::NextTab { (i + 1) % n } else { (i + n - 1) % n });
            }
            Cmd::MoveTabForward | Cmd::MoveTabBackward => {
                let p = &mut self.panes[pane];
                let i = p.active;
                let j = if cmd == Cmd::MoveTabForward { i + 1 } else { i.wrapping_sub(1) };
                if j < p.tabs.len() {
                    p.tabs.swap(i, j);
                    p.active = j;
                }
            }
            Cmd::TabColor(c) => self.doc_mut().tab_color = c,
            Cmd::TogglePin => {
                let d = self.doc_mut();
                d.pinned = !d.pinned;
                // Pinned tabs go to the front.
                let p = &mut self.panes[pane];
                let pinned: Vec<u64> = p.tabs.iter().copied().filter(|&t| self.docs.iter().any(|d| d.id == t && d.pinned)).collect();
                let rest: Vec<u64> = p.tabs.iter().copied().filter(|t| !pinned.contains(t)).collect();
                p.tabs = pinned.into_iter().chain(rest).collect();
                p.active = p.tabs.iter().position(|&t| t == id).unwrap_or(0);
            }
            Cmd::FoldAll => self.doc_mut().fold_all(true, None),
            Cmd::UnfoldAll => self.doc_mut().fold_all(false, None),
            Cmd::FoldCurrent | Cmd::UnfoldCurrent => {
                let d = self.doc_mut();
                let l = d.line_of(d.cursor);
                if let Some(h) = d.enclosing_fold(l) {
                    if (cmd == Cmd::FoldCurrent) != d.folded.contains(&h) {
                        d.toggle_fold(h);
                    }
                }
            }
            Cmd::FoldLevel(n) => self.doc_mut().fold_all(true, Some(n as usize)),
            Cmd::UnfoldLevel(n) => self.doc_mut().fold_all(false, Some(n as usize)),
            Cmd::HideLines => {
                let d = self.doc_mut();
                let s = d.sel();
                let (l1, l2) = (d.line_of(s.start), d.line_of(s.end));
                if l2 > l1 {
                    d.hide_lines(l1..l2 + 1);
                } else {
                    d.hide_lines(l1..l1 + 1);
                }
            }
            Cmd::Summary => self.dlg.summary = true,
            Cmd::ProjectPanel(n) => {
                let i = (n as usize - 1).min(2);
                self.s.show_project[i] = !self.s.show_project[i];
            }
            Cmd::FolderWorkspace => self.s.show_workspace = !self.s.show_workspace,
            Cmd::DocMap => self.s.show_doc_map = !self.s.show_doc_map,
            Cmd::DocList => self.s.show_doc_list = !self.s.show_doc_list,
            Cmd::FunctionList => self.s.show_function_list = !self.s.show_function_list,
            Cmd::Monitoring => {
                let d = self.doc_mut();
                if d.path.is_some() {
                    d.monitoring = !d.monitoring;
                    d.read_only = d.monitoring;
                    if d.monitoring {
                        let n = d.text().len();
                        d.select(n..n);
                    }
                }
            }
            Cmd::WindowsDialog => self.dlg.windows = Some(Default::default()),
            Cmd::SortTabs(k) => self.sort_tabs(self.cur, k),

            // ---- Encoding / language ---------------------------------------------------------
            Cmd::Reinterpret(enc) => {
                let d = self.doc_mut();
                if d.path.is_none() || d.disk_missing {
                    d.encoding = enc;
                } else if d.is_dirty() {
                    d.encoding = enc;
                    self.confirms.push_back(Confirm::ReloadDiscard(id));
                } else {
                    let eol = self.s.default_eol;
                    if let Err(e) = self.doc_mut().load_from_disk(Some(enc), eol) {
                        self.message = Some((tr("Encoding").into(), e));
                    }
                }
            }
            Cmd::ConvertTo(enc) => {
                let d = self.doc_mut();
                if d.encoding != enc {
                    d.encoding = enc;
                    d.force_dirty();
                }
            }
            Cmd::SetEol(eol) => self.doc_mut().set_eol(eol),
            Cmd::SetSyntax(s) => {
                let d = self.doc_mut();
                d.syntax = s;
                d.user_lang = None;
            }
            Cmd::SetUserLang(n) => self.doc_mut().user_lang = Some(n),
            Cmd::AutoSyntax => {
                let d = self.doc();
                let udl = d.path.as_deref().and_then(|p| self.find_udl_for(p));
                let s = self.hl.detect(d.path.as_deref(), &d.text()[..d.text().len().min(4096)]);
                let d = self.doc_mut();
                d.user_lang = udl;
                d.syntax = s;
            }
            Cmd::DefineLanguage => self.prefs.udl_open = true,
            Cmd::OpenUdlFolder => {
                let _ = std::fs::create_dir_all(crate::udl::dir());
                let _ = tools::open_with_default(&crate::udl::dir().display().to_string());
            }
            Cmd::ImportTheme => self.pick(ctx, Pick::ImportTheme, false, false, None, Some(("TextMate theme", vec!["tmTheme"]))),
            Cmd::ImportSyntax => self.pick(ctx, Pick::ImportSyntax, false, false, None, Some(("Sublime syntax", vec!["sublime-syntax"]))),

            // ---- Settings --------------------------------------------------------------------
            Cmd::Preferences => self.prefs.open_at(crate::ui_prefs::PrefPage::General),
            Cmd::StyleConfigurator => self.prefs.style_open = true,
            Cmd::ShortcutMapper => self.prefs.shortcuts_open = true,
            Cmd::EditContextMenu => self.prefs.open_at(crate::ui_prefs::PrefPage::ContextMenu),

            // ---- Tools -----------------------------------------------------------------------
            Cmd::HashText(h) => self.dlg.hash = Some(crate::ui_dialogs::HashDlg { kind: h, ..Default::default() }),
            Cmd::HashFiles(h) => self.pick(ctx, Pick::HashFiles(h), false, false, None, None),
            Cmd::HashSelection(h) => {
                let d = self.doc();
                let mut out = Vec::new();
                for (a, c) in d.all_sels() {
                    if a != c {
                        out.push(tools::hash_bytes(h, &d.text().as_bytes()[a.min(c)..a.max(c)]));
                    }
                }
                if out.is_empty() {
                    out.push(tools::hash_bytes(h, d.text().as_bytes()));
                }
                ctx.copy_text(out.join("\n"));
            }

            // ---- Macro -----------------------------------------------------------------------
            Cmd::MacroRecord => {
                if self.macro_.recording.is_some() {
                    self.run_inner(ctx, Cmd::MacroStop);
                } else {
                    self.macro_.recording = Some(Vec::new());
                    self.focus_editor = true;
                }
            }
            Cmd::MacroStop => {
                if let Some(ev) = self.macro_.recording.take() {
                    self.macro_.last = ev;
                }
            }
            Cmd::MacroPlay => {
                if self.macro_.recording.is_none() {
                    let chunks = crate::app::macro_chunks(&self.macro_.last);
                    self.macro_.queue.extend(chunks);
                }
            }
            Cmd::MacroSave => {
                if !self.macro_.last.is_empty() {
                    self.dlg.save_macro = Some((String::new(), String::new()));
                }
            }
            Cmd::MacroRunMulti => self.dlg.macro_multi = Some(Default::default()),
            Cmd::MacroRun(i) => {
                if let Some(m) = self.s.macros.get(i) {
                    let chunks = crate::app::macro_chunks(&m.events);
                    self.macro_.queue.extend(chunks);
                }
            }
            Cmd::MacroManage => self.prefs.open_at(crate::ui_prefs::PrefPage::Macros),

            // ---- Run -------------------------------------------------------------------------
            Cmd::RunDialog => self.dlg.run = Some(Default::default()),
            Cmd::RunCommand(i) => {
                if let Some(rc) = self.s.run_commands.get(i) {
                    let c = tools::expand_vars(&rc.command, &self.run_vars());
                    let dir = self.doc().path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
                    if let Err(e) = tools::spawn_shell(&c, dir.as_deref()) {
                        self.message = Some((tr("Run").into(), e));
                    }
                }
            }
            Cmd::RunManage => self.prefs.open_at(crate::ui_prefs::PrefPage::RunCommands),

            // ---- Plugins ---------------------------------------------------------------------
            Cmd::Compare => self.compare(),
            Cmd::ExportHtml | Cmd::ExportRtf => {
                let ext = if cmd == Cmd::ExportHtml { "html" } else { "rtf" };
                let name = format!("{}.{ext}", self.doc().title());
                let what = if cmd == Cmd::ExportHtml { Pick::ExportHtml } else { Pick::ExportRtf };
                self.pick(ctx, what, false, true, Some(name), Some((ext, vec![ext])));
            }
            Cmd::CopyHtml => {
                let html = self.export(true);
                let plain = self.export_plain();
                if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_html(html, Some(plain))) {
                    self.message = Some((tr("Error").into(), e.to_string()));
                }
            }
            Cmd::CopyRtf => {
                let rtf = self.export(false);
                if let Err(e) = tools::clipboard_set_mime("text/rtf", &rtf) {
                    self.message = Some((tr("Error").into(), e));
                }
            }
            Cmd::PasteHtml | Cmd::PasteRtf => {
                let r = if cmd == Cmd::PasteHtml {
                    arboard::Clipboard::new().and_then(|mut c| c.get().html()).map_err(|e| e.to_string())
                } else {
                    tools::clipboard_get_mime("text/rtf").or_else(|_| tools::clipboard_get_mime("application/rtf"))
                };
                match r {
                    Ok(t) => self.inject(ctx, Event::Paste(t)),
                    Err(e) => self.message = Some((tr("Paste Special").into(), e)),
                }
            }
            Cmd::CompareClear => {
                for d in &mut self.docs {
                    d.diff_marks.clear();
                }
            }
            Cmd::Base64Encode => self.convert_selection(|s| Ok(tools::base64_encode(s))),
            Cmd::Base64Decode => self.convert_selection(tools::base64_decode),
            Cmd::UrlEncode => self.convert_selection(|s| Ok(tools::url_encode(s))),
            Cmd::UrlDecode => self.convert_selection(|s| Ok(tools::url_decode(s))),
            Cmd::QuotedPrintableEncode => self.convert_selection(|s| Ok(tools::qp_encode(s))),
            Cmd::QuotedPrintableDecode => self.convert_selection(|s| Ok(tools::qp_decode(s))),
            Cmd::JsonFormat => {
                let ind = editor::indent_unit_for(&self.s.tabs_for(self.doc().syntax.as_deref()));
                self.convert_selection(|s| tools::json_format(s, &ind));
                if self.doc().syntax.is_none() {
                    self.doc_mut().syntax = Some("JSON".into());
                }
            }
            Cmd::JsonMinify => self.convert_selection(tools::json_minify),
            Cmd::XmlFormat => {
                let ind = editor::indent_unit_for(&self.s.tabs_for(self.doc().syntax.as_deref()));
                self.convert_selection(|s| tools::xml_format(s, &ind))
            }
            Cmd::XmlMinify => self.convert_selection(tools::xml_minify),
            Cmd::XmlCheck => {
                let r = tools::xml_check(self.doc().text());
                self.message = Some((tr("Check XML syntax").into(), r.map_or_else(|e| e, |_| tr("No error detected.").into())));
            }
            Cmd::AsciiToHex => self.convert_selection(|s| Ok(tools::ascii_to_hex(s))),
            Cmd::HexToAscii => self.convert_selection(tools::hex_to_ascii),
            Cmd::ScriptPlugin(path) => self.run_script_plugin(&path),
            Cmd::OpenPluginsFolder => {
                let _ = std::fs::create_dir_all(tools::plugins_dir());
                let _ = tools::open_with_default(&tools::plugins_dir().display().to_string());
            }

            // ---- Help ------------------------------------------------------------------------
            Cmd::About => self.dlg.about = true,
            Cmd::CommandLineHelp => self.dlg.cmdline = true,
            Cmd::CheckUpdate => self.start_update_check(ctx, true),
            Cmd::UpdaterProxy => self.dlg.update_proxy = Some(self.s.update_proxy.clone()),
            Cmd::DebugInfo => self.dlg.debug = true,
            Cmd::OpenUrl(u) => {
                let u = if u.starts_with("www.") { format!("https://{u}") } else { u };
                let _ = tools::open_with_default(&u);
            }
        }
    }

    fn comment(&mut self, cmd: Cmd) {
        let udl = self.doc().user_lang.as_ref().and_then(|n| self.udls.iter().find(|u| &u.name == n)).cloned();
        let (line, block): (Option<String>, Option<(String, String)>) = match &udl {
            Some(u) => (u.line_comment.first().cloned(), u.block_comment.first().cloned()),
            None => {
                let (l, b) = highlight::comment_tokens(self.doc().syntax.as_deref());
                (l.map(str::to_owned), b.map(|(a, c)| (a.to_owned(), c.to_owned())))
            }
        };
        match cmd {
            Cmd::ToggleComment | Cmd::SingleComment | Cmd::SingleUncomment => {
                if let Some(tok) = line {
                    let d = self.doc();
                    let block_r = textops::line_block(d.text(), &d.sel());
                    let lines = &d.text()[block_r];
                    let all = lines.lines().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with(tok.trim_end()));
                    let act = match cmd {
                        Cmd::SingleComment => !all || true,
                        Cmd::SingleUncomment => all,
                        _ => true,
                    };
                    if cmd == Cmd::SingleComment && all {
                        // Comment again (nested), like Notepad++
                        let t = tok.to_string();
                        self.transform(|text, s| {
                            let b = textops::line_block(text, s);
                            let new: Vec<String> = text[b.clone()].split('\n').map(|l| if l.trim().is_empty() { l.to_string() } else { format!("{t} {l}") }).collect();
                            let n = new.join("\n");
                            let mut out = text.to_string();
                            out.replace_range(b.clone(), &n);
                            Some((out, b.start..b.start + n.len()))
                        });
                    } else if act {
                        self.transform(|t, s| textops::toggle_line_comment(t, s, &tok));
                    }
                } else if let Some((o, c)) = block {
                    self.transform(|t, s| textops::toggle_block_comment(t, s, &o, &c));
                }
            }
            Cmd::BlockComment | Cmd::BlockUncomment => {
                if let Some((o, c)) = block {
                    let d = self.doc();
                    let r = if d.has_sel() { d.sel() } else { textops::line_block(d.text(), &d.sel()) };
                    let t = d.text()[r].trim();
                    let is = t.starts_with(&o) && t.ends_with(&c);
                    if (cmd == Cmd::BlockComment) != is {
                        self.transform(|t, s| textops::toggle_block_comment(t, s, &o, &c));
                    }
                } else if let Some(tok) = line {
                    self.transform(|t, s| textops::toggle_line_comment(t, s, &tok));
                }
            }
            _ => {}
        }
    }

    fn multi_select(&mut self, m: Match, all: bool) {
        let extra = self.s.word_chars.clone();
        let d = self.doc_mut();
        if !d.has_sel() {
            if let Some(r) = editor::word_at(d.text(), d.cursor, &extra) {
                d.set_sel(r.start, r.end);
            }
            if !all {
                return;
            }
        }
        let word = d.selected_text().to_string();
        if word.is_empty() || word.contains('\n') {
            return;
        }
        let case = matches!(m, Match::CaseOnly | Match::CaseAndWholeWord);
        let whole = matches!(m, Match::IgnoreCaseWholeWord | Match::WholeWordOnly | Match::CaseAndWholeWord);
        let pat = if whole { format!(r"\b{}\b", regex::escape(&word)) } else { regex::escape(&word) };
        let Ok(re) = regex::RegexBuilder::new(&pat).case_insensitive(!case).build() else { return };
        if all {
            let mut sels: Vec<(usize, usize)> = re.find_iter(d.text()).take(10_000).map(|x| (x.start(), x.end())).collect();
            // Keep the current one primary.
            let cur = d.sel();
            if let Some(i) = sels.iter().position(|&(a, _)| a == cur.start) {
                let s = sels.remove(i);
                sels.insert(0, s);
            }
            d.set_all_sels(sels);
        } else {
            let after = d.all_sels().iter().map(|&(a, c)| a.max(c)).max().unwrap_or(0);
            let next = re.find_at(d.text(), after).or_else(|| re.find(d.text())).map(|x| (x.start(), x.end()));
            if let Some(n) = next {
                if !d.all_sels().iter().any(|&(a, c)| a.min(c) == n.0) {
                    let mut sels = vec![n];
                    sels.extend(d.all_sels());
                    d.set_all_sels(sels);
                    d.view.scroll = Some(ScrollTo::Cursor);
                }
            }
        }
    }

    fn compare(&mut self) {
        // Compare the documents of the two views, or the active and the previous tab.
        let (a, b) = match (self.panes[0].active_id(), self.panes[1].active_id()) {
            (Some(a), Some(b)) if a != b => (a, b),
            _ => {
                let p = &self.panes[self.cur];
                if p.tabs.len() < 2 {
                    self.message = Some((tr("Compare").into(), tr("Open two documents to compare.").into()));
                    return;
                }
                let a = p.tabs[if p.active == 0 { 1 } else { p.active - 1 }];
                let b = p.tabs[p.active];
                // Show them side by side: b goes to the second view.
                let pane = self.cur;
                let p = &mut self.panes[pane];
                p.tabs.retain(|&t| t != b);
                p.active = p.tabs.iter().position(|&t| t == a).unwrap_or(0);
                if pane == 1 {
                    self.panes[0].tabs.push(b);
                    self.panes[0].active = self.panes[0].tabs.len() - 1;
                    (b, a)
                } else {
                    self.add_tab(1, b);
                    (a, b)
                }
            }
        };
        let (ta, tb) = (self.doc_by_id(a).unwrap().text().to_string(), self.doc_by_id(b).unwrap().text().to_string());
        let (ma, mb, n) = tools::compare(&ta, &tb);
        self.doc_by_id_mut(a).unwrap().diff_marks = ma;
        self.doc_by_id_mut(b).unwrap().diff_marks = mb;
        self.sync_v = true;
        self.find.status = None;
        if n == 0 {
            self.message = Some((tr("Compare").into(), tr("Files match.").into()));
        }
    }

    fn run_script_plugin(&mut self, path: &str) {
        let d = self.doc();
        let r = if d.has_sel() { d.sel() } else { 0..d.text().len() };
        let input = d.text()[r.clone()].to_string();
        let env = vec![
            ("NRS_FILE", d.path_string().unwrap_or_default()),
            ("NRS_LANGUAGE", d.syntax.clone().unwrap_or_default()),
            ("NRS_SELECTION", if d.has_sel() { "1".into() } else { "0".into() }),
            ("NRS_LINE", (d.line_of(d.cursor) + 1).to_string()),
        ];
        match tools::run_script(path, &input, &env) {
            Ok(Some(out)) => {
                let d = self.doc_mut();
                let mut t = d.text().to_string();
                t.replace_range(r.clone(), &out);
                d.set_text(t, r.start..r.start + out.len());
            }
            Ok(None) => {}
            Err(e) => self.message = Some((tr("Plugin").into(), e)),
        }
    }

    pub fn print_postscript(&mut self) -> String {
        let id = self.active_id();
        let theme = self.hl.theme(&self.s.theme_light.clone()).clone();
        let i = self.idx(id);
        let syntax = self.docs[i].syntax.clone();
        // Highlight everything (with the light theme for paper).
        let mut cache = crate::highlight::HlCache::default();
        let d = &self.docs[i];
        let starts: Vec<usize> = std::iter::once(0).chain(d.text().match_indices('\n').map(|(i, _)| i + 1)).collect();
        if let (Some(syn), true) = (syntax.as_deref(), self.s.print_colors) {
            cache.ensure(&self.hl, d.text(), &starts, syn, &self.s.theme_light, usize::MAX, std::time::Instant::now() + std::time::Duration::from_secs(30));
        }
        let fg = crate::highlight::theme_colors(&theme).fg;
        let mut lines = Vec::new();
        for l in 0..d.line_count() {
            let text = d.line_text(l);
            let mut runs = Vec::new();
            match cache.line_spans(l) {
                Some(spans) => {
                    let mut pos = 0;
                    for (len, st) in spans {
                        let end = (pos + len).min(text.len());
                        if end > pos {
                            runs.push((text[pos..end].to_string(), [st.foreground.r, st.foreground.g, st.foreground.b]));
                        }
                        pos += len;
                    }
                }
                None => runs.push((text.to_string(), if self.s.print_colors { [fg.r(), fg.g(), fg.b()] } else { [0, 0, 0] })),
            }
            lines.push(runs);
        }
        let header = tools::expand_vars(&self.s.print_header, &self.run_vars());
        tools::postscript(&lines, &header, &self.s.print_footer, self.s.print_font_size, self.s.print_line_numbers)
    }

    /// Window > Sort By for the tabs of one view (the active document stays active).
    pub fn sort_tabs(&mut self, pane: usize, k: u8) {
        let active = self.panes[pane].active_id();
        let mut keyed: Vec<(u64, (String, u128, u64))> = self.panes[pane]
            .tabs
            .iter()
            .filter_map(|&id| {
                let d = self.doc_by_id(id)?;
                let path = d.path.as_ref().map_or(d.title(), |p| p.display().to_string());
                let ext = d.path.as_ref().and_then(|p| p.extension()).map_or(String::new(), |e| e.to_string_lossy().to_lowercase());
                let mtime = d.disk_mtime.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |t| t.as_nanos());
                let key = match k / 2 {
                    0 => d.title().to_lowercase(),
                    1 => path.to_lowercase(),
                    2 => format!("{ext}\u{0}{}", d.title().to_lowercase()),
                    _ => String::new(),
                };
                Some((id, (key, mtime, d.text().len() as u64)))
            })
            .collect();
        keyed.sort_by(|a, b| match k / 2 {
            3 => a.1 .2.cmp(&b.1 .2),
            4 => a.1 .1.cmp(&b.1 .1),
            _ => a.1 .0.cmp(&b.1 .0),
        });
        if k % 2 == 1 {
            keyed.reverse();
        }
        let p = &mut self.panes[pane];
        p.tabs = keyed.into_iter().map(|(id, _)| id).collect();
        if let Some(a) = active {
            p.active = p.tabs.iter().position(|&t| t == a).unwrap_or(0);
        }
    }

    /// Selected text (or the whole document) as styled runs with the current theme.
    fn export_runs(&self) -> (Vec<Vec<tools::ExportRun>>, [u8; 3], [u8; 3]) {
        let d = self.doc();
        let theme = self.hl.theme(self.s.theme());
        let colors = crate::highlight::theme_colors(theme);
        let rgb = |c: egui::Color32| [c.r(), c.g(), c.b()];
        let (fg, bg) = (rgb(colors.fg), rgb(colors.bg));
        let range = if d.has_sel() { d.sel() } else { 0..d.text().len() };
        let (l0, l1) = (d.line_of(range.start), d.line_of(range.end));
        let mut cache = crate::highlight::HlCache::default();
        let starts: Vec<usize> = std::iter::once(0).chain(d.text().match_indices('\n').map(|(i, _)| i + 1)).collect();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        if let Some(u) = d.user_lang.as_ref().and_then(|n| self.udls.iter().find(|u| &u.name == n)) {
            cache.ensure_udl(u, d.text(), &starts, l1 + 1, deadline);
        } else if let Some(syn) = d.syntax.as_deref() {
            cache.ensure(&self.hl, d.text(), &starts, syn, self.s.theme(), l1 + 1, deadline);
        }
        let mut lines = Vec::new();
        for l in l0..=l1 {
            let ls = d.line_start(l);
            let text = d.line_text(l);
            let (from, to) = (range.start.saturating_sub(ls).min(text.len()), (range.end - ls.min(range.end)).min(text.len()));
            let mut runs = Vec::new();
            let mut pos = 0;
            let spans: Vec<(usize, Option<syntect::highlighting::Style>)> = match cache.line_spans(l) {
                Some(sp) => sp.iter().map(|(n, st)| (*n, Some(*st))).collect(),
                None => vec![(text.len(), None)],
            };
            for (len, st) in spans {
                let (a, b) = (pos.max(from), (pos + len).min(to));
                if b > a && text.is_char_boundary(a) && text.is_char_boundary(b) {
                    let (c, bold, it) = match st {
                        Some(st) => (
                            [st.foreground.r, st.foreground.g, st.foreground.b],
                            st.font_style.contains(syntect::highlighting::FontStyle::BOLD),
                            st.font_style.contains(syntect::highlighting::FontStyle::ITALIC),
                        ),
                        None => (fg, false, false),
                    };
                    runs.push((text[a..b].to_string(), c, bold, it));
                }
                pos += len;
            }
            lines.push(runs);
        }
        (lines, fg, bg)
    }

    fn export_plain(&self) -> String {
        let d = self.doc();
        if d.has_sel() { d.selected_text().to_string() } else { d.text().to_string() }
    }

    /// HTML (`html` = true) or RTF of the selection / document.
    pub fn export(&self, html: bool) -> String {
        let (lines, fg, bg) = self.export_runs();
        let font = self.s.font_path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map_or("Hack".to_string(), |s| s.to_string_lossy().into_owned());
        let size = self.s.font_size * 0.75;
        if html {
            let tab = self.s.tabs_for(self.doc().syntax.as_deref()).width;
            tools::to_html(&lines, fg, bg, &font, size, &self.doc().title(), tab)
        } else {
            tools::to_rtf(&lines, fg, bg, &font, size)
        }
    }

    pub fn handle_pick(&mut self, ctx: &egui::Context, what: Pick, paths: Vec<PathBuf>) {
        let _ = ctx;
        let Some(first) = paths.first().cloned() else { return };
        match what {
            Pick::WorkspaceFolder => {
                let p = first.display().to_string();
                if !self.s.workspace_roots.contains(&p) {
                    self.s.workspace_roots.push(p);
                }
                self.s.show_workspace = true;
            }
            Pick::LoadSession => self.load_session_file(&first),
            Pick::ExportHtml | Pick::ExportRtf => {
                let data = self.export(what == Pick::ExportHtml);
                if let Err(e) = std::fs::write(&first, data) {
                    self.message = Some((tr("Error").into(), format!("{}: {e}", first.display())));
                }
            }
            Pick::SaveSession => {
                if let Err(e) = self.session().save_to(&first) {
                    self.message = Some((tr("Save Session").into(), e));
                }
            }
            Pick::HashFiles(h) => {
                let mut out = String::new();
                for p in &paths {
                    match std::fs::read(p) {
                        Ok(b) => out.push_str(&format!("{}  {}\n", tools::hash_bytes(h, &b), p.display())),
                        Err(e) => out.push_str(&format!("{e}  {}\n", p.display())),
                    }
                }
                self.dlg.hash = Some(crate::ui_dialogs::HashDlg { kind: h, input: String::new(), output: out, files: true });
            }
            Pick::FifDir => self.s.fif_dir = first.display().to_string(),
            Pick::ImportTheme | Pick::ImportSyntax => {
                let sub = if what == Pick::ImportTheme { "themes" } else { "syntaxes" };
                let dir = crate::settings::config_dir().join(sub);
                let _ = std::fs::create_dir_all(&dir);
                let mut n = 0;
                for p in &paths {
                    if let Some(name) = p.file_name() {
                        if std::fs::copy(p, dir.join(name)).is_ok() {
                            n += 1;
                        }
                    }
                }
                self.hl = crate::highlight::Highlighting::new();
                self.keywords_cache = Default::default();
                for d in &mut self.docs {
                    d.hl.invalidate_from(0);
                }
                self.message = Some((tr("Import").into(), trf("{} file(s) imported.", &[&n])));
            }
            Pick::UdlImport => {
                let mut n = 0;
                for p in &paths {
                    if let Ok(s) = std::fs::read_to_string(p) {
                        match crate::udl::UserLang::from_xml(&s) {
                            Ok(langs) => {
                                for mut l in langs {
                                    l.file = None;
                                    let _ = crate::udl::save(&mut l);
                                    self.udls.retain(|u| u.name != l.name);
                                    self.udls.push(l);
                                    n += 1;
                                }
                            }
                            Err(e) => self.message = Some((tr("Import").into(), e)),
                        }
                    }
                }
                if n > 0 {
                    self.message = Some((tr("Import").into(), trf("{} language(s) imported.", &[&n])));
                }
            }
            Pick::UdlExport(i) => {
                if let Some(l) = self.udls.get(i) {
                    if let Err(e) = std::fs::write(&first, l.to_xml()) {
                        self.message = Some((tr("Export").into(), e.to_string()));
                    }
                }
            }
            Pick::ProjectOpen(i) => match crate::project::Workspace::load(&first) {
                Ok(w) => {
                    self.s.project_files[i] = first.display().to_string();
                    self.projects[i] = Some(w);
                }
                Err(e) => self.message = Some((tr("Project").into(), e)),
            },
            Pick::ProjectSaveAs(i) => {
                if let Some(w) = &mut self.projects[i] {
                    w.path = Some(first.clone());
                    match w.save() {
                        Ok(()) => self.s.project_files[i] = first.display().to_string(),
                        Err(e) => self.message = Some((tr("Project").into(), e)),
                    }
                }
            }
            Pick::ProjectAddFiles(i, at) => {
                if let Some(w) = &mut self.projects[i] {
                    if let Some(crate::project::Node::Folder { children, .. }) = w.node_mut(&at) {
                        children.extend(paths.into_iter().map(|p| crate::project::Node::File { path: p }));
                        w.dirty = true;
                    }
                }
            }
            Pick::ProjectAddDir(i, at) => {
                if let Some(w) = &mut self.projects[i] {
                    if let Some(crate::project::Node::Folder { children, .. }) = w.node_mut(&at) {
                        children.push(crate::project::folder_from_dir(&first));
                        w.dirty = true;
                    }
                }
            }
            Pick::PrintToFile => {
                let ps = self.print_postscript();
                if let Err(e) = std::fs::write(&first, ps) {
                    self.message = Some((tr("Print").into(), e.to_string()));
                }
            }
            Pick::BackupDir => self.s.backup_dir = first.display().to_string(),
            Pick::DefaultDir => self.s.custom_dir = first.display().to_string(),
        }
    }

    // ---- search --------------------------------------------------------------------------------

    pub fn regex(&mut self) -> Option<Regex> {
        match search::build_regex(&self.find.what, &self.s.search) {
            Ok(r) => {
                push_history(&mut self.s.find_history, &self.find.what);
                Some(r)
            }
            Err(e) => {
                self.find.status = Some((e, true));
                None
            }
        }
    }

    pub fn find_next(&mut self, backward: bool) -> bool {
        let Some(re) = self.regex() else { return false };
        let wrap = self.s.search.wrap;
        let what = self.find.what.clone();
        let doc = self.doc_mut();
        let sel = doc.sel();
        let from = if backward { sel.start } else { sel.end };
        match search::find(doc.text(), &re, from, !backward, wrap) {
            Some((r, wrapped)) => {
                doc.select(r);
                self.find.status = wrapped.then(|| {
                    let msg = if backward { tr("Find: reached the top of the document, continued from the end.") } else { tr("Find: reached the end of the document, continued from the top.") };
                    (msg.to_string(), false)
                });
                true
            }
            None => {
                self.find.status = Some((trf("Find: Can't find the text \"{}\"", &[&what]), true));
                false
            }
        }
    }

    pub fn search_range(&self) -> std::ops::Range<usize> {
        let d = self.doc();
        if self.find.in_selection && d.has_sel() {
            d.sel()
        } else {
            0..d.text().len()
        }
    }

    pub fn count(&mut self) {
        let Some(re) = self.regex() else { return };
        let r = self.search_range();
        let n = re.find_iter(&self.doc().text()[r]).count();
        self.find.status = Some((trf("Count: {} matches.", &[&n]), n == 0));
    }

    pub fn replace_one(&mut self) {
        let Some(re) = self.regex() else { return };
        push_history(&mut self.s.replace_history, &self.find.with);
        let repl = search::replacement(&self.find.with, self.s.search.mode);
        let backward = self.find.backward;
        let doc = self.doc_mut();
        if doc.read_only {
            return;
        }
        let sel = doc.sel();
        if let Some(rep) = search::replace_one(doc.text(), &re, &repl, sel.clone()) {
            doc.replace(sel.clone(), &rep, EditKind::Other);
            doc.set_sel(sel.start + rep.len(), sel.start + rep.len());
        }
        if !self.find_next(backward) {
            self.find.status = Some((tr("Replace: no more occurrence was found.").into(), true));
        }
    }

    pub fn replace_all(&mut self, all_docs: bool) {
        let Some(re) = self.regex() else { return };
        push_history(&mut self.s.replace_history, &self.find.with);
        let repl = search::replacement(&self.find.with, self.s.search.mode);
        let mut total = 0;
        let mut files = 0;
        let in_sel = self.find.in_selection && !all_docs;
        let range = self.search_range();
        let active = self.active_id();
        let targets: Vec<u64> = if all_docs { self.docs.iter().map(|d| d.id).collect() } else { vec![active] };
        for id in targets {
            let Some(doc) = self.doc_by_id_mut(id) else { continue };
            if doc.read_only {
                continue;
            }
            let r = if in_sel && id == active { range.clone() } else { 0..doc.text().len() };
            let (new, n) = search::replace_all(doc.text(), &re, &repl, r.clone());
            if n > 0 {
                let new_end = r.end + new.len() - doc.text().len();
                let sel = if in_sel { r.start..new_end } else { doc.cursor.min(new.len())..doc.cursor.min(new.len()) };
                let fix = |p: usize| {
                    let mut p = p.min(new.len());
                    while !new.is_char_boundary(p) {
                        p -= 1;
                    }
                    p
                };
                let sel = fix(sel.start)..fix(sel.end);
                doc.set_text(new, sel);
                total += n;
                files += 1;
            }
        }
        self.find.status = Some(if all_docs {
            (trf("Replace All: {} occurrences were replaced in {} documents.", &[&total, &files]), total == 0)
        } else {
            (trf("Replace All: {} occurrences were replaced.", &[&total]), total == 0)
        });
    }

    pub fn find_all(&mut self, all_docs: bool) {
        let Some(re) = self.regex() else { return };
        let ids: Vec<u64> = if all_docs { self.all_tab_ids() } else { vec![self.active_id()] };
        let mut files = Vec::new();
        let mut total = 0;
        for id in ids {
            let Some(d) = self.doc_by_id(id) else { continue };
            let hits = search::find_all(d.text(), &re, 100_000usize.saturating_sub(total));
            if !hits.is_empty() {
                total += hits.len();
                files.push(ResultFile { path: d.path.clone(), doc_id: Some(id), hits, collapsed: false });
            }
        }
        self.find.status = Some((trf("Find All: {} hits in {} documents.", &[&total, &files.len()]), total == 0));
        let title = format!("{} \"{}\" ({})", tr("Search"), self.find.what, trf("{} hits in {} files", &[&total, &files.len()]));
        self.add_results(title, files);
    }

    pub fn add_results(&mut self, title: String, files: Vec<ResultFile>) {
        for b in &mut self.results {
            b.collapsed = true;
        }
        self.results.insert(0, ResultBlock { title, files, collapsed: false });
        self.results.truncate(20);
        self.result_pos = None;
        self.show_results = true;
    }

    pub fn find_in_files(&mut self, ctx: &egui::Context) {
        let Some(re) = self.regex() else { return };
        let dir = PathBuf::from(&self.s.fif_dir);
        if !dir.is_dir() {
            self.find.status = Some((trf("Directory not found: {}", &[&self.s.fif_dir]), true));
            return;
        }
        let filter = match crate::findfiles::Filter::new(&self.s.fif_filters) {
            Ok(f) => f,
            Err(e) => {
                self.find.status = Some((e, true));
                return;
            }
        };
        push_history(&mut self.s.fif_dir_history, &self.s.fif_dir.clone());
        push_history(&mut self.s.fif_filter_history, &self.s.fif_filters.clone());
        if let Some(job) = &self.fif {
            job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx2 = ctx.clone();
        crate::findfiles::spawn(dir, filter, self.s.fif_subfolders, self.s.fif_hidden, re, cancel.clone(), tx, move || ctx2.request_repaint());
        self.fif = Some(FifJob { cancel, rx, scanned: 0 });
        let title = format!("{} \"{}\" — {} [{}]", tr("Search"), self.find.what, self.s.fif_dir, self.s.fif_filters);
        self.add_results(title, Vec::new());
        self.find.status = Some((tr("Searching…").into(), false));
    }

    pub fn replace_in_files(&mut self) {
        let Some(re) = self.regex() else { return };
        let repl = search::replacement(&self.find.with, self.s.search.mode);
        let filter = match crate::findfiles::Filter::new(&self.s.fif_filters) {
            Ok(f) => f,
            Err(e) => {
                self.find.status = Some((e, true));
                return;
            }
        };
        // Open documents are replaced in the editor (undoable) instead of on disk.
        let dir = PathBuf::from(&self.s.fif_dir);
        let cancel = AtomicBool::new(false);
        let list = crate::findfiles::files(&dir, &filter, self.s.fif_subfolders, self.s.fif_hidden, &cancel);
        let mut skip = Vec::new();
        let mut in_editor = 0;
        for p in &list {
            if let Some(d) = self.docs.iter_mut().find(|d| d.path.as_ref() == Some(p)) {
                skip.push(p.clone());
                let (new, n) = search::replace_all(d.text(), &re, &repl, 0..d.text().len());
                if n > 0 && !d.read_only {
                    let c = d.cursor.min(new.len());
                    let c = if new.is_char_boundary(c) { c } else { 0 };
                    d.set_text(new, c..c);
                    in_editor += n;
                }
            }
        }
        match crate::findfiles::replace_in_files(&dir, &filter, self.s.fif_subfolders, self.s.fif_hidden, &re, &repl, &skip) {
            Ok((files, n)) => {
                self.find.status = Some((trf("Replace in Files: {} occurrences replaced in {} files (+{} in open documents).", &[&n, &files, &in_editor]), n + in_editor == 0))
            }
            Err(e) => self.find.status = Some((e, true)),
        }
    }

    pub fn mark_all(&mut self) {
        let Some(re) = self.regex() else { return };
        let r = self.search_range();
        let bookmark = self.find.bookmark_lines;
        let purge = self.find.purge;
        let doc = self.doc_mut();
        if purge {
            doc.marks.retain(|(_, s)| *s != 0);
        }
        let base = r.start;
        let found: Vec<(std::ops::Range<usize>, u8)> =
            re.find_iter(&doc.text()[r]).filter(|m| !m.is_empty()).take(200_000).map(|m| (base + m.start()..base + m.end(), 0)).collect();
        if bookmark {
            for (m, _) in &found {
                let l = doc.line_of(m.start);
                doc.bookmarks.insert(l);
            }
        }
        let n = found.len();
        doc.marks.extend(found);
        self.find.status = Some((trf("Mark: {} matches.", &[&n]), n == 0));
    }

    pub fn open_find(&mut self, tab: FindTab) {
        let extra = self.s.word_chars.clone();
        let fill = self.s.fill_find_with_selection;
        let i = self.idx(self.active_id());
        let d = &self.docs[i];
        if fill && d.has_sel() && !d.selected_text().contains('\n') && d.selected_text().len() < 1024 {
            self.find.what = d.selected_text().to_owned();
        } else if fill && !d.has_sel() {
            if let Some(r) = editor::word_at(d.text(), d.cursor, &extra) {
                self.find.what = d.text()[r].to_owned();
            }
        }
        self.find.in_selection = d.has_sel() && d.selected_text().contains('\n');
        self.find.open = true;
        self.find.tab = tab;
        self.find.focus = true;
        self.find.status = None;
    }

    /// F4 / Shift+F4: next/previous entry of the search results.
    pub fn goto_result(&mut self, next: bool) {
        let Some(block) = self.results.first() else { return };
        let flat: Vec<(usize, usize)> = block.files.iter().enumerate().flat_map(|(fi, f)| (0..f.hits.len()).map(move |hi| (fi, hi))).collect();
        if flat.is_empty() {
            return;
        }
        let cur = self.result_pos.and_then(|(_, f, h)| flat.iter().position(|&x| x == (f, h)));
        let k = match cur {
            None => {
                if next {
                    0
                } else {
                    flat.len() - 1
                }
            }
            Some(c) => {
                if next {
                    (c + 1) % flat.len()
                } else {
                    (c + flat.len() - 1) % flat.len()
                }
            }
        };
        let (fi, hi) = flat[k];
        self.result_pos = Some((0, fi, hi));
        self.open_result(0, fi, hi);
    }

    pub fn open_result(&mut self, bi: usize, fi: usize, hi: usize) {
        let Some(f) = self.results.get(bi).and_then(|b| b.files.get(fi)) else { return };
        let hit = f.hits[hi].clone();
        let id = match (f.doc_id.filter(|&id| self.try_idx(id).is_some()), f.path.clone()) {
            (Some(id), _) => {
                self.activate_id(id);
                Some(id)
            }
            (None, Some(p)) => self.open_path(&p),
            _ => None,
        };
        let Some(id) = id else { return };
        let (pane, _) = (self.cur, ());
        self.with_view(pane, id, |app| {
            let d = app.doc_by_id_mut(id).unwrap();
            let line_ok = hit.line < d.line_count();
            let t = d.text();
            if hit.range.end <= t.len() && t.is_char_boundary(hit.range.start) && t.is_char_boundary(hit.range.end) && line_ok && d.line_of(hit.range.start) == hit.line {
                d.select(hit.range.clone());
            } else if line_ok {
                d.goto_line(hit.line);
            }
        });
        self.focus_editor = true;
    }
}

/// Notepad++ style custom date format (yyyy, MM, dd, HH, mm, ss, ddd, MMM …).
pub fn format_custom_date(fmt: &str, t: chrono::DateTime<chrono::Local>) -> String {
    let map = [
        ("yyyy", "%Y"),
        ("yy", "%y"),
        ("MMMM", "%B"),
        ("MMM", "%b"),
        ("MM", "%m"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("dd", "%d"),
        ("HH", "%H"),
        ("hh", "%I"),
        ("mm", "%M"),
        ("ss", "%S"),
        ("tt", "%p"),
    ];
    let mut out = String::new();
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for (k, v) in map {
            if let Some(r) = rest.strip_prefix(k) {
                out.push_str(&t.format(v).to_string());
                rest = r;
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap();
        if c == '%' {
            out.push_str("%%");
        } else {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    out.replace("%%", "%")
}

#[allow(dead_code)]
fn unused(_: LineOp, _: Hash) {}
