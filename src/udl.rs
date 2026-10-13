//! User Defined Languages, compatible with Notepad++'s `userDefineLang` XML format
//! (files in ~/.config/notepad-rs/userDefineLangs/ can be copied from Notepad++).

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use syntect::highlighting::{Color, FontStyle, Style};

use crate::highlight::Spans;

#[derive(Clone, Debug, PartialEq, Hash)]
#[derive(Default)]
pub struct UStyle {
    pub fg: [u8; 3],
    pub bg: Option<[u8; 3]>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}


pub const STYLE_NAMES: [&str; 23] = [
    "DEFAULT",
    "COMMENTS",
    "LINE COMMENTS",
    "NUMBERS",
    "KEYWORDS1",
    "KEYWORDS2",
    "KEYWORDS3",
    "KEYWORDS4",
    "KEYWORDS5",
    "KEYWORDS6",
    "KEYWORDS7",
    "KEYWORDS8",
    "OPERATORS",
    "FOLDER IN CODE1",
    "FOLDER IN CODE2",
    "DELIMITERS1",
    "DELIMITERS2",
    "DELIMITERS3",
    "DELIMITERS4",
    "DELIMITERS5",
    "DELIMITERS6",
    "DELIMITERS7",
    "DELIMITERS8",
];

#[derive(Clone, Debug, PartialEq, Hash, Default)]
pub struct Delimiter {
    pub open: String,
    pub escape: String,
    pub close: String,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct UserLang {
    pub name: String,
    pub ext: Vec<String>,
    pub case_ignored: bool,
    pub line_comment: Vec<String>,
    pub block_comment: Vec<(String, String)>,
    /// Keyword lists 1–8, space separated.
    pub keywords: [String; 8],
    pub operators1: String,
    pub operators2: String,
    pub fold_open: String,
    pub fold_middle: String,
    pub fold_close: String,
    pub delimiters: [Delimiter; 8],
    pub styles: BTreeMap<String, UStyle>,
    /// Source file (for saving back).
    pub file: Option<PathBuf>,
}

impl Default for UserLang {
    fn default() -> Self {
        let mut styles = BTreeMap::new();
        let defaults: [(&str, [u8; 3], bool); 16] = [
            ("DEFAULT", [0, 0, 0], false),
            ("COMMENTS", [0, 128, 0], false),
            ("LINE COMMENTS", [0, 128, 0], false),
            ("NUMBERS", [255, 128, 0], false),
            ("KEYWORDS1", [0, 0, 255], true),
            ("KEYWORDS2", [128, 0, 255], true),
            ("KEYWORDS3", [0, 128, 128], false),
            ("KEYWORDS4", [128, 64, 0], false),
            ("KEYWORDS5", [0, 0, 128], false),
            ("KEYWORDS6", [128, 0, 0], false),
            ("KEYWORDS7", [64, 128, 128], false),
            ("KEYWORDS8", [128, 128, 0], false),
            ("OPERATORS", [128, 0, 64], true),
            ("FOLDER IN CODE1", [0, 0, 255], true),
            ("FOLDER IN CODE2", [0, 0, 255], true),
            ("DELIMITERS1", [128, 128, 128], false),
        ];
        for (n, fg, bold) in defaults {
            styles.insert(n.to_string(), UStyle { fg, bold, ..Default::default() });
        }
        for i in 2..=8 {
            styles.insert(format!("DELIMITERS{i}"), UStyle { fg: [128, 128, 128], ..Default::default() });
        }
        Self {
            name: String::new(),
            ext: Vec::new(),
            case_ignored: false,
            line_comment: Vec::new(),
            block_comment: Vec::new(),
            keywords: Default::default(),
            operators1: String::new(),
            operators2: String::new(),
            fold_open: String::new(),
            fold_middle: String::new(),
            fold_close: String::new(),
            delimiters: Default::default(),
            styles,
            file: None,
        }
    }
}

fn style_of(u: Option<&UStyle>, dark: bool) -> Style {
    let mut s = Style::default();
    let Some(u) = u else {
        return s;
    };
    let mut fg = u.fg;
    if dark && fg.iter().map(|&c| c as u32).sum::<u32>() < 200 {
        // Black text on a dark background: lighten it.
        fg = [fg[0].saturating_add(170), fg[1].saturating_add(170), fg[2].saturating_add(170)];
    }
    s.foreground = Color { r: fg[0], g: fg[1], b: fg[2], a: 255 };
    s.background = u.bg.map_or(Color { r: 0, g: 0, b: 0, a: 0 }, |b| Color { r: b[0], g: b[1], b: b[2], a: 110 });
    let mut fs = FontStyle::empty();
    if u.bold {
        fs |= FontStyle::BOLD;
    }
    if u.italic {
        fs |= FontStyle::ITALIC;
    }
    if u.underline {
        fs |= FontStyle::UNDERLINE;
    }
    s.font_style = fs;
    s
}

impl UserLang {
    pub fn signature(&self) -> String {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut h);
        DARK.with(|d| d.get()).hash(&mut h);
        h.finish().to_string()
    }

    fn style(&self, name: &str) -> Style {
        style_of(self.styles.get(name), DARK.with(|d| d.get()))
    }

    fn words(&self, i: usize) -> Vec<String> {
        self.keywords[i]
            .split_whitespace()
            .map(|w| if self.case_ignored { w.to_lowercase() } else { w.to_string() })
            .collect()
    }

    /// Highlights one line. `state`: 0 normal, 1+k inside block comment k, 100+k inside delimiter k.
    pub fn highlight_line(&self, line: &str, state: u8) -> (Spans, u8) {
        let mut spans: Spans = Vec::new();
        let push = |spans: &mut Spans, len: usize, st: Style| {
            if len == 0 {
                return;
            }
            if let Some(last) = spans.last_mut() {
                if last.1 == st {
                    last.0 += len;
                    return;
                }
            }
            spans.push((len, st));
        };
        let kw: Vec<Vec<String>> = (0..8).map(|i| self.words(i)).collect();
        let fold_words: Vec<String> = [&self.fold_open, &self.fold_middle, &self.fold_close]
            .iter()
            .flat_map(|s| s.split_whitespace())
            .map(|w| if self.case_ignored { w.to_lowercase() } else { w.to_string() })
            .collect();
        let ops: Vec<&str> = self.operators1.split_whitespace().chain(self.operators2.split_whitespace()).collect();
        let starts_at = |pos: usize, tok: &str| !tok.is_empty() && line[pos..].starts_with(tok);
        let mut pos = 0;
        let mut state = state;

        // Continue a block comment or delimiter from the previous line.
        if (1..100).contains(&state) {
            let (_, close) = &self.block_comment[(state - 1) as usize % self.block_comment.len().max(1)];
            match line.find(close.as_str()).filter(|_| !close.is_empty()) {
                Some(i) => {
                    push(&mut spans, i + close.len(), self.style("COMMENTS"));
                    pos = i + close.len();
                    state = 0;
                }
                None => {
                    push(&mut spans, line.len(), self.style("COMMENTS"));
                    return (spans, state);
                }
            }
        } else if state >= 100 {
            let k = (state - 100) as usize;
            let d = &self.delimiters[k.min(7)];
            match find_close(line, 0, d) {
                Some(e) => {
                    push(&mut spans, e, self.style(&format!("DELIMITERS{}", k + 1)));
                    pos = e;
                    state = 0;
                }
                None => {
                    push(&mut spans, line.len(), self.style(&format!("DELIMITERS{}", k + 1)));
                    return (spans, state);
                }
            }
        }

        while pos < line.len() {
            // Line comment
            if self.line_comment.iter().any(|t| starts_at(pos, t)) {
                push(&mut spans, line.len() - pos, self.style("LINE COMMENTS"));
                return (spans, 0);
            }
            // Block comment
            if let Some((k, (open, close))) = self.block_comment.iter().enumerate().find(|(_, (o, _))| starts_at(pos, o)) {
                let from = pos + open.len();
                match line[from..].find(close.as_str()).filter(|_| !close.is_empty()) {
                    Some(i) => {
                        push(&mut spans, open.len() + i + close.len(), self.style("COMMENTS"));
                        pos = from + i + close.len();
                        continue;
                    }
                    None => {
                        push(&mut spans, line.len() - pos, self.style("COMMENTS"));
                        return (spans, 1 + k as u8);
                    }
                }
            }
            // Delimiters (strings)
            if let Some((k, d)) = self.delimiters.iter().enumerate().find(|(_, d)| starts_at(pos, &d.open)) {
                let from = pos + d.open.len();
                let st = self.style(&format!("DELIMITERS{}", k + 1));
                match find_close(line, from, d) {
                    Some(e) => {
                        push(&mut spans, e - pos, st);
                        pos = e;
                        continue;
                    }
                    None => {
                        push(&mut spans, line.len() - pos, st);
                        return (spans, if d.close.is_empty() { 0 } else { 100 + k as u8 });
                    }
                }
            }
            // Operators
            if let Some(op) = ops.iter().filter(|o| starts_at(pos, o)).max_by_key(|o| o.len()) {
                push(&mut spans, op.len(), self.style("OPERATORS"));
                pos += op.len();
                continue;
            }
            let c = line[pos..].chars().next().unwrap();
            if c.is_whitespace() {
                push(&mut spans, c.len_utf8(), self.style("DEFAULT"));
                pos += c.len_utf8();
                continue;
            }
            // Word: up to whitespace, operator or delimiter start.
            let mut end = pos;
            for (i, ch) in line[pos..].char_indices() {
                let p = pos + i;
                if ch.is_whitespace()
                    || (i > 0 && (ops.iter().any(|o| starts_at(p, o)) || self.delimiters.iter().any(|d| starts_at(p, &d.open))))
                {
                    break;
                }
                end = p + ch.len_utf8();
            }
            let word = &line[pos..end];
            let key = if self.case_ignored { word.to_lowercase() } else { word.to_string() };
            let style = if is_number(word) {
                self.style("NUMBERS")
            } else if fold_words.contains(&key) {
                self.style("FOLDER IN CODE1")
            } else if let Some(k) = kw.iter().position(|l| l.contains(&key)) {
                self.style(&format!("KEYWORDS{}", k + 1))
            } else {
                self.style("DEFAULT")
            };
            push(&mut spans, end - pos, style);
            pos = end;
        }
        (spans, state)
    }

    // ---- Notepad++ XML ----------------------------------------------------------------------

    pub fn to_xml(&self) -> String {
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
        let mut comments = String::new();
        for t in &self.line_comment {
            comments.push_str(&format!("00{} ", t));
        }
        for (o, c) in &self.block_comment {
            comments.push_str(&format!("03{} 04{} ", o, c));
        }
        let mut delims = String::new();
        for (i, d) in self.delimiters.iter().enumerate() {
            delims.push_str(&format!("{:02}{} {:02}{} {:02}{} ", i * 3, d.open, i * 3 + 1, d.escape, i * 3 + 2, d.close));
        }
        let hex = |c: [u8; 3]| format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<NotepadPlus>\n");
        x.push_str(&format!(
            "    <UserLang name=\"{}\" ext=\"{}\" udlVersion=\"2.1\">\n        <Settings>\n            <Global caseIgnored=\"{}\" allowFoldOfComments=\"no\" foldCompact=\"no\" forcePureLC=\"0\" decimalSeparator=\"0\" />\n        </Settings>\n        <KeywordLists>\n",
            esc(&self.name),
            esc(&self.ext.join(" ")),
            if self.case_ignored { "yes" } else { "no" }
        ));
        let mut kl = |n: &str, v: &str| x.push_str(&format!("            <Keywords name=\"{n}\">{}</Keywords>\n", esc(v)));
        kl("Comments", comments.trim());
        kl("Operators1", &self.operators1);
        kl("Operators2", &self.operators2);
        kl("Folders in code1, open", &self.fold_open);
        kl("Folders in code1, middle", &self.fold_middle);
        kl("Folders in code1, close", &self.fold_close);
        for (i, k) in self.keywords.iter().enumerate() {
            kl(&format!("Keywords{}", i + 1), k);
        }
        kl("Delimiters", delims.trim());
        x.push_str("        </KeywordLists>\n        <Styles>\n");
        for n in STYLE_NAMES {
            let s = self.styles.get(n).cloned().unwrap_or_default();
            let fs = s.bold as u8 | (s.italic as u8) << 1 | (s.underline as u8) << 2;
            x.push_str(&format!(
                "            <WordsStyle name=\"{n}\" fgColor=\"{}\" bgColor=\"{}\" colorStyle=\"{}\" fontStyle=\"{fs}\" nesting=\"0\" />\n",
                hex(s.fg),
                hex(s.bg.unwrap_or([255, 255, 255])),
                if s.bg.is_some() { 3 } else { 1 }
            ));
        }
        x.push_str("        </Styles>\n    </UserLang>\n</NotepadPlus>\n");
        x
    }

    pub fn from_xml(xml: &str) -> Result<Vec<UserLang>, String> {
        let mut r = Reader::from_str(xml);
        r.config_mut().trim_text(false);
        let mut out = Vec::new();
        let mut cur: Option<UserLang> = None;
        let mut kw_name: Option<String> = None;
        let mut kw_text = String::new();
        let attr = |e: &BytesStart, n: &str| -> Option<String> {
            e.attributes().flatten().find(|a| a.key.as_ref() == n.as_bytes()).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
        };
        loop {
            match r.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"UserLang" => {
                    let mut l = UserLang { name: attr(&e, "name").unwrap_or_default(), ..Default::default() };
                    l.ext = attr(&e, "ext").unwrap_or_default().split_whitespace().map(str::to_owned).collect();
                    cur = Some(l);
                }
                Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"Global" => {
                    if let Some(l) = &mut cur {
                        l.case_ignored = attr(&e, "caseIgnored").as_deref() == Some("yes");
                    }
                }
                Event::Start(e) if e.name().as_ref() == b"Keywords" => {
                    kw_name = attr(&e, "name");
                    kw_text.clear();
                }
                Event::Empty(e) if e.name().as_ref() == b"Keywords" => {
                    let _ = attr(&e, "name");
                }
                Event::Text(t) if kw_name.is_some() => kw_text.push_str(&t.unescape().map_err(|e| e.to_string())?),
                Event::CData(t) if kw_name.is_some() => kw_text.push_str(&String::from_utf8_lossy(&t)),
                Event::End(e) if e.name().as_ref() == b"Keywords" => {
                    if let (Some(l), Some(n)) = (&mut cur, kw_name.take()) {
                        l.apply_keyword_list(&n, &kw_text);
                    }
                }
                Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"WordsStyle" => {
                    if let Some(l) = &mut cur {
                        let name = attr(&e, "name").unwrap_or_default();
                        let parse = |s: Option<String>| -> Option<[u8; 3]> {
                            let s = s?;
                            let v = u32::from_str_radix(s.trim_start_matches('#'), 16).ok()?;
                            Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
                        };
                        let color_style: u8 = attr(&e, "colorStyle").and_then(|v| v.parse().ok()).unwrap_or(3);
                        let fs: u8 = attr(&e, "fontStyle").and_then(|v| v.parse().ok()).unwrap_or(0);
                        let st = UStyle {
                            fg: parse(attr(&e, "fgColor")).unwrap_or([0, 0, 0]),
                            bg: parse(attr(&e, "bgColor")).filter(|b| color_style & 2 != 0 && *b != [255, 255, 255]),
                            bold: fs & 1 != 0,
                            italic: fs & 2 != 0,
                            underline: fs & 4 != 0,
                        };
                        l.styles.insert(name, st);
                    }
                }
                Event::End(e) if e.name().as_ref() == b"UserLang" => {
                    if let Some(l) = cur.take() {
                        out.push(l);
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if out.is_empty() {
            return Err("No <UserLang> definition found".into());
        }
        Ok(out)
    }

    fn apply_keyword_list(&mut self, name: &str, text: &str) {
        match name {
            "Comments" => {
                for (code, val) in coded_tokens(text) {
                    match code {
                        0 if !val.is_empty() => self.line_comment.push(val),
                        3 if !val.is_empty() => self.block_comment.push((val, String::new())),
                        4 => {
                            if let Some(b) = self.block_comment.iter_mut().find(|b| b.1.is_empty()) {
                                b.1 = val;
                            }
                        }
                        _ => {}
                    }
                }
                self.block_comment.retain(|b| !b.1.is_empty());
            }
            "Delimiters" => {
                for (code, val) in coded_tokens(text) {
                    let (k, part) = (code / 3, code % 3);
                    if k < 8 {
                        let d = &mut self.delimiters[k];
                        let slot = match part {
                            0 => &mut d.open,
                            1 => &mut d.escape,
                            _ => &mut d.close,
                        };
                        if slot.is_empty() {
                            *slot = val;
                        } else if !val.is_empty() {
                            slot.push(' ');
                            slot.push_str(&val);
                        }
                    }
                }
            }
            "Operators1" => self.operators1 = text.trim().to_string(),
            "Operators2" => self.operators2 = text.trim().to_string(),
            "Folders in code1, open" => self.fold_open = text.trim().to_string(),
            "Folders in code1, middle" => self.fold_middle = text.trim().to_string(),
            "Folders in code1, close" => self.fold_close = text.trim().to_string(),
            n if n.starts_with("Keywords") => {
                if let Ok(i) = n[8..].parse::<usize>() {
                    if (1..=8).contains(&i) {
                        self.keywords[i - 1] = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    }
                }
            }
            _ => {}
        }
    }

    pub fn matches_path(&self, path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        self.ext.iter().any(|e| e.eq_ignore_ascii_case(ext) || e.eq_ignore_ascii_case(name))
    }
}

/// "00// 01 03/* 04*/" → [(0,"//"), (1,""), (3,"/*"), (4,"*/")]
fn coded_tokens(text: &str) -> Vec<(usize, String)> {
    text.split_whitespace()
        .filter_map(|t| {
            let code = t.get(..2)?.parse::<usize>().ok()?;
            Some((code, t[2..].to_string()))
        })
        .collect()
}

fn find_close(line: &str, from: usize, d: &Delimiter) -> Option<usize> {
    if d.close.is_empty() {
        // No close token: until the end of the line.
        return None;
    }
    let mut i = from;
    while i < line.len() {
        if !d.escape.is_empty() && line[i..].starts_with(d.escape.as_str()) {
            i += d.escape.len();
            i += line[i..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if line[i..].starts_with(d.close.as_str()) {
            return Some(i + d.close.len());
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

fn is_number(w: &str) -> bool {
    let w = w.strip_prefix('-').unwrap_or(w);
    if let Some(h) = w.strip_prefix("0x").or_else(|| w.strip_prefix("0X")) {
        return !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit());
    }
    w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '_')
}

thread_local! {
    static DARK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Dark backgrounds lighten very dark UDL colours.
pub fn set_dark(dark: bool) {
    DARK.with(|d| d.set(dark));
}

pub fn dir() -> PathBuf {
    crate::settings::config_dir().join("userDefineLangs")
}

/// Loads all user languages from the config directory.
pub fn load_all() -> Vec<UserLang> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir()) {
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "xml")).collect();
        files.sort();
        for f in files {
            if let Ok(s) = std::fs::read_to_string(&f) {
                if let Ok(langs) = UserLang::from_xml(&s) {
                    for mut l in langs {
                        l.file = Some(f.clone());
                        v.push(l);
                    }
                }
            }
        }
    }
    v
}

pub fn save(lang: &mut UserLang) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let safe: String = lang.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let path = lang.file.clone().unwrap_or_else(|| dir().join(format!("{safe}.xml")));
    std::fs::write(&path, lang.to_xml()).map_err(|e| e.to_string())?;
    lang.file = Some(path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<NotepadPlus>
    <UserLang name="Demo" ext="demo dm" udlVersion="2.1">
        <Settings><Global caseIgnored="yes" /></Settings>
        <KeywordLists>
            <Keywords name="Comments">00# 01 02 03(* 04*)</Keywords>
            <Keywords name="Operators1">= + ;</Keywords>
            <Keywords name="Keywords1">IF THEN</Keywords>
            <Keywords name="Keywords2">print</Keywords>
            <Keywords name="Delimiters">00&quot; 01\ 02&quot;</Keywords>
        </KeywordLists>
        <Styles>
            <WordsStyle name="KEYWORDS1" fgColor="0000FF" bgColor="FFFFFF" colorStyle="1" fontStyle="1" />
        </Styles>
    </UserLang>
</NotepadPlus>"#;

    #[test]
    fn parse_and_highlight() {
        let l = &UserLang::from_xml(SAMPLE).unwrap()[0];
        assert_eq!(l.name, "Demo");
        assert_eq!(l.ext, vec!["demo", "dm"]);
        assert_eq!(l.line_comment, vec!["#"]);
        assert_eq!(l.block_comment, vec![("(*".to_string(), "*)".to_string())]);
        assert_eq!(l.delimiters[0].open, "\"");
        assert_eq!(l.delimiters[0].escape, "\\");
        assert!(l.styles["KEYWORDS1"].bold);
        let (spans, st) = l.highlight_line("if x = \"a\\\"b\" (* c", 0);
        assert_eq!(st, 1, "open block comment continues");
        let blue = spans[0].1.foreground;
        assert_eq!((blue.r, blue.g, blue.b), (0, 0, 255));
        let total: usize = spans.iter().map(|s| s.0).sum();
        assert_eq!(total, "if x = \"a\\\"b\" (* c".len());
        let (_, st) = l.highlight_line("still *) print", 1);
        assert_eq!(st, 0);
        // Round trip through XML
        let again = &UserLang::from_xml(&l.to_xml()).unwrap()[0];
        assert_eq!(again.keywords, l.keywords);
        assert_eq!(again.block_comment, l.block_comment);
        assert_eq!(again.delimiters, l.delimiters);
    }
}
