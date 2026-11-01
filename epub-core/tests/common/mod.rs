//! Helpers shared by the integration tests.
//!
//! Each file in `tests/` is compiled as its own crate; files in a
//! subdirectory like `tests/common/mod.rs` are not, so this is the usual
//! place for shared code. Each test file pulls it in with `mod common;`.

use std::path::PathBuf;

use epub_core::TocEntry;

/// Absolute path of a file in `tests/fixtures/`.
pub fn fixture(name: &str) -> PathBuf {
    // `env!` reads an environment variable at *compile* time; Cargo sets
    // CARGO_MANIFEST_DIR to the folder containing epub-core's Cargo.toml.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The TOC tree flattened in reading order (parents before children).
pub fn flatten(entries: &[TocEntry]) -> Vec<&TocEntry> {
    let mut out = Vec::new();
    for entry in entries {
        out.push(entry);
        out.extend(flatten(&entry.children));
    }
    out
}
