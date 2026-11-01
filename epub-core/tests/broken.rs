//! Integration tests against hand-made broken EPUBs (see
//! `tests/fixtures/hand-made/`, rebuilt with `hand-made/build.sh`).

mod common;

use common::fixture;
use epub_core::{Book, EpubError, Href, flatten};

#[test]
fn recoverable_problems_become_warnings() -> anyhow::Result<()> {
    let book = Book::open(fixture("broken.epub"))?;

    let meta = book.metadata();
    assert_eq!(meta.title.as_deref(), Some("A Broken Book"));
    assert!(meta.authors.is_empty());
    assert_eq!(meta.language, None);

    let has_warning = |needle: &str| book.warnings().iter().any(|w| w.contains(needle));
    for needle in [
        "mimetype entry is not application/epub+zip",
        "unknown manifest id \"ghost\"",
        "unknown manifest id \"escape\"",
        "manifest item \"escape\"",
        "manifest item \"remote\"",
        "without id or href",
        "could not read nav document: invalid XML in OPS/nav.xhtml: ",
    ] {
        assert!(has_warning(needle), "no warning containing {needle:?}");
    }
    assert_eq!(book.warnings().len(), 7, "{:#?}", book.warnings());
    Ok(())
}

#[test]
fn spine_skips_unknown_ids_and_decodes_paths() -> anyhow::Result<()> {
    let book = Book::open(fixture("broken.epub"))?;

    let paths: Vec<&str> = book.spine().iter().map(|i| i.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "OPS/text/Chapter 1.xhtml",
            "OPS/text/ch2.xhtml",
            "OPS/text/missing.xhtml"
        ]
    );
    assert!(!book.spine()[1].linear);
    Ok(())
}

#[test]
fn invalid_nav_falls_back_to_ncx() -> anyhow::Result<()> {
    let book = Book::open(fixture("broken.epub"))?;
    let toc = flatten(book.toc());

    let labels: Vec<&str> = toc.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Chapter 1", "Middle of chapter 1", "Escaping link"]
    );
    assert_eq!(
        toc[1].target,
        Some(Href {
            path: "OPS/text/Chapter 1.xhtml".into(),
            fragment: Some("middle".into()),
        })
    );
    // A link escaping the archive is dropped, but its entry is kept.
    assert_eq!(toc[2].target, None);
    Ok(())
}

#[test]
fn empty_nav_falls_back_to_ncx() -> anyhow::Result<()> {
    let book = Book::open(fixture("empty-nav.epub"))?;

    assert_eq!(book.metadata().authors, ["First Author", "Second Author"]);
    let labels: Vec<&str> = book.toc().iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["Only Chapter"]);
    assert_eq!(
        book.warnings(),
        ["OPS/nav.xhtml has an empty table of contents"]
    );
    Ok(())
}

#[test]
fn missing_resource_is_a_clear_error() -> anyhow::Result<()> {
    let mut book = Book::open(fixture("broken.epub"))?;

    // `matches!` checks a value against a pattern, including nested fields.
    let err = book.read_bytes("OPS/text/missing.xhtml");
    assert!(
        matches!(&err, Err(EpubError::MissingFile(p)) if p == "OPS/text/missing.xhtml"),
        "{err:?}"
    );
    // Present entries are still readable.
    assert_eq!(
        book.read_bytes("OPS/style.css")?,
        b"body { margin: 1em; }\n"
    );
    Ok(())
}

#[test]
fn fatal_problems_are_errors_not_panics() {
    // `Result::err()` turns `Result<T, E>` into `Option<E>`.
    let open = |name: &str| Book::open(fixture(name)).err();

    assert!(matches!(open("not-a-zip.epub"), Some(EpubError::Zip(_))));
    assert!(matches!(
        open("no-container.epub"),
        Some(EpubError::MissingFile(p)) if p == "META-INF/container.xml"
    ));
    assert!(matches!(
        open("bad-opf.epub"),
        Some(EpubError::Xml { path, .. }) if path == "OPS/package.opf"
    ));
    assert!(matches!(
        open("empty-spine.epub"),
        Some(EpubError::Invalid(_))
    ));
    assert!(matches!(
        open("does-not-exist.epub"),
        Some(EpubError::Io(_))
    ));
}
