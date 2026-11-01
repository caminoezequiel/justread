//! Print an EPUB's metadata, spine and table of contents.
//!
//! Usage: cargo run -p epub-core --example info -- book.epub

use anyhow::Context;
use epub_core::{Book, TocEntry};

fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .context("usage: info <book.epub>")?;
    // `with_context` takes a closure so the message is only built on error.
    let book = Book::open(&path).with_context(|| format!("opening {}", path.display()))?;

    let meta = book.metadata();
    println!(
        "Title:    {}",
        meta.title.as_deref().unwrap_or("(untitled)")
    );
    println!("Authors:  {}", meta.authors.join(", "));
    println!("Language: {}", meta.language.as_deref().unwrap_or("?"));

    println!("\nSpine ({} items):", book.spine().len());
    // `enumerate` yields (index, item) pairs.
    for (i, item) in book.spine().iter().enumerate() {
        let note = if item.linear { "" } else { "  [non-linear]" };
        println!("  {i:3}  {}{note}", item.path);
    }

    println!("\nTable of contents:");
    if book.toc().is_empty() {
        println!("  (none)");
    }
    print_toc(book.toc(), 1);

    if !book.warnings().is_empty() {
        println!("\nWarnings:");
        for w in book.warnings() {
            println!("  - {w}");
        }
    }
    Ok(())
}

fn print_toc(entries: &[TocEntry], depth: usize) {
    for entry in entries {
        let target = match &entry.target {
            Some(href) => match &href.fragment {
                Some(f) => format!("{}#{f}", href.path),
                None => href.path.clone(),
            },
            None => "-".to_owned(),
        };
        println!("{}{}  -> {target}", "  ".repeat(depth), entry.label);
        print_toc(&entry.children, depth + 1);
    }
}
