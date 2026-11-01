//! Where reading progress is stored, and loading/saving it with errors
//! reported instead of crashing (a reader that can't save progress is still
//! a working reader).

use std::error::Error;
use std::path::PathBuf;

use directories::ProjectDirs;
use epub_core::ProgressStore;

/// Overrides the data directory, e.g. to test without touching real data.
const DATA_DIR_ENV: &str = "JUSTREAD_DATA_DIR";

/// `progress.json` in the OS's per-user, per-machine data directory:
/// `~/.local/share/justread` on Linux, `~/Library/Application Support/justread`
/// on macOS, `%LOCALAPPDATA%\justread\data` on Windows.
pub fn progress_file() -> Option<PathBuf> {
    let dir = match std::env::var_os(DATA_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => ProjectDirs::from("", "", "justread")?
            .data_local_dir()
            .to_path_buf(),
    };
    Some(dir.join("progress.json"))
}

/// Load the store; `None` disables saving (no data directory, or the file
/// exists but can't be read, in which case we must not overwrite it).
pub fn load(file: Option<PathBuf>) -> Option<ProgressStore> {
    let Some(file) = file else {
        eprintln!("justread: no data directory found; reading progress won't be saved");
        return None;
    };
    match ProgressStore::open(&file) {
        Ok((store, warning)) => {
            if let Some(warning) = warning {
                eprintln!("justread: {warning}");
            }
            Some(store)
        }
        Err(e) => {
            eprintln!("justread: reading progress disabled: {}", chain(&e));
            None
        }
    }
}

pub fn save(store: &ProgressStore) {
    if let Err(e) = store.save() {
        eprintln!("justread: could not save reading progress: {}", chain(&e));
    }
}

/// An error and its causes on one line. `&dyn Error` accepts any error type.
fn chain(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}
