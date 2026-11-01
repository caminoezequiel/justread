//! Reading state of the open book and the navigation rules, kept free of UI
//! code so it can be unit tested.

use epub_core::{Contents, Href, Position};

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
    /// The TOC section (fragment id) currently at the top of the screen, as
    /// reported by the chapter; `None` before its first section heading.
    pub section: Option<String>,
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
            section: None,
            pending: None,
        }
    }

    /// Continue from a saved position (ignored if it doesn't fit this book).
    pub fn restore(&mut self, position: Position) {
        if position.spine_index >= self.contents.spine.len() {
            return;
        }
        self.index = position.spine_index;
        self.fraction = position.fraction.clamp(0.0, 1.0);
        if self.fraction > 0.0 {
            self.pending = Some(Scroll::Fraction(self.fraction));
        }
    }

    /// The current position, for saving.
    pub fn position(&self) -> Position {
        Position {
            spine_index: self.index,
            fraction: self.fraction,
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
        self.section = None;
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
        if !self.is_current(url_path) {
            return None;
        }
        self.pending.take()
    }

    /// The iframe scrolled. Events from a document that is no longer current
    /// (sent just before a chapter change) are ignored.
    pub fn on_scroll(&mut self, url_path: &str, fraction: f64, section: Option<String>) {
        if self.is_current(url_path) {
            self.fraction = fraction.clamp(0.0, 1.0);
            self.section = section;
        }
    }

    /// Whether `url_path` (from the chapter iframe) is the current document.
    pub fn is_current(&self, url_path: &str) -> bool {
        protocol::parse_url_path(url_path).is_some_and(|(book_id, path)| {
            book_id == self.book_id
                && self
                    .contents
                    .spine
                    .get(self.index)
                    .is_some_and(|item| item.path == path)
        })
    }

    /// Section ids the chapter should report while scrolling (see `reader.js`).
    pub fn section_ids(&self) -> Vec<String> {
        self.contents.section_ids(self.index)
    }

    /// Target of the TOC entry to highlight: the current section, or the
    /// chapter if no section heading has been reached yet.
    pub fn current_toc_target(&self) -> Option<Href> {
        self.contents
            .toc_entry_at(self.index, self.section.as_deref())
            .and_then(|entry| entry.target.clone())
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
/// id to scroll to and `Fraction` a saved scroll position, while `Top` and
/// `End` need nothing. (No `Eq` derive: `f64` only supports `PartialEq`,
/// because NaN is not equal to itself.)
#[derive(Debug, Clone, PartialEq)]
pub enum Scroll {
    Top,
    Fragment(String),
    End,
    Fraction(f64),
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
    fn restoring_waits_for_the_document_then_scrolls_to_the_fraction() {
        let mut r = reader();
        r.restore(Position {
            spine_index: 1,
            fraction: 0.4,
        });
        assert_eq!(r.index, 1);
        assert_eq!(r.on_ready("/3/b%20c.xhtml"), Some(Scroll::Fraction(0.4)));
        assert_eq!(
            r.position(),
            Position {
                spine_index: 1,
                fraction: 0.4
            }
        );

        let mut r = reader();
        r.restore(Position {
            spine_index: 99,
            fraction: 0.5,
        });
        assert_eq!(r.index, 0, "out-of-range positions are ignored");
    }

    #[test]
    fn scroll_from_previous_document_is_ignored() {
        let mut r = reader();
        r.next();
        r.on_scroll("/3/a.xhtml", 0.9, Some("s".into()));
        assert_eq!(r.fraction, 0.0);
        assert_eq!(r.section, None);
        r.on_scroll("/3/b%20c.xhtml", 0.5, Some("s".into()));
        assert_eq!(r.section.as_deref(), Some("s"));
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
