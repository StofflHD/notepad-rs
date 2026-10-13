mod api;
mod app;
mod assoc;
mod cmd;
mod commands;
mod doc;
mod editor;
mod findfiles;
mod fold;
mod fonts;
mod highlight;
mod i18n;
mod i18n_de;
mod icons;
mod ipc;
mod project;
mod search;
mod settings;
mod textops;
mod theme;
mod tools;
mod tray;
mod udl;
mod update;
mod ui_dialogs;
mod ui_menus;
mod ui_panels;
mod ui_prefs;

use std::path::PathBuf;

use app::CliOptions;

/// Expands wildcards (`*.rs`, `src/**/*.txt` with -r).
fn expand(arg: &str, cwd: &std::path::Path, recursive: bool) -> Vec<PathBuf> {
    let p = cwd.join(arg);
    if !arg.contains(['*', '?', '[']) {
        return vec![p];
    }
    let (dir, pat) = match arg.rfind('/') {
        Some(i) => (cwd.join(&arg[..i]), &arg[i + 1..]),
        None => (cwd.to_path_buf(), arg),
    };
    let Ok(glob) = globset::Glob::new(pat).map(|g| g.compile_matcher()) else { return vec![p] };
    let depth = if recursive { usize::MAX } else { 1 };
    let mut v: Vec<PathBuf> = walkdir::WalkDir::new(&dir)
        .max_depth(depth)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && glob.is_match(e.file_name()))
        .map(|e| e.into_path())
        .collect();
    v.sort();
    v
}

fn parse_args(args: &[String]) -> (CliOptions, bool) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut o = CliOptions::default();
    let mut new_instance = false;
    let mut recursive = false;
    let mut it = args.iter().peekable();
    let mut plain = Vec::new();
    while let Some(a) = it.next() {
        let l = a.to_lowercase();
        match () {
            _ if l == "--new-instance" || l == "-multiinst" => new_instance = true,
            _ if l == "-nosession" => o.no_session = true,
            _ if l == "-ro" => o.read_only = true,
            _ if l == "-alwaysontop" => o.always_on_top = true,
            _ if l == "-systemtray" => o.system_tray = true,
            _ if l == "-notabbar" => o.no_tab_bar = true,
            _ if l == "-monitor" => o.monitor = true,
            _ if l == "-r" => recursive = true,
            _ if l == "-opensession" => o.open_session = it.next().map(|f| cwd.join(f)),
            _ if l.starts_with("-udl=") => o.udl = Some(a[5..].trim_matches('"').to_string()),
            _ if l.starts_with("-n") && a[2..].parse::<usize>().is_ok() => o.line = a[2..].parse().ok(),
            _ if l.starts_with("-c") && a[2..].parse::<usize>().is_ok() => o.column = a[2..].parse().ok(),
            _ if l.starts_with("-l") && a.len() > 2 => o.lang = Some(a[2..].to_string()),
            _ if a.starts_with('-') && a.len() > 1 && !std::path::Path::new(a).exists() => {}
            _ => plain.push(a.clone()),
        }
    }
    for a in plain {
        o.files.extend(expand(&a, &cwd, recursive));
    }
    (o, new_instance)
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{} {}\n\n{}", app::APP_NAME, env!("CARGO_PKG_VERSION"), ui_dialogs::CMDLINE_HELP);
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("{} {}", app::APP_NAME, env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let (cli, new_instance) = parse_args(&args);

    let settings = settings::Settings::load();
    if settings.single_instance && !new_instance && cli.open_session.is_none() && ipc::send_to_running(&cli.files) {
        return Ok(());
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(app::APP_NAME)
        .with_app_id("notepad-rs")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([400.0, 250.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../packaging/notepad-rs.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(app::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, cli)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parsing() {
        let args: Vec<String> = ["-n12", "-c3", "-lPython", "-ro", "-multiInst", "-udl=\"My Lang\"", "x.txt"].iter().map(|s| s.to_string()).collect();
        let (o, ni) = parse_args(&args);
        assert!(ni && o.read_only);
        assert_eq!((o.line, o.column), (Some(12), Some(3)));
        assert_eq!(o.lang.as_deref(), Some("Python"));
        assert_eq!(o.udl.as_deref(), Some("My Lang"));
        assert!(o.files[0].ends_with("x.txt"));
    }
}
