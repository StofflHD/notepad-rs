//! Settings and session snapshot, stored as JSON in ~/.config/notepad-rs/.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::doc::{Eol, TextEncoding};
use crate::search;

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("notepad-rs")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurrentLine {
    None,
    Highlight,
    Frame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeMode {
    None,
    Line,
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupMode {
    None,
    /// file.ext.bak next to the file
    Simple,
    /// timestamped copies in the backup directory
    Verbose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenDir {
    FollowCurrent,
    RememberLast,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Completion {
    Off,
    Words,
    Functions,
    Both,
}

/// RGBA colour stored as array (serde friendly).
pub type Rgba = [u8; 4];

/// Colour overrides from the Style Configurator (`None` = derived from the theme).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Styles {
    pub selection: Option<Rgba>,
    pub current_line: Option<Rgba>,
    pub caret: Option<Rgba>,
    pub smart_highlight: Rgba,
    pub find_mark: Rgba,
    /// "Style all occurrences" 1–5
    pub mark_styles: [Rgba; 5],
    pub brace: Rgba,
    pub bookmark: Rgba,
    pub edge: Rgba,
    pub indent_guide: Option<Rgba>,
    pub line_number: Option<Rgba>,
    pub change_modified: Rgba,
    pub change_saved: Rgba,
    pub url: Rgba,
    pub tab_colors: [Rgba; 5],
    pub bold_global: bool,
    pub italic_comments: bool,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            selection: None,
            current_line: None,
            caret: None,
            smart_highlight: [0, 220, 0, 80],
            find_mark: [255, 0, 0, 90],
            mark_styles: [[0, 255, 255, 110], [255, 128, 0, 110], [255, 255, 0, 120], [128, 0, 255, 90], [0, 128, 0, 110]],
            brace: [0, 90, 255, 80],
            bookmark: [40, 110, 220, 255],
            edge: [128, 128, 128, 120],
            indent_guide: None,
            line_number: None,
            change_modified: [255, 140, 0, 255],
            change_saved: [80, 180, 80, 255],
            url: [0, 102, 204, 255],
            tab_colors: [[230, 200, 70, 255], [120, 190, 90, 255], [90, 160, 230, 255], [240, 140, 60, 255], [200, 110, 200, 255]],
            bold_global: false,
            italic_comments: false,
        }
    }
}

/// A command for the Run menu.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct RunCommand {
    pub name: String,
    pub command: String,
    pub shortcut: String,
}

/// A recorded macro saved under a name.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct SavedMacro {
    pub name: String,
    pub events: Vec<eframe::egui::Event>,
    pub shortcut: String,
}

/// Tab settings per language.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct TabSetting {
    pub width: usize,
    pub use_spaces: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "en" or "de"
    pub ui_lang: String,
    pub dark_mode: bool,
    pub theme_light: String,
    pub theme_dark: String,
    pub font_size: f32,
    /// Path of a monospace TTF/OTF; `None` = built-in Hack.
    pub font_path: Option<String>,
    pub ui_scale: f32,
    pub styles: Styles,

    // View
    pub word_wrap: bool,
    pub show_whitespace: bool,
    pub show_eol: bool,
    pub show_indent_guides: bool,
    pub show_wrap_symbol: bool,
    pub show_ctrl_chars: bool,
    pub show_non_printing: bool,
    pub line_numbers: bool,
    pub line_numbers_dynamic: bool,
    pub bookmark_margin: bool,
    pub fold_margin: bool,
    pub change_history: bool,
    pub current_line: CurrentLine,
    pub edge_mode: EdgeMode,
    pub edge_column: usize,
    pub caret_width: f32,
    pub caret_blink: bool,
    pub scroll_beyond_last_line: bool,
    pub show_toolbar: bool,
    pub show_status_bar: bool,
    pub show_tab_bar: bool,
    pub show_menu_bar: bool,
    pub tabs_multiline: bool,
    /// Tab bar as a vertical list left of the editor.
    pub tabs_vertical: bool,
    pub tab_close_button: bool,
    pub tab_double_click_close: bool,
    pub tab_max_width: f32,

    // Editing
    pub smart_highlight: bool,
    pub smart_match_case: bool,
    pub smart_whole_word: bool,
    pub brace_match: bool,
    pub tag_match: bool,
    pub multi_editing: bool,
    pub clickable_links: bool,
    pub auto_indent: bool,
    pub tab_width: usize,
    pub use_spaces: bool,
    pub lang_tabs: BTreeMap<String, TabSetting>,
    pub auto_close_parens: bool,
    pub auto_close_brackets: bool,
    pub auto_close_braces: bool,
    pub auto_close_dquote: bool,
    pub auto_close_squote: bool,
    pub auto_close_tags: bool,
    pub completion: Completion,
    pub completion_from: usize,
    pub completion_ignore_numbers: bool,
    /// Show the function parameters hint when typing `(`.
    pub calltip_on_input: bool,
    /// Additional characters that count as word characters.
    pub word_chars: String,
    pub copy_line_without_selection: bool,

    // New document / files
    pub default_eol: Eol,
    pub default_encoding: TextEncoding,
    pub default_lang: Option<String>,
    pub open_dir: OpenDir,
    pub custom_dir: String,
    pub last_dir: String,
    pub max_recent: usize,
    pub recent_full_path: bool,
    pub recent: Vec<String>,
    /// Restore open documents including unsaved changes (session snapshot).
    pub remember_session: bool,
    pub backup_interval_secs: u64,
    pub backup_on_save: BackupMode,
    pub backup_dir: String,
    pub single_instance: bool,
    pub file_autodetect: bool,
    /// Reload files changed by other programs without asking (if unmodified).
    pub auto_reload: bool,
    pub autodetect_scroll_end: bool,
    /// Syntax highlighting is switched off above this size (MB).
    pub highlight_limit_mb: f32,
    /// Folding, smart highlighting etc. are switched off above this size (MB).
    pub large_file_mb: f32,
    pub date_format: String,

    // Search
    pub search: search::Options,
    pub find_history: Vec<String>,
    pub replace_history: Vec<String>,
    pub fif_dir_history: Vec<String>,
    pub fif_filter_history: Vec<String>,
    pub fif_filters: String,
    pub fif_dir: String,
    pub fif_subfolders: bool,
    pub fif_hidden: bool,
    pub fill_find_with_selection: bool,
    pub search_engine: String,
    pub confirm_replace_all_docs: bool,

    // Print
    pub print_line_numbers: bool,
    pub print_colors: bool,
    pub print_font_size: f32,
    pub print_header: String,
    pub print_footer: String,
    pub print_command: String,

    // Customization
    pub shortcuts: BTreeMap<String, String>,
    pub run_commands: Vec<RunCommand>,
    pub macros: Vec<SavedMacro>,
    pub context_menu_extra: Vec<String>,

    // Panels
    pub workspace_roots: Vec<String>,
    pub show_doc_list: bool,
    pub show_function_list: bool,
    pub show_workspace: bool,
    pub show_doc_map: bool,
    pub show_char_panel: bool,
    pub show_clipboard_history: bool,
    pub show_project: [bool; 3],
    pub project_files: [String; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_lang: "en".into(),
            dark_mode: false,
            theme_light: "InspiredGitHub".into(),
            theme_dark: "Monokai Extended".into(),
            font_size: 14.0,
            font_path: None,
            ui_scale: 1.0,
            styles: Styles::default(),
            word_wrap: false,
            show_whitespace: false,
            show_eol: false,
            show_indent_guides: true,
            show_wrap_symbol: false,
            show_ctrl_chars: true,
            show_non_printing: false,
            line_numbers: true,
            line_numbers_dynamic: true,
            bookmark_margin: true,
            fold_margin: true,
            change_history: true,
            current_line: CurrentLine::Highlight,
            edge_mode: EdgeMode::None,
            edge_column: 80,
            caret_width: 2.0,
            caret_blink: true,
            scroll_beyond_last_line: true,
            show_toolbar: true,
            show_status_bar: true,
            show_tab_bar: true,
            show_menu_bar: true,
            tabs_multiline: false,
            tabs_vertical: false,
            tab_close_button: true,
            tab_double_click_close: false,
            tab_max_width: 250.0,
            smart_highlight: true,
            smart_match_case: false,
            smart_whole_word: true,
            brace_match: true,
            tag_match: true,
            multi_editing: true,
            clickable_links: true,
            auto_indent: true,
            tab_width: 4,
            use_spaces: false,
            lang_tabs: BTreeMap::new(),
            auto_close_parens: false,
            auto_close_brackets: false,
            auto_close_braces: false,
            auto_close_dquote: false,
            auto_close_squote: false,
            auto_close_tags: false,
            completion: Completion::Both,
            completion_from: 3,
            completion_ignore_numbers: true,
            calltip_on_input: true,
            word_chars: String::new(),
            copy_line_without_selection: true,
            default_eol: Eol::Lf,
            default_encoding: TextEncoding::utf8(),
            default_lang: None,
            open_dir: OpenDir::FollowCurrent,
            custom_dir: String::new(),
            last_dir: String::new(),
            max_recent: 15,
            recent_full_path: true,
            recent: Vec::new(),
            remember_session: true,
            backup_interval_secs: 7,
            backup_on_save: BackupMode::None,
            backup_dir: String::new(),
            single_instance: true,
            file_autodetect: true,
            auto_reload: false,
            autodetect_scroll_end: false,
            highlight_limit_mb: 4.0,
            large_file_mb: 200.0,
            date_format: "yyyy-MM-dd HH:mm:ss".into(),
            search: search::Options::default(),
            find_history: Vec::new(),
            replace_history: Vec::new(),
            fif_dir_history: Vec::new(),
            fif_filter_history: Vec::new(),
            fif_filters: "*.*".into(),
            fif_dir: String::new(),
            fif_subfolders: true,
            fif_hidden: false,
            fill_find_with_selection: true,
            search_engine: "https://duckduckgo.com/?q=$(CURRENT_WORD)".into(),
            confirm_replace_all_docs: true,
            print_line_numbers: true,
            print_colors: true,
            print_font_size: 9.0,
            print_header: "$(FULL_CURRENT_PATH)".into(),
            print_footer: "$(PAGE) / $(PAGES)".into(),
            print_command: "lp".into(),
            shortcuts: BTreeMap::new(),
            run_commands: vec![
                RunCommand { name: "Open in default application".into(), command: "xdg-open \"$(FULL_CURRENT_PATH)\"".into(), shortcut: String::new() },
                RunCommand { name: "Open containing folder".into(), command: "xdg-open \"$(CURRENT_DIRECTORY)\"".into(), shortcut: String::new() },
                RunCommand { name: "Search on Wikipedia".into(), command: "xdg-open \"https://en.wikipedia.org/wiki/Special:Search?search=$(CURRENT_WORD)\"".into(), shortcut: String::new() },
            ],
            macros: Vec::new(),
            context_menu_extra: Vec::new(),
            workspace_roots: Vec::new(),
            show_doc_list: false,
            show_function_list: false,
            show_workspace: false,
            show_doc_map: false,
            show_char_panel: false,
            show_clipboard_history: false,
            show_project: [false; 3],
            project_files: [String::new(), String::new(), String::new()],
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(config_dir().join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        write_json("config.json", self)
    }

    pub fn theme(&self) -> &str {
        if self.dark_mode {
            &self.theme_dark
        } else {
            &self.theme_light
        }
    }

    pub fn add_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(self.max_recent.max(1));
    }

    /// Tab width / spaces for a language (falls back to the global setting).
    pub fn tabs_for(&self, lang: Option<&str>) -> TabSetting {
        lang.and_then(|l| self.lang_tabs.get(l).copied())
            .unwrap_or(TabSetting { width: self.tab_width, use_spaces: self.use_spaces })
    }
}

pub fn push_history(list: &mut Vec<String>, s: &str) {
    if s.is_empty() {
        return;
    }
    list.retain(|x| x != s);
    list.insert(0, s.to_owned());
    list.truncate(20);
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SessionDoc {
    pub path: Option<String>,
    pub untitled_no: usize,
    /// Unsaved content (only for modified or untitled documents).
    pub text: Option<String>,
    pub encoding: TextEncoding,
    pub eol: Option<Eol>,
    pub syntax: Option<String>,
    pub user_lang: Option<String>,
    pub sel: (usize, usize),
    pub first_line: usize,
    pub bookmarks: Vec<usize>,
    pub folded: Vec<usize>,
    pub read_only: bool,
    pub pinned: bool,
    pub tab_color: u8,
    /// 0 = main view, 1 = second view, 2 = both (cloned)
    pub view: u8,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Session {
    pub docs: Vec<SessionDoc>,
    pub active: usize,
    /// Active document index of the second view.
    pub active2: Option<usize>,
}

impl Session {
    pub fn load() -> Self {
        Self::load_from(&config_dir().join("session.json")).unwrap_or_default()
    }

    pub fn load_from(path: &std::path::Path) -> Option<Self> {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save(&self) -> Result<(), String> {
        write_json("session.json", self)
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}

/// Writes atomically (temp file + rename) so a crash never leaves a truncated file.
pub fn write_json<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| e.to_string())
}
