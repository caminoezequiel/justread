//! `META-INF/container.xml` and the OPF package document.

use std::collections::HashMap;

use crate::error::{EpubError, Result};
use crate::href::Href;
use crate::xml;

pub const CONTAINER_PATH: &str = "META-INF/container.xml";

/// Read `rootfile/@full-path` from container.xml: the location of the OPF.
pub fn opf_path(container_xml: &str) -> Result<String> {
    let doc = xml::parse(container_xml, CONTAINER_PATH)?;
    let full_path = doc
        .descendants()
        .filter(|n| xml::is(*n, "rootfile"))
        // Prefer the OPF rootfile if several renditions are listed.
        .find(|n| {
            n.attribute("media-type")
                .is_none_or(|m| m == "application/oebps-package+xml")
        })
        .and_then(|n| n.attribute("full-path"))
        .ok_or_else(|| EpubError::Invalid("container.xml has no rootfile".into()))?;

    // full-path is relative to the archive root, not to container.xml.
    Ok(Href::resolve("", full_path)?.path)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestItem {
    pub id: String,
    /// Zip entry path, already resolved against the OPF's folder.
    pub path: String,
    pub media_type: String,
    pub properties: Vec<String>,
}

/// The parts of the OPF the rest of the crate needs.
#[derive(Debug)]
pub struct Package {
    pub metadata: Metadata,
    pub manifest: HashMap<String, ManifestItem>,
    /// Spine entries in reading order: (idref, linear).
    pub spine: Vec<(String, bool)>,
    /// EPUB2: `<spine toc="ncx-id">`.
    pub ncx_id: Option<String>,
}

impl Package {
    /// The EPUB3 navigation document, if declared.
    pub fn nav_item(&self) -> Option<&ManifestItem> {
        self.manifest
            .values()
            .find(|item| item.properties.iter().any(|p| p == "nav"))
    }

    /// The EPUB2 NCX: the spine's `toc` attribute, or any item with the NCX media type.
    pub fn ncx_item(&self) -> Option<&ManifestItem> {
        self.ncx_id
            .as_deref()
            .and_then(|id| self.manifest.get(id))
            .or_else(|| {
                self.manifest
                    .values()
                    .find(|item| item.media_type == "application/x-dtbncx+xml")
            })
    }
}

/// Parse the OPF found at `opf_path`.
///
/// Problems that don't stop the book from being read (a manifest item with a
/// bad href, say) are pushed onto `warnings` instead of failing. Taking
/// `&mut Vec<String>` lets us append to the caller's vector without taking
/// ownership of it.
pub fn parse(opf_xml: &str, opf_path: &str, warnings: &mut Vec<String>) -> Result<Package> {
    let doc = xml::parse(opf_xml, opf_path)?;
    let package = doc.root_element();
    if !xml::is(package, "package") {
        return Err(EpubError::Invalid(format!(
            "{opf_path}: root element is <{}>, expected <package>",
            package.tag_name().name()
        )));
    }

    let metadata = xml::child(package, "metadata")
        .map(parse_metadata)
        .unwrap_or_default();

    let mut manifest = HashMap::new();
    if let Some(node) = xml::child(package, "manifest") {
        for item in node.children().filter(|n| xml::is(*n, "item")) {
            let (Some(id), Some(href)) = (item.attribute("id"), item.attribute("href")) else {
                warnings.push(format!("manifest <item> without id or href in {opf_path}"));
                continue;
            };
            match Href::resolve(opf_path, href) {
                Ok(resolved) => {
                    manifest.insert(
                        id.to_owned(),
                        ManifestItem {
                            id: id.to_owned(),
                            path: resolved.path,
                            media_type: item.attribute("media-type").unwrap_or("").to_owned(),
                            properties: item
                                .attribute("properties")
                                .unwrap_or("")
                                .split_whitespace()
                                .map(str::to_owned)
                                .collect(),
                        },
                    );
                }
                Err(e) => warnings.push(format!("manifest item {id:?}: {e}")),
            }
        }
    }

    let spine_node = xml::child(package, "spine")
        .ok_or_else(|| EpubError::Invalid(format!("{opf_path} has no <spine>")))?;
    let spine = spine_node
        .children()
        .filter(|n| xml::is(*n, "itemref"))
        .filter_map(|n| {
            let idref = n.attribute("idref")?.to_owned();
            let linear = n.attribute("linear") != Some("no");
            Some((idref, linear))
        })
        .collect();

    Ok(Package {
        metadata,
        manifest,
        spine,
        ncx_id: spine_node.attribute("toc").map(str::to_owned),
    })
}

fn parse_metadata(node: roxmltree::Node) -> Metadata {
    // A closure that captures `node` by reference; called once per field below.
    let texts = |name: &str| -> Vec<String> {
        node.children()
            .filter(|n| xml::is(*n, name))
            .map(xml::text_of)
            .filter(|t| !t.is_empty())
            .collect()
    };
    Metadata {
        title: texts("title").into_iter().next(),
        authors: texts("creator"),
        language: texts("language").into_iter().next(),
    }
}
