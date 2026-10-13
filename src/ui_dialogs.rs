//! Dialog windows: Find/Replace/Find in Files/Mark, incremental search, Go to, Column Editor,
//! characters in range, hashes, summary, Run, macros, Windows list, Print, Rename, confirmations
//! and the help windows.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eframe::egui::{self, vec2, Align2, Button, Color32, Key, RichText, TextEdit, Ui};

use crate::app::{fmt_num, App, Confirm, FindTab, Pick, APP_NAME};
use crate::cmd::{self, Cmd, Hash};
use crate::i18n::{tr, trf};
use crate::search::Mode;
use crate::settings::{RunCommand, SavedMacro};
use crate::textops::{self, NumFormat};
use crate::tools;

#[derive(Default)]
pub struct IncSearch {
    pub text: String,
    pub match_case: bool,
    pub highlight_all: bool,
    pub start: Option<usize>,
    pub found: Option<bool>,
    pub focused_once: bool,
}

#[derive(Default)]
pub struct PrintDlg {
    pub printer: String,
    pub copies: u32,
    pub printers: Vec<String>,
}

pub struct ColumnDlg {
    pub text_mode: bool,
    pub text: String,
    pub initial: String,
    pub step: String,
    pub repeat: String,
    pub leading: u8,
    pub format: NumFormat,
}

impl Default for ColumnDlg {
    fn default() -> Self {
        Self { text_mode: true, text: String::new(), initial: "1".into(), step: "1".into(), repeat: "1".into(), leading: 0, format: NumFormat::Dec }
    }
}

#[derive(Default)]
pub struct CharRangeDlg {
    pub mode: u8,
    pub from: String,
    pub to: String,
    pub backward: bool,
    pub status: String,
}

#[derive(Default)]
pub struct HashDlg {
    pub kind: Hash,
    pub input: String,
    pub output: String,
    pub files: bool,
}


#[derive(Default)]
pub struct RunDlg {
    pub command: String,
    pub save_name: String,
    pub saving: bool,
}

#[derive(Default)]
pub struct MacroMulti {
    /// None = current recorded macro
    pub macro_idx: Option<usize>,
    pub count: String,
    pub until_end: bool,
}

#[derive(Default)]
pub struct WindowsDlg {
    pub sort: u8,
    pub selected: BTreeSet<u64>,
}

#[derive(Default)]
pub struct Dialogs {
    pub goto_open: bool,
    pub goto_focus: bool,
    pub goto_offset: bool,
    pub goto_input: String,
    pub rename: Option<(u64, String)>,
    pub print: Option<PrintDlg>,
    pub column_editor: Option<ColumnDlg>,
    pub char_range: Option<CharRangeDlg>,
    pub hash: Option<HashDlg>,
    pub summary: bool,
    pub run: Option<RunDlg>,
    pub save_macro: Option<(String, String)>,
    pub macro_multi: Option<MacroMulti>,
    pub windows: Option<WindowsDlg>,
    pub about: bool,
    pub cmdline: bool,
    pub debug: bool,
    pub tree_edit: Option<crate::ui_panels::TreeEdit>,
    /// "Set Updater Proxy" dialog (edited value).
    pub update_proxy: Option<String>,
}

pub enum UpdateState {
    Checking,
    UpToDate,
    Available(crate::update::Release),
    Installing,
    Installed(std::path::PathBuf),
    Error(String),
}

pub struct UpdateDlg {
    pub state: UpdateState,
    /// Opened from the menu (an automatic check only shows a found update).
    pub manual: bool,
}

impl Dialogs {
    /// A dialog with a text field is open (the editor must not grab the focus).
    pub fn any_text_dialog(&self) -> bool {
        self.goto_open
            || self.rename.is_some()
            || self.column_editor.is_some()
            || self.run.is_some()
            || self.hash.is_some()
            || self.save_macro.is_some()
            || self.macro_multi.is_some()
            || self.char_range.is_some()
            || self.tree_edit.is_some()
            || self.update_proxy.is_some()
    }
}

fn esc_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(Key::Escape))
}

fn history_menu(ui: &mut Ui, id: &str, history: &[String], target: &mut String) {
    ui.push_id(id, |ui| {
        ui.add_enabled_ui(!history.is_empty(), |ui| {
            ui.menu_button("▾", |ui| {
                for h in history {
                    let label: String = h.chars().take(80).collect();
                    if ui.button(label).clicked() {
                        *target = h.clone();
                        ui.close_menu();
                    }
                }
            });
        });
    });
}

impl App {
    pub fn dialogs_ui(&mut self, ctx: &egui::Context) {
        self.find_window(ctx);
        self.goto_window(ctx);
        self.confirm_window(ctx);
        self.small_dialogs(ctx);
        self.message_window(ctx);
    }

    fn message_window(&mut self, ctx: &egui::Context) {
        if let Some((title, msg)) = self.message.clone() {
            let mut close = false;
            egui::Window::new(title).id(egui::Id::new("message")).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).order(egui::Order::Foreground).show(ctx, |ui| {
                ui.label(msg);
                ui.add_space(6.0);
                close = ui.button(tr("OK")).clicked();
            });
            if close || esc_pressed(ctx) || ctx.input(|i| i.key_pressed(Key::Enter)) {
                self.message = None;
            }
        }
    }

    fn find_window(&mut self, ctx: &egui::Context) {
        if !self.find.open {
            return;
        }
        let mut open = true;
        let mut action: Option<&'static str> = None;
        let title = match self.find.tab {
            FindTab::Find => tr("Find"),
            FindTab::Replace => tr("Replace"),
            FindTab::FindInFiles => tr("Find in Files"),
            FindTab::Mark => tr("Mark"),
        };
        egui::Window::new(title)
            .id(egui::Id::new("find_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(ctx.screen_rect().right_top() + vec2(-620.0, 90.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [(FindTab::Find, "Find"), (FindTab::Replace, "Replace"), (FindTab::FindInFiles, "Find in Files"), (FindTab::Mark, "Mark")] {
                        if ui.selectable_label(self.find.tab == tab, tr(label)).clicked() {
                            self.find.tab = tab;
                        }
                    }
                });
                ui.separator();
                let fif = self.find.tab == FindTab::FindInFiles;
                egui::Grid::new("find_grid").num_columns(3).spacing([6.0, 6.0]).show(ui, |ui| {
                    ui.label(tr("Find what :"));
                    let r = ui.add_sized([330.0, 20.0], TextEdit::singleline(&mut self.find.what).id_salt("find_what"));
                    if self.find.focus {
                        r.request_focus();
                        if let Some(mut st) = egui::text_edit::TextEditState::load(ui.ctx(), r.id) {
                            let n = self.find.what.chars().count();
                            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(n))));
                            st.store(ui.ctx(), r.id);
                        }
                        self.find.focus = false;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let shift = ui.input(|i| i.modifiers.shift);
                        action = Some(match self.find.tab {
                            FindTab::Mark => "mark",
                            FindTab::FindInFiles => "fif",
                            _ if shift => "prev",
                            _ => "next",
                        });
                        r.request_focus();
                    }
                    history_menu(ui, "fh", &self.s.find_history.clone(), &mut self.find.what);
                    ui.end_row();
                    if matches!(self.find.tab, FindTab::Replace | FindTab::FindInFiles) {
                        ui.label(tr("Replace with :"));
                        ui.add_sized([330.0, 20.0], TextEdit::singleline(&mut self.find.with).id_salt("find_with"));
                        history_menu(ui, "rh", &self.s.replace_history.clone(), &mut self.find.with);
                        ui.end_row();
                    }
                    if fif {
                        ui.label(tr("Filters :"));
                        ui.add_sized([330.0, 20.0], TextEdit::singleline(&mut self.s.fif_filters).hint_text("*.rs *.toml !*.min.js !+/target"));
                        history_menu(ui, "fifh", &self.s.fif_filter_history.clone(), &mut self.s.fif_filters);
                        ui.end_row();
                        ui.label(tr("Directory :"));
                        ui.add_sized([330.0, 20.0], TextEdit::singleline(&mut self.s.fif_dir));
                        ui.horizontal(|ui| {
                            history_menu(ui, "fifd", &self.s.fif_dir_history.clone(), &mut self.s.fif_dir);
                            if ui.button("…").on_hover_text(tr("Browse")).clicked() {
                                action = Some("browse");
                            }
                            if ui.button("⬆").on_hover_text(tr("Follow current document")).clicked() {
                                action = Some("follow");
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        if self.find.tab == FindTab::Mark {
                            ui.checkbox(&mut self.find.bookmark_lines, tr("Bookmark line"));
                            ui.checkbox(&mut self.find.purge, tr("Purge for each search"));
                        }
                        if fif {
                            ui.checkbox(&mut self.s.fif_subfolders, tr("In all sub-folders"));
                            ui.checkbox(&mut self.s.fif_hidden, tr("In hidden folders"));
                        }
                        if !fif {
                            ui.checkbox(&mut self.find.backward, tr("Backward direction"));
                        }
                        ui.checkbox(&mut self.s.search.whole_word, tr("Match whole word only"));
                        ui.checkbox(&mut self.s.search.match_case, tr("Match case"));
                        if !fif {
                            ui.checkbox(&mut self.s.search.wrap, tr("Wrap around"));
                        }
                        if matches!(self.find.tab, FindTab::Replace | FindTab::Mark) {
                            ui.checkbox(&mut self.find.in_selection, tr("In selection"));
                        }
                        ui.add_space(4.0);
                        ui.group(|ui| {
                            ui.label(tr("Search Mode"));
                            ui.radio_value(&mut self.s.search.mode, Mode::Normal, tr("Normal"));
                            ui.radio_value(&mut self.s.search.mode, Mode::Extended, tr("Extended (\\n, \\r, \\t, \\0, \\x...)"));
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.s.search.mode, Mode::Regex, tr("Regular expression"));
                                ui.add_enabled(self.s.search.mode == Mode::Regex, egui::Checkbox::new(&mut self.s.search.dot_all, tr(". matches newline")));
                            });
                        });
                    });
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        let w = 250.0;
                        let mut btn = |ui: &mut Ui, label: &str, a: &'static str| {
                            if ui.add_sized([w, 22.0], Button::new(tr(label))).clicked() {
                                action = Some(a);
                            }
                        };
                        match self.find.tab {
                            FindTab::Find => {
                                btn(ui, "Find Next", "next");
                                btn(ui, "Count", "count");
                                btn(ui, "Find All in Current Document", "all");
                                btn(ui, "Find All in All Opened Documents", "all_docs");
                            }
                            FindTab::Replace => {
                                btn(ui, "Find Next", "next");
                                btn(ui, "Replace", "replace");
                                btn(ui, "Replace All", "replace_all");
                                btn(ui, "Replace All in All Opened Documents", "replace_all_docs");
                            }
                            FindTab::FindInFiles => {
                                if self.fif.is_some() {
                                    btn(ui, "Stop", "stop");
                                } else {
                                    btn(ui, "Find All", "fif");
                                }
                                btn(ui, "Replace in Files", "rif");
                            }
                            FindTab::Mark => {
                                btn(ui, "Mark All", "mark");
                                btn(ui, "Clear all marks", "clear_marks");
                                btn(ui, "Copy Marked Text", "copy_marked");
                            }
                        }
                        btn(ui, "Close", "close");
                    });
                });
                if let Some((msg, err)) = &self.find.status {
                    ui.separator();
                    let c = if *err { Color32::from_rgb(210, 50, 40) } else { Color32::from_rgb(40, 150, 60) };
                    ui.label(RichText::new(msg).color(c));
                }
            });
        if esc_pressed(ctx) && !self.editor_focused && self.message.is_none() && self.confirms.is_empty() {
            open = false;
        }
        match action {
            Some("next") => {
                let b = self.find.backward;
                self.run_find_in_view(|app| {
                    app.find_next(b);
                });
            }
            Some("prev") => {
                let b = !self.find.backward;
                self.run_find_in_view(|app| {
                    app.find_next(b);
                });
            }
            Some("count") => self.run_find_in_view(|app| app.count()),
            Some("all") => self.find_all(false),
            Some("all_docs") => self.find_all(true),
            Some("replace") => self.run_find_in_view(|app| app.replace_one()),
            Some("replace_all") => self.run_find_in_view(|app| app.replace_all(false)),
            Some("replace_all_docs") => {
                if self.s.confirm_replace_all_docs {
                    self.confirms.push_back(Confirm::ReplaceAllDocs);
                } else {
                    self.replace_all(true);
                }
            }
            Some("fif") => self.find_in_files(ctx),
            Some("stop") => {
                if let Some(j) = &self.fif {
                    j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Some("rif") => self.confirms.push_back(Confirm::ReplaceInFiles),
            Some("browse") => self.pick(ctx, Pick::FifDir, true, false, None, None),
            Some("follow") => {
                if let Some(d) = self.doc().path.as_ref().and_then(|p| p.parent()) {
                    self.s.fif_dir = d.display().to_string();
                }
            }
            Some("mark") => self.run_find_in_view(|app| app.mark_all()),
            Some("clear_marks") => {
                self.doc_mut().marks.retain(|(_, s)| *s != 0);
                self.find.status = None;
            }
            Some("copy_marked") => self.run(ctx, Cmd::CopyStyled(6)),
            Some("close") => open = false,
            _ => {}
        }
        if !open {
            self.find.open = false;
            self.focus_editor = true;
        }
    }

    /// Runs a search operation with the active view's selection state.
    fn run_find_in_view(&mut self, f: impl FnOnce(&mut Self)) {
        let (pane, id) = (self.cur, self.active_id());
        self.with_view(pane, id, f);
    }

    pub fn inc_search_bar(&mut self, ui: &mut Ui) {
        let Some(mut inc) = self.inc_search.take() else { return };
        let mut close = false;
        let mut dir: Option<bool> = None;
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui.small_button("✖").clicked() {
                close = true;
            }
            ui.label(tr("Find :"));
            let r = ui.add(TextEdit::singleline(&mut inc.text).desired_width(260.0).id_salt("incsearch"));
            if !inc.focused_once {
                r.request_focus();
                inc.focused_once = true;
            }
            changed |= r.changed();
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                dir = Some(!ui.input(|i| i.modifiers.shift));
                r.request_focus();
            }
            if ui.button("◀").on_hover_text(tr("Find Previous")).clicked() {
                dir = Some(false);
            }
            if ui.button("▶").on_hover_text(tr("Find Next")).clicked() {
                dir = Some(true);
            }
            changed |= ui.checkbox(&mut inc.highlight_all, tr("Highlight all")).changed();
            changed |= ui.checkbox(&mut inc.match_case, tr("Match case")).changed();
            match inc.found {
                Some(false) if !inc.text.is_empty() => {
                    ui.label(RichText::new(tr("Phrase not found")).color(Color32::from_rgb(210, 50, 40)));
                }
                _ => {}
            }
        });
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        let (pane, id) = (self.cur, self.active_id());
        if changed || dir.is_some() {
            let text = inc.text.clone();
            let case = inc.match_case;
            let hl_all = inc.highlight_all;
            let start = *inc.start.get_or_insert(self.doc().sel().start);
            let found = self.with_view(pane, id, |app| {
                let d = app.doc_mut();
                d.marks.retain(|(_, s)| *s != 0);
                if text.is_empty() {
                    let p = start.min(d.text().len());
                    d.set_sel(p, p);
                    return None;
                }
                let re = regex::RegexBuilder::new(&regex::escape(&text)).case_insensitive(!case).build().ok()?;
                if hl_all {
                    let ms: Vec<_> = re.find_iter(d.text()).take(100_000).map(|m| (m.range(), 0u8)).collect();
                    d.marks.extend(ms);
                }
                let from = match dir {
                    None => start,
                    Some(true) => d.sel().end,
                    Some(false) => d.sel().start,
                };
                let r = crate::search::find(d.text(), &re, from, dir != Some(false), true);
                if let Some((r, _)) = &r {
                    d.select(r.clone());
                }
                Some(r.is_some())
            });
            inc.found = found;
        }
        if close {
            self.with_view(pane, id, |app| app.doc_mut().marks.retain(|(_, s)| *s != 0));
            self.focus_editor = true;
        } else {
            self.inc_search = Some(inc);
        }
    }

    fn goto_window(&mut self, ctx: &egui::Context) {
        if !self.dlg.goto_open {
            return;
        }
        let mut open = true;
        let mut go = false;
        let (line, _) = self.doc().line_col(self.doc().cursor);
        let lines = self.doc().line_count();
        let len = self.doc().text().len();
        egui::Window::new(tr("Go To..."))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 120.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.dlg.goto_offset, false, tr("Line"));
                    ui.radio_value(&mut self.dlg.goto_offset, true, tr("Offset"));
                });
                egui::Grid::new("goto").num_columns(2).show(ui, |ui| {
                    ui.label(tr("You are here :"));
                    ui.label(if self.dlg.goto_offset { self.doc().cursor.to_string() } else { (line + 1).to_string() });
                    ui.end_row();
                    ui.label(tr("You want to go to :"));
                    let r = ui.add(TextEdit::singleline(&mut self.dlg.goto_input).desired_width(120.0));
                    if self.dlg.goto_focus {
                        r.request_focus();
                        self.dlg.goto_focus = false;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        go = true;
                    }
                    ui.end_row();
                    ui.label(tr("You can't go further than :"));
                    ui.label(if self.dlg.goto_offset { len.to_string() } else { lines.to_string() });
                    ui.end_row();
                });
                if ui.button(tr("Go")).clicked() {
                    go = true;
                }
            });
        if go {
            if let Ok(n) = self.dlg.goto_input.trim().replace(['.', ','], "").parse::<usize>() {
                let offset = self.dlg.goto_offset;
                self.run_find_in_view(|app| {
                    let d = app.doc_mut();
                    if offset {
                        let p = d.clamp(n);
                        d.select(p..p);
                    } else {
                        d.goto_line(n.saturating_sub(1));
                    }
                });
                open = false;
            }
        }
        if esc_pressed(ctx) {
            open = false;
        }
        if !open {
            self.dlg.goto_open = false;
            self.focus_editor = true;
        }
    }

    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(&c) = self.confirms.front() else { return };
        let doc_id = match c {
            Confirm::SaveBeforeClose(id) | Confirm::ReloadChanged(id) | Confirm::KeepMissing(id) | Confirm::ReloadDiscard(id) | Confirm::TrashFile(id) => Some(id),
            _ => None,
        };
        let name = match doc_id {
            Some(id) => match self.doc_by_id(id) {
                Some(d) => d.path_string().unwrap_or_else(|| d.title()),
                None => {
                    self.confirms.pop_front();
                    return;
                }
            },
            None => String::new(),
        };
        let dirty = doc_id.and_then(|id| self.doc_by_id(id)).is_some_and(|d| d.is_dirty());
        let (title, text, buttons): (&str, String, &[&str]) = match c {
            Confirm::SaveBeforeClose(_) => ("Save", trf("Save file \"{}\" ?", &[&name]), &["Yes", "No", "Cancel"]),
            Confirm::ReloadChanged(_) => (
                "Reload",
                if dirty {
                    trf("\"{}\"\n\nThis file has been modified by another program.\nDo you want to reload it and lose the changes made in NotepadRS?", &[&name])
                } else {
                    trf("\"{}\"\n\nThis file has been modified by another program.\nDo you want to reload it?", &[&name])
                },
                &["Yes", "No"],
            ),
            Confirm::KeepMissing(_) => ("Keep non existing file", trf("\"{}\"\n\ndoesn't exist anymore.\nKeep this file in editor?", &[&name]), &["Yes", "No"]),
            Confirm::ReloadDiscard(_) => ("Reload", trf("Are you sure you want to reload the current file and lose the changes made in NotepadRS?\n\n{}", &[&name]), &["Yes", "No"]),
            Confirm::TrashFile(_) => ("Move to Recycle Bin", trf("Move \"{}\" to the trash and close it?", &[&name]), &["Yes", "No"]),
            Confirm::ReplaceAllDocs => ("Replace All", tr("Are you sure you want to replace all occurrences in all open documents?").into(), &["Yes", "No"]),
            Confirm::ReplaceInFiles => (
                "Replace in Files",
                trf("Are you sure you want to replace all occurrences in:\n{}\nfor file type: {}", &[&self.s.fif_dir, &self.s.fif_filters]),
                &["Yes", "No"],
            ),
        };
        if let Some(id) = doc_id {
            if !matches!(c, Confirm::SaveBeforeClose(_)) || self.doc_by_id(id).is_some() {
                self.activate_id(id);
            }
        }
        let mut answer = None;
        egui::Area::new(egui::Id::new("modal_dim")).order(egui::Order::Middle).fixed_pos(egui::pos2(0.0, 0.0)).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.allocate_rect(r, egui::Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(70));
        });
        egui::Window::new(tr(title)).id(egui::Id::new("confirm")).collapsible(false).resizable(false).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.label(text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (n, b) in buttons.iter().enumerate() {
                    let r = ui.add_sized([90.0, 24.0], Button::new(tr(b)));
                    if n == 0 && !ctx.memory(|m| m.focused().is_some()) {
                        r.request_focus();
                    }
                    if r.clicked() {
                        answer = Some(n);
                    }
                }
            });
        });
        if esc_pressed(ctx) {
            answer = Some(buttons.len() - 1);
        }
        let Some(a) = answer else { return };
        self.confirms.pop_front();
        match (c, a) {
            (Confirm::SaveBeforeClose(id), 0) => {
                let pane = self.close_queue.iter().find(|(x, _)| *x == id).map_or(self.cur, |(_, p)| *p);
                if self.save(id) {
                    self.close_queue.retain(|(x, _)| *x != id);
                    self.close_tab_now(id, pane);
                } else if self.doc_by_id(id).is_some_and(|d| d.path.is_none()) {
                    self.close_after_save = Some(id);
                } else {
                    self.cancel_close();
                }
            }
            (Confirm::SaveBeforeClose(id), 1) => {
                let pane = self.close_queue.iter().find(|(x, _)| *x == id).map_or(self.cur, |(_, p)| *p);
                self.close_queue.retain(|(x, _)| *x != id);
                self.close_tab_now(id, pane);
            }
            (Confirm::SaveBeforeClose(_), _) => self.cancel_close(),
            (Confirm::ReloadChanged(id) | Confirm::ReloadDiscard(id), 0) => {
                let eol = self.s.default_eol;
                if let Some(d) = self.doc_by_id_mut(id) {
                    let enc = d.encoding.clone();
                    if let Err(e) = d.load_from_disk(Some(enc), eol) {
                        self.message = Some((tr("Reload").into(), e));
                    }
                }
            }
            (Confirm::KeepMissing(id), 0) => {
                if let Some(d) = self.doc_by_id_mut(id) {
                    d.force_dirty();
                }
            }
            (Confirm::KeepMissing(id), _) => {
                for p in 0..2 {
                    if self.panes[p].tabs.contains(&id) {
                        self.close_tab_now(id, p);
                    }
                }
            }
            (Confirm::TrashFile(id), 0) => {
                if let Some(p) = self.doc_by_id(id).and_then(|d| d.path.clone()) {
                    match tools::move_to_trash(&p) {
                        Ok(()) => {
                            for pn in 0..2 {
                                if self.panes[pn].tabs.contains(&id) {
                                    self.close_tab_now(id, pn);
                                }
                            }
                        }
                        Err(e) => self.message = Some((tr("Move to Recycle Bin").into(), e)),
                    }
                }
            }
            (Confirm::ReplaceAllDocs, 0) => self.replace_all(true),
            (Confirm::ReplaceInFiles, 0) => self.replace_in_files(),
            _ => {}
        }
    }

    fn small_dialogs(&mut self, ctx: &egui::Context) {
        // Rename
        if let Some((id, mut name)) = self.dlg.rename.take() {
            let mut open = true;
            let mut ok = false;
            egui::Window::new(tr("Rename")).open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                ui.label(tr("New name:"));
                let r = ui.add(TextEdit::singleline(&mut name).desired_width(320.0));
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    ok = true;
                }
                ok |= ui.button(tr("OK")).clicked();
            });
            if ok {
                if let Some(d) = self.doc_by_id_mut(id) {
                    if let Some(old) = d.path.clone() {
                        let new = old.with_file_name(&name);
                        match std::fs::rename(&old, &new) {
                            Ok(()) => {
                                d.path = Some(new.clone());
                                d.disk_mtime = std::fs::metadata(&new).and_then(|m| m.modified()).ok();
                            }
                            Err(e) => self.message = Some((tr("Rename").into(), e.to_string())),
                        }
                    }
                }
            } else if open && !esc_pressed(ctx) {
                self.dlg.rename = Some((id, name));
            }
        }

        // Print
        if let Some(mut p) = self.dlg.print.take() {
            if p.printers.is_empty() && p.copies == 0 {
                p.printers = tools::printers();
                p.printer = p.printers.first().cloned().unwrap_or_default();
                p.copies = 1;
            }
            let mut open = true;
            let mut act = None;
            egui::Window::new(tr("Print")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("print").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Printer:"));
                    egui::ComboBox::from_id_salt("printer").selected_text(if p.printer.is_empty() { tr("(default)") } else { &p.printer }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.printer, String::new(), tr("(default)"));
                        for pr in &p.printers {
                            ui.selectable_value(&mut p.printer, pr.clone(), pr);
                        }
                    });
                    ui.end_row();
                    ui.label(tr("Copies:"));
                    ui.add(egui::DragValue::new(&mut p.copies).range(1..=99));
                    ui.end_row();
                    ui.label(tr("Font size:"));
                    ui.add(egui::DragValue::new(&mut self.s.print_font_size).range(5.0..=20.0).speed(0.2));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut self.s.print_line_numbers, tr("Print line numbers"));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut self.s.print_colors, tr("Colours (syntax highlighting)"));
                    ui.end_row();
                    ui.label(tr("Header:"));
                    ui.text_edit_singleline(&mut self.s.print_header);
                    ui.end_row();
                    ui.label(tr("Footer:"));
                    ui.text_edit_singleline(&mut self.s.print_footer);
                    ui.end_row();
                });
                if p.printers.is_empty() {
                    ui.label(RichText::new(tr("No printers found (CUPS). You can still print to a PostScript file.")).weak());
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("Print")).clicked() {
                        act = Some(true);
                    }
                    if ui.button(tr("Print to file (.ps)...")).clicked() {
                        act = Some(false);
                    }
                });
            });
            match act {
                Some(true) => {
                    let ps = self.print_postscript();
                    let cmd = self.s.print_command.clone();
                    if let Err(e) = tools::send_to_printer(&cmd, Some(&p.printer), p.copies, &ps) {
                        self.message = Some((tr("Print").into(), e));
                    }
                }
                Some(false) => {
                    let name = format!("{}.ps", self.doc().title());
                    self.pick(ctx, Pick::PrintToFile, false, true, Some(name), None);
                }
                None if open && !esc_pressed(ctx) => self.dlg.print = Some(p),
                None => {}
            }
        }

        // Column editor
        if let Some(mut c) = self.dlg.column_editor.take() {
            let mut open = true;
            let mut ok = false;
            egui::Window::new(tr("Column / Multi-Selection Editor")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.radio_value(&mut c.text_mode, true, tr("Text to Insert"));
                ui.add_enabled(c.text_mode, TextEdit::singleline(&mut c.text).desired_width(280.0));
                ui.add_space(6.0);
                ui.radio_value(&mut c.text_mode, false, tr("Number to Insert"));
                ui.add_enabled_ui(!c.text_mode, |ui| {
                    egui::Grid::new("colnum").num_columns(2).show(ui, |ui| {
                        ui.label(tr("Initial number :"));
                        ui.add(TextEdit::singleline(&mut c.initial).desired_width(80.0));
                        ui.end_row();
                        ui.label(tr("Increase by :"));
                        ui.add(TextEdit::singleline(&mut c.step).desired_width(80.0));
                        ui.end_row();
                        ui.label(tr("Repeat :"));
                        ui.add(TextEdit::singleline(&mut c.repeat).desired_width(80.0));
                        ui.end_row();
                        ui.label(tr("Leading :"));
                        egui::ComboBox::from_id_salt("leading").selected_text(tr(["None", "Zeros", "Spaces"][c.leading as usize])).show_ui(ui, |ui| {
                            for (i, l) in ["None", "Zeros", "Spaces"].iter().enumerate() {
                                ui.selectable_value(&mut c.leading, i as u8, tr(l));
                            }
                        });
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Format :"));
                        ui.radio_value(&mut c.format, NumFormat::Dec, "Dec");
                        ui.radio_value(&mut c.format, NumFormat::Hex, "Hex");
                        ui.radio_value(&mut c.format, NumFormat::Oct, "Oct");
                        ui.radio_value(&mut c.format, NumFormat::Bin, "Bin");
                    });
                });
                ok = ui.button(tr("OK")).clicked();
            });
            if ok {
                self.apply_column_editor(&c);
                self.focus_editor = true;
            } else if open && !esc_pressed(ctx) {
                self.dlg.column_editor = Some(c);
            }
        }

        // Find characters in range
        if let Some(mut c) = self.dlg.char_range.take() {
            let mut open = true;
            let mut find = false;
            egui::Window::new(tr("Find Characters in Range...")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.radio_value(&mut c.mode, 0, tr("Non-ASCII Characters (128-255)"));
                ui.radio_value(&mut c.mode, 1, tr("ASCII Characters (0-127)"));
                ui.radio_value(&mut c.mode, 3, tr("Non-ASCII Characters (> 127, incl. Unicode)"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut c.mode, 2, tr("My Range:"));
                    ui.add(TextEdit::singleline(&mut c.from).desired_width(50.0));
                    ui.label("-");
                    ui.add(TextEdit::singleline(&mut c.to).desired_width(50.0));
                });
                ui.checkbox(&mut c.backward, tr("Backward direction"));
                find = ui.button(tr("Find")).clicked();
                if !c.status.is_empty() {
                    ui.label(&c.status);
                }
            });
            if find {
                let (lo, hi) = match c.mode {
                    0 => (128, 255),
                    1 => (0, 127),
                    3 => (128, u32::MAX),
                    _ => (c.from.trim().parse().unwrap_or(0), c.to.trim().parse().unwrap_or(0)),
                };
                let back = c.backward;
                let found = self.with_view(self.cur, self.active_id(), |app| {
                    let d = app.doc_mut();
                    let t = d.text();
                    let hit = if back {
                        t[..d.sel().start].char_indices().rev().find(|(_, ch)| (lo..=hi).contains(&(*ch as u32))).map(|(i, ch)| i..i + ch.len_utf8())
                    } else {
                        let from = d.sel().end;
                        t[from..].char_indices().find(|(_, ch)| (lo..=hi).contains(&(*ch as u32))).map(|(i, ch)| from + i..from + i + ch.len_utf8())
                    };
                    if let Some(r) = hit.clone() {
                        d.select(r);
                    }
                    hit.is_some()
                });
                c.status = if found { String::new() } else { tr("Can't find any character in the range.").into() };
            }
            if open && !esc_pressed(ctx) {
                self.dlg.char_range = Some(c);
            }
        }

        // Hash
        if let Some(mut h) = self.dlg.hash.take() {
            let mut open = true;
            egui::Window::new(trf("Generate {} digest", &[&tools::hash_name(h.kind)])).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
                if !h.files {
                    ui.label(tr("Text:"));
                    let r = ui.add(TextEdit::multiline(&mut h.input).desired_width(540.0).desired_rows(5));
                    if r.changed() || h.output.is_empty() {
                        h.output = h.input.lines().filter(|l| !l.is_empty()).map(|l| tools::hash_bytes(h.kind, l.as_bytes())).collect::<Vec<_>>().join("\n");
                    }
                    ui.checkbox(&mut true.clone(), tr("Treat each line as a separate string"));
                }
                ui.label(tr("Digest:"));
                ui.add(TextEdit::multiline(&mut h.output.as_str()).desired_width(540.0).desired_rows(5).font(egui::TextStyle::Monospace));
                if ui.button(tr("Copy to clipboard")).clicked() {
                    ui.ctx().copy_text(h.output.clone());
                }
            });
            if open && !esc_pressed(ctx) {
                self.dlg.hash = Some(h);
            }
        }

        // Summary
        if self.dlg.summary {
            let d = self.doc();
            let text = d.text();
            let words = text.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).count();
            let chars_no_eol = text.chars().filter(|&c| c != '\n').count();
            let non_blank = text.chars().filter(|c| !c.is_whitespace()).count();
            let meta = d.path.as_ref().and_then(|p| std::fs::metadata(p).ok());
            let fmt_time = |t: Option<std::time::SystemTime>| t.map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default();
            let mut info = Vec::new();
            info.push((tr("Full file path:").to_string(), d.path_string().unwrap_or_else(|| d.title())));
            if let Some(m) = &meta {
                info.push((tr("Created:").into(), fmt_time(m.created().ok())));
                info.push((tr("Modified:").into(), fmt_time(m.modified().ok())));
            }
            info.push((tr("Characters (without line endings):").into(), fmt_num(chars_no_eol)));
            info.push((tr("Characters (without blanks):").into(), fmt_num(non_blank)));
            info.push((tr("Words:").into(), fmt_num(words)));
            info.push((tr("Lines:").into(), fmt_num(d.line_count())));
            info.push((tr("Document length:").into(), fmt_num(d.disk_text().len())));
            if d.has_sel() {
                let sel = d.selected_text();
                info.push((tr("Selected characters:").into(), fmt_num(sel.chars().count())));
                info.push((tr("Selected bytes:").into(), fmt_num(sel.len())));
            }
            let mut open = true;
            egui::Window::new(tr("Summary")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("summary").num_columns(2).striped(true).show(ui, |ui| {
                    for (k, v) in &info {
                        ui.label(k);
                        ui.label(v);
                        ui.end_row();
                    }
                });
            });
            self.dlg.summary = open && !esc_pressed(ctx);
        }

        // Run
        if let Some(mut r) = self.dlg.run.take() {
            let mut open = true;
            let mut run = false;
            let mut save = false;
            egui::Window::new(tr("Run...")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(tr("The Program to Run"));
                ui.horizontal(|ui| {
                    let te = ui.add(TextEdit::singleline(&mut r.command).desired_width(380.0).hint_text("xdg-open \"$(FULL_CURRENT_PATH)\""));
                    if !r.saving {
                        te.request_focus();
                    }
                    if te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        run = true;
                    }
                    ui.menu_button("$(…)", |ui| {
                        for v in ["$(FULL_CURRENT_PATH)", "$(CURRENT_DIRECTORY)", "$(FILE_NAME)", "$(NAME_PART)", "$(EXT_PART)", "$(CURRENT_WORD)", "$(CURRENT_LINE)", "$(CURRENT_COLUMN)", "$(CURRENT_LINESTR)", "$(NPP_DIRECTORY)", "$(SYS.HOME)"] {
                            if ui.button(v).clicked() {
                                r.command.push_str(v);
                                ui.close_menu();
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    run |= ui.button(tr("Run!")).clicked();
                    if ui.button(tr("Save...")).clicked() {
                        r.saving = true;
                    }
                });
                if r.saving {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(tr("Name:"));
                        ui.text_edit_singleline(&mut r.save_name);
                        save = ui.button(tr("OK")).clicked();
                    });
                }
            });
            if run && !r.command.trim().is_empty() {
                let c = tools::expand_vars(&r.command, &self.run_vars());
                let dir = self.doc().path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
                if let Err(e) = tools::spawn_shell(&c, dir.as_deref()) {
                    self.message = Some((tr("Run").into(), e));
                }
            }
            if save && !r.save_name.trim().is_empty() {
                self.s.run_commands.push(RunCommand { name: r.save_name.trim().to_string(), command: r.command.clone(), shortcut: String::new() });
                r.saving = false;
            }
            if open && !run && !esc_pressed(ctx) {
                self.dlg.run = Some(r);
            }
        }

        // Save macro
        if let Some((mut name, mut shortcut)) = self.dlg.save_macro.take() {
            let mut open = true;
            let mut ok = false;
            egui::Window::new(tr("Save Macro")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("savemacro").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Name :"));
                    ui.text_edit_singleline(&mut name).request_focus();
                    ui.end_row();
                    ui.label(tr("Shortcut :"));
                    ui.add(TextEdit::singleline(&mut shortcut).hint_text("Ctrl+Alt+1"));
                    ui.end_row();
                });
                if !shortcut.is_empty() && cmd::parse_shortcut(&shortcut).is_none() {
                    ui.label(RichText::new(tr("Invalid shortcut")).color(Color32::RED));
                }
                ok = ui.button(tr("OK")).clicked();
            });
            if ok && !name.trim().is_empty() {
                self.s.macros.push(SavedMacro { name: name.trim().into(), events: self.macro_.last.clone(), shortcut });
            } else if open && !esc_pressed(ctx) {
                self.dlg.save_macro = Some((name, shortcut));
            }
        }

        // Run macro multiple times
        if let Some(mut m) = self.dlg.macro_multi.take() {
            let mut open = true;
            let mut run = false;
            if m.count.is_empty() {
                m.count = "1".into();
            }
            egui::Window::new(tr("Run a Macro Multiple Times")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Macro to run"));
                    let sel = m.macro_idx.and_then(|i| self.s.macros.get(i)).map_or(tr("Current recorded macro").to_string(), |x| x.name.clone());
                    egui::ComboBox::from_id_salt("macro_sel").selected_text(sel).show_ui(ui, |ui| {
                        ui.selectable_value(&mut m.macro_idx, None, tr("Current recorded macro"));
                        for (i, x) in self.s.macros.iter().enumerate() {
                            ui.selectable_value(&mut m.macro_idx, Some(i), &x.name);
                        }
                    });
                });
                ui.radio_value(&mut m.until_end, false, "");
                ui.horizontal(|ui| {
                    ui.label(tr("Run"));
                    ui.add(TextEdit::singleline(&mut m.count).desired_width(60.0));
                    ui.label(tr("times"));
                });
                ui.radio_value(&mut m.until_end, true, tr("Run until the end of file"));
                run = ui.button(tr("Run")).clicked();
            });
            if run {
                let events = m.macro_idx.and_then(|i| self.s.macros.get(i)).map_or(self.macro_.last.clone(), |x| x.events.clone());
                let chunks = crate::app::macro_chunks(&events);
                if m.until_end {
                    self.macro_.until_end = Some((chunks, usize::MAX));
                } else if let Ok(n) = m.count.trim().parse::<usize>() {
                    for _ in 0..n.min(100_000) {
                        self.macro_.queue.extend(chunks.iter().cloned());
                    }
                }
            } else if open && !esc_pressed(ctx) {
                self.dlg.macro_multi = Some(m);
            }
        }

        // Windows dialog
        if let Some(mut w) = self.dlg.windows.take() {
            let mut open = true;
            let mut act: Option<&str> = None;
            let mut rows: Vec<(u64, String, String, String, usize, bool)> = self
                .docs
                .iter()
                .map(|d| {
                    let ty = d.syntax.clone().unwrap_or_else(|| tr("Normal text").into());
                    (d.id, d.title(), d.path.as_ref().and_then(|p| p.parent()).map(|p| p.display().to_string()).unwrap_or_default(), ty, d.text().len(), d.is_dirty())
                })
                .collect();
            match w.sort {
                1 => rows.sort_by_key(|r| r.1.to_lowercase()),
                2 => rows.sort_by_key(|r| r.2.to_lowercase()),
                3 => rows.sort_by_key(|r| r.3.to_lowercase()),
                4 => rows.sort_by_key(|r| r.4),
                _ => {}
            }
            egui::Window::new(tr("Windows")).open(&mut open).collapsible(false).default_width(700.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        egui::Grid::new("windows").striped(true).num_columns(4).show(ui, |ui| {
                            for (i, h) in ["Name", "Path", "Type", "Size"].iter().enumerate() {
                                if ui.button(RichText::new(tr(h)).strong()).clicked() {
                                    w.sort = i as u8 + 1;
                                }
                            }
                            ui.end_row();
                            for (id, name, path, ty, size, dirty) in &rows {
                                let sel = w.selected.contains(id);
                                if ui.selectable_label(sel, format!("{}{name}", if *dirty { "*" } else { "" })).clicked() {
                                    if ui.input(|i| i.modifiers.ctrl) {
                                        if !w.selected.remove(id) {
                                            w.selected.insert(*id);
                                        }
                                    } else {
                                        w.selected = [*id].into();
                                    }
                                }
                                ui.label(path);
                                ui.label(ty);
                                ui.label(fmt_num(*size));
                                ui.end_row();
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        for (a, l) in [("activate", "Activate"), ("save", "Save"), ("close", "Close window(s)")] {
                            if ui.add_sized([140.0, 22.0], Button::new(tr(l))).clicked() {
                                act = Some(a);
                            }
                        }
                    });
                });
            });
            match act {
                Some("activate") => {
                    if let Some(&id) = w.selected.iter().next() {
                        self.activate_id(id);
                    }
                }
                Some("save") => {
                    for &id in &w.selected {
                        self.save(id);
                    }
                }
                Some("close") => {
                    let ids: Vec<u64> = w.selected.iter().copied().collect();
                    self.request_close(ids);
                    w.selected.clear();
                }
                _ => {}
            }
            if open && !esc_pressed(ctx) {
                self.dlg.windows = Some(w);
            }
        }

        self.update_ui(ctx);

        // About / command line / debug
        if self.dlg.about {
            let mut open = true;
            egui::Window::new(trf("About {}", &[&APP_NAME])).open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading(format!("{APP_NAME} {}", env!("CARGO_PKG_VERSION")));
                    ui.label(tr("A Notepad++ style text and source code editor for Linux, written in Rust."));
                    ui.add_space(6.0);
                    ui.label(trf("{} languages with syntax highlighting, {} colour themes.", &[&self.hl.syntax_names().len(), &self.hl.theme_names().len()]));
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr("UI: egui/eframe · Highlighting: syntect with the syntax definitions of bat (two-face)")).small());
                    ui.label(RichText::new(tr("Independent re-implementation – no code from Notepad++ was used.")).small());
                    ui.add_space(6.0);
                    ui.label(env!("CARGO_PKG_AUTHORS"));
                });
            });
            self.dlg.about = open && !esc_pressed(ctx);
        }
        if self.dlg.cmdline {
            let mut open = true;
            egui::Window::new(tr("Command Line Arguments")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.monospace(CMDLINE_HELP);
            });
            self.dlg.cmdline = open && !esc_pressed(ctx);
        }
        if self.dlg.debug {
            let mut open = true;
            let info = self.debug_info();
            egui::Window::new(tr("Debug Info")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.monospace(&info);
                if ui.button(tr("Copy debug info into clipboard")).clicked() {
                    ui.ctx().copy_text(info.clone());
                }
            });
            self.dlg.debug = open && !esc_pressed(ctx);
        }
    }

    fn update_ui(&mut self, ctx: &egui::Context) {
        if let Some(mut proxy) = self.dlg.update_proxy.take() {
            let mut open = true;
            let mut done = false;
            egui::Window::new(tr("Set Updater Proxy")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(tr("Proxy for update checks and downloads (empty = none):"));
                let r = ui.add(egui::TextEdit::singleline(&mut proxy).hint_text("http://proxy.example:3128").desired_width(320.0));
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                        self.s.update_proxy = proxy.trim().to_string();
                        done = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        done = true;
                    }
                });
            });
            if open && !done && !esc_pressed(ctx) {
                self.dlg.update_proxy = Some(proxy);
            }
        }
        let Some(dlg) = self.update.take() else { return };
        let mut open = true;
        let mut keep = true;
        let mut action: Option<u8> = None; // 1 install, 2 release page, 3 skip, 4 restart
        let current = env!("CARGO_PKG_VERSION");
        egui::Window::new(trf("Update {}", &[&APP_NAME])).open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.set_min_width(380.0);
            match &dlg.state {
                UpdateState::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Checking for updates…"));
                    });
                }
                UpdateState::Installing => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Downloading and verifying the update…"));
                    });
                }
                UpdateState::UpToDate => {
                    ui.label(trf("{} {} is up to date.", &[&APP_NAME, &current]));
                    if ui.button(tr("OK")).clicked() {
                        keep = false;
                    }
                }
                UpdateState::Available(rel) => {
                    ui.label(RichText::new(trf("{} {} is available (installed: {}).", &[&APP_NAME, &rel.version, &current])).strong());
                    if !rel.notes.trim().is_empty() {
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            ui.label(rel.notes.trim());
                        });
                    }
                    let appimage = crate::update::running_appimage();
                    if appimage.is_none() {
                        ui.label(RichText::new(tr("Not started from an AppImage: download the new version from the release page.")).weak());
                    } else if rel.appimage.is_none() {
                        ui.label(RichText::new(tr("The release has no AppImage for this system.")).weak());
                    }
                    ui.horizontal(|ui| {
                        if appimage.is_some() && rel.appimage.is_some() && ui.button(tr("Update now")).clicked() {
                            action = Some(1);
                        }
                        if ui.button(tr("Open release page")).clicked() {
                            action = Some(2);
                        }
                        if ui.button(tr("Later")).clicked() {
                            keep = false;
                        }
                        if ui.button(tr("Skip this version")).clicked() {
                            action = Some(3);
                        }
                    });
                }
                UpdateState::Installed(p) => {
                    ui.label(tr("The update has been installed. It takes effect after a restart."));
                    ui.label(RichText::new(p.display().to_string()).weak());
                    ui.horizontal(|ui| {
                        if ui.button(tr("Restart now")).clicked() {
                            action = Some(4);
                        }
                        if ui.button(tr("Later")).clicked() {
                            keep = false;
                        }
                    });
                }
                UpdateState::Error(e) => {
                    ui.label(RichText::new(tr("The update failed:")).strong());
                    ui.label(e);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Open release page")).clicked() {
                            action = Some(2);
                        }
                        if ui.button(tr("OK")).clicked() {
                            keep = false;
                        }
                    });
                }
            }
        });
        let busy = matches!(dlg.state, UpdateState::Checking | UpdateState::Installing);
        if !busy && esc_pressed(ctx) {
            keep = false;
        }
        match (action, &dlg.state) {
            (Some(1), UpdateState::Available(rel)) => {
                let rel = rel.clone();
                self.start_update_install(ctx, rel);
                return;
            }
            (Some(2), UpdateState::Available(rel)) => {
                let _ = tools::open_with_default(&rel.page);
                keep = false;
            }
            (Some(2), _) => {
                let _ = tools::open_with_default(&crate::update::releases_page());
                keep = false;
            }
            (Some(3), UpdateState::Available(rel)) => {
                self.s.update_skip = rel.version.clone();
                keep = false;
            }
            (Some(4), UpdateState::Installed(p)) => {
                self.restart_exe = Some(p.clone());
                self.explicit_exit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                keep = false;
            }
            _ => {}
        }
        // A running check/download continues in the background when the window is closed.
        if open && keep {
            self.update = Some(dlg);
        }
    }

    fn debug_info(&self) -> String {
        let os = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|s| s.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| l[12..].trim_matches('"').to_string()))
            .unwrap_or_else(|| std::env::consts::OS.into());
        format!(
            "{APP_NAME} v{}\nBuild: Rust, egui 0.29\nPath: {}\nConfig: {}\nOS: {os}\nSession type: {}\nLocale: {}\nSyntaxes: {}\nThemes: {}\nUser defined languages: {}\nScript plugins: {}",
            env!("CARGO_PKG_VERSION"),
            std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
            crate::settings::config_dir().display(),
            std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
            std::env::var("LANG").unwrap_or_default(),
            self.hl.syntax_names().len(),
            self.hl.theme_names().len(),
            self.udls.len(),
            tools::script_plugins().len(),
        )
    }

    fn apply_column_editor(&mut self, c: &ColumnDlg) {
        let (pane, id) = (self.cur, self.active_id());
        let c_text = c.text.clone();
        let text_mode = c.text_mode;
        let initial: i64 = c.initial.trim().parse().unwrap_or(1);
        let step: i64 = c.step.trim().parse().unwrap_or(1);
        let repeat: usize = c.repeat.trim().parse().unwrap_or(1).max(1);
        let format = c.format;
        let leading = c.leading;
        self.with_view(pane, id, move |app| {
            let d = app.doc_mut();
            if d.read_only {
                return;
            }
            let sels = d.all_sels();
            // Lines and column: from the selections (column mode) or caret line to selection end.
            let mut lines: Vec<usize> = sels.iter().map(|&(_, cpos)| d.line_of(cpos)).collect();
            lines.sort();
            lines.dedup();
            let col = sels.iter().map(|&(a, cp)| d.line_col(a.min(cp)).1).min().unwrap_or(0);
            let range = if lines.len() > 1 {
                lines[0]..lines[lines.len() - 1] + 1
            } else {
                let s = d.sel();
                d.line_of(s.start)..d.line_of(s.end) + 1
            };
            let width = if text_mode {
                0
            } else {
                (0..range.len()).map(|i| textops::column_number(i, initial, step, repeat, format, 0, ' ').len()).max().unwrap_or(0)
            };
            let pad = match leading {
                1 => '0',
                _ => ' ',
            };
            let item = |i: usize| {
                if text_mode {
                    c_text.clone()
                } else if leading == 0 {
                    textops::column_number(i, initial, step, repeat, format, 0, ' ')
                } else {
                    textops::column_number(i, initial, step, repeat, format, width, pad)
                }
            };
            if let Some(new) = textops::column_insert(d.text(), range.clone(), col, item) {
                let start = d.line_start(range.start);
                d.set_text(new, start..start);
            }
        });
    }
}

pub const CMDLINE_HELP: &str = "notepad-rs [--help] [--new-instance] [-multiInst] [-nosession] [-ro]
           [-n<line>] [-c<column>] [-l<language>] [-udl=\"<name>\"]
           [-alwaysOnTop] [-systemtray] [-notabbar] [-monitor] [-openSession <file>]
           [-r] [file …]

  --help              this help
  --new-instance,
  -multiInst          start a new window instead of using the running one
  -nosession          don't load/save the previous session
  -ro                 open the files read-only
  -n<line>            go to line
  -c<column>          go to column
  -l<language>        language of the opened files (e.g. -lPython)
  -udl=\"<name>\"       user defined language of the opened files
  -alwaysOnTop        keep the window on top
  -systemtray         start minimized to the system tray
  -notabbar           hide the tab bar
  -monitor            open the files with monitoring (tail -f)
  -openSession <file> load a session file
  -r                  open files recursively (with wildcards: -r \"src/*.rs\")
  file                files to open (wildcards allowed)";

#[allow(dead_code)]
fn unused(_: PathBuf) {}
