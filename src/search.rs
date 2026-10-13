//! Find/replace engine (modes "Normal", "Erweitert" and "Regulärer Ausdruck" like Notepad++).

use std::ops::Range;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Normal,
    Extended,
    Regex,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Options {
    pub mode: Mode,
    pub match_case: bool,
    pub whole_word: bool,
    pub wrap: bool,
    /// Regex mode: `.` also matches line breaks.
    pub dot_all: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { mode: Mode::Normal, match_case: false, whole_word: false, wrap: true, dot_all: false }
    }
}

/// Resolves `\n \r \t \0 \\ \xHH \uHHHH` (search mode "Erweitert").
pub fn unescape_extended(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            // The text is stored with \n line endings; \r\n in a pattern means a line break.
            Some('r') => {
                if it.peek() == Some(&'\\') {
                    let mut look = it.clone();
                    look.next();
                    if look.peek() == Some(&'n') {
                        it.next();
                        it.next();
                    }
                }
                out.push('\n');
            }
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some(k @ ('x' | 'u')) => {
                let n = if k == 'x' { 2 } else { 4 };
                let hex: String = it.clone().take(n).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).filter(|_| hex.len() == n) {
                    Some(ch) => {
                        out.push(ch);
                        for _ in 0..n {
                            it.next();
                        }
                    }
                    None => {
                        out.push('\\');
                        out.push(k);
                    }
                }
            }
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub fn build_regex(pattern: &str, o: &Options) -> Result<Regex, String> {
    if pattern.is_empty() {
        return Err("Leerer Suchbegriff".into());
    }
    let mut p = match o.mode {
        Mode::Normal => regex::escape(pattern),
        Mode::Extended => regex::escape(&unescape_extended(pattern)),
        // \r\n from Windows habits: the buffer only contains \n.
        Mode::Regex => pattern.replace("\\r\\n", "\\n").replace("\\r", "\\n"),
    };
    if o.whole_word {
        p = format!(r"\b(?:{p})\b");
    }
    RegexBuilder::new(&p)
        .case_insensitive(!o.match_case)
        .multi_line(true)
        .dot_matches_new_line(o.mode == Mode::Regex && o.dot_all)
        .size_limit(50 << 20)
        .build()
        .map_err(|e| e.to_string())
}

/// Replacement string in `regex` syntax for the given mode.
pub fn replacement(repl: &str, mode: Mode) -> String {
    match mode {
        Mode::Normal => repl.replace('$', "$$"),
        Mode::Extended => unescape_extended(repl).replace('$', "$$"),
        Mode::Regex => {
            // Notepad++ (Boost) style: \1 … \9, \0, $1, ${name}, \n, \t, \\.
            let mut out = String::new();
            let mut it = repl.chars().peekable();
            while let Some(c) = it.next() {
                if c != '\\' {
                    out.push(c);
                    continue;
                }
                match it.next() {
                    Some(d @ '0'..='9') => out.push_str(&format!("${{{d}}}")),
                    Some('n') | Some('r') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('\\') => out.push('\\'),
                    Some('$') => out.push_str("$$"),
                    Some(o) => {
                        out.push('\\');
                        out.push(o);
                    }
                    None => out.push('\\'),
                }
            }
            out
        }
    }
}

fn next_char_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(s.len() + 1, |c| i + c.len_utf8())
}

/// Next match at or after `from` (or before `from` when searching backwards).
/// An empty match exactly at `from` is skipped, so repeated searches advance.
pub fn find(text: &str, re: &Regex, from: usize, forward: bool, wrap: bool) -> Option<(Range<usize>, bool)> {
    let from = from.min(text.len());
    if forward {
        let mut start = from;
        loop {
            if start > text.len() {
                break;
            }
            match re.find_at(text, start) {
                Some(m) if m.start() == m.end() && m.start() == from && from != 0 => {
                    start = next_char_boundary(text, from);
                }
                Some(m) => return Some((m.range(), false)),
                None => break,
            }
        }
        if wrap {
            return re.find(text).filter(|m| m.start() < from || from == 0).map(|m| (m.range(), true));
        }
        None
    } else {
        let before = re.find_iter(text).take_while(|m| m.end() <= from).filter(|m| m.start() < from).last();
        if let Some(m) = before {
            return Some((m.range(), false));
        }
        if wrap {
            return re.find_iter(text).filter(|m| m.start() >= from).last().map(|m| (m.range(), true));
        }
        None
    }
}

/// Replaces all matches within `range`; returns the new text and the number of replacements.
pub fn replace_all(text: &str, re: &Regex, repl: &str, range: Range<usize>) -> (String, usize) {
    let hay = &text[range.clone()];
    let count = re.find_iter(hay).count();
    if count == 0 {
        return (text.to_owned(), 0);
    }
    let replaced = re.replace_all(hay, repl);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..range.start]);
    out.push_str(&replaced);
    out.push_str(&text[range.end..]);
    (out, count)
}

/// If `sel` is exactly a match, returns its expanded replacement.
pub fn replace_one(text: &str, re: &Regex, repl: &str, sel: Range<usize>) -> Option<String> {
    let caps = re.captures_at(text, sel.start)?;
    let m = caps.get(0)?;
    if m.range() != sel {
        return None;
    }
    let mut out = String::new();
    caps.expand(repl, &mut out);
    Some(out)
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub line: usize,
    pub line_text: String,
    pub range: Range<usize>,
}

/// All matches with line information (for "Alle finden").
pub fn find_all(text: &str, re: &Regex, limit: usize) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut line = 0;
    let mut scanned = 0;
    for m in re.find_iter(text) {
        line += text.as_bytes()[scanned..m.start()].iter().filter(|&&b| b == b'\n').count();
        scanned = m.start();
        let ls = text[..m.start()].rfind('\n').map_or(0, |i| i + 1);
        let le = text[m.start()..].find('\n').map_or(text.len(), |i| m.start() + i);
        let mut line_text: String = text[ls..le].chars().take(300).collect();
        line_text = line_text.replace('\t', "    ");
        hits.push(Hit { line, line_text, range: m.range() });
        if hits.len() >= limit {
            break;
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(mode: Mode) -> Options {
        Options { mode, ..Default::default() }
    }

    #[test]
    fn normal_and_case() {
        let re = build_regex("a.b", &opts(Mode::Normal)).unwrap();
        assert_eq!(find("xxA.Bx", &re, 0, true, false), Some((2..5, false)));
        let mut o = opts(Mode::Normal);
        o.match_case = true;
        let re = build_regex("a.b", &o).unwrap();
        assert_eq!(find("xxA.Bx", &re, 0, true, false), None);
    }

    #[test]
    fn whole_word_and_wrap() {
        let mut o = opts(Mode::Normal);
        o.whole_word = true;
        let re = build_regex("in", &o).unwrap();
        let t = "int in pin in";
        assert_eq!(find(t, &re, 0, true, false).unwrap().0, 4..6);
        assert_eq!(find(t, &re, 6, true, false).unwrap().0, 11..13);
        assert_eq!(find(t, &re, 13, true, true), Some((4..6, true)));
        assert_eq!(find(t, &re, 11, false, false).unwrap().0, 4..6);
        assert_eq!(find(t, &re, 4, false, true), Some((11..13, true)));
    }

    #[test]
    fn extended() {
        assert_eq!(unescape_extended(r"a\r\nb\tc\x41\\"), "a\nb\tcA\\");
        let re = build_regex(r"a\nb", &opts(Mode::Extended)).unwrap();
        assert!(re.is_match("a\nb"));
    }

    #[test]
    fn regex_replace() {
        let re = build_regex(r"(\w+)=(\d+)", &opts(Mode::Regex)).unwrap();
        let r = replacement(r"\2:\1 $$", Mode::Regex);
        let (out, n) = replace_all("a=1 b=2", &re, &r, 0..7);
        assert_eq!((out.as_str(), n), ("1:a $ 2:b $", 2));
        assert_eq!(replace_one("a=1 b=2", &re, &r, 4..7).as_deref(), Some("2:b $"));
        assert_eq!(replace_one("a=1 b=2", &re, &r, 3..7), None);
        let re = build_regex("x", &opts(Mode::Normal)).unwrap();
        assert_eq!(replace_all("x$x", &re, &replacement("$1", Mode::Normal), 0..3).0, "$1$$1");
    }

    #[test]
    fn empty_matches_advance() {
        let re = build_regex("^", &opts(Mode::Regex)).unwrap();
        let t = "a\nb\nc";
        assert_eq!(find(t, &re, 0, true, false).unwrap().0, 0..0);
        assert_eq!(find(t, &re, 2, true, false).unwrap().0, 4..4);
        let hits = find_all(t, &re, 100);
        assert_eq!(hits.iter().map(|h| h.line).collect::<Vec<_>>(), vec![0, 1, 2]);
    }
}
