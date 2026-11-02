//! Reading state of the open book and the navigation rules, kept free of UI
//! code so it can be unit tested.

use epub_core::{Contents, Href, Position};

use crate::history::History;
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
    /// The page being read in a PDF, as its viewer reports it.
    pdf_page: Option<PageNumber>,
    /// The last page anchor the current chapter scrolled past, with the
    /// spine index it belongs to (see `Contents::page_number`).
    page_anchor: Option<(usize, String)>,
    /// Where to scroll once the newly loaded document reports ready.
    pending: Option<Scroll>,
    /// Places left by jumps, for Back/Forward.
    history: History,
    /// A search hit has been shown since the search started; showing more
    /// hits doesn't record more places, so Back returns to where you were
    /// reading before the search.
    in_search: bool,
}

impl Reader {
    pub fn new(book_id: u64, contents: Contents) -> Self {
        Reader {
            book_id,
            contents,
            index: 0,
            fraction: 0.0,
            section: None,
            pdf_page: None,
            page_anchor: None,
            pending: None,
            history: History::default(),
            in_search: false,
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

    /// Like [`Self::go_to`], but remembers the current place for
    /// [`Self::go_back`]: for jumps (links, TOC entries), not for reading on.
    pub fn jump(&mut self, index: usize, fragment: Option<String>) -> Option<Scroll> {
        if index >= self.contents.spine.len() {
            return None;
        }
        self.history.record(self.position());
        self.in_search = false;
        self.go_to(index, fragment)
    }

    /// Show search hit `occurrence` of `query` in spine item `index`: like
    /// [`Self::go_to`], but scrolling to (and highlighting) the match. The
    /// first hit shown in a search is a jump (see [`Self::end_search`]).
    pub fn show_match(&mut self, index: usize, query: &str, occurrence: usize) -> Option<Scroll> {
        let scroll = Scroll::Match {
            query: query.to_owned(),
            occurrence,
        };
        if !self.in_search {
            self.history.record(self.position());
            self.in_search = true;
        }
        if index == self.index {
            self.pending = None;
            return Some(scroll);
        }
        self.go_to(index, None);
        self.pending = Some(scroll);
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
        self.jump(index, decode_fragment(hash))
    }

    /// The search was closed or replaced: its next hit is a new jump.
    pub fn end_search(&mut self) {
        self.in_search = false;
    }

    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.history.can_go_forward()
    }

    /// Return to the place the last jump left. Like [`Self::go_to`], returns
    /// the scroll to send now if the document is already loaded.
    pub fn go_back(&mut self) -> Option<Scroll> {
        let to = self.history.back(self.position())?;
        self.in_search = false;
        self.go_to_position(to)
    }

    /// Undo a [`Self::go_back`].
    pub fn go_forward(&mut self) -> Option<Scroll> {
        let to = self.history.forward(self.position())?;
        self.in_search = false;
        self.go_to_position(to)
    }

    fn go_to_position(&mut self, to: Position) -> Option<Scroll> {
        if to.spine_index >= self.contents.spine.len() {
            return None;
        }
        let scroll = Scroll::Fraction(to.fraction);
        let now = if to.spine_index == self.index {
            self.pending = None;
            Some(scroll)
        } else {
            self.go_to(to.spine_index, None);
            self.pending = Some(scroll);
            None
        };
        // Known already, so progress is right before the chapter reports in.
        self.fraction = to.fraction;
        now
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

    /// A PDF page came to the middle of the window. Ignored for documents
    /// that aren't current, like `on_scroll`.
    pub fn on_page(&mut self, url_path: &str, number: usize, count: usize) {
        if self.is_current(url_path) {
            self.pdf_page = Some(PageNumber { number, count });
        }
    }

    /// The chapter scrolled past page anchor `anchor` (`None`: none yet).
    pub fn on_page_anchor(&mut self, url_path: &str, anchor: Option<String>) {
        if self.is_current(url_path) {
            self.page_anchor = anchor.map(|id| (self.index, id));
        }
    }

    /// The page being read: from the PDF viewer, or from the book's page list.
    /// `None` for an EPUB without a page list (see `estimated_page`).
    pub fn page(&self) -> Option<PageNumber> {
        if let Some(page) = self.pdf_page {
            return Some(page);
        }
        let anchor = self
            .page_anchor
            .as_ref()
            .filter(|(index, _)| *index == self.index)
            .map(|(_, id)| id.as_str());
        let number = self.contents.page_number(self.index, anchor)?;
        Some(PageNumber {
            number,
            count: self.contents.pages.len(),
        })
    }

    /// Page estimate for a book without a page list, from how far through
    /// the book the reader is.
    pub fn estimated_page(&self) -> PageNumber {
        let bytes: u64 = self.contents.spine.iter().map(|item| item.size).sum();
        let count = usize::try_from(bytes.div_ceil(BYTES_PER_PAGE))
            .unwrap_or(usize::MAX)
            .max(1);
        let number = (self.progress() * count as f64).ceil() as usize;
        PageNumber {
            number: number.clamp(1, count),
            count,
        }
    }

    /// Page anchors of the current chapter, for the chapter to report.
    pub fn page_ids(&self) -> Vec<String> {
        self.contents.page_ids(self.index)
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

/// A printed page of an EPUB is about this many bytes of XHTML; used only to
/// estimate pages in books without a page list.
const BYTES_PER_PAGE: u64 = 2000;

/// A printed page `number` (1-based) out of `count` pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageNumber {
    pub number: usize,
    pub count: usize,
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
    /// The `occurrence`-th search match of `query` (see `epub_core::SearchHit`).
    Match {
        query: String,
        occurrence: usize,
    },
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
            pages: Vec::new(),
        };
        Reader::new(3, contents)
    }

    #[test]
    fn page_is_taken_only_from_the_current_document() {
        let mut r = reader();
        r.on_page("/4/a.xhtml", 5, 340);
        assert_eq!(r.page(), None);
        r.on_page("/3/a.xhtml", 5, 340);
        assert_eq!(
            r.page(),
            Some(PageNumber {
                number: 5,
                count: 340
            })
        );
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
    fn search_matches_scroll_now_or_after_loading() {
        let mut r = reader();
        let hit = |occurrence| Scroll::Match {
            query: "q".into(),
            occurrence,
        };
        assert_eq!(r.show_match(0, "q", 2), Some(hit(2)), "same document");
        assert_eq!(r.show_match(1, "q", 0), None);
        assert_eq!(r.index, 1);
        assert_eq!(r.on_ready("/3/b%20c.xhtml"), Some(hit(0)));
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
        assert!(!r.can_go_back(), "nothing to go back to");
    }

    #[test]
    fn back_returns_to_where_a_link_was_followed() {
        let mut r = reader();
        r.on_scroll("/3/a.xhtml", 0.3, None);
        r.follow_link("/3/b%20c.xhtml", "#note");
        r.on_ready("/3/b%20c.xhtml");
        r.on_scroll("/3/b%20c.xhtml", 0.8, None);

        assert_eq!(r.go_back(), None, "other document: wait for it to load");
        assert_eq!(r.index, 0);
        assert_eq!(r.progress(), r.contents.progress(0, 0.3));
        assert_eq!(r.on_ready("/3/a.xhtml"), Some(Scroll::Fraction(0.3)));
        assert!(!r.can_go_back());

        assert_eq!(r.go_forward(), None);
        assert_eq!(r.index, 1);
        assert_eq!(r.on_ready("/3/b%20c.xhtml"), Some(Scroll::Fraction(0.8)));
    }

    #[test]
    fn back_within_a_document_scrolls_immediately() {
        let mut r = reader();
        r.on_scroll("/3/a.xhtml", 0.3, None);
        assert_eq!(
            r.follow_link("/3/a.xhtml", "#n1"),
            Some(Scroll::Fragment("n1".into()))
        );
        assert_eq!(r.go_back(), Some(Scroll::Fraction(0.3)));
        assert_eq!(r.on_ready("/3/a.xhtml"), None, "nothing left pending");
    }

    #[test]
    fn reading_on_is_not_a_jump() {
        let mut r = reader();
        r.next();
        r.turn_back();
        r.go_to(1, None);
        assert!(!r.can_go_back());
        r.jump(0, None);
        assert!(r.can_go_back(), "TOC entries are jumps");
    }

    #[test]
    fn browsing_search_hits_records_one_place() {
        let mut r = reader();
        r.on_scroll("/3/a.xhtml", 0.3, None);
        r.show_match(0, "q", 0);
        r.show_match(1, "q", 0);
        r.show_match(1, "q", 1);
        r.go_back();
        assert_eq!(r.position().spine_index, 0);
        assert_eq!(r.fraction, 0.3, "back to before the search");
        assert!(!r.can_go_back());

        r.end_search();
        r.show_match(1, "q", 0);
        assert!(r.can_go_back(), "a new search records again");
    }
}
