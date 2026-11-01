//! The app side of the postMessage bridge; the chapter side is `reader.js`.
//!
//! One long-running `eval` script forwards messages from the chapter iframe
//! (and arrow keys pressed while the app window has focus) to Rust via
//! `dioxus.send`, and forwards commands from Rust to the iframe.

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
  if (e.altKey || e.ctrlKey || e.metaKey) return;
  if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
    e.preventDefault();
    dioxus.send({ kind: "key", key: e.key });
  }
});
while (true) {
  const command = await dioxus.recv();
  const f = frame();
  if (f && f.contentWindow) f.contentWindow.postMessage({ justread: command }, "*");
}
"#;

/// Messages from the page. `#[serde(tag = "kind")]` maps the JS object
/// `{kind: "key", key: "ArrowLeft"}` to `PageEvent::Key { key: .. }`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageEvent {
    Key { key: String },
    Link { path: String, hash: String },
    Scroll { path: String, fraction: f64 },
    Ready { path: String },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Command {
    ScrollTo { fragment: Option<String> },
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
        let command = Command::ScrollTo {
            fragment: scroll.fragment,
        };
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
