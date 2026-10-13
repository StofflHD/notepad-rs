//! Font setup: optional user-selected monospace font plus system fallbacks for
//! characters the built-in fonts lack (CJK, many symbols).

use std::path::{Path, PathBuf};

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

fn font_dirs() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts")];
    if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        v.push(h.join(".local/share/fonts"));
        v.push(h.join(".fonts"));
    }
    v
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 6 {
            walk(&p, depth + 1, out);
        } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_lowercase().as_str(), "ttf" | "otf" | "ttc")) {
            out.push(p);
        }
    }
}

fn all_fonts() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for d in font_dirs() {
        walk(&d, 0, &mut v);
    }
    v
}

/// Regular-weight monospace fonts installed on the system: (display name, path).
pub fn monospace_fonts() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = all_fonts()
        .into_iter()
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_string();
            let l = stem.to_lowercase();
            let mono = ["mono", "code", "courier", "consol", "hack", "terminus", "fixed", "inconsolata", "iosevka"]
                .iter()
                .any(|k| l.contains(k));
            let styled = ["bold", "italic", "oblique", "light", "thin", "medium", "semibold", "extra", "black", "condensed"]
                .iter()
                .any(|k| l.contains(k));
            (mono && !styled && !l.contains("cjk")).then(|| (stem.replace(['-', '_'], " "), p.display().to_string()))
        })
        .collect();
    v.sort_by_key(|(n, _)| n.to_lowercase());
    v.dedup_by(|a, b| a.0 == b.0);
    v
}

/// Fallback fonts for scripts missing from the built-in fonts.
fn fallbacks() -> Vec<PathBuf> {
    let wanted = [
        "DejaVuSansMono.ttf",
        "DejaVuSans.ttf",
        "NotoSansMonoCJK-Regular.ttc",
        "NotoSansCJK-Regular.ttc",
        "NotoSansCJKjp-Regular.otf",
        "wqy-microhei.ttc",
        "DroidSansFallbackFull.ttf",
        "NotoSansSymbols2-Regular.ttf",
    ];
    let fonts = all_fonts();
    let mut out = Vec::new();
    let mut have_cjk = false;
    for w in wanted {
        let is_cjk = w.contains("CJK") || w.contains("wqy") || w.contains("Droid");
        if is_cjk && have_cjk {
            continue;
        }
        if let Some(p) = fonts.iter().find(|p| p.file_name().and_then(|n| n.to_str()) == Some(w)) {
            out.push(p.clone());
            have_cjk |= is_cjk;
        }
    }
    out
}

pub fn apply(ctx: &egui::Context, font_path: Option<&str>) {
    let mut defs = FontDefinitions::default();
    if let Some(path) = font_path {
        if let Ok(bytes) = std::fs::read(path) {
            defs.font_data.insert("user-mono".into(), FontData::from_owned(bytes));
            defs.families.entry(FontFamily::Monospace).or_default().insert(0, "user-mono".into());
        }
    }
    for (i, p) in fallbacks().into_iter().enumerate() {
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let name = format!("fallback-{i}");
        defs.font_data.insert(name.clone(), FontData::from_owned(bytes));
        for fam in [FontFamily::Monospace, FontFamily::Proportional] {
            defs.families.entry(fam).or_default().push(name.clone());
        }
    }
    ctx.set_fonts(defs);
}
