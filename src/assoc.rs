//! Preferences > File Association: registers NotepadRS as the default application for MIME types
//! (freedesktop way: a desktop entry in ~/.local/share/applications plus ~/.config/mimeapps.list).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const DESKTOP_ID: &str = "notepad-rs.desktop";

pub const GROUPS: &[(&str, &[(&str, &str)])] = &[
    ("Text", &[("Plain text (.txt)", "text/plain"), ("Log files (.log)", "text/x-log"), ("Markdown (.md)", "text/markdown"), ("CSV (.csv)", "text/csv"), ("INI / config (.ini, .conf)", "text/x-ini")]),
    (
        "Web",
        &[
            ("HTML (.html)", "text/html"),
            ("CSS (.css)", "text/css"),
            ("JavaScript (.js)", "text/javascript"),
            ("JavaScript (application/javascript)", "application/javascript"),
            ("PHP (.php)", "application/x-php"),
            ("JSON (.json)", "application/json"),
            ("XML (.xml)", "application/xml"),
            ("XML (text/xml)", "text/xml"),
            ("YAML (.yaml)", "application/x-yaml"),
            ("TOML (.toml)", "application/toml"),
        ],
    ),
    (
        "Source code",
        &[
            ("C (.c)", "text/x-csrc"),
            ("C header (.h)", "text/x-chdr"),
            ("C++ (.cpp)", "text/x-c++src"),
            ("C++ header (.hpp)", "text/x-c++hdr"),
            ("C# (.cs)", "text/x-csharp"),
            ("Java (.java)", "text/x-java"),
            ("Python (.py)", "text/x-python"),
            ("Python 3 (.py)", "text/x-python3"),
            ("Rust (.rs)", "text/rust"),
            ("Rust (text/x-rust)", "text/x-rust"),
            ("Go (.go)", "text/x-go"),
            ("Lua (.lua)", "text/x-lua"),
            ("Perl (.pl)", "application/x-perl"),
            ("Ruby (.rb)", "application/x-ruby"),
            ("SQL (.sql)", "application/sql"),
            ("Makefile", "text/x-makefile"),
            ("CMake (CMakeLists.txt)", "text/x-cmake"),
            ("TeX (.tex)", "text/x-tex"),
        ],
    ),
    ("Scripts", &[("Shell script (.sh)", "application/x-shellscript"), ("Shell script (text/x-shellscript)", "text/x-shellscript"), ("Batch (.bat)", "application/x-bat"), ("Desktop entry (.desktop)", "application/x-desktop")]),
];

pub struct Dirs {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl Dirs {
    pub fn user() -> Dirs {
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let env = |k: &str, d: PathBuf| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or(d);
        Dirs { config: env("XDG_CONFIG_HOME", home.join(".config")), data: env("XDG_DATA_HOME", home.join(".local/share")) }
    }

    fn mimeapps(&self) -> PathBuf {
        self.config.join("mimeapps.list")
    }

    fn desktop(&self) -> PathBuf {
        self.data.join("applications").join(DESKTOP_ID)
    }
}

/// The program to start: the AppImage when running from one, otherwise this binary.
pub fn exe() -> PathBuf {
    std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok()).unwrap_or_else(|| "notepad-rs".into())
}

/// MIME types currently associated with NotepadRS (default application).
pub fn current(dirs: &Dirs) -> BTreeSet<String> {
    let text = std::fs::read_to_string(dirs.mimeapps()).unwrap_or_default();
    let mut out = BTreeSet::new();
    let mut in_defaults = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_defaults = l == "[Default Applications]";
        } else if in_defaults {
            if let Some((k, v)) = l.split_once('=') {
                if v.split(';').next().map(str::trim) == Some(DESKTOP_ID) {
                    out.insert(k.trim().to_string());
                }
            }
        }
    }
    out
}

/// Rewrites `[Default Applications]` so that exactly `selected` point to NotepadRS first.
fn update_mimeapps(text: &str, selected: &BTreeSet<String>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut seen_defaults = false;
    let mut pending: BTreeSet<&String> = selected.iter().collect();
    let flush = |out: &mut Vec<String>, pending: &mut BTreeSet<&String>| {
        for m in std::mem::take(pending) {
            out.push(format!("{m}={DESKTOP_ID};"));
        }
    };
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            if section == "[Default Applications]" {
                // Insert the new entries before blank lines that end the section.
                let mut tail = Vec::new();
                while out.last().is_some_and(|s| s.trim().is_empty()) {
                    tail.push(out.pop().unwrap());
                }
                flush(&mut out, &mut pending);
                out.extend(tail);
            }
            section = l.to_string();
            seen_defaults |= l == "[Default Applications]";
            out.push(line.to_string());
            continue;
        }
        if section == "[Default Applications]" {
            if let Some((k, v)) = l.split_once('=') {
                let k = k.trim();
                let rest: Vec<&str> = v.split(';').map(str::trim).filter(|a| !a.is_empty() && *a != DESKTOP_ID).collect();
                if selected.contains(k) {
                    pending.remove(&k.to_string());
                    out.push(format!("{k}={DESKTOP_ID};{}", rest.iter().map(|a| format!("{a};")).collect::<String>()));
                } else if v.contains(DESKTOP_ID) {
                    if !rest.is_empty() {
                        out.push(format!("{k}={}", rest.iter().map(|a| format!("{a};")).collect::<String>()));
                    }
                } else {
                    out.push(line.to_string());
                }
                continue;
            }
        }
        out.push(line.to_string());
    }
    if section == "[Default Applications]" {
        flush(&mut out, &mut pending);
    } else if !pending.is_empty() {
        if !seen_defaults {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push("[Default Applications]".into());
        }
        flush(&mut out, &mut pending);
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

fn desktop_entry(exe: &Path, icon: &Path, mimes: &BTreeSet<String>) -> String {
    let all: BTreeSet<&str> = GROUPS.iter().flat_map(|(_, v)| v.iter().map(|(_, m)| *m)).chain(mimes.iter().map(String::as_str)).collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=NotepadRS\nGenericName=Text Editor\nGenericName[de]=Texteditor\nComment=Notepad++-style text and source code editor\nExec=\"{}\" %F\nIcon={}\nTerminal=false\nCategories=Utility;TextEditor;Development;\nMimeType={}\nStartupWMClass=notepad-rs\nStartupNotify=true\n",
        exe.display(),
        icon.display(),
        all.iter().map(|m| format!("{m};")).collect::<String>()
    )
}

/// Installs the desktop entry + icon and makes NotepadRS the default for `selected`
/// (types that were associated before but are not selected any more are released).
pub fn apply(dirs: &Dirs, selected: &BTreeSet<String>) -> Result<(), String> {
    let err = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let icon = dirs.data.join("icons/hicolor/256x256/apps/notepad-rs.png");
    std::fs::create_dir_all(icon.parent().unwrap()).map_err(|e| err(&icon, e))?;
    std::fs::write(&icon, include_bytes!("../packaging/notepad-rs.png")).map_err(|e| err(&icon, e))?;
    let desktop = dirs.desktop();
    std::fs::create_dir_all(desktop.parent().unwrap()).map_err(|e| err(&desktop, e))?;
    std::fs::write(&desktop, desktop_entry(&exe(), &icon, selected)).map_err(|e| err(&desktop, e))?;
    let ma = dirs.mimeapps();
    let old = std::fs::read_to_string(&ma).unwrap_or_default();
    std::fs::create_dir_all(&dirs.config).map_err(|e| err(&dirs.config, e))?;
    std::fs::write(&ma, update_mimeapps(&old, selected)).map_err(|e| err(&ma, e))?;
    // Refresh the desktop database when the tool exists (not required).
    let _ = std::process::Command::new("update-desktop-database").arg(dirs.data.join("applications")).stderr(std::process::Stdio::null()).status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mimeapps_update() {
        let sel: BTreeSet<String> = ["text/plain".to_string(), "text/x-python".to_string()].into();
        let old = "[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\ntext/plain=org.gnome.TextEditor.desktop;\ntext/html=firefox.desktop;\napplication/json=notepad-rs.desktop;\n\n[Removed Associations]\n";
        let new = update_mimeapps(old, &sel);
        assert_eq!(
            new,
            "[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\ntext/plain=notepad-rs.desktop;org.gnome.TextEditor.desktop;\ntext/html=firefox.desktop;\ntext/x-python=notepad-rs.desktop;\n\n[Removed Associations]\n"
        );
        assert_eq!(update_mimeapps("", &sel), "[Default Applications]\ntext/plain=notepad-rs.desktop;\ntext/x-python=notepad-rs.desktop;\n");
        // Releasing everything leaves the other defaults alone.
        assert_eq!(update_mimeapps(&new, &BTreeSet::new()), "[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\ntext/plain=org.gnome.TextEditor.desktop;\ntext/html=firefox.desktop;\n\n[Removed Associations]\n");

        let dir = std::env::temp_dir().join(format!("nrs-assoc-{}", std::process::id()));
        let dirs = Dirs { config: dir.join("config"), data: dir.join("data") };
        apply(&dirs, &sel).unwrap();
        assert_eq!(current(&dirs), sel);
        assert!(std::fs::read_to_string(dirs.desktop()).unwrap().contains("text/x-python;"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
