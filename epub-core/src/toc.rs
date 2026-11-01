//! Table of contents: EPUB3 `nav.xhtml` or EPUB2 `toc.ncx`.

use roxmltree::Node;

use crate::error::Result;
use crate::href::Href;
use crate::xml;

const OPS_NS: &str = "http://www.idpf.org/2007/ops";

/// One TOC entry; entries nest to form the chapter/section tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub label: String,
    /// `None` for headings that don't link anywhere (a `<span>` in nav.xhtml).
    pub target: Option<Href>,
    pub children: Vec<TocEntry>,
}

/// The TOC tree flattened in reading order (parents before their children).
pub fn flatten(entries: &[TocEntry]) -> Vec<&TocEntry> {
    let mut out = Vec::new();
    for entry in entries {
        out.push(entry);
        out.extend(flatten(&entry.children));
    }
    out
}

/// Parse an EPUB3 navigation document located at `nav_path`.
pub fn parse_nav(text: &str, nav_path: &str) -> Result<Vec<TocEntry>> {
    let doc = xml::parse(text, nav_path)?;
    let navs: Vec<Node> = doc.descendants().filter(|n| xml::is(*n, "nav")).collect();
    let is_toc = |n: &Node| {
        n.attribute((OPS_NS, "type"))
            .is_some_and(|t| t.split_whitespace().any(|t| t == "toc"))
    };
    // Fall back to the first <nav> if none is marked epub:type="toc".
    let Some(nav) = navs.iter().find(|n| is_toc(n)).or(navs.first()) else {
        return Ok(Vec::new());
    };
    Ok(xml::child(*nav, "ol")
        .map(|ol| parse_ol(ol, nav_path))
        .unwrap_or_default())
}

fn parse_ol(ol: Node, nav_path: &str) -> Vec<TocEntry> {
    ol.children()
        .filter(|n| xml::is(*n, "li"))
        .filter_map(|li| {
            let label_node = li
                .children()
                .find(|c| xml::is(*c, "a") || xml::is(*c, "span"))?;
            Some(TocEntry {
                label: xml::text_of(label_node),
                target: label_node
                    .attribute("href")
                    .and_then(|h| Href::resolve(nav_path, h).ok()),
                children: xml::child(li, "ol")
                    .map(|ol| parse_ol(ol, nav_path))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// Parse an EPUB2 NCX document located at `ncx_path`.
pub fn parse_ncx(text: &str, ncx_path: &str) -> Result<Vec<TocEntry>> {
    let doc = xml::parse(text, ncx_path)?;
    Ok(doc
        .descendants()
        .find(|n| xml::is(*n, "navMap"))
        .map(|map| parse_nav_points(map, ncx_path))
        .unwrap_or_default())
}

/// Recursive: each navPoint may contain further navPoints.
fn parse_nav_points(parent: Node, ncx_path: &str) -> Vec<TocEntry> {
    parent
        .children()
        .filter(|n| xml::is(*n, "navPoint"))
        .map(|point| TocEntry {
            label: xml::child(point, "navLabel")
                .map(xml::text_of)
                .unwrap_or_default(),
            target: xml::child(point, "content")
                .and_then(|c| c.attribute("src"))
                .and_then(|src| Href::resolve(ncx_path, src).ok()),
            children: parse_nav_points(point, ncx_path),
        })
        .collect()
}
