//! Integration tests for full-text search.

mod common;

use common::fixture;
use epub_core::Book;

#[test]
fn finds_matches_in_reading_order_ignoring_case() -> anyhow::Result<()> {
    let mut book = Book::open(fixture("frankenstein-epub3.epub"))?;
    let results = book.search("ELIZABETH", 1000);

    assert!(results.skipped.is_empty(), "{:?}", results.skipped);
    assert!(!results.truncated);
    assert!(results.hits.len() > 50, "only {} hits", results.hits.len());
    assert!(results.hits.iter().all(|h| h.matched == "Elizabeth"));

    // Sorted by document, then numbered 0, 1, 2... within each document.
    for pair in results.hits.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.spine_index == b.spine_index {
            assert_eq!(b.occurrence, a.occurrence + 1);
        } else {
            assert!(b.spine_index > a.spine_index);
            assert_eq!(b.occurrence, 0);
        }
    }
    Ok(())
}

#[test]
fn whitespace_in_the_query_and_text_is_flexible() -> anyhow::Result<()> {
    let mut book = Book::open(fixture("alice-epub2.epub"))?;
    let exact = book.search("Down the Rabbit-Hole", 100);
    let sloppy = book.search("  down   THE rabbit-hole ", 100);

    assert!(!exact.hits.is_empty());
    assert_eq!(exact, sloppy);
    // The snippet shows the book's own spelling, with context around it.
    let hit = &exact.hits[0];
    assert_eq!(hit.matched, "Down the Rabbit-Hole");
    assert!(!hit.after.is_empty());
    Ok(())
}

#[test]
fn limit_truncates_the_results() -> anyhow::Result<()> {
    let mut book = Book::open(fixture("frankenstein-epub3.epub"))?;
    let results = book.search("the", 25);
    assert_eq!(results.hits.len(), 25);
    assert!(results.truncated);
    Ok(())
}

#[test]
fn unreadable_documents_are_skipped_not_fatal() -> anyhow::Result<()> {
    let mut book = Book::open(fixture("broken.epub"))?;
    let results = book.search("hello", 10);

    assert_eq!(results.skipped, ["OPS/text/missing.xhtml"]);
    assert_eq!(results.hits.len(), 1);
    assert_eq!(results.hits[0].spine_index, 0);
    assert_eq!(results.hits[0].matched, "Hello");
    Ok(())
}

#[test]
fn nothing_found_is_an_empty_result() -> anyhow::Result<()> {
    let mut book = Book::open(fixture("peter-rabbit-images.epub"))?;
    assert!(book.search("xylophone quantum", 10).hits.is_empty());
    assert!(book.search("", 10).hits.is_empty());
    Ok(())
}
