//! Syntax highlighting with syntect (bat's syntax/theme collection via two-face),
//! plus user-imported `.tmTheme` / `.sublime-syntax` files and user defined languages.
//!
//! Lines are highlighted lazily: the parser state at the start of every line is cached and
//! only lines up to the end of the view are processed, within a time budget per frame.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::text::{LayoutJob, LayoutSection};
use eframe::egui::{Color32, FontId, Stroke, TextFormat};
use syntect::highlighting::{FontStyle, HighlightState, Highlighter, RangedHighlightIterator, Style, Theme, ThemeSet};
use syntect::parsing::{ParseState, Scope, ScopeStack, ScopeStackOp, SyntaxReference, SyntaxSet};
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

use crate::udl::UserLang;

pub struct Highlighting {
    pub syntaxes: Arc<SyntaxSet>,
    themes: EmbeddedLazyThemeSet,
    custom_themes: BTreeMap<String, Theme>,
    keywords: std::cell::RefCell<HashMap<String, Rc<Vec<String>>>>,
}

/// Themes offered in the settings (the ANSI/base16 ones need a terminal palette).
pub fn builtin_theme_names() -> Vec<&'static str> {
    EmbeddedLazyThemeSet::theme_names()
        .iter()
        .filter(|t| !matches!(t, EmbeddedThemeName::Ansi | EmbeddedThemeName::Base16 | EmbeddedThemeName::Base16_256))
        .map(|t| t.as_name())
        .collect()
}

impl Default for Highlighting {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighting {
    pub fn new() -> Self {
        let mut syntaxes = two_face::syntax::extra_newlines();
        // User syntaxes: ~/.config/notepad-rs/syntaxes/*.sublime-syntax
        let dir = crate::settings::config_dir().join("syntaxes");
        if dir.is_dir() {
            let mut b = syntaxes.clone().into_builder();
            if b.add_from_folder(&dir, true).is_ok() {
                syntaxes = b.build();
            }
        }
        let mut custom_themes = BTreeMap::new();
        if let Ok(rd) = std::fs::read_dir(crate::settings::config_dir().join("themes")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "tmTheme") {
                    if let Ok(t) = ThemeSet::get_theme(&p) {
                        let name = t.name.clone().unwrap_or_else(|| p.file_stem().unwrap().to_string_lossy().into_owned());
                        custom_themes.insert(name, t);
                    }
                }
            }
        }
        Self {
            syntaxes: Arc::new(syntaxes),
            themes: two_face::theme::extra(),
            custom_themes,
            keywords: Default::default(),
        }
    }

    pub fn theme_names(&self) -> Vec<String> {
        let mut v: Vec<String> = builtin_theme_names().into_iter().map(str::to_owned).collect();
        v.extend(self.custom_themes.keys().cloned());
        v
    }

    pub fn theme(&self, name: &str) -> &Theme {
        if let Some(t) = self.custom_themes.get(name) {
            return t;
        }
        let t = EmbeddedLazyThemeSet::theme_names()
            .iter()
            .find(|t| t.as_name() == name)
            .copied()
            .unwrap_or(EmbeddedThemeName::InspiredGithub);
        self.themes.get(t)
    }

    pub fn syntax(&self, name: &str) -> Option<&SyntaxReference> {
        self.syntaxes.find_syntax_by_name(name)
    }

    /// Syntax for a file name, falling back to the first line (shebang, `<?xml` …).
    pub fn detect(&self, path: Option<&Path>, text: &str) -> Option<String> {
        let by_path = path.and_then(|p| {
            self.syntaxes.find_syntax_for_file(p).ok().flatten().or_else(|| {
                // Names like "Dockerfile.dev" or "Makefile.am"
                let name = p.file_name()?.to_str()?;
                let stem = name.split('.').next()?;
                self.syntaxes.find_syntax_by_extension(stem)
            })
        });
        let first_line = text.lines().next().unwrap_or("");
        by_path
            .or_else(|| self.syntaxes.find_syntax_by_first_line(first_line))
            .filter(|s| s.name != "Plain Text")
            .map(|s| s.name.clone())
    }

    /// Visible syntax names, sorted case-insensitively.
    pub fn syntax_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .syntaxes
            .syntaxes()
            .iter()
            .filter(|s| !s.hidden && s.name != "Plain Text")
            .map(|s| s.name.clone())
            .collect();
        v.sort_by_key(|s| s.to_lowercase());
        v.dedup();
        v
    }

    /// Keywords of a language, harvested from the word alternatives in its grammar
    /// (`\b(if|else|while)\b`). Used for function/keyword completion.
    pub fn keywords(&self, syntax: &str) -> Rc<Vec<String>> {
        if self.keywords.borrow().is_empty() {
            // One pass over all grammars (the builder exposes the definitions).
            let alt = regex::Regex::new(r"([A-Za-z_][A-Za-z0-9_]*(?:\|[A-Za-z_][A-Za-z0-9_]*)+)").unwrap();
            let builder = (*self.syntaxes).clone().into_builder();
            let mut map = self.keywords.borrow_mut();
            for def in builder.syntaxes() {
                let mut words = BTreeSet::new();
                for ctx in def.contexts.values() {
                    for p in &ctx.patterns {
                        if let syntect::parsing::syntax_definition::Pattern::Match(m) = p {
                            for c in alt.captures_iter(m.regex.regex_str()) {
                                for w in c[1].split('|') {
                                    if w.len() >= 2 && w.len() <= 40 {
                                        words.insert(w.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                map.insert(def.name.clone(), Rc::new(words.into_iter().collect()));
            }
            map.insert(String::new(), Rc::new(Vec::new()));
        }
        self.keywords.borrow().get(syntax).cloned().unwrap_or_default()
    }
}

type States = Rc<(ParseState, HighlightState)>;

/// Styled runs of one line: (byte length, style).
pub type Spans = Vec<(usize, Style)>;

/// Per-document highlighting state. Lines are highlighted lazily (only as far as the view
/// needs them) and invalidated from the first edited line on.
#[derive(Default)]
pub struct HlCache {
    key: Option<(String, String)>,
    /// `states[i]` = parser state at the start of line i (one more entry than `spans`).
    states: Vec<States>,
    /// Same for user defined languages.
    udl_states: Vec<u8>,
    spans: Vec<Spans>,
}

pub struct Colors {
    pub fg: Color32,
    pub bg: Color32,
}

pub fn theme_colors(theme: &Theme) -> Colors {
    let c = |c: Option<syntect::highlighting::Color>, d: Color32| c.map_or(d, to_color);
    Colors { fg: c(theme.settings.foreground, Color32::BLACK), bg: c(theme.settings.background, Color32::WHITE) }
}

pub fn to_color(c: syntect::highlighting::Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

impl HlCache {
    pub fn invalidate_from(&mut self, line: usize) {
        if self.spans.len() > line {
            self.spans.truncate(line);
            self.states.truncate(line + 1);
            self.udl_states.truncate(line + 1);
        }
    }

    pub fn line_spans(&self, line: usize) -> Option<&Spans> {
        self.spans.get(line)
    }

    /// Number of lines already highlighted.
    #[cfg(test)]
    pub fn valid_lines(&self) -> usize {
        self.spans.len()
    }

    fn set_key(&mut self, key: (String, String)) {
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.spans.clear();
            self.states.clear();
            self.udl_states.clear();
        }
    }

    /// Highlights lines up to `upto` (exclusive) until `deadline`. Returns `true` when done.
    #[allow(clippy::too_many_arguments)]
    pub fn ensure(
        &mut self,
        hl: &Highlighting,
        text: &str,
        line_starts: &[usize],
        syntax: &str,
        theme_name: &str,
        upto: usize,
        deadline: Instant,
    ) -> bool {
        self.set_key((syntax.to_owned(), theme_name.to_owned()));
        let Some(syn) = hl.syntax(syntax) else { return true };
        let theme = hl.theme(theme_name);
        let highlighter = Highlighter::new(theme);
        if self.states.is_empty() {
            self.states.push(Rc::new((ParseState::new(syn), HighlightState::new(&highlighter, ScopeStack::new()))));
        }
        let upto = upto.min(line_starts.len());
        let mut i = self.spans.len();
        while i < upto {
            let start = line_starts[i];
            let end = line_starts.get(i + 1).copied().unwrap_or(text.len());
            let line = &text[start..end];
            let state = self.states[i].clone();
            let (mut ps, mut hs) = (state.0.clone(), state.1.clone());
            let spans = match ps.parse_line(line, &hl.syntaxes) {
                Ok(ops) => RangedHighlightIterator::new(&mut hs, &ops, line, &highlighter)
                    .map(|(style, _, range)| (range.len(), style))
                    .collect(),
                Err(_) => vec![(line.len(), Style::default())],
            };
            self.spans.push(spans);
            let next = (ps, hs);
            // Consecutive lines often share a state; avoid storing copies.
            self.states.push(if *state == next { state } else { Rc::new(next) });
            i += 1;
            if i % 64 == 0 && Instant::now() > deadline {
                return i >= upto;
            }
        }
        true
    }

    /// Same as [`Self::ensure`] for a user defined language.
    pub fn ensure_udl(&mut self, lang: &UserLang, text: &str, line_starts: &[usize], upto: usize, deadline: Instant) -> bool {
        self.set_key((format!("udl:{}", lang.name), lang.signature()));
        if self.udl_states.is_empty() {
            self.udl_states.push(0);
        }
        let upto = upto.min(line_starts.len());
        let mut i = self.spans.len();
        while i < upto {
            let start = line_starts[i];
            let end = line_starts.get(i + 1).map_or(text.len(), |&e| e - 1);
            let (spans, next) = lang.highlight_line(&text[start..end], self.udl_states[i]);
            self.spans.push(spans);
            self.udl_states.push(next);
            i += 1;
            if i % 256 == 0 && Instant::now() > deadline {
                return i >= upto;
            }
        }
        true
    }
}

/// Options for rendering one line.
pub struct JobOpts<'a> {
    pub font: &'a FontId,
    pub fg: Color32,
    pub tab_width: usize,
    pub char_w: f32,
    pub show_ctrl: bool,
    pub show_non_printing: bool,
    pub italic_comments: bool,
    /// Byte ranges of URLs in the line (drawn underlined).
    pub urls: &'a [Range<usize>],
    pub url_color: Color32,
}

/// Visible replacement for control and invisible characters.
pub fn display_char(c: char, show_ctrl: bool, show_non_printing: bool) -> Option<char> {
    match c {
        '\t' => Some(' '),
        '\u{0}'..='\u{1f}' if show_ctrl => char::from_u32(0x2400 + c as u32),
        '\u{7f}' if show_ctrl => Some('␡'),
        '\u{a0}' if show_non_printing => Some('⍽'),
        '\u{200b}'..='\u{200f}' | '\u{2028}' | '\u{2029}' | '\u{feff}' if show_non_printing => Some('¤'),
        '\u{0}'..='\u{1f}' | '\u{7f}' => Some(' '),
        _ => None,
    }
}

/// Builds the layout job for one line (without line break). Tabs are drawn as a space
/// plus leading space up to the next tab stop; control characters as visible symbols.
/// The char count of the job text always equals the char count of the line.
pub fn line_job(line: &str, spans: Option<&Spans>, o: &JobOpts) -> LayoutJob {
    let mut job = LayoutJob::default();
    let fmt = |style: Option<&Style>, url: bool| {
        let mut f = match style {
            Some(s) => TextFormat {
                font_id: o.font.clone(),
                color: to_color(s.foreground),
                italics: s.font_style.contains(FontStyle::ITALIC) || (o.italic_comments && s.font_style.contains(FontStyle::ITALIC)),
                underline: if s.font_style.contains(FontStyle::UNDERLINE) {
                    Stroke::new(1.0_f32, to_color(s.foreground))
                } else {
                    Stroke::NONE
                },
                background: if s.background.a > 0 && s.background.a < 255 { to_color(s.background) } else { Color32::TRANSPARENT },
                ..Default::default()
            },
            None => TextFormat::simple(o.font.clone(), o.fg),
        };
        if url {
            f.color = o.url_color;
            f.underline = Stroke::new(1.0_f32, o.url_color);
        }
        f
    };
    // Style runs clipped to the line, split at URL boundaries.
    let mut runs: Vec<(Range<usize>, Option<&Style>)> = Vec::new();
    match spans {
        Some(spans) => {
            let mut pos = 0;
            for (len, style) in spans {
                let end = (pos + len).min(line.len());
                if end > pos {
                    runs.push((pos..end, Some(style)));
                }
                pos += len;
            }
            if pos < line.len() {
                runs.push((pos..line.len(), None));
            }
        }
        None => runs.push((0..line.len(), None)),
    }
    let mut cuts: Vec<(Range<usize>, Option<&Style>, bool)> = Vec::new();
    for (r, st) in runs {
        let mut start = r.start;
        for u in o.urls.iter().filter(|u| u.start < r.end && u.end > r.start) {
            if u.start > start {
                cuts.push((start..u.start, st, false));
            }
            let (a, b) = (u.start.max(start), u.end.min(r.end));
            cuts.push((a..b, st, true));
            start = b;
        }
        if start < r.end {
            cuts.push((start..r.end, st, false));
        }
    }

    let needs_map = line.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) || (o.show_non_printing && !line.is_ascii());
    let has_tabs = line.contains('\t');
    let disp = |c: char| display_char(c, o.show_ctrl, o.show_non_printing).filter(|_| c != '\t').unwrap_or(if c == '\t' { ' ' } else { c });
    job.text = if needs_map || has_tabs { line.chars().map(disp).collect() } else { line.to_owned() };
    // Byte offsets differ when replacement chars have another UTF-8 length: map them.
    let map: Option<Vec<usize>> = needs_map.then(|| {
        let mut m = Vec::with_capacity(line.len() + 1);
        let mut out = 0;
        for c in line.chars() {
            let d = disp(c);
            for _ in 0..c.len_utf8() {
                m.push(out);
            }
            out += d.len_utf8();
        }
        m.push(out);
        m
    });
    let to_disp = |b: usize| map.as_ref().map_or(b, |m| m[b]);

    let tab_width = o.tab_width.max(1);
    let mut col = 0usize;
    let mut leading = 0.0;
    for (range, style, url) in cuts {
        let format = fmt(style, url);
        if !has_tabs {
            push_section(&mut job, to_disp(range.start)..to_disp(range.end), 0.0, format);
            continue;
        }
        // Split at tabs: the section after a tab gets the remaining tab width as leading space.
        let mut seg_start = range.start;
        for (i, c) in line[range.clone()].char_indices() {
            let i = range.start + i;
            if c == '\t' {
                push_section(&mut job, to_disp(seg_start)..to_disp(i + 1), leading, format.clone());
                leading = (tab_width - 1 - col % tab_width) as f32 * o.char_w;
                col += tab_width - col % tab_width;
                seg_start = i + 1;
            } else {
                col += 1;
            }
        }
        if seg_start < range.end {
            push_section(&mut job, to_disp(seg_start)..to_disp(range.end), leading, format);
            leading = 0.0;
        }
    }
    if job.sections.is_empty() {
        job.sections.push(LayoutSection { leading_space: 0.0, byte_range: 0..0, format: fmt(None, false) });
    }
    job
}

fn push_section(job: &mut LayoutJob, range: Range<usize>, leading_space: f32, format: TextFormat) {
    if range.is_empty() {
        return;
    }
    if let Some(last) = job.sections.last_mut() {
        if leading_space == 0.0 && last.format == format && last.byte_range.end == range.start {
            last.byte_range.end = range.end;
            return;
        }
    }
    job.sections.push(LayoutSection { leading_space, byte_range: range, format });
}

/// Line-comment token (Ctrl+Q) and block comment delimiters of a language.
pub fn comment_tokens(syntax: Option<&str>) -> (Option<&'static str>, Option<(&'static str, &'static str)>) {
    let Some(name) = syntax else { return (None, None) };
    let n = name.to_lowercase();
    let line = |t| (Some(t), None);
    match () {
        _ if ["html", "xml", "markdown", "svg", "vue", "xaml", "plist"].iter().any(|k| n.contains(k)) => {
            (None, Some(("<!--", "-->")))
        }
        _ if n == "css" || n.contains("scss") || n.contains("less") => (None, Some(("/*", "*/"))),
        _ if ["sql", "lua", "haskell", "ada", "elm", "vhdl"].iter().any(|k| n.contains(k)) => (
            Some("--"),
            if n.contains("sql") {
                Some(("/*", "*/"))
            } else if n.contains("lua") {
                Some(("--[[", "]]"))
            } else if n.contains("haskell") || n.contains("elm") {
                Some(("{-", "-}"))
            } else {
                None
            },
        ),
        _ if ["lisp", "clojure", "scheme", "ini", "assembly", "asm", "nasm", "autohotkey"].iter().any(|k| n.contains(k)) => {
            line(";")
        }
        _ if ["latex", "tex", "erlang", "matlab", "prolog"].iter().any(|k| n.contains(k)) => line("%"),
        _ if n.contains("batch") => line("REM "),
        _ if n.contains("visual basic") || n == "vb" || n.contains("vbscript") => line("'"),
        _ if [
            "python", "shell", "bash", "perl", "ruby", "yaml", "toml", "make", "docker", "r", "cmake", "powershell",
            "nix", "julia", "nim", "elixir", "crystal", "fish", "tcl", "awk", "conf", "nginx", "apache", "properties",
            "dotenv", "git", "ssh", "hosts", "cabal", "terraform", "puppet", "coffee", "graphql", "just",
        ]
        .iter()
        .any(|k| n == *k || n.contains(k)) =>
        {
            (Some("#"), if n.contains("powershell") { Some(("<#", "#>")) } else { None })
        }
        _ => (Some("//"), Some(("/*", "*/"))),
    }
}

/// An entry of the function list.
#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    /// Nesting depth (classes contain methods).
    pub depth: usize,
}

/// Extracts functions/classes using the grammar's scopes (`entity.name.function` …).
/// Runs on a background thread; `cancel` aborts early.
pub fn symbols(ss: &SyntaxSet, syntax: &str, text: &str, cancel: &std::sync::atomic::AtomicBool) -> Vec<Symbol> {
    let Some(syn) = ss.find_syntax_by_name(syntax) else { return Vec::new() };
    let kinds: Vec<(Scope, &'static str)> = [
        ("entity.name.function", "function"),
        ("entity.name.method", "method"),
        ("entity.name.class", "class"),
        ("entity.name.struct", "struct"),
        ("entity.name.enum", "enum"),
        ("entity.name.trait", "trait"),
        ("entity.name.interface", "interface"),
        ("entity.name.impl", "impl"),
        ("entity.name.namespace", "namespace"),
        ("entity.name.module", "module"),
        ("entity.name.type", "type"),
        ("entity.name.section", "section"),
    ]
    .iter()
    .filter_map(|(s, k)| Scope::new(s).ok().map(|sc| (sc, *k)))
    .collect();
    let mut ps = ParseState::new(syn);
    let mut stack = ScopeStack::new();
    let mut out: Vec<Symbol> = Vec::new();
    // Open entity scopes: (stack depth, kind, start byte in line)
    let mut open: Vec<(usize, &'static str, usize)> = Vec::new();
    // Nesting by braces/indentation (approximation).
    let mut brace_depth: usize = 0;
    let mut containers: Vec<(usize, usize)> = Vec::new(); // (brace depth, indent)
    for (line_no, line) in text.split_inclusive('\n').enumerate() {
        if line_no % 512 == 0 && cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Vec::new();
        }
        if line.len() > 20_000 {
            continue;
        }
        let Ok(ops) = ps.parse_line(line, ss) else { break };
        let indent = line.len() - line.trim_start().len();
        let mut last = 0;
        let count = |from: usize, to: usize, d: &mut usize| {
            for b in &line.as_bytes()[from..to] {
                match b {
                    b'{' => *d += 1,
                    b'}' => *d = d.saturating_sub(1),
                    _ => {}
                }
            }
        };
        for (pos, op) in ops {
            count(last, pos, &mut brace_depth);
            last = pos;
            match &op {
                ScopeStackOp::Push(s) => {
                    if let Some((_, k)) = kinds.iter().find(|(sc, _)| sc.is_prefix_of(*s)) {
                        open.push((stack.len() + 1, k, pos));
                    }
                }
                ScopeStackOp::Pop(n) => {
                    let new_len = stack.len().saturating_sub(*n);
                    while let Some(&(d, k, start)) = open.last() {
                        if d <= new_len {
                            break;
                        }
                        open.pop();
                        let name = line[start.min(pos)..pos].trim().to_string();
                        if name.is_empty() || name.len() >= 200 {
                            continue;
                        }
                        let uses_braces = text.contains('{');
                        while containers.last().is_some_and(|&(bd, ind)| if uses_braces { brace_depth <= bd } else { indent <= ind }) {
                            containers.pop();
                        }
                        out.push(Symbol { name, kind: k, line: line_no, depth: containers.len() });
                        if matches!(k, "class" | "struct" | "impl" | "trait" | "interface" | "namespace" | "module" | "enum") {
                            containers.push((brace_depth, indent));
                        }
                    }
                }
                _ => {}
            }
            let _ = stack.apply(&op);
        }
        count(last, line.len(), &mut brace_depth);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(font: &FontId) -> JobOpts<'_> {
        JobOpts {
            font,
            fg: Color32::WHITE,
            tab_width: 4,
            char_w: 6.0,
            show_ctrl: true,
            show_non_printing: false,
            italic_comments: false,
            urls: &[],
            url_color: Color32::BLUE,
        }
    }

    #[test]
    fn lazy_highlighting() {
        let hl = Highlighting::new();
        let text = "fn main() {\n    let s = \"x\";\n}\n";
        let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
        let far = Instant::now() + std::time::Duration::from_secs(10);
        let mut c = HlCache::default();
        assert!(c.ensure(&hl, text, &starts, "Rust", "Monokai Extended", 2, far));
        assert_eq!(c.valid_lines(), 2);
        assert!(c.ensure(&hl, text, &starts, "Rust", "Monokai Extended", 99, far));
        assert_eq!(c.valid_lines(), 4);
        let colors: std::collections::HashSet<_> =
            c.line_spans(1).unwrap().iter().map(|(_, s)| (s.foreground.r, s.foreground.g, s.foreground.b)).collect();
        assert!(colors.len() >= 3, "keyword, string and plain text differ");
        c.invalidate_from(1);
        assert_eq!(c.valid_lines(), 1);
    }

    #[test]
    fn tabs_and_control_chars() {
        let font = FontId::monospace(10.0);
        let job = line_job("a\tbc\td", None, &opts(&font));
        assert_eq!(job.text, "a bc d");
        let leads: Vec<f32> = job.sections.iter().map(|s| s.leading_space).collect();
        assert_eq!(leads, vec![0.0, 12.0, 6.0]);
        let job = line_job("a\u{1}b", None, &opts(&font));
        assert_eq!(job.text, "a␁b");
        assert_eq!(job.sections.last().unwrap().byte_range.end, job.text.len());
        let urls: Vec<Range<usize>> = std::iter::once(2..6).collect();
        let mut o = opts(&font);
        o.urls = &urls;
        let job = line_job("x http y", None, &o);
        assert_eq!(job.sections.len(), 3);
        assert!(job.sections[1].format.underline.width > 0.0);
    }

    #[test]
    fn detection_and_keywords() {
        let hl = Highlighting::new();
        assert_eq!(hl.detect(Some(Path::new("/x/main.rs")), "").as_deref(), Some("Rust"));
        assert_eq!(hl.detect(Some(Path::new("/x/Dockerfile")), "").as_deref(), Some("Dockerfile"));
        assert_eq!(hl.detect(None, "#!/bin/bash\n").as_deref(), Some("Bourne Again Shell (bash)"));
        assert_eq!(hl.detect(Some(Path::new("/x/notes.txt")), "hello"), None);
        assert!(hl.syntax_names().len() > 100);
        assert!(builtin_theme_names().contains(&"Monokai Extended"));
        let kw = hl.keywords("Python");
        assert!(kw.iter().any(|k| k == "while") && kw.iter().any(|k| k == "return"), "{kw:?}");
    }

    #[test]
    fn function_list() {
        let hl = Highlighting::new();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let src = "struct A;\nimpl A {\n    fn one(&self) {}\n}\nfn two() {}\n";
        let s = symbols(&hl.syntaxes, "Rust", src, &cancel);
        let names: Vec<(&str, usize)> = s.iter().map(|x| (x.name.as_str(), x.line)).collect();
        assert!(names.contains(&("one", 2)) && names.contains(&("two", 4)), "{s:?}");
        let py = "class K:\n    def m(self):\n        pass\ndef f():\n    pass\n";
        let s = symbols(&hl.syntaxes, "Python", py, &cancel);
        let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"K") && names.contains(&"m") && names.contains(&"f"), "{s:?}");
    }
}
