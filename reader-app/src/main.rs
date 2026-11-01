//! justread: a lightweight EPUB reader.
//!
//! Usage: `justread [book.epub]`. Without a book, the last one read is reopened.

// On Windows, release builds are GUI programs: no console window opens next
// to the app. (`cfg_attr` applies an attribute only when the condition holds.)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod bridge;
mod preferences;
mod protocol;
mod reader;
mod storage;

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder, icon_from_memory};

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
    let mut config = Config::new()
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
    // Window/taskbar icon. Installed bundles also carry the icons from
    // Dioxus.toml; this covers `cargo run` and X11/Windows title bars.
    match icon_from_memory(include_bytes!("../icons/256x256.png")) {
        Ok(icon) => config = config.with_icon(icon),
        Err(e) => eprintln!("justread: could not load the window icon: {e}"),
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(shared)
        .with_context(style)
        .with_context(startup)
        .launch(app::App);
}
