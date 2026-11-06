//! "Add to application menu": moves the running AppImage to `~/Applications` and creates a
//! desktop entry + icon, so NotepadRS appears in the start menu and in "Open with" lists. No root
//! needed, and the updater can still replace the file. Uninstall removes entry, icon, file
//! associations and the installed AppImage (settings in ~/.config/notepad-rs stay).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::assoc::Dirs;

/// `~/Applications`, the usual place for AppImages (also used by AppImageLauncher / Gear Lever).
pub fn apps_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Applications")
}

/// A path for display, with the home directory shortened to `~`.
pub fn tilde(p: &Path) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match p.strip_prefix(&home) {
        Ok(rest) if !home.is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

/// NotepadRS has a desktop entry (installed via this menu item or the file association page).
pub fn is_installed(dirs: &Dirs) -> bool {
    dirs.desktop_path().is_file()
}

/// Moves (or, across file systems, copies) `appimage` into `dest_dir`. Returns the new path.
fn place(appimage: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let err = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    std::fs::create_dir_all(dest_dir).map_err(|e| err(dest_dir, e))?;
    let name = appimage.file_name().ok_or("no file name")?;
    let dest = dest_dir.join(name);
    if dest == appimage {
        return Ok(dest);
    }
    if std::fs::rename(appimage, &dest).is_err() {
        // Different file system (or no write access to the source folder): copy.
        let tmp = dest_dir.join(format!(".{}.part", name.to_string_lossy()));
        std::fs::copy(appimage, &tmp).map_err(|e| err(&tmp, e))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| err(&tmp, e))?;
        std::fs::rename(&tmp, &dest).map_err(|e| err(&dest, e))?;
        let _ = std::fs::remove_file(appimage);
    }
    Ok(dest)
}

/// Installs: AppImage into `dest_dir`, desktop entry and icon. Returns the installed AppImage.
pub fn install(dirs: &Dirs, appimage: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let dest = place(appimage, dest_dir)?;
    crate::assoc::write_entry(dirs, &dest)?;
    Ok(dest)
}

/// Removes the desktop entry, icon and file associations; deletes the AppImage when it lives in
/// `dest_dir` (the running program keeps working until it exits).
pub fn uninstall(dirs: &Dirs, appimage: Option<&Path>, dest_dir: &Path) -> Result<(), String> {
    crate::assoc::apply_defaults(dirs, &BTreeSet::new())?;
    crate::assoc::remove_entry(dirs)?;
    if let Some(a) = appimage.filter(|a| a.parent() == Some(dest_dir)) {
        let _ = std::fs::remove_file(a);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_and_uninstall() {
        let dir = std::env::temp_dir().join(format!("nrs-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dirs = Dirs { config: dir.join("config"), data: dir.join("data") };
        let downloads = dir.join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let src = downloads.join("NotepadRS-0.1.9-x86_64.AppImage");
        std::fs::write(&src, b"appimage").unwrap();
        let apps = dir.join("Applications");
        assert!(!is_installed(&dirs));

        let dest = install(&dirs, &src, &apps).unwrap();
        assert_eq!(dest, apps.join("NotepadRS-0.1.9-x86_64.AppImage"));
        assert!(dest.is_file() && !src.exists(), "moved, not copied");
        assert!(is_installed(&dirs));
        let entry = std::fs::read_to_string(dirs.desktop_path()).unwrap();
        assert!(entry.contains(&format!("Exec=\"{}\" %F", dest.display())) && entry.contains("Categories=Utility;TextEditor;Development;"), "{entry}");
        assert!(dirs.data.join("icons/hicolor/256x256/apps/notepad-rs.png").is_file());
        // Installing again from the installed location changes nothing.
        assert_eq!(install(&dirs, &dest, &apps).unwrap(), dest);

        // File associations are released on uninstall, other defaults stay.
        std::fs::create_dir_all(&dirs.config).unwrap();
        std::fs::write(dirs.config.join("mimeapps.list"), "[Default Applications]\ntext/plain=notepad-rs.desktop;kate.desktop;\ntext/html=firefox.desktop;\n").unwrap();
        uninstall(&dirs, Some(&dest), &apps).unwrap();
        assert!(!is_installed(&dirs) && !dest.exists());
        assert!(!dirs.data.join("icons/hicolor/256x256/apps/notepad-rs.png").exists());
        assert_eq!(std::fs::read_to_string(dirs.config.join("mimeapps.list")).unwrap(), "[Default Applications]\ntext/plain=kate.desktop;\ntext/html=firefox.desktop;\n");
        // An AppImage outside ~/Applications is never deleted.
        std::fs::write(&src, b"appimage").unwrap();
        install(&dirs, &src, &downloads).unwrap();
        uninstall(&dirs, Some(&src), &apps).unwrap();
        assert!(src.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
