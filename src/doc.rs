//! A single open document: text, file binding, encoding/EOL handling and undo history.

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};

use crate::highlight::HlCache;

/// Files above this size are refused (the whole text is kept in memory and laid out at once).
pub const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Eol {
    Crlf,
    Lf,
    Cr,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Eol::Crlf => "Windows (CR LF)",
            Eol::Lf => "Unix (LF)",
            Eol::Cr => "Macintosh (CR)",
        }
    }
}

/// Encoding of the file on disk. `name` is an encoding_rs name ("UTF-8", "UTF-16LE", "windows-1252" …).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEncoding {
    pub name: String,
    pub bom: bool,
}

impl Default for TextEncoding {
    fn default() -> Self {
        Self::utf8()
    }
}

impl TextEncoding {
    pub fn utf8() -> Self {
        Self { name: "UTF-8".into(), bom: false }
    }
    pub fn new(enc: &'static Encoding, bom: bool) -> Self {
        Self { name: enc.name().into(), bom }
    }
    pub fn encoding(&self) -> &'static Encoding {
        Encoding::for_label(self.name.as_bytes()).unwrap_or(encoding_rs::UTF_8)
    }
    pub fn label(&self) -> String {
        match (self.name.as_str(), self.bom) {
            ("UTF-8", false) => "UTF-8".into(),
            ("UTF-8", true) => "UTF-8-BOM".into(),
            ("UTF-16LE", _) => "UTF-16 LE BOM".into(),
            ("UTF-16BE", _) => "UTF-16 BE BOM".into(),
            ("windows-1252", _) => "ANSI (Windows-1252)".into(),
            (n, _) => n.to_string(),
        }
    }

    pub fn decode(&self, bytes: &[u8]) -> (String, bool) {
        let enc = self.encoding();
        let body = strip_bom(bytes, enc);
        let (text, had_errors) = enc.decode_without_bom_handling(body);
        (text.into_owned(), had_errors)
    }

    /// Returns the encoded bytes and whether characters had to be replaced.
    pub fn encode(&self, text: &str) -> (Vec<u8>, bool) {
        let enc = self.encoding();
        if enc == encoding_rs::UTF_16LE || enc == encoding_rs::UTF_16BE {
            // encoding_rs only decodes UTF-16, so encode by hand. Always with BOM, like Notepad++.
            let le = enc == encoding_rs::UTF_16LE;
            let mut out = Vec::with_capacity(text.len() * 2 + 2);
            for unit in std::iter::once(0xFEFF).chain(text.encode_utf16()) {
                out.extend_from_slice(&if le { unit.to_le_bytes() } else { unit.to_be_bytes() });
            }
            return (out, false);
        }
        if enc == encoding_rs::UTF_8 {
            let mut out = Vec::with_capacity(text.len() + 3);
            if self.bom {
                out.extend_from_slice(b"\xEF\xBB\xBF");
            }
            out.extend_from_slice(text.as_bytes());
            return (out, false);
        }
        let (bytes, _, had_errors) = enc.encode(text);
        (bytes.into_owned(), had_errors)
    }
}

fn strip_bom<'a>(bytes: &'a [u8], enc: &'static Encoding) -> &'a [u8] {
    if enc == encoding_rs::UTF_8 {
        bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
    } else if enc == encoding_rs::UTF_16LE {
        bytes.strip_prefix(b"\xFF\xFE").unwrap_or(bytes)
    } else if enc == encoding_rs::UTF_16BE {
        bytes.strip_prefix(b"\xFE\xFF").unwrap_or(bytes)
    } else {
        bytes
    }
}

/// Guesses the encoding of raw file content.
pub fn detect_encoding(bytes: &[u8]) -> TextEncoding {
    if bytes.starts_with(b"\xEF\xBB\xBF") {
        return TextEncoding::new(encoding_rs::UTF_8, true);
    }
    if bytes.starts_with(b"\xFF\xFE") {
        return TextEncoding::new(encoding_rs::UTF_16LE, true);
    }
    if bytes.starts_with(b"\xFE\xFF") {
        return TextEncoding::new(encoding_rs::UTF_16BE, true);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return TextEncoding::utf8();
    }
    // UTF-16 without BOM: mostly-ASCII text has a NUL in every other byte.
    let sample = &bytes[..bytes.len().min(4096) & !1];
    if sample.len() >= 4 {
        let zeros_odd = sample.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
        let zeros_even = sample.iter().step_by(2).filter(|&&b| b == 0).count();
        let half = sample.len() / 2;
        if zeros_odd * 10 > half * 7 && zeros_even * 10 < half {
            return TextEncoding::new(encoding_rs::UTF_16LE, false);
        }
        if zeros_even * 10 > half * 7 && zeros_odd * 10 < half {
            return TextEncoding::new(encoding_rs::UTF_16BE, false);
        }
    }
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(bytes, true);
    TextEncoding::new(det.guess(None, chardetng::Utf8Detection::Allow), false)
}

/// Detects line endings and converts all of them to `\n`.
/// Returns the dominant EOL and, for files with mixed endings, the EOL of every line.
pub fn normalize_eol(text: String, default: Eol) -> (String, Eol, Option<Vec<Eol>>) {
    let b = text.as_bytes();
    let mut eols = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\r' if b.get(i + 1) == Some(&b'\n') => {
                eols.push(Eol::Crlf);
                i += 1;
            }
            b'\r' => eols.push(Eol::Cr),
            b'\n' => eols.push(Eol::Lf),
            _ => {}
        }
        i += 1;
    }
    let count = |e: Eol| eols.iter().filter(|&&x| x == e).count();
    let (crlf, lf, cr) = (count(Eol::Crlf), count(Eol::Lf), count(Eol::Cr));
    let eol = if eols.is_empty() {
        default
    } else if crlf >= lf && crlf >= cr {
        Eol::Crlf
    } else if lf >= cr {
        Eol::Lf
    } else {
        Eol::Cr
    };
    let mixed = [crlf, lf, cr].iter().filter(|&&n| n > 0).count() > 1;
    let text = if crlf == 0 && cr == 0 { text } else { text.replace("\r\n", "\n").replace('\r', "\n") };
    (text, eol, mixed.then_some(eols))
}

#[derive(Clone, Debug)]
struct Edit {
    pos: usize,
    removed: String,
    inserted: String,
}

/// How an edit was made; consecutive edits of the same kind form one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Backspace,
    Delete,
    Other,
}

/// All selections of a view: primary first, as (anchor, cursor).
pub type Sels = Vec<(usize, usize)>;

#[derive(Clone, Debug)]
struct Group {
    edits: Vec<Edit>,
    sel_before: Sels,
    sel_after: Sels,
    kind: EditKind,
    serial: u64,
}

/// Where the view is scrolled to and pending scroll requests.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub first_line: usize,
    /// Wrapped row inside `first_line`.
    pub first_row: usize,
    pub x_offset: f32,
    /// Remembered x position for vertical cursor movement.
    pub preferred_x: Option<f32>,
    pub scroll: Option<ScrollTo>,
    /// Widest line seen so far (horizontal scroll range).
    pub max_width: f32,
    /// At a wrap boundary the caret belongs to the end of the upper row.
    pub row_end_affinity: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollTo {
    /// Minimal scrolling to make the cursor visible.
    Cursor,
    /// Cursor in the middle of the view (search results, go to line).
    Center,
}

/// Rectangular (column) selection: anchor and caret as line + x position in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectSel {
    pub anchor_line: usize,
    pub anchor_x: f32,
    pub caret_line: usize,
    pub caret_x: f32,
}

/// Selection and scroll state of one view. A document shown in both views
/// (cloned) keeps the state of the second view in `Document::alt`.
#[derive(Clone, Debug, Default)]
pub struct ViewState {
    pub cursor: usize,
    pub anchor: usize,
    pub extra: Sels,
    pub rect: Option<RectSel>,
    pub view: View,
}

/// Change history per line (margin markers like Notepad++ 8.6).
pub const CHANGE_NONE: u8 = 0;
pub const CHANGE_MODIFIED: u8 = 1;
pub const CHANGE_SAVED: u8 = 2;

pub struct Document {
    pub id: u64,
    pub path: Option<PathBuf>,
    /// Number for "new N" titles of untitled documents.
    pub untitled_no: usize,
    text: String,
    /// Byte offset of every line start.
    line_starts: Vec<usize>,
    /// EOL of every line break when the file has mixed line endings.
    line_eols: Option<Vec<Eol>>,
    changes: Vec<u8>,
    pub encoding: TextEncoding,
    pub eol: Eol,
    /// syntect syntax name; `None` = plain text.
    pub syntax: Option<String>,
    /// User defined language; takes precedence over `syntax`.
    pub user_lang: Option<String>,
    pub read_only: bool,
    pub bookmarks: BTreeSet<usize>,
    /// Marked byte ranges: style 0 = "Mark" (find), 1–5 = "Style all occurrences".
    pub marks: Vec<(Range<usize>, u8)>,
    pub disk_mtime: Option<SystemTime>,
    pub disk_missing: bool,
    /// Primary selection as byte offsets; the cursor is where the caret is drawn.
    pub cursor: usize,
    pub anchor: usize,
    /// Additional selections (multi-editing), as (anchor, cursor).
    pub extra: Sels,
    pub rect: Option<RectSel>,
    pub view: View,
    pub alt: Option<Box<ViewState>>,
    /// Folded fold-header lines.
    pub folded: BTreeSet<usize>,
    /// Ranges of lines hidden via "Hide Lines".
    pub hidden_ranges: Vec<Range<usize>>,
    fold_cache: Option<(u64, Vec<u32>, Instant)>,
    hidden_cache: Option<(u64, u64, Vec<Range<usize>>)>,
    fold_gen: u64,
    pub hl: HlCache,
    /// Incremented on every text change.
    pub version: u64,
    pub pinned: bool,
    pub tab_color: u8,
    pub monitoring: bool,
    /// Line backgrounds from the Compare plugin: 1 added, 2 removed, 3 changed.
    pub diff_marks: std::collections::BTreeMap<usize, u8>,
    /// Serial of the undo group that matches the file on disk (0 = empty history,
    /// `u64::MAX` = no state matches, e.g. after an encoding change).
    saved_state: u64,
    next_serial: u64,
    undo: Vec<Group>,
    redo: Vec<Group>,
    last_edit: Option<Instant>,
}

impl Document {
    pub fn new_untitled(id: u64, untitled_no: usize, eol: Eol) -> Self {
        Self {
            id,
            path: None,
            untitled_no,
            text: String::new(),
            line_starts: vec![0],
            line_eols: None,
            changes: vec![CHANGE_NONE],
            encoding: TextEncoding::utf8(),
            eol,
            syntax: None,
            user_lang: None,
            read_only: false,
            bookmarks: BTreeSet::new(),
            marks: Vec::new(),
            disk_mtime: None,
            disk_missing: false,
            cursor: 0,
            anchor: 0,
            extra: Vec::new(),
            rect: None,
            view: View::default(),
            alt: None,
            folded: BTreeSet::new(),
            hidden_ranges: Vec::new(),
            fold_cache: None,
            hidden_cache: None,
            fold_gen: 0,
            hl: HlCache::default(),
            version: 0,
            pinned: false,
            tab_color: 0,
            monitoring: false,
            diff_marks: Default::default(),
            saved_state: 0,
            next_serial: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
        }
    }

    /// Loads a file; `encoding` forces an encoding instead of detecting one.
    pub fn open(id: u64, path: &Path, encoding: Option<TextEncoding>, default_eol: Eol) -> Result<Self, String> {
        let mut doc = Self::new_untitled(id, 0, default_eol);
        doc.path = Some(path.to_path_buf());
        doc.load_from_disk(encoding, default_eol)?;
        Ok(doc)
    }

    /// (Re)reads the bound file, replacing text and history.
    pub fn load_from_disk(&mut self, encoding: Option<TextEncoding>, default_eol: Eol) -> Result<(), String> {
        let path = self.path.clone().ok_or("No file")?;
        let meta = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.is_dir() {
            return Err(format!("{} is a directory", path.display()));
        }
        if meta.len() > MAX_FILE_SIZE {
            return Err(format!(
                "{} is too large ({} MB, maximum {} MB)",
                path.display(),
                meta.len() / 1_048_576,
                MAX_FILE_SIZE / 1_048_576
            ));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let encoding = encoding.unwrap_or_else(|| detect_encoding(&bytes));
        let (text, _) = encoding.decode(&bytes);
        let (text, eol, eols) = normalize_eol(text, default_eol);
        self.encoding = encoding;
        self.eol = eol;
        self.undo.clear();
        self.redo.clear();
        self.saved_state = 0;
        self.marks.clear();
        self.disk_mtime = meta.modified().ok();
        self.disk_missing = false;
        self.replace_text_raw(text);
        self.line_eols = eols;
        self.changes = vec![CHANGE_NONE; self.line_count()];
        let lines = self.line_count();
        self.bookmarks.retain(|&b| b < lines);
        self.folded.clear();
        self.hidden_ranges.clear();
        self.fold_gen += 1;
        Ok(())
    }

    /// Sets the content without history (loading, session restore).
    pub fn replace_text_raw(&mut self, text: String) {
        self.text = text;
        self.line_starts = compute_line_starts(&self.text);
        self.line_eols = None;
        self.changes = vec![CHANGE_NONE; self.line_starts.len()];
        let fix = |p: usize, t: &str| {
            let mut p = p.min(t.len());
            while !t.is_char_boundary(p) {
                p -= 1;
            }
            p
        };
        self.cursor = fix(self.cursor, &self.text);
        self.anchor = fix(self.anchor, &self.text);
        self.extra.clear();
        self.rect = None;
        if let Some(a) = &mut self.alt {
            a.cursor = fix(a.cursor, &self.text);
            a.anchor = fix(a.anchor, &self.text);
            a.extra.clear();
        }
        self.view.first_line = self.view.first_line.min(self.line_count() - 1);
        self.view.first_row = 0;
        self.view.max_width = 0.0;
        self.hl.invalidate_from(0);
        self.version += 1;
    }

    /// Text as written to disk (line endings applied).
    pub fn disk_text(&self) -> std::borrow::Cow<'_, str> {
        if let Some(eols) = &self.line_eols {
            let mut out = String::with_capacity(self.text.len() + eols.len());
            for (i, line) in self.text.split('\n').enumerate() {
                if i > 0 {
                    out.push_str(eols.get(i - 1).copied().unwrap_or(self.eol).as_str());
                }
                out.push_str(line);
            }
            return std::borrow::Cow::Owned(out);
        }
        if self.eol == Eol::Lf {
            std::borrow::Cow::Borrowed(self.text.as_str())
        } else {
            std::borrow::Cow::Owned(self.text.replace('\n', self.eol.as_str()))
        }
    }

    /// Encodes and writes the text to `path`. Returns a warning when characters could not be encoded.
    pub fn write_to(&self, path: &Path) -> Result<Option<String>, String> {
        let (bytes, lossy) = self.encoding.encode(&self.disk_text());
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(lossy.then(|| format!("Some characters cannot be represented in {} and were replaced.", self.encoding.label())))
    }

    pub fn save_as(&mut self, path: &Path) -> Result<Option<String>, String> {
        let warning = self.write_to(path)?;
        self.path = Some(path.to_path_buf());
        self.saved_state = self.state();
        self.disk_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        self.disk_missing = false;
        for c in &mut self.changes {
            if *c == CHANGE_MODIFIED {
                *c = CHANGE_SAVED;
            }
        }
        // Typing after saving starts a new undo step.
        self.last_edit = None;
        Ok(warning)
    }

    pub fn is_dirty(&self) -> bool {
        self.state() != self.saved_state
    }

    fn state(&self) -> u64 {
        self.undo.last().map_or(0, |g| g.serial)
    }

    /// Marks the document as modified although the text is unchanged (encoding/EOL change, session restore).
    pub fn force_dirty(&mut self) {
        self.saved_state = u64::MAX;
    }

    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()),
            None => format!("new {}", self.untitled_no),
        }
    }

    pub fn path_string(&self) -> Option<String> {
        self.path.as_ref().map(|p| p.display().to_string())
    }

    /// Sets the line ending of the whole document (EOL conversion).
    pub fn set_eol(&mut self, eol: Eol) {
        if self.eol != eol || self.line_eols.is_some() {
            self.eol = eol;
            self.line_eols = None;
            self.force_dirty();
        }
    }

    pub fn has_mixed_eols(&self) -> bool {
        self.line_eols.is_some()
    }

    /// EOL after `line` (for "Show End of Line").
    pub fn line_eol(&self, line: usize) -> Eol {
        self.line_eols.as_ref().and_then(|v| v.get(line).copied()).unwrap_or(self.eol)
    }

    pub fn line_change(&self, line: usize) -> u8 {
        self.changes.get(line).copied().unwrap_or(CHANGE_NONE)
    }

    pub fn clear_change_history(&mut self) {
        self.changes.iter_mut().for_each(|c| *c = CHANGE_NONE);
    }

    // ---- text access -------------------------------------------------------------------------

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Highlighting cache together with the text it describes.
    pub fn hl_parts(&mut self) -> (&mut HlCache, &str, &[usize]) {
        (&mut self.hl, &self.text, &self.line_starts)
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts.get(line).copied().unwrap_or(self.text.len())
    }

    /// Line content without the line break.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let start = self.line_start(line);
        let end = self.line_starts.get(line + 1).map_or(self.text.len(), |&s| s - 1);
        start..end
    }

    /// Line including its line break.
    pub fn line_range_full(&self, line: usize) -> Range<usize> {
        let start = self.line_start(line);
        let end = self.line_starts.get(line + 1).copied().unwrap_or(self.text.len());
        start..end
    }

    pub fn line_text(&self, line: usize) -> &str {
        &self.text[self.line_range(line)]
    }

    pub fn line_of(&self, pos: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= pos).saturating_sub(1)
    }

    /// 0-based line and 0-based char column.
    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        let line = self.line_of(pos);
        let start = self.line_starts[line];
        (line, self.text[start..pos.max(start)].chars().count())
    }

    /// Byte position of a char column in a line (clamped to the line end).
    pub fn pos_at_col(&self, line: usize, col: usize) -> usize {
        let r = self.line_range(line);
        self.text[r.clone()].char_indices().nth(col).map_or(r.end, |(i, _)| r.start + i)
    }

    pub fn clamp(&self, pos: usize) -> usize {
        let mut p = pos.min(self.text.len());
        while !self.text.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    pub fn prev_char(&self, pos: usize) -> usize {
        self.text[..pos].chars().next_back().map_or(pos, |c| pos - c.len_utf8())
    }

    pub fn next_char(&self, pos: usize) -> usize {
        self.text[pos..].chars().next().map_or(pos, |c| pos + c.len_utf8())
    }

    // ---- selection -----------------------------------------------------------------------------

    pub fn sel(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    pub fn has_sel(&self) -> bool {
        self.anchor != self.cursor
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.sel()]
    }

    /// All selections, primary first.
    pub fn all_sels(&self) -> Sels {
        let mut v = vec![(self.anchor, self.cursor)];
        v.extend(self.extra.iter().copied());
        v
    }

    /// Sets all selections (primary first); overlapping selections are merged.
    pub fn set_all_sels(&mut self, sels: Sels) {
        let Some(&(a, c)) = sels.first() else { return };
        self.anchor = self.clamp(a);
        self.cursor = self.clamp(c);
        let mut extra: Sels = Vec::new();
        for &(a, c) in &sels[1..] {
            let (a, c) = (self.clamp(a), self.clamp(c));
            let r = a.min(c)..a.max(c);
            let overlaps = |(x, y): (usize, usize)| {
                let o = x.min(y)..x.max(y);
                (o.start < r.end && r.start < o.end) || (o.is_empty() && r.is_empty() && o.start == r.start)
                    || (o.is_empty() && r.contains(&o.start))
                    || (r.is_empty() && o.contains(&r.start))
            };
            if overlaps((self.anchor, self.cursor)) || extra.iter().any(|&s| overlaps(s)) {
                continue;
            }
            extra.push((a, c));
        }
        self.extra = extra;
    }

    pub fn is_multi(&self) -> bool {
        !self.extra.is_empty()
    }

    pub fn set_sel(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.clamp(anchor);
        self.cursor = self.clamp(cursor);
        self.extra.clear();
        self.rect = None;
    }

    /// Selects a range (cursor at the end) and centers it in the view.
    pub fn select(&mut self, r: Range<usize>) {
        self.set_sel(r.start, r.end);
        self.view.preferred_x = None;
        self.view.scroll = Some(ScrollTo::Center);
        let l = self.line_of(r.start);
        self.ensure_line_visible(l);
    }

    pub fn goto_line(&mut self, line: usize) {
        let p = self.line_start(line.min(self.line_count() - 1));
        self.select(p..p);
    }

    /// Swaps the selection/scroll state with the one of the other view (cloned documents).
    pub fn swap_alt(&mut self) {
        let alt = self.alt.get_or_insert_with(|| {
            Box::new(ViewState { cursor: 0, anchor: 0, extra: Vec::new(), rect: None, view: View::default() })
        });
        std::mem::swap(&mut self.cursor, &mut alt.cursor);
        std::mem::swap(&mut self.anchor, &mut alt.anchor);
        std::mem::swap(&mut self.extra, &mut alt.extra);
        std::mem::swap(&mut self.rect, &mut alt.rect);
        std::mem::swap(&mut self.view, &mut alt.view);
    }

    // ---- folding & hidden lines -----------------------------------------------------------------

    /// For every line the last line of the fold it starts (`u32::MAX` = no fold header).
    pub fn fold_ends(&mut self) -> &[u32] {
        // Large files: while typing, the (slightly stale) previous structure is reused for a moment.
        let recompute = match &self.fold_cache {
            None => true,
            Some((v, _, t)) => *v != self.version && (self.text.len() < (2 << 20) || t.elapsed().as_millis() > 600),
        };
        if recompute {
            let mode = crate::fold::mode_for(self.syntax.as_deref(), self.user_lang.is_some());
            let ends = crate::fold::compute(&self.text, &self.line_starts, mode);
            self.fold_cache = Some((self.version, ends, Instant::now()));
        }
        &self.fold_cache.as_ref().unwrap().1
    }

    pub fn fold_end(&mut self, line: usize) -> Option<usize> {
        let e = *self.fold_ends().get(line)?;
        (e != u32::MAX).then_some(e as usize)
    }

    /// Merged ranges of hidden lines (folds and "Hide Lines").
    pub fn hidden(&mut self) -> &[Range<usize>] {
        let key = (self.version, self.fold_gen);
        if self.hidden_cache.as_ref().is_none_or(|(v, g, _)| (*v, *g) != key) {
            let mut ranges: Vec<Range<usize>> = self.hidden_ranges.clone();
            let folded: Vec<usize> = self.folded.iter().copied().collect();
            if !folded.is_empty() {
                let ends = self.fold_ends().to_vec();
                for h in folded {
                    if let Some(&e) = ends.get(h) {
                        if e != u32::MAX && e as usize > h {
                            ranges.push(h + 1..e as usize + 1);
                        }
                    }
                }
            }
            ranges.sort_by_key(|r| r.start);
            let mut merged: Vec<Range<usize>> = Vec::new();
            for r in ranges {
                match merged.last_mut() {
                    Some(m) if r.start <= m.end => m.end = m.end.max(r.end),
                    _ => merged.push(r),
                }
            }
            self.hidden_cache = Some((key.0, key.1, merged));
        }
        &self.hidden_cache.as_ref().unwrap().2
    }

    pub fn has_hidden(&mut self) -> bool {
        !(self.folded.is_empty() && self.hidden_ranges.is_empty()) && !self.hidden().is_empty()
    }

    pub fn is_line_hidden(&mut self, line: usize) -> bool {
        if self.folded.is_empty() && self.hidden_ranges.is_empty() {
            return false;
        }
        let h = self.hidden();
        let i = h.partition_point(|r| r.end <= line);
        h.get(i).is_some_and(|r| r.contains(&line))
    }

    /// First visible line at or after `line` (or the last visible line before it).
    pub fn visible_from(&mut self, line: usize) -> usize {
        if self.folded.is_empty() && self.hidden_ranges.is_empty() {
            return line.min(self.line_count() - 1);
        }
        let n = self.line_count();
        let h = self.hidden();
        let i = h.partition_point(|r| r.end <= line);
        match h.get(i) {
            Some(r) if r.contains(&line) => {
                if r.end < n {
                    r.end
                } else {
                    r.start.saturating_sub(1)
                }
            }
            _ => line.min(n - 1),
        }
    }

    /// Next visible line after `line`.
    pub fn next_visible(&mut self, line: usize) -> Option<usize> {
        let n = self.line_count();
        if line + 1 >= n {
            return None;
        }
        let l = self.visible_from(line + 1);
        (l > line).then_some(l)
    }

    /// Previous visible line before `line`.
    pub fn prev_visible(&mut self, line: usize) -> Option<usize> {
        if line == 0 {
            return None;
        }
        if self.folded.is_empty() && self.hidden_ranges.is_empty() {
            return Some(line - 1);
        }
        let h = self.hidden();
        let target = line - 1;
        let i = h.partition_point(|r| r.end <= target);
        match h.get(i) {
            Some(r) if r.contains(&target) => r.start.checked_sub(1),
            _ => Some(target),
        }
    }

    /// Unfolds/unhides everything that hides `line`.
    pub fn ensure_line_visible(&mut self, line: usize) {
        if !self.is_line_hidden(line) {
            return;
        }
        let ends = self.fold_ends().to_vec();
        self.folded.retain(|&h| !(h < line && ends.get(h).is_some_and(|&e| e != u32::MAX && line <= e as usize)));
        self.hidden_ranges.retain(|r| !r.contains(&line));
        self.fold_gen += 1;
    }

    pub fn toggle_fold(&mut self, line: usize) {
        if self.fold_end(line).is_none() {
            return;
        }
        if !self.folded.remove(&line) {
            self.folded.insert(line);
            // Move carets out of the folded block.
            let end = self.fold_end(line).unwrap_or(line);
            let le = self.line_range(line).end;
            let inside = |p: usize, d: &Self| {
                let l = d.line_of(p);
                l > line && l <= end
            };
            if inside(self.cursor, self) || inside(self.anchor, self) {
                self.set_sel(le, le);
            }
        }
        self.fold_gen += 1;
    }

    /// Folds (`fold = true`) or unfolds all headers; `level` limits to a nesting level (1 = outermost).
    pub fn fold_all(&mut self, fold: bool, level: Option<usize>) {
        let ends = self.fold_ends().to_vec();
        let mut depth_end: Vec<usize> = Vec::new();
        for (line, &e) in ends.iter().enumerate() {
            while depth_end.last().is_some_and(|&d| line > d) {
                depth_end.pop();
            }
            if e != u32::MAX {
                let depth = depth_end.len() + 1;
                if level.is_none_or(|l| l == depth) {
                    if fold {
                        self.folded.insert(line);
                    } else {
                        self.folded.remove(&line);
                    }
                }
                depth_end.push(e as usize);
            }
        }
        if !fold && level.is_none() {
            self.folded.clear();
        }
        if fold {
            let l = self.line_of(self.cursor);
            let v = self.visible_from(l);
            if v != l {
                let p = self.line_start(v);
                self.set_sel(p, p);
            }
        }
        self.fold_gen += 1;
    }

    /// Fold header containing `line` (innermost).
    pub fn enclosing_fold(&mut self, line: usize) -> Option<usize> {
        let ends = self.fold_ends();
        (0..=line).rev().find(|&h| ends.get(h).is_some_and(|&e| e != u32::MAX && e as usize >= line))
    }

    pub fn hide_lines(&mut self, r: Range<usize>) {
        if r.is_empty() {
            return;
        }
        self.hidden_ranges.push(r.clone());
        let p = self.line_start(r.end.min(self.line_count() - 1));
        let p = if r.end >= self.line_count() { self.line_start(r.start.saturating_sub(1)) } else { p };
        self.set_sel(p, p);
        self.fold_gen += 1;
    }

    pub fn show_hidden_at(&mut self, line: usize) {
        self.hidden_ranges.retain(|r| !(r.start == line + 1 || r.end == line || r.contains(&line)));
        self.fold_gen += 1;
    }

    // ---- editing -------------------------------------------------------------------------------

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Replaces `range` with `with`; the cursor ends up after the inserted text.
    pub fn replace(&mut self, range: Range<usize>, with: &str, kind: EditKind) {
        if self.read_only || (range.is_empty() && with.is_empty()) {
            return;
        }
        let sel_before = self.all_sels();
        let edit = Edit { pos: range.start, removed: self.text[range.clone()].to_string(), inserted: with.to_string() };
        self.apply(&edit);
        let c = range.start + with.len();
        self.anchor = c;
        self.cursor = c;
        self.extra.clear();
        self.rect = None;
        let now = Instant::now();
        let recent = self.last_edit.is_some_and(|t| now.duration_since(t).as_secs_f32() < 2.0);
        let merged = kind != EditKind::Other
            && recent
            && self.state() != self.saved_state
            && self.redo.is_empty()
            && self
                .undo
                .last_mut()
                .is_some_and(|g| g.kind == kind && g.edits.len() == 1 && try_merge(g.edits.last_mut().unwrap(), &edit));
        if merged {
            self.undo.last_mut().unwrap().sel_after = vec![(c, c)];
        } else {
            self.push_group(Group { edits: vec![edit], sel_before, sel_after: vec![(c, c)], kind, serial: 0 });
        }
        self.last_edit = Some(now);
        self.view.preferred_x = None;
        self.view.scroll = Some(ScrollTo::Cursor);
    }

    /// Several non-overlapping replacements as one undo step (multi-editing).
    /// `edits[i]` belongs to selection i; afterwards each selection is a caret after its insertion
    /// (or selects the inserted text when `select` is set).
    pub fn replace_multi(&mut self, edits: Vec<(Range<usize>, String)>, select: bool) {
        if self.read_only || edits.is_empty() {
            return;
        }
        if edits.len() == 1 && !select {
            let (r, s) = edits.into_iter().next().unwrap();
            self.replace(r, &s, EditKind::Other);
            return;
        }
        let sel_before = self.all_sels();
        let mut order: Vec<usize> = (0..edits.len()).collect();
        order.sort_by_key(|&i| edits[i].0.start);
        // New positions: shift by the size changes of all earlier edits.
        let mut result = vec![(0usize, 0usize); edits.len()];
        let mut shift: isize = 0;
        for &i in &order {
            let (r, s) = &edits[i];
            let start = (r.start as isize + shift) as usize;
            result[i] = if select { (start, start + s.len()) } else { (start + s.len(), start + s.len()) };
            shift += s.len() as isize - r.len() as isize;
        }
        let mut applied = Vec::new();
        for &i in order.iter().rev() {
            let (r, s) = &edits[i];
            if r.is_empty() && s.is_empty() {
                continue;
            }
            let e = Edit { pos: r.start, removed: self.text[r.clone()].to_string(), inserted: s.clone() };
            self.apply(&e);
            applied.push(e);
        }
        if applied.is_empty() {
            return;
        }
        self.rect = None;
        self.set_all_sels(result.clone());
        self.push_group(Group { edits: applied, sel_before, sel_after: result, kind: EditKind::Other, serial: 0 });
        self.last_edit = None;
        self.view.preferred_x = None;
        self.view.scroll = Some(ScrollTo::Cursor);
    }

    /// Replaces the whole text with `new` as one undo step and sets the selection (bytes).
    pub fn set_text(&mut self, new: String, sel: Range<usize>) {
        if self.read_only {
            return;
        }
        if new != self.text {
            let sel_before = self.all_sels();
            let edit = diff(&self.text, &new);
            self.apply(&edit);
            self.push_group(Group {
                edits: vec![edit],
                sel_before,
                sel_after: vec![(sel.start, sel.end)],
                kind: EditKind::Other,
                serial: 0,
            });
            self.last_edit = None;
        }
        self.set_sel(sel.start, sel.end);
        self.view.preferred_x = None;
        self.view.scroll = Some(ScrollTo::Cursor);
    }

    pub fn undo(&mut self) {
        let Some(g) = self.undo.pop() else { return };
        for e in g.edits.iter().rev() {
            self.apply(&Edit { pos: e.pos, removed: e.inserted.clone(), inserted: e.removed.clone() });
        }
        self.rect = None;
        self.set_all_sels(g.sel_before.clone());
        self.redo.push(g);
        self.last_edit = None;
        self.view.preferred_x = None;
        self.view.scroll = Some(ScrollTo::Cursor);
    }

    pub fn redo(&mut self) {
        let Some(g) = self.redo.pop() else { return };
        for e in &g.edits {
            self.apply(e);
        }
        self.rect = None;
        self.set_all_sels(g.sel_after.clone());
        self.undo.push(g);
        self.last_edit = None;
        self.view.preferred_x = None;
        self.view.scroll = Some(ScrollTo::Cursor);
    }

    fn push_group(&mut self, mut g: Group) {
        g.serial = self.next_serial;
        self.next_serial += 1;
        self.undo.push(g);
        self.redo.clear();
        // Bound memory: drop the oldest steps.
        if self.undo.len() > 5000 {
            self.undo.drain(..1000);
        }
    }

    /// Applies an edit to text, line index, annotations and highlighting cache.
    fn apply(&mut self, e: &Edit) {
        let line = self.line_of(e.pos);
        let removed_end = e.pos + e.removed.len();
        let delta = e.inserted.len() as isize - e.removed.len() as isize;

        // Line index: drop starts inside the removed text, shift the rest, add new ones.
        let first = line + 1;
        let last = self.line_starts.partition_point(|&s| s <= removed_end);
        let new_starts: Vec<usize> = e.inserted.match_indices('\n').map(|(i, _)| e.pos + i + 1).collect();
        let inserted_nl = new_starts.len();
        let removed_nl = last - first;
        if delta != 0 {
            for s in &mut self.line_starts[last..] {
                *s = (*s as isize + delta) as usize;
            }
        }
        self.line_starts.splice(first..last, new_starts);
        if let Some(eols) = &mut self.line_eols {
            // eols[i] = EOL after line i
            let eol = self.eol;
            let a = line.min(eols.len());
            let b = (line + removed_nl).min(eols.len());
            eols.splice(a..b, std::iter::repeat_n(eol, inserted_nl));
        }
        self.changes.splice(first..last, std::iter::repeat_n(CHANGE_MODIFIED, inserted_nl));
        if let Some(c) = self.changes.get_mut(line) {
            *c = CHANGE_MODIFIED;
        }

        self.text.replace_range(e.pos..removed_end, &e.inserted);

        // Marks
        self.marks.retain_mut(|(m, _)| {
            if m.end <= e.pos {
                true
            } else if m.start >= removed_end {
                m.start = (m.start as isize + delta) as usize;
                m.end = (m.end as isize + delta) as usize;
                true
            } else {
                false
            }
        });
        // Line-based annotations stay on their lines.
        if removed_nl > 0 || inserted_nl > 0 {
            // Text inserted at the very start of a line pushes that line down.
            let at_start = e.pos == self.line_starts[line] && !e.inserted.is_empty() && e.removed.is_empty();
            let shift = |b: usize| -> Option<usize> {
                if b < line || (b == line && !at_start) {
                    Some(b)
                } else if b > line && b <= line + removed_nl {
                    None
                } else {
                    Some((b as isize + inserted_nl as isize - removed_nl as isize) as usize)
                }
            };
            if !self.bookmarks.is_empty() {
                self.bookmarks = self.bookmarks.iter().map(|&b| shift(b).unwrap_or(line)).collect();
            }
            if !self.folded.is_empty() {
                self.folded = self.folded.iter().filter_map(|&b| shift(b)).collect();
            }
            if !self.hidden_ranges.is_empty() {
                self.hidden_ranges = self
                    .hidden_ranges
                    .iter()
                    .filter_map(|r| {
                        let s = shift(r.start)?;
                        let e2 = shift(r.end - 1)? + 1;
                        (e2 > s).then_some(s..e2)
                    })
                    .collect();
            }
            self.fold_gen += 1;
        }
        // Keep selections pointing at the same text where possible.
        let fix = |p: usize| {
            if p <= e.pos {
                p
            } else if p >= removed_end {
                (p as isize + delta) as usize
            } else {
                e.pos + e.inserted.len()
            }
        };
        self.cursor = fix(self.cursor);
        self.anchor = fix(self.anchor);
        for s in &mut self.extra {
            *s = (fix(s.0), fix(s.1));
        }
        if let Some(a) = &mut self.alt {
            a.cursor = fix(a.cursor);
            a.anchor = fix(a.anchor);
            for s in &mut a.extra {
                *s = (fix(s.0), fix(s.1));
            }
        }
        self.hl.invalidate_from(line);
        self.diff_marks.clear();
        self.version += 1;
    }
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect()
}

/// Single edit transforming `old` into `new` (common prefix/suffix removed).
fn diff(old: &str, new: &str) -> Edit {
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(pre) || !new.is_char_boundary(pre) {
        pre -= 1;
    }
    let max_suf = a.len().min(b.len()) - pre;
    let mut suf = a.iter().rev().zip(b.iter().rev()).take(max_suf).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(a.len() - suf) || !new.is_char_boundary(b.len() - suf) {
        suf -= 1;
    }
    Edit { pos: pre, removed: old[pre..a.len() - suf].to_string(), inserted: new[pre..b.len() - suf].to_string() }
}

/// Merges consecutive typing / backspace / delete into one undo step.
fn try_merge(last: &mut Edit, e: &Edit) -> bool {
    let ws = |s: &str| s.chars().any(char::is_whitespace);
    if last.removed.is_empty() && e.removed.is_empty() && !e.inserted.contains('\n') {
        // Typing: a word after whitespace starts a new step, like most editors.
        if e.pos == last.pos + last.inserted.len() && !(ws(&last.inserted) && !ws(&e.inserted)) {
            last.inserted.push_str(&e.inserted);
            return true;
        }
    } else if last.inserted.is_empty() && e.inserted.is_empty() && !e.removed.contains('\n') {
        if e.pos + e.removed.len() == last.pos {
            last.pos = e.pos;
            last.removed.insert_str(0, &e.removed);
            return true;
        }
        if e.pos == last.pos {
            last.removed.push_str(&e.removed);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eol_detection() {
        assert_eq!(normalize_eol("a\r\nb\r\n".into(), Eol::Lf), ("a\nb\n".into(), Eol::Crlf, None));
        assert_eq!(normalize_eol("a\nb".into(), Eol::Crlf), ("a\nb".into(), Eol::Lf, None));
        assert_eq!(normalize_eol("a\rb\r".into(), Eol::Lf), ("a\nb\n".into(), Eol::Cr, None));
        assert_eq!(normalize_eol("abc".into(), Eol::Crlf).1, Eol::Crlf);
        let (t, e, m) = normalize_eol("a\r\nb\nc\r\n".into(), Eol::Lf);
        assert_eq!((t.as_str(), e), ("a\nb\nc\n", Eol::Crlf));
        assert_eq!(m, Some(vec![Eol::Crlf, Eol::Lf, Eol::Crlf]));
    }

    #[test]
    fn mixed_eols_roundtrip() {
        let mut d = Document::new_untitled(1, 1, Eol::Lf);
        let (t, e, m) = normalize_eol("a\r\nb\nc".into(), Eol::Lf);
        d.replace_text_raw(t);
        d.eol = e;
        d.line_eols = m;
        assert_eq!(d.disk_text(), "a\r\nb\nc");
        d.replace(1..1, "X\nY", EditKind::Other);
        assert_eq!(d.disk_text(), "aX\r\nY\r\nb\nc", "new line gets the dominant EOL");
        d.undo();
        assert_eq!(d.disk_text(), "a\r\nb\nc");
    }

    #[test]
    fn encodings_roundtrip() {
        let t = "Grüße €";
        for enc in [
            TextEncoding::utf8(),
            TextEncoding::new(encoding_rs::UTF_8, true),
            TextEncoding::new(encoding_rs::UTF_16LE, true),
            TextEncoding::new(encoding_rs::UTF_16BE, true),
            TextEncoding::new(encoding_rs::WINDOWS_1252, false),
        ] {
            let (bytes, lossy) = enc.encode(t);
            assert!(!lossy);
            assert_eq!(detect_encoding(&bytes).name, enc.name, "{}", enc.name);
            assert_eq!(enc.decode(&bytes).0, t);
        }
        assert!(TextEncoding::new(encoding_rs::WINDOWS_1252, false).encode("日本").1);
    }

    fn check_lines(d: &Document) {
        assert_eq!(d.line_starts, compute_line_starts(&d.text), "line index for {:?}", d.text);
        assert_eq!(d.changes.len(), d.line_starts.len());
    }

    #[test]
    fn edits_and_undo() {
        let mut d = Document::new_untitled(1, 1, Eol::Lf);
        for (i, c) in "hallo".chars().enumerate() {
            d.replace(i..i, &c.to_string(), EditKind::Typing);
        }
        d.replace(5..5, "\nwelt", EditKind::Typing);
        check_lines(&d);
        assert_eq!(d.line_count(), 2);
        assert_eq!(d.line_text(1), "welt");
        assert!(d.is_dirty());
        d.set_text("HALLO\nwelt".into(), 0..5);
        d.replace(0..10, "", EditKind::Delete);
        check_lines(&d);
        d.undo();
        assert_eq!(d.text(), "HALLO\nwelt");
        d.undo();
        d.undo();
        assert_eq!(d.text(), "hallo");
        d.undo();
        assert_eq!(d.text(), "");
        check_lines(&d);
        assert!(!d.is_dirty());
        d.redo();
        d.redo();
        d.redo();
        assert_eq!(d.text(), "HALLO\nwelt");
        check_lines(&d);
    }

    #[test]
    fn multi_edit() {
        let mut d = Document::new_untitled(1, 1, Eol::Lf);
        d.set_text("ab\nab\nab".into(), 0..0);
        d.set_all_sels(vec![(1, 1), (4, 4), (7, 7)]);
        let edits = d.all_sels().iter().map(|&(a, c)| (a.min(c)..a.max(c), "X".to_string())).collect();
        d.replace_multi(edits, false);
        assert_eq!(d.text(), "aXb\naXb\naXb");
        assert_eq!(d.all_sels(), vec![(2, 2), (6, 6), (10, 10)]);
        d.undo();
        assert_eq!(d.text(), "ab\nab\nab");
        assert_eq!(d.all_sels(), vec![(1, 1), (4, 4), (7, 7)]);
        check_lines(&d);
    }

    #[test]
    fn line_index_random_edits() {
        let mut d = Document::new_untitled(1, 1, Eol::Lf);
        let mut seed = 7u32;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            (seed >> 16) as usize % n.max(1)
        };
        for _ in 0..500 {
            let len = d.text().len();
            let a = d.clamp(rnd(len + 1));
            let b = d.clamp((a + rnd(6)).min(len));
            let ins = ["", "x", "\n", "a\nb", "\n\n", "ä"][rnd(6)];
            d.replace(a..b, ins, EditKind::Other);
            check_lines(&d);
        }
        while d.can_undo() {
            d.undo();
            check_lines(&d);
        }
        assert_eq!(d.text(), "");
    }

    #[test]
    fn diff_multibyte() {
        let e = diff("aäb", "aöb");
        assert_eq!((e.pos, e.removed.as_str(), e.inserted.as_str()), (1, "ä", "ö"));
        let e = diff("ää", "ä");
        assert_eq!((e.removed.as_str(), e.inserted.as_str()), ("ä", ""));
    }

    #[test]
    fn bookmarks_follow_lines() {
        let mut d = Document::new_untitled(1, 1, Eol::Lf);
        d.set_text("a\nb\nc\n".into(), 0..0);
        d.bookmarks.insert(2);
        d.replace(0..0, "x\n", EditKind::Other);
        assert!(d.bookmarks.contains(&3));
        d.undo();
        assert!(d.bookmarks.contains(&2));
        assert_eq!(d.line_col(4), (2, 0));
    }

    #[test]
    fn folding_and_hidden_lines() {
        let mut d = Document::new_untitled(1, 1, Eol::Lf);
        d.syntax = Some("Rust".into());
        d.set_text("fn a() {\n    x;\n    y;\n}\nfn b() {\n}\n".into(), 0..0);
        assert_eq!(d.fold_end(0), Some(3));
        assert_eq!(d.fold_end(1), None);
        d.toggle_fold(0);
        assert!(d.is_line_hidden(1) && d.is_line_hidden(3) && !d.is_line_hidden(4));
        assert_eq!(d.next_visible(0), Some(4));
        assert_eq!(d.prev_visible(4), Some(0));
        d.replace(0..0, "// c\n", EditKind::Other);
        assert!(d.folded.contains(&1), "fold header follows its line");
        d.ensure_line_visible(3);
        assert!(d.folded.is_empty());
        d.hide_lines(1..3);
        assert!(d.is_line_hidden(2) && !d.is_line_hidden(3));
        d.show_hidden_at(0);
        assert!(!d.is_line_hidden(2));
        d.fold_all(true, None);
        assert_eq!(d.folded.len(), 2);
        d.fold_all(false, None);
        assert!(d.folded.is_empty());
    }

    #[test]
    fn change_history() {
        let mut d = Document::new_untitled(1, 1, Eol::Lf);
        d.replace_text_raw("a\nb\nc".into());
        d.replace(2..2, "x\ny", EditKind::Other);
        assert_eq!(d.changes, vec![0, 1, 1, 0]);
        let dir = std::env::temp_dir().join(format!("nrs-ch-{}", std::process::id()));
        d.save_as(&dir).unwrap();
        assert_eq!(d.changes, vec![0, 2, 2, 0]);
        let _ = std::fs::remove_file(dir);
    }
}
