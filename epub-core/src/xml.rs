//! Small helpers on top of `roxmltree`.
//!
//! Element names are matched by *local name* only (ignoring namespaces):
//! real-world EPUBs get namespaces wrong often enough that being strict here
//! would reject books that every other reader opens fine.

use roxmltree::{Document, Node, ParsingOptions};

use crate::error::{EpubError, Result};

/// Parse `text` (the contents of `path`, used for error messages).
///
/// The returned `Document<'a>` borrows from `text`: it can't outlive the
/// string, and the borrow checker enforces that at compile time.
pub fn parse<'a>(text: &'a str, path: &str) -> Result<Document<'a>> {
    let options = ParsingOptions {
        // EPUB2 files commonly start with `<!DOCTYPE html PUBLIC ...>`.
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    Document::parse_with_options(text, options).map_err(|source| EpubError::Xml {
        path: path.to_owned(),
        source,
    })
}

pub fn is(node: Node, local_name: &str) -> bool {
    node.is_element() && node.has_tag_name(local_name)
}

/// First direct child element with the given local name.
pub fn child<'a, 'i>(node: Node<'a, 'i>, local_name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|c| is(*c, local_name))
}

/// All descendant text, with runs of whitespace collapsed to single spaces.
pub fn text_of(node: Node) -> String {
    let raw: String = node
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}
