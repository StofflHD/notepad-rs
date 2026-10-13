//! Preferences, Style Configurator, Shortcut Mapper, context menu editor, macro/run command
//! management and the User Defined Language editor.

use eframe::egui::{self, vec2, Button, Color32, Key, RichText, TextEdit, Ui};

use crate::app::{App, Pick};
use crate::cmd::{self, Cmd};
use crate::doc::Eol;
use crate::i18n::{tr, trf};
use crate::settings::{BackupMode, Completion, CurrentLine, EdgeMode, OpenDir, Rgba, Styles, TabSetting};
use crate::udl::{UserLang, STYLE_NAMES};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PrefPage {
    #[default]
    General,
    Editing,
    Margins,
    NewDocument,
    DefaultDirectory,
    RecentFiles,
    Language,
    Highlighting,
    Print,
    Searching,
    Backup,
    AutoCompletion,
    MultiInstance,
    Delimiter,
    Performance,
    CloudLink,
    SearchEngine,
    Misc,
    FileAssociation,
    ContextMenu,
    Macros,
    RunCommands,
}

const PAGES: &[(PrefPage, &str)] = &[
    (PrefPage::General, "General"),
    (PrefPage::Editing, "Editing"),
    (PrefPage::Margins, "Margins/Border/Edge"),
    (PrefPage::NewDocument, "New Document"),
    (PrefPage::DefaultDirectory, "Default Directory"),
    (PrefPage::RecentFiles, "Recent Files History"),
    (PrefPage::Language, "Language"),
    (PrefPage::Highlighting, "Highlighting"),
    (PrefPage::Print, "Print"),
    (PrefPage::Searching, "Searching"),
    (PrefPage::Backup, "Backup"),
    (PrefPage::AutoCompletion, "Auto-Completion"),
    (PrefPage::MultiInstance, "Multi-Instance & Date"),
    (PrefPage::Delimiter, "Delimiter"),
    (PrefPage::Performance, "Performance"),
    (PrefPage::CloudLink, "Cloud & Link"),
    (PrefPage::SearchEngine, "Search Engine"),
    (PrefPage::Misc, "MISC."),
    (PrefPage::FileAssociation, "File Association"),
    (PrefPage::ContextMenu, "Context Menu"),
    (PrefPage::Macros, "Macros"),
    (PrefPage::RunCommands, "Run Commands"),
];

#[derive(Default)]
pub struct PrefsState {
    pub open: bool,
    pub page: PrefPage,
    pub style_open: bool,
    pub shortcuts_open: bool,
    pub udl_open: bool,
    sc_filter: String,
    /// Shortcut being edited: (id, new value)
    sc_edit: Option<(String, String)>,
    udl_sel: usize,
    udl_new: String,
    cm_filter: String,
    lang_sel: String,
    /// File association page: selected MIME types (loaded when the page is shown) and result.
    assoc_sel: Option<std::collections::BTreeSet<String>>,
    assoc_msg: String,
}

impl PrefsState {
    pub fn open_at(&mut self, p: PrefPage) {
        self.open = true;
        self.page = p;
    }
}

fn color_edit(ui: &mut Ui, label: &str, c: &mut Rgba) -> bool {
    ui.label(tr(label));
    let mut col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    let r = ui.color_edit_button_srgba(&mut col).changed();
    if r {
        *c = col.to_srgba_unmultiplied();
    }
    ui.end_row();
    r
}

fn opt_color_edit(ui: &mut Ui, label: &str, c: &mut Option<Rgba>, default: Color32) {
    ui.label(tr(label));
    ui.horizontal(|ui| {
        let mut on = c.is_some();
        if ui.checkbox(&mut on, tr("custom")).changed() {
            *c = if on { Some(default.to_srgba_unmultiplied()) } else { None };
        }
        if let Some(v) = c {
            let mut col = Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3]);
            if ui.color_edit_button_srgba(&mut col).changed() {
                *v = col.to_srgba_unmultiplied();
            }
        }
    });
    ui.end_row();
}

impl App {
    pub fn prefs_ui(&mut self, ctx: &egui::Context) {
        self.preferences_window(ctx);
        self.style_window(ctx);
        self.shortcut_window(ctx);
        self.udl_window(ctx);
    }

    fn preferences_window(&mut self, ctx: &egui::Context) {
        if !self.prefs.open {
            return;
        }
        let mut open = true;
        egui::Window::new(tr("Preferences")).open(&mut open).collapsible(false).default_size(vec2(760.0, 520.0)).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    egui::ScrollArea::vertical().id_salt("prefpages").max_height(480.0).show(ui, |ui| {
                        for (p, l) in PAGES {
                            if ui.selectable_label(self.prefs.page == *p, tr(l)).clicked() {
                                self.prefs.page = *p;
                            }
                        }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().id_salt("prefbody").max_height(480.0).show(ui, |ui| {
                        ui.set_min_width(520.0);
                        self.pref_page(ui, ctx);
                    });
                });
            });
        });
        if !open || (ctx.input(|i| i.key_pressed(Key::Escape)) && !ctx.memory(|m| m.focused().is_some())) {
            self.prefs.open = false;
        }
    }

    fn pref_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut pick_req: Option<Pick> = None;
        self.pref_page_inner(ui, ctx, &mut pick_req);
        if let Some(p) = pick_req {
            self.pick(ctx, p, true, false, None, None);
        }
    }

    fn pref_page_inner(&mut self, ui: &mut Ui, ctx: &egui::Context, pick_req: &mut Option<Pick>) {
        let s = &mut self.s;
        match self.prefs.page {
            PrefPage::General => {
                ui.heading(tr("General"));
                ui.horizontal(|ui| {
                    ui.label(tr("Localization"));
                    egui::ComboBox::from_id_salt("lang").selected_text(crate::i18n::LANGUAGES.iter().find(|(c, _)| *c == s.ui_lang).map_or("English", |l| l.1)).show_ui(ui, |ui| {
                        for (code, name) in crate::i18n::LANGUAGES {
                            if ui.selectable_value(&mut s.ui_lang, code.to_string(), *name).changed() {
                                crate::i18n::set_language(code);
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                ui.checkbox(&mut s.show_toolbar, tr("Show toolbar"));
                ui.checkbox(&mut s.show_status_bar, tr("Show status bar"));
                ui.checkbox(&mut s.show_menu_bar, tr("Show menu bar"));
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Tab Bar")).strong());
                ui.checkbox(&mut s.show_tab_bar, tr("Show tab bar"));
                ui.checkbox(&mut s.tabs_multiline, tr("Multi-line"));
                ui.checkbox(&mut s.tabs_vertical, tr("Vertical"));
                ui.checkbox(&mut s.tab_close_button, tr("Show close button on each tab"));
                ui.checkbox(&mut s.tab_double_click_close, tr("Double click to close document"));
                ui.horizontal(|ui| {
                    ui.label(tr("Max. tab width"));
                    ui.add(egui::DragValue::new(&mut s.tab_max_width).range(60.0..=600.0));
                });
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Dark Mode")).strong());
                if ui.checkbox(&mut s.dark_mode, tr("Enable dark mode")).changed() {
                    ctx.set_visuals(if s.dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() });
                }
                ui.horizontal(|ui| {
                    ui.label(tr("UI scale"));
                    if ui.add(egui::Slider::new(&mut s.ui_scale, 0.75..=2.0).step_by(0.05)).changed() {
                        ctx.set_zoom_factor(s.ui_scale);
                    }
                });
            }
            PrefPage::Editing => {
                ui.heading(tr("Editing"));
                egui::Grid::new("caret").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Caret width"));
                    ui.add(egui::Slider::new(&mut s.caret_width, 1.0..=4.0).step_by(1.0));
                    ui.end_row();
                    ui.label(tr("Current line indicator"));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut s.current_line, CurrentLine::None, tr("None"));
                        ui.radio_value(&mut s.current_line, CurrentLine::Highlight, tr("Highlight Background"));
                        ui.radio_value(&mut s.current_line, CurrentLine::Frame, tr("Frame"));
                    });
                    ui.end_row();
                });
                ui.checkbox(&mut s.caret_blink, tr("Blinking caret"));
                ui.checkbox(&mut s.multi_editing, tr("Enable Multi-Editing (Ctrl+Mouse click/selection)"));
                ui.checkbox(&mut s.scroll_beyond_last_line, tr("Enable scrolling beyond last line"));
                ui.checkbox(&mut s.copy_line_without_selection, tr("Copy (Cut) the current line when there is no selection"));
                ui.checkbox(&mut s.word_wrap, tr("Word wrap"));
                ui.checkbox(&mut s.show_wrap_symbol, tr("Show wrap symbol"));
                ui.checkbox(&mut s.show_indent_guides, tr("Show indent guides"));
                ui.checkbox(&mut s.show_ctrl_chars, tr("Show control characters (NUL, ESC …)"));
                ui.checkbox(&mut s.show_non_printing, tr("Show non-printing characters (NBSP, zero width …)"));
                ui.checkbox(&mut s.auto_indent, tr("Auto-indent"));
            }
            PrefPage::Margins => {
                ui.heading(tr("Margins/Border/Edge"));
                ui.checkbox(&mut s.line_numbers, tr("Display line number"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.line_numbers_dynamic, true, tr("Dynamic width"));
                    ui.radio_value(&mut s.line_numbers_dynamic, false, tr("Constant width"));
                });
                ui.checkbox(&mut s.bookmark_margin, tr("Display bookmark"));
                ui.checkbox(&mut s.change_history, tr("Display change history"));
                ui.checkbox(&mut s.fold_margin, tr("Display folding margin"));
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Vertical Edge Settings")).strong());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.edge_mode, EdgeMode::None, tr("None"));
                    ui.radio_value(&mut s.edge_mode, EdgeMode::Line, tr("Line mode"));
                    ui.radio_value(&mut s.edge_mode, EdgeMode::Background, tr("Background mode"));
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Column"));
                    ui.add(egui::DragValue::new(&mut s.edge_column).range(1..=500));
                });
            }
            PrefPage::NewDocument => {
                ui.heading(tr("New Document"));
                ui.label(RichText::new(tr("Format (Line ending)")).strong());
                ui.horizontal(|ui| {
                    for eol in [Eol::Crlf, Eol::Lf, Eol::Cr] {
                        ui.radio_value(&mut s.default_eol, eol, eol.label());
                    }
                });
                ui.label(RichText::new(tr("Encoding")).strong());
                ui.horizontal_wrapped(|ui| {
                    for (label, enc) in App::encodings_main() {
                        let sel = s.default_encoding == enc;
                        if ui.radio(sel, label).clicked() {
                            s.default_encoding = enc;
                        }
                    }
                });
                ui.label(RichText::new(tr("Default language")).strong());
                let langs = self.hl.syntax_names();
                egui::ComboBox::from_id_salt("deflang").selected_text(s.default_lang.clone().unwrap_or_else(|| tr("Normal Text").into())).height(400.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.default_lang, None, tr("Normal Text"));
                    for l in langs {
                        ui.selectable_value(&mut s.default_lang, Some(l.clone()), l);
                    }
                });
            }
            PrefPage::DefaultDirectory => {
                ui.heading(tr("Default Directory"));
                ui.radio_value(&mut s.open_dir, OpenDir::FollowCurrent, tr("Follow current document"));
                ui.radio_value(&mut s.open_dir, OpenDir::RememberLast, tr("Remember last used directory"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.open_dir, OpenDir::Custom, "");
                    ui.text_edit_singleline(&mut s.custom_dir);
                    if ui.button("…").clicked() {
                        *pick_req = Some(Pick::DefaultDir);
                    }
                });
            }
            PrefPage::RecentFiles => {
                ui.heading(tr("Recent Files History"));
                ui.horizontal(|ui| {
                    ui.label(tr("Max. number of entries"));
                    ui.add(egui::DragValue::new(&mut s.max_recent).range(0..=50));
                });
                ui.checkbox(&mut s.recent_full_path, tr("Display full file path"));
            }
            PrefPage::Language => {
                ui.heading(tr("Tab Settings"));
                ui.horizontal(|ui| {
                    ui.label(tr("Default tab size"));
                    ui.add(egui::DragValue::new(&mut s.tab_width).range(1..=16));
                    ui.checkbox(&mut s.use_spaces, tr("Replace by space"));
                });
                ui.add_space(6.0);
                ui.label(tr("Per language (overrides the default):"));
                let names = self.hl.syntax_names();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("tablang").selected_text(if self.prefs.lang_sel.is_empty() { tr("Choose…").to_string() } else { self.prefs.lang_sel.clone() }).height(400.0).show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(&mut self.prefs.lang_sel, n.clone(), n);
                        }
                    });
                    if !self.prefs.lang_sel.is_empty() {
                        let key = self.prefs.lang_sel.clone();
                        let mut on = s.lang_tabs.contains_key(&key);
                        if ui.checkbox(&mut on, tr("Use own setting")).changed() {
                            if on {
                                s.lang_tabs.insert(key.clone(), TabSetting { width: s.tab_width, use_spaces: s.use_spaces });
                            } else {
                                s.lang_tabs.remove(&key);
                            }
                        }
                        if let Some(t) = s.lang_tabs.get_mut(&key) {
                            ui.add(egui::DragValue::new(&mut t.width).range(1..=16));
                            ui.checkbox(&mut t.use_spaces, tr("Replace by space"));
                        }
                    }
                });
                for (k, t) in s.lang_tabs.clone() {
                    ui.label(format!("{k}: {} {}", t.width, if t.use_spaces { tr("spaces") } else { tr("tabs") }));
                }
            }
            PrefPage::Highlighting => {
                ui.heading(tr("Highlighting"));
                ui.checkbox(&mut s.smart_highlight, tr("Enable Smart Highlighting"));
                ui.checkbox(&mut s.smart_match_case, tr("Match case"));
                ui.checkbox(&mut s.smart_whole_word, tr("Match whole word only"));
                ui.add_space(6.0);
                ui.checkbox(&mut s.brace_match, tr("Highlight matching braces"));
                ui.checkbox(&mut s.tag_match, tr("Highlight matching tags (HTML/XML)"));
            }
            PrefPage::Print => {
                ui.heading(tr("Print"));
                ui.checkbox(&mut s.print_line_numbers, tr("Print line number"));
                ui.checkbox(&mut s.print_colors, tr("Colour (syntax highlighting)"));
                egui::Grid::new("print").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Font size"));
                    ui.add(egui::DragValue::new(&mut s.print_font_size).range(5.0..=20.0));
                    ui.end_row();
                    ui.label(tr("Header"));
                    ui.text_edit_singleline(&mut s.print_header);
                    ui.end_row();
                    ui.label(tr("Footer"));
                    ui.text_edit_singleline(&mut s.print_footer);
                    ui.end_row();
                    ui.label(tr("Print command"));
                    ui.text_edit_singleline(&mut s.print_command);
                    ui.end_row();
                });
                ui.label(RichText::new(tr("Variables: $(FULL_CURRENT_PATH), $(FILE_NAME), $(PAGE), $(PAGES)")).weak());
            }
            PrefPage::Searching => {
                ui.heading(tr("Searching"));
                ui.checkbox(&mut s.fill_find_with_selection, tr("Fill Find Field with Selected Text"));
                ui.checkbox(&mut s.confirm_replace_all_docs, tr("Confirm Replace All in All Opened Documents"));
            }
            PrefPage::Backup => {
                ui.heading(tr("Session snapshot & periodic backup"));
                ui.checkbox(&mut s.remember_session, tr("Remember current session (incl. unsaved changes) for next launch"));
                ui.horizontal(|ui| {
                    ui.label(tr("Backup every"));
                    ui.add(egui::DragValue::new(&mut s.backup_interval_secs).range(1..=3600));
                    ui.label(tr("seconds"));
                });
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Backup on save")).strong());
                ui.radio_value(&mut s.backup_on_save, BackupMode::None, tr("None"));
                ui.radio_value(&mut s.backup_on_save, BackupMode::Simple, tr("Simple backup (file.ext.bak)"));
                ui.radio_value(&mut s.backup_on_save, BackupMode::Verbose, tr("Verbose backup (timestamped copies)"));
                ui.horizontal(|ui| {
                    ui.label(tr("Directory"));
                    ui.text_edit_singleline(&mut s.backup_dir);
                    if ui.button("…").clicked() {
                        *pick_req = Some(Pick::BackupDir);
                    }
                });
            }
            PrefPage::AutoCompletion => {
                ui.heading(tr("Auto-Completion"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.completion, Completion::Off, tr("Off"));
                    ui.radio_value(&mut s.completion, Completion::Functions, tr("Function completion"));
                    ui.radio_value(&mut s.completion, Completion::Words, tr("Word completion"));
                    ui.radio_value(&mut s.completion, Completion::Both, tr("Function and word completion"));
                });
                ui.horizontal(|ui| {
                    ui.label(tr("From"));
                    ui.add(egui::DragValue::new(&mut s.completion_from).range(1..=9));
                    ui.label(tr("th character"));
                });
                ui.checkbox(&mut s.completion_ignore_numbers, tr("Ignore numbers"));
                ui.checkbox(&mut s.calltip_on_input, tr("Function parameters hint on input"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("API files (Notepad++ format):")).weak());
                    if ui.link(crate::api::dir().display().to_string()).clicked() {
                        let d = crate::api::dir();
                        let _ = std::fs::create_dir_all(&d);
                        let _ = crate::tools::open_with_default(&d.to_string_lossy());
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Auto-Insert")).strong());
                ui.horizontal(|ui| {
                    ui.checkbox(&mut s.auto_close_parens, "( )");
                    ui.checkbox(&mut s.auto_close_brackets, "[ ]");
                    ui.checkbox(&mut s.auto_close_braces, "{ }");
                    ui.checkbox(&mut s.auto_close_dquote, "\" \"");
                    ui.checkbox(&mut s.auto_close_squote, "' '");
                    ui.checkbox(&mut s.auto_close_tags, tr("html/xml close tag"));
                });
            }
            PrefPage::MultiInstance => {
                ui.heading(tr("Multi-Instance & Date"));
                ui.radio_value(&mut s.single_instance, true, tr("Mono-instance (default)"));
                ui.radio_value(&mut s.single_instance, false, tr("Always in multi-instance mode"));
                ui.label(RichText::new(tr("Takes effect at the next start.")).weak());
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Customize insert Date Time")).strong());
                ui.text_edit_singleline(&mut s.date_format);
                ui.label(crate::commands::format_custom_date(&s.date_format, chrono::Local::now()));
                ui.label(RichText::new("yyyy yy MMMM MMM MM dddd ddd dd HH hh mm ss tt").weak());
            }
            PrefPage::Delimiter => {
                ui.heading(tr("Word character list"));
                ui.label(tr("Additional characters that are part of a word (double-click, Ctrl+arrows, smart highlighting):"));
                ui.text_edit_singleline(&mut s.word_chars);
            }
            PrefPage::Performance => {
                ui.heading(tr("Large File Restriction"));
                ui.horizontal(|ui| {
                    ui.label(tr("Disable syntax highlighting above (MB)"));
                    ui.add(egui::DragValue::new(&mut s.highlight_limit_mb).range(0.1..=1024.0));
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Disable folding, smart highlighting, links above (MB)"));
                    ui.add(egui::DragValue::new(&mut s.large_file_mb).range(1.0..=2048.0));
                });
            }
            PrefPage::FileAssociation => {
                ui.heading(tr("File Association"));
                ui.label(tr("Open these file types with NotepadRS by default (desktop entry + mimeapps.list):"));
                let dirs = crate::assoc::Dirs::user();
                let sel = self.prefs.assoc_sel.get_or_insert_with(|| crate::assoc::current(&dirs));
                for (group, items) in crate::assoc::GROUPS {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr(group)).strong());
                        let all = items.iter().all(|(_, m)| sel.contains(*m));
                        if ui.small_button(if all { tr("None") } else { tr("All") }).clicked() {
                            for (_, m) in *items {
                                if all {
                                    sel.remove(*m);
                                } else {
                                    sel.insert(m.to_string());
                                }
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (label, m) in *items {
                            let mut on = sel.contains(*m);
                            if ui.checkbox(&mut on, *label).on_hover_text(*m).changed() {
                                if on {
                                    sel.insert(m.to_string());
                                } else {
                                    sel.remove(*m);
                                }
                            }
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Apply")).clicked() {
                        self.prefs.assoc_msg = match crate::assoc::apply(&dirs, sel) {
                            Ok(()) => trf("Done: {} file types are opened with NotepadRS.", &[&sel.len()]),
                            Err(e) => e,
                        };
                    }
                    if ui.button(tr("Reload")).clicked() {
                        *sel = crate::assoc::current(&dirs);
                        self.prefs.assoc_msg.clear();
                    }
                });
                if !self.prefs.assoc_msg.is_empty() {
                    ui.label(&self.prefs.assoc_msg);
                }
                ui.label(RichText::new(trf("Program: {}", &[&crate::assoc::exe().display()])).weak());
            }
            PrefPage::CloudLink => {
                ui.heading(tr("Clickable Link Settings"));
                ui.checkbox(&mut s.clickable_links, tr("Enable (double-click opens the link)"));
            }
            PrefPage::SearchEngine => {
                ui.heading(tr("Search Engine (used by \"Search on Internet\")"));
                for (n, u) in [
                    ("DuckDuckGo", "https://duckduckgo.com/?q=$(CURRENT_WORD)"),
                    ("Google", "https://www.google.com/search?q=$(CURRENT_WORD)"),
                    ("Bing", "https://www.bing.com/search?q=$(CURRENT_WORD)"),
                    ("Startpage", "https://www.startpage.com/do/search?q=$(CURRENT_WORD)"),
                    ("Wikipedia", "https://en.wikipedia.org/wiki/Special:Search?search=$(CURRENT_WORD)"),
                ] {
                    if ui.radio(s.search_engine == u, n).clicked() {
                        s.search_engine = u.into();
                    }
                }
                ui.label(tr("Custom:"));
                ui.text_edit_singleline(&mut s.search_engine);
            }
            PrefPage::Misc => {
                ui.heading(tr("MISC."));
                ui.label(RichText::new(tr("File Status Auto-Detection")).strong());
                ui.checkbox(&mut s.file_autodetect, tr("Enable"));
                ui.checkbox(&mut s.auto_reload, tr("Update silently"));
                ui.checkbox(&mut s.autodetect_scroll_end, tr("Scroll to the last line after update"));
            }
            PrefPage::ContextMenu => {
                ui.heading(tr("Edit Popup ContextMenu"));
                ui.label(tr("Additional commands at the end of the editor's context menu:"));
                ui.text_edit_singleline(&mut self.prefs.cm_filter);
                let f = self.prefs.cm_filter.to_lowercase();
                egui::ScrollArea::vertical().id_salt("cm").max_height(380.0).show(ui, |ui| {
                    for e in cmd::registry() {
                        let label = tr(e.label);
                        if !f.is_empty() && !label.to_lowercase().contains(&f) {
                            continue;
                        }
                        let mut on = s.context_menu_extra.iter().any(|x| x == e.id);
                        if ui.checkbox(&mut on, label).changed() {
                            if on {
                                s.context_menu_extra.push(e.id.to_string());
                            } else {
                                s.context_menu_extra.retain(|x| x != e.id);
                            }
                        }
                    }
                });
            }
            PrefPage::Macros => {
                ui.heading(tr("Macros"));
                let mut remove = None;
                egui::Grid::new("macros").num_columns(3).striped(true).show(ui, |ui| {
                    for (i, m) in s.macros.iter_mut().enumerate() {
                        ui.text_edit_singleline(&mut m.name);
                        ui.add(TextEdit::singleline(&mut m.shortcut).desired_width(140.0).hint_text(tr("Shortcut")));
                        if ui.button(tr("Delete")).clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    s.macros.remove(i);
                }
                if s.macros.is_empty() {
                    ui.label(RichText::new(tr("Record a macro and use Macro → Save Currently Recorded Macro.")).weak());
                }
            }
            PrefPage::RunCommands => {
                ui.heading(tr("Run Commands"));
                let mut remove = None;
                egui::Grid::new("runs").num_columns(4).striped(true).show(ui, |ui| {
                    for (i, r) in s.run_commands.iter_mut().enumerate() {
                        ui.add(TextEdit::singleline(&mut r.name).desired_width(140.0));
                        ui.add(TextEdit::singleline(&mut r.command).desired_width(240.0));
                        ui.add(TextEdit::singleline(&mut r.shortcut).desired_width(100.0).hint_text(tr("Shortcut")));
                        if ui.button(tr("Delete")).clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    s.run_commands.remove(i);
                }
                if ui.button(tr("Add")).clicked() {
                    s.run_commands.push(Default::default());
                }
            }
        }
    }

    fn style_window(&mut self, ctx: &egui::Context) {
        if !self.prefs.style_open {
            return;
        }
        let mut open = true;
        let themes = self.hl.theme_names();
        egui::Window::new(tr("Style Configurator")).open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                let s = &mut self.s;
                egui::Grid::new("themes").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Theme (light mode)"));
                    egui::ComboBox::from_id_salt("tl").selected_text(&s.theme_light).height(400.0).show_ui(ui, |ui| {
                        for t in &themes {
                            ui.selectable_value(&mut s.theme_light, t.clone(), t);
                        }
                    });
                    ui.end_row();
                    ui.label(tr("Theme (dark mode)"));
                    egui::ComboBox::from_id_salt("td").selected_text(&s.theme_dark).height(400.0).show_ui(ui, |ui| {
                        for t in &themes {
                            ui.selectable_value(&mut s.theme_dark, t.clone(), t);
                        }
                    });
                    ui.end_row();
                    ui.label(tr("Font"));
                    let fonts = self.mono_fonts.get_or_insert_with(crate::fonts::monospace_fonts).clone();
                    let cur = s.font_path.as_ref().and_then(|p| fonts.iter().find(|f| &f.1 == p)).map_or(tr("Default (Hack)").to_string(), |f| f.0.clone());
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("font").selected_text(cur).height(400.0).show_ui(ui, |ui| {
                        if ui.selectable_label(s.font_path.is_none(), tr("Default (Hack)")).clicked() {
                            s.font_path = None;
                            changed = true;
                        }
                        for (n, p) in &fonts {
                            if ui.selectable_label(s.font_path.as_deref() == Some(p), n).clicked() {
                                s.font_path = Some(p.clone());
                                changed = true;
                            }
                        }
                    });
                    if changed {
                        crate::fonts::apply(ctx, s.font_path.as_deref());
                    }
                    ui.end_row();
                    ui.label(tr("Font size"));
                    ui.add(egui::DragValue::new(&mut s.font_size).range(6.0..=72.0).speed(0.2));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut s.styles.italic_comments, tr("Italic comments (if the theme marks them)"));
                    ui.end_row();
                });
                ui.separator();
                ui.label(RichText::new(tr("Global Styles")).strong());
                let theme = self.hl.theme(s.theme()).clone();
                let pal = crate::editor::Palette::new(&theme, &Styles::default_for(s));
                let st = &mut s.styles;
                egui::Grid::new("styles").num_columns(2).show(ui, |ui| {
                    opt_color_edit(ui, "Selected text colour", &mut st.selection, pal.selection);
                    opt_color_edit(ui, "Current line background colour", &mut st.current_line, pal.line_hl);
                    opt_color_edit(ui, "Caret colour", &mut st.caret, pal.caret);
                    opt_color_edit(ui, "Indent guideline style", &mut st.indent_guide, pal.guide);
                    opt_color_edit(ui, "Line number margin", &mut st.line_number, pal.gutter_fg);
                    color_edit(ui, "Smart Highlighting", &mut st.smart_highlight);
                    color_edit(ui, "Find Mark Style", &mut st.find_mark);
                    for i in 0..5 {
                        color_edit(ui, ["Mark Style 1", "Mark Style 2", "Mark Style 3", "Mark Style 4", "Mark Style 5"][i], &mut st.mark_styles[i]);
                    }
                    color_edit(ui, "Brace highlight style", &mut st.brace);
                    color_edit(ui, "Bookmark margin", &mut st.bookmark);
                    color_edit(ui, "Edge colour", &mut st.edge);
                    color_edit(ui, "Change History modified", &mut st.change_modified);
                    color_edit(ui, "Change History saved", &mut st.change_saved);
                    color_edit(ui, "URL hovered", &mut st.url);
                    for i in 0..5 {
                        color_edit(ui, ["Tab colour 1", "Tab colour 2", "Tab colour 3", "Tab colour 4", "Tab colour 5"][i], &mut st.tab_colors[i]);
                    }
                });
                if ui.button(tr("Reset to defaults")).clicked() {
                    s.styles = Styles::default();
                }
            });
        });
        if !open {
            self.prefs.style_open = false;
        }
    }

    fn shortcut_window(&mut self, ctx: &egui::Context) {
        if !self.prefs.shortcuts_open {
            return;
        }
        let mut open = true;
        // Capture a key combination while editing.
        if let Some((_, val)) = &mut self.prefs.sc_edit {
            ctx.input_mut(|i| {
                let mut taken = None;
                for e in &i.events {
                    if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                        if !matches!(key, Key::Escape) {
                            taken = Some(cmd::format_shortcut(*modifiers, *key));
                        }
                    }
                }
                if let Some(t) = taken {
                    *val = t;
                    i.events.clear();
                }
            });
        }
        let mut finish: Option<Option<String>> = None;
        let mut cancel_edit = false;
        egui::Window::new(tr("Shortcut Mapper")).open(&mut open).collapsible(false).default_size(vec2(720.0, 520.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Filter :"));
                ui.text_edit_singleline(&mut self.prefs.sc_filter);
            });
            ui.separator();
            let f = self.prefs.sc_filter.to_lowercase();
            // Conflicts
            let mut used: std::collections::HashMap<String, Vec<&str>> = Default::default();
            for e in cmd::registry() {
                let sc = cmd::shortcut_of(e, &self.s.shortcuts);
                if !sc.is_empty() {
                    used.entry(sc.to_string()).or_default().push(e.id);
                }
            }
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("scmap").num_columns(4).striped(true).show(ui, |ui| {
                    ui.strong(tr("Category"));
                    ui.strong(tr("Name"));
                    ui.strong(tr("Shortcut"));
                    ui.label("");
                    ui.end_row();
                    for e in cmd::registry() {
                        let label = tr(e.label);
                        let sc = cmd::shortcut_of(e, &self.s.shortcuts).to_string();
                        if !f.is_empty() && !label.to_lowercase().contains(&f) && !sc.to_lowercase().contains(&f) {
                            continue;
                        }
                        let cat = e.id.split('.').next().unwrap_or("");
                        ui.label(tr(cat));
                        ui.label(label);
                        let conflict = used.get(&sc).is_some_and(|v| v.len() > 1);
                        let t = if conflict { RichText::new(&sc).color(Color32::RED) } else { RichText::new(&sc) };
                        ui.label(t);
                        if ui.small_button(tr("Modify")).clicked() {
                            self.prefs.sc_edit = Some((e.id.to_string(), sc.clone()));
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some((id, val)) = &mut self.prefs.sc_edit {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(trf("Press the new shortcut for \"{}\":", &[&cmd::Cmd::from_id(id).map(|c| cmd::label(&c)).unwrap_or_default()]));
                    ui.strong(if val.is_empty() { tr("(none)") } else { val.as_str() });
                });
                if let Some(other) = used.get(val.as_str()).and_then(|v| v.iter().find(|x| **x != id.as_str())) {
                    ui.label(RichText::new(trf("Conflict with: {}", &[&Cmd::from_id(other).map(|c| cmd::label(&c)).unwrap_or_default()])).color(Color32::RED));
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() {
                        finish = Some(Some(val.clone()));
                    }
                    if ui.button(tr("Clear")).clicked() {
                        finish = Some(Some(String::new()));
                    }
                    if ui.button(tr("Reset to default")).clicked() {
                        finish = Some(None);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel_edit = true;
                    }
                });
            }
        });
        if cancel_edit {
            self.prefs.sc_edit = None;
        }
        if let Some(r) = finish {
            if let Some((id, _)) = self.prefs.sc_edit.take() {
                match r {
                    Some(v) => {
                        let default = cmd::registry().iter().find(|e| e.id == id).map_or("", |e| e.shortcut);
                        if v == default {
                            self.s.shortcuts.remove(&id);
                        } else {
                            self.s.shortcuts.insert(id, v);
                        }
                    }
                    None => {
                        self.s.shortcuts.remove(&id);
                    }
                }
            }
        }
        if !open {
            self.prefs.shortcuts_open = false;
            self.prefs.sc_edit = None;
        }
    }

    fn udl_window(&mut self, ctx: &egui::Context) {
        if !self.prefs.udl_open {
            return;
        }
        let mut open = true;
        let mut action: Option<&str> = None;
        egui::Window::new(tr("User Defined Language")).open(&mut open).collapsible(false).default_size(vec2(760.0, 560.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("User language:"));
                let cur = self.udls.get(self.prefs.udl_sel).map_or(tr("(none)").to_string(), |u| u.name.clone());
                egui::ComboBox::from_id_salt("udlsel").selected_text(cur).show_ui(ui, |ui| {
                    for (i, u) in self.udls.iter().enumerate() {
                        ui.selectable_value(&mut self.prefs.udl_sel, i, &u.name);
                    }
                });
                ui.add(TextEdit::singleline(&mut self.prefs.udl_new).desired_width(120.0).hint_text(tr("New name")));
                if ui.button(tr("Create New...")).clicked() {
                    action = Some("new");
                }
                if ui.button(tr("Save As...")).clicked() {
                    action = Some("copy");
                }
                if ui.button(tr("Remove")).clicked() {
                    action = Some("remove");
                }
                if ui.button(tr("Import...")).clicked() {
                    action = Some("import");
                }
                if ui.button(tr("Export...")).clicked() {
                    action = Some("export");
                }
            });
            ui.separator();
            let Some(l) = self.udls.get_mut(self.prefs.udl_sel) else {
                ui.label(tr("No user defined language yet. Enter a name and click \"Create New...\", or import a Notepad++ UDL XML file."));
                return;
            };
            let mut changed = false;
            egui::ScrollArea::vertical().max_height(470.0).show(ui, |ui| {
                egui::Grid::new("udlhead").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Ext. :"));
                    let mut ext = l.ext.join(" ");
                    if ui.text_edit_singleline(&mut ext).changed() {
                        l.ext = ext.split_whitespace().map(str::to_owned).collect();
                        changed = true;
                    }
                    ui.end_row();
                    ui.label("");
                    changed |= ui.checkbox(&mut l.case_ignored, tr("Ignore case")).changed();
                    ui.end_row();
                });
                ui.collapsing(tr("Keywords Lists"), |ui| {
                    for i in 0..8 {
                        ui.horizontal(|ui| {
                            ui.label(trf("{}st group", &[&(i + 1)]));
                            changed |= ui.add(TextEdit::multiline(&mut l.keywords[i]).desired_rows(2).desired_width(480.0)).changed();
                            style_button(ui, l, &format!("KEYWORDS{}", i + 1), &mut changed);
                        });
                    }
                });
                ui.collapsing(tr("Comment & Number"), |ui| {
                    egui::Grid::new("udlcom").num_columns(3).show(ui, |ui| {
                        ui.label(tr("Line comment open"));
                        let mut lc = l.line_comment.join(" ");
                        if ui.text_edit_singleline(&mut lc).changed() {
                            l.line_comment = lc.split_whitespace().map(str::to_owned).collect();
                            changed = true;
                        }
                        style_button(ui, l, "LINE COMMENTS", &mut changed);
                        ui.end_row();
                        let (mut o, mut c) = l.block_comment.first().cloned().unwrap_or_default();
                        ui.label(tr("Block comment open / close"));
                        let r1 = ui.add(TextEdit::singleline(&mut o).desired_width(80.0));
                        let r2 = ui.add(TextEdit::singleline(&mut c).desired_width(80.0));
                        if r1.changed() || r2.changed() {
                            l.block_comment = if o.is_empty() { Vec::new() } else { vec![(o, c)] };
                            changed = true;
                        }
                        style_button(ui, l, "COMMENTS", &mut changed);
                        ui.end_row();
                        ui.label(tr("Numbers"));
                        ui.label("");
                        style_button(ui, l, "NUMBERS", &mut changed);
                        ui.end_row();
                    });
                });
                ui.collapsing(tr("Operators & Delimiters"), |ui| {
                    egui::Grid::new("udlops").num_columns(3).show(ui, |ui| {
                        ui.label(tr("Operators 1"));
                        changed |= ui.text_edit_singleline(&mut l.operators1).changed();
                        style_button(ui, l, "OPERATORS", &mut changed);
                        ui.end_row();
                        ui.label(tr("Operators 2"));
                        changed |= ui.text_edit_singleline(&mut l.operators2).changed();
                        ui.label("");
                        ui.end_row();
                        ui.label(tr("Folding open / middle / close"));
                        ui.horizontal(|ui| {
                            changed |= ui.add(TextEdit::singleline(&mut l.fold_open).desired_width(80.0)).changed();
                            changed |= ui.add(TextEdit::singleline(&mut l.fold_middle).desired_width(80.0)).changed();
                            changed |= ui.add(TextEdit::singleline(&mut l.fold_close).desired_width(80.0)).changed();
                        });
                        style_button(ui, l, "FOLDER IN CODE1", &mut changed);
                        ui.end_row();
                        for k in 0..8 {
                            ui.label(trf("Delimiter {} open / escape / close", &[&(k + 1)]));
                            ui.horizontal(|ui| {
                                let d = &mut l.delimiters[k];
                                changed |= ui.add(TextEdit::singleline(&mut d.open).desired_width(60.0)).changed();
                                changed |= ui.add(TextEdit::singleline(&mut d.escape).desired_width(60.0)).changed();
                                changed |= ui.add(TextEdit::singleline(&mut d.close).desired_width(60.0)).changed();
                            });
                            style_button(ui, l, &format!("DELIMITERS{}", k + 1), &mut changed);
                            ui.end_row();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Default style"));
                    style_button(ui, l, "DEFAULT", &mut changed);
                });
            });
            if changed {
                let _ = crate::udl::save(l);
                for d in &mut self.docs {
                    d.hl.invalidate_from(0);
                }
            }
        });
        match action {
            Some("new") | Some("copy") => {
                let name = self.prefs.udl_new.trim().to_string();
                if !name.is_empty() && !self.udls.iter().any(|u| u.name == name) {
                    let mut l = if action == Some("copy") { self.udls.get(self.prefs.udl_sel).cloned().unwrap_or_default() } else { UserLang::default() };
                    l.name = name;
                    l.file = None;
                    let _ = crate::udl::save(&mut l);
                    self.udls.push(l);
                    self.prefs.udl_sel = self.udls.len() - 1;
                    self.prefs.udl_new.clear();
                }
            }
            Some("remove") => {
                if self.prefs.udl_sel < self.udls.len() {
                    let l = self.udls.remove(self.prefs.udl_sel);
                    if let Some(f) = l.file {
                        let _ = std::fs::remove_file(f);
                    }
                    self.prefs.udl_sel = 0;
                }
            }
            Some("import") => self.pick(ctx, Pick::UdlImport, false, false, None, Some(("Notepad++ UDL", vec!["xml"]))),
            Some("export") => {
                if let Some(l) = self.udls.get(self.prefs.udl_sel) {
                    let name = format!("{}.xml", l.name);
                    self.pick(ctx, Pick::UdlExport(self.prefs.udl_sel), false, true, Some(name), None);
                }
            }
            _ => {}
        }
        if !open {
            self.prefs.udl_open = false;
        }
    }
}

/// Colour/font style button for a UDL style.
fn style_button(ui: &mut Ui, l: &mut UserLang, name: &str, changed: &mut bool) {
    let st = l.styles.entry(name.to_string()).or_default();
    ui.menu_button(RichText::new("■ ").color(Color32::from_rgb(st.fg[0], st.fg[1], st.fg[2])).strong(), |ui| {
        ui.label(tr("Styler"));
        ui.horizontal(|ui| {
            ui.label(tr("Foreground"));
            let mut c = Color32::from_rgb(st.fg[0], st.fg[1], st.fg[2]);
            if ui.color_edit_button_srgba(&mut c).changed() {
                st.fg = [c.r(), c.g(), c.b()];
                *changed = true;
            }
        });
        ui.horizontal(|ui| {
            let mut on = st.bg.is_some();
            if ui.checkbox(&mut on, tr("Background")).changed() {
                st.bg = on.then_some([255, 255, 200]);
                *changed = true;
            }
            if let Some(b) = &mut st.bg {
                let mut c = Color32::from_rgb(b[0], b[1], b[2]);
                if ui.color_edit_button_srgba(&mut c).changed() {
                    *b = [c.r(), c.g(), c.b()];
                    *changed = true;
                }
            }
        });
        *changed |= ui.checkbox(&mut st.bold, tr("Bold")).changed();
        *changed |= ui.checkbox(&mut st.italic, tr("Italic")).changed();
        *changed |= ui.checkbox(&mut st.underline, tr("Underline")).changed();
    });
    let _ = STYLE_NAMES;
}

impl Styles {
    /// Styles without user colour overrides (to show the theme defaults).
    pub fn default_for(s: &crate::settings::Settings) -> crate::settings::Settings {
        let mut c = s.clone();
        c.styles.selection = None;
        c.styles.current_line = None;
        c.styles.caret = None;
        c.styles.indent_guide = None;
        c.styles.line_number = None;
        c
    }
}

#[allow(dead_code)]
fn unused(_: Button) {}
