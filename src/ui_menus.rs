//! Menu bar (same structure as Notepad++), toolbar, tab bars and status bar.

use eframe::egui::{self, pos2, vec2, Align, Align2, Button, Color32, FontId, Layout, RichText, Sense, Stroke, Ui};

use crate::app::{fmt_num, App};
use crate::cmd::{self, Cmd, Hash, Match};
use crate::doc::{Eol, TextEncoding};
use crate::i18n::{tr, trf};
use crate::textops::{Case, LineOp};

struct TabInfo {
    title: String,
    read_only: bool,
    pinned: bool,
    monitoring: bool,
    dirty: bool,
    tab_color: u8,
    path: Option<std::path::PathBuf>,
}

pub fn charset_groups() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        ("Arabic", vec![("ISO 8859-6", "ISO-8859-6"), ("Windows-1256", "windows-1256")]),
        ("Baltic", vec![("ISO 8859-4", "ISO-8859-4"), ("ISO 8859-13", "ISO-8859-13"), ("Windows-1257", "windows-1257")]),
        ("Celtic", vec![("ISO 8859-14", "ISO-8859-14")]),
        ("Cyrillic", vec![("ISO 8859-5", "ISO-8859-5"), ("KOI8-R", "KOI8-R"), ("KOI8-U", "KOI8-U"), ("Windows-1251", "windows-1251"), ("OEM 866", "IBM866"), ("Macintosh", "x-mac-cyrillic")]),
        ("Central European", vec![("ISO 8859-2", "ISO-8859-2"), ("Windows-1250", "windows-1250")]),
        ("Chinese", vec![("Big5 (Traditional)", "Big5"), ("GB2312 / GBK (Simplified)", "GBK"), ("GB18030", "gb18030")]),
        ("Eastern European", vec![("ISO 8859-10", "ISO-8859-10"), ("ISO 8859-16", "ISO-8859-16")]),
        ("Greek", vec![("ISO 8859-7", "ISO-8859-7"), ("Windows-1253", "windows-1253")]),
        ("Hebrew", vec![("ISO 8859-8", "ISO-8859-8"), ("Windows-1255", "windows-1255")]),
        ("Japanese", vec![("Shift-JIS", "Shift_JIS"), ("EUC-JP", "EUC-JP"), ("ISO-2022-JP", "ISO-2022-JP")]),
        ("Korean", vec![("EUC-KR / Windows 949", "EUC-KR")]),
        ("North European", vec![("ISO 8859-10", "ISO-8859-10")]),
        ("Thai", vec![("TIS-620 / Windows-874", "windows-874")]),
        ("Turkish", vec![("ISO 8859-3", "ISO-8859-3"), ("Windows-1254", "windows-1254")]),
        ("Western European", vec![("ISO 8859-1 / Windows-1252", "windows-1252"), ("ISO 8859-15", "ISO-8859-15"), ("Macintosh", "macintosh")]),
        ("Vietnamese", vec![("Windows-1258", "windows-1258")]),
    ]
}

fn canonical(label: &str) -> String {
    encoding_rs::Encoding::for_label(label.as_bytes()).map_or_else(|| label.to_string(), |e| e.name().to_string())
}

impl App {
    /// Menu item for a registered command (label and shortcut from the registry).
    fn mi(&self, ui: &mut Ui, cmds: &mut Vec<Cmd>, c: Cmd) {
        let label = cmd::label(&c);
        let sc = cmd::shortcut_text(&c, &self.s.shortcuts);
        if ui.add(Button::new(label).shortcut_text(sc)).clicked() {
            cmds.push(c);
            ui.close_menu();
        }
    }

    fn mi_en(&self, ui: &mut Ui, cmds: &mut Vec<Cmd>, c: Cmd, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| self.mi(ui, cmds, c));
    }

    fn mi_check(&self, ui: &mut Ui, cmds: &mut Vec<Cmd>, c: Cmd, checked: bool) {
        let label = format!("{} {}", if checked { "✔" } else { "   " }, cmd::label(&c));
        let sc = cmd::shortcut_text(&c, &self.s.shortcuts);
        if ui.add(Button::new(label).shortcut_text(sc)).clicked() {
            cmds.push(c);
            ui.close_menu();
        }
    }

    pub fn menu_bar(&mut self, ui: &mut Ui, cmds: &mut Vec<Cmd>) {
        let has_path = self.doc().path.is_some();
        let has_sel = self.doc().has_sel();
        egui::menu::bar(ui, |ui| {
            ui.menu_button(tr("File"), |ui| {
                self.mi(ui, cmds, Cmd::New);
                self.mi(ui, cmds, Cmd::Open);
                ui.menu_button(tr("Open Containing Folder"), |ui| {
                    self.mi_en(ui, cmds, Cmd::OpenContainingFolder, has_path);
                    self.mi(ui, cmds, Cmd::OpenTerminal);
                    self.mi_en(ui, cmds, Cmd::OpenFolderAsWorkspace, true);
                });
                self.mi_en(ui, cmds, Cmd::OpenInDefaultViewer, has_path);
                self.mi_en(ui, cmds, Cmd::Reload, has_path);
                self.mi(ui, cmds, Cmd::Save);
                self.mi(ui, cmds, Cmd::SaveAs);
                self.mi(ui, cmds, Cmd::SaveCopyAs);
                self.mi(ui, cmds, Cmd::SaveAll);
                self.mi(ui, cmds, Cmd::Rename);
                self.mi(ui, cmds, Cmd::Close);
                self.mi(ui, cmds, Cmd::CloseAll);
                ui.menu_button(tr("Close More"), |ui| {
                    self.mi(ui, cmds, Cmd::CloseAllButCurrent);
                    self.mi(ui, cmds, Cmd::CloseAllButPinned);
                    self.mi(ui, cmds, Cmd::CloseLeft);
                    self.mi(ui, cmds, Cmd::CloseRight);
                    self.mi(ui, cmds, Cmd::CloseUnchanged);
                });
                self.mi_en(ui, cmds, Cmd::MoveToTrash, has_path);
                ui.separator();
                self.mi(ui, cmds, Cmd::LoadSession);
                self.mi(ui, cmds, Cmd::SaveSession);
                ui.separator();
                self.mi(ui, cmds, Cmd::Print);
                self.mi(ui, cmds, Cmd::PrintNow);
                ui.separator();
                self.mi_en(ui, cmds, Cmd::ReopenClosed, !self.closed_paths.is_empty());
                self.mi(ui, cmds, Cmd::OpenAllRecent);
                self.mi(ui, cmds, Cmd::ClearRecent);
                if !self.s.recent.is_empty() {
                    ui.separator();
                    for (n, p) in self.s.recent.iter().enumerate() {
                        let label = if self.s.recent_full_path { p.clone() } else { std::path::Path::new(p).file_name().map_or(p.clone(), |f| f.to_string_lossy().into_owned()) };
                        if ui.button(format!("{}: {label}", n + 1)).clicked() {
                            cmds.push(Cmd::OpenPath(p.clone()));
                            ui.close_menu();
                        }
                    }
                }
                ui.separator();
                self.mi(ui, cmds, Cmd::Exit);
            });

            ui.menu_button(tr("Edit"), |ui| {
                self.mi_en(ui, cmds, Cmd::Undo, self.doc().can_undo());
                self.mi_en(ui, cmds, Cmd::Redo, self.doc().can_redo());
                ui.separator();
                self.mi(ui, cmds, Cmd::Cut);
                self.mi(ui, cmds, Cmd::Copy);
                self.mi(ui, cmds, Cmd::Paste);
                self.mi(ui, cmds, Cmd::Delete);
                self.mi(ui, cmds, Cmd::SelectAll);
                self.mi(ui, cmds, Cmd::BeginEndSelect);
                self.mi(ui, cmds, Cmd::BeginEndSelectColumn);
                ui.separator();
                ui.menu_button(tr("Insert"), |ui| {
                    self.mi(ui, cmds, Cmd::InsertDateShort);
                    self.mi(ui, cmds, Cmd::InsertDateLong);
                    self.mi(ui, cmds, Cmd::InsertDateCustom);
                });
                ui.menu_button(tr("Copy to Clipboard"), |ui| {
                    self.mi_en(ui, cmds, Cmd::CopyPath, has_path);
                    self.mi(ui, cmds, Cmd::CopyName);
                    self.mi_en(ui, cmds, Cmd::CopyDir, has_path);
                    ui.separator();
                    self.mi(ui, cmds, Cmd::CopyAllNames);
                    self.mi(ui, cmds, Cmd::CopyAllPaths);
                });
                ui.menu_button(tr("Indent"), |ui| {
                    self.mi(ui, cmds, Cmd::Indent);
                    self.mi(ui, cmds, Cmd::Unindent);
                });
                ui.menu_button(tr("Convert Case to"), |ui| {
                    for c in [Case::Upper, Case::Lower, Case::Proper, Case::ProperBlend, Case::Sentence, Case::SentenceBlend, Case::Invert, Case::Random] {
                        self.mi(ui, cmds, Cmd::Case(c));
                    }
                });
                ui.menu_button(tr("Line Operations"), |ui| {
                    self.mi(ui, cmds, Cmd::Duplicate);
                    self.mi(ui, cmds, Cmd::DeleteLine);
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::RemoveDuplicates));
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::RemoveConsecutiveDuplicates));
                    self.mi(ui, cmds, Cmd::SplitLines);
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::Join));
                    self.mi(ui, cmds, Cmd::MoveUp);
                    self.mi(ui, cmds, Cmd::MoveDown);
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::RemoveEmpty));
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::RemoveBlank));
                    self.mi(ui, cmds, Cmd::BlankAbove);
                    self.mi(ui, cmds, Cmd::BlankBelow);
                    ui.separator();
                    for op in [
                        LineOp::SortAsc,
                        LineOp::SortDesc,
                        LineOp::SortAscNoCase,
                        LineOp::SortDescNoCase,
                        LineOp::SortLocaleAsc,
                        LineOp::SortLocaleDesc,
                        LineOp::SortLengthAsc,
                        LineOp::SortLengthDesc,
                        LineOp::SortIntAsc,
                        LineOp::SortIntDesc,
                        LineOp::SortDecimalCommaAsc,
                        LineOp::SortDecimalCommaDesc,
                        LineOp::SortDecimalDotAsc,
                        LineOp::SortDecimalDotDesc,
                    ] {
                        self.mi(ui, cmds, Cmd::LineOp(op));
                    }
                    ui.separator();
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::Reverse));
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::Randomize));
                });
                ui.menu_button(tr("Comment/Uncomment"), |ui| {
                    self.mi(ui, cmds, Cmd::ToggleComment);
                    self.mi(ui, cmds, Cmd::SingleComment);
                    self.mi(ui, cmds, Cmd::SingleUncomment);
                    self.mi(ui, cmds, Cmd::BlockComment);
                    self.mi(ui, cmds, Cmd::BlockUncomment);
                });
                ui.menu_button(tr("Auto-Completion"), |ui| {
                    for (l, sc) in [("Function Completion", "Ctrl+Space"), ("Word Completion", "Ctrl+Enter"), ("Function Parameters Hint", "Ctrl+Shift+Space"), ("Path Completion", "Ctrl+Alt+Space")] {
                        ui.add(Button::new(tr(l)).shortcut_text(sc)).on_hover_text(tr("Use the shortcut in the editor."));
                    }
                });
                ui.menu_button(tr("EOL Conversion"), |ui| {
                    let cur = self.doc().eol;
                    let mixed = self.doc().has_mixed_eols();
                    for eol in [Eol::Crlf, Eol::Lf, Eol::Cr] {
                        if ui.radio(cur == eol && !mixed, eol.label()).clicked() {
                            cmds.push(Cmd::SetEol(eol));
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button(tr("Blank Operations"), |ui| {
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::TrimTrailing));
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::TrimLeading));
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::TrimBoth));
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::EolToSpace));
                    self.mi(ui, cmds, Cmd::LineOp(LineOp::TrimBothEolToSpace));
                    ui.separator();
                    self.mi(ui, cmds, Cmd::TabsToSpaces);
                    self.mi(ui, cmds, Cmd::SpacesToTabsAll);
                    self.mi(ui, cmds, Cmd::SpacesToTabsLeading);
                });
                ui.menu_button(tr("Paste Special"), |ui| {
                    self.mi(ui, cmds, Cmd::Paste);
                    self.mi(ui, cmds, Cmd::PasteHtml);
                    self.mi(ui, cmds, Cmd::PasteRtf);
                    ui.label(RichText::new(tr("Binary content is handled by normal copy/paste.")).weak());
                });
                ui.menu_button(tr("On Selection"), |ui| {
                    self.mi(ui, cmds, Cmd::OpenSelectedFile);
                    self.mi(ui, cmds, Cmd::OpenSelectedFolder);
                    self.mi_en(ui, cmds, Cmd::Redact(false), has_sel);
                    self.mi_en(ui, cmds, Cmd::Redact(true), has_sel);
                    self.mi(ui, cmds, Cmd::SearchInternet);
                    self.mi(ui, cmds, Cmd::ChangeSearchEngine);
                });
                ui.menu_button(tr("Multi-select All"), |ui| {
                    for m in [Match::IgnoreCaseWholeWord, Match::CaseOnly, Match::WholeWordOnly, Match::CaseAndWholeWord] {
                        self.mi(ui, cmds, Cmd::MultiSelectAll(m));
                    }
                });
                ui.menu_button(tr("Multi-select Next"), |ui| {
                    for m in [Match::IgnoreCaseWholeWord, Match::CaseOnly, Match::WholeWordOnly, Match::CaseAndWholeWord] {
                        self.mi(ui, cmds, Cmd::MultiSelectNext(m));
                    }
                });
                self.mi(ui, cmds, Cmd::MultiSelectUndo);
                self.mi(ui, cmds, Cmd::MultiSelectSkip);
                ui.separator();
                self.mi(ui, cmds, Cmd::ColumnMode);
                self.mi(ui, cmds, Cmd::ColumnEditor);
                self.mi_check(ui, cmds, Cmd::CharPanel, self.s.show_char_panel);
                self.mi_check(ui, cmds, Cmd::ClipboardHistory, self.s.show_clipboard_history);
                ui.separator();
                self.mi_check(ui, cmds, Cmd::ToggleReadOnly, self.doc().read_only);
                self.mi(ui, cmds, Cmd::ReadOnlyAll);
                self.mi(ui, cmds, Cmd::ClearReadOnlyAll);
                self.mi_en(ui, cmds, Cmd::ToggleFileReadOnly, has_path);
            });

            ui.menu_button(tr("Search"), |ui| {
                self.mi(ui, cmds, Cmd::Find);
                self.mi(ui, cmds, Cmd::FindInFiles);
                self.mi(ui, cmds, Cmd::FindNext);
                self.mi(ui, cmds, Cmd::FindPrev);
                self.mi(ui, cmds, Cmd::FindSelNext);
                self.mi(ui, cmds, Cmd::FindSelPrev);
                self.mi(ui, cmds, Cmd::FindVolatileNext);
                self.mi(ui, cmds, Cmd::FindVolatilePrev);
                self.mi(ui, cmds, Cmd::Replace);
                self.mi(ui, cmds, Cmd::IncrementalSearch);
                self.mi_check(ui, cmds, Cmd::ToggleResults, self.show_results);
                self.mi(ui, cmds, Cmd::NextResult);
                self.mi(ui, cmds, Cmd::PrevResult);
                self.mi(ui, cmds, Cmd::GotoLine);
                self.mi(ui, cmds, Cmd::GotoBrace);
                self.mi(ui, cmds, Cmd::SelectBetweenBraces);
                self.mi(ui, cmds, Cmd::Mark);
                ui.separator();
                ui.menu_button(tr("Change History"), |ui| {
                    self.mi(ui, cmds, Cmd::NextChange);
                    self.mi(ui, cmds, Cmd::PrevChange);
                    self.mi(ui, cmds, Cmd::ClearChangeHistory);
                });
                ui.separator();
                ui.menu_button(tr("Style All Occurrences of Token"), |ui| {
                    for i in 1..=5 {
                        self.mi(ui, cmds, Cmd::StyleAll(i));
                    }
                });
                ui.menu_button(tr("Style One Token"), |ui| {
                    for i in 1..=5 {
                        self.mi(ui, cmds, Cmd::StyleOne(i));
                    }
                });
                ui.menu_button(tr("Clear Style"), |ui| {
                    for i in 1..=5 {
                        self.mi(ui, cmds, Cmd::ClearStyle(i));
                    }
                    self.mi(ui, cmds, Cmd::ClearStyle(0));
                    self.mi(ui, cmds, Cmd::ClearMarks);
                });
                ui.menu_button(tr("Jump Up"), |ui| {
                    for i in 1..=5 {
                        self.mi(ui, cmds, Cmd::JumpStyle(i, false));
                    }
                    self.mi(ui, cmds, Cmd::JumpStyle(0, false));
                });
                ui.menu_button(tr("Jump Down"), |ui| {
                    for i in 1..=5 {
                        self.mi(ui, cmds, Cmd::JumpStyle(i, true));
                    }
                    self.mi(ui, cmds, Cmd::JumpStyle(0, true));
                });
                ui.menu_button(tr("Copy Styled Text"), |ui| {
                    for i in 1..=5 {
                        self.mi(ui, cmds, Cmd::CopyStyled(i));
                    }
                    self.mi(ui, cmds, Cmd::CopyStyled(0));
                    self.mi(ui, cmds, Cmd::CopyStyled(6));
                });
                ui.separator();
                ui.menu_button(tr("Bookmark"), |ui| {
                    self.mi(ui, cmds, Cmd::ToggleBookmark);
                    self.mi(ui, cmds, Cmd::NextBookmark);
                    self.mi(ui, cmds, Cmd::PrevBookmark);
                    self.mi(ui, cmds, Cmd::ClearBookmarks);
                    self.mi(ui, cmds, Cmd::CutBookmarked);
                    self.mi(ui, cmds, Cmd::CopyBookmarked);
                    self.mi(ui, cmds, Cmd::PasteBookmarked);
                    self.mi(ui, cmds, Cmd::RemoveBookmarked);
                    self.mi(ui, cmds, Cmd::RemoveUnbookmarked);
                    self.mi(ui, cmds, Cmd::InverseBookmarks);
                });
                ui.separator();
                self.mi(ui, cmds, Cmd::FindCharsInRange);
            });

            ui.menu_button(tr("View"), |ui| {
                self.mi_check(ui, cmds, Cmd::ToggleAlwaysOnTop, self.always_on_top);
                self.mi_check(ui, cmds, Cmd::ToggleFullscreen, self.fullscreen);
                self.mi_check(ui, cmds, Cmd::PostIt, self.postit);
                self.mi_check(ui, cmds, Cmd::DistractionFree, self.distraction_free);
                ui.separator();
                ui.menu_button(tr("View Current File in"), |ui| {
                    self.mi_en(ui, cmds, Cmd::OpenInDefaultViewer, has_path);
                    for (i, rc) in self.s.run_commands.iter().enumerate().filter(|(_, r)| r.command.contains("xdg-open") || r.command.contains("firefox") || r.command.contains("chrom")) {
                        if ui.button(&rc.name).clicked() {
                            cmds.push(Cmd::RunCommand(i));
                            ui.close_menu();
                        }
                    }
                });
                ui.separator();
                ui.menu_button(tr("Show Symbol"), |ui| {
                    self.mi_check(ui, cmds, Cmd::ToggleWhitespace, self.s.show_whitespace);
                    self.mi_check(ui, cmds, Cmd::ToggleEol, self.s.show_eol);
                    self.mi_check(ui, cmds, Cmd::ToggleNonPrinting, self.s.show_non_printing);
                    self.mi_check(ui, cmds, Cmd::ToggleCtrlChars, self.s.show_ctrl_chars);
                    self.mi_check(ui, cmds, Cmd::ToggleShowAll, self.s.show_whitespace && self.s.show_eol);
                    ui.separator();
                    self.mi_check(ui, cmds, Cmd::ToggleIndentGuides, self.s.show_indent_guides);
                    self.mi_check(ui, cmds, Cmd::ToggleWrapSymbol, self.s.show_wrap_symbol);
                });
                ui.menu_button(tr("Zoom"), |ui| {
                    self.mi(ui, cmds, Cmd::ZoomIn);
                    self.mi(ui, cmds, Cmd::ZoomOut);
                    self.mi(ui, cmds, Cmd::ZoomReset);
                });
                ui.menu_button(tr("Move/Clone Current Document"), |ui| {
                    self.mi(ui, cmds, Cmd::MoveToOtherView);
                    self.mi(ui, cmds, Cmd::CloneToOtherView);
                    self.mi(ui, cmds, Cmd::MoveToNewInstance);
                    self.mi(ui, cmds, Cmd::OpenInNewInstance);
                });
                ui.menu_button(tr("Tab"), |ui| {
                    for i in 0..9 {
                        self.mi(ui, cmds, Cmd::TabN(i));
                    }
                    self.mi(ui, cmds, Cmd::FirstTab);
                    self.mi(ui, cmds, Cmd::LastTab);
                    self.mi(ui, cmds, Cmd::NextTab);
                    self.mi(ui, cmds, Cmd::PrevTab);
                    ui.separator();
                    self.mi(ui, cmds, Cmd::MoveTabForward);
                    self.mi(ui, cmds, Cmd::MoveTabBackward);
                    ui.separator();
                    self.mi_check(ui, cmds, Cmd::TogglePin, self.doc().pinned);
                    for i in 1..=5 {
                        self.mi(ui, cmds, Cmd::TabColor(i));
                    }
                    self.mi(ui, cmds, Cmd::TabColor(0));
                });
                self.mi_check(ui, cmds, Cmd::ToggleWrap, self.s.word_wrap);
                self.mi(ui, cmds, Cmd::FocusOtherView);
                self.mi(ui, cmds, Cmd::HideLines);
                ui.separator();
                self.mi(ui, cmds, Cmd::FoldAll);
                self.mi(ui, cmds, Cmd::UnfoldAll);
                self.mi(ui, cmds, Cmd::FoldCurrent);
                self.mi(ui, cmds, Cmd::UnfoldCurrent);
                ui.menu_button(tr("Fold Level"), |ui| {
                    for i in 1..=8 {
                        self.mi(ui, cmds, Cmd::FoldLevel(i));
                    }
                });
                ui.menu_button(tr("Unfold Level"), |ui| {
                    for i in 1..=8 {
                        self.mi(ui, cmds, Cmd::UnfoldLevel(i));
                    }
                });
                ui.separator();
                self.mi(ui, cmds, Cmd::Summary);
                ui.separator();
                ui.menu_button(tr("Project Panels"), |ui| {
                    for i in 1..=3u8 {
                        self.mi_check(ui, cmds, Cmd::ProjectPanel(i), self.s.show_project[i as usize - 1]);
                    }
                });
                self.mi_check(ui, cmds, Cmd::FolderWorkspace, self.s.show_workspace);
                self.mi_check(ui, cmds, Cmd::DocMap, self.s.show_doc_map);
                self.mi_check(ui, cmds, Cmd::DocList, self.s.show_doc_list);
                self.mi_check(ui, cmds, Cmd::FunctionList, self.s.show_function_list);
                ui.separator();
                self.mi_check(ui, cmds, Cmd::SyncVertical, self.sync_v);
                self.mi_check(ui, cmds, Cmd::SyncHorizontal, self.sync_h);
                ui.separator();
                self.mi_check(ui, cmds, Cmd::Monitoring, self.doc().monitoring);
                ui.separator();
                ui.checkbox(&mut self.s.show_toolbar, tr("Toolbar"));
                ui.checkbox(&mut self.s.show_status_bar, tr("Status Bar"));
                ui.checkbox(&mut self.s.show_tab_bar, tr("Tab Bar"));
                ui.checkbox(&mut self.s.show_menu_bar, tr("Menu Bar (Alt shows it again)"));
            });

            ui.menu_button(tr("Encoding"), |ui| {
                let cur = self.doc().encoding.clone();
                for (label, enc) in App::encodings_main() {
                    if ui.radio(cur == enc, label).clicked() {
                        cmds.push(Cmd::Reinterpret(enc));
                        ui.close_menu();
                    }
                }
                ui.menu_button(tr("Character sets"), |ui| {
                    for (group, list) in charset_groups() {
                        ui.menu_button(tr(group), |ui| {
                            for (label, name) in list {
                                let enc = TextEncoding { name: canonical(name), bom: false };
                                if ui.radio(cur.name == enc.name, label).clicked() {
                                    cmds.push(Cmd::Reinterpret(enc));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
                ui.separator();
                for (label, enc) in App::encodings_main() {
                    if ui.button(trf("Convert to {}", &[&label])).clicked() {
                        cmds.push(Cmd::ConvertTo(enc));
                        ui.close_menu();
                    }
                }
                ui.menu_button(tr("Convert to character set"), |ui| {
                    for (group, list) in charset_groups() {
                        ui.menu_button(tr(group), |ui| {
                            for (label, name) in list {
                                if ui.button(label).clicked() {
                                    cmds.push(Cmd::ConvertTo(TextEncoding { name: canonical(name), bom: false }));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
            });

            ui.menu_button(tr("Language"), |ui| {
                let cur = self.doc().syntax.clone();
                let cur_udl = self.doc().user_lang.clone();
                if ui.radio(cur.is_none() && cur_udl.is_none(), tr("None (Normal Text)")).clicked() {
                    cmds.push(Cmd::SetSyntax(None));
                    ui.close_menu();
                }
                self.mi(ui, cmds, Cmd::AutoSyntax);
                ui.separator();
                for (letter, names) in &self.syntax_groups {
                    let label = if cur.as_ref().is_some_and(|c| names.contains(c)) && cur_udl.is_none() { format!("● {letter}") } else { letter.to_string() };
                    ui.menu_button(label, |ui| {
                        egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                            for n in names {
                                if ui.radio(cur.as_deref() == Some(n.as_str()) && cur_udl.is_none(), n).clicked() {
                                    cmds.push(Cmd::SetSyntax(Some(n.clone())));
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                }
                ui.separator();
                ui.menu_button(tr("User Defined Language"), |ui| {
                    self.mi(ui, cmds, Cmd::DefineLanguage);
                    self.mi(ui, cmds, Cmd::OpenUdlFolder);
                    if ui.button(tr("Notepad++ User Defined Languages Collection")).clicked() {
                        cmds.push(Cmd::OpenUrl("https://github.com/notepad-plus-plus/userDefinedLanguages".into()));
                        ui.close_menu();
                    }
                });
                for u in &self.udls {
                    if ui.radio(cur_udl.as_deref() == Some(u.name.as_str()), &u.name).clicked() {
                        cmds.push(Cmd::SetUserLang(u.name.clone()));
                        ui.close_menu();
                    }
                }
            });

            ui.menu_button(tr("Settings"), |ui| {
                self.mi(ui, cmds, Cmd::Preferences);
                self.mi(ui, cmds, Cmd::StyleConfigurator);
                self.mi(ui, cmds, Cmd::ShortcutMapper);
                ui.separator();
                self.mi(ui, cmds, Cmd::ImportTheme);
                self.mi(ui, cmds, Cmd::ImportSyntax);
                ui.separator();
                self.mi(ui, cmds, Cmd::EditContextMenu);
                ui.separator();
                if ui.checkbox(&mut self.s.dark_mode, tr("Dark Mode")).changed() {
                    crate::theme::apply(ui.ctx(), self.s.dark_mode);
                }
            });

            ui.menu_button(tr("Tools"), |ui| {
                for (h, n) in [(Hash::Md5, "MD5"), (Hash::Sha1, "SHA-1"), (Hash::Sha256, "SHA-256"), (Hash::Sha512, "SHA-512")] {
                    ui.menu_button(n, |ui| {
                        self.mi(ui, cmds, Cmd::HashText(h));
                        self.mi(ui, cmds, Cmd::HashFiles(h));
                        self.mi(ui, cmds, Cmd::HashSelection(h));
                    });
                }
            });

            ui.menu_button(tr("Macro"), |ui| {
                let rec = self.macro_.recording.is_some();
                self.mi_en(ui, cmds, Cmd::MacroRecord, !rec);
                self.mi_en(ui, cmds, Cmd::MacroStop, rec);
                self.mi_en(ui, cmds, Cmd::MacroPlay, !rec && !self.macro_.last.is_empty());
                self.mi_en(ui, cmds, Cmd::MacroSave, !rec && !self.macro_.last.is_empty());
                self.mi_en(ui, cmds, Cmd::MacroRunMulti, !rec && (!self.macro_.last.is_empty() || !self.s.macros.is_empty()));
                if !self.s.macros.is_empty() {
                    ui.separator();
                    for (i, m) in self.s.macros.iter().enumerate() {
                        if ui.add(Button::new(&m.name).shortcut_text(&m.shortcut)).clicked() {
                            cmds.push(Cmd::MacroRun(i));
                            ui.close_menu();
                        }
                    }
                }
                ui.separator();
                self.mi(ui, cmds, Cmd::MacroManage);
            });

            ui.menu_button(tr("Run"), |ui| {
                self.mi(ui, cmds, Cmd::RunDialog);
                ui.separator();
                for (i, r) in self.s.run_commands.iter().enumerate() {
                    if ui.add(Button::new(&r.name).shortcut_text(&r.shortcut)).on_hover_text(&r.command).clicked() {
                        cmds.push(Cmd::RunCommand(i));
                        ui.close_menu();
                    }
                }
                ui.separator();
                self.mi(ui, cmds, Cmd::RunManage);
            });

            ui.menu_button(tr("Plugins"), |ui| {
                ui.menu_button("Compare", |ui| {
                    self.mi(ui, cmds, Cmd::Compare);
                    self.mi(ui, cmds, Cmd::CompareClear);
                });
                ui.menu_button("MIME Tools", |ui| {
                    self.mi(ui, cmds, Cmd::Base64Encode);
                    self.mi(ui, cmds, Cmd::Base64Decode);
                    self.mi(ui, cmds, Cmd::UrlEncode);
                    self.mi(ui, cmds, Cmd::UrlDecode);
                    self.mi(ui, cmds, Cmd::QuotedPrintableEncode);
                    self.mi(ui, cmds, Cmd::QuotedPrintableDecode);
                });
                ui.menu_button("NppExport", |ui| {
                    self.mi(ui, cmds, Cmd::ExportHtml);
                    self.mi(ui, cmds, Cmd::ExportRtf);
                    ui.separator();
                    self.mi(ui, cmds, Cmd::CopyHtml);
                    self.mi(ui, cmds, Cmd::CopyRtf);
                });
                ui.menu_button("Converter", |ui| {
                    self.mi(ui, cmds, Cmd::AsciiToHex);
                    self.mi(ui, cmds, Cmd::HexToAscii);
                });
                ui.menu_button("JSON Tools", |ui| {
                    self.mi(ui, cmds, Cmd::JsonFormat);
                    self.mi(ui, cmds, Cmd::JsonMinify);
                });
                ui.menu_button("XML Tools", |ui| {
                    self.mi(ui, cmds, Cmd::XmlFormat);
                    self.mi(ui, cmds, Cmd::XmlMinify);
                    self.mi(ui, cmds, Cmd::XmlCheck);
                });
                let scripts = crate::tools::script_plugins();
                ui.menu_button(tr("Scripts"), |ui| {
                    if scripts.is_empty() {
                        ui.label(RichText::new(tr("Put executable scripts into the plugins folder.")).weak());
                    }
                    for (name, path) in scripts {
                        if ui.button(name).on_hover_text(&path).clicked() {
                            cmds.push(Cmd::ScriptPlugin(path));
                            ui.close_menu();
                        }
                    }
                });
                ui.separator();
                self.mi(ui, cmds, Cmd::OpenPluginsFolder);
            });

            ui.menu_button(tr("Window"), |ui| {
                self.mi(ui, cmds, Cmd::WindowsDialog);
                ui.menu_button(tr("Sort By"), |ui| {
                    for k in 0..10 {
                        self.mi(ui, cmds, Cmd::SortTabs(k));
                    }
                });
                ui.separator();
                for p in 0..2 {
                    for (i, &id) in self.panes[p].tabs.iter().enumerate().take(30) {
                        if let Some(d) = self.doc_by_id(id) {
                            let active = p == self.cur && i == self.panes[p].active;
                            if ui.radio(active, format!("{}{}", d.title(), if d.is_dirty() { " *" } else { "" })).clicked() {
                                self.cur = p;
                                cmds.push(Cmd::TabN(i as u8));
                                ui.close_menu();
                            }
                        }
                    }
                }
            });

            ui.menu_button("?", |ui| {
                self.mi(ui, cmds, Cmd::CommandLineHelp);
                ui.separator();
                self.mi(ui, cmds, Cmd::CheckUpdate);
                self.mi(ui, cmds, Cmd::UpdaterProxy);
                ui.separator();
                for (l, u) in [
                    ("Notepad++ Online User Manual", "https://npp-user-manual.org/"),
                    ("Notepad++ Home", "https://notepad-plus-plus.org/"),
                ] {
                    if ui.button(tr(l)).clicked() {
                        cmds.push(Cmd::OpenUrl(u.into()));
                        ui.close_menu();
                    }
                }
                self.mi(ui, cmds, Cmd::DebugInfo);
                ui.separator();
                self.mi(ui, cmds, Cmd::About);
            });

            if self.macro_.recording.is_some() {
                ui.label(RichText::new("⏺ REC").color(Color32::from_rgb(210, 40, 40)));
            }
        });
    }

    pub fn toolbar(&mut self, ui: &mut Ui, cmds: &mut Vec<Cmd>) {
        /// (icon, command, enabled, toggle state)
        type ToolButton = (&'static str, Cmd, bool, Option<bool>);
        // Same buttons and order as the Notepad++ toolbar.
        let d = self.doc();
        let (undo, redo, dirty, unsaved, monitoring) = (d.can_undo(), d.can_redo(), d.is_dirty(), d.path.is_none(), d.monitoring);
        let any_dirty = self.docs.iter().any(|d| d.is_dirty());
        let rec = self.macro_.recording.is_some();
        let has_macro = !self.macro_.last.is_empty();
        let s = self.s.clone();
        let dark = s.dark_mode;
        // None = separator
        let items: Vec<Option<ToolButton>> = vec![
            Some(("new", Cmd::New, true, None)),
            Some(("open", Cmd::Open, true, None)),
            Some(("save", Cmd::Save, dirty || unsaved, None)),
            Some(("saveall", Cmd::SaveAll, any_dirty, None)),
            Some(("close", Cmd::Close, true, None)),
            Some(("closeall", Cmd::CloseAll, true, None)),
            Some(("print", Cmd::Print, true, None)),
            None,
            Some(("cut", Cmd::Cut, true, None)),
            Some(("copy", Cmd::Copy, true, None)),
            Some(("paste", Cmd::Paste, true, None)),
            None,
            Some(("undo", Cmd::Undo, undo, None)),
            Some(("redo", Cmd::Redo, redo, None)),
            None,
            Some(("find", Cmd::Find, true, None)),
            Some(("findrep", Cmd::Replace, true, None)),
            None,
            Some(("zoomIn", Cmd::ZoomIn, true, None)),
            Some(("zoomOut", Cmd::ZoomOut, true, None)),
            None,
            Some(("syncV", Cmd::SyncVertical, true, Some(self.sync_v))),
            Some(("syncH", Cmd::SyncHorizontal, true, Some(self.sync_h))),
            None,
            Some(("wrap", Cmd::ToggleWrap, true, Some(s.word_wrap))),
            Some(("allChars", Cmd::ToggleShowAll, true, Some(s.show_whitespace && s.show_eol))),
            Some(("indentGuide", Cmd::ToggleIndentGuides, true, Some(s.show_indent_guides))),
            None,
            Some(("udl", Cmd::DefineLanguage, true, None)),
            Some(("docMap", Cmd::DocMap, true, Some(s.show_doc_map))),
            Some(("docList", Cmd::DocList, true, Some(s.show_doc_list))),
            Some(("funcList", Cmd::FunctionList, true, Some(s.show_function_list))),
            Some(("fileBrowser", Cmd::FolderWorkspace, true, Some(s.show_workspace))),
            None,
            Some(("monitoring", Cmd::Monitoring, true, Some(monitoring))),
            None,
            Some(("startrecord", Cmd::MacroRecord, !rec, None)),
            Some(("stoprecord", Cmd::MacroStop, rec, None)),
            Some(("playrecord", Cmd::MacroPlay, !rec && has_macro, None)),
            Some(("playrecord_m", Cmd::MacroRunMulti, !rec && has_macro, None)),
            Some(("saverecord", Cmd::MacroSave, !rec && has_macro, None)),
        ];
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            for item in items {
                let Some((icon, c, enabled, state)) = item else {
                    ui.separator();
                    continue;
                };
                let Some(tex) = self.icons.get(&ctx, dark, icon) else { continue };
                // Own button like Notepad++: 24×22 with a 16 px icon; pressed toggles and hovered
                // buttons get a frame, disabled icons are faded.
                let sense = if enabled { Sense::click() } else { Sense::hover() };
                let (rect, r) = ui.allocate_exact_size(vec2(24.0, 22.0), sense);
                let vis = ui.visuals();
                let on = state.unwrap_or(false);
                if on || (r.hovered() && enabled) {
                    let (fill, stroke) = if on { (vis.widgets.active.bg_fill, vis.widgets.active.bg_stroke) } else { (vis.widgets.hovered.bg_fill, vis.widgets.noninteractive.bg_stroke) };
                    ui.painter().rect(rect, 3.0, fill, stroke);
                }
                let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(80) };
                let icon_rect = egui::Rect::from_center_size(rect.center(), vec2(16.0, 16.0));
                ui.painter().image(tex, icon_rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
                let t = cmd::shortcut_text(&c, &s.shortcuts);
                let tip = if t.is_empty() { cmd::label(&c) } else { format!("{} ({t})", cmd::label(&c)) };
                if r.on_hover_text(tip).clicked() {
                    cmds.push(c);
                }
            }
        });
    }

    pub fn tab_bar(&mut self, ui: &mut Ui, pane: usize, cmds: &mut Vec<Cmd>) {
        let mut rects = Vec::new();
        let mut dragged: Option<usize> = None;
        let vertical = self.s.tabs_vertical;
        let multiline = self.s.tabs_multiline && !vertical;
        let mut draw = |app: &mut App, ui: &mut Ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
            for i in 0..app.panes[pane].tabs.len() {
                let id = app.panes[pane].tabs[i];
                let Some(d) = app.doc_by_id(id).map(|d| TabInfo {
                    title: d.title(),
                    read_only: d.read_only,
                    pinned: d.pinned,
                    monitoring: d.monitoring,
                    dirty: d.is_dirty(),
                    tab_color: d.tab_color,
                    path: d.path.clone(),
                }) else {
                    continue;
                };
                let active = i == app.panes[pane].active;
                let mut title = d.title.clone();
                if d.read_only {
                    title = format!("🔒 {title}");
                }
                if d.pinned {
                    title = format!("📌 {title}");
                }
                if d.monitoring {
                    title = format!("👁 {title}");
                }
                let vis = ui.visuals().clone();
                let color = if d.dirty {
                    Color32::from_rgb(220, 60, 50)
                } else if active {
                    vis.strong_text_color()
                } else {
                    vis.text_color()
                };
                let font = FontId::proportional(13.5);
                let mut text = format!("{}{title}", if d.dirty { "● " } else { "" });
                let close_w = if app.s.tab_close_button { 26.0 } else { 6.0 };
                let max_w = if vertical { (ui.available_width() - 14.0 - close_w).max(30.0) } else { app.s.tab_max_width };
                let mut galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), color);
                while galley.size().x > max_w && text.chars().count() > 4 {
                    text = text.chars().take(text.chars().count() - 2).collect::<String>().trim_end_matches('…').to_string() + "…";
                    galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), color);
                }
                let size = if vertical { vec2(ui.available_width(), 24.0) } else { vec2(galley.size().x + 14.0 + close_w, 26.0) };
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
                rects.push(rect);
                let bg = if active {
                    vis.panel_fill
                } else if resp.hovered() {
                    vis.widgets.hovered.weak_bg_fill
                } else {
                    vis.widgets.inactive.weak_bg_fill
                };
                let round = if vertical { egui::Rounding { nw: 4.0, ne: 0.0, sw: 4.0, se: 0.0 } } else { egui::Rounding { nw: 4.0, ne: 4.0, sw: 0.0, se: 0.0 } };
                let p = ui.painter();
                p.rect_filled(rect, round, bg);
                if d.tab_color > 0 {
                    let c = crate::editor::rgba(app.s.styles.tab_colors[(d.tab_color as usize - 1).min(4)]);
                    let strip = if vertical {
                        egui::Rect::from_min_size(rect.right_top() - vec2(4.0, 0.0), vec2(4.0, rect.height()))
                    } else {
                        egui::Rect::from_min_size(rect.left_bottom() - vec2(0.0, 4.0), vec2(rect.width(), 4.0))
                    };
                    p.rect_filled(strip, 0.0, c);
                }
                if active && vertical {
                    p.line_segment([rect.left_top() + vec2(0.5, 2.0), rect.left_bottom() + vec2(0.5, -2.0)], Stroke::new(3.0_f32, crate::theme::accent(app.s.dark_mode)));
                } else if active {
                    p.line_segment([rect.left_top() + vec2(2.0, 0.5), rect.right_top() + vec2(-2.0, 0.5)], Stroke::new(3.0_f32, crate::theme::accent(app.s.dark_mode)));
                } else {
                    p.rect_stroke(rect, round, Stroke::new(1.0_f32, vis.widgets.noninteractive.bg_stroke.color));
                }
                p.galley(pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0), galley, color);
                let close = egui::Rect::from_center_size(pos2(rect.right() - 13.0, rect.center().y), vec2(16.0, 16.0));
                let over_close = app.s.tab_close_button && resp.hover_pos().is_some_and(|p| close.contains(p));
                if over_close {
                    p.rect_filled(close, 3.0, Color32::from_rgb(220, 60, 50));
                }
                if app.s.tab_close_button && (active || resp.hovered()) {
                    p.text(close.center(), Align2::CENTER_CENTER, "×", FontId::proportional(15.0), if over_close { Color32::WHITE } else { vis.text_color() });
                }
                if resp.clicked() {
                    if over_close {
                        app.cur = pane;
                        cmds.push(Cmd::CloseDoc(id));
                    } else {
                        app.activate_tab(pane, i);
                    }
                }
                if resp.double_clicked() && app.s.tab_double_click_close {
                    app.cur = pane;
                    cmds.push(Cmd::CloseDoc(id));
                }
                if resp.middle_clicked() {
                    app.cur = pane;
                    cmds.push(Cmd::CloseDoc(id));
                }
                if resp.dragged() {
                    dragged = Some(i);
                }
                let resp = match &d.path {
                    Some(p) => resp.on_hover_text(p.display().to_string()),
                    None => resp,
                };
                resp.context_menu(|ui| {
                    app.activate_tab(pane, i);
                    let mi = |ui: &mut Ui, c: Cmd, cmds: &mut Vec<Cmd>, s: &crate::settings::Settings| {
                        if ui.add(Button::new(cmd::label(&c)).shortcut_text(cmd::shortcut_text(&c, &s.shortcuts))).clicked() {
                            cmds.push(c);
                            ui.close_menu();
                        }
                    };
                    let s = app.s.clone();
                    mi(ui, Cmd::Close, cmds, &s);
                    mi(ui, Cmd::CloseAllButCurrent, cmds, &s);
                    mi(ui, Cmd::CloseAllButPinned, cmds, &s);
                    mi(ui, Cmd::CloseLeft, cmds, &s);
                    mi(ui, Cmd::CloseRight, cmds, &s);
                    mi(ui, Cmd::CloseUnchanged, cmds, &s);
                    ui.separator();
                    mi(ui, Cmd::Save, cmds, &s);
                    mi(ui, Cmd::SaveAs, cmds, &s);
                    mi(ui, Cmd::Rename, cmds, &s);
                    mi(ui, Cmd::MoveToTrash, cmds, &s);
                    mi(ui, Cmd::Reload, cmds, &s);
                    mi(ui, Cmd::Print, cmds, &s);
                    ui.separator();
                    mi(ui, Cmd::OpenContainingFolder, cmds, &s);
                    mi(ui, Cmd::OpenTerminal, cmds, &s);
                    mi(ui, Cmd::OpenInDefaultViewer, cmds, &s);
                    ui.separator();
                    mi(ui, Cmd::TogglePin, cmds, &s);
                    ui.menu_button(tr("Apply Color to Tab"), |ui| {
                        for k in 1..=5 {
                            mi(ui, Cmd::TabColor(k), cmds, &s);
                        }
                        mi(ui, Cmd::TabColor(0), cmds, &s);
                    });
                    mi(ui, Cmd::ToggleReadOnly, cmds, &s);
                    mi(ui, Cmd::ToggleFileReadOnly, cmds, &s);
                    ui.separator();
                    mi(ui, Cmd::CopyPath, cmds, &s);
                    mi(ui, Cmd::CopyName, cmds, &s);
                    mi(ui, Cmd::CopyDir, cmds, &s);
                    ui.separator();
                    mi(ui, Cmd::MoveToOtherView, cmds, &s);
                    mi(ui, Cmd::CloneToOtherView, cmds, &s);
                });
            }
        };
        let bar_rect = if vertical {
            let out = egui::ScrollArea::vertical().id_salt(("vtabs", pane)).auto_shrink([false, false]).show(ui, |ui| ui.vertical(|ui| draw(self, ui)).response.rect);
            let free = egui::Rect::from_min_max(pos2(out.inner_rect.left(), out.inner_rect.top() + out.content_size.y), out.inner_rect.max);
            if free.height() > 0.0 && ui.interact(free, ui.id().with(("vtabs_free", pane)), Sense::click()).double_clicked() {
                self.cur = pane;
                cmds.push(Cmd::New);
            }
            out.inner_rect
        } else if multiline {
            ui.horizontal_wrapped(|ui| draw(self, ui)).response.rect
        } else {
            let out = egui::ScrollArea::horizontal()
                .id_salt(("tabs", pane))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| ui.horizontal(|ui| draw(self, ui)).response.rect);
            let free = egui::Rect::from_min_max(pos2(out.inner_rect.left() + out.content_size.x, out.inner_rect.top()), out.inner_rect.max);
            if free.width() > 0.0 {
                let r = ui.interact(free, ui.id().with(("tabs_free", pane)), Sense::click());
                if r.double_clicked() {
                    self.cur = pane;
                    cmds.push(Cmd::New);
                }
            }
            out.inner_rect
        };
        let _ = bar_rect;
        // Reorder by dragging a tab over another one.
        if let (Some(from), Some(p)) = (dragged, ui.ctx().pointer_interact_pos()) {
            if let Some(to) = rects.iter().position(|r| r.contains(p) || (!multiline && if vertical { r.y_range().contains(p.y) } else { r.x_range().contains(p.x) })) {
                if to != from {
                    let pn = &mut self.panes[pane];
                    let id = pn.tabs.remove(from);
                    pn.tabs.insert(to, id);
                    if pn.active == from {
                        pn.active = to;
                    } else if from < pn.active && to >= pn.active {
                        pn.active -= 1;
                    } else if from > pn.active && to <= pn.active {
                        pn.active += 1;
                    }
                }
            }
        }
    }

    pub fn status_bar(&mut self, ui: &mut Ui, cmds: &mut Vec<Cmd>) {
        let limit = (self.s.highlight_limit_mb * 1_048_576.0) as usize;
        let d = self.doc();
        ui.horizontal(|ui| {
            let lang = match (&d.user_lang, &d.syntax) {
                (Some(u), _) => format!("{u} ({})", tr("User Defined")),
                (None, Some(s)) if d.text().len() > limit => trf("{} (highlighting off: file > {} MB)", &[s, &self.s.highlight_limit_mb]),
                (None, Some(s)) => s.clone(),
                (None, None) => tr("Normal text file").to_string(),
            };
            ui.label(lang);
            ui.separator();
            ui.label(trf("length : {}    lines : {}", &[&fmt_num(d.text().len()), &fmt_num(d.line_count())]));
            ui.separator();
            let (line, col) = d.line_col(d.cursor);
            ui.label(trf("Ln : {}    Col : {}    Pos : {}", &[&fmt_num(line + 1), &(col + 1), &fmt_num(d.cursor + 1)]));
            ui.separator();
            if d.is_multi() {
                let total: usize = d.all_sels().iter().map(|&(a, c)| d.text()[a.min(c)..a.max(c)].chars().count()).sum();
                ui.label(trf("Sel : {} carets | {} chars", &[&d.all_sels().len(), &total]));
            } else {
                let sel = d.selected_text();
                let sel_lines = if sel.is_empty() { 0 } else { sel.matches('\n').count() + 1 };
                ui.label(trf("Sel : {} | {}", &[&fmt_num(sel.chars().count()), &sel_lines]));
            }
            if self.macro_.recording.is_some() {
                ui.separator();
                ui.label(RichText::new(tr("⏺ Recording macro")).color(Color32::from_rgb(210, 40, 40)));
            }
            if d.disk_missing && d.path.is_some() {
                ui.separator();
                ui.label(RichText::new(tr("File does not exist on disk")).color(Color32::from_rgb(220, 120, 0)));
            }
            if let Some(f) = &self.fif {
                ui.separator();
                ui.spinner();
                ui.label(trf("Searching… {} files", &[&f.scanned]));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(if self.overwrite { "OVR" } else { "INS" });
                ui.separator();
                ui.menu_button(d.encoding.label(), |ui| {
                    for (label, enc) in App::encodings_main() {
                        if ui.button(trf("Convert to {}", &[&label])).clicked() {
                            cmds.push(Cmd::ConvertTo(enc));
                            ui.close_menu();
                        }
                    }
                });
                ui.separator();
                let eol_label = if d.has_mixed_eols() { tr("Mixed EOLs").to_string() } else { d.eol.label().to_string() };
                ui.menu_button(eol_label, |ui| {
                    for eol in [Eol::Crlf, Eol::Lf, Eol::Cr] {
                        if ui.radio(d.eol == eol, eol.label()).clicked() {
                            cmds.push(Cmd::SetEol(eol));
                            ui.close_menu();
                        }
                    }
                });
            });
        });
    }
}
