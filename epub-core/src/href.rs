//! Turning hrefs found inside the book into paths of zip entries.
//!
//! Hrefs in OPF/nav/NCX files are URLs: percent-encoded, relative to the
//! document that contains them, and possibly carrying a `#fragment`.
//! Zip entry names are plain, root-relative paths like `OEBPS/text/ch1.xhtml`.

use percent_encoding::percent_decode_str;

use crate::error::{EpubError, Result};

/// A resolved link target: a zip entry path plus an optional fragment id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Href {
    /// Path of the zip entry, e.g. `OEBPS/text/ch1.xhtml` (decoded, no leading `/`).
    pub path: String,
    /// The part after `#`, decoded, e.g. `section-2`.
    pub fragment: Option<String>,
}

impl Href {
    /// Resolve `href` as written inside the document at `from_doc`.
    ///
    /// `Href::resolve("OEBPS/nav.xhtml", "text/ch%201.xhtml#s2")` gives
    /// path `OEBPS/text/ch 1.xhtml` and fragment `s2`. A bare `#s2` points
    /// back into `from_doc` itself.
    pub fn resolve(from_doc: &str, href: &str) -> Result<Href> {
        // `split_once` returns `Option<(&str, &str)>`; both halves borrow from
        // `href`, so no allocation happens here.
        let (path_part, fragment) = match href.split_once('#') {
            Some((p, f)) => (p, Some(decode(f, href)?)),
            None => (href, None),
        };

        let path = if path_part.is_empty() {
            from_doc.to_owned()
        } else {
            join(parent_dir(from_doc), &decode(path_part, href)?)
                .ok_or_else(|| EpubError::BadHref(href.to_owned()))?
        };

        if path.is_empty() {
            return Err(EpubError::BadHref(href.to_owned()));
        }
        Ok(Href { path, fragment })
    }
}

/// Directory part of a zip path: `"OEBPS/content.opf"` -> `"OEBPS"`, `"a.opf"` -> `""`.
///
/// The returned `&str` is a slice of the input, which is why the signature ties
/// the output lifetime to the input (`'a` is inferred here by lifetime elision).
pub fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

/// Join a decoded relative path onto `base_dir`, normalising `.` and `..`.
///
/// Returns `None` for anything that would escape the archive root or that is
/// not a local path at all (e.g. `http://...`).
pub fn join(base_dir: &str, rel: &str) -> Option<String> {
    if has_scheme(rel) {
        return None;
    }
    // A leading `/` means "from the archive root".
    let (mut parts, rel): (Vec<&str>, &str) = match rel.strip_prefix('/') {
        Some(rest) => (Vec::new(), rest),
        None => (base_dir.split('/').filter(|s| !s.is_empty()).collect(), rel),
    };
    for segment in rel.split('/') {
        match segment {
            "" | "." => {}
            // `?` on an Option returns `None` early, just like it does for `Err`.
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

fn decode(s: &str, original: &str) -> Result<String> {
    percent_decode_str(s)
        .decode_utf8()
        .map(|cow| cow.into_owned())
        .map_err(|_| EpubError::BadHref(original.to_owned()))
}

/// True for `http:`, `mailto:`, `data:` ... (a `:` before any `/`).
fn has_scheme(s: &str) -> bool {
    match s.find(':') {
        Some(colon) => !s[..colon].contains('/'),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(from: &str, href: &str) -> Option<(String, Option<String>)> {
        Href::resolve(from, href).ok().map(|h| (h.path, h.fragment))
    }

    #[test]
    fn resolves_relative_paths_and_fragments() {
        assert_eq!(
            resolve("OEBPS/nav.xhtml", "text/ch%201.xhtml#s%202"),
            Some(("OEBPS/text/ch 1.xhtml".into(), Some("s 2".into())))
        );
        assert_eq!(
            resolve("OEBPS/text/a.xhtml", "../images/x.png"),
            Some(("OEBPS/images/x.png".into(), None))
        );
        assert_eq!(
            resolve("OEBPS/a.xhtml", "#top"),
            Some(("OEBPS/a.xhtml".into(), Some("top".into())))
        );
        assert_eq!(
            resolve("content.opf", "./b.xhtml"),
            Some(("b.xhtml".into(), None))
        );
    }

    #[test]
    fn rejects_escaping_and_remote_hrefs() {
        assert_eq!(resolve("OEBPS/a.xhtml", "../../etc/passwd"), None);
        assert_eq!(resolve("OEBPS/a.xhtml", "http://example.com/x"), None);
        assert_eq!(resolve("OEBPS/a.xhtml", "%FF.xhtml"), None);
    }
}
