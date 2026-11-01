//! Full-text search over a book's spine documents.
//!
//! A hit is identified by (spine index, occurrence): "the 3rd match in this
//! document". The reader app's chapter script (`reader.js`) finds that same
//! occurrence in the rendered page, so both sides must see the same text and
//! normalize it the same way:
//!
//! - text = all text nodes under `<body>`, in document order, except inside
//!   `<script>` and `<style>`;
//! - runs of [`is_space`] characters count as one space;
//! - each character is lowercased when that gives exactly one character;
//! - matches are counted left to right without overlapping.

use crate::xml;

/// One match, with some surrounding text for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub spine_index: usize,
    /// 0 for the first match in that document, 1 for the second, ...
    pub occurrence: usize,
    /// Text just before the match (whitespace collapsed, may start with "…").
    pub before: String,
    /// The matched text as it appears in the book.
    pub matched: String,
    /// Text just after the match (may end with "…").
    pub after: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// `true` if there were more hits than the limit passed to `search`.
    pub truncated: bool,
    /// Spine documents that couldn't be searched (missing, not UTF-8, not
    /// well-formed XML), as zip paths.
    pub skipped: Vec<String>,
}

const CONTEXT_BEFORE: usize = 40;
const CONTEXT_AFTER: usize = 60;

/// Whitespace for search purposes. Deliberately an explicit list (also in
/// `reader.js`) rather than `char::is_whitespace`, whose definition differs
/// from JavaScript's `\s`.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '\u{0C}' | '\u{A0}' | '\u{2000}'..='\u{200A}' | '\u{3000}'
    )
}

/// Lowercase `c` when that yields exactly one character (so positions in
/// the folded text still map one-to-one onto the original).
pub fn fold_char(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// Text folded for matching, plus, for each folded character, the index of
/// the original character it came from.
struct Folded {
    chars: Vec<char>,
    origin: Vec<usize>,
}

fn fold(text: &[char]) -> Folded {
    let mut chars = Vec::with_capacity(text.len());
    let mut origin = Vec::with_capacity(text.len());
    let mut in_space = false;
    for (i, &c) in text.iter().enumerate() {
        if is_space(c) {
            if !in_space {
                chars.push(' ');
                origin.push(i);
            }
            in_space = true;
        } else {
            chars.push(fold_char(c));
            origin.push(i);
            in_space = false;
        }
    }
    Folded { chars, origin }
}

/// Fold a query: like the text, plus leading/trailing whitespace removed.
fn fold_query(query: &str) -> Vec<char> {
    let chars: Vec<char> = query.chars().collect();
    let mut folded = fold(&chars).chars;
    while folded.first() == Some(&' ') {
        folded.remove(0);
    }
    while folded.last() == Some(&' ') {
        folded.pop();
    }
    folded
}

/// Start positions of non-overlapping matches of `needle` in `haystack`.
fn find_all(haystack: &[char], needle: &[char]) -> Vec<usize> {
    let mut starts = Vec::new();
    if needle.is_empty() || needle.len() > haystack.len() {
        return starts;
    }
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if haystack[i..i + needle.len()] == *needle {
            starts.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    starts
}

/// The searchable text of an XHTML document (see the module docs), or
/// `None` if it isn't well-formed XML.
pub(crate) fn body_text(xhtml: &str) -> Option<String> {
    let doc = xml::parse(xhtml, "").ok()?;
    let body = doc.descendants().find(|n| xml::is(*n, "body"))?;
    let skipped = |n: roxmltree::Node| {
        n.ancestors()
            .any(|a| xml::is(a, "script") || xml::is(a, "style"))
    };
    Some(
        body.descendants()
            .filter(|n| n.is_text() && !skipped(*n))
            .filter_map(|n| n.text())
            .collect(),
    )
}

/// All hits of `query` in one document's `text`, for spine item `spine_index`.
pub(crate) fn search_text(text: &str, query: &str, spine_index: usize) -> Vec<SearchHit> {
    let needle = fold_query(query);
    let original: Vec<char> = text.chars().collect();
    let folded = fold(&original);
    find_all(&folded.chars, &needle)
        .into_iter()
        .enumerate()
        .map(|(occurrence, start)| {
            let first = folded.origin[start];
            let last = folded.origin[start + needle.len() - 1];
            SearchHit {
                spine_index,
                occurrence,
                before: snippet(&original, first.saturating_sub(CONTEXT_BEFORE), first, true),
                matched: snippet(&original, first, last + 1, false),
                after: snippet(
                    &original,
                    last + 1,
                    (last + 1 + CONTEXT_AFTER).min(original.len()),
                    false,
                ),
            }
        })
        .collect()
}

/// `text[from..to]` with whitespace runs collapsed, and "…" where it was cut.
fn snippet(text: &[char], from: usize, to: usize, is_before: bool) -> String {
    let mut out = String::new();
    if is_before && from > 0 {
        out.push('…');
    }
    let mut in_space = false;
    for &c in &text[from..to] {
        if is_space(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    if !is_before && to < text.len() && from != to && to - from >= CONTEXT_AFTER {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_ignores_case_and_whitespace_runs() {
        let hits = search_text("The  Quick\n\tBROWN fox", "quick brown", 4);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched, "Quick BROWN");
        assert_eq!(hits[0].spine_index, 4);
        assert_eq!(hits[0].before, "The ");
    }

    #[test]
    fn matches_do_not_overlap_and_are_numbered() {
        let hits = search_text("aaaa", "aa", 0);
        let occurrences: Vec<usize> = hits.iter().map(|h| h.occurrence).collect();
        assert_eq!(occurrences, [0, 1]);
    }

    #[test]
    fn blank_query_finds_nothing() {
        assert!(search_text("anything", "   ", 0).is_empty());
    }

    #[test]
    fn body_text_skips_head_script_and_style() {
        let xhtml = "<html><head><title>T</title></head><body><p>a<b>b</b></p>\
                     <script>x</script><style>y</style><p>c</p></body></html>";
        assert_eq!(body_text(xhtml).as_deref(), Some("abc"));
        assert_eq!(body_text("<p>not closed"), None);
    }
}
