//! "Find in Files" / "Replace in Files": directory search on a background thread.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;

use crate::doc::{detect_encoding, normalize_eol};
use crate::search::{find_all, Hit};

pub enum Msg {
    File { path: PathBuf, hits: Vec<Hit> },
    Progress { scanned: usize },
    Done { scanned: usize, cancelled: bool },
}

/// Parsed filter: "*.rs *.toml; !*.min.js !+/target" (Notepad++ syntax, `!+\dir` excludes folders).
pub struct Filter {
    include: GlobSet,
    exclude: GlobSet,
    exclude_dirs: GlobSet,
    any: bool,
}

impl Filter {
    pub fn new(spec: &str) -> Result<Self, String> {
        let (mut inc, mut exc, mut dirs) = (GlobSetBuilder::new(), GlobSetBuilder::new(), GlobSetBuilder::new());
        let mut any = true;
        for tok in spec.split(|c: char| c == ';' || c.is_whitespace()).filter(|t| !t.is_empty()) {
            let glob = |p: &str| Glob::new(p).map_err(|e| e.to_string());
            if let Some(d) = tok.strip_prefix("!+").or_else(|| tok.strip_prefix("!\\")) {
                dirs.add(glob(d.trim_start_matches(['/', '\\']))?);
            } else if let Some(x) = tok.strip_prefix('!') {
                exc.add(glob(x)?);
            } else if tok != "*.*" && tok != "*" {
                inc.add(glob(tok)?);
                any = false;
            }
        }
        Ok(Self {
            include: inc.build().map_err(|e| e.to_string())?,
            exclude: exc.build().map_err(|e| e.to_string())?,
            exclude_dirs: dirs.build().map_err(|e| e.to_string())?,
            any,
        })
    }

    pub fn file_matches(&self, name: &str) -> bool {
        (self.any || self.include.is_match(name)) && !self.exclude.is_match(name)
    }

    pub fn dir_excluded(&self, name: &str) -> bool {
        self.exclude_dirs.is_match(name)
    }
}

/// All files below `dir` matching the filter.
pub fn files(dir: &Path, filter: &Filter, subfolders: bool, hidden: bool, cancel: &AtomicBool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let walker = walkdir::WalkDir::new(dir)
        .max_depth(if subfolders { usize::MAX } else { 1 })
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if e.depth() == 0 {
                return true;
            }
            if !hidden && name.starts_with('.') {
                return false;
            }
            !(e.file_type().is_dir() && filter.dir_excluded(&name))
        });
    for e in walker.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if e.file_type().is_file() && filter.file_matches(&e.file_name().to_string_lossy()) {
            out.push(e.into_path());
        }
    }
    out
}

/// Reads a text file (skips binaries and huge files).
pub fn read_text(path: &Path) -> Option<(String, crate::doc::TextEncoding)> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 100 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let enc = detect_encoding(&bytes);
    if !enc.name.starts_with("UTF-16") && bytes[..bytes.len().min(8192)].contains(&0) {
        return None;
    }
    Some((enc.decode(&bytes).0, enc))
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(dir: PathBuf, filter: Filter, subfolders: bool, hidden: bool, re: Regex, cancel: Arc<AtomicBool>, tx: Sender<Msg>, wake: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let list = files(&dir, &filter, subfolders, hidden, &cancel);
        let mut scanned = 0;
        let mut total_hits = 0;
        for path in list {
            if cancel.load(Ordering::Relaxed) || total_hits > 200_000 {
                break;
            }
            scanned += 1;
            if let Some((text, _)) = read_text(&path) {
                let (text, _, _) = normalize_eol(text, crate::doc::Eol::Lf);
                let hits = find_all(&text, &re, 50_000);
                if !hits.is_empty() {
                    total_hits += hits.len();
                    if tx.send(Msg::File { path, hits }).is_err() {
                        return;
                    }
                    wake();
                }
            }
            if scanned % 200 == 0 {
                let _ = tx.send(Msg::Progress { scanned });
                wake();
            }
        }
        let _ = tx.send(Msg::Done { scanned, cancelled: cancel.load(Ordering::Relaxed) });
        wake();
    });
}

/// Replaces in all matching files (keeping encoding and line endings). Returns (files, replacements).
pub fn replace_in_files(dir: &Path, filter: &Filter, subfolders: bool, hidden: bool, re: &Regex, repl: &str, skip: &[PathBuf]) -> Result<(usize, usize), String> {
    let cancel = AtomicBool::new(false);
    let (mut files_changed, mut count) = (0, 0);
    for path in files(dir, filter, subfolders, hidden, &cancel) {
        if skip.contains(&path) {
            continue;
        }
        let Some((text, enc)) = read_text(&path) else { continue };
        let (norm, eol, eols) = normalize_eol(text, crate::doc::Eol::Lf);
        let n = re.find_iter(&norm).count();
        if n == 0 {
            continue;
        }
        let replaced = re.replace_all(&norm, repl).into_owned();
        // Restore line endings (mixed files get their dominant EOL).
        let _ = eols;
        let out = if eol == crate::doc::Eol::Lf { replaced } else { replaced.replace('\n', eol.as_str()) };
        let (bytes, _) = enc.encode(&out);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        files_changed += 1;
        count += n;
    }
    Ok((files_changed, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_and_search() {
        let dir = std::env::temp_dir().join(format!("nrs-fif-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("a.rs"), "fn main() {}\n// TODO one\n").unwrap();
        std::fs::write(dir.join("sub/b.rs"), "TODO two\r\nx\r\n").unwrap();
        std::fs::write(dir.join("target/c.rs"), "TODO skip").unwrap();
        std::fs::write(dir.join("d.txt"), "TODO txt").unwrap();
        std::fs::write(dir.join("bin.rs"), b"TODO\0\x01").unwrap();
        let f = Filter::new("*.rs !+/target").unwrap();
        let cancel = AtomicBool::new(false);
        let mut found = files(&dir, &f, true, false, &cancel);
        found.sort();
        assert_eq!(found.len(), 3, "{found:?}");
        let re = Regex::new("TODO").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(dir.clone(), Filter::new("*.rs !+/target").unwrap(), true, false, re.clone(), Arc::new(AtomicBool::new(false)), tx, || {});
        let mut hit_files = 0;
        for m in rx {
            match m {
                Msg::File { .. } => hit_files += 1,
                Msg::Done { scanned, .. } => {
                    assert_eq!(scanned, 3);
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(hit_files, 2, "binary file skipped");
        let (files, n) = replace_in_files(&dir, &f, true, false, &re, "DONE", &[]).unwrap();
        assert_eq!((files, n), (2, 2));
        assert_eq!(std::fs::read_to_string(dir.join("sub/b.rs")).unwrap(), "DONE two\r\nx\r\n", "CRLF kept");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
