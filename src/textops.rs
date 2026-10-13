//! Text transformations from the "Bearbeiten" menu. All work on byte ranges and return
//! the new text plus the new selection (bytes, anchor..cursor order not preserved).

use std::ops::Range;

pub type Out = Option<(String, Range<usize>)>;

/// Byte range of the full lines touched by `sel` (without the final newline).
/// A selection ending at column 0 does not include that line.
pub fn line_block(text: &str, sel: &Range<usize>) -> Range<usize> {
    let mut end = sel.end;
    if end > sel.start && text[..end].ends_with('\n') {
        end -= 1;
    }
    let start = text[..sel.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[end..].find('\n').map_or(text.len(), |i| end + i);
    start..end.max(start)
}

/// Lines of the selection, or of the whole text when nothing is selected.
fn block_or_all(text: &str, sel: &Range<usize>) -> Range<usize> {
    if sel.is_empty() {
        0..text.len()
    } else {
        line_block(text, sel)
    }
}

fn splice(text: &str, r: Range<usize>, with: &str) -> String {
    let mut s = String::with_capacity(text.len() + with.len());
    s.push_str(&text[..r.start]);
    s.push_str(with);
    s.push_str(&text[r.end..]);
    s
}

/// Applies `f` to the lines of the block; keeps a trailing newline of the whole text intact.
fn map_lines(text: &str, block: Range<usize>, f: impl FnOnce(Vec<&str>) -> Vec<String>) -> Out {
    let mut body = &text[block.clone()];
    let mut block = block;
    if body.ends_with('\n') {
        body = &body[..body.len() - 1];
        block.end -= 1;
    }
    let lines: Vec<&str> = body.split('\n').collect();
    let new = f(lines).join("\n");
    if new == body {
        return None;
    }
    let out = splice(text, block.clone(), &new);
    Some((out, block.start..block.start + new.len()))
}

pub fn duplicate(text: &str, sel: &Range<usize>) -> Out {
    if !sel.is_empty() {
        let s = &text[sel.clone()];
        let out = splice(text, sel.end..sel.end, s);
        return Some((out, sel.end..sel.end + s.len()));
    }
    let b = line_block(text, sel);
    let line = &text[b.clone()];
    let out = splice(text, b.end..b.end, &format!("\n{line}"));
    let cur = sel.start + line.len() + 1;
    Some((out, cur..cur))
}

pub fn delete_lines(text: &str, sel: &Range<usize>) -> Out {
    let b = line_block(text, sel);
    let (start, end) = if b.end < text.len() {
        (b.start, b.end + 1)
    } else if b.start > 0 {
        (b.start - 1, b.end)
    } else {
        (b.start, b.end)
    };
    if start == end {
        return None;
    }
    let out = splice(text, start..end, "");
    let cur = if b.end < text.len() { b.start } else { out[..start].rfind('\n').map_or(0, |i| i + 1) };
    Some((out, cur..cur))
}

pub fn move_lines(text: &str, sel: &Range<usize>, up: bool) -> Out {
    let b = line_block(text, sel);
    if up {
        if b.start == 0 {
            return None;
        }
        let prev_start = text[..b.start - 1].rfind('\n').map_or(0, |i| i + 1);
        let prev = &text[prev_start..b.start - 1];
        let block = &text[b.clone()];
        let out = splice(text, prev_start..b.end, &format!("{block}\n{prev}"));
        let shift = prev.len() + 1;
        Some((out, sel.start - shift..sel.end - shift))
    } else {
        if b.end >= text.len() {
            return None;
        }
        let next_end = text[b.end + 1..].find('\n').map_or(text.len(), |i| b.end + 1 + i);
        let next = &text[b.end + 1..next_end];
        let block = &text[b.clone()];
        let out = splice(text, b.start..next_end, &format!("{next}\n{block}"));
        let shift = next.len() + 1;
        Some((out, sel.start + shift..sel.end + shift))
    }
}

/// Ctrl+Q: comments the lines out, or removes the comment if all lines are commented.
pub fn toggle_line_comment(text: &str, sel: &Range<usize>, token: &str) -> Out {
    let b = line_block(text, sel);
    let tok = token.trim_end();
    map_lines(text, b, |lines| {
        let all_commented = lines.iter().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with(tok));
        let indent = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);
        lines
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    l.to_string()
                } else if all_commented {
                    let ws = l.len() - l.trim_start().len();
                    let rest = &l[ws + tok.len()..];
                    let rest = rest.strip_prefix(' ').filter(|_| token.ends_with(' ') || tok.len() <= 3).unwrap_or(rest);
                    format!("{}{}", &l[..ws], rest)
                } else {
                    let sep = if token.ends_with(' ') { "" } else { " " };
                    format!("{}{token}{sep}{}", &l[..indent], &l[indent..])
                }
            })
            .collect()
    })
}

/// Block comment around the selection (or the current line); removes it if already present.
pub fn toggle_block_comment(text: &str, sel: &Range<usize>, open: &str, close: &str) -> Out {
    let r = if sel.is_empty() { line_block(text, sel) } else { sel.clone() };
    let s = &text[r.clone()];
    let t = s.trim();
    if t.starts_with(open) && t.ends_with(close) && t.len() >= open.len() + close.len() {
        let inner = t[open.len()..t.len() - close.len()].trim_matches(' ');
        let lead = &s[..s.len() - s.trim_start().len()];
        let new = format!("{lead}{inner}");
        return Some((splice(text, r.clone(), &new), r.start..r.start + new.len()));
    }
    let new = format!("{open} {s} {close}");
    Some((splice(text, r.clone(), &new), r.start..r.start + new.len()))
}

pub fn indent(text: &str, sel: &Range<usize>, unit: &str) -> Out {
    let b = line_block(text, sel);
    let r = map_lines(text, b, |lines| {
        lines.iter().map(|l| if l.is_empty() { String::new() } else { format!("{unit}{l}") }).collect()
    });
    r
}

pub fn unindent(text: &str, sel: &Range<usize>, width: usize) -> Out {
    let b = line_block(text, sel);
    map_lines(text, b, |lines| {
        lines
            .iter()
            .map(|l| {
                if let Some(rest) = l.strip_prefix('\t') {
                    return rest.to_string();
                }
                let n = l.bytes().take(width).take_while(|&c| c == b' ').count();
                l[n..].to_string()
            })
            .collect()
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Case {
    Upper,
    Lower,
    /// Each word capitalized, rest lowercase
    Proper,
    /// Each word capitalized, rest unchanged
    ProperBlend,
    Sentence,
    SentenceBlend,
    Invert,
    Random,
}

fn case_str(s: &str, case: Case) -> String {
    let up = |c: char, out: &mut String| out.extend(c.to_uppercase());
    let low = |c: char, out: &mut String| out.extend(c.to_lowercase());
    let mut out = String::with_capacity(s.len());
    match case {
        Case::Upper => return s.to_uppercase(),
        Case::Lower => return s.to_lowercase(),
        Case::Invert => {
            for c in s.chars() {
                if c.is_uppercase() {
                    low(c, &mut out)
                } else {
                    up(c, &mut out)
                }
            }
        }
        Case::Random => {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            for c in s.chars() {
                if rng.gen_bool(0.5) {
                    up(c, &mut out)
                } else {
                    low(c, &mut out)
                }
            }
        }
        Case::Proper | Case::ProperBlend => {
            let mut start = true;
            for c in s.chars() {
                if start && c.is_alphabetic() {
                    up(c, &mut out);
                } else if case == Case::Proper {
                    low(c, &mut out);
                } else {
                    out.push(c);
                }
                start = !c.is_alphanumeric() && c != '\'';
            }
        }
        Case::Sentence | Case::SentenceBlend => {
            let mut start = true;
            for c in s.chars() {
                if start && c.is_alphabetic() {
                    up(c, &mut out);
                    start = false;
                } else if case == Case::Sentence {
                    low(c, &mut out);
                } else {
                    out.push(c);
                }
                if matches!(c, '.' | '!' | '?' | '\n') {
                    start = true;
                }
            }
        }
    }
    out
}

pub fn change_case(text: &str, sel: &Range<usize>, case: Case) -> Out {
    if sel.is_empty() {
        return None;
    }
    let new = case_str(&text[sel.clone()], case);
    Some((splice(text, sel.clone(), &new), sel.start..sel.start + new.len()))
}

/// Case conversion for several selections (multi-selection / column mode).
pub fn case_multi(text: &str, sels: &[Range<usize>], case: Case) -> Vec<(Range<usize>, String)> {
    sels.iter().filter(|r| !r.is_empty()).map(|r| (r.clone(), case_str(&text[r.clone()], case))).collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineOp {
    SortAsc,
    SortDesc,
    SortAscNoCase,
    SortDescNoCase,
    SortLocaleAsc,
    SortLocaleDesc,
    SortLengthAsc,
    SortLengthDesc,
    SortIntAsc,
    SortIntDesc,
    SortDecimalCommaAsc,
    SortDecimalCommaDesc,
    SortDecimalDotAsc,
    SortDecimalDotDesc,
    Reverse,
    Randomize,
    RemoveDuplicates,
    RemoveConsecutiveDuplicates,
    RemoveEmpty,
    RemoveBlank,
    TrimTrailing,
    TrimLeading,
    TrimBoth,
    EolToSpace,
    TrimBothEolToSpace,
    Join,
}

/// Collation key approximating locale order: case and accents ignored first.
fn locale_key(s: &str) -> (String, String) {
    let base: String = s
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            'ß' => 's',
            c => c,
        })
        .collect();
    (base, s.to_string())
}

fn number_key(s: &str, decimal: Option<char>) -> Option<f64> {
    let t = s.trim();
    let tok: String = t
        .chars()
        .enumerate()
        .take_while(|(i, c)| c.is_ascii_digit() || (*i == 0 && (*c == '-' || *c == '+')) || Some(*c) == decimal)
        .map(|(_, c)| c)
        .collect();
    let tok = if decimal == Some(',') { tok.replace(',', ".") } else { tok };
    tok.parse::<f64>().ok()
}

pub fn line_op(text: &str, sel: &Range<usize>, op: LineOp, tab_width: usize) -> Out {
    let b = block_or_all(text, sel);
    let _ = tab_width;
    map_lines(text, b, |mut lines| {
        let by_num = |lines: &mut Vec<&str>, dec: Option<char>, desc: bool| {
            // Lines without a number keep their relative order at the end (like Notepad++).
            lines.sort_by(|a, b| {
                let o = match (number_key(a, dec), number_key(b, dec)) {
                    (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                };
                if desc {
                    o.reverse()
                } else {
                    o
                }
            });
        };
        match op {
            LineOp::SortAsc => lines.sort(),
            LineOp::SortDesc => lines.sort_by(|a, b| b.cmp(a)),
            LineOp::SortAscNoCase => lines.sort_by_cached_key(|l| l.to_lowercase()),
            LineOp::SortDescNoCase => {
                lines.sort_by_cached_key(|l| l.to_lowercase());
                lines.reverse();
            }
            LineOp::SortLocaleAsc => lines.sort_by_cached_key(|l| locale_key(l)),
            LineOp::SortLocaleDesc => {
                lines.sort_by_cached_key(|l| locale_key(l));
                lines.reverse();
            }
            LineOp::SortLengthAsc => lines.sort_by_key(|l| l.chars().count()),
            LineOp::SortLengthDesc => lines.sort_by_key(|l| std::cmp::Reverse(l.chars().count())),
            LineOp::SortIntAsc => by_num(&mut lines, None, false),
            LineOp::SortIntDesc => by_num(&mut lines, None, true),
            LineOp::SortDecimalCommaAsc => by_num(&mut lines, Some(','), false),
            LineOp::SortDecimalCommaDesc => by_num(&mut lines, Some(','), true),
            LineOp::SortDecimalDotAsc => by_num(&mut lines, Some('.'), false),
            LineOp::SortDecimalDotDesc => by_num(&mut lines, Some('.'), true),
            LineOp::Reverse => lines.reverse(),
            LineOp::Randomize => {
                use rand::seq::SliceRandom;
                lines.shuffle(&mut rand::thread_rng());
            }
            LineOp::RemoveDuplicates => {
                let mut seen = std::collections::HashSet::new();
                lines.retain(|l| seen.insert(*l));
            }
            LineOp::RemoveConsecutiveDuplicates => lines.dedup(),
            LineOp::RemoveEmpty => lines.retain(|l| !l.is_empty()),
            LineOp::RemoveBlank => lines.retain(|l| !l.trim().is_empty()),
            LineOp::TrimTrailing => return lines.iter().map(|l| l.trim_end().to_string()).collect(),
            LineOp::TrimLeading => return lines.iter().map(|l| l.trim_start().to_string()).collect(),
            LineOp::TrimBoth => return lines.iter().map(|l| l.trim().to_string()).collect(),
            LineOp::EolToSpace => return vec![lines.join(" ")],
            LineOp::TrimBothEolToSpace => {
                return vec![lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")]
            }
            LineOp::Join => {
                let joined = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
                return vec![joined];
            }
        }
        lines.into_iter().map(str::to_string).collect()
    })
}

/// Splits the selected lines at word boundaries so no line exceeds `width` characters.
pub fn split_lines(text: &str, sel: &Range<usize>, width: usize) -> Out {
    let b = block_or_all(text, sel);
    let width = width.max(1);
    map_lines(text, b, |lines| {
        let mut out = Vec::new();
        for l in lines {
            let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
            let mut cur = String::new();
            for w in l.split_whitespace() {
                if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > width {
                    out.push(std::mem::take(&mut cur));
                }
                if cur.is_empty() {
                    cur.push_str(&indent);
                    cur.push_str(w);
                } else {
                    cur.push(' ');
                    cur.push_str(w);
                }
            }
            out.push(cur);
        }
        out
    })
}

/// Number format for the column editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumFormat {
    Dec,
    Hex,
    Oct,
    Bin,
}

/// Column editor "number to insert": value for line `i`.
pub fn column_number(i: usize, initial: i64, step: i64, repeat: usize, fmt: NumFormat, width: usize, leading: char) -> String {
    let n = initial + step * (i / repeat.max(1)) as i64;
    let neg = n < 0;
    let a = n.unsigned_abs();
    let mut s = match fmt {
        NumFormat::Dec => a.to_string(),
        NumFormat::Hex => format!("{a:X}"),
        NumFormat::Oct => format!("{a:o}"),
        NumFormat::Bin => format!("{a:b}"),
    };
    if neg {
        s.insert(0, '-');
    }
    let pad = width.saturating_sub(s.chars().count());
    format!("{}{s}", leading.to_string().repeat(pad))
}

/// Inserts `item(i)` at char column `col` of every line in `lines`, padding short lines with spaces.
pub fn column_insert(text: &str, lines: Range<usize>, col: usize, item: impl Fn(usize) -> String) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    for (n, l) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        if lines.contains(&n) {
            let chars = l.chars().count();
            let s = item(n - lines.start);
            if chars < col {
                out.push_str(l);
                out.extend(std::iter::repeat_n(' ', col - chars));
                out.push_str(&s);
            } else {
                let b = l.char_indices().nth(col).map_or(l.len(), |(i, _)| i);
                out.push_str(&l[..b]);
                out.push_str(&s);
                out.push_str(&l[b..]);
            }
        } else {
            out.push_str(l);
        }
    }
    (out != text).then_some(out)
}

/// Replaces every non-whitespace character with █ (or ● with `dot`).
pub fn redact(s: &str, dot: bool) -> String {
    s.chars().map(|c| if c.is_whitespace() { c } else if dot { '●' } else { '█' }).collect()
}

/// Range between the innermost brackets around `pos` (without the brackets).
pub fn between_braces(text: &str, pos: usize) -> Option<Range<usize>> {
    let b = text.as_bytes();
    let mut depth: [i32; 3] = [0; 3];
    let opens = *b"([{";
    let closes = *b")]}";
    let mut i = pos;
    while i > 0 {
        i -= 1;
        if let Some(k) = closes.iter().position(|&c| c == b[i]) {
            depth[k] += 1;
        } else if let Some(k) = opens.iter().position(|&c| c == b[i]) {
            if depth[k] == 0 {
                let (_, close) = matching_bracket(text, i + 1)?;
                return (close >= pos).then_some(i + 1..close);
            }
            depth[k] -= 1;
        }
    }
    None
}

/// Tabs → spaces (all) or leading spaces → tabs.
pub fn convert_tabs(text: &str, sel: &Range<usize>, to_spaces: bool, width: usize) -> Out {
    let b = block_or_all(text, sel);
    let width = width.max(1);
    map_lines(text, b, |lines| {
        lines
            .iter()
            .map(|l| {
                if to_spaces {
                    let mut out = String::new();
                    let mut col = 0;
                    for c in l.chars() {
                        if c == '\t' {
                            let n = width - col % width;
                            out.extend(std::iter::repeat_n(' ', n));
                            col += n;
                        } else {
                            out.push(c);
                            col += 1;
                        }
                    }
                    out
                } else {
                    let ws_len = l.len() - l.trim_start_matches([' ', '\t']).len();
                    let mut col = 0;
                    for c in l[..ws_len].chars() {
                        col = if c == '\t' { (col / width + 1) * width } else { col + 1 };
                    }
                    format!("{}{}{}", "\t".repeat(col / width), " ".repeat(col % width), &l[ws_len..])
                }
            })
            .collect()
    })
}

/// Inserts an empty line above/below the current line, cursor on it (keeps indentation).
pub fn insert_blank_line(text: &str, sel: &Range<usize>, above: bool) -> Out {
    let b = line_block(text, &(sel.start..sel.start));
    let line = &text[b.clone()];
    let ind = &line[..line.len() - line.trim_start().len()];
    if above {
        let out = splice(text, b.start..b.start, &format!("{ind}\n"));
        let c = b.start + ind.len();
        Some((out, c..c))
    } else {
        let out = splice(text, b.end..b.end, &format!("\n{ind}"));
        let c = b.end + 1 + ind.len();
        Some((out, c..c))
    }
}

/// Position of the bracket matching the one at/before `pos`, as (bracket, match) byte offsets.
pub fn matching_bracket(text: &str, pos: usize) -> Option<(usize, usize)> {
    const PAIRS: [(u8, u8); 3] = [(b'(', b')'), (b'[', b']'), (b'{', b'}')];
    let b = text.as_bytes();
    let at = |i: usize| b.get(i).copied();
    let cands = [pos.checked_sub(1), Some(pos)];
    for i in cands.into_iter().flatten() {
        let Some(c) = at(i) else { continue };
        for (open, close) in PAIRS {
            if c == open {
                let mut depth = 0usize;
                for (j, &x) in b.iter().enumerate().skip(i).take(4_000_000) {
                    if x == open {
                        depth += 1;
                    } else if x == close {
                        depth -= 1;
                        if depth == 0 {
                            return Some((i, j));
                        }
                    }
                }
                return None;
            }
            if c == close {
                let mut depth = 0usize;
                for j in (i.saturating_sub(4_000_000)..=i).rev() {
                    if b[j] == close {
                        depth += 1;
                    } else if b[j] == open {
                        depth -= 1;
                        if depth == 0 {
                            return Some((i, j));
                        }
                    }
                }
                return None;
            }
        }
    }
    None
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block() {
        let t = "aa\nbb\ncc";
        assert_eq!(line_block(t, &(4..4)), 3..5);
        assert_eq!(line_block(t, &(1..6)), 0..5);
        assert_eq!(line_block(t, &(1..7)), 0..8);
    }

    #[test]
    fn dup_delete_move() {
        let t = "aa\nbb\ncc";
        assert_eq!(duplicate(t, &(4..4)).unwrap().0, "aa\nbb\nbb\ncc");
        assert_eq!(delete_lines(t, &(4..4)).unwrap().0, "aa\ncc");
        assert_eq!(delete_lines(t, &(7..7)).unwrap().0, "aa\nbb");
        assert_eq!(delete_lines("x", &(0..0)).unwrap().0, "");
        let (m, s) = move_lines(t, &(4..4), true).unwrap();
        assert_eq!((m.as_str(), s), ("bb\naa\ncc", 1..1));
        let (m, s) = move_lines(t, &(4..4), false).unwrap();
        assert_eq!((m.as_str(), s), ("aa\ncc\nbb", 7..7));
        assert!(move_lines(t, &(0..0), true).is_none());
    }

    #[test]
    fn comments() {
        let t = "  a\n  b\n";
        let (c, _) = toggle_line_comment(t, &(0..6), "//").unwrap();
        assert_eq!(c, "  // a\n  // b\n");
        let (u, _) = toggle_line_comment(&c, &(0..12), "//").unwrap();
        assert_eq!(u, t);
        let (b, _) = toggle_block_comment("x", &(0..1), "<!--", "-->").unwrap();
        assert_eq!(b, "<!-- x -->");
        assert_eq!(toggle_block_comment(&b, &(0..b.len()), "<!--", "-->").unwrap().0, "x");
    }

    #[test]
    fn line_ops() {
        let t = "b\na\nb\n\nc  \n";
        assert_eq!(line_op(t, &(0..0), LineOp::RemoveDuplicates, 4).unwrap().0, "b\na\n\nc  \n");
        assert_eq!(line_op(t, &(0..0), LineOp::TrimTrailing, 4).unwrap().0, "b\na\nb\n\nc\n");
        assert_eq!(line_op("3\n10\n2", &(0..0), LineOp::SortIntAsc, 4).unwrap().0, "2\n3\n10");
        assert_eq!(line_op("a\n b \nc", &(0..0), LineOp::Join, 4).unwrap().0, "a b c");
        assert_eq!(convert_tabs("\tx\ty", &(0..0), true, 4).unwrap().0, "    x   y");
        assert_eq!(convert_tabs("      x", &(0..0), false, 4).unwrap().0, "\t  x");
        assert_eq!(indent("a\nb", &(0..3), "\t").unwrap().0, "\ta\n\tb");
        assert_eq!(unindent("\ta\n  b", &(0..6), 4).unwrap().0, "a\nb");
    }

    #[test]
    fn case_and_brackets() {
        assert_eq!(change_case("hallo wELT", &(0..10), Case::Proper).unwrap().0, "Hallo Welt");
        assert_eq!(change_case("hallo wELT", &(0..10), Case::ProperBlend).unwrap().0, "Hallo WELT");
        assert_eq!(between_braces("f(a[1], b)", 5), Some(4..5));
        assert_eq!(between_braces("f(a[1], b)", 8), Some(2..9));
        assert_eq!(column_insert("a\nbbb\nc", 0..3, 2, |i| column_number(i, 1, 2, 1, NumFormat::Dec, 2, '0')).unwrap(), "a 01\nbb03b\nc 05");
        assert_eq!(split_lines("aa bb cc dd", &(0..0), 5).unwrap().0, "aa bb\ncc dd");
        assert_eq!(line_op("b10\na2\n3", &(0..0), LineOp::SortLengthAsc, 4).unwrap().0, "3\na2\nb10");
        assert_eq!(line_op("1,5\n1,25\nx", &(0..0), LineOp::SortDecimalCommaAsc, 4).unwrap().0, "1,25\n1,5\nx");
        assert_eq!(change_case("aB", &(0..2), Case::Invert).unwrap().0, "Ab");
        assert_eq!(matching_bracket("f(a[1])", 1), Some((1, 6)));
        assert_eq!(matching_bracket("f(a[1])", 7), Some((6, 1)));
    }
}
