//! Integration tests for reading-progress persistence.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::fixture;
use epub_core::{BookKey, Position, ProgressStore};

/// A fresh, empty directory for one test, under Cargo's per-target temp dir
/// (`CARGO_TARGET_TMPDIR` is set by Cargo for integration tests).
fn scratch_dir(test: &str) -> anyhow::Result<PathBuf> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("progress")
        .join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn at(spine_index: usize, fraction: f64) -> Position {
    Position {
        spine_index,
        fraction,
    }
}

#[test]
fn book_key_is_the_sha256_of_the_file() -> anyhow::Result<()> {
    let key = BookKey::from_file(&fixture("not-a-zip.epub"))?;
    // Same value as `sha256sum tests/fixtures/not-a-zip.epub`.
    assert_eq!(
        key.as_str(),
        "4ea402e6d455a4f1322677246320e08f3038509fa8e23f1a7d65ae3976d91cf1"
    );

    let alice = BookKey::from_file(&fixture("alice-epub2.epub"))?;
    assert_ne!(key, alice);
    assert_eq!(alice, BookKey::from_file(&fixture("alice-epub2.epub"))?);
    Ok(())
}

#[test]
fn positions_survive_save_and_reload() -> anyhow::Result<()> {
    let file = scratch_dir("round_trip")?.join("nested/dir/progress.json");
    let alice = BookKey::from_file(&fixture("alice-epub2.epub"))?;
    let frank = BookKey::from_file(&fixture("frankenstein-epub3.epub"))?;

    let (mut store, warning) = ProgressStore::open(&file)?;
    assert!(warning.is_none());
    assert_eq!(store.position(&alice), None);
    assert_eq!(store.last_opened(), None);

    store.set_position(
        &alice,
        Path::new("/books/alice.epub"),
        Some("Alice"),
        at(3, 0.25),
    );
    store.set_position(&frank, Path::new("/books/frank.epub"), None, at(7, 1.0));
    store.set_last_opened(Path::new("/books/alice.epub"));
    store.save()?; // creates the missing parent directories

    let (reloaded, warning) = ProgressStore::open(&file)?;
    assert!(warning.is_none());
    assert_eq!(reloaded.position(&alice), Some(at(3, 0.25)));
    assert_eq!(reloaded.position(&frank), Some(at(7, 1.0)));
    assert_eq!(reloaded.last_opened(), Some(Path::new("/books/alice.epub")));

    // The file is plain, readable JSON.
    let text = fs::read_to_string(&file)?;
    assert!(text.contains("\"title\": \"Alice\""), "{text}");
    Ok(())
}

#[test]
fn saving_replaces_the_file_and_leaves_no_temp_file() -> anyhow::Result<()> {
    let dir = scratch_dir("atomic")?;
    let file = dir.join("progress.json");
    let key = BookKey::from_file(&fixture("alice-epub2.epub"))?;
    let book = Path::new("/books/alice.epub");

    let mut store = ProgressStore::empty(&file);
    store.set_position(&key, book, None, at(1, 0.5));
    store.save()?;
    store.set_position(&key, book, None, at(2, 0.75));
    store.save()?;

    let (reloaded, _) = ProgressStore::open(&file)?;
    assert_eq!(reloaded.position(&key), Some(at(2, 0.75)));
    let names: Vec<String> = fs::read_dir(&dir)?
        .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    assert_eq!(names, ["progress.json"]);
    Ok(())
}

#[test]
fn corrupt_file_is_moved_aside_not_overwritten() -> anyhow::Result<()> {
    let dir = scratch_dir("corrupt")?;
    let file = dir.join("progress.json");
    fs::write(&file, "{ this is not json")?;

    let (store, warning) = ProgressStore::open(&file)?;
    let warning = warning.ok_or_else(|| anyhow::anyhow!("expected a warning"))?;
    assert!(warning.contains("progress.json.corrupt"), "{warning}");
    assert_eq!(store.last_opened(), None);
    assert!(!file.exists());
    assert_eq!(
        fs::read_to_string(dir.join("progress.json.corrupt"))?,
        "{ this is not json"
    );
    Ok(())
}

#[test]
fn unknown_and_missing_fields_are_tolerated() -> anyhow::Result<()> {
    let file = scratch_dir("lenient")?.join("progress.json");
    let key = BookKey::from_file(&fixture("alice-epub2.epub"))?;
    // A field from some future version ("theme"), no "last_opened", and a
    // record without path/title.
    let json = format!(
        r#"{{ "theme": "dark", "books": {{ "{}": {{ "spine_index": 4, "fraction": 0.5 }} }} }}"#,
        key.as_str()
    );
    fs::write(&file, json)?;

    let (store, warning) = ProgressStore::open(&file)?;
    assert!(warning.is_none());
    assert_eq!(store.last_opened(), None);
    assert_eq!(store.position(&key), Some(at(4, 0.5)));
    Ok(())
}

#[test]
fn non_finite_fractions_are_stored_as_top() -> anyhow::Result<()> {
    let file = scratch_dir("nan")?.join("progress.json");
    let key = BookKey::from_file(&fixture("alice-epub2.epub"))?;
    let mut store = ProgressStore::empty(&file);
    store.set_position(&key, Path::new("/b.epub"), None, at(2, f64::NAN));
    store.save()?; // would fail: JSON has no NaN

    let (reloaded, _) = ProgressStore::open(&file)?;
    assert_eq!(reloaded.position(&key), Some(at(2, 0.0)));
    Ok(())
}
