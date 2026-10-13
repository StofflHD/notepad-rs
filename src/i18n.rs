//! UI localization. English is the source language; other languages are lookup tables.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

static LANG: AtomicU8 = AtomicU8::new(0);

pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("de", "Deutsch")];

pub fn set_language(code: &str) {
    LANG.store(if code == "de" { 1 } else { 0 }, Ordering::Relaxed);
}

fn german() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| crate::i18n_de::ENTRIES.iter().copied().collect())
}

/// Translates a UI string (returns the input when there is no translation).
pub fn tr(s: &str) -> &str {
    if LANG.load(Ordering::Relaxed) == 1 {
        if let Some(t) = german().get(s) {
            return t;
        }
    }
    s
}

/// Translates a template and fills `{}` placeholders in order.
pub fn trf(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let t = tr(template);
    let mut out = String::with_capacity(t.len() + 16);
    let mut it = args.iter();
    let mut rest = t;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = it.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn german_covers_all_ui_strings() {
        let map: std::collections::HashMap<&str, &str> = crate::i18n_de::ENTRIES.iter().copied().collect();
        let re = regex::Regex::new(r#"\btrf?\(\s*"((?:[^"\\]|\\.)*)""#).unwrap();
        let mut missing = Vec::new();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let src = std::fs::read_to_string(e.path()).unwrap();
            for c in re.captures_iter(&src) {
                // Source literal → runtime string
                let mut k = String::new();
                let mut it = c[1].chars();
                while let Some(ch) = it.next() {
                    if ch == '\\' {
                        match it.next() {
                            Some('n') => k.push('\n'),
                            Some('t') => k.push('\t'),
                            Some(o) => k.push(o),
                            None => {}
                        }
                    } else {
                        k.push(ch);
                    }
                }
                if !map.contains_key(k.as_str()) {
                    missing.push(k);
                }
            }
        }
        for e in crate::cmd::registry() {
            if !map.contains_key(e.label) {
                missing.push(e.label.to_string());
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "missing German translations: {missing:#?}");
    }
}
