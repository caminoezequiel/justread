//! Google Translate in a window of its own, for translating whole sentences.
//!
//! Google refuses to be shown in an iframe (`X-Frame-Options: SAMEORIGIN`),
//! and Dioxus windows send every http(s) navigation to the system browser, so
//! this is a plain tao window with its own wry webview, outside Dioxus. Tao
//! creates windows only from the event loop, so the popup asks for a page by
//! filling [`GoogleRequests`] and [`GoogleWindow::handle_event`], called by
//! `main.rs` for every event, opens it. The window is created once and then
//! reused; closing it only hides it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Poll;

use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::tao::event_loop::EventLoopWindowTarget;
use dioxus::desktop::tao::window::{Window, WindowBuilder};
use dioxus::desktop::wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder};
use dioxus::desktop::{LogicalSize, wry};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

/// Longest text sent to Google (its own limit is 5000 characters).
const MAX_CHARS: usize = 5000;

/// The Google Translate page for `text`, English to Spanish.
pub fn translate_url(text: &str) -> String {
    let text: String = text.chars().take(MAX_CHARS).collect();
    format!(
        "https://translate.google.com/?sl=en&tl=es&op=translate&text={}",
        utf8_percent_encode(&text, NON_ALPHANUMERIC)
    )
}

/// The page the window should show next, set from the UI. Dioxus contexts
/// must be shareable between threads, hence `Arc<Mutex<…>>` even though
/// everything here runs on the main thread.
#[derive(Clone, Default)]
pub struct GoogleRequests(Arc<Mutex<Option<String>>>);

impl GoogleRequests {
    /// Ask for `text` to be shown. The event loop sleeps until something
    /// happens, so also give it something: a task that wakes itself once.
    /// Waking a Dioxus task sends the loop an event, and the handler then
    /// opens the page. (`poll_fn` turns a closure into a `Future`; `Pending`
    /// means "not done yet, poll me again once woken".)
    pub fn open(&self, text: &str) {
        *self.lock() = Some(translate_url(text));
        let mut woken = false;
        dioxus::prelude::spawn(std::future::poll_fn(move |cx| {
            if woken {
                return Poll::Ready(());
            }
            woken = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }));
    }

    fn take(&self) -> Option<String> {
        self.lock().take()
    }

    /// A panic while holding the lock "poisons" it; the `Option` inside is
    /// still fine to use, so take it anyway.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The window, once it exists. Owned by the event handler in `main.rs`.
pub struct GoogleWindow {
    requests: GoogleRequests,
    /// Browser data (cookies, so Google's consent choice is remembered).
    data_dir: Option<PathBuf>,
    // Tuple elements are dropped in order: the webview goes before the
    // window it lives in and its context.
    open: Option<(WebView, Window, WebContext)>,
}

impl GoogleWindow {
    pub fn new(requests: GoogleRequests, data_dir: Option<PathBuf>) -> Self {
        GoogleWindow {
            requests,
            data_dir,
            open: None,
        }
    }

    /// Show a pending request, and hide the window when it is closed.
    pub fn handle_event<T>(&mut self, event: &Event<T>, target: &EventLoopWindowTarget<T>) {
        if let Event::WindowEvent {
            window_id,
            event: WindowEvent::CloseRequested,
            ..
        } = event
            && let Some((_, window, _)) = &self.open
            && window.id() == *window_id
        {
            window.set_visible(false);
        }
        if let Some(url) = self.requests.take()
            && let Err(e) = self.show(&url, target)
        {
            eprintln!("justread: could not open Google Translate: {e}");
        }
    }

    fn show<T>(&mut self, url: &str, target: &EventLoopWindowTarget<T>) -> Result<(), String> {
        if let Some((webview, window, _)) = &self.open {
            webview.load_url(url).map_err(|e| e.to_string())?;
            window.set_visible(true);
            window.set_focus();
            return Ok(());
        }
        let window = WindowBuilder::new()
            .with_title("Google Translate — justread")
            .with_inner_size(LogicalSize::new(900.0, 650.0))
            .build(target)
            .map_err(|e| e.to_string())?;
        let mut context = WebContext::new(self.data_dir.clone());
        let webview = build(
            WebViewBuilder::new_with_web_context(&mut context)
                .with_url(url)
                // Links that would open yet another window stay closed.
                .with_new_window_req_handler(|_, _| NewWindowResponse::Deny),
            &window,
        )
        .map_err(|e| e.to_string())?;
        self.open = Some((webview, window, context));
        Ok(())
    }
}

/// Put the webview in the window: GTK on Linux (like Dioxus itself does),
/// the native window handle elsewhere.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn build(builder: WebViewBuilder, window: &Window) -> wry::Result<WebView> {
    builder.build(window)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn build(builder: WebViewBuilder, window: &Window) -> wry::Result<WebView> {
    use dioxus::desktop::tao::platform::unix::WindowExtUnix;
    use dioxus::desktop::wry::WebViewBuilderExtUnix;
    match window.default_vbox() {
        Some(vbox) => builder.build_gtk(vbox),
        None => Err(wry::Error::UnsupportedWindowHandle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encodes_the_text() {
        assert_eq!(
            translate_url("He ran out of time & money."),
            "https://translate.google.com/?sl=en&tl=es&op=translate\
             &text=He%20ran%20out%20of%20time%20%26%20money%2E"
        );
    }

    #[test]
    fn long_text_is_cut() {
        let url = translate_url(&"a".repeat(6000));
        assert_eq!(url.len(), translate_url("").len() + MAX_CHARS);
    }
}
