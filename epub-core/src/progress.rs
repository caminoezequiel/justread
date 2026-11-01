//! Remembering where the reader is in each book.
//!
//! Books are identified by the SHA-256 of the file ([`BookKey`]), so a book
//! keeps its position when it is moved or renamed. Everything is kept in one
//! small JSON file, which is always replaced atomically: a crash mid-write
//! leaves the previous version intact, never a half-written file.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProgressError {
    #[error("could not access {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("could not encode reading progress")]
    Json(#[from] serde_json::Error),
}

/// Identity of a book file: the hex SHA-256 of its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BookKey(String);

impl BookKey {
    /// Hash the file at `path`, reading it in chunks so large books don't
    /// have to fit in memory.
    pub fn from_file(path: &Path) -> io::Result<BookKey> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        // The digest derefs to `&[u8]`; format each byte as two hex digits.
        let hex = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(BookKey(hex))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A reading position: which spine item, and how far it is scrolled.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub spine_index: usize,
    /// `0.0` = top of the document, `1.0` = bottom.
    pub fraction: f64,
}

/// What is stored per book. `path` and `title` are not needed to restore a
/// position; they make the JSON file readable by a human.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BookRecord {
    #[serde(flatten)]
    position: Position,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    title: Option<String>,
}

/// The on-disk format. `#[serde(default)]` lets older or partial files load:
/// a missing field just gets its default value.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    last_opened: Option<PathBuf>,
    /// `BTreeMap` (sorted) instead of `HashMap` so the file's contents are
    /// stable from one save to the next.
    #[serde(default)]
    books: BTreeMap<String, BookRecord>,
}

pub struct ProgressStore {
    file: PathBuf,
    data: Data,
}

impl ProgressStore {
    /// Load the store from `file`.
    ///
    /// A missing file gives an empty store. A file that can't be parsed is
    /// moved aside to `<file>.corrupt` (so it is never silently overwritten)
    /// and an empty store is returned together with a warning.
    pub fn open(
        file: impl Into<PathBuf>,
    ) -> Result<(ProgressStore, Option<String>), ProgressError> {
        let file = file.into();
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(source) => return Err(ProgressError::Io { path: file, source }),
        };
        if text.trim().is_empty() {
            return Ok((ProgressStore::empty(file), None));
        }
        match serde_json::from_str(&text) {
            Ok(data) => Ok((ProgressStore { file, data }, None)),
            Err(parse_error) => {
                let aside = sibling(&file, ".corrupt");
                fs::rename(&file, &aside).map_err(|source| ProgressError::Io {
                    path: aside.clone(),
                    source,
                })?;
                let warning = format!(
                    "{} could not be read ({parse_error}); it was moved to {} and reading progress starts fresh",
                    file.display(),
                    aside.display()
                );
                Ok((ProgressStore::empty(file), Some(warning)))
            }
        }
    }

    pub fn empty(file: impl Into<PathBuf>) -> ProgressStore {
        ProgressStore {
            file: file.into(),
            data: Data::default(),
        }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn position(&self, key: &BookKey) -> Option<Position> {
        self.data
            .books
            .get(key.as_str())
            .map(|record| record.position)
    }

    /// Record `position` for a book. Non-finite fractions (NaN can't be
    /// stored in JSON) are recorded as the top of the document.
    pub fn set_position(
        &mut self,
        key: &BookKey,
        book_path: &Path,
        title: Option<&str>,
        position: Position,
    ) {
        let fraction = if position.fraction.is_finite() {
            position.fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.data.books.insert(
            key.as_str().to_owned(),
            BookRecord {
                position: Position {
                    spine_index: position.spine_index,
                    fraction,
                },
                path: Some(book_path.to_owned()),
                title: title.map(str::to_owned),
            },
        );
    }

    /// The book that was open most recently, if any.
    pub fn last_opened(&self) -> Option<&Path> {
        self.data.last_opened.as_deref()
    }

    pub fn set_last_opened(&mut self, path: &Path) {
        self.data.last_opened = Some(path.to_owned());
    }

    /// Write the store to disk atomically (see [`write_atomic`]).
    pub fn save(&self) -> Result<(), ProgressError> {
        let json = serde_json::to_vec_pretty(&self.data)?;
        write_atomic(&self.file, &json).map_err(|source| ProgressError::Io {
            path: self.file.clone(),
            source,
        })
    }
}

/// Replace `file` with `contents` atomically, creating its folder if needed:
/// write a temporary file next to it, flush that to the disk, then rename it
/// over the original. A rename within one directory replaces the file in a
/// single step, so readers see either the old or the new contents, never a mix.
pub fn write_atomic(file: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = sibling(file, ".tmp");
    {
        // This block limits the file handle's lifetime: it is closed
        // (dropped) at the closing brace, before the rename below.
        let mut out = File::create(&temp)?;
        out.write_all(contents)?;
        out.sync_all()?;
    }
    fs::rename(&temp, file)
}

/// `dir/progress.json` + `.tmp` -> `dir/progress.json.tmp`.
fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
