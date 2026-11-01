//! Reading state of the open book and the navigation rules, kept free of UI
//! code so it can be unit tested.

use epub_core::Contents;

use crate::protocol;

#[derive(Debug, Clone, PartialEq)]
pub struct Reader {
    /// Changes on every open; part of every `epub://` URL.
    pub book_id: u64,
    pub contents: Contents,
    /// Current position in the spine.
    pub index: usize,
    /// How far the current document is scrolled, `0.0..=1.0`.
    pub fraction: f64,
    /// Where to scroll once the newly loaded document reports ready.
    pending: Option<Scroll>,
}

impl Reader {
    pub fn new(book_id: u64, contents: Contents) -> Self {
        Reader {
            book_id,
            contents,
            index: 0,
            fraction: 0.0,
            pending: None,
        }
    }

    pub fn chapter_url(&self) -> String {
        // `index` is always kept in range by the methods below, and the spine
        // is never empty (`Book::open` rejects that), so this lookup can't fail;
        // `map_or` still avoids a panic if that ever changed.
        self.contents
            .spine
            .get(self.index)
            .map_or_else(String::new, |item| protocol::url(self.book_id, &item.path))
    }

    pub fn has_next(&self) -> bool {
        self.index + 1 < self.contents.spine.len()
    }

    pub fn has_prev(&self) -> bool {
        self.index > 0
    }

    pub fn next(&mut self) {
        if self.has_next() {
            self.go_to(self.index + 1, None);
        }
    }

    pub fn prev(&mut self) {
        if self.has_prev() {
            self.go_to(self.index - 1, None);
        }
    }

    /// Page back past the start of a chapter: open the previous one at its
    /// end, so reading backwards is continuous.
    pub fn turn_back(&mut self) {
        if self.has_prev() {
            self.go_to(self.index - 1, None);
            self.pending = Some(Scroll::End);
        }
    }

    /// Move to spine item `index`, optionally at `fragment`.
    ///
    /// Returns `Some(scroll)` when the document is already loaded and only
    /// needs scrolling: the iframe won't reload, so the caller must send the
    /// scroll request itself. Otherwise the fragment is kept until
    /// [`Self::on_ready`].
    pub fn go_to(&mut self, index: usize, fragment: Option<String>) -> Option<Scroll> {
        if index >= self.contents.spine.len() {
            return None;
        }
        if index == self.index {
            self.pending = None;
            return Some(fragment.map_or(Scroll::Top, Scroll::Fragment));
        }
        self.index = index;
        self.fraction = 0.0;
        // A new document starts at the top by itself; only a fragment needs
        // a scroll once it has loaded.
        self.pending = fragment.map(Scroll::Fragment);
        None
    }

    /// Follow a link clicked inside the chapter (`url_path` is the URL's path,
    /// `hash` its `#fragment`, both still percent-encoded).
    pub fn follow_link(&mut self, url_path: &str, hash: &str) -> Option<Scroll> {
        let (book_id, path) = protocol::parse_url_path(url_path)?;
        if book_id != self.book_id {
            return None;
        }
        let index = self.contents.spine_index_of(&path)?;
        self.go_to(index, decode_fragment(hash))
    }

    /// The iframe finished loading `url_path`. Returns where to scroll, if
    /// that document is the current one and a scroll is waiting.
    pub fn on_ready(&mut self, url_path: &str) -> Option<Scroll> {
        let (book_id, path) = protocol::parse_url_path(url_path)?;
        let current = self.contents.spine.get(self.index)?;
        if book_id != self.book_id || path != current.path {
            return None;
        }
        self.pending.take()
    }

    /// The iframe scrolled. Events from a document that is no longer current
    /// (sent just before a chapter change) are ignored.
    pub fn on_scroll(&mut self, url_path: &str, fraction: f64) {
        let is_current = protocol::parse_url_path(url_path).is_some_and(|(book_id, path)| {
            book_id == self.book_id
                && self
                    .contents
                    .spine
                    .get(self.index)
                    .is_some_and(|item| item.path == path)
        });
        if is_current {
            self.fraction = fraction.clamp(0.0, 1.0);
        }
    }

    pub fn chapter_title(&self) -> Option<&str> {
        self.contents.chapter_title(self.index)
    }

    pub fn progress(&self) -> f64 {
        self.contents.progress(self.index, self.fraction)
    }
}

/// Where the chapter iframe should scroll to.
///
/// Unlike a C enum, Rust enum variants can carry data: `Fragment` holds the
/// id to scroll to, while `Top` and `End` need nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scroll {
    Top,
    Fragment(String),
    End,
}

fn decode_fragment(hash: &str) -> Option<String> {
    let raw = hash.strip_prefix('#').filter(|f| !f.is_empty())?;
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()?;
    Some(decoded.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use epub_core::{Metadata, SpineItem};

    fn reader() -> Reader {
        let item = |path: &str| SpineItem {
            id: path.into(),
            path: path.into(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            size: 100,
        };
        let contents = Contents {
            metadata: Metadata::default(),
            spine: vec![item("a.xhtml"), item("b c.xhtml")],
            toc: Vec::new(),
        };
        Reader::new(3, contents)
    }

    #[test]
    fn next_and_prev_stay_in_range() {
        let mut r = reader();
        r.prev();
        assert_eq!(r.index, 0);
        r.next();
        r.next();
        assert_eq!(r.index, 1);
        assert!(!r.has_next());
    }

    #[test]
    fn fragment_waits_for_ready_when_changing_document() {
        let mut r = reader();
        assert_eq!(r.follow_link("/3/b%20c.xhtml", "#n%201"), None);
        assert_eq!(r.index, 1);
        assert_eq!(r.on_ready("/3/a.xhtml"), None, "stale document");
        assert_eq!(
            r.on_ready("/3/b%20c.xhtml"),
            Some(Scroll::Fragment("n 1".into()))
        );
        assert_eq!(r.on_ready("/3/b%20c.xhtml"), None, "only once");
    }

    #[test]
    fn same_document_scrolls_immediately() {
        let mut r = reader();
        assert_eq!(
            r.go_to(0, Some("x".into())),
            Some(Scroll::Fragment("x".into()))
        );
        assert_eq!(r.follow_link("/3/a.xhtml", ""), Some(Scroll::Top));
    }

    #[test]
    fn paging_back_lands_at_the_end_of_the_previous_chapter() {
        let mut r = reader();
        r.turn_back();
        assert_eq!(r.index, 0, "nothing before the first chapter");
        r.next();
        assert_eq!(r.on_ready("/3/b%20c.xhtml"), None, "next starts at the top");
        r.turn_back();
        assert_eq!(r.index, 0);
        assert_eq!(r.on_ready("/3/a.xhtml"), Some(Scroll::End));
    }

    #[test]
    fn scroll_from_previous_document_is_ignored() {
        let mut r = reader();
        r.next();
        r.on_scroll("/3/a.xhtml", 0.9);
        assert_eq!(r.fraction, 0.0);
        r.on_scroll("/3/b%20c.xhtml", 0.5);
        assert_eq!(r.fraction, 0.5);
    }

    #[test]
    fn links_to_other_books_or_non_spine_files_are_ignored() {
        let mut r = reader();
        assert_eq!(r.follow_link("/2/b%20c.xhtml", ""), None);
        assert_eq!(r.follow_link("/3/image.png", ""), None);
        assert_eq!(r.index, 0);
    }
}
