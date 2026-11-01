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
  if (f && e.source === f.contentWindow && e.data && e.data.justread) dioxus.send(e.data.justread);
});
addEventListener("keydown", (e) => {
  if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
  const f = frame();
  if (f && f.contentWindow && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
    e.preventDefault();
    f.contentWindow.postMessage({ justread: { kind: "page", forward: e.key === "ArrowRight" } }, "*");
  }
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
    Link {
        path: String,
        hash: String,
    },
    Scroll {
        path: String,
        fraction: f64,
    },
    Ready {
        path: String,
    },
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
        };
        self.send(command);
    }

    pub fn restyle(&self, version: u64) {
        self.send(Command::Restyle { version });
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
