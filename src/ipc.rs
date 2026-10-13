//! Single instance: a second start hands its file arguments to the running window
//! through a Unix socket in $XDG_RUNTIME_DIR and exits.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

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
    s.write_all(msg.as_bytes()).is_ok()
}

/// Listens for other instances. Each received batch is sent as a list of paths.
/// Returns false if the socket could not be created.
pub fn listen(tx: Sender<Vec<PathBuf>>, wake: impl Fn() + Send + 'static) -> bool {
    let path = socket_path();
    // A socket file without a listener is left over from a crash.
    if UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let Ok(listener) = UnixListener::bind(&path) else { return false };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut files = Vec::new();
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if line.is_empty() {
                    break;
                }
                files.push(PathBuf::from(line));
            }
            if tx.send(files).is_err() {
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
