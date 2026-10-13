//! Tools and built-in "plugins": hashes, MIME tools, JSON/XML formatting, hex conversion,
//! compare, Run-menu variables, trash, printing and script plugins.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;

use crate::cmd::Hash;

pub fn hash_bytes(kind: Hash, data: &[u8]) -> String {
    use sha2::Digest;
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    match kind {
        Hash::Md5 => hex(&md5::Md5::digest(data)),
        Hash::Sha1 => hex(&sha1::Sha1::digest(data)),
        Hash::Sha256 => hex(&sha2::Sha256::digest(data)),
        Hash::Sha512 => hex(&sha2::Sha512::digest(data)),
    }
}

pub fn hash_name(kind: Hash) -> &'static str {
    match kind {
        Hash::Md5 => "MD5",
        Hash::Sha1 => "SHA-1",
        Hash::Sha256 => "SHA-256",
        Hash::Sha512 => "SHA-512",
    }
}

// ---- MIME tools ---------------------------------------------------------------------------------

pub fn base64_encode(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

pub fn base64_decode(s: &str) -> Result<String, String> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(clean.as_bytes())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(clean.as_bytes()))
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(clean.trim_end_matches('=').as_bytes()))
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn qp_encode(s: &str) -> String {
    let mut out = String::new();
    let mut line_len = 0;
    for b in s.bytes() {
        let enc = if b == b'\n' {
            out.push('\n');
            line_len = 0;
            continue;
        } else if (33..=126).contains(&b) && b != b'=' || b == b' ' {
            (b as char).to_string()
        } else {
            format!("={b:02X}")
        };
        if line_len + enc.len() > 75 {
            out.push_str("=\n");
            line_len = 0;
        }
        line_len += enc.len();
        out.push_str(&enc);
    }
    out
}

pub fn qp_decode(s: &str) -> String {
    let s = s.replace("=\r\n", "").replace("=\n", "");
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn ascii_to_hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

pub fn hex_to_ascii(s: &str) -> Result<String, String> {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() % 2 != 0 {
        return Err("Odd number of hex digits".into());
    }
    let bytes: Vec<u8> = (0..clean.len()).step_by(2).map(|i| u8::from_str_radix(&clean[i..i + 2], 16).unwrap()).collect();
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ---- JSON / XML ---------------------------------------------------------------------------------

pub fn json_format(s: &str, indent: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("JSON: {e}"))?;
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(&v, &mut ser).map_err(|e| e.to_string())?;
    Ok(String::from_utf8(buf).unwrap_or_default())
}

pub fn json_minify(s: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("JSON: {e}"))?;
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

/// Checks well-formedness; returns the error with line number.
pub fn xml_check(s: &str) -> Result<(), String> {
    let mut r = quick_xml::Reader::from_str(s);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let line_of = |pos: usize| s[..pos.min(s.len())].matches('\n').count() + 1;
    loop {
        match r.read_event() {
            Ok(quick_xml::events::Event::Start(e)) => stack.push(e.name().as_ref().to_vec()),
            Ok(quick_xml::events::Event::End(e)) => {
                if stack.pop().as_deref() != Some(e.name().as_ref()) {
                    return Err(format!("Line {}: unexpected </{}>", line_of(r.buffer_position() as usize), String::from_utf8_lossy(e.name().as_ref())));
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("Line {}: {e}", line_of(r.error_position() as usize))),
        }
    }
    if let Some(open) = stack.last() {
        return Err(format!("Unclosed element <{}>", String::from_utf8_lossy(open)));
    }
    Ok(())
}

pub fn xml_format(s: &str, indent: &str) -> Result<String, String> {
    xml_check(s)?;
    let mut r = quick_xml::Reader::from_str(s);
    r.config_mut().trim_text(true);
    let mut w = quick_xml::Writer::new_with_indent(Vec::new(), indent.as_bytes().first().copied().unwrap_or(b' '), indent.len().max(1));
    loop {
        match r.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(e) => w.write_event(e).map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(String::from_utf8_lossy(&w.into_inner()).into_owned())
}

pub fn xml_minify(s: &str) -> Result<String, String> {
    xml_check(s)?;
    let mut r = quick_xml::Reader::from_str(s);
    r.config_mut().trim_text(true);
    let mut w = quick_xml::Writer::new(Vec::new());
    loop {
        match r.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(e) => w.write_event(e).map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(String::from_utf8_lossy(&w.into_inner()).into_owned())
}

// ---- Compare ------------------------------------------------------------------------------------

/// Line markers for two texts: (markers of a, markers of b); 1 added, 2 removed, 3 changed.
pub fn compare(a: &str, b: &str) -> (BTreeMap<usize, u8>, BTreeMap<usize, u8>, usize) {
    use similar::{DiffOp, TextDiff};
    let diff = TextDiff::from_lines(a, b);
    let (mut ma, mut mb) = (BTreeMap::new(), BTreeMap::new());
    let mut blocks = 0;
    for op in diff.ops() {
        match *op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete { old_index, old_len, .. } => {
                blocks += 1;
                for i in old_index..old_index + old_len {
                    ma.insert(i, 2);
                }
            }
            DiffOp::Insert { new_index, new_len, .. } => {
                blocks += 1;
                for i in new_index..new_index + new_len {
                    mb.insert(i, 1);
                }
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                blocks += 1;
                for i in old_index..old_index + old_len {
                    ma.insert(i, 3);
                }
                for i in new_index..new_index + new_len {
                    mb.insert(i, 3);
                }
            }
        }
    }
    (ma, mb, blocks)
}

// ---- Run menu -----------------------------------------------------------------------------------

#[derive(Default, Clone)]
pub struct RunVars {
    pub full_path: String,
    pub word: String,
    pub line: usize,
    pub column: usize,
    pub line_text: String,
    pub selection: String,
}

/// Expands Notepad++ Run variables like `$(FULL_CURRENT_PATH)` and `$(SYS.HOME)`.
pub fn expand_vars(cmd: &str, v: &RunVars) -> String {
    let p = Path::new(&v.full_path);
    let exe = std::env::current_exe().unwrap_or_default();
    let mut out = cmd
        .replace("$(FULL_CURRENT_PATH)", &v.full_path)
        .replace("$(CURRENT_DIRECTORY)", &p.parent().map(|d| d.display().to_string()).unwrap_or_default())
        .replace("$(FILE_NAME)", &p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        .replace("$(NAME_PART)", &p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        .replace("$(EXT_PART)", &p.extension().map(|n| format!(".{}", n.to_string_lossy())).unwrap_or_default())
        .replace("$(CURRENT_WORD)", &v.word)
        .replace("$(CURRENT_LINE)", &v.line.to_string())
        .replace("$(CURRENT_COLUMN)", &v.column.to_string())
        .replace("$(CURRENT_LINESTR)", &v.line_text)
        .replace("$(SELECTION)", &v.selection)
        .replace("$(NPP_DIRECTORY)", &exe.parent().map(|d| d.display().to_string()).unwrap_or_default())
        .replace("$(NPP_FULL_FILE_PATH)", &exe.display().to_string());
    while let Some(i) = out.find("$(SYS.") {
        let Some(e) = out[i..].find(')') else { break };
        let name = &out[i + 6..i + e];
        let val = std::env::var(name).unwrap_or_default();
        out.replace_range(i..i + e + 1, &val);
    }
    out
}

/// Starts a shell command detached from the editor.
pub fn spawn_shell(cmd: &str, dir: Option<&Path>) -> Result<(), String> {
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(cmd);
    if let Some(d) = dir.filter(|d| d.is_dir()) {
        c.current_dir(d);
    }
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn open_with_default(target: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open").arg(target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Opens a terminal in `dir` (tries $TERMINAL and common emulators).
pub fn open_terminal(dir: &Path) -> Result<(), String> {
    let mut candidates: Vec<String> = std::env::var("TERMINAL").into_iter().collect();
    candidates.extend(
        ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "kitty", "alacritty", "wezterm", "foot", "tilix", "xterm"]
            .iter()
            .map(|s| s.to_string()),
    );
    for t in candidates {
        if std::process::Command::new(&t).current_dir(dir).spawn().is_ok() {
            return Ok(());
        }
    }
    Err("No terminal emulator found (set $TERMINAL)".into())
}

// ---- Trash (freedesktop.org spec) -----------------------------------------------------------------

pub fn move_to_trash(path: &Path) -> Result<(), String> {
    // Prefer gio (handles other mounts), fall back to the home trash.
    if std::process::Command::new("gio").arg("trash").arg(path).status().is_ok_and(|s| s.success()) {
        return Ok(());
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or("No home directory")?;
    let trash = data.join("Trash");
    std::fs::create_dir_all(trash.join("files")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(trash.join("info")).map_err(|e| e.to_string())?;
    let name = path.file_name().ok_or("Invalid path")?.to_string_lossy().into_owned();
    let mut target = name.clone();
    let mut n = 1;
    while trash.join("files").join(&target).exists() {
        n += 1;
        target = format!("{name}.{n}");
    }
    let abs = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let info = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        url_encode(&abs.display().to_string()).replace("%2F", "/"),
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S")
    );
    std::fs::write(trash.join("info").join(format!("{target}.trashinfo")), info).map_err(|e| e.to_string())?;
    std::fs::rename(&abs, trash.join("files").join(&target)).map_err(|e| e.to_string())
}

// ---- Printing -------------------------------------------------------------------------------------

/// A line to print: text runs with RGB colour.
pub type PrintLine = Vec<(String, [u8; 3])>;

fn ps_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\t' => out.push_str("    "),
            c if (c as u32) < 0x20 => out.push(' '),
            c if (c as u32) < 0x80 => out.push(c),
            c if (c as u32) <= 0xFF => out.push_str(&format!("\\{:03o}", c as u32)),
            _ => out.push('?'),
        }
    }
    out
}

/// Generates a PostScript document (A4, Courier) with header, footer and optional line numbers.
pub fn postscript(lines: &[PrintLine], header: &str, footer_tpl: &str, font_size: f32, line_numbers: bool) -> String {
    let (w, h) = (595.0, 842.0);
    let margin = 40.0;
    let lh = font_size * 1.2;
    let top = h - margin - font_size * 2.5;
    let per_page = (((top - margin - font_size * 2.0) / lh).floor() as usize).max(1);
    let char_w = font_size * 0.6;
    let num_w = if line_numbers { (lines.len().to_string().len() as f32 + 1.0) * char_w } else { 0.0 };
    let max_chars = (((w - 2.0 * margin - num_w) / char_w).floor() as usize).max(10);
    // Wrap long lines.
    let mut rows: Vec<(Option<usize>, PrintLine)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let mut cur: PrintLine = Vec::new();
        let mut len = 0;
        let mut first = true;
        for (t, c) in l {
            let mut rest: &str = t;
            while !rest.is_empty() {
                let take = (max_chars - len).max(1);
                let cut = rest.char_indices().nth(take).map_or(rest.len(), |(b, _)| b);
                cur.push((rest[..cut].to_string(), *c));
                len += rest[..cut].chars().count();
                rest = &rest[cut..];
                if len >= max_chars {
                    rows.push((first.then_some(i), std::mem::take(&mut cur)));
                    first = false;
                    len = 0;
                }
            }
        }
        if !cur.is_empty() || len == 0 && first {
            rows.push((first.then_some(i), cur));
        } else if !cur.is_empty() {
            rows.push((None, cur));
        }
    }
    let pages = rows.len().div_ceil(per_page).max(1);
    let mut ps = String::from("%!PS-Adobe-3.0\n%%Creator: NotepadRS\n%%DocumentMedia: A4 595 842 0 () ()\n");
    ps.push_str(&format!("%%Pages: {pages}\n%%EndComments\n"));
    ps.push_str("/F { /Courier findfont exch scalefont setfont } def\n/ISOF { /Courier findfont dup length dict begin {1 index /FID ne {def} {pop pop} ifelse} forall /Encoding ISOLatin1Encoding def currentdict end /CourierLatin1 exch definefont pop } def ISOF\n/F { /CourierLatin1 findfont exch scalefont setfont } def\n");
    for p in 0..pages {
        ps.push_str(&format!("%%Page: {} {}\n", p + 1, p + 1));
        ps.push_str("0 0 0 setrgbcolor\n");
        ps.push_str(&format!("{} F\n", font_size * 0.9));
        ps.push_str(&format!("{margin} {} moveto ({}) show\n", h - margin - font_size, ps_escape(header)));
        let footer = footer_tpl.replace("$(PAGE)", &(p + 1).to_string()).replace("$(PAGES)", &pages.to_string());
        ps.push_str(&format!("{margin} {} moveto ({}) show\n", margin - font_size * 0.5, ps_escape(&footer)));
        ps.push_str(&format!("{font_size} F\n"));
        for (k, (num, row)) in rows.iter().skip(p * per_page).take(per_page).enumerate() {
            let y = top - k as f32 * lh;
            if let Some(n) = num.filter(|_| line_numbers) {
                ps.push_str(&format!("0.5 0.5 0.5 setrgbcolor {margin} {y} moveto ({}) show\n", n + 1));
            }
            ps.push_str(&format!("{} {y} moveto\n", margin + num_w));
            for (t, c) in row {
                ps.push_str(&format!(
                    "{:.3} {:.3} {:.3} setrgbcolor ({}) show\n",
                    c[0] as f32 / 255.0,
                    c[1] as f32 / 255.0,
                    c[2] as f32 / 255.0,
                    ps_escape(t)
                ));
            }
        }
        ps.push_str("showpage\n");
    }
    ps.push_str("%%EOF\n");
    ps
}

/// Installed printers (CUPS).
pub fn printers() -> Vec<String> {
    let out = std::process::Command::new("lpstat").arg("-a").output();
    let Ok(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split_whitespace().next().map(str::to_owned)).collect()
}

pub fn send_to_printer(command: &str, printer: Option<&str>, copies: u32, ps: &str) -> Result<(), String> {
    use std::io::Write;
    let mut parts = command.split_whitespace();
    let prog = parts.next().ok_or("No print command")?;
    let mut c = std::process::Command::new(prog);
    c.args(parts);
    if let Some(p) = printer.filter(|p| !p.is_empty()) {
        c.arg("-d").arg(p);
    }
    if copies > 1 {
        c.arg("-n").arg(copies.to_string());
    }
    let mut child = c.stdin(std::process::Stdio::piped()).spawn().map_err(|e| format!("{prog}: {e}"))?;
    child.stdin.take().unwrap().write_all(ps.as_bytes()).map_err(|e| e.to_string())?;
    let st = child.wait().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("{prog} failed ({st})"))
    }
}

// ---- Script plugins ---------------------------------------------------------------------------------

pub fn plugins_dir() -> PathBuf {
    crate::settings::config_dir().join("plugins")
}

/// Executable files in the plugins folder: (menu name, path).
pub fn script_plugins() -> Vec<(String, String)> {
    use std::os::unix::fs::PermissionsExt;
    let mut v: Vec<(String, String)> = std::fs::read_dir(plugins_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .map(|e| {
            let p = e.path();
            (p.file_stem().unwrap_or_default().to_string_lossy().replace('_', " "), p.display().to_string())
        })
        .collect();
    v.sort();
    v
}

/// Runs a script plugin: input on stdin, output replaces the input (if the script succeeds).
pub fn run_script(path: &str, input: &str, env: &[(&str, String)]) -> Result<Option<String>, String> {
    use std::io::Write;
    let mut c = std::process::Command::new(path);
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().unwrap();
    let data = input.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(data.as_bytes());
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    if !out.status.success() {
        return Err(format!("{}\n{}", out.status, String::from_utf8_lossy(&out.stderr)));
    }
    let s = String::from_utf8_lossy(&out.stdout).into_owned();
    Ok((!s.is_empty()).then_some(s))
}

// ---- NppExport: HTML / RTF ------------------------------------------------------------------------

/// A styled run: text, colour, bold, italic.
pub type ExportRun = (String, [u8; 3], bool, bool);

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn html_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(ch),
        }
    }
    o
}

/// Highlighted text as a standalone HTML document (like NppExport).
pub fn to_html(lines: &[Vec<ExportRun>], fg: [u8; 3], bg: [u8; 3], font: &str, size: f32, title: &str, tab: usize) -> String {
    let mut out = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"UTF-8\">\n<title>{}</title>\n</head>\n<body style=\"margin:0\">\n<pre style=\"margin:0;padding:8px;background:{};color:{};font-family:'{}',monospace;font-size:{}pt;tab-size:{tab}\">",
        html_escape(title),
        hex(bg),
        hex(fg),
        html_escape(font),
        size.round()
    );
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for (t, c, b, it) in line {
            if t.is_empty() {
                continue;
            }
            if *c == fg && !b && !it {
                out.push_str(&html_escape(t));
                continue;
            }
            out.push_str(&format!("<span style=\"color:{}", hex(*c)));
            if *b {
                out.push_str(";font-weight:bold");
            }
            if *it {
                out.push_str(";font-style:italic");
            }
            out.push_str("\">");
            out.push_str(&html_escape(t));
            out.push_str("</span>");
        }
    }
    out.push_str("</pre>\n</body>\n</html>\n");
    out
}

/// Highlighted text as RTF.
pub fn to_rtf(lines: &[Vec<ExportRun>], fg: [u8; 3], bg: [u8; 3], font: &str, size: f32) -> String {
    let mut colors: Vec<[u8; 3]> = vec![fg, bg];
    for line in lines {
        for (_, c, _, _) in line {
            if !colors.contains(c) {
                colors.push(*c);
            }
        }
    }
    let mut out = format!("{{\\rtf1\\ansi\\ansicpg1252\\deff0{{\\fonttbl{{\\f0\\fmodern {};}}}}{{\\colortbl;", font.replace(['{', '}', '\\', ';'], ""));
    for c in &colors {
        out.push_str(&format!("\\red{}\\green{}\\blue{};", c[0], c[1], c[2]));
    }
    out.push_str(&format!("}}\n\\f0\\fs{}\\cb2\\highlight2 ", (size * 2.0).round() as u32));
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push_str("\\par\n");
        }
        for (t, c, b, it) in line {
            if t.is_empty() {
                continue;
            }
            let ci = colors.iter().position(|x| x == c).unwrap_or(0) + 1;
            out.push_str(&format!("{{\\cf{ci}{}{} ", if *b { "\\b" } else { "" }, if *it { "\\i" } else { "" }));
            for ch in t.chars() {
                match ch {
                    '\\' | '{' | '}' => {
                        out.push('\\');
                        out.push(ch);
                    }
                    '\t' => out.push_str("\\tab "),
                    c if (c as u32) < 0x80 => out.push(c),
                    c => {
                        let mut buf = [0u16; 2];
                        for u in c.encode_utf16(&mut buf) {
                            out.push_str(&format!("\\u{}?", *u as i16));
                        }
                    }
                }
            }
            out.push('}');
        }
    }
    out.push_str("}\n");
    out
}

fn which(cmd: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
}

/// Puts data of a MIME type on the clipboard (wl-copy / xclip). Formats other than text and HTML
/// are not supported by the clipboard library.
pub fn clipboard_set_mime(mime: &str, data: &str) -> Result<(), String> {
    use std::io::Write;
    let (prog, args): (&str, Vec<&str>) = if std::env::var_os("WAYLAND_DISPLAY").is_some() && which("wl-copy") {
        ("wl-copy", vec!["--type", mime])
    } else if which("xclip") {
        ("xclip", vec!["-selection", "clipboard", "-t", mime, "-i"])
    } else {
        return Err(crate::i18n::tr("This needs wl-clipboard (Wayland) or xclip (X11).").into());
    };
    let mut child = std::process::Command::new(prog).args(&args).stdin(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("stdin")?.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Reads clipboard data of a MIME type (wl-paste / xclip).
pub fn clipboard_get_mime(mime: &str) -> Result<String, String> {
    let (prog, args): (&str, Vec<&str>) = if std::env::var_os("WAYLAND_DISPLAY").is_some() && which("wl-paste") {
        ("wl-paste", vec!["--no-newline", "--type", mime])
    } else if which("xclip") {
        ("xclip", vec!["-selection", "clipboard", "-t", mime, "-o"])
    } else {
        return Err(crate::i18n::tr("This needs wl-clipboard (Wayland) or xclip (X11).").into());
    };
    let out = std::process::Command::new(prog).args(&args).output().map_err(|e| e.to_string())?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(crate::i18n::tr("The clipboard has no content of this type.").into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_and_rtf_export() {
        let fg = [0, 0, 0];
        let lines = vec![vec![("fn".to_string(), [0, 0, 255], true, false), (" a<b> ".to_string(), fg, false, false)], vec![("{é}".to_string(), [255, 0, 0], false, true)]];
        let h = to_html(&lines, fg, [255, 255, 255], "Hack", 10.0, "t.rs", 4);
        assert!(h.contains("<span style=\"color:#0000ff;font-weight:bold\">fn</span> a&lt;b&gt; \n<span style=\"color:#ff0000;font-style:italic\">{é}</span>"), "{h}");
        let r = to_rtf(&lines, fg, [255, 255, 255], "Hack", 10.0);
        assert!(r.starts_with("{\\rtf1"));
        assert!(r.contains("{\\cf3\\b fn}"), "{r}");
        assert!(r.contains("\\par\n{\\cf4\\i \\{\\u233?\\}}"), "{r}");
    }

    #[test]
    fn hashes() {
        assert_eq!(hash_bytes(Hash::Md5, b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hash_bytes(Hash::Sha1, b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert!(hash_bytes(Hash::Sha256, b"abc").starts_with("ba7816bf"));
        assert_eq!(hash_bytes(Hash::Sha512, b"").len(), 128);
    }

    #[test]
    fn mime_tools() {
        assert_eq!(base64_encode("Grüße"), "R3LDvMOfZQ==");
        assert_eq!(base64_decode("R3LDvMOfZQ==").unwrap(), "Grüße");
        assert_eq!(url_encode("a b/ä"), "a%20b%2F%C3%A4");
        assert_eq!(url_decode("a%20b%2F%C3%A4+x"), "a b/ä x");
        assert_eq!(qp_decode(&qp_encode("Grüße = gut")), "Grüße = gut");
        assert_eq!(ascii_to_hex("AB"), "41 42");
        assert_eq!(hex_to_ascii("41 42").unwrap(), "AB");
    }

    #[test]
    fn json_xml() {
        assert_eq!(json_format("{\"b\":1,\"a\":[1,2]}", "  ").unwrap(), "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ]\n}");
        assert_eq!(json_minify("{ \"a\" : 1 }").unwrap(), "{\"a\":1}");
        assert!(json_format("{", "  ").is_err());
        assert!(xml_check("<a><b></a>").is_err());
        assert_eq!(xml_format("<a><b>x</b></a>", "  ").unwrap(), "<a>\n  <b>x</b>\n</a>");
        assert_eq!(xml_minify("<a>\n  <b>x</b>\n</a>").unwrap(), "<a><b>x</b></a>");
    }

    #[test]
    fn compare_lines() {
        let (a, b, n) = compare("1\n2\n3\n", "1\nX\n3\n4\n");
        assert_eq!(a.get(&1), Some(&3));
        assert_eq!(b.get(&1), Some(&3));
        assert_eq!(b.get(&3), Some(&1));
        assert_eq!(n, 2);
    }

    #[test]
    fn run_vars() {
        let v = RunVars { full_path: "/tmp/x/file.rs".into(), word: "foo".into(), line: 3, column: 7, ..Default::default() };
        assert_eq!(expand_vars("$(CURRENT_DIRECTORY)|$(FILE_NAME)|$(NAME_PART)$(EXT_PART)|$(CURRENT_WORD):$(CURRENT_LINE)", &v), "/tmp/x|file.rs|file.rs|foo:3");
        std::env::set_var("NRS_TEST_VAR", "ok");
        assert_eq!(expand_vars("$(SYS.NRS_TEST_VAR)", &v), "ok");
    }

    #[test]
    fn postscript_pages() {
        let lines: Vec<PrintLine> = (0..200).map(|i| vec![(format!("line {i} (ä)"), [0, 0, 0])]).collect();
        let ps = postscript(&lines, "hdr", "$(PAGE)/$(PAGES)", 9.0, true);
        assert!(ps.contains("%%Pages: 4"), "{}", &ps[..200]);
        assert!(ps.contains("\\(\\344\\)"));
        assert!(ps.contains("(1/4) show"));
    }

    #[test]
    fn script_plugin() {
        let r = run_script("/bin/cat", "hello", &[]).unwrap();
        assert_eq!(r.as_deref(), Some("hello"));
        assert!(run_script("/bin/false", "x", &[]).is_err());
    }
}
