//! Single instance: a second start hands its file arguments to the running window
//! through a Unix socket in $XDG_RUNTIME_DIR and exits.
//!
//! Message: one path per line, an empty line, then optionally the activation token the
//! launcher (file manager, application menu) gave the second process. Older instances stop
//! reading at the empty line, so they simply ignore the token.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// A batch handed over by another start.
pub struct Request {
    pub files: Vec<PathBuf>,
    /// xdg-activation token: on Wayland the window may only take the focus with it.
    pub token: Option<String>,
}

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let user = std::env::var("USER").unwrap_or_default();
    dir.join(format!("notepad-rs-{user}.sock"))
}

/// Tries to pass `files` to a running instance. `true` = handed over, this process can exit.
pub fn send_to_running(files: &[PathBuf]) -> bool {
    let Ok(mut s) = UnixStream::connect(socket_path()) else { return false };
    let mut msg = String::new();
    for f in files {
        msg.push_str(&f.display().to_string());
        msg.push('\n');
    }
    // An empty line asks the instance to come to the front.
    msg.push('\n');
    if let Some(token) = std::env::var("XDG_ACTIVATION_TOKEN").ok().filter(|t| !t.is_empty() && !t.contains('\n')) {
        msg.push_str(&token);
        msg.push('\n');
    }
    s.write_all(msg.as_bytes()).is_ok()
}

/// Listens for other instances and forwards each received batch.
/// Returns false if the socket could not be created.
pub fn listen(tx: Sender<Request>, wake: impl Fn() + Send + 'static) -> bool {
    let path = socket_path();
    // A socket file without a listener is left over from a crash.
    if UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let Ok(listener) = UnixListener::bind(&path) else { return false };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut lines = BufReader::new(stream).lines().map_while(Result::ok);
            let files = lines.by_ref().take_while(|l| !l.is_empty()).map(PathBuf::from).collect();
            let token = lines.next().filter(|t| !t.is_empty());
            if tx.send(Request { files, token }).is_err() {
                break;
            }
            wake();
        }
    });
    true
}

pub fn cleanup() {
    let _ = std::fs::remove_file(socket_path());
}
