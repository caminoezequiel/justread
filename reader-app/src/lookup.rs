//! The translation popup for a clicked word or a selection: offline
//! dictionary entries, the sentence, and buttons that open Google Translate.

use dioxus::prelude::*;

use crate::bridge::{Rect, Viewport};
use crate::dictionary::{Dictionary, Entry};
use crate::google::GoogleRequests;

/// What was clicked and what the dictionary says about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Lookup {
    pub text: String,
    /// The rest of the sentence around `text`.
    pub before: String,
    pub after: String,
    pub entries: Vec<Entry>,
    pub rect: Rect,
    pub viewport: Viewport,
}

impl Lookup {
    pub fn new(
        text: String,
        before: String,
        after: String,
        rect: Rect,
        viewport: Viewport,
    ) -> Self {
        let entries = Dictionary::shared()
            .map(|d| d.lookup(&text, &after))
            .unwrap_or_default();
        Lookup {
            text,
            before,
            after,
            entries,
            rect,
            viewport,
        }
    }

    pub fn sentence(&self) -> String {
        format!("{}{}{}", self.before, self.text, self.after)
            .trim()
            .to_owned()
    }
}

/// Senses listed before "N more"; Wiktionary has dozens for common words.
const FIRST_SENSES: usize = 4;

/// Popup width in CSS pixels (style.css uses the same value).
const WIDTH: f64 = 360.0;
/// Space between the popup and the word or the window's edge.
const GAP: f64 = 8.0;

/// Inline style placing the popup below the word, or above it when the word
/// is low in the window, without leaving the window.
pub fn popup_style(rect: Rect, viewport: Viewport) -> String {
    let left = rect.left.min(viewport.width - WIDTH - GAP).max(GAP).round();
    if rect.bottom < viewport.height * 0.6 {
        let top = (rect.bottom + GAP).round();
        let max = (viewport.height - top - GAP).round();
        format!("left: {left}px; top: {top}px; max-height: {max}px;")
    } else {
        let bottom = (viewport.height - rect.top + GAP).round();
        let max = (rect.top - 2.0 * GAP).round();
        format!("left: {left}px; bottom: {bottom}px; max-height: {max}px;")
    }
}

#[component]
pub fn LookupPopup(lookup: Lookup, on_close: EventHandler<()>) -> Element {
    let google = use_context::<GoogleRequests>();
    let sentence = lookup.sentence();
    let has_sentence = sentence != lookup.text;
    let text = lookup.text.clone();
    let sentence_google = google.clone();

    rsx! {
        div {
            class: "lookup",
            role: "dialog",
            aria_label: "Translation",
            style: popup_style(lookup.rect, lookup.viewport),
            div { class: "lookup-head",
                span { class: "query", "{lookup.text}" }
                button { title: "Close (Esc)", onclick: move |_| on_close.call(()), "✕" }
            }
            if lookup.entries.is_empty() {
                p { class: "muted", "No offline entry for “{lookup.text}”." }
            }
            for entry in lookup.entries {
                EntryView { key: "{entry.headword}", entry }
            }
            if has_sentence {
                p { class: "sentence",
                    "{lookup.before}"
                    strong { "{lookup.text}" }
                    "{lookup.after}"
                }
            }
            p { class: "credit", "Dictionary: Wiktionary (CC BY-SA)" }
            div { class: "actions",
                if has_sentence {
                    button {
                        title: "Open the whole sentence in Google Translate",
                        onclick: move |_| sentence_google.open(&sentence),
                        "Translate sentence ↗"
                    }
                }
                button {
                    title: "Open this text in Google Translate",
                    onclick: move |_| google.open(&text),
                    if text.contains(' ') { "Translate selection ↗" } else { "Translate word ↗" }
                }
            }
        }
    }
}

/// One headword with its first senses, and a button for the rest.
#[component]
fn EntryView(entry: Entry) -> Element {
    let mut expanded = use_signal(|| false);
    let hidden = entry.senses.len().saturating_sub(FIRST_SENSES);
    let shown = if expanded() {
        entry.senses.len()
    } else {
        FIRST_SENSES
    };
    rsx! {
        div { class: "entry",
            div { class: "headword", "{entry.headword}" }
            ol {
                for (i, sense) in entry.senses.into_iter().take(shown).enumerate() {
                    li { key: "{i}",
                        span { class: "spanish", {sense.spanish.join(", ")} }
                        " "
                        span { class: "pos", "{sense.pos}" }
                        " "
                        span { class: "sense", "{sense.sense}" }
                    }
                }
            }
            if hidden > 0 && !expanded() {
                button { class: "more", onclick: move |_| expanded.set(true), "{hidden} more" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: Viewport = Viewport {
        width: 1000.0,
        height: 800.0,
    };

    fn rect(left: f64, top: f64) -> Rect {
        Rect {
            left,
            top,
            right: left + 40.0,
            bottom: top + 20.0,
        }
    }

    #[test]
    fn below_a_word_high_on_the_screen() {
        assert_eq!(
            popup_style(rect(100.0, 100.0), VIEW),
            "left: 100px; top: 128px; max-height: 664px;"
        );
    }

    #[test]
    fn above_a_word_low_on_the_screen() {
        assert_eq!(
            popup_style(rect(100.0, 700.0), VIEW),
            "left: 100px; bottom: 108px; max-height: 684px;"
        );
    }

    #[test]
    fn stays_inside_the_window() {
        assert!(popup_style(rect(950.0, 100.0), VIEW).starts_with("left: 632px;"));
        assert!(popup_style(rect(-20.0, 100.0), VIEW).starts_with("left: 8px;"));
    }

    #[test]
    fn the_sentence_joins_the_parts() {
        let lookup = Lookup {
            text: "ran".into(),
            before: "He ".into(),
            after: " out of time. ".into(),
            entries: Vec::new(),
            rect: rect(0.0, 0.0),
            viewport: VIEW,
        };
        assert_eq!(lookup.sentence(), "He ran out of time.");
    }
}
