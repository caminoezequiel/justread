//! justread: a lightweight EPUB reader.
//!
//! Usage: `reader-app [book.epub]`. Without a book, the last one read is reopened.

mod app;
mod bridge;
mod preferences;
mod protocol;
mod reader;
mod storage;

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    let shared = protocol::SharedBook::default();
    let data_dir = storage::data_dir();
    let preferences_file = data_dir.as_ref().map(|dir| dir.join("preferences.json"));
    let preferences = preferences_file
        .as_deref()
        .map(storage::load_preferences)
        .unwrap_or_default();
    let style = protocol::SharedStyle::new(preferences.chapter_css());
    let startup = app::Startup {
        book: std::env::args_os().nth(1).map(PathBuf::from),
        progress_file: data_dir.as_ref().map(|dir| dir.join("progress.json")),
        preferences_file,
        preferences,
    };

    // The protocol handler gets its own clones of the shared handles; the
    // `move` closure takes ownership of those clones.
    let handler_book = shared.clone();
    let handler_style = style.clone();
    let config = Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title("justread")
                .with_inner_size(LogicalSize::new(1200.0, 850.0)),
        )
        .with_menu(None)
        .with_custom_head(format!("<style>{}</style>", include_str!("style.css")))
        .with_custom_protocol(protocol::SCHEME, move |_webview, request| {
            protocol::handle(&handler_book, &handler_style, &request)
        });

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(shared)
        .with_context(style)
        .with_context(startup)
        .launch(app::App);
}
