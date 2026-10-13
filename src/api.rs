//! Function parameter hints ("call tips").
//!
//! Signatures come from Notepad++ API files (`~/.config/notepad-rs/autoCompletion/<lang>.xml`,
//! same format as Notepad++'s `autoCompletion` folder) and from function definitions found in the
//! current document.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// One overload of a function.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Overload {
    pub ret: String,
    pub params: Vec<String>,
    pub descr: String,
}

/// API of one language.
#[derive(Clone, Debug, Default)]
pub struct Api {
    pub language: String,
    pub ignore_case: bool,
    pub start: char,
    pub stop: char,
    pub sep: char,
    /// Function name → overloads (keys lower-cased when `ignore_case`).
    pub funcs: HashMap<String, Vec<Overload>>,
    /// All keywords (functions and plain words) for completion.
    pub words: Vec<String>,
}

impl Api {
    pub fn lookup(&self, name: &str) -> Option<&Vec<Overload>> {
        if self.ignore_case {
            self.funcs.get(&name.to_lowercase())
        } else {
            self.funcs.get(name)
        }
    }

    pub fn from_xml(xml: &str) -> Result<Api, String> {
        let mut r = Reader::from_str(xml);
        let mut api = Api { start: '(', stop: ')', sep: ',', ..Default::default() };
        let attr = |e: &BytesStart, n: &str| -> Option<String> {
            e.attributes().flatten().find(|a| a.key.as_ref() == n.as_bytes()).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
        };
        let first = |s: Option<String>, d: char| s.and_then(|s| s.chars().next()).unwrap_or(d);
        let mut kw: Option<(String, bool)> = None;
        let mut overloads: Vec<Overload> = Vec::new();
        let mut cur: Option<Overload> = None;
        loop {
            let ev = r.read_event().map_err(|e| e.to_string())?;
            let empty = matches!(ev, Event::Empty(_));
            match ev {
                Event::Start(e) | Event::Empty(e) => match e.name().as_ref() {
                    b"AutoComplete" => api.language = attr(&e, "language").unwrap_or_default(),
                    b"Environment" => {
                        api.ignore_case = attr(&e, "ignoreCase").as_deref() == Some("yes");
                        api.start = first(attr(&e, "startFunc"), '(');
                        api.stop = first(attr(&e, "stopFunc"), ')');
                        api.sep = first(attr(&e, "paramSeparator"), ',');
                    }
                    b"KeyWord" => {
                        let name = attr(&e, "name").unwrap_or_default();
                        let func = attr(&e, "func").as_deref() == Some("yes");
                        if !name.is_empty() {
                            api.words.push(name.clone());
                        }
                        if empty {
                            if func {
                                api.insert(name, vec![Overload::default()]);
                            }
                        } else {
                            kw = Some((name, func));
                            overloads.clear();
                        }
                    }
                    b"Overload" => {
                        let o = Overload { ret: attr(&e, "retVal").unwrap_or_default(), params: Vec::new(), descr: attr(&e, "descr").unwrap_or_default() };
                        if empty {
                            overloads.push(o);
                        } else {
                            cur = Some(o);
                        }
                    }
                    b"Param" => {
                        if let Some(o) = &mut cur {
                            o.params.push(attr(&e, "name").unwrap_or_default());
                        }
                    }
                    _ => {}
                },
                Event::End(e) => match e.name().as_ref() {
                    b"Overload" => overloads.extend(cur.take()),
                    b"KeyWord" => {
                        if let Some((name, func)) = kw.take() {
                            if func || !overloads.is_empty() {
                                let o = std::mem::take(&mut overloads);
                                api.insert(name, if o.is_empty() { vec![Overload::default()] } else { o });
                            }
                        }
                    }
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(api)
    }

    fn insert(&mut self, name: String, o: Vec<Overload>) {
        let key = if self.ignore_case { name.to_lowercase() } else { name };
        self.funcs.entry(key).or_default().extend(o);
    }
}

pub fn dir() -> PathBuf {
    crate::settings::config_dir().join("autoCompletion")
}

fn norm(s: &str) -> String {
    let s = s.to_lowercase();
    match s.as_str() {
        "c++" => "cpp".into(),
        "c#" => "cs".into(),
        "bourne again shell (bash)" | "shell script (bash)" | "bash" => "bash".into(),
        "javascript (babel)" | "js" => "javascript".into(),
        _ => s.chars().filter(|c| c.is_alphanumeric()).collect(),
    }
}

/// All API files, keyed by normalized language name (file stem and `language` attribute).
fn all() -> &'static HashMap<String, Api> {
    static APIS: OnceLock<HashMap<String, Api>> = OnceLock::new();
    APIS.get_or_init(|| {
        let mut m = HashMap::new();
        let Ok(rd) = std::fs::read_dir(dir()) else { return m };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "xml") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let Ok(api) = Api::from_xml(&text) else { continue };
            let stem = p.file_stem().map(|s| norm(&s.to_string_lossy())).unwrap_or_default();
            if !api.language.is_empty() {
                m.insert(norm(&api.language), api.clone());
            }
            m.insert(stem, api);
        }
        m
    })
}

/// API for a syntax (or user defined language) name.
pub fn for_lang(name: &str) -> Option<&'static Api> {
    all().get(&norm(name))
}

/// Signatures defined in the document itself: `fn f(a, b)`, `def f(a)`, `function f(x)`,
/// `int f(int a) {`.
pub fn from_text(text: &str, name: &str) -> Vec<Overload> {
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Vec::new();
    }
    let n = regex::escape(name);
    let pat = format!(
        r"(?m)(?:\b(?:fn|def|function|func|sub|proc|procedure|fun)\s+{n}\s*(?:<[^>()]*>)?\s*\(([^)]*)\)|^[ \t]*(?:[\w:<>,\*&\[\]]+[ \t]+[\*&]*)+{n}[ \t]*\(([^;{{)]*)\)[ \t]*(?:const[ \t]*)?(?:\{{|$))"
    );
    let Ok(re) = regex::Regex::new(&pat) else { return Vec::new() };
    let mut out: Vec<Overload> = Vec::new();
    for c in re.captures_iter(text).take(20) {
        let params = c.get(1).or(c.get(2)).map_or("", |m| m.as_str());
        let params: Vec<String> = params.split(',').map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect();
        let o = Overload { params, ..Default::default() };
        if !out.contains(&o) {
            out.push(o);
        }
    }
    out
}

/// Active call tip.
#[derive(Clone, Debug)]
pub struct CallTip {
    pub doc_id: u64,
    pub name: String,
    /// Byte position just after the opening parenthesis.
    pub open: usize,
    pub overloads: Vec<Overload>,
    pub current: usize,
    pub stop: char,
    pub sep: char,
}

/// Finds the innermost unclosed `(` before `pos` on the current line(s) (up to 2000 bytes back)
/// and the function name in front of it. Returns (name, position after the parenthesis).
pub fn enclosing_call(text: &str, pos: usize, start: char, stop: char) -> Option<(String, usize)> {
    let mut from = pos.saturating_sub(2000);
    while !text.is_char_boundary(from) {
        from -= 1;
    }
    let mut depth = 0i32;
    for (i, ch) in text[from..pos].char_indices().rev() {
        if ch == stop {
            depth += 1;
        } else if ch == start {
            if depth == 0 {
                let open = from + i;
                let name = word_before(text, open);
                return (!name.is_empty()).then(|| (name, open + ch.len_utf8()));
            }
            depth -= 1;
        } else if ch == ';' || ch == '{' || ch == '}' {
            return None;
        }
    }
    None
}

pub fn word_before(text: &str, pos: usize) -> String {
    let before = text[..pos].trim_end_matches([' ', '\t']);
    let start = before.char_indices().rev().take_while(|(_, c)| c.is_alphanumeric() || *c == '_').last().map_or(before.len(), |(i, _)| i);
    before[start..].to_string()
}

/// Index of the parameter the caret is in (separators at nesting depth 0 between `open` and `pos`),
/// or `None` when the call has been closed.
pub fn param_index(text: &str, open: usize, pos: usize, tip: &CallTip) -> Option<usize> {
    if pos < open || pos > text.len() {
        return None;
    }
    let mut depth = 0i32;
    let mut idx = 0;
    let mut quote: Option<char> = None;
    for ch in text[open..pos].chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            c if c == tip.stop || c == ')' || c == ']' || c == '}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            c if c == tip.sep && depth == 0 => idx += 1,
            _ => {}
        }
    }
    Some(idx)
}

impl Overload {
    /// Signature split into (text before current param, current param, rest).
    pub fn render(&self, name: &str, param: usize, sep: char) -> (String, String, String) {
        let mut parts = [String::new(), String::new(), String::new()];
        if !self.ret.is_empty() {
            parts[0] = format!("{} ", self.ret);
        }
        parts[0].push_str(name);
        parts[0].push('(');
        for (i, p) in self.params.iter().enumerate() {
            let k = match i.cmp(&param) {
                std::cmp::Ordering::Less => 0,
                std::cmp::Ordering::Equal => 1,
                std::cmp::Ordering::Greater => 2,
            };
            if i > 0 {
                // The separator belongs to the text before the parameter.
                parts[if k == 1 { 0 } else { k }].push_str(&format!("{sep} "));
            }
            parts[k].push_str(p);
        }
        parts[2].push(')');
        let [a, b, c] = parts;
        (a, b, c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
  <AutoComplete language="PHP">
    <Environment ignoreCase="yes" startFunc="(" stopFunc=")" paramSeparator="," terminal=";" />
    <KeyWord name="abs" func="yes">
      <Overload retVal="number" descr="Absolute value"><Param name="mixed number" /></Overload>
    </KeyWord>
    <KeyWord name="str_replace" func="yes">
      <Overload retVal="mixed"><Param name="search" /><Param name="replace" /><Param name="subject" /></Overload>
      <Overload retVal="mixed"><Param name="search" /><Param name="replace" /><Param name="subject" /><Param name="&amp;count" /></Overload>
    </KeyWord>
    <KeyWord name="echo" />
  </AutoComplete>
</NotepadPlus>"#;

    #[test]
    fn parses_api_xml() {
        let api = Api::from_xml(XML).unwrap();
        assert_eq!(api.language, "PHP");
        assert!(api.ignore_case);
        assert_eq!(api.words, vec!["abs", "str_replace", "echo"]);
        assert_eq!(api.lookup("STR_REPLACE").unwrap().len(), 2);
        assert_eq!(api.lookup("str_replace").unwrap()[1].params[3], "&count");
        assert!(api.lookup("echo").is_none());
        let (a, b, c) = api.lookup("abs").unwrap()[0].render("abs", 0, ',');
        assert_eq!((a.as_str(), b.as_str(), c.as_str()), ("number abs(", "mixed number", ")"));
        let o = &api.lookup("str_replace").unwrap()[0];
        let (a, b, c) = o.render("str_replace", 1, ',');
        assert_eq!(format!("{a}[{b}]{c}"), "mixed str_replace(search, [replace], subject)");
    }

    #[test]
    fn document_signatures_and_param_index() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }\ndef greet(name, greeting='hi'):\nint mul(int x, int y) {\n";
        assert_eq!(from_text(src, "add")[0].params, vec!["a: i32", "b: i32"]);
        assert_eq!(from_text(src, "greet")[0].params, vec!["name", "greeting='hi'"]);
        assert_eq!(from_text(src, "mul")[0].params, vec!["int x", "int y"]);
        assert!(from_text(src, "nothing").is_empty());

        let text = "x = add(1, f(2, 3), ";
        let (name, open) = enclosing_call(text, text.len(), '(', ')').unwrap();
        assert_eq!(name, "add");
        let tip = CallTip { doc_id: 0, name, open, overloads: vec![], current: 0, stop: ')', sep: ',' };
        assert_eq!(param_index(text, open, text.len(), &tip), Some(2));
        let closed = "x = add(1, 2)";
        assert_eq!(param_index(closed, 8, closed.len(), &tip), None);
        assert_eq!(param_index("add(\"a,b\", ", 4, 11, &tip), Some(1));
    }
}
