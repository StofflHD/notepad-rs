//! Commands triggered from menus, toolbar, shortcuts and context menus, with a registry of
//! stable ids, labels and default shortcuts (used by the Shortcut Mapper).

use eframe::egui::{Key, Modifiers};

use crate::doc::{Eol, TextEncoding};
use crate::textops::{Case, Case as TCase, LineOp, LineOp as TLineOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Hash {
    Md5,
    Sha1,
    #[default]
    Sha256,
    Sha512,
}

/// Multi-select search options (Notepad++ "Multi-select All/Next").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    IgnoreCaseWholeWord,
    CaseOnly,
    WholeWordOnly,
    CaseAndWholeWord,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    // File
    New,
    Open,
    OpenPath(String),
    OpenFolderAsWorkspace,
    OpenInDefaultViewer,
    OpenContainingFolder,
    OpenTerminal,
    Reload,
    Save,
    SaveAs,
    SaveCopyAs,
    SaveAll,
    Rename,
    Close,
    CloseDoc(u64),
    CloseAll,
    CloseAllButCurrent,
    CloseAllButPinned,
    CloseLeft,
    CloseRight,
    CloseUnchanged,
    MoveToTrash,
    LoadSession,
    SaveSession,
    Print,
    PrintNow,
    OpenAllRecent,
    ClearRecent,
    ReopenClosed,
    Exit,
    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    BeginEndSelect,
    BeginEndSelectColumn,
    InsertDateShort,
    InsertDateLong,
    InsertDateCustom,
    CopyPath,
    CopyName,
    CopyDir,
    CopyAllNames,
    CopyAllPaths,
    Indent,
    Unindent,
    Case(Case),
    LineOp(LineOp),
    Duplicate,
    DeleteLine,
    SplitLines,
    MoveUp,
    MoveDown,
    BlankAbove,
    BlankBelow,
    ToggleComment,
    SingleComment,
    SingleUncomment,
    BlockComment,
    BlockUncomment,
    TabsToSpaces,
    SpacesToTabsLeading,
    SpacesToTabsAll,
    MultiSelectAll(Match),
    MultiSelectNext(Match),
    MultiSelectUndo,
    MultiSelectSkip,
    ColumnMode,
    ColumnEditor,
    CharPanel,
    ClipboardHistory,
    ToggleReadOnly,
    ReadOnlyAll,
    ClearReadOnlyAll,
    ToggleFileReadOnly,
    OpenSelectedFile,
    OpenSelectedFolder,
    Redact(bool),
    SearchInternet,
    ChangeSearchEngine,
    // Search
    Find,
    FindInFiles,
    FindNext,
    FindPrev,
    FindSelNext,
    FindSelPrev,
    FindVolatileNext,
    FindVolatilePrev,
    Replace,
    IncrementalSearch,
    Mark,
    ToggleResults,
    NextResult,
    PrevResult,
    GotoLine,
    GotoBrace,
    SelectBetweenBraces,
    StyleAll(u8),
    StyleOne(u8),
    /// 0 = all styles
    ClearStyle(u8),
    /// (style, down); style 0 = find mark style
    JumpStyle(u8, bool),
    /// 0 = all styles, 6 = find mark style
    CopyStyled(u8),
    ToggleBookmark,
    NextBookmark,
    PrevBookmark,
    ClearBookmarks,
    CutBookmarked,
    CopyBookmarked,
    PasteBookmarked,
    RemoveBookmarked,
    RemoveUnbookmarked,
    InverseBookmarks,
    FindCharsInRange,
    NextChange,
    PrevChange,
    ClearChangeHistory,
    ClearMarks,
    // View
    ToggleAlwaysOnTop,
    ToggleFullscreen,
    PostIt,
    DistractionFree,
    ToggleWhitespace,
    ToggleEol,
    ToggleShowAll,
    ToggleIndentGuides,
    ToggleWrapSymbol,
    ToggleWrap,
    ToggleNonPrinting,
    ToggleCtrlChars,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    MoveToOtherView,
    CloneToOtherView,
    OpenInNewInstance,
    MoveToNewInstance,
    FocusOtherView,
    SyncVertical,
    SyncHorizontal,
    TabN(u8),
    FirstTab,
    LastTab,
    NextTab,
    PrevTab,
    MoveTabForward,
    MoveTabBackward,
    /// 0 = remove colour
    TabColor(u8),
    TogglePin,
    FoldAll,
    UnfoldAll,
    FoldCurrent,
    UnfoldCurrent,
    FoldLevel(u8),
    UnfoldLevel(u8),
    HideLines,
    Summary,
    ProjectPanel(u8),
    FolderWorkspace,
    DocMap,
    DocList,
    FunctionList,
    Monitoring,
    WindowsDialog,
    /// Window > Sort By: 0/1 name, 2/3 path, 4/5 type, 6/7 length, 8/9 modified time (asc/desc).
    SortTabs(u8),
    // Encoding / language
    Reinterpret(TextEncoding),
    ConvertTo(TextEncoding),
    SetEol(Eol),
    SetSyntax(Option<String>),
    SetUserLang(String),
    AutoSyntax,
    DefineLanguage,
    OpenUdlFolder,
    ImportTheme,
    ImportSyntax,
    // Settings
    Preferences,
    StyleConfigurator,
    ShortcutMapper,
    EditContextMenu,
    // Tools
    HashText(Hash),
    HashFiles(Hash),
    HashSelection(Hash),
    // Macro
    MacroRecord,
    MacroStop,
    MacroPlay,
    MacroSave,
    MacroRunMulti,
    MacroRun(usize),
    MacroManage,
    // Run
    RunDialog,
    RunCommand(usize),
    RunManage,
    // Plugins (built-in equivalents of popular Notepad++ plugins)
    Compare,
    CompareClear,
    Base64Encode,
    Base64Decode,
    UrlEncode,
    UrlDecode,
    QuotedPrintableEncode,
    QuotedPrintableDecode,
    JsonFormat,
    JsonMinify,
    XmlFormat,
    XmlMinify,
    XmlCheck,
    AsciiToHex,
    HexToAscii,
    ExportHtml,
    ExportRtf,
    CopyHtml,
    CopyRtf,
    PasteHtml,
    PasteRtf,
    ScriptPlugin(String),
    OpenPluginsFolder,
    // Help
    About,
    CommandLineHelp,
    CheckUpdate,
    UpdaterProxy,
    DebugInfo,
    OpenUrl(String),
}

/// Registry entry: id, command, English label, default shortcut, editor focus required.
pub struct Entry {
    pub id: &'static str,
    pub cmd: Cmd,
    pub label: &'static str,
    pub shortcut: &'static str,
    pub editor_only: bool,
}

macro_rules! e {
    ($id:expr, $cmd:expr, $label:expr) => {
        Entry { id: $id, cmd: $cmd, label: $label, shortcut: "", editor_only: false }
    };
    ($id:expr, $cmd:expr, $label:expr, $sc:expr) => {
        Entry { id: $id, cmd: $cmd, label: $label, shortcut: $sc, editor_only: false }
    };
    ($id:expr, $cmd:expr, $label:expr, $sc:expr, editor) => {
        Entry { id: $id, cmd: $cmd, label: $label, shortcut: $sc, editor_only: true }
    };
}

pub fn registry() -> &'static [Entry] {
    static R: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    R.get_or_init(build_registry)
}

fn build_registry() -> Vec<Entry> {
    use Cmd::*;
    let mut v = vec![
        e!("file.new", New, "New", "Ctrl+N"),
        e!("file.open", Open, "Open...", "Ctrl+O"),
        e!("file.open_workspace", OpenFolderAsWorkspace, "Open Folder as Workspace..."),
        e!("file.open_default", OpenInDefaultViewer, "Open in Default Viewer"),
        e!("file.open_folder", OpenContainingFolder, "Open Containing Folder in File Manager"),
        e!("file.open_terminal", OpenTerminal, "Open Containing Folder in Terminal"),
        e!("file.reload", Reload, "Reload from Disk"),
        e!("file.save", Save, "Save", "Ctrl+S"),
        e!("file.save_as", SaveAs, "Save As...", "Ctrl+Alt+S"),
        e!("file.save_copy", SaveCopyAs, "Save a Copy As..."),
        e!("file.save_all", SaveAll, "Save All", "Ctrl+Shift+S"),
        e!("file.rename", Rename, "Rename..."),
        e!("file.close", Close, "Close", "Ctrl+W"),
        e!("file.close_all", CloseAll, "Close All", "Ctrl+Shift+W"),
        e!("file.close_others", CloseAllButCurrent, "Close All BUT Current Document"),
        e!("file.close_unpinned", CloseAllButPinned, "Close All BUT Pinned Documents"),
        e!("file.close_left", CloseLeft, "Close All to the Left"),
        e!("file.close_right", CloseRight, "Close All to the Right"),
        e!("file.close_unchanged", CloseUnchanged, "Close All Unchanged"),
        e!("file.trash", MoveToTrash, "Move to Recycle Bin"),
        e!("file.load_session", LoadSession, "Load Session..."),
        e!("file.save_session", SaveSession, "Save Session..."),
        e!("file.print", Print, "Print...", "Ctrl+P"),
        e!("file.print_now", PrintNow, "Print Now"),
        e!("file.open_all_recent", OpenAllRecent, "Open All Recent Files"),
        e!("file.clear_recent", ClearRecent, "Empty Recent Files List"),
        e!("file.reopen_closed", ReopenClosed, "Restore Recent Closed File", "Ctrl+Shift+T"),
        e!("file.exit", Exit, "Exit", "Alt+F4"),
        e!("edit.undo", Undo, "Undo", "Ctrl+Z"),
        e!("edit.redo", Redo, "Redo", "Ctrl+Y"),
        e!("edit.cut", Cut, "Cut", "Ctrl+X"),
        e!("edit.copy", Copy, "Copy", "Ctrl+C"),
        e!("edit.paste", Paste, "Paste", "Ctrl+V"),
        e!("edit.delete", Delete, "Delete", "Del"),
        e!("edit.select_all", SelectAll, "Select All", "Ctrl+A"),
        e!("edit.begin_end_select", BeginEndSelect, "Begin/End Select"),
        e!("edit.begin_end_select_col", BeginEndSelectColumn, "Begin/End Select in Column Mode"),
        e!("edit.date_short", InsertDateShort, "Date Time (short)"),
        e!("edit.date_long", InsertDateLong, "Date Time (long)"),
        e!("edit.date_custom", InsertDateCustom, "Date Time (customized)"),
        e!("edit.copy_path", CopyPath, "Copy Current File Path"),
        e!("edit.copy_name", CopyName, "Copy Current Filename"),
        e!("edit.copy_dir", CopyDir, "Copy Current Dir. Path"),
        e!("edit.copy_all_names", CopyAllNames, "Copy All Filenames"),
        e!("edit.copy_all_paths", CopyAllPaths, "Copy All File Paths"),
        e!("edit.indent", Indent, "Increase Line Indent", "Tab", editor),
        e!("edit.unindent", Unindent, "Decrease Line Indent", "Shift+Tab", editor),
        e!("edit.upper", Case(TCase::Upper), "UPPERCASE", "Ctrl+Shift+U", editor),
        e!("edit.lower", Case(TCase::Lower), "lowercase", "Ctrl+U", editor),
        e!("edit.proper", Case(TCase::Proper), "Proper Case", "Alt+U", editor),
        e!("edit.proper_blend", Case(TCase::ProperBlend), "Proper Case (blend)", "Alt+Shift+U", editor),
        e!("edit.sentence", Case(TCase::Sentence), "Sentence case", "Ctrl+Alt+U", editor),
        e!("edit.sentence_blend", Case(TCase::SentenceBlend), "Sentence case (blend)", "Ctrl+Alt+Shift+U", editor),
        e!("edit.invert", Case(TCase::Invert), "iNVERT cASE", "", editor),
        e!("edit.random", Case(TCase::Random), "ranDOm CasE", "", editor),
        e!("edit.duplicate", Duplicate, "Duplicate Current Line", "Ctrl+D", editor),
        e!("edit.remove_dups", LineOp(TLineOp::RemoveDuplicates), "Remove Duplicate Lines"),
        e!("edit.remove_consecutive_dups", LineOp(TLineOp::RemoveConsecutiveDuplicates), "Remove Consecutive Duplicate Lines"),
        e!("edit.split_lines", SplitLines, "Split Lines", "Ctrl+I", editor),
        e!("edit.join_lines", LineOp(TLineOp::Join), "Join Lines", "Ctrl+J", editor),
        e!("edit.move_up", MoveUp, "Move Up Current Line", "Ctrl+Shift+Up", editor),
        e!("edit.move_down", MoveDown, "Move Down Current Line", "Ctrl+Shift+Down", editor),
        e!("edit.remove_empty", LineOp(TLineOp::RemoveEmpty), "Remove Empty Lines"),
        e!("edit.remove_blank", LineOp(TLineOp::RemoveBlank), "Remove Empty Lines (Containing Blank characters)"),
        e!("edit.blank_above", BlankAbove, "Insert Blank Line Above Current", "Ctrl+Alt+Enter", editor),
        e!("edit.blank_below", BlankBelow, "Insert Blank Line Below Current", "Ctrl+Alt+Shift+Enter", editor),
        e!("edit.delete_line", DeleteLine, "Delete Current Line", "Ctrl+L", editor),
        e!("edit.sort_asc", LineOp(TLineOp::SortAsc), "Sort Lines Lexicographically Ascending"),
        e!("edit.sort_desc", LineOp(TLineOp::SortDesc), "Sort Lines Lexicographically Descending"),
        e!("edit.sort_asc_nocase", LineOp(TLineOp::SortAscNoCase), "Sort Lines Lex. Ascending Ignoring Case"),
        e!("edit.sort_desc_nocase", LineOp(TLineOp::SortDescNoCase), "Sort Lines Lex. Descending Ignoring Case"),
        e!("edit.sort_locale_asc", LineOp(TLineOp::SortLocaleAsc), "Sort Lines In Locale Order Ascending"),
        e!("edit.sort_locale_desc", LineOp(TLineOp::SortLocaleDesc), "Sort Lines In Locale Order Descending"),
        e!("edit.sort_len_asc", LineOp(TLineOp::SortLengthAsc), "Sort Lines By Length Ascending"),
        e!("edit.sort_len_desc", LineOp(TLineOp::SortLengthDesc), "Sort Lines By Length Descending"),
        e!("edit.sort_int_asc", LineOp(TLineOp::SortIntAsc), "Sort Lines As Integers Ascending"),
        e!("edit.sort_int_desc", LineOp(TLineOp::SortIntDesc), "Sort Lines As Integers Descending"),
        e!("edit.sort_dec_comma_asc", LineOp(TLineOp::SortDecimalCommaAsc), "Sort Lines As Decimals (Comma) Ascending"),
        e!("edit.sort_dec_comma_desc", LineOp(TLineOp::SortDecimalCommaDesc), "Sort Lines As Decimals (Comma) Descending"),
        e!("edit.sort_dec_dot_asc", LineOp(TLineOp::SortDecimalDotAsc), "Sort Lines As Decimals (Dot) Ascending"),
        e!("edit.sort_dec_dot_desc", LineOp(TLineOp::SortDecimalDotDesc), "Sort Lines As Decimals (Dot) Descending"),
        e!("edit.reverse", LineOp(TLineOp::Reverse), "Reverse Line Order"),
        e!("edit.randomize", LineOp(TLineOp::Randomize), "Randomize Line Order"),
        e!("edit.toggle_comment", ToggleComment, "Toggle Single Line Comment", "Ctrl+Q", editor),
        e!("edit.single_comment", SingleComment, "Single Line Comment", "Ctrl+K", editor),
        e!("edit.single_uncomment", SingleUncomment, "Single Line Uncomment", "Ctrl+Shift+K", editor),
        e!("edit.block_comment", BlockComment, "Block Comment", "Ctrl+Shift+Q", editor),
        e!("edit.block_uncomment", BlockUncomment, "Block Uncomment", "", editor),
        e!("edit.trim_trailing", LineOp(TLineOp::TrimTrailing), "Trim Trailing Space"),
        e!("edit.trim_leading", LineOp(TLineOp::TrimLeading), "Trim Leading Space"),
        e!("edit.trim_both", LineOp(TLineOp::TrimBoth), "Trim Leading and Trailing Space"),
        e!("edit.eol_to_space", LineOp(TLineOp::EolToSpace), "EOL to Space"),
        e!("edit.trim_eol_to_space", LineOp(TLineOp::TrimBothEolToSpace), "Trim both and EOL to Space"),
        e!("edit.tab_to_space", TabsToSpaces, "TAB to Space"),
        e!("edit.space_to_tab_all", SpacesToTabsAll, "Space to TAB (All)"),
        e!("edit.space_to_tab_leading", SpacesToTabsLeading, "Space to TAB (Leading)"),
        e!("edit.multi_all_icww", MultiSelectAll(Match::IgnoreCaseWholeWord), "Multi-select All: Ignore Case & Whole Word"),
        e!("edit.multi_all_case", MultiSelectAll(Match::CaseOnly), "Multi-select All: Match Case Only"),
        e!("edit.multi_all_word", MultiSelectAll(Match::WholeWordOnly), "Multi-select All: Match Whole Word Only"),
        e!("edit.multi_all_both", MultiSelectAll(Match::CaseAndWholeWord), "Multi-select All: Match Case & Whole Word"),
        e!("edit.multi_next_icww", MultiSelectNext(Match::IgnoreCaseWholeWord), "Multi-select Next: Ignore Case & Whole Word"),
        e!("edit.multi_next_case", MultiSelectNext(Match::CaseOnly), "Multi-select Next: Match Case Only"),
        e!("edit.multi_next_word", MultiSelectNext(Match::WholeWordOnly), "Multi-select Next: Match Whole Word Only"),
        e!("edit.multi_next_both", MultiSelectNext(Match::CaseAndWholeWord), "Multi-select Next: Match Case & Whole Word", "Ctrl+Shift+D"),
        e!("edit.multi_undo", MultiSelectUndo, "Undo the Latest Added Multi-Select"),
        e!("edit.multi_skip", MultiSelectSkip, "Skip Current & Go to Next Multi-select"),
        e!("edit.column_mode", ColumnMode, "Column Mode..."),
        e!("edit.column_editor", ColumnEditor, "Column Editor...", "Alt+C"),
        e!("edit.char_panel", CharPanel, "Character Panel"),
        e!("edit.clipboard_history", ClipboardHistory, "Clipboard History"),
        e!("edit.read_only", ToggleReadOnly, "Read-Only on Current Document"),
        e!("edit.read_only_all", ReadOnlyAll, "Read-Only for All Documents"),
        e!("edit.clear_read_only_all", ClearReadOnlyAll, "Clear Read-Only for All Documents"),
        e!("edit.file_read_only", ToggleFileReadOnly, "Read-Only Attribute of the File"),
        e!("edit.open_selected_file", OpenSelectedFile, "Open File"),
        e!("edit.open_selected_folder", OpenSelectedFolder, "Open Containing Folder in File Manager"),
        e!("edit.redact", Redact(false), "Redact Selection █"),
        e!("edit.redact_dot", Redact(true), "Redact Selection ●"),
        e!("edit.search_internet", SearchInternet, "Search on Internet"),
        e!("edit.search_engine", ChangeSearchEngine, "Change Search Engine..."),
        e!("search.find", Find, "Find...", "Ctrl+F"),
        e!("search.find_in_files", FindInFiles, "Find in Files...", "Ctrl+Shift+F"),
        e!("search.find_next", FindNext, "Find Next", "F3"),
        e!("search.find_prev", FindPrev, "Find Previous", "Shift+F3"),
        e!("search.select_find_next", FindSelNext, "Select and Find Next", "Ctrl+F3"),
        e!("search.select_find_prev", FindSelPrev, "Select and Find Previous", "Ctrl+Shift+F3"),
        e!("search.volatile_next", FindVolatileNext, "Find (Volatile) Next", "Ctrl+Alt+F3"),
        e!("search.volatile_prev", FindVolatilePrev, "Find (Volatile) Previous", "Ctrl+Alt+Shift+F3"),
        e!("search.replace", Replace, "Replace...", "Ctrl+H"),
        e!("search.incremental", IncrementalSearch, "Incremental Search", "Ctrl+Alt+I"),
        e!("search.mark", Mark, "Mark...", "Ctrl+M"),
        e!("search.results", ToggleResults, "Search Results Window", "F7"),
        e!("search.next_result", NextResult, "Next Search Result", "F4"),
        e!("search.prev_result", PrevResult, "Previous Search Result", "Shift+F4"),
        e!("search.goto", GotoLine, "Go to...", "Ctrl+G"),
        e!("search.goto_brace", GotoBrace, "Go to Matching Brace", "Ctrl+B", editor),
        e!("search.select_braces", SelectBetweenBraces, "Select All In-between {} [] or ()", "Ctrl+Alt+B", editor),
        e!("search.toggle_bookmark", ToggleBookmark, "Toggle Bookmark", "Ctrl+F2"),
        e!("search.next_bookmark", NextBookmark, "Next Bookmark", "F2"),
        e!("search.prev_bookmark", PrevBookmark, "Previous Bookmark", "Shift+F2"),
        e!("search.clear_bookmarks", ClearBookmarks, "Clear All Bookmarks"),
        e!("search.cut_bookmarked", CutBookmarked, "Cut Bookmarked Lines"),
        e!("search.copy_bookmarked", CopyBookmarked, "Copy Bookmarked Lines"),
        e!("search.paste_bookmarked", PasteBookmarked, "Paste to (Replace) Bookmarked Lines"),
        e!("search.remove_bookmarked", RemoveBookmarked, "Remove Bookmarked Lines"),
        e!("search.remove_unbookmarked", RemoveUnbookmarked, "Remove Non-Bookmarked Lines"),
        e!("search.inverse_bookmarks", InverseBookmarks, "Inverse Bookmarks"),
        e!("search.chars_in_range", FindCharsInRange, "Find characters in range..."),
        e!("search.next_change", NextChange, "Go to Next Change"),
        e!("search.prev_change", PrevChange, "Go to Previous Change"),
        e!("search.clear_changes", ClearChangeHistory, "Clear Change History"),
        e!("search.clear_marks", ClearMarks, "Clear all marks"),
        e!("view.always_on_top", ToggleAlwaysOnTop, "Always on Top"),
        e!("view.fullscreen", ToggleFullscreen, "Toggle Full Screen Mode", "F11"),
        e!("view.postit", PostIt, "Post-It", "F12"),
        e!("view.distraction_free", DistractionFree, "Distraction Free Mode"),
        e!("view.whitespace", ToggleWhitespace, "Show Space and Tab"),
        e!("view.eol", ToggleEol, "Show End of Line"),
        e!("view.all_chars", ToggleShowAll, "Show All Characters"),
        e!("view.non_printing", ToggleNonPrinting, "Show Non-Printing Characters"),
        e!("view.ctrl_chars", ToggleCtrlChars, "Show Control Characters & Unicode EOL"),
        e!("view.indent_guides", ToggleIndentGuides, "Show Indent Guide"),
        e!("view.wrap_symbol", ToggleWrapSymbol, "Show Wrap Symbol"),
        e!("view.wrap", ToggleWrap, "Word wrap"),
        e!("view.zoom_in", ZoomIn, "Zoom In (Ctrl+Mouse Wheel Up)", "Ctrl+Plus"),
        e!("view.zoom_out", ZoomOut, "Zoom Out (Ctrl+Mouse Wheel Down)", "Ctrl+Minus"),
        e!("view.zoom_reset", ZoomReset, "Restore Default Zoom", "Ctrl+0"),
        e!("view.move_other", MoveToOtherView, "Move to Other View"),
        e!("view.clone_other", CloneToOtherView, "Clone to Other View"),
        e!("view.new_instance_open", OpenInNewInstance, "Open in New Instance"),
        e!("view.new_instance_move", MoveToNewInstance, "Move to New Instance"),
        e!("view.focus_other", FocusOtherView, "Focus on Another View", "F8"),
        e!("view.sync_v", SyncVertical, "Synchronize Vertical Scrolling"),
        e!("view.sync_h", SyncHorizontal, "Synchronize Horizontal Scrolling"),
        e!("view.first_tab", FirstTab, "First Tab"),
        e!("view.last_tab", LastTab, "Last Tab"),
        e!("view.next_tab", NextTab, "Next Tab", "Ctrl+PageDown"),
        e!("view.prev_tab", PrevTab, "Previous Tab", "Ctrl+PageUp"),
        e!("view.move_tab_fwd", MoveTabForward, "Move Tab Forward", "Ctrl+Shift+PageDown"),
        e!("view.move_tab_back", MoveTabBackward, "Move Tab Backward", "Ctrl+Shift+PageUp"),
        e!("view.tab_color0", TabColor(0), "Remove Color"),
        e!("view.pin", TogglePin, "Pin Tab"),
        e!("view.fold_all", FoldAll, "Fold All", "Alt+0"),
        e!("view.unfold_all", UnfoldAll, "Unfold All", "Alt+Shift+0"),
        e!("view.fold_current", FoldCurrent, "Fold Current Level", "Ctrl+Alt+F"),
        e!("view.unfold_current", UnfoldCurrent, "Unfold Current Level", "Ctrl+Alt+Shift+F"),
        e!("view.hide_lines", HideLines, "Hide Lines", "Alt+H"),
        e!("view.summary", Summary, "Summary..."),
        e!("view.workspace", FolderWorkspace, "Folder as Workspace"),
        e!("view.doc_map", DocMap, "Document Map"),
        e!("view.doc_list", DocList, "Document List"),
        e!("view.function_list", FunctionList, "Function List"),
        e!("view.monitoring", Monitoring, "Monitoring (tail -f)"),
        e!("window.windows", WindowsDialog, "Windows..."),
        e!("window.sort0", SortTabs(0), "Name A to Z"),
        e!("window.sort1", SortTabs(1), "Name Z to A"),
        e!("window.sort2", SortTabs(2), "Path A to Z"),
        e!("window.sort3", SortTabs(3), "Path Z to A"),
        e!("window.sort4", SortTabs(4), "Type A to Z"),
        e!("window.sort5", SortTabs(5), "Type Z to A"),
        e!("window.sort6", SortTabs(6), "Content Length Ascending"),
        e!("window.sort7", SortTabs(7), "Content Length Descending"),
        e!("window.sort8", SortTabs(8), "Modified Time Ascending"),
        e!("window.sort9", SortTabs(9), "Modified Time Descending"),
        e!("lang.auto", AutoSyntax, "Auto-detect"),
        e!("lang.define", DefineLanguage, "Define your language..."),
        e!("lang.udl_folder", OpenUdlFolder, "Open User Defined Language folder..."),
        e!("lang.import_theme", ImportTheme, "Import style theme(s)..."),
        e!("lang.import_syntax", ImportSyntax, "Import syntax definition(s)..."),
        e!("settings.preferences", Preferences, "Preferences..."),
        e!("settings.style", StyleConfigurator, "Style Configurator..."),
        e!("settings.shortcuts", ShortcutMapper, "Shortcut Mapper..."),
        e!("settings.context_menu", EditContextMenu, "Edit Popup ContextMenu"),
        e!("macro.record", MacroRecord, "Start Recording", "Ctrl+Shift+R"),
        e!("macro.stop", MacroStop, "Stop Recording"),
        e!("macro.play", MacroPlay, "Playback", "Ctrl+Shift+P"),
        e!("macro.save", MacroSave, "Save Currently Recorded Macro..."),
        e!("macro.run_multi", MacroRunMulti, "Run a Macro Multiple Times..."),
        e!("macro.manage", MacroManage, "Modify Shortcut/Delete Macro..."),
        e!("run.run", RunDialog, "Run...", "F5"),
        e!("run.manage", RunManage, "Modify Shortcut/Delete Command..."),
        e!("plugins.compare", Compare, "Compare", "Ctrl+Alt+C"),
        e!("plugins.compare_clear", CompareClear, "Clear Compare Results"),
        e!("plugins.base64_enc", Base64Encode, "Base64 Encode"),
        e!("plugins.base64_dec", Base64Decode, "Base64 Decode"),
        e!("plugins.url_enc", UrlEncode, "URL Encode"),
        e!("plugins.url_dec", UrlDecode, "URL Decode"),
        e!("plugins.qp_enc", QuotedPrintableEncode, "Quoted-printable Encode"),
        e!("plugins.qp_dec", QuotedPrintableDecode, "Quoted-printable Decode"),
        e!("plugins.json_format", JsonFormat, "Format JSON", "Ctrl+Alt+Shift+M"),
        e!("plugins.json_minify", JsonMinify, "Minify JSON"),
        e!("plugins.xml_format", XmlFormat, "Pretty print XML", "Ctrl+Alt+Shift+B"),
        e!("plugins.xml_minify", XmlMinify, "Linearize XML"),
        e!("plugins.xml_check", XmlCheck, "Check XML syntax"),
        e!("plugins.ascii_hex", AsciiToHex, "ASCII -> HEX"),
        e!("plugins.hex_ascii", HexToAscii, "HEX -> ASCII"),
        e!("plugins.export_html", ExportHtml, "Export to HTML..."),
        e!("plugins.export_rtf", ExportRtf, "Export to RTF..."),
        e!("plugins.copy_html", CopyHtml, "Copy HTML to clipboard"),
        e!("plugins.copy_rtf", CopyRtf, "Copy RTF to clipboard"),
        e!("edit.paste_html", PasteHtml, "Paste HTML Content"),
        e!("edit.paste_rtf", PasteRtf, "Paste RTF Content"),
        e!("plugins.folder", OpenPluginsFolder, "Open Plugins Folder..."),
        e!("help.about", About, "About NotepadRS", "F1"),
        e!("help.cmdline", CommandLineHelp, "Command Line Arguments..."),
        e!("help.update", CheckUpdate, "Update NotepadRS"),
        e!("help.update_proxy", UpdaterProxy, "Set Updater Proxy..."),
        e!("help.debug", DebugInfo, "Debug Info..."),
    ];
    for i in 1..=5u8 {
        let n = ["", "1st", "2nd", "3rd", "4th", "5th"][i as usize];
        v.push(Entry { id: leak(format!("search.style_all{i}")), cmd: StyleAll(i), label: leak(format!("Style All Occurrences: Using {n} Style")), shortcut: "", editor_only: false });
        v.push(Entry { id: leak(format!("search.style_one{i}")), cmd: StyleOne(i), label: leak(format!("Style One Token: Using {n} Style")), shortcut: "", editor_only: false });
        v.push(Entry { id: leak(format!("search.clear_style{i}")), cmd: ClearStyle(i), label: leak(format!("Clear {n} Style")), shortcut: "", editor_only: false });
        v.push(Entry { id: leak(format!("view.tab_color{i}")), cmd: TabColor(i), label: leak(format!("Apply Color {i}")), shortcut: "", editor_only: false });
    }
    v.push(e!("search.clear_style_all", ClearStyle(0), "Clear All Styles"));
    for i in 0..=5u8 {
        let n = if i == 0 { "Find Mark Style".to_string() } else { format!("{} style", ["", "1st", "2nd", "3rd", "4th", "5th"][i as usize]) };
        v.push(Entry { id: leak(format!("search.jump_down{i}")), cmd: JumpStyle(i, true), label: leak(format!("Jump Down: {n}")), shortcut: "", editor_only: false });
        v.push(Entry { id: leak(format!("search.jump_up{i}")), cmd: JumpStyle(i, false), label: leak(format!("Jump Up: {n}")), shortcut: "", editor_only: false });
    }
    for i in 0..=6u8 {
        let n = match i {
            0 => "All Styles".to_string(),
            6 => "Find Mark Style".to_string(),
            _ => format!("{} Style", ["", "1st", "2nd", "3rd", "4th", "5th"][i as usize]),
        };
        v.push(Entry { id: leak(format!("search.copy_styled{i}")), cmd: CopyStyled(i), label: leak(format!("Copy Styled Text: {n}")), shortcut: "", editor_only: false });
    }
    for i in 1..=8u8 {
        v.push(Entry { id: leak(format!("view.fold{i}")), cmd: FoldLevel(i), label: leak(format!("Fold Level {i}")), shortcut: leak(format!("Alt+{i}")), editor_only: false });
        v.push(Entry { id: leak(format!("view.unfold{i}")), cmd: UnfoldLevel(i), label: leak(format!("Unfold Level {i}")), shortcut: leak(format!("Alt+Shift+{i}")), editor_only: false });
    }
    for i in 0..9u8 {
        v.push(Entry { id: leak(format!("view.tab{}", i + 1)), cmd: TabN(i), label: leak(format!("Tab {}", i + 1)), shortcut: leak(format!("Ctrl+{}", i + 1)), editor_only: false });
    }
    for i in 1..=3u8 {
        v.push(Entry { id: leak(format!("view.project{i}")), cmd: ProjectPanel(i), label: leak(format!("Project Panel {i}")), shortcut: "", editor_only: false });
    }
    for (h, n) in [(Hash::Md5, "MD5"), (Hash::Sha1, "SHA-1"), (Hash::Sha256, "SHA-256"), (Hash::Sha512, "SHA-512")] {
        let k = n.to_lowercase().replace('-', "");
        v.push(Entry { id: leak(format!("tools.{k}")), cmd: HashText(h), label: leak(format!("{n}: Generate...")), shortcut: "", editor_only: false });
        v.push(Entry { id: leak(format!("tools.{k}_files")), cmd: HashFiles(h), label: leak(format!("{n}: Generate from files...")), shortcut: "", editor_only: false });
        v.push(Entry { id: leak(format!("tools.{k}_sel")), cmd: HashSelection(h), label: leak(format!("{n}: Generate from selection into clipboard")), shortcut: "", editor_only: false });
    }
    v
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl Cmd {
    #[cfg(test)]
    pub fn id(&self) -> Option<&'static str> {
        registry().iter().find(|e| &e.cmd == self).map(|e| e.id)
    }

    pub fn from_id(id: &str) -> Option<Cmd> {
        registry().iter().find(|e| e.id == id).map(|e| e.cmd.clone())
    }
}

/// English label of a command (translated).
pub fn label(cmd: &Cmd) -> String {
    registry().iter().find(|e| &e.cmd == cmd).map_or_else(|| format!("{cmd:?}"), |e| crate::i18n::tr(e.label).to_string())
}

/// Parses "Ctrl+Shift+S" into modifiers and key.
pub fn parse_shortcut(s: &str) -> Option<(Modifiers, Key)> {
    if s.trim().is_empty() {
        return None;
    }
    let mut m = Modifiers::NONE;
    let parts: Vec<&str> = s.split('+').collect();
    // "Ctrl++" → key "+"
    let (mods, key) = if s.ends_with("++") { (&parts[..parts.len() - 2], "Plus") } else { (&parts[..parts.len() - 1], *parts.last()?) };
    for p in mods {
        match p.trim().to_lowercase().as_str() {
            "ctrl" | "strg" => m.ctrl = true,
            "shift" | "umschalt" => m.shift = true,
            "alt" => m.alt = true,
            _ => return None,
        }
    }
    let key = match key.trim() {
        "Del" => Key::Delete,
        "Ins" => Key::Insert,
        "Up" => Key::ArrowUp,
        "Down" => Key::ArrowDown,
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        "+" => Key::Plus,
        "-" => Key::Minus,
        k => Key::from_name(k)?,
    };
    Some((m, key))
}

pub fn format_shortcut(m: Modifiers, key: Key) -> String {
    let mut s = String::new();
    if m.ctrl || m.command {
        s.push_str("Ctrl+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.shift {
        s.push_str("Shift+");
    }
    s.push_str(match key {
        Key::Delete => "Del",
        Key::Insert => "Ins",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        k => k.name(),
    });
    s
}

/// Effective shortcut of a registry entry (user override or default).
pub fn shortcut_of<'a>(e: &'a Entry, overrides: &'a std::collections::BTreeMap<String, String>) -> &'a str {
    overrides.get(e.id).map_or(e.shortcut, |s| s.as_str())
}

/// Shortcut text for a command (for menus).
pub fn shortcut_text(cmd: &Cmd, overrides: &std::collections::BTreeMap<String, String>) -> String {
    registry().iter().find(|e| &e.cmd == cmd).map_or(String::new(), |e| shortcut_of(e, overrides).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_unique_and_shortcuts_parse() {
        let mut ids = std::collections::HashSet::new();
        let mut keys = std::collections::HashMap::new();
        for e in registry() {
            assert!(ids.insert(e.id), "duplicate id {}", e.id);
            if !e.shortcut.is_empty() && !matches!(e.shortcut, "Tab" | "Shift+Tab" | "Del" | "Alt+F4") {
                let k = parse_shortcut(e.shortcut).unwrap_or_else(|| panic!("bad shortcut {}", e.shortcut));
                if let Some(other) = keys.insert(format_shortcut(k.0, k.1), e.id) {
                    panic!("shortcut {} used by {} and {}", e.shortcut, other, e.id);
                }
            }
        }
        assert_eq!(Cmd::from_id("file.save"), Some(Cmd::Save));
        assert_eq!(Cmd::Save.id(), Some("file.save"));
        assert_eq!(parse_shortcut("Ctrl+Shift+S"), Some((Modifiers { ctrl: true, shift: true, ..Modifiers::NONE }, Key::S)));
        assert_eq!(parse_shortcut("Ctrl++"), Some((Modifiers::CTRL, Key::Plus)));
        assert!(registry().len() > 250);
    }
}
