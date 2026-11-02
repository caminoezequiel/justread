//! PDF books. A PDF is shown by PDF.js (`pdf-viewer/`) inside the same chapter
//! iframe as an EPUB, so navigation, translation and progress all go through
//! the code the EPUB chapters use. The book has one "chapter": the viewer.

use std::path::Path;

use epub_core::{Contents, Href, Metadata, SpineItem, TocEntry};
use serde::Deserialize;

/// Path of the viewer page inside the book's `epub://` URLs.
pub const VIEWER_PATH: &str = "viewer.html";
/// Path of the PDF file itself, relative to the viewer page.
pub const FILE_PATH: &str = "document.pdf";

/// A PDF file, judged by its extension. A file that isn't a readable PDF
/// shows an error in the viewer.
pub fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

/// The book's structure: the title from the file name, one chapter, no TOC
/// yet (the viewer sends the outline once PDF.js has read it).
pub fn contents(path: &Path, size: u64) -> Contents {
    Contents {
        metadata: Metadata {
            title: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
            authors: Vec::new(),
            language: None,
        },
        spine: vec![SpineItem {
            id: "pdf".to_owned(),
            path: VIEWER_PATH.to_owned(),
            media_type: "text/html".to_owned(),
            linear: true,
            size,
        }],
        toc: Vec::new(),
    }
}

/// One entry of the PDF's outline, as PDF.js reports it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OutlineItem {
    pub title: String,
    /// 0-based page, `None` when the entry has no destination.
    pub page: Option<usize>,
    #[serde(default)]
    pub items: Vec<OutlineItem>,
}

/// The outline as a TOC: each entry scrolls to its page's `#page-N` anchor.
pub fn toc(items: &[OutlineItem]) -> Vec<TocEntry> {
    items
        .iter()
        .map(|item| TocEntry {
            label: item.title.clone(),
            target: item.page.map(|page| Href {
                path: VIEWER_PATH.to_owned(),
                fragment: Some(format!("page-{}", page + 1)),
            }),
            children: toc(&item.items),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_pdf_extension_in_any_case() {
        assert!(is_pdf(Path::new("book.pdf")));
        assert!(is_pdf(Path::new("/books/BOOK.PDF")));
        assert!(!is_pdf(Path::new("book.epub")));
        assert!(!is_pdf(Path::new("pdf")));
    }

    #[test]
    fn outline_entries_scroll_to_their_page() {
        let outline = vec![OutlineItem {
            title: "Part".to_owned(),
            page: Some(4),
            items: vec![OutlineItem {
                title: "Chapter".to_owned(),
                page: None,
                items: Vec::new(),
            }],
        }];
        let toc = toc(&outline);
        let target = toc[0].target.as_ref().expect("page target");
        assert_eq!(target.path, VIEWER_PATH);
        assert_eq!(target.fragment.as_deref(), Some("page-5"));
        assert_eq!(toc[0].children[0].label, "Chapter");
        assert_eq!(toc[0].children[0].target, None);
    }

    #[test]
    fn one_chapter_named_after_the_file() {
        let contents = contents(Path::new("/books/Staff Engineer.pdf"), 42);
        assert_eq!(contents.metadata.title.as_deref(), Some("Staff Engineer"));
        assert_eq!(contents.spine.len(), 1);
        assert_eq!(contents.spine[0].path, VIEWER_PATH);
        assert_eq!(contents.spine[0].size, 42);
        assert!(contents.toc.is_empty());
    }
}
