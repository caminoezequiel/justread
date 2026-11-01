//! justread: a lightweight EPUB reader.
//!
//! Usage: `reader-app [book.epub]`. Without a book, the last one read is reopened.

mod app;
mod bridge;
mod progress;
mod protocol;
mod reader;

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    let shared = protocol::SharedBook::default();
    let startup = app::Startup {
        book: std::env::args_os().nth(1).map(PathBuf::from),
        progress_file: progress::progress_file(),
    };

    // The protocol handler gets its own clone of the shared book handle; the
    // `move` closure takes ownership of that clone.
    let handler_book = shared.clone();
    let config = Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title("justread")
                .with_inner_size(LogicalSize::new(1200.0, 850.0)),
        )
        .with_menu(None)
        .with_custom_head(format!("<style>{}</style>", include_str!("style.css")))
        .with_custom_protocol(protocol::SCHEME, move |_webview, request| {
            protocol::handle(&handler_book, &request)
        });

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(shared)
        .with_context(startup)
        .launch(app::App);
}
