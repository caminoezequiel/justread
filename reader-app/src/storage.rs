//! Where the app keeps its files (reading progress, preferences), and
//! loading/saving them with errors reported instead of crashing: a reader
//! that can't save is still a working reader.

use std::error::Error;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use epub_core::ProgressStore;

use crate::preferences::Preferences;

/// Overrides the data directory, e.g. to test without touching real data.
const DATA_DIR_ENV: &str = "JUSTREAD_DATA_DIR";

/// The OS's per-user, per-machine data directory: `~/.local/share/justread`
/// on Linux, `~/Library/Application Support/justread` on macOS,
/// `%LOCALAPPDATA%\justread\data` on Windows.
pub fn data_dir() -> Option<PathBuf> {
    match std::env::var_os(DATA_DIR_ENV) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => Some(
            ProjectDirs::from("", "", "justread")?
                .data_local_dir()
                .to_path_buf(),
        ),
    }
}

/// Load the progress store; `None` disables saving (no data directory, or
/// the file exists but can't be read, in which case we must not overwrite it).
pub fn load_progress(file: Option<PathBuf>) -> Option<ProgressStore> {
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

pub fn save_progress(store: &ProgressStore) {
    if let Err(e) = store.save() {
        eprintln!("justread: could not save reading progress: {}", chain(&e));
    }
}

/// Preferences from `file`, or the defaults if it is missing or unreadable.
/// (Unlike reading progress, losing preferences costs a few clicks at most,
/// so a bad file is simply replaced on the next change.)
pub fn load_preferences(file: &Path) -> Preferences {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Preferences::default(),
        Err(e) => {
            eprintln!("justread: could not read {}: {e}", file.display());
            return Preferences::default();
        }
    };
    match serde_json::from_str::<Preferences>(&text) {
        Ok(preferences) => preferences.sanitized(),
        Err(e) => {
            eprintln!(
                "justread: ignoring {} ({e}); using default preferences",
                file.display()
            );
            Preferences::default()
        }
    }
}

pub fn save_preferences(file: &Path, preferences: &Preferences) {
    let result = serde_json::to_vec_pretty(preferences)
        .map_err(|e| e.to_string())
        .and_then(|json| epub_core::write_atomic(file, &json).map_err(|e| e.to_string()));
    if let Err(e) = result {
        eprintln!(
            "justread: could not save preferences to {}: {e}",
            file.display()
        );
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
