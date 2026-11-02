//! The app side of the postMessage bridge; the chapter side is `reader.js`.
//!
//! One long-running `eval` script forwards messages from the chapter iframe
//! to Rust via `dioxus.send`, and forwards commands from Rust to the iframe.
//! Arrow keys pressed while the app window (not the chapter) has focus are
//! passed straight to the chapter, which knows whether to turn a page or ask
//! for the next chapter.

use dioxus::document::{Eval, EvalError, eval};
use serde::{Deserialize, Serialize};

use crate::reader::Scroll;

const BRIDGE_JS: &str = r#"
const frame = () => document.getElementById("chapter");
addEventListener("message", (e) => {
  const f = frame();
  if (!f || e.source !== f.contentWindow || !e.data || !e.data.justread) return;
  const msg = e.data.justread;
  // A word to translate: its place in the window, for positioning the popup.
  if (msg.kind === "lookup" && msg.rect) {
    const r = f.getBoundingClientRect();
    msg.rect = {
      left: msg.rect.left + r.left,
      top: msg.rect.top + r.top,
      right: msg.rect.right + r.left,
      bottom: msg.rect.bottom + r.top,
    };
    msg.viewport = { width: innerWidth, height: innerHeight };
  }
  dioxus.send(msg);
});
// Ctrl/Cmd + "+"/"=", "-", "0" (text size), Ctrl/Cmd+F (search), Escape
// and Alt+←/→ (back/forward); reader.js detects the same keys inside the
// chapter.
const shortcut = (e) => {
  if (e.key === "Escape") return "escape";
  if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
    if (e.key === "ArrowLeft") return "back";
    if (e.key === "ArrowRight") return "forward";
  }
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
  if (e.key === "+" || e.key === "=") return "bigger";
  if (e.key === "-" || e.key === "_") return "smaller";
  if (e.key === "0") return "reset_size";
  if (e.key === "f" || e.key === "F") return "find";
  return null;
};
addEventListener("keydown", (e) => {
  const action = shortcut(e);
  if (action) {
    e.preventDefault();
    dioxus.send({ kind: "shortcut", action });
    return;
  }
  // Arrow keys in a text field (e.g. the search box) edit the text.
  if (e.target.closest && e.target.closest("input, textarea, select")) return;
  if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
  const f = frame();
  if (f && f.contentWindow && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
    e.preventDefault();
    f.contentWindow.postMessage({ justread: { kind: "page", forward: e.key === "ArrowRight" } }, "*");
  }
});
addEventListener("mouseup", (e) => {
  if (e.button !== 3 && e.button !== 4) return;
  e.preventDefault();
  dioxus.send({ kind: "shortcut", action: e.button === 3 ? "back" : "forward" });
});
while (true) {
  const command = await dioxus.recv();
  const f = frame();
  if (f && f.contentWindow) f.contentWindow.postMessage({ justread: command }, "*");
}
"#;

/// Messages from the page. `#[serde(tag = "kind")]` maps the JS object
/// `{kind: "turn", forward: true}` to `PageEvent::Turn { forward: true }`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageEvent {
    /// Arrow key at the end (`forward`) or start of a chapter.
    Turn {
        forward: bool,
    },
    /// An in-book link was clicked in document `from`, scrolled to
    /// `fraction` with `section` at the top (as in `Scroll`).
    Link {
        path: String,
        hash: String,
        from: String,
        fraction: f64,
        #[serde(default)]
        section: Option<String>,
    },
    Scroll {
        path: String,
        fraction: f64,
        /// Id of the TOC section at the top of the screen, if any.
        #[serde(default)]
        section: Option<String>,
    },
    Ready {
        path: String,
    },
    /// The chapter couldn't locate a search hit it was asked to show.
    FindFailed,
    /// A keyboard shortcut, pressed in the app window or in the chapter.
    Shortcut {
        action: Shortcut,
    },
    /// A word was clicked or text selected: `text`, with the rest of its
    /// sentence before and after it, at `rect` in the app window.
    Lookup {
        text: String,
        before: String,
        after: String,
        rect: Rect,
        viewport: Viewport,
    },
    /// A click in the chapter that wasn't on a word.
    Dismiss,
}

/// A rectangle in CSS pixels of the app window.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shortcut {
    Bigger,
    Smaller,
    ResetSize,
    /// Ctrl/Cmd+F: focus the search box.
    Find,
    Escape,
    /// Alt+← or the mouse's back button: undo the last jump.
    Back,
    /// Alt+→ or the mouse's forward button.
    Forward,
}

/// Requests sent to the chapter; `kind` is what `reader.js` matches on.
#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
enum Command {
    /// To an element id, or to the top when `fragment` is `None`.
    #[serde(rename = "scroll_to")]
    Fragment { fragment: Option<String> },
    #[serde(rename = "scroll_to_end")]
    End,
    #[serde(rename = "scroll_to_fraction")]
    Fraction { fraction: f64 },
    /// Reload the reader stylesheet; `version` makes the URL unique.
    #[serde(rename = "restyle")]
    Restyle { version: u64 },
    /// Highlight and scroll to the `occurrence`-th match of `query`.
    #[serde(rename = "find")]
    Find { query: String, occurrence: usize },
    #[serde(rename = "clear_find")]
    ClearFind,
    /// Report which of these element ids is at the top while scrolling.
    #[serde(rename = "track_sections")]
    TrackSections { ids: Vec<String> },
    /// Highlight the text of the last `lookup`, or remove that highlight.
    #[serde(rename = "mark_lookup")]
    MarkLookup,
    #[serde(rename = "clear_lookup")]
    ClearLookup,
}

/// Handle to the bridge script. Cloning it gives another handle to the same
/// script (the `Eval` inside is a shared handle).
#[derive(Clone)]
pub struct Bridge(Eval);

impl Bridge {
    pub fn start() -> Self {
        Bridge(eval(BRIDGE_JS))
    }

    pub fn scroll(&self, scroll: Scroll) {
        let command = match scroll {
            Scroll::Top => Command::Fragment { fragment: None },
            Scroll::Fragment(id) => Command::Fragment { fragment: Some(id) },
            Scroll::End => Command::End,
            Scroll::Fraction(fraction) => Command::Fraction { fraction },
            Scroll::Match { query, occurrence } => Command::Find { query, occurrence },
        };
        self.send(command);
    }

    pub fn restyle(&self, version: u64) {
        self.send(Command::Restyle { version });
    }

    pub fn clear_find(&self) {
        self.send(Command::ClearFind);
    }

    pub fn track_sections(&self, ids: Vec<String>) {
        self.send(Command::TrackSections { ids });
    }

    pub fn mark_lookup(&self) {
        self.send(Command::MarkLookup);
    }

    pub fn clear_lookup(&self) {
        self.send(Command::ClearLookup);
    }

    fn send(&self, command: Command) {
        if let Err(e) = self.0.send(command) {
            eprintln!("justread: could not send to the chapter: {e}");
        }
    }

    /// Wait for the next message. `None` once the script has stopped.
    pub async fn next_event(&mut self) -> Option<PageEvent> {
        loop {
            match self.0.recv::<PageEvent>().await {
                Ok(event) => return Some(event),
                // A malformed message (e.g. posted by a script in the book):
                // skip it and keep listening.
                Err(EvalError::Serialization(e)) => eprintln!("justread: ignored message: {e}"),
                Err(e) => {
                    eprintln!("justread: bridge stopped: {e}");
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_messages_deserialize() -> Result<(), serde_json::Error> {
        let event: PageEvent = serde_json::from_str(
            r#"{"kind": "lookup", "text": "ran", "before": "He ", "after": " out.",
                "rect": {"left": 1, "top": 2, "right": 3, "bottom": 4},
                "viewport": {"width": 800, "height": 600}}"#,
        )?;
        let PageEvent::Lookup { text, rect, .. } = event else {
            panic!("not a lookup: {event:?}");
        };
        assert_eq!(text, "ran");
        assert_eq!(rect.bottom, 4.0);
        assert!(matches!(
            serde_json::from_str(r#"{"kind": "dismiss"}"#)?,
            PageEvent::Dismiss
        ));
        Ok(())
    }
}
