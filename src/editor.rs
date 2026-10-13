//! The text editor widget. Only visible lines are laid out and highlighted, so files with
//! millions of lines stay responsive (like Scintilla in Notepad++).
//!
//! Supports multiple carets (Ctrl+click), column selection (Alt+drag, Alt+Shift+arrows),
//! folding, hidden lines, drag & drop of text, auto-completion and the Notepad++ margins.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::text::CCursor;
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, Event, EventFilter, FontId, Galley, Id, Key, Modifiers, Pos2,
    Rect, Sense, Shape, Stroke, Ui,
};

use crate::cmd::Cmd;
use crate::doc::{Document, EditKind, RectSel, ScrollTo, CHANGE_MODIFIED, CHANGE_SAVED};
use crate::highlight::{self, Highlighting, JobOpts};
use crate::settings::{Completion as CompletionMode, CurrentLine, EdgeMode, Rgba, Settings, TabSetting};
use crate::textops;
use crate::udl::UserLang;

pub fn rgba(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

pub struct Palette {
    pub bg: Color32,
    pub fg: Color32,
    pub gutter_bg: Color32,
    pub gutter_fg: Color32,
    pub gutter_cur: Color32,
    pub line_hl: Color32,
    pub selection: Color32,
    pub caret: Color32,
    pub whitespace: Color32,
    pub smart: Color32,
    pub marks: [Color32; 6],
    pub brace: Color32,
    pub bookmark: Color32,
    pub edge: Color32,
    pub guide: Color32,
    pub change_modified: Color32,
    pub change_saved: Color32,
    pub url: Color32,
    pub fold: Color32,
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn blend_over(top: Color32, bottom: Color32) -> Color32 {
    let a = top.a() as f32 / 255.0;
    let l = |t: u8, b: u8| (t as f32 * a + b as f32 * (1.0 - a)) as u8;
    Color32::from_rgb(l(top.r(), bottom.r()), l(top.g(), bottom.g()), l(top.b(), bottom.b()))
}

impl Palette {
    pub fn new(theme: &syntect::highlighting::Theme, s: &Settings) -> Self {
        let c = highlight::theme_colors(theme);
        let ts = &theme.settings;
        let st = &s.styles;
        let opt = |o: Option<syntect::highlighting::Color>, d: Color32| o.map_or(d, highlight::to_color);
        let solid = |c: Color32, bg: Color32| if c.a() < 255 { blend_over(c, bg) } else { c };
        let selection = st.selection.map(rgba).unwrap_or_else(|| solid(opt(ts.selection, mix(c.bg, Color32::from_rgb(60, 120, 220), 0.35)), c.bg));
        let line_hl = st.current_line.map(rgba).unwrap_or_else(|| solid(opt(ts.line_highlight, mix(c.bg, c.fg, 0.07)), c.bg));
        let dark = c.bg.r() as u32 + c.bg.g() as u32 + c.bg.b() as u32 <= 384;
        Self {
            bg: c.bg,
            fg: c.fg,
            gutter_bg: mix(c.bg, c.fg, 0.06),
            gutter_fg: st.line_number.map(rgba).unwrap_or(mix(c.bg, c.fg, 0.45)),
            gutter_cur: c.fg,
            line_hl,
            selection,
            caret: st.caret.map(rgba).unwrap_or(opt(ts.caret, c.fg)),
            whitespace: mix(c.bg, c.fg, 0.35),
            smart: rgba(st.smart_highlight),
            marks: [
                rgba(st.find_mark),
                rgba(st.mark_styles[0]),
                rgba(st.mark_styles[1]),
                rgba(st.mark_styles[2]),
                rgba(st.mark_styles[3]),
                rgba(st.mark_styles[4]),
            ],
            brace: rgba(st.brace),
            bookmark: rgba(st.bookmark),
            edge: rgba(st.edge),
            guide: st.indent_guide.map(rgba).unwrap_or(mix(c.bg, c.fg, 0.2)),
            change_modified: rgba(st.change_modified),
            change_saved: rgba(st.change_saved),
            url: if dark { Color32::from_rgb(100, 170, 255) } else { rgba(st.url) },
            fold: mix(c.bg, c.fg, 0.5),
        }
    }
}

/// Auto-completion popup state.
#[derive(Clone, Debug, Default)]
pub struct Completion {
    pub items: Vec<String>,
    pub selected: usize,
    /// Byte position where the completed word starts.
    pub start: usize,
    pub doc_id: u64,
}

pub struct Cfg<'a> {
    pub hl: &'a Highlighting,
    pub s: &'a Settings,
    pub pal: &'a Palette,
    /// Syntax used for highlighting (None when switched off or plain text).
    pub syntax: Option<&'a str>,
    pub udl: Option<&'a UserLang>,
    pub overwrite: &'a mut bool,
    /// Commands from the context menu.
    pub cmds: &'a mut Vec<Cmd>,
    pub completion: &'a mut Option<Completion>,
    /// Function parameter hint.
    pub calltip: &'a mut Option<crate::api::CallTip>,
    /// Notepad++ API file of the language.
    pub api: Option<&'static crate::api::Api>,
    /// Language keywords for completion.
    pub keywords: &'a [String],
    pub tabs: TabSetting,
    /// Large file: folding, smart highlighting, links etc. off.
    pub large: bool,
    /// 0 = main view, 1 = second view
    pub view: usize,
    /// Text copied to the clipboard in this frame (for the clipboard history).
    pub copied: &'a mut Option<String>,
}

#[derive(Default)]
pub struct Output {
    pub focused: bool,
    pub clicked: bool,
}

pub fn editor_id(doc_id: u64, view: usize) -> Id {
    Id::new(("editor", doc_id, view))
}

/// Lays out single lines.
struct Lay {
    font: FontId,
    fg: Color32,
    tab_width: usize,
    char_w: f32,
    wrap_width: f32,
    use_hl: bool,
    show_ctrl: bool,
    show_non_printing: bool,
    italic_comments: bool,
    links: bool,
    url_color: Color32,
}

fn url_regex() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"(?:https?|ftp|file)://[^\s<>"'`]+[^\s<>"'`.,;:!?)\]}]|mailto:[^\s<>"']+|www\.[^\s<>"'`]+[^\s<>"'`.,;:!?)\]}]"#).unwrap())
}

pub fn urls_in(line: &str) -> Vec<Range<usize>> {
    if line.len() > 5000 || !(line.contains("://") || line.contains("mailto:") || line.contains("www.")) {
        return Vec::new();
    }
    url_regex().find_iter(line).map(|m| m.range()).collect()
}

impl Lay {
    fn galley(&self, ui: &Ui, doc: &Document, line: usize) -> Arc<Galley> {
        let spans = if self.use_hl { doc.hl.line_spans(line) } else { None };
        let text = doc.line_text(line);
        let urls = if self.links { urls_in(text) } else { Vec::new() };
        let o = JobOpts {
            font: &self.font,
            fg: self.fg,
            tab_width: self.tab_width,
            char_w: self.char_w,
            show_ctrl: self.show_ctrl,
            show_non_printing: self.show_non_printing,
            italic_comments: self.italic_comments,
            urls: &urls,
            url_color: self.url_color,
        };
        let mut job = highlight::line_job(text, spans, &o);
        job.wrap.max_width = self.wrap_width;
        ui.fonts(|f| f.layout_job(job))
    }

    fn rows(&self, ui: &Ui, doc: &Document, line: usize) -> usize {
        if self.wrap_width.is_finite() {
            self.galley(ui, doc, line).rows.len().max(1)
        } else {
            1
        }
    }
}

struct VisLine {
    line: usize,
    galley: Arc<Galley>,
    /// Screen y of the galley top.
    top: f32,
}

fn char_index(line: &str, byte: usize) -> usize {
    line[..byte.min(line.len())].chars().count()
}

fn byte_index(line: &str, ci: usize) -> usize {
    line.char_indices().nth(ci).map_or(line.len(), |(b, _)| b)
}

fn ccursor(doc: &Document, ci: usize) -> CCursor {
    CCursor { index: ci, prefer_next_row: !doc.view.row_end_affinity }
}

#[derive(PartialEq, Clone, Copy)]
enum Class {
    Space,
    Word,
    Punct,
}

fn class(c: char, extra: &str) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' || extra.contains(c) {
        Class::Word
    } else {
        Class::Punct
    }
}

pub fn word_left(doc: &Document, pos: usize, extra: &str) -> usize {
    let t = doc.text();
    let mut it = t[..pos].char_indices().rev().peekable();
    let mut p = pos;
    if let Some(&(i, '\n')) = it.peek() {
        return i;
    }
    while let Some(&(i, c)) = it.peek() {
        if c == '\n' || class(c, extra) != Class::Space {
            break;
        }
        p = i;
        it.next();
    }
    if let Some(&(_, c)) = it.peek() {
        if c == '\n' {
            return p;
        }
        let k = class(c, extra);
        while let Some(&(i, c)) = it.peek() {
            if c == '\n' || class(c, extra) != k {
                break;
            }
            p = i;
            it.next();
        }
    }
    p
}

pub fn word_right(doc: &Document, pos: usize, extra: &str) -> usize {
    let t = doc.text();
    let mut it = t[pos..].char_indices().map(|(i, c)| (pos + i, c)).peekable();
    if let Some(&(i, '\n')) = it.peek() {
        return i + 1;
    }
    let mut p = pos;
    if let Some(&(_, c)) = it.peek() {
        let k = class(c, extra);
        if k != Class::Space {
            while let Some(&(i, c)) = it.peek() {
                if class(c, extra) != k || c == '\n' {
                    break;
                }
                p = i + c.len_utf8();
                it.next();
            }
        }
    }
    while let Some(&(i, c)) = it.peek() {
        if c == '\n' || class(c, extra) != Class::Space {
            break;
        }
        p = i + c.len_utf8();
        it.next();
    }
    p
}

/// Word (identifier characters plus `extra`) around a byte position.
pub fn word_at(text: &str, pos: usize, extra: &str) -> Option<Range<usize>> {
    let is_w = |c: char| c.is_alphanumeric() || c == '_' || extra.contains(c);
    let start = text[..pos].char_indices().rev().take_while(|(_, c)| is_w(*c)).last().map_or(pos, |(i, _)| i);
    let end = text[pos..].char_indices().find(|(_, c)| !is_w(*c)).map_or(text.len(), |(i, _)| pos + i);
    (start < end).then_some(start..end)
}

#[derive(Clone, Copy, PartialEq, Default)]
enum DragMode {
    #[default]
    None,
    /// Normal text selection
    Select,
    /// Selecting whole lines from the line number margin
    Lines(usize),
    /// Alt+drag column selection
    Rect,
    /// Pressed inside a selection; becomes a text move once dragged
    MaybeMove,
    MoveText,
    /// Ctrl+drag: adds a selection
    AddSel,
}

#[derive(Clone, Copy, Default)]
struct DragState {
    mode: DragMode,
    anchor: usize,
    press: Option<Pos2>,
}

struct Geo {
    gutter: Rect,
    mark_x: f32,
    num_rect: Option<(f32, f32)>,
    change_x: Option<f32>,
    fold_rect: Option<(f32, f32)>,
    text_rect: Rect,
    row_h: f32,
    char_w: f32,
}

pub fn show(ui: &mut Ui, doc: &mut Document, cfg: Cfg<'_>) -> Output {
    let id = editor_id(doc.id, cfg.view);
    let rect = ui.available_rect_before_wrap();
    ui.advance_cursor_after_rect(rect);
    let s = cfg.s;
    let pal = cfg.pal;
    let large = cfg.large;
    let extra_word = s.word_chars.clone();

    let font = FontId::monospace(s.font_size);
    let (row_h, char_w) = ui.fonts(|f| (f.row_height(&font), f.glyph_width(&font, ' ')));
    let wrap = s.word_wrap;
    // Margins: bookmark | line numbers | change history | fold
    let digits = if s.line_numbers_dynamic { doc.line_count().to_string().len().max(3) } else { doc.line_count().to_string().len().max(6) };
    let mut x = rect.min.x;
    let mark_x = x;
    if s.bookmark_margin {
        x += (row_h * 0.9).round() + 2.0;
    }
    let num_rect = s.line_numbers.then(|| {
        let w = digits as f32 * char_w * 0.92 + char_w;
        let r = (x, x + w);
        x += w;
        r
    });
    let change_x = (s.change_history).then(|| {
        let r = x + 2.0;
        x += 6.0;
        r
    });
    let foldable = s.fold_margin && !large && crate::fold::mode_for(doc.syntax.as_deref(), doc.user_lang.is_some()) != crate::fold::Mode::None;
    let fold_rect = foldable.then(|| {
        let w = (row_h * 0.85).round();
        let r = (x, x + w);
        x += w;
        r
    });
    x += 2.0;
    let sb_w = 12.0;
    let gutter = Rect::from_min_max(rect.min, pos2(x, rect.max.y));
    let text_rect = Rect::from_min_max(
        pos2(gutter.max.x + 4.0, rect.min.y),
        pos2(rect.max.x - sb_w, rect.max.y - if wrap { 0.0 } else { sb_w }),
    );
    let geo = Geo { gutter, mark_x, num_rect, change_x, fold_rect, text_rect, row_h, char_w };
    let lay = Lay {
        font: font.clone(),
        fg: pal.fg,
        tab_width: cfg.tabs.width,
        char_w,
        wrap_width: if wrap { (text_rect.width() - 4.0).max(char_w * 8.0) } else { f32::INFINITY },
        use_hl: cfg.syntax.is_some() || cfg.udl.is_some(),
        show_ctrl: s.show_ctrl_chars,
        show_non_printing: s.show_non_printing,
        italic_comments: s.styles.italic_comments,
        links: s.clickable_links && !large,
        url_color: pal.url,
    };
    if wrap {
        doc.view.x_offset = 0.0;
    }
    let vis_rows = ((text_rect.height() / row_h).floor() as usize).max(1);
    {
        let fl = doc.view.first_line;
        doc.view.first_line = doc.visible_from(fl);
    }

    // ---- interaction ---------------------------------------------------------------------------
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let hover = ui.input(|i| i.pointer.hover_pos());
    if resp.hovered() {
        if let Some(p) = hover {
            if text_rect.contains(p) {
                let drag: DragState = ui.data(|d| d.get_temp(id.with("drag")).unwrap_or_default());
                ui.ctx().set_cursor_icon(if drag.mode == DragMode::MoveText { CursorIcon::Grabbing } else { CursorIcon::Text });
            } else if gutter.contains(p) {
                ui.ctx().set_cursor_icon(CursorIcon::Default);
            }
        }
    }
    if resp.clicked() || resp.drag_started() || resp.secondary_clicked() {
        resp.request_focus();
    }
    let focused = ui.memory(|m| m.has_focus(id));
    if focused {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(id, EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })
        });
    }

    let mut activity = false;
    let visible = layout_visible(ui, doc, &lay, text_rect);

    // ---- mouse ---------------------------------------------------------------------------------
    let (mods, pressed, released, ptr) =
        ui.input(|i| (i.modifiers, i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.interact_pos()));
    let mut drag: DragState = ui.data(|d| d.get_temp(id.with("drag")).unwrap_or_default());
    if let Some(p) = ptr {
        if pressed && resp.hovered() && gutter.contains(p) {
            if let Some(line) = line_at(&visible, doc, p.y) {
                let in_fold = fold_rect.is_some_and(|(a, b)| p.x >= a && p.x < b);
                let in_marks = s.bookmark_margin && p.x < mark_x + (row_h * 0.9).round() + 2.0;
                if in_fold {
                    if doc.fold_end(line).is_some() {
                        doc.toggle_fold(line);
                    } else if doc.hidden_ranges.iter().any(|r| r.start == line + 1 || r.end == line) {
                        doc.show_hidden_at(line);
                    }
                } else if in_marks {
                    if !doc.bookmarks.remove(&line) {
                        doc.bookmarks.insert(line);
                    }
                } else {
                    select_lines(doc, line, line);
                    drag = DragState { mode: DragMode::Lines(line), ..Default::default() };
                }
            }
        } else if pressed && resp.hovered() && text_rect.contains(p) {
            let pos = pos_at(doc, &visible, text_rect, p);
            *cfg.completion = None;
            if mods.alt {
                // Column selection
                let line = doc.line_of(pos);
                let x = p.x - (text_rect.left() - doc.view.x_offset);
                let anchor = if mods.shift { doc.rect.map(|r| (r.anchor_line, r.anchor_x)) } else { None };
                let (al, ax) = anchor.unwrap_or((line, x));
                let r = RectSel { anchor_line: al, anchor_x: ax, caret_line: line, caret_x: x };
                apply_rect(ui, doc, &lay, r);
                drag = DragState { mode: DragMode::Rect, ..Default::default() };
            } else if (mods.ctrl || mods.command) && s.multi_editing {
                // Add a caret (or remove one that is already there).
                let mut sels = doc.all_sels();
                if let Some(i) = sels.iter().position(|&(a, c)| a.min(c) <= pos && pos <= a.max(c)).filter(|_| sels.len() > 1) {
                    sels.remove(i);
                    doc.set_all_sels(sels);
                } else {
                    let mut v = vec![(pos, pos), (doc.anchor, doc.cursor)];
                    v.extend(doc.extra.iter().copied());
                    doc.set_all_sels(v);
                }
                doc.rect = None;
                drag = DragState { mode: DragMode::AddSel, anchor: pos, press: Some(p) };
            } else if mods.shift {
                doc.extra.clear();
                doc.rect = None;
                doc.cursor = pos;
                drag = DragState { mode: DragMode::Select, anchor: doc.anchor, press: Some(p) };
            } else if doc.has_sel() && !doc.is_multi() && doc.sel().contains(&pos) && !doc.read_only {
                drag = DragState { mode: DragMode::MaybeMove, anchor: pos, press: Some(p) };
            } else {
                doc.set_sel(pos, pos);
                drag = DragState { mode: DragMode::Select, anchor: pos, press: Some(p) };
            }
            doc.view.preferred_x = None;
            doc.view.row_end_affinity = false;
            activity = true;
        } else if resp.dragged_by(egui::PointerButton::Primary) && drag.mode != DragMode::None {
            // Auto-scroll while selecting beyond the edges.
            if p.y < text_rect.top() {
                scroll_rows(ui, doc, &lay, -1);
                ui.ctx().request_repaint();
            } else if p.y > text_rect.bottom() {
                scroll_rows(ui, doc, &lay, 1);
                ui.ctx().request_repaint();
            }
            let visible = layout_visible(ui, doc, &lay, text_rect);
            let pos = pos_at(doc, &visible, text_rect, p);
            match drag.mode {
                DragMode::Select | DragMode::AddSel => {
                    doc.anchor = drag.anchor;
                    doc.cursor = pos;
                }
                DragMode::Lines(start) => {
                    if let Some(line) = line_at(&visible, doc, p.y) {
                        select_lines(doc, start, line);
                    }
                }
                DragMode::Rect => {
                    let line = doc.line_of(pos);
                    let x = p.x - (text_rect.left() - doc.view.x_offset);
                    if let Some(mut r) = doc.rect {
                        r.caret_line = line;
                        r.caret_x = x.max(0.0);
                        apply_rect(ui, doc, &lay, r);
                    }
                }
                DragMode::MaybeMove
                    if drag.press.is_some_and(|q| q.distance(p) > 4.0) => {
                        drag.mode = DragMode::MoveText;
                    }
                _ => {}
            }
            activity = true;
        }
        if released {
            match drag.mode {
                DragMode::MaybeMove => {
                    let pos = pos_at(doc, &visible, text_rect, p);
                    doc.set_sel(pos, pos);
                }
                DragMode::MoveText if text_rect.contains(p) => {
                    let pos = pos_at(doc, &visible, text_rect, p);
                    move_text(doc, pos, mods.ctrl);
                }
                _ => {}
            }
            drag = DragState::default();
        }
        if resp.double_clicked() && text_rect.contains(p) && !mods.ctrl {
            let pos = pos_at(doc, &visible, text_rect, p);
            let line = doc.line_of(pos);
            let ls = doc.line_start(line);
            let url = if lay.links {
                urls_in(doc.line_text(line)).into_iter().find(|u| u.start + ls <= pos && pos < u.end + ls).map(|u| doc.line_text(line)[u].to_string())
            } else {
                None
            };
            if let Some(u) = url {
                cfg.cmds.push(Cmd::OpenUrl(u));
            } else if let Some(w) = word_at(doc.text(), pos, &extra_word) {
                doc.set_sel(w.start, w.end);
            }
        }
        if resp.triple_clicked() && text_rect.contains(p) {
            let pos = pos_at(doc, &visible, text_rect, p);
            let line = doc.line_of(pos);
            select_lines(doc, line, line);
        }
    }
    if !ui.input(|i| i.pointer.primary_down()) && !released {
        drag = DragState::default();
    }
    ui.data_mut(|d| d.insert_temp(id.with("drag"), drag));

    // Wheel
    if resp.hovered() || resp.contains_pointer() {
        let (delta, ctrl) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.ctrl));
        if !ctrl {
            if delta.y != 0.0 {
                let acc = ui.data_mut(|d| {
                    let a = d.get_temp_mut_or_default::<f32>(id.with("acc"));
                    *a += -delta.y / row_h;
                    let whole = a.trunc();
                    *a -= whole;
                    whole as isize
                });
                if acc != 0 {
                    scroll_rows(ui, doc, &lay, acc);
                    *cfg.completion = None;
                }
            }
            if delta.x != 0.0 && !wrap {
                doc.view.x_offset = (doc.view.x_offset - delta.x).max(0.0);
            }
        }
    }

    // ---- keyboard ------------------------------------------------------------------------------
    if focused {
        let events = ui.input(|i| i.events.clone());
        for ev in events {
            if cfg.completion.as_ref().is_some_and(|c| c.doc_id != doc.id) {
                *cfg.completion = None;
            }
            if cfg.calltip.as_ref().is_some_and(|c| c.doc_id != doc.id) {
                *cfg.calltip = None;
            }
            match ev {
                Event::Text(t) => {
                    type_text(doc, &t, *cfg.overwrite, s);
                    activity = true;
                    let open = cfg.completion.is_some();
                    update_completion(doc, open, cfg.completion, s, cfg.keywords, &extra_word, true);
                    let start = cfg.api.map_or('(', |a| a.start);
                    if s.calltip_on_input && t.ends_with(start) && !doc.is_multi() {
                        *cfg.completion = None;
                        open_calltip(doc, cfg.api, cfg.calltip);
                    }
                }
                Event::Ime(egui::ImeEvent::Commit(t)) if !t.is_empty() => {
                    type_text(doc, &t, false, s);
                    activity = true;
                }
                Event::Paste(t) => {
                    paste(doc, &t);
                    activity = true;
                }
                Event::Copy => *cfg.copied = Some(copy(ui, doc, false, s.copy_line_without_selection)),
                Event::Cut => {
                    *cfg.copied = Some(copy(ui, doc, true, s.copy_line_without_selection));
                    activity = true;
                }
                Event::Key { key, pressed: true, modifiers, .. } => {
                    if cfg.completion.is_some() && handle_completion_key(doc, cfg.completion, key, modifiers) {
                        activity = true;
                        continue;
                    }
                    if key == Key::Escape && cfg.calltip.is_some() {
                        *cfg.calltip = None;
                        continue;
                    }
                    if key == Key::Space && modifiers.ctrl && modifiers.shift && !modifiers.alt {
                        // Ctrl+Shift+Space: function parameters hint
                        open_calltip(doc, cfg.api, cfg.calltip);
                        continue;
                    }
                    if key == Key::Insert && modifiers.is_none() {
                        *cfg.overwrite = !*cfg.overwrite;
                    } else if key == Key::Space && modifiers.ctrl && !modifiers.shift {
                        // Ctrl+Space: function/word completion; Ctrl+Alt+Space: path completion
                        if modifiers.alt {
                            *cfg.completion = path_completion(doc);
                        } else {
                            update_completion(doc, false, cfg.completion, s, cfg.keywords, &extra_word, false);
                        }
                    } else if key == Key::Enter && modifiers.ctrl && !modifiers.alt && !modifiers.shift {
                        // Ctrl+Enter: word completion
                        update_completion(doc, false, cfg.completion, s, &[], &extra_word, false);
                    } else if handle_key(ui, doc, &lay, s, &cfg.tabs, key, modifiers, vis_rows, &extra_word) {
                        activity = true;
                        if key == Key::Backspace && cfg.completion.is_some() {
                            update_completion(doc, true, cfg.completion, s, cfg.keywords, &extra_word, true);
                        } else {
                            *cfg.completion = None;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ---- scrolling & highlighting --------------------------------------------------------------
    if let Some(mode) = doc.view.scroll.take() {
        let l = doc.line_of(doc.cursor);
        doc.ensure_line_visible(l);
        scroll_into_view(ui, doc, &lay, vis_rows, text_rect.width(), mode);
    }
    clamp_scroll(doc, s.scroll_beyond_last_line, vis_rows);
    let upto = doc.view.first_line + vis_rows * 4 + 2;
    let deadline = Instant::now() + Duration::from_millis(12);
    let done = if let Some(udl) = cfg.udl {
        let (cache, text, starts) = doc.hl_parts();
        cache.ensure_udl(udl, text, starts, upto, deadline)
    } else if let Some(syn) = cfg.syntax {
        let (cache, text, starts) = doc.hl_parts();
        cache.ensure(cfg.hl, text, starts, syn, s.theme(), upto, deadline)
    } else {
        true
    };
    if !done {
        ui.ctx().request_repaint();
    }
    let visible = layout_visible(ui, doc, &lay, text_rect);

    // ---- painting ------------------------------------------------------------------------------
    paint(ui, doc, &cfg, &lay, &geo, &visible, rect, focused, activity, id, drag);
    scrollbars(ui, doc, rect, text_rect, sb_w, vis_rows, pal, wrap, char_w, cfg.view);

    // Parameter hint (closed when the caret leaves the call)
    if let Some(tip) = cfg.calltip.as_mut() {
        match crate::api::param_index(doc.text(), tip.open, doc.cursor, tip) {
            Some(param) if !doc.is_multi() => calltip_popup(ui, doc, &visible, text_rect, tip, param),
            _ => *cfg.calltip = None,
        }
    }

    // Completion popup
    if let Some(c) = cfg.completion.as_mut() {
        if let Some(sel) = completion_popup(ui, doc, &visible, text_rect, c, row_h) {
            let c = c.clone();
            accept_completion(doc, &c, sel);
            *cfg.completion = None;
        }
    }

    resp.context_menu(|ui| context_menu(ui, doc, cfg.cmds, &s.context_menu_extra));

    Output { focused, clicked: resp.clicked() || resp.drag_started() }
}

#[allow(clippy::too_many_arguments)]
fn paint(
    ui: &Ui,
    doc: &mut Document,
    cfg: &Cfg<'_>,
    lay: &Lay,
    geo: &Geo,
    visible: &[VisLine],
    rect: Rect,
    focused: bool,
    activity: bool,
    id: Id,
    drag: DragState,
) {
    let _ = lay;
    let s = cfg.s;
    let pal = cfg.pal;
    let (text_rect, row_h, char_w) = (geo.text_rect, geo.row_h, geo.char_w);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal.bg);
    let tp = ui.painter_at(text_rect.expand2(vec2(4.0, 0.0)));
    let x0 = text_rect.left() - doc.view.x_offset;
    let cur_line = doc.line_of(doc.cursor);
    let sels: Vec<Range<usize>> = doc.all_sels().iter().map(|&(a, c)| a.min(c)..a.max(c)).collect();
    let extra_word = &s.word_chars;
    let large = cfg.large;

    let mut under: Vec<Shape> = Vec::new();
    let line_rect = |vl: &VisLine| {
        Rect::from_min_max(pos2(text_rect.left() - 4.0, vl.top), pos2(text_rect.right(), vl.top + vl.galley.size().y.max(row_h)))
    };
    // Smart highlighting: all occurrences of the selected word.
    let sel0 = doc.sel();
    let smart_word: Option<String> = (s.smart_highlight && !large && doc.has_sel() && !doc.is_multi())
        .then(|| doc.selected_text())
        .filter(|w| !w.contains('\n') && w.chars().count() < 200)
        .filter(|_| !s.smart_whole_word || word_at(doc.text(), sel0.start, extra_word) == Some(sel0.clone()))
        .map(str::to_owned);
    let brace = if s.brace_match && !doc.has_sel() && !large { textops::matching_bracket(doc.text(), doc.cursor) } else { None };
    let tag_pair = if s.tag_match && !large && crate::fold::mode_for(doc.syntax.as_deref(), false) == crate::fold::Mode::Tags {
        matching_tags(doc.text(), doc.cursor)
    } else {
        None
    };

    for vl in visible {
        let ls = doc.line_start(vl.line);
        let ltext = doc.line_text(vl.line);
        let le = ls + ltext.len();
        let paint_range = |a: usize, b: usize, color: Color32, under: &mut Vec<Shape>| {
            for r in range_rects(&vl.galley, ltext, a.saturating_sub(ls), b.saturating_sub(ls).min(ltext.len())) {
                under.push(Shape::rect_filled(r.translate(vec2(x0, vl.top)), 1.0, color));
            }
        };
        if let Some(&k) = doc.diff_marks.get(&vl.line) {
            let c = match k {
                1 => Color32::from_rgba_unmultiplied(60, 200, 60, 60),
                2 => Color32::from_rgba_unmultiplied(230, 60, 60, 60),
                _ => Color32::from_rgba_unmultiplied(230, 200, 40, 70),
            };
            under.push(Shape::rect_filled(line_rect(vl), 0.0, c));
        }
        if vl.line == cur_line {
            match s.current_line {
                CurrentLine::Highlight => under.push(Shape::rect_filled(line_rect(vl), 0.0, pal.line_hl)),
                CurrentLine::Frame => under.push(Shape::rect_stroke(line_rect(vl).shrink(0.5), 0.0, Stroke::new(1.0_f32, pal.whitespace))),
                CurrentLine::None => {}
            }
        }
        if s.edge_mode == EdgeMode::Background && ltext.chars().count() > s.edge_column {
            let x = x0 + s.edge_column as f32 * char_w;
            under.push(Shape::rect_filled(
                Rect::from_min_max(pos2(x, vl.top), pos2(x0 + vl.galley.size().x, vl.top + vl.galley.size().y)),
                0.0,
                pal.edge,
            ));
        }
        for (m, st) in doc.marks.iter().filter(|(m, _)| m.start <= le && m.end >= ls) {
            paint_range(m.start.max(ls), m.end, pal.marks[(*st as usize).min(5)], &mut under);
        }
        if let Some(w) = &smart_word {
            let (hay, needle) = if s.smart_match_case { (ltext.to_string(), w.clone()) } else { (ltext.to_lowercase(), w.to_lowercase()) };
            if hay.len() == ltext.len() {
                for (i, _) in hay.match_indices(needle.as_str()) {
                    let (a, b) = (ls + i, ls + i + w.len());
                    if (a, b) != (sel0.start, sel0.end) && (!s.smart_whole_word || word_at(doc.text(), a, extra_word) == Some(a..b)) {
                        paint_range(a, b, pal.smart, &mut under);
                    }
                }
            }
        }
        if let Some((p, q)) = brace {
            for x in [p, q] {
                if x >= ls && x < le {
                    paint_range(x, x + 1, pal.brace, &mut under);
                }
            }
        }
        if let Some((a, b)) = &tag_pair {
            for r in [a, b] {
                if r.start >= ls && r.start <= le {
                    paint_range(r.start, r.end, pal.brace, &mut under);
                }
            }
        }
        for sel in &sels {
            if sel.start <= le && sel.end >= ls && !sel.is_empty() {
                paint_range(sel.start.max(ls), sel.end.min(le), pal.selection, &mut under);
                if sel.end > le && sel.start <= le {
                    // Selected line break
                    let end = vl.galley.rows.last().map_or(0.0, |r| r.rect.right());
                    let y = vl.galley.rows.last().map_or(0.0, |r| r.rect.top());
                    let r = Rect::from_min_size(pos2(x0 + end, vl.top + y), vec2(char_w * 0.6, row_h));
                    under.push(Shape::rect_filled(r, 0.0, pal.selection));
                }
            }
        }
    }
    tp.extend(under);

    // Indent guides
    if s.show_indent_guides && !large {
        let tw = cfg.tabs.width.max(1);
        let indent_cols = |l: &str| -> Option<usize> {
            if l.trim().is_empty() {
                return None;
            }
            let mut c = 0;
            for ch in l.chars() {
                match ch {
                    ' ' => c += 1,
                    '\t' => c = (c / tw + 1) * tw,
                    _ => break,
                }
            }
            Some(c)
        };
        for vl in visible {
            let own = indent_cols(doc.line_text(vl.line));
            let cols = own.unwrap_or_else(|| {
                // Blank line: continue the guides of the surrounding block.
                let next = (vl.line + 1..(vl.line + 100).min(doc.line_count())).find_map(|l| indent_cols(doc.line_text(l)));
                let prev = (vl.line.saturating_sub(100)..vl.line).rev().find_map(|l| indent_cols(doc.line_text(l)));
                next.unwrap_or(0).min(prev.unwrap_or(0)) + 1
            });
            let mut c = tw;
            while c < cols {
                let x = x0 + (c - tw) as f32 * char_w + 1.0;
                let mut y = vl.top;
                while y < vl.top + row_h {
                    tp.line_segment([pos2(x, y), pos2(x, (y + 1.0).min(vl.top + row_h))], Stroke::new(1.0_f32, pal.guide));
                    y += 2.0;
                }
                c += tw;
            }
        }
    }

    for vl in visible {
        tp.galley(pos2(x0, vl.top), vl.galley.clone(), pal.fg);
        doc.view.max_width = doc.view.max_width.max(vl.galley.size().x);
        if s.show_whitespace || s.show_eol || s.show_wrap_symbol {
            paint_whitespace(&tp, doc, vl, x0, char_w, row_h, pal.whitespace, s);
        }
        if s.show_ctrl_chars || s.show_non_printing {
            // Frame control characters (drawn as ␀ … symbols) like Scintilla.
            let line = doc.line_text(vl.line);
            if line.chars().any(|c| c != '\t' && c != ' ' && highlight::display_char(c, s.show_ctrl_chars, s.show_non_printing).is_some()) {
                let chars: Vec<char> = line.chars().collect();
                let mut idx = 0;
                for row in &vl.galley.rows {
                    for (gi, g) in row.glyphs.iter().enumerate() {
                        let c = chars.get(idx + gi).copied().unwrap_or(' ');
                        if c != '\t' && c != ' ' && highlight::display_char(c, s.show_ctrl_chars, s.show_non_printing).is_some() {
                            let r = Rect::from_min_size(pos2(x0 + g.pos.x, vl.top + row.rect.top() + 1.0), vec2(g.advance_width, row.rect.height() - 2.0));
                            tp.rect_stroke(r, 2.0, Stroke::new(1.0_f32, pal.whitespace));
                        }
                    }
                    idx += row.char_count_including_newline();
                }
            }
        }
        // Folded block: line under the header
        if doc.folded.contains(&vl.line) {
            let y = vl.top + vl.galley.size().y.max(row_h) - 0.5;
            tp.line_segment([pos2(text_rect.left() - 4.0, y), pos2(text_rect.right(), y)], Stroke::new(1.0_f32, pal.fold));
        }
        let hidden_after = doc.hidden_ranges.iter().any(|r| r.start == vl.line + 1);
        let hidden_before = doc.hidden_ranges.iter().any(|r| r.end == vl.line);
        if hidden_after || hidden_before {
            let y = if hidden_after { vl.top + vl.galley.size().y.max(row_h) - 0.5 } else { vl.top + 0.5 };
            tp.line_segment([pos2(text_rect.left() - 4.0, y), pos2(text_rect.right(), y)], Stroke::new(1.0_f32, Color32::from_rgb(220, 120, 0)));
        }
    }

    // Edge line
    if s.edge_mode == EdgeMode::Line {
        let x = x0 + s.edge_column as f32 * char_w;
        if x > text_rect.left() {
            tp.line_segment([pos2(x, text_rect.top()), pos2(x, text_rect.bottom())], Stroke::new(1.0_f32, pal.edge));
        }
    }

    // Carets
    let t = ui.input(|i| i.time);
    if activity {
        ui.data_mut(|d| d.insert_temp(id.with("blink"), t));
    }
    let since = t - ui.data(|d| d.get_temp::<f64>(id.with("blink")).unwrap_or(0.0));
    let on = !s.caret_blink || since % 1.0 < 0.55;
    let mut caret_rects = Vec::new();
    for &(_, c) in doc.all_sels().iter() {
        let line = doc.line_of(c);
        let Some(vl) = visible.iter().find(|vl| vl.line == line) else { continue };
        let ltext = doc.line_text(line);
        let ci = char_index(ltext, c - doc.line_start(line));
        let pos = vl.galley.pos_from_ccursor(ccursor(doc, ci));
        caret_rects.push(Rect::from_min_size(pos2(x0 + pos.min.x, vl.top + pos.min.y), vec2(s.caret_width.max(1.0), pos.height().max(row_h))));
    }
    if focused {
        if on {
            for (i, caret) in caret_rects.iter().enumerate() {
                let color = if i == 0 { pal.caret } else { pal.caret.gamma_multiply(0.7) };
                if *cfg.overwrite {
                    let r = Rect::from_min_size(pos2(caret.left(), caret.bottom() - 2.0), vec2(char_w, 2.0));
                    tp.rect_filled(r, 0.0, color);
                } else {
                    tp.rect_filled(*caret, 0.0, color);
                }
            }
        }
        if s.caret_blink {
            let wait = if on { 0.55 - since % 1.0 } else { 1.0 - since % 1.0 };
            ui.ctx().request_repaint_after(Duration::from_secs_f64(wait.max(0.01)));
        }
        if let Some(c) = caret_rects.first() {
            ui.ctx().output_mut(|o| o.ime = Some(egui::output::IMEOutput { rect: text_rect, cursor_rect: *c }));
        }
    }
    // Drop position while dragging text
    if drag.mode == DragMode::MoveText {
        if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
            let pos = pos_at(doc, visible, text_rect, p);
            let line = doc.line_of(pos);
            if let Some(vl) = visible.iter().find(|vl| vl.line == line) {
                let ci = char_index(doc.line_text(line), pos - doc.line_start(line));
                let r = vl.galley.pos_from_ccursor(CCursor::new(ci));
                tp.rect_filled(Rect::from_min_size(pos2(x0 + r.min.x, vl.top + r.min.y), vec2(2.0, row_h)), 0.0, pal.caret.gamma_multiply(0.6));
            }
        }
    }

    // Gutter
    let gutter = geo.gutter;
    painter.rect_filled(gutter, 0.0, pal.gutter_bg);
    let gp = ui.painter_at(gutter);
    let num_font = FontId::monospace(s.font_size * 0.9);
    // Fold markers of the visible lines: (is header, Some(is block end) when inside a block)
    let fold_info: Option<Vec<(bool, Option<bool>)>> = geo.fold_rect.map(|_| {
        let ends = doc.fold_ends();
        let get = |l: usize| ends.get(l).copied().unwrap_or(u32::MAX);
        visible
            .iter()
            .map(|vl| {
                let e = get(vl.line);
                let header = e != u32::MAX && e as usize > vl.line;
                let inside = if header {
                    None
                } else {
                    (vl.line.saturating_sub(5000)..vl.line).rev().find(|&hl| get(hl) != u32::MAX && get(hl) as usize >= vl.line).map(|hl| get(hl) as usize == vl.line)
                };
                (header, inside)
            })
            .collect()
    });
    for (vi, vl) in visible.iter().enumerate() {
        let mid = vl.top + row_h / 2.0;
        let h = vl.galley.size().y.max(row_h);
        if doc.bookmarks.contains(&vl.line) && s.bookmark_margin {
            let r = (row_h * 0.32).max(3.0);
            gp.circle(pos2(geo.mark_x + (row_h * 0.9).round() / 2.0 + 1.0, mid), r, pal.bookmark, Stroke::new(1.0_f32, pal.bookmark.gamma_multiply(0.6)));
        }
        if let Some((_, right)) = geo.num_rect {
            let color = if vl.line == cur_line { pal.gutter_cur } else { pal.gutter_fg };
            gp.text(pos2(right - 2.0, mid), Align2::RIGHT_CENTER, (vl.line + 1).to_string(), num_font.clone(), color);
        }
        if let Some(cx) = geo.change_x {
            let c = match doc.line_change(vl.line) {
                CHANGE_MODIFIED => Some(pal.change_modified),
                CHANGE_SAVED => Some(pal.change_saved),
                _ => None,
            };
            if let Some(c) = c {
                gp.rect_filled(Rect::from_min_size(pos2(cx, vl.top), vec2(3.0, h)), 0.0, c);
            }
        }
        if let (Some((a, b)), Some(info)) = (geo.fold_rect, &fold_info) {
            let cx = (a + b) / 2.0;
            let box_s = ((b - a) * 0.62).round();
            let (header, inside) = info[vi];
            if header {
                let r = Rect::from_center_size(pos2(cx, mid), vec2(box_s, box_s));
                gp.rect_stroke(r, 1.0, Stroke::new(1.0_f32, pal.fold));
                gp.line_segment([pos2(r.left() + 2.0, mid), pos2(r.right() - 2.0, mid)], Stroke::new(1.0_f32, pal.fold));
                if doc.folded.contains(&vl.line) {
                    gp.line_segment([pos2(cx, r.top() + 2.0), pos2(cx, r.bottom() - 2.0)], Stroke::new(1.0_f32, pal.fold));
                } else {
                    gp.line_segment([pos2(cx, r.bottom()), pos2(cx, vl.top + h)], Stroke::new(1.0_f32, pal.fold));
                }
            } else {
                // Inside a block: vertical line; at its end a small corner.
                if let Some(end) = inside {
                    let y2 = if end { mid } else { vl.top + h };
                    gp.line_segment([pos2(cx, vl.top), pos2(cx, y2)], Stroke::new(1.0_f32, pal.fold));
                    if end {
                        gp.line_segment([pos2(cx, mid), pos2(cx + box_s / 2.0, mid)], Stroke::new(1.0_f32, pal.fold));
                    }
                }
            }
        }
        if doc.hidden_ranges.iter().any(|r| r.start == vl.line + 1 || r.end == vl.line) {
            let cx = geo.fold_rect.map_or(gutter.right() - 6.0, |(a, b)| (a + b) / 2.0);
            gp.text(pos2(cx, mid), Align2::CENTER_CENTER, "↕", FontId::proportional(row_h * 0.7), Color32::from_rgb(220, 120, 0));
        }
    }
}

fn select_lines(doc: &mut Document, a: usize, b: usize) {
    let (l1, l2) = (a.min(b), a.max(b));
    let start = doc.line_start(l1);
    let end = doc.line_range_full(l2).end;
    if b >= a {
        doc.set_sel(start, end);
    } else {
        doc.set_sel(end, start);
    }
}

/// Converts a rectangular selection into one selection per line.
fn apply_rect(ui: &Ui, doc: &mut Document, lay: &Lay, r: RectSel) {
    let (l1, l2) = (r.anchor_line.min(r.caret_line), r.anchor_line.max(r.caret_line));
    let (xa, xc) = (r.anchor_x, r.caret_x);
    let mut sels = Vec::new();
    let mut line = l1;
    loop {
        if !doc.is_line_hidden(line) {
            let g = lay.galley(ui, doc, line);
            let y = g.rows.first().map_or(0.0, |row| row.rect.center().y);
            let ltext = doc.line_text(line);
            let ls = doc.line_start(line);
            let at = |x: f32| ls + byte_index(ltext, g.cursor_from_pos(vec2(x, y)).ccursor.index);
            let sel = (at(xa), at(xc));
            if line == r.caret_line {
                sels.insert(0, sel);
            } else {
                sels.push(sel);
            }
        }
        if line >= l2 {
            break;
        }
        line += 1;
    }
    doc.set_all_sels(sels);
    doc.rect = Some(r);
}

fn layout_visible(ui: &Ui, doc: &mut Document, lay: &Lay, text_rect: Rect) -> Vec<VisLine> {
    let mut out = Vec::new();
    let mut line = doc.visible_from(doc.view.first_line);
    let first = lay.galley(ui, doc, line);
    let row = doc.view.first_row.min(first.rows.len().saturating_sub(1));
    doc.view.first_row = row;
    let mut top = text_rect.top() - first.rows.get(row).map_or(0.0, |r| r.rect.top());
    let mut g = first;
    loop {
        let h = g.size().y.max(1.0);
        out.push(VisLine { line, galley: g, top });
        top += h;
        if top > text_rect.bottom() {
            break;
        }
        match doc.next_visible(line) {
            Some(n) => line = n,
            None => break,
        }
        g = lay.galley(ui, doc, line);
    }
    out
}

fn line_at(visible: &[VisLine], doc: &Document, y: f32) -> Option<usize> {
    let vl = visible.iter().find(|vl| y < vl.top + vl.galley.size().y.max(1.0)).or(visible.last())?;
    Some(vl.line.min(doc.line_count() - 1))
}

/// Byte position under a screen point.
fn pos_at(doc: &Document, visible: &[VisLine], text_rect: Rect, p: Pos2) -> usize {
    let Some(first) = visible.first() else { return 0 };
    let last = visible.last().unwrap();
    let vl = if p.y < first.top {
        first
    } else {
        visible.iter().find(|vl| p.y < vl.top + vl.galley.size().y.max(1.0)).unwrap_or(last)
    };
    if p.y >= last.top + last.galley.size().y && last.line + 1 >= doc.line_count() {
        return doc.text().len();
    }
    let local = vec2(p.x - (text_rect.left() - doc.view.x_offset), p.y - vl.top);
    let c = vl.galley.cursor_from_pos(local);
    let ltext = doc.line_text(vl.line);
    doc.line_start(vl.line) + byte_index(ltext, c.ccursor.index)
}

/// Rectangles (galley coordinates) covering bytes `a..b` of the line.
fn range_rects(galley: &Galley, line: &str, a: usize, b: usize) -> Vec<Rect> {
    let (ca, cb) = (char_index(line, a), char_index(line, b));
    let mut out = Vec::new();
    let mut start = 0;
    for row in &galley.rows {
        let n = row.char_count_including_newline();
        let (rs, re) = (start, start + n);
        start = re;
        let (x, y) = (ca.max(rs), cb.min(re));
        if x < y || (ca == cb && ca >= rs && ca < re) {
            let x1 = row.x_offset(x - rs);
            let x2 = if y > x { row.x_offset(y - rs) } else { x1 };
            out.push(Rect::from_min_max(pos2(x1, row.rect.top()), pos2(x2.max(x1 + 1.0), row.rect.bottom())));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn paint_whitespace(p: &egui::Painter, doc: &Document, vl: &VisLine, x0: f32, char_w: f32, row_h: f32, color: Color32, s: &Settings) {
    let chars: Vec<char> = doc.line_text(vl.line).chars().collect();
    let mut idx = 0;
    let nrows = vl.galley.rows.len();
    for (ri, row) in vl.galley.rows.iter().enumerate() {
        if s.show_whitespace {
            for (gi, g) in row.glyphs.iter().enumerate() {
                let c = chars.get(idx + gi).copied().unwrap_or(' ');
                let y = vl.top + row.rect.center().y;
                if c == ' ' {
                    let x = x0 + g.pos.x + g.advance_width / 2.0;
                    p.circle_filled(pos2(x, y), (char_w * 0.09).max(1.0), color);
                } else if c == '\t' {
                    let x1 = x0 + g.pos.x + 1.0;
                    let x2 = row.glyphs.get(gi + 1).map_or(x1 + char_w * s.tab_width as f32, |n| x0 + n.pos.x) - 2.0;
                    p.line_segment([pos2(x1, y), pos2(x2, y)], Stroke::new(1.0_f32, color));
                    p.line_segment([pos2(x2 - 3.0, y - 3.0), pos2(x2, y)], Stroke::new(1.0_f32, color));
                    p.line_segment([pos2(x2 - 3.0, y + 3.0), pos2(x2, y)], Stroke::new(1.0_f32, color));
                }
            }
        }
        if s.show_wrap_symbol && ri + 1 < nrows {
            p.text(pos2(x0 + row.rect.right() + 2.0, vl.top + row.rect.center().y), Align2::LEFT_CENTER, "↵", FontId::monospace(row_h * 0.6), color);
        }
        idx += row.char_count_including_newline();
    }
    if s.show_eol && vl.line + 1 < doc.line_count() {
        if let Some(row) = vl.galley.rows.last() {
            let x = x0 + row.rect.right() + 2.0;
            let label = match doc.line_eol(vl.line) {
                crate::doc::Eol::Crlf => "CRLF",
                crate::doc::Eol::Lf => "LF",
                crate::doc::Eol::Cr => "CR",
            };
            let font = FontId::monospace((row_h * 0.55).max(7.0));
            let g = p.layout_no_wrap(label.into(), font, Color32::WHITE);
            let r = Align2::LEFT_CENTER.anchor_size(pos2(x + 2.0, vl.top + row.rect.center().y), g.size());
            p.rect_filled(r.expand(1.0), 2.0, color);
            p.galley(r.min, g, Color32::WHITE);
        }
    }
}

/// Positions of the start tag and end tag names around the caret (HTML/XML).
pub fn matching_tags(text: &str, pos: usize) -> Option<(Range<usize>, Range<usize>)> {
    let lt = text[..pos.min(text.len())].rfind('<')?;
    if text[lt..pos].contains('>') && !text[lt..pos].ends_with('>') {
        return None;
    }
    let close = text[lt..].starts_with("</");
    let ns = lt + if close { 2 } else { 1 };
    let name_len = text[ns..].find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))?;
    if name_len == 0 {
        return None;
    }
    let name = &text[ns..ns + name_len];
    let gt = lt + text[lt..].find('>')?;
    if text.as_bytes()[gt - 1] == b'/' {
        return None;
    }
    let this = ns..ns + name_len;
    let re = regex::Regex::new(&format!(r"<(/?){}\b[^>]*?(/?)>", regex::escape(name))).ok()?;
    if !close {
        let mut depth = 0;
        for c in re.captures_iter(&text[gt + 1..]) {
            let m = c.get(0).unwrap();
            if &c[2] == "/" {
                continue;
            }
            if &c[1] == "/" {
                if depth == 0 {
                    let s = gt + 1 + m.start() + 2;
                    return Some((this, s..s + name_len));
                }
                depth -= 1;
            } else {
                depth += 1;
            }
        }
        None
    } else {
        let mut stack: Vec<usize> = Vec::new();
        for c in re.captures_iter(&text[..lt]) {
            let m = c.get(0).unwrap();
            if &c[2] == "/" {
                continue;
            }
            if &c[1] == "/" {
                stack.pop();
            } else {
                stack.push(m.start() + 1);
            }
        }
        let s = stack.pop()?;
        Some((s..s + name_len, this))
    }
}

/// Typed text: replaces every selection; handles overwrite mode and auto-close.
fn type_text(doc: &mut Document, t: &str, overwrite: bool, s: &Settings) {
    if t.chars().any(|c| c.is_control() && c != '\t') || doc.read_only {
        return;
    }
    if doc.is_multi() {
        let edits = doc.all_sels().iter().map(|&(a, c)| (a.min(c)..a.max(c), t.to_string())).collect();
        doc.replace_multi(edits, false);
        return;
    }
    let mut r = doc.sel();
    let next = doc.text()[r.end..].chars().next();
    let single = t.chars().count() == 1;
    let ch = t.chars().next().unwrap_or(' ');
    // Typing the closing char of an auto-inserted pair steps over it.
    let closers = [(')', s.auto_close_parens), (']', s.auto_close_brackets), ('}', s.auto_close_braces), ('"', s.auto_close_dquote), ('\'', s.auto_close_squote)];
    if single && r.is_empty() && next == Some(ch) && closers.iter().any(|&(c, on)| on && c == ch) {
        let p = doc.next_char(r.end);
        doc.set_sel(p, p);
        return;
    }
    if overwrite && r.is_empty() {
        let line_end = doc.line_range(doc.line_of(r.start)).end;
        let mut end = r.start;
        for _ in t.chars() {
            if end < line_end {
                end = doc.next_char(end);
            }
        }
        r.end = end;
    }
    let pair = match ch {
        '(' if s.auto_close_parens => Some(')'),
        '[' if s.auto_close_brackets => Some(']'),
        '{' if s.auto_close_braces => Some('}'),
        '"' if s.auto_close_dquote => Some('"'),
        '\'' if s.auto_close_squote => Some('\''),
        _ => None,
    };
    let word_before = doc.text()[..r.start].chars().next_back().is_some_and(|c| c.is_alphanumeric());
    if single && !overwrite {
        if let Some(close) = pair.filter(|_| !(matches!(ch, '"' | '\'') && word_before)) {
            if !r.is_empty() {
                // Wrap the selection.
                let inner = doc.text()[r.clone()].to_string();
                doc.replace(r.clone(), &format!("{ch}{inner}{close}"), EditKind::Other);
                doc.set_sel(r.start + 1, r.start + 1 + inner.len());
            } else {
                doc.replace(r.clone(), &format!("{ch}{close}"), EditKind::Typing);
                let p = r.start + ch.len_utf8();
                doc.set_sel(p, p);
            }
            return;
        }
    }
    doc.replace(r.clone(), t, EditKind::Typing);
    // HTML/XML: close the tag after typing '>'
    if s.auto_close_tags && ch == '>' && single {
        let c = doc.cursor;
        let before = &doc.text()[..c];
        if let Some(lt) = before.rfind('<') {
            let tag = &before[lt + 1..c - 1];
            let name: String = tag.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')).collect();
            let void = ["br", "hr", "img", "input", "meta", "link", "area", "base", "col", "embed", "source", "track", "wbr", "param"];
            if !name.is_empty() && !tag.ends_with('/') && !tag.starts_with('/') && !void.contains(&name.to_lowercase().as_str()) {
                let close = format!("</{name}>");
                doc.replace(c..c, &close, EditKind::Other);
                doc.set_sel(c, c);
            }
        }
    }
}

fn paste(doc: &mut Document, t: &str) {
    let t = t.replace("\r\n", "\n").replace('\r', "\n");
    if doc.is_multi() {
        let sels = doc.all_sels();
        // One line per selection when the counts match (column paste).
        let lines: Vec<&str> = t.trim_end_matches('\n').split('\n').collect();
        let mut order: Vec<usize> = (0..sels.len()).collect();
        order.sort_by_key(|&i| sels[i].0.min(sels[i].1));
        let mut texts = vec![String::new(); sels.len()];
        for (k, &i) in order.iter().enumerate() {
            texts[i] = if lines.len() == sels.len() { lines[k].to_string() } else { t.clone() };
        }
        let edits = sels.iter().zip(texts).map(|(&(a, c), s)| (a.min(c)..a.max(c), s)).collect();
        doc.replace_multi(edits, false);
    } else {
        let r = doc.sel();
        doc.replace(r, &t, EditKind::Other);
    }
}

fn copy(ui: &Ui, doc: &mut Document, cut: bool, line_without_sel: bool) -> String {
    if doc.is_multi() {
        let mut sels = doc.all_sels();
        sels.sort_by_key(|&(a, c)| a.min(c));
        let text: Vec<&str> = sels.iter().map(|&(a, c)| &doc.text()[a.min(c)..a.max(c)]).collect();
        let joined = text.join("\n");
        ui.ctx().copy_text(joined.clone());
        if cut && !doc.read_only {
            let edits = doc.all_sels().iter().map(|&(a, c)| (a.min(c)..a.max(c), String::new())).collect();
            doc.replace_multi(edits, false);
        }
        return joined;
    }
    // Without selection the whole line is copied/cut, like Notepad++.
    let r = if doc.has_sel() || !line_without_sel { doc.sel() } else { doc.line_range_full(doc.line_of(doc.cursor)) };
    let mut text = doc.text()[r.clone()].to_string();
    if !doc.has_sel() && line_without_sel && !text.ends_with('\n') {
        text.push('\n');
    }
    ui.ctx().copy_text(text.clone());
    if cut && !doc.read_only && !r.is_empty() {
        doc.replace(r, "", EditKind::Other);
    }
    text
}

fn move_text(doc: &mut Document, pos: usize, copy: bool) {
    let sel = doc.sel();
    if sel.contains(&pos) || pos == sel.end {
        doc.set_sel(pos, pos);
        return;
    }
    let text = doc.text()[sel.clone()].to_string();
    let mut t = doc.text().to_string();
    let new_start = if copy {
        t.insert_str(pos, &text);
        pos
    } else if pos < sel.start {
        t.replace_range(sel.clone(), "");
        t.insert_str(pos, &text);
        pos
    } else {
        t.insert_str(pos, &text);
        t.replace_range(sel.clone(), "");
        pos - text.len()
    };
    doc.set_text(t, new_start..new_start + text.len());
}

/// Applies a caret movement to all selections.
fn move_all(doc: &mut Document, extend: bool, f: impl Fn(&Document, usize, usize) -> usize) {
    let sels: Vec<(usize, usize)> = doc
        .all_sels()
        .into_iter()
        .map(|(a, c)| {
            let n = f(doc, a, c);
            if extend {
                (a, n)
            } else {
                (n, n)
            }
        })
        .collect();
    doc.rect = None;
    doc.set_all_sels(sels);
    doc.view.row_end_affinity = false;
    doc.view.scroll = Some(ScrollTo::Cursor);
}

/// Edits every selection (`f` returns the replaced range and the new text).
fn edit_all(doc: &mut Document, kind: EditKind, f: impl Fn(&Document, usize, usize) -> Option<(Range<usize>, String)>) {
    let edits: Vec<(Range<usize>, String)> = doc.all_sels().into_iter().filter_map(|(a, c)| f(doc, a, c)).collect();
    if edits.len() == 1 && !doc.is_multi() {
        let (r, s) = edits.into_iter().next().unwrap();
        doc.replace(r, &s, kind);
    } else if !edits.is_empty() {
        doc.replace_multi(edits, false);
    }
}

/// Returns true if the key was handled.
#[allow(clippy::too_many_arguments)]
fn handle_key(ui: &Ui, doc: &mut Document, lay: &Lay, s: &Settings, tabs: &TabSetting, key: Key, m: Modifiers, vis_rows: usize, extra: &str) -> bool {
    let shift = m.shift;
    let ctrl = m.ctrl || m.command;
    let plain_or_shift = !ctrl && !m.alt;
    // Column selection with Alt+Shift+arrows
    if m.alt && shift && !ctrl && matches!(key, Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight) {
        let line = doc.line_of(doc.cursor);
        let g = lay.galley(ui, doc, line);
        let ci = char_index(doc.line_text(line), doc.cursor - doc.line_start(line));
        let x = g.pos_from_ccursor(CCursor::new(ci)).min.x;
        let mut r = doc.rect.unwrap_or(RectSel { anchor_line: line, anchor_x: x, caret_line: line, caret_x: x });
        match key {
            Key::ArrowUp => r.caret_line = doc.prev_visible(r.caret_line).unwrap_or(r.caret_line),
            Key::ArrowDown => r.caret_line = doc.next_visible(r.caret_line).unwrap_or(r.caret_line),
            Key::ArrowLeft => r.caret_x = (r.caret_x - lay.char_w).max(0.0),
            _ => r.caret_x += lay.char_w,
        }
        apply_rect(ui, doc, lay, r);
        doc.view.scroll = Some(ScrollTo::Cursor);
        return true;
    }
    match key {
        Key::ArrowLeft if !m.alt => {
            move_all(doc, shift, |d, a, c| {
                if a != c && !shift && !ctrl {
                    a.min(c)
                } else if ctrl {
                    word_left(d, c, extra)
                } else {
                    d.prev_char(c)
                }
            });
            doc.view.preferred_x = None;
        }
        Key::ArrowRight if !m.alt => {
            move_all(doc, shift, |d, a, c| {
                if a != c && !shift && !ctrl {
                    a.max(c)
                } else if ctrl {
                    word_right(d, c, extra)
                } else {
                    d.next_char(c)
                }
            });
            doc.view.preferred_x = None;
        }
        Key::ArrowUp | Key::ArrowDown if ctrl && !shift && !m.alt => {
            // Ctrl+Up/Down scrolls without moving the caret.
            scroll_rows(ui, doc, lay, if key == Key::ArrowUp { -1 } else { 1 });
        }
        Key::ArrowUp | Key::ArrowDown if plain_or_shift => {
            let d = if key == Key::ArrowUp { -1 } else { 1 };
            if doc.is_multi() {
                let sels = doc.all_sels();
                let moved: Vec<(usize, usize)> = sels
                    .iter()
                    .map(|&(a, c)| {
                        let n = vertical_target(ui, doc, lay, c, d, None).0;
                        if shift {
                            (a, n)
                        } else {
                            (n, n)
                        }
                    })
                    .collect();
                doc.rect = None;
                doc.set_all_sels(moved);
                doc.view.scroll = Some(ScrollTo::Cursor);
            } else {
                move_vertical(ui, doc, lay, d, shift);
            }
        }
        Key::PageUp | Key::PageDown if plain_or_shift => {
            let d = vis_rows.saturating_sub(1).max(1) as isize * if key == Key::PageUp { -1 } else { 1 };
            scroll_rows(ui, doc, lay, d);
            doc.extra.clear();
            move_vertical(ui, doc, lay, d, shift);
        }
        Key::Home if !m.alt => {
            move_all(doc, shift, |d, _a, c| {
                if ctrl {
                    0
                } else {
                    let line = d.line_of(c);
                    let r = d.line_range(line);
                    let t = &d.text()[r.clone()];
                    let first = r.start + t.len() - t.trim_start().len();
                    if c == first {
                        r.start
                    } else {
                        first
                    }
                }
            });
            doc.view.preferred_x = None;
        }
        Key::End if !m.alt => {
            move_all(doc, shift, |d, _a, c| if ctrl { d.text().len() } else { d.line_range(d.line_of(c)).end });
            doc.view.preferred_x = None;
        }
        Key::Backspace if !m.alt => {
            let w = tabs.width.max(1);
            let spaces = tabs.use_spaces;
            let kind = if doc.has_sel() || ctrl { EditKind::Other } else { EditKind::Backspace };
            edit_all(doc, kind, |d, a, c| {
                if a != c {
                    return Some((a.min(c)..a.max(c), String::new()));
                }
                if c == 0 {
                    return None;
                }
                if ctrl {
                    return Some((word_left(d, c, extra)..c, String::new()));
                }
                // In space indentation, delete back to the previous tab stop.
                let ls = d.line_start(d.line_of(c));
                let before = &d.text()[ls..c];
                let n = if spaces && !before.is_empty() && before.bytes().all(|b| b == b' ') {
                    let r = before.len() % w;
                    if r == 0 {
                        w
                    } else {
                        r
                    }
                } else {
                    c - d.prev_char(c)
                };
                Some((c - n..c, String::new()))
            });
        }
        Key::Delete if !m.alt && !shift => {
            let kind = if doc.has_sel() || ctrl { EditKind::Other } else { EditKind::Delete };
            edit_all(doc, kind, |d, a, c| {
                if a != c {
                    Some((a.min(c)..a.max(c), String::new()))
                } else if ctrl {
                    Some((c..word_right(d, c, extra), String::new()))
                } else {
                    let p = d.next_char(c);
                    (p > c).then(|| (c..p, String::new()))
                }
            });
        }
        Key::Enter if !ctrl && !m.alt => {
            if doc.read_only {
                return true;
            }
            let unit = indent_unit_for(tabs);
            if doc.is_multi() {
                let auto = s.auto_indent;
                edit_all(doc, EditKind::Other, |d, a, c| {
                    let r = a.min(c)..a.max(c);
                    let ls = d.line_start(d.line_of(r.start));
                    let ind: String = if auto { d.text()[ls..r.start].chars().take_while(|c| *c == ' ' || *c == '\t').collect() } else { String::new() };
                    Some((r, format!("\n{ind}")))
                });
                return true;
            }
            let mut ins = String::from("\n");
            if s.auto_indent {
                let ls = doc.line_start(doc.line_of(doc.sel().start));
                let before = &doc.text()[ls..doc.sel().start];
                let indent: String = before.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                ins.push_str(&indent);
                let after = &doc.text()[doc.sel().end..];
                let bt = before.trim_end();
                let opener = bt.ends_with('{') || bt.ends_with('[') || (bt.ends_with(':') && doc.syntax.as_deref().is_some_and(|s| s.contains("Python")));
                if opener && (doc.syntax.is_some() || doc.user_lang.is_some()) {
                    ins.push_str(&unit);
                    let after_t = after.trim_start_matches([' ', '\t']);
                    if after_t.starts_with('}') || after_t.starts_with(']') {
                        // Put the closing brace on its own line.
                        let close = format!("\n{indent}");
                        let r = doc.sel();
                        doc.replace(r, &format!("{ins}{close}"), EditKind::Other);
                        let c = doc.cursor - close.len();
                        doc.set_sel(c, c);
                        return true;
                    }
                }
            }
            let r = doc.sel();
            doc.replace(r, &ins, EditKind::Typing);
        }
        Key::Tab if !ctrl && !m.alt => {
            if doc.read_only {
                return true;
            }
            let sel = doc.sel();
            let multiline = doc.text()[sel.clone()].contains('\n');
            if doc.is_multi() {
                let unit = indent_unit_for(tabs);
                if !shift {
                    edit_all(doc, EditKind::Other, |_, a, c| Some((a.min(c)..a.max(c), unit.clone())));
                }
            } else if shift {
                if let Some((t, r)) = textops::unindent(doc.text(), &sel, tabs.width) {
                    let r = if sel.is_empty() { cursor_after_unindent(doc, &t) } else { r };
                    doc.set_text(t, r);
                }
            } else if multiline {
                if let Some((t, r)) = textops::indent(doc.text(), &sel, &indent_unit_for(tabs)) {
                    doc.set_text(t, r);
                }
            } else {
                let ins = if tabs.use_spaces {
                    let (_, col) = doc.line_col(sel.start);
                    let w = tabs.width.max(1);
                    " ".repeat(w - col % w)
                } else {
                    "\t".to_string()
                };
                doc.replace(sel, &ins, EditKind::Typing);
            }
        }
        Key::Escape => {
            if doc.is_multi() || doc.rect.is_some() {
                let (a, c) = (doc.anchor, doc.cursor);
                doc.set_sel(a, c);
            } else if doc.has_sel() {
                doc.anchor = doc.cursor;
            } else {
                return false;
            }
        }
        Key::A if ctrl && !shift && !m.alt => {
            doc.set_sel(0, doc.text().len());
        }
        Key::Z if ctrl && !m.alt => {
            if shift {
                doc.redo()
            } else {
                doc.undo()
            }
        }
        Key::Y if ctrl && !shift && !m.alt => doc.redo(),
        _ => return false,
    }
    true
}

fn cursor_after_unindent(doc: &Document, new_text: &str) -> Range<usize> {
    let line = doc.line_of(doc.cursor);
    let start = std::iter::once(0).chain(new_text.match_indices('\n').map(|(i, _)| i + 1)).nth(line).unwrap_or(new_text.len());
    let old_ls = doc.line_start(line);
    let removed = doc.line_text(line).len() - new_text[start..].split('\n').next().unwrap_or("").len();
    let c = (start + (doc.cursor - old_ls).saturating_sub(removed)).min(new_text.len());
    c..c
}

pub fn indent_unit_for(t: &TabSetting) -> String {
    if t.use_spaces {
        " ".repeat(t.width.max(1))
    } else {
        "\t".into()
    }
}

/// Target of a vertical move by `rows` display rows from `pos`; returns (pos, x, affinity).
fn vertical_target(ui: &Ui, doc: &mut Document, lay: &Lay, pos: usize, rows: isize, pref_x: Option<f32>) -> (usize, f32, bool) {
    let line = doc.line_of(pos);
    let g = lay.galley(ui, doc, line);
    let ltext = doc.line_text(line);
    let ci = char_index(ltext, pos - doc.line_start(line));
    let cur = g.from_ccursor(ccursor(doc, ci));
    let x = pref_x.unwrap_or_else(|| g.pos_from_cursor(&cur).min.x);
    let (mut l, mut row) = (line, cur.rcursor.row as isize + rows);
    let mut target_x = x;
    let mut g = g;
    while row < 0 {
        match doc.prev_visible(l) {
            Some(p) => {
                l = p;
                g = lay.galley(ui, doc, l);
                row += g.rows.len().max(1) as isize;
            }
            None => {
                row = 0;
                target_x = 0.0;
                break;
            }
        }
    }
    while row as usize >= g.rows.len().max(1) {
        match doc.next_visible(l) {
            Some(n) => {
                row -= g.rows.len().max(1) as isize;
                l = n;
                g = lay.galley(ui, doc, l);
            }
            None => {
                row = g.rows.len().max(1) as isize - 1;
                target_x = f32::INFINITY;
                break;
            }
        }
    }
    let y = g.rows.get(row as usize).map_or(0.0, |r| r.rect.center().y);
    let c = g.cursor_from_pos(vec2(target_x.min(1e7), y));
    let p = doc.line_start(l) + byte_index(doc.line_text(l), c.ccursor.index);
    (p, x, !c.ccursor.prefer_next_row)
}

fn move_vertical(ui: &Ui, doc: &mut Document, lay: &Lay, rows: isize, extend: bool) {
    let (pos, x, aff) = vertical_target(ui, doc, lay, doc.cursor, rows, doc.view.preferred_x);
    doc.cursor = pos;
    if !extend {
        doc.anchor = pos;
    }
    doc.extra.clear();
    doc.rect = None;
    doc.view.scroll = Some(ScrollTo::Cursor);
    doc.view.preferred_x = Some(x);
    doc.view.row_end_affinity = aff;
}

/// Scrolls the view by display rows.
fn scroll_rows(ui: &Ui, doc: &mut Document, lay: &Lay, rows: isize) {
    if !lay.wrap_width.is_finite() {
        let mut l = doc.view.first_line;
        if rows > 0 {
            for _ in 0..rows {
                match doc.next_visible(l) {
                    Some(n) => l = n,
                    None => break,
                }
            }
        } else {
            for _ in 0..(-rows) {
                match doc.prev_visible(l) {
                    Some(p) => l = p,
                    None => break,
                }
            }
        }
        doc.view.first_line = l;
        doc.view.first_row = 0;
        return;
    }
    let (mut l, mut r) = (doc.view.first_line, doc.view.first_row as isize + rows);
    while r < 0 {
        match doc.prev_visible(l) {
            Some(p) => {
                l = p;
                r += lay.rows(ui, doc, l) as isize;
            }
            None => {
                r = 0;
                break;
            }
        }
    }
    loop {
        let len = lay.rows(ui, doc, l) as isize;
        if r < len {
            break;
        }
        match doc.next_visible(l) {
            Some(n) => {
                r -= len;
                l = n;
            }
            None => {
                r = len - 1;
                break;
            }
        }
    }
    doc.view.first_line = l;
    doc.view.first_row = r as usize;
}

/// Public scrolling by lines (synchronized scrolling of the two views).
pub fn scroll_lines(doc: &mut Document, lines: isize) {
    let mut l = doc.view.first_line;
    for _ in 0..lines.unsigned_abs() {
        let n = if lines > 0 { doc.next_visible(l) } else { doc.prev_visible(l) };
        match n {
            Some(n) => l = n,
            None => break,
        }
    }
    doc.view.first_line = l;
    doc.view.first_row = 0;
}

fn clamp_scroll(doc: &mut Document, beyond_end: bool, vis_rows: usize) {
    let n = doc.line_count();
    if doc.view.first_line >= n {
        doc.view.first_line = n - 1;
        doc.view.first_row = 0;
    }
    if !beyond_end && doc.view.first_line + vis_rows > n && !doc.has_hidden() {
        doc.view.first_line = n.saturating_sub(vis_rows);
        doc.view.first_row = 0;
    }
}

fn scroll_into_view(ui: &Ui, doc: &mut Document, lay: &Lay, vis_rows: usize, width: f32, mode: ScrollTo) {
    let line = doc.line_of(doc.cursor);
    let g = lay.galley(ui, doc, line);
    let ci = char_index(doc.line_text(line), doc.cursor - doc.line_start(line));
    let row = if lay.wrap_width.is_finite() { g.from_ccursor(ccursor(doc, ci)).rcursor.row } else { 0 };
    let (fl, fr) = (doc.view.first_line, doc.view.first_row);

    let set_top_back = |doc: &mut Document, back: usize| {
        let (mut l, mut r, mut n) = (line, row, back);
        while n > 0 {
            if r > 0 {
                let s = r.min(n);
                r -= s;
                n -= s;
            } else if let Some(p) = doc.prev_visible(l) {
                l = p;
                r = lay.rows(ui, doc, l) - 1;
                n -= 1;
            } else {
                break;
            }
        }
        doc.view.first_line = l;
        doc.view.first_row = r;
    };

    let far = line + vis_rows * 3 < fl || line > fl + vis_rows * 3;
    if mode == ScrollTo::Center || far {
        let visible_now = (line, row) >= (fl, fr) && rows_between(ui, doc, lay, (fl, fr), (line, row), vis_rows) + 1 < vis_rows;
        if mode == ScrollTo::Cursor || !visible_now {
            set_top_back(doc, vis_rows / 2);
        }
    } else if (line, row) < (fl, fr) {
        doc.view.first_line = line;
        doc.view.first_row = row;
    } else {
        let d = rows_between(ui, doc, lay, (fl, fr), (line, row), vis_rows + 1);
        if d >= vis_rows {
            set_top_back(doc, vis_rows - 1);
        }
    }

    if !lay.wrap_width.is_finite() {
        let x = g.pos_from_ccursor(ccursor(doc, ci)).min.x;
        let margin = lay.char_w * 4.0;
        if x < doc.view.x_offset + lay.char_w {
            doc.view.x_offset = (x - margin).max(0.0);
        } else if x > doc.view.x_offset + width - lay.char_w * 2.0 {
            doc.view.x_offset = x - width + margin;
        }
    }
}

/// Display rows from `a` to `b` (capped at `cap`).
fn rows_between(ui: &Ui, doc: &mut Document, lay: &Lay, a: (usize, usize), b: (usize, usize), cap: usize) -> usize {
    let (mut l, mut r, mut n) = (a.0, a.1, 0);
    while (l, r) < b && n < cap {
        if l == b.0 {
            n += b.1 - r;
            break;
        }
        n += lay.rows(ui, doc, l) - r;
        r = 0;
        match doc.next_visible(l) {
            Some(x) => l = x,
            None => break,
        }
    }
    n.min(cap)
}

#[allow(clippy::too_many_arguments)]
fn scrollbars(ui: &Ui, doc: &mut Document, rect: Rect, text_rect: Rect, w: f32, vis_rows: usize, pal: &Palette, wrap: bool, char_w: f32, view: usize) {
    let n = doc.line_count();
    let track_color = pal.gutter_bg;
    let thumb_color = pal.gutter_fg.gamma_multiply(0.6);
    let thumb_hover = pal.gutter_fg;

    // Vertical: one step per line; the last line can be scrolled to the top.
    let vrect = Rect::from_min_max(pos2(rect.right() - w, rect.top()), pos2(rect.right(), text_rect.bottom()));
    let total = (n + vis_rows - 1).max(1) as f32;
    let frac = vis_rows as f32 / total;
    let th = (vrect.height() * frac).clamp(20.0_f32.min(vrect.height()), vrect.height());
    let pos = doc.view.first_line as f32 / (n.max(2) - 1) as f32;
    let ty = vrect.top() + (vrect.height() - th) * pos.clamp(0.0, 1.0);
    let thumb = Rect::from_min_size(pos2(vrect.left() + 2.0, ty), vec2(w - 4.0, th));
    let resp = ui.interact(vrect, Id::new(("vsb", doc.id, view)), Sense::click_and_drag());
    ui.painter().rect_filled(vrect, 0.0, track_color);
    let hot = resp.hovered() || resp.dragged();
    if n > 1 {
        ui.painter().rect_filled(thumb, 3.0, if hot { thumb_hover } else { thumb_color });
    }
    // Bookmarks as ticks in the scrollbar
    let tick = |line: usize| vrect.top() + vrect.height() * line as f32 / n.max(1) as f32;
    for &b in doc.bookmarks.iter().take(2000) {
        let y = tick(b);
        ui.painter().line_segment([pos2(vrect.left() + 1.0, y), pos2(vrect.right() - 1.0, y)], Stroke::new(2.0_f32, pal.bookmark));
    }
    if let Some(p) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let gid = Id::new(("vsb_grab", doc.id, view));
            let grab = ui.data_mut(|d| {
                let g = d.get_temp_mut_or_insert_with(gid, || f32::NAN);
                if resp.drag_started() || resp.clicked() || g.is_nan() {
                    *g = if thumb.contains(p) { p.y - thumb.top() } else { th / 2.0 };
                }
                *g
            });
            let f = ((p.y - grab - vrect.top()) / (vrect.height() - th).max(1.0)).clamp(0.0, 1.0);
            let target = (f * (n - 1) as f32).round() as usize;
            doc.view.first_line = doc.visible_from(target);
            doc.view.first_row = 0;
        }
        if resp.drag_stopped() {
            ui.data_mut(|d| d.remove::<f32>(Id::new(("vsb_grab", doc.id, view))));
        }
    }

    // Horizontal (no wrap)
    if wrap {
        return;
    }
    let hrect = Rect::from_min_max(pos2(text_rect.left() - 4.0, rect.bottom() - w), pos2(rect.right() - w, rect.bottom()));
    ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), hrect.top()), rect.max), 0.0, track_color);
    let content = (doc.view.max_width + char_w * 4.0).max(doc.view.x_offset + text_rect.width());
    let frac = (text_rect.width() / content).min(1.0);
    let tw = (hrect.width() * frac).max(20.0_f32.min(hrect.width()));
    let max_off = (content - text_rect.width()).max(1.0);
    let tx = hrect.left() + (hrect.width() - tw) * (doc.view.x_offset / max_off).clamp(0.0, 1.0);
    let thumb = Rect::from_min_size(pos2(tx, hrect.top() + 2.0), vec2(tw, w - 4.0));
    let resp = ui.interact(hrect, Id::new(("hsb", doc.id, view)), Sense::click_and_drag());
    let hot = resp.hovered() || resp.dragged();
    if frac < 1.0 {
        ui.painter().rect_filled(thumb, 3.0, if hot { thumb_hover } else { thumb_color });
    }
    if let Some(p) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let f = ((p.x - tw / 2.0 - hrect.left()) / (hrect.width() - tw).max(1.0)).clamp(0.0, 1.0);
            doc.view.x_offset = f * max_off;
        }
    }
}

// ---- auto-completion ---------------------------------------------------------------------------

fn prefix_at(doc: &Document, extra: &str) -> Option<(usize, String)> {
    let c = doc.cursor;
    let is_w = |ch: char| ch.is_alphanumeric() || ch == '_' || extra.contains(ch);
    let start = doc.text()[..c].char_indices().rev().take_while(|(_, ch)| is_w(*ch)).last().map(|(i, _)| i)?;
    Some((start, doc.text()[start..c].to_string()))
}

/// Opens/updates the completion list for the word before the caret.
/// `auto` = triggered by typing (respects the minimum prefix length).
fn update_completion(doc: &Document, was_open: bool, out: &mut Option<Completion>, s: &Settings, keywords: &[String], extra: &str, auto: bool) {
    *out = None;
    if doc.is_multi() || doc.has_sel() {
        return;
    }
    if auto && s.completion == CompletionMode::Off && !was_open {
        return;
    }
    let Some((start, prefix)) = prefix_at(doc, extra) else { return };
    if auto && !was_open && prefix.chars().count() < s.completion_from.max(1) {
        return;
    }
    if s.completion_ignore_numbers && prefix.chars().all(|c| c.is_ascii_digit()) {
        return;
    }
    let lower = prefix.to_lowercase();
    let mut items: Vec<String> = Vec::new();
    let want_kw = !auto || matches!(s.completion, CompletionMode::Functions | CompletionMode::Both);
    let want_words = keywords.is_empty() || !auto || matches!(s.completion, CompletionMode::Words | CompletionMode::Both);
    if want_kw {
        items.extend(keywords.iter().filter(|k| k.to_lowercase().starts_with(&lower) && **k != prefix).cloned());
    }
    if want_words {
        // Words of the document (near the caret for huge files).
        let t = doc.text();
        let from = doc.clamp(doc.cursor.saturating_sub(1 << 20));
        let to = doc.clamp((doc.cursor + (1 << 20)).min(t.len()));
        let mut seen = std::collections::HashSet::new();
        let is_w = |ch: char| ch.is_alphanumeric() || ch == '_' || extra.contains(ch);
        let mut ws: Option<usize> = None;
        for (i, ch) in t[from..to].char_indices().chain(std::iter::once((to - from, ' '))) {
            let i = from + i;
            if is_w(ch) {
                ws.get_or_insert(i);
            } else if let Some(w0) = ws.take() {
                let w = &t[w0..i];
                if w0 != start && w.len() > prefix.len() && w.to_lowercase().starts_with(&lower) && seen.insert(w) {
                    items.push(w.to_string());
                }
            }
        }
    }
    items.sort_by_key(|w| w.to_lowercase());
    items.dedup();
    items.truncate(200);
    if !items.is_empty() {
        *out = Some(Completion { items, selected: 0, start, doc_id: doc.id });
    }
}

/// Opens the parameter hint for the call around the caret.
fn open_calltip(doc: &Document, api: Option<&crate::api::Api>, out: &mut Option<crate::api::CallTip>) {
    let (start, stop, sep) = api.map_or(('(', ')', ','), |a| (a.start, a.stop, a.sep));
    *out = None;
    let Some((name, open)) = crate::api::enclosing_call(doc.text(), doc.cursor, start, stop) else { return };
    let mut overloads = api.and_then(|a| a.lookup(&name)).cloned().unwrap_or_default();
    if overloads.is_empty() {
        overloads = crate::api::from_text(doc.text(), &name);
    }
    if !overloads.is_empty() {
        *out = Some(crate::api::CallTip { doc_id: doc.id, name, open, overloads, current: 0, stop, sep });
    }
}

fn calltip_popup(ui: &Ui, doc: &Document, visible: &[VisLine], text_rect: Rect, tip: &mut crate::api::CallTip, param: usize) {
    use egui::text::{LayoutJob, TextFormat};
    let open = tip.open.saturating_sub(1);
    let line = doc.line_of(open);
    let Some(vl) = visible.iter().find(|vl| vl.line == line) else { return };
    let ci = char_index(doc.line_text(line), open - doc.line_start(line));
    let p = vl.galley.pos_from_ccursor(CCursor::new(ci));
    let anchor = pos2(text_rect.left() - doc.view.x_offset + p.min.x, vl.top + p.max.y + 2.0);
    let n = tip.overloads.len();
    tip.current = tip.current.min(n - 1);
    let o = &tip.overloads[tip.current];
    let (pre, cur, post) = o.render(&tip.name, param, tip.sep);
    let font = egui::FontId::monospace(ui.style().text_styles[&egui::TextStyle::Body].size);
    let fg = ui.visuals().text_color();
    let mut job = LayoutJob::default();
    let normal = TextFormat { font_id: font.clone(), color: fg, ..Default::default() };
    job.append(&pre, 0.0, normal.clone());
    job.append(&cur, 0.0, TextFormat { color: ui.visuals().strong_text_color(), background: ui.visuals().selection.bg_fill.gamma_multiply(0.6), ..normal.clone() });
    job.append(&post, 0.0, normal);
    egui::Area::new(Id::new("calltip_popup")).order(egui::Order::Foreground).fixed_pos(anchor).interactable(true).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_max_width(640.0);
            ui.horizontal(|ui| {
                if n > 1 {
                    if ui.small_button("▲").clicked() {
                        tip.current = (tip.current + n - 1) % n;
                    }
                    ui.label(format!("{}/{}", tip.current + 1, n));
                    if ui.small_button("▼").clicked() {
                        tip.current = (tip.current + 1) % n;
                    }
                }
                ui.label(job);
            });
            if !o.descr.is_empty() {
                ui.label(egui::RichText::new(&o.descr).weak());
            }
        });
    });
}

fn path_completion(doc: &Document) -> Option<Completion> {
    let c = doc.cursor;
    let ls = doc.line_start(doc.line_of(c));
    let before = &doc.text()[ls..c];
    let start_rel = before.rfind(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | '(' | '=')).map_or(0, |i| i + 1);
    let token = &before[start_rel..];
    let (dir, name) = match token.rfind('/') {
        Some(i) => (&token[..=i], &token[i + 1..]),
        None => ("./", token),
    };
    let base = if let Some(rest) = dir.strip_prefix('~') {
        std::env::var("HOME").unwrap_or_default() + rest
    } else if dir.starts_with('/') {
        dir.to_string()
    } else {
        doc.path.as_ref().and_then(|p| p.parent()).map(|p| p.join(dir).display().to_string()).unwrap_or(dir.to_string())
    };
    let mut items: Vec<String> = std::fs::read_dir(&base)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            n.starts_with(name).then(|| if is_dir { format!("{n}/") } else { n })
        })
        .collect();
    items.sort();
    if items.is_empty() {
        return None;
    }
    Some(Completion { items, selected: 0, start: c - name.len(), doc_id: doc.id })
}

fn accept_completion(doc: &mut Document, c: &Completion, idx: usize) {
    let Some(item) = c.items.get(idx) else { return };
    let r = c.start..doc.cursor.max(c.start);
    doc.replace(r, item, EditKind::Other);
}

fn handle_completion_key(doc: &mut Document, comp: &mut Option<Completion>, key: Key, m: Modifiers) -> bool {
    let Some(c) = comp.as_mut() else { return false };
    if m.ctrl || m.alt {
        return false;
    }
    match key {
        Key::ArrowDown => c.selected = (c.selected + 1).min(c.items.len() - 1),
        Key::ArrowUp => c.selected = c.selected.saturating_sub(1),
        Key::PageDown => c.selected = (c.selected + 8).min(c.items.len() - 1),
        Key::PageUp => c.selected = c.selected.saturating_sub(8),
        Key::Enter | Key::Tab => {
            let c = comp.take().unwrap();
            accept_completion(doc, &c, c.selected);
        }
        Key::Escape => *comp = None,
        _ => return false,
    }
    true
}

fn completion_popup(ui: &Ui, doc: &Document, visible: &[VisLine], text_rect: Rect, c: &mut Completion, row_h: f32) -> Option<usize> {
    let line = doc.line_of(c.start);
    let vl = visible.iter().find(|vl| vl.line == line)?;
    let ci = char_index(doc.line_text(line), c.start - doc.line_start(line));
    let p = vl.galley.pos_from_ccursor(CCursor::new(ci));
    let pos = pos2(text_rect.left() - doc.view.x_offset + p.min.x, vl.top + p.max.y + 2.0);
    let mut chosen = None;
    egui::Area::new(Id::new("completion_popup")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_max_width(320.0);
            egui::ScrollArea::vertical().max_height(row_h * 10.0).show(ui, |ui| {
                for (i, item) in c.items.iter().enumerate() {
                    let r = ui.selectable_label(i == c.selected, egui::RichText::new(item).monospace());
                    if i == c.selected {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        chosen = Some(i);
                    }
                }
            });
        });
    });
    chosen
}

fn context_menu(ui: &mut Ui, doc: &Document, cmds: &mut Vec<Cmd>, extra: &[String]) {
    use crate::i18n::tr;
    let mut item = |ui: &mut Ui, label: &str, sc: &str, cmd: Cmd, enabled: bool| {
        if ui.add_enabled(enabled, egui::Button::new(tr(label)).shortcut_text(sc)).clicked() {
            cmds.push(cmd);
            ui.close_menu();
        }
    };
    let rw = !doc.read_only;
    let sel = doc.has_sel();
    item(ui, "Cut", "Ctrl+X", Cmd::Cut, rw);
    item(ui, "Copy", "Ctrl+C", Cmd::Copy, true);
    item(ui, "Paste", "Ctrl+V", Cmd::Paste, rw);
    item(ui, "Delete", "Del", Cmd::Delete, rw && sel);
    item(ui, "Select All", "Ctrl+A", Cmd::SelectAll, true);
    ui.separator();
    item(ui, "Style all occurrences of token", "", Cmd::StyleAll(1), true);
    item(ui, "Clear all styles", "", Cmd::ClearStyle(0), true);
    ui.separator();
    item(ui, "UPPERCASE", "Ctrl+Shift+U", Cmd::Case(textops::Case::Upper), rw && sel);
    item(ui, "lowercase", "Ctrl+U", Cmd::Case(textops::Case::Lower), rw && sel);
    ui.separator();
    item(ui, "Open File", "", Cmd::OpenSelectedFile, sel);
    item(ui, "Search on Internet", "", Cmd::SearchInternet, true);
    ui.separator();
    item(ui, "Toggle Single Line Comment", "Ctrl+Q", Cmd::ToggleComment, rw);
    item(ui, "Block Comment", "Ctrl+Shift+Q", Cmd::BlockComment, rw);
    item(ui, "Block Uncomment", "Ctrl+Shift+K", Cmd::BlockUncomment, rw);
    ui.separator();
    item(ui, "Hide Lines", "Alt+H", Cmd::HideLines, true);
    item(ui, "Toggle Bookmark", "Ctrl+F2", Cmd::ToggleBookmark, true);
    if !extra.is_empty() {
        ui.separator();
        for e in extra {
            if let Some(cmd) = Cmd::from_id(e) {
                item(ui, &crate::cmd::label(&cmd), "", cmd, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Eol;
    use eframe::egui::{CentralPanel, Context, RawInput};

    struct Harness {
        ctx: Context,
        hl: Highlighting,
        s: Settings,
        doc: Document,
        overwrite: bool,
        completion: Option<Completion>,
        calltip: Option<crate::api::CallTip>,
        text_left: f32,
    }

    impl Harness {
        fn new(text: &str) -> Self {
            let mut doc = Document::new_untitled(1, 1, Eol::Lf);
            doc.set_text(text.to_string(), 0..0);
            Self { ctx: Context::default(), hl: Highlighting::new(), s: Settings::default(), doc, overwrite: false, completion: None, calltip: None, text_left: 0.0 }
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.frame_mods(events, Modifiers::NONE);
        }

        fn frame_mods(&mut self, events: Vec<Event>, modifiers: Modifiers) {
            let raw = RawInput { events, modifiers, screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 400.0))), ..Default::default() };
            let pal = Palette::new(self.hl.theme(self.s.theme()), &self.s);
            let mut cmds = Vec::new();
            let mut copied = None;
            let kw: Vec<String> = Vec::new();
            let mut left = 0.0;
            let _ = self.ctx.run(raw, |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    ui.memory_mut(|m| m.request_focus(editor_id(self.doc.id, 0)));
                    let syntax = self.doc.syntax.clone();
                    let tabs = self.s.tabs_for(syntax.as_deref());
                    left = ui.available_rect_before_wrap().left();
                    show(
                        ui,
                        &mut self.doc,
                        Cfg {
                            hl: &self.hl,
                            s: &self.s,
                            pal: &pal,
                            syntax: syntax.as_deref(),
                            udl: None,
                            overwrite: &mut self.overwrite,
                            cmds: &mut cmds,
                            completion: &mut self.completion,
                            calltip: &mut self.calltip,
                            api: None,
                            keywords: &kw,
                            tabs,
                            large: false,
                            view: 0,
                            copied: &mut copied,
                        },
                    );
                });
            });
            self.text_left = left;
        }

        fn key(&mut self, key: Key, modifiers: Modifiers) {
            self.frame(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
        }

        fn typ(&mut self, t: &str) {
            self.frame(vec![Event::Text(t.into())]);
        }

        fn click(&mut self, p: Pos2, modifiers: Modifiers) {
            self.frame_mods(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers }], modifiers);
            self.frame_mods(vec![Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers }], modifiers);
        }

        fn row_h(&self) -> f32 {
            self.ctx.fonts(|f| f.row_height(&FontId::monospace(self.s.font_size)))
        }
    }

    #[test]
    fn typing_and_editing_keys() {
        let mut h = Harness::new("");
        h.doc.syntax = Some("Rust".into());
        h.frame(vec![]);
        h.typ("fn x() {");
        h.key(Key::Enter, Modifiers::NONE);
        assert_eq!(h.doc.text(), "fn x() {\n\t", "auto-indent after brace");
        h.typ("a");
        h.key(Key::Enter, Modifiers::NONE);
        assert_eq!(h.doc.text(), "fn x() {\n\ta\n\t");
        h.key(Key::Backspace, Modifiers::NONE);
        h.typ("}");
        assert_eq!(h.doc.text(), "fn x() {\n\ta\n}");
        h.key(Key::ArrowUp, Modifiers::NONE);
        h.key(Key::End, Modifiers::NONE);
        h.key(Key::Home, Modifiers::NONE);
        assert_eq!(h.doc.line_col(h.doc.cursor), (1, 1), "smart home: first non-blank");
        h.key(Key::Home, Modifiers::NONE);
        assert_eq!(h.doc.line_col(h.doc.cursor), (1, 0));
        h.key(Key::End, Modifiers::SHIFT);
        assert_eq!(h.doc.selected_text(), "\ta");
        h.typ("b");
        assert_eq!(h.doc.text(), "fn x() {\nb\n}");
        h.key(Key::Z, Modifiers::CTRL);
        assert_eq!(h.doc.text(), "fn x() {\n\ta\n}");
        h.key(Key::Y, Modifiers::CTRL);
        assert_eq!(h.doc.text(), "fn x() {\nb\n}");
        h.key(Key::ArrowLeft, Modifiers::CTRL);
        assert_eq!(h.doc.line_col(h.doc.cursor), (1, 0));
        h.key(Key::Delete, Modifiers::NONE);
        assert_eq!(h.doc.text(), "fn x() {\n\n}");
        h.key(Key::End, Modifiers::CTRL);
        assert_eq!(h.doc.cursor, h.doc.text().len());
        h.key(Key::A, Modifiers::CTRL);
        assert_eq!(h.doc.selected_text(), h.doc.text());
    }

    #[test]
    fn tab_indents_selection_and_overwrite() {
        let mut h = Harness::new("a\nb\nc");
        h.frame(vec![]);
        h.key(Key::ArrowDown, Modifiers::SHIFT);
        h.key(Key::ArrowDown, Modifiers::SHIFT);
        h.key(Key::Tab, Modifiers::NONE);
        assert_eq!(h.doc.text(), "\ta\n\tb\nc", "lines touched by the selection (end at col 0 excluded)");
        h.key(Key::Tab, Modifiers::SHIFT);
        assert_eq!(h.doc.text(), "a\nb\nc");
        h.doc.set_sel(0, 0);
        h.key(Key::Insert, Modifiers::NONE);
        h.typ("XY");
        assert_eq!(h.doc.text(), "XY\nb\nc", "overwrite stops at the line end");
    }

    #[test]
    fn multi_caret_and_column_selection() {
        let alt_shift = Modifiers { alt: true, shift: true, ..Modifiers::NONE };
        let mut h = Harness::new("abc\nabc\nabc");
        h.frame(vec![]);
        h.key(Key::ArrowRight, Modifiers::NONE);
        h.key(Key::ArrowDown, alt_shift);
        h.key(Key::ArrowDown, alt_shift);
        assert_eq!(h.doc.all_sels().len(), 3);
        h.typ("X");
        assert_eq!(h.doc.text(), "aXbc\naXbc\naXbc");
        h.key(Key::Backspace, Modifiers::NONE);
        assert_eq!(h.doc.text(), "abc\nabc\nabc");
        h.key(Key::End, Modifiers::NONE);
        h.typ(";");
        assert_eq!(h.doc.text(), "abc;\nabc;\nabc;");
        h.key(Key::Z, Modifiers::CTRL);
        assert_eq!(h.doc.text(), "abc\nabc\nabc");
        h.key(Key::Escape, Modifiers::NONE);
        assert!(!h.doc.is_multi());
        // Column selection, then paste distributes the clipboard lines
        h.doc.set_sel(0, 0);
        h.key(Key::ArrowDown, alt_shift);
        h.key(Key::ArrowRight, alt_shift);
        assert_eq!(h.doc.all_sels().iter().map(|&(a, c)| c.abs_diff(a)).collect::<Vec<_>>(), vec![1, 1]);
        h.frame(vec![Event::Paste("1\n2".into())]);
        assert_eq!(h.doc.text(), "1bc\n2bc\nabc");
        // Ctrl+click adds a caret: click at the end of line 3
        h.doc.set_sel(0, 0);
        let y = h.row_h() * 2.5 + 8.0;
        h.click(pos2(700.0, y), Modifiers::CTRL);
        assert_eq!(h.doc.all_sels().len(), 2);
        h.typ("_");
        assert_eq!(h.doc.text(), "_1bc\n2bc\nabc_");
    }

    #[test]
    fn folding_hides_lines_and_navigation_skips_them() {
        let mut h = Harness::new("fn a() {\n    x;\n    y;\n}\nend");
        h.doc.syntax = Some("Rust".into());
        h.frame(vec![]);
        h.doc.toggle_fold(0);
        h.frame(vec![]);
        h.key(Key::ArrowDown, Modifiers::NONE);
        assert_eq!(h.doc.line_of(h.doc.cursor), 4, "down skips the folded block");
        h.key(Key::ArrowUp, Modifiers::NONE);
        assert_eq!(h.doc.line_of(h.doc.cursor), 0);
        // Jumping into the block unfolds it
        h.doc.goto_line(2);
        h.frame(vec![]);
        assert!(h.doc.folded.is_empty());
    }

    #[test]
    fn scrolling_follows_cursor() {
        let text: String = (0..1000).map(|i| format!("line {i}\n")).collect();
        let mut h = Harness::new(&text);
        h.frame(vec![]);
        h.key(Key::End, Modifiers::CTRL);
        h.frame(vec![]);
        let fl = h.doc.view.first_line;
        assert!(fl > 900 && fl <= 1000, "view at the end, first_line={fl}");
        h.key(Key::Home, Modifiers::CTRL);
        assert_eq!(h.doc.view.first_line, 0);
        h.key(Key::PageDown, Modifiers::NONE);
        let (line, _) = h.doc.line_col(h.doc.cursor);
        assert!(line > 10, "page down moved the cursor to line {line}");
        assert!(h.doc.view.first_line > 0);
        h.doc.goto_line(500);
        h.frame(vec![]);
        let fl = h.doc.view.first_line;
        assert!(fl < 500 && fl > 470, "goto centers, first_line={fl}");
    }

    #[test]
    fn wrapped_vertical_movement() {
        let long = "word ".repeat(100);
        let mut h = Harness::new(&format!("{long}\nend"));
        h.s.word_wrap = true;
        h.frame(vec![]);
        h.key(Key::ArrowDown, Modifiers::NONE);
        let (line, col) = h.doc.line_col(h.doc.cursor);
        assert_eq!(line, 0, "down moves to the next display row of the same line");
        assert!(col > 20, "col={col}");
        for _ in 0..10 {
            h.key(Key::ArrowDown, Modifiers::NONE);
        }
        assert_eq!(h.doc.line_col(h.doc.cursor).0, 1);
    }

    #[test]
    fn mouse_click_places_cursor() {
        let mut h = Harness::new("hello world\nsecond line");
        h.frame(vec![]);
        let p = pos2(700.0, 8.0 + h.row_h() * 1.5);
        h.click(p, Modifiers::NONE);
        assert_eq!(h.doc.cursor, h.doc.text().len());
        let _ = h.text_left;
    }

    #[test]
    fn copy_without_selection_copies_line() {
        let mut h = Harness::new("one\ntwo\n");
        h.frame(vec![]);
        h.frame(vec![Event::Cut]);
        assert_eq!(h.doc.text(), "two\n");
        h.frame(vec![Event::Paste("x\r\ny".into())]);
        assert_eq!(h.doc.text(), "x\nytwo\n");
    }

    #[test]
    fn auto_close_and_completion() {
        let mut h = Harness::new("");
        h.s.auto_close_parens = true;
        h.s.auto_close_dquote = true;
        h.frame(vec![]);
        h.typ("f");
        h.typ("(");
        assert_eq!(h.doc.text(), "f()");
        h.typ("\"");
        assert_eq!(h.doc.text(), "f(\"\")");
        h.typ("\"");
        h.typ(")");
        assert_eq!(h.doc.text(), "f(\"\")", "closing chars step over");
        assert_eq!(h.doc.cursor, 5);
        // Word completion from the document
        h.doc.set_text("alphabet\nalpine\n".into(), 16..16);
        h.typ("a");
        h.typ("l");
        h.typ("p");
        assert!(h.completion.as_ref().is_some_and(|c| c.items == vec!["alphabet", "alpine"]), "{:?}", h.completion);
        h.key(Key::ArrowDown, Modifiers::NONE);
        h.key(Key::Enter, Modifiers::NONE);
        assert_eq!(h.doc.text(), "alphabet\nalpine\nalpine");
        assert!(h.completion.is_none());
    }

    #[test]
    fn parameter_hint() {
        let src = "fn add(left: i32, right: i32) {}\n";
        let mut h = Harness::new(src);
        h.doc.set_text(src.into(), src.len()..src.len());
        h.frame(vec![]);
        for c in ["a", "d", "d", "("] {
            h.typ(c);
        }
        let tip = h.calltip.clone().expect("hint opens on (");
        assert_eq!((tip.name.as_str(), tip.overloads[0].params.len()), ("add", 2));
        h.typ("1");
        h.typ(",");
        assert_eq!(crate::api::param_index(h.doc.text(), tip.open, h.doc.cursor, &tip), Some(1));
        h.typ(")");
        h.frame(vec![]);
        assert!(h.calltip.is_none(), "closed after )");
        // Ctrl+Shift+Space reopens it inside the call
        h.key(Key::ArrowLeft, Modifiers::NONE);
        h.key(Key::Space, Modifiers { ctrl: true, shift: true, ..Default::default() });
        assert!(h.calltip.is_some());
        h.key(Key::Escape, Modifiers::NONE);
        assert!(h.calltip.is_none());
    }

    #[test]
    fn tag_matching() {
        let t = "<a><b>x</b></a>";
        assert_eq!(matching_tags(t, 1), Some((1..2, 13..14)));
        assert_eq!(matching_tags(t, 9), Some((4..5, 9..10)));
    }

    #[test]
    #[ignore]
    fn perf_large_file() {
        let text: String = (0..1_000_000).map(|i| format!("{i}: fn value_{i}() -> u32 {{ \"string\" + 42 }} // comment\n")).collect();
        eprintln!("size: {} MB", text.len() / 1_048_576);
        let mut h = Harness::new("");
        let t = Instant::now();
        h.doc.replace_text_raw(text);
        h.doc.syntax = Some("Rust".into());
        h.s.highlight_limit_mb = 1000.0;
        h.frame(vec![]);
        eprintln!("load + first frame: {:?}", t.elapsed());
        let t = Instant::now();
        for _ in 0..50 {
            h.typ("x");
        }
        eprintln!("typing at top: {:?}/char", t.elapsed() / 50);
        let t = Instant::now();
        for _ in 0..50 {
            h.key(Key::PageDown, Modifiers::NONE);
        }
        eprintln!("page down: {:?}/frame", t.elapsed() / 50);
    }
}
