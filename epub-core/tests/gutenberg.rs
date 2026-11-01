//! Integration tests against real Project Gutenberg books.

mod common;

use std::io::Cursor;

use common::{fixture, flatten};
use epub_core::{Book, Href};

const FRANKENSTEIN: &str = "frankenstein-epub3.epub";
const ALICE: &str = "alice-epub2.epub";
const PETER_RABBIT: &str = "peter-rabbit-images.epub";
const ALL_BOOKS: [&str; 3] = [FRANKENSTEIN, ALICE, PETER_RABBIT];

// Tests return `anyhow::Result<()>` so they can use `?`; an `Err` fails the test
// and prints the error, which reads better than a panic from `unwrap()`.

#[test]
fn epub3_metadata_spine_and_nav_toc() -> anyhow::Result<()> {
    let book = Book::open(fixture(FRANKENSTEIN))?;

    let meta = book.metadata();
    assert_eq!(
        meta.title.as_deref(),
        Some("Frankenstein; or, The modern Prometheus")
    );
    assert_eq!(meta.authors, ["Mary Wollstonecraft Shelley"]);
    assert_eq!(meta.language.as_deref(), Some("en"));

    assert_eq!(book.spine().len(), 32);
    assert_eq!(book.spine()[0].path, "OEBPS/wrap0000.xhtml");
    assert!(book.spine().iter().all(|item| item.linear));

    // No warnings means the EPUB3 nav document parsed (no NCX fallback).
    assert_eq!(book.warnings(), [] as [String; 0]);
    let toc = flatten(book.toc());
    assert_eq!(toc.len(), 32);
    let chapter1 = toc
        .iter()
        .find(|e| e.label == "Chapter 1")
        .ok_or_else(|| anyhow::anyhow!("no 'Chapter 1' entry"))?;
    assert_eq!(
        chapter1.target,
        Some(Href {
            path: "OEBPS/4328224471496264690_84-h-6.htm.xhtml".into(),
            fragment: Some("pgepubid00007".into()),
        })
    );
    Ok(())
}

#[test]
fn epub2_uses_ncx_toc() -> anyhow::Result<()> {
    let book = Book::open(fixture(ALICE))?;

    assert_eq!(
        book.metadata().title.as_deref(),
        Some("Alice's Adventures in Wonderland")
    );
    assert_eq!(book.metadata().authors, ["Lewis Carroll"]);
    assert_eq!(book.spine().len(), 14);
    assert_eq!(book.warnings(), [] as [String; 0]);

    let labels: Vec<&str> = flatten(book.toc())
        .iter()
        .map(|e| e.label.as_str())
        .collect();
    assert_eq!(labels.len(), 16);
    assert!(labels.contains(&"CHAPTER I. Down the Rabbit-Hole"));
    assert!(labels.contains(&"CHAPTER XII. Alice’s Evidence"));
    Ok(())
}

/// Every chapter and TOC target must exist, every TOC target must be in the
/// spine, and every fragment must be an id in its target document.
#[test]
fn all_spine_items_and_toc_targets_resolve() -> anyhow::Result<()> {
    for name in ALL_BOOKS {
        let mut book = Book::open(fixture(name))?;

        // `.to_vec()` clones the spine so we don't hold a borrow of `book`
        // while calling `read_bytes`, which needs `&mut book`.
        for item in book.spine().to_vec() {
            let bytes = book.read_bytes(&item.path)?;
            assert!(!bytes.is_empty(), "{name}: {} is empty", item.path);
        }

        let targets: Vec<Href> = flatten(book.toc())
            .into_iter()
            .filter_map(|e| e.target.clone())
            .collect();
        assert!(!targets.is_empty(), "{name}: TOC has no links");
        for target in targets {
            assert!(
                book.spine_index_of(&target.path).is_some(),
                "{name}: TOC target {} is not in the spine",
                target.path
            );
            let text = String::from_utf8(book.read_bytes(&target.path)?)?;
            if let Some(id) = &target.fragment {
                assert!(
                    text.contains(&format!("id=\"{id}\"")),
                    "{name}: #{id} not found in {}",
                    target.path
                );
            }
        }
    }
    Ok(())
}

/// Images referenced by `<img src>` in a chapter resolve to real image data:
/// this is the same lookup the reader's iframe will do for relative URLs.
#[test]
fn chapter_images_resolve_to_image_bytes() -> anyhow::Result<()> {
    let mut book = Book::open(fixture(PETER_RABBIT))?;
    let chapter = book.spine()[1].path.clone();
    let xhtml = String::from_utf8(book.read_bytes(&chapter)?)?;

    let doc = roxmltree::Document::parse(&xhtml)?;
    let sources: Vec<&str> = doc
        .descendants()
        .filter(|n| n.has_tag_name("img"))
        .filter_map(|n| n.attribute("src"))
        .collect();
    assert!(sources.len() >= 25, "found only {} images", sources.len());

    for src in sources {
        let href = Href::resolve(&chapter, src)?;
        let bytes = book.read_bytes(&href.path)?;
        let is_jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
        let is_png = bytes.starts_with(b"\x89PNG");
        assert!(is_jpeg || is_png, "{} is not a JPEG/PNG", href.path);
    }
    Ok(())
}

#[test]
fn opens_from_memory_like_from_disk() -> anyhow::Result<()> {
    let bytes = std::fs::read(fixture(ALICE))?;
    let from_memory = Book::from_reader(Cursor::new(bytes))?;
    let from_disk = Book::open(fixture(ALICE))?;

    assert_eq!(from_memory.metadata(), from_disk.metadata());
    assert_eq!(from_memory.spine(), from_disk.spine());
    assert_eq!(from_memory.toc(), from_disk.toc());
    Ok(())
}
