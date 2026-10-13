//! Fold structure: which lines start a foldable block and where it ends.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// `{ … }` blocks (C-like languages, JSON, CSS …)
    Braces,
    /// Indentation (Python, YAML …)
    Indent,
    /// Markup tags (XML, HTML)
    Tags,
    /// Markdown headings
    Headings,
    None,
}

pub fn mode_for(syntax: Option<&str>, user_lang: bool) -> Mode {
    if user_lang {
        return Mode::Braces;
    }
    let Some(s) = syntax else { return Mode::None };
    let n = s.to_lowercase();
    if ["python", "yaml", "coffee", "nim", "f#", "haskell", "elm", "sass", "pug", "jade", "makefile", "nix", "restructured", "cabal"]
        .iter()
        .any(|k| n.contains(k))
    {
        Mode::Indent
    } else if ["xml", "html", "svg", "xaml", "plist", "vue"].iter().any(|k| n.contains(k)) {
        Mode::Tags
    } else if n.contains("markdown") {
        Mode::Headings
    } else if ["ini", "plain", "diff", "log", "csv", "properties"].iter().any(|k| n.contains(k)) {
        Mode::None
    } else {
        Mode::Braces
    }
}

/// For each line the last line of the block it opens, or `u32::MAX`.
pub fn compute(text: &str, starts: &[usize], mode: Mode) -> Vec<u32> {
    let n = starts.len();
    let mut ends = vec![u32::MAX; n];
    let line_text = |i: usize| {
        let s = starts[i];
        let e = starts.get(i + 1).map_or(text.len(), |&x| x - 1);
        &text[s..e]
    };
    match mode {
        Mode::None => {}
        Mode::Braces => braces(text, starts, &mut ends),
        Mode::Indent => {
            // Stack of (indent, header line)
            let mut stack: Vec<(usize, usize)> = Vec::new();
            let mut last_non_blank = 0;
            for i in 0..n {
                let l = line_text(i);
                if l.trim().is_empty() {
                    continue;
                }
                let ind = indent_width(l);
                while let Some(&(si, h)) = stack.last() {
                    if ind <= si {
                        stack.pop();
                        if last_non_blank > h {
                            ends[h] = last_non_blank as u32;
                        }
                    } else {
                        break;
                    }
                }
                stack.push((ind, i));
                last_non_blank = i;
            }
            for (_, h) in stack {
                if last_non_blank > h {
                    ends[h] = last_non_blank as u32;
                }
            }
        }
        Mode::Tags => tags(text, starts, &mut ends),
        Mode::Headings => {
            let mut open: Vec<(usize, usize)> = Vec::new(); // (level, line)
            let mut fence = false;
            for i in 0..n {
                let l = line_text(i);
                if l.trim_start().starts_with("```") {
                    fence = !fence;
                }
                if fence {
                    continue;
                }
                let level = l.chars().take_while(|&c| c == '#').count();
                if level > 0 && l[level..].starts_with(' ') {
                    while let Some(&(lv, h)) = open.last() {
                        if lv >= level {
                            open.pop();
                            if i - 1 > h {
                                ends[h] = (i - 1) as u32;
                            }
                        } else {
                            break;
                        }
                    }
                    open.push((level, i));
                }
            }
            let last = n - 1 - (0..n).rev().take_while(|&i| line_text(i).trim().is_empty()).count().min(n - 1);
            for (_, h) in open {
                if last > h {
                    ends[h] = last as u32;
                }
            }
        }
    }
    ends
}

fn indent_width(l: &str) -> usize {
    let mut w = 0;
    for c in l.chars() {
        match c {
            ' ' => w += 1,
            '\t' => w = (w / 4 + 1) * 4,
            _ => break,
        }
    }
    w
}

/// Brace matching that skips strings and comments (C-style heuristics).
fn braces(text: &str, starts: &[usize], ends: &mut [u32]) {
    let b = text.as_bytes();
    let mut stack: Vec<usize> = Vec::new(); // line of `{`
    let mut line = 0;
    let mut i = 0;
    let mut in_block = false;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if in_block {
            if c == b'*' && b.get(i + 1) == Some(&b'/') {
                in_block = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        match c {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                // Line comment: skip to line end.
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                in_block = true;
                i += 2;
                continue;
            }
            b'\'' if !is_char_literal(b, i) => {}
            b'"' | b'\'' | b'`' => {
                // String literal (single line, except backticks).
                let q = c;
                i += 1;
                while i < b.len() && b[i] != q && (b[i] != b'\n' || q == b'`') {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    if i < b.len() && b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }
            b'{' => stack.push(line),
            b'}' => {
                if let Some(open) = stack.pop() {
                    if line > open {
                        let e = ends[open];
                        ends[open] = if e == u32::MAX { line as u32 } else { e.max(line as u32) };
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    let _ = starts;
}

/// `'x'`, `'\n'`, `'\u{41}'` – but not lifetimes (`'a`) or apostrophes in prose.
fn is_char_literal(b: &[u8], i: usize) -> bool {
    if b.get(i + 1) == Some(&b'\\') {
        return b[i + 2..].iter().take(12).take_while(|&&c| c != b'\n').any(|&c| c == b'\'');
    }
    // One (possibly multi-byte) character followed by a quote.
    let rest = &b[(i + 1).min(b.len())..];
    let len = match rest.first() {
        Some(&c) if c < 0x80 => 1,
        Some(&c) if c >= 0xF0 => 4,
        Some(&c) if c >= 0xE0 => 3,
        Some(_) => 2,
        None => return false,
    };
    rest.get(len) == Some(&b'\'')
}

const VOID: [&str; 14] =
    ["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];

fn tags(text: &str, starts: &[usize], ends: &mut [u32]) {
    let b = text.as_bytes();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    let line_of = |pos: usize| starts.partition_point(|&s| s <= pos).saturating_sub(1);
    while let Some(off) = text[i..].find('<') {
        let p = i + off;
        let rest = &text[p..];
        if rest.starts_with("<!--") {
            let e = rest.find("-->").map_or(text.len(), |e| p + e + 3);
            let (l1, l2) = (line_of(p), line_of(e.saturating_sub(1)));
            if l2 > l1 {
                ends[l1] = l2 as u32;
            }
            i = e;
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            i = p + 2;
            continue;
        }
        let close = rest.starts_with("</");
        let name_start = p + if close { 2 } else { 1 };
        let name: String = text[name_start..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
            .collect();
        if name.is_empty() {
            i = p + 1;
            continue;
        }
        let gt = text[p..].find('>').map_or(text.len(), |g| p + g);
        let self_closing = gt > 0 && b.get(gt - 1) == Some(&b'/');
        if close {
            if let Some(pos) = stack.iter().rposition(|(n, _)| n.eq_ignore_ascii_case(&name)) {
                let (_, open_line) = stack[pos].clone();
                stack.truncate(pos);
                let l = line_of(p);
                if l > open_line && ends[open_line] == u32::MAX {
                    ends[open_line] = l as u32;
                }
            }
        } else if !self_closing && !VOID.contains(&name.to_lowercase().as_str()) {
            stack.push((name, line_of(p)));
        }
        i = gt.max(p + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(t: &str, m: Mode) -> Vec<Option<u32>> {
        let starts: Vec<usize> = std::iter::once(0).chain(t.match_indices('\n').map(|(i, _)| i + 1)).collect();
        compute(t, &starts, m).into_iter().map(|e| (e != u32::MAX).then_some(e)).collect()
    }

    #[test]
    fn brace_folds() {
        let t = "fn a(x: &'static str) {\n  let s = \"{\"; let c = '{';\n  if x {\n  } else {\n  }\n}\n";
        let r = run(t, Mode::Braces);
        assert_eq!(r[0], Some(5));
        assert_eq!(r[2], Some(3));
        assert_eq!(r[3], Some(4));
        assert_eq!(r[1], None);
    }

    #[test]
    fn indent_folds() {
        let t = "def a():\n    x = 1\n\n    y = 2\nb = 3\n";
        let r = run(t, Mode::Indent);
        assert_eq!(r[0], Some(3));
        assert_eq!(r[4], None);
    }

    #[test]
    fn tag_and_heading_folds() {
        let t = "<a>\n  <b/>\n  <c>\n  </c>\n  <br>\n</a>\n";
        let r = run(t, Mode::Tags);
        assert_eq!(r[0], Some(5));
        assert_eq!(r[2], Some(3));
        let t = "# A\ntext\n## B\nmore\n# C\nend\n";
        let r = run(t, Mode::Headings);
        assert_eq!(r[0], Some(3));
        assert_eq!(r[2], Some(3));
        assert_eq!(r[4], Some(5));
    }
}
