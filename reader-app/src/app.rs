//! The Dioxus components.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use epub_core::{Book, EpubError, Href, TocEntry};

use crate::bridge::{Bridge, PageEvent};
use crate::protocol::{OpenBook, SharedBook};
use crate::reader::Reader;

/// Book passed on the command line, if any.
#[derive(Clone)]
pub struct StartupPath(pub Option<PathBuf>);

/// Source of unique book ids. An atomic integer can be updated through a
/// shared reference (`&`), which is all a `static` gives us.
static NEXT_BOOK_ID: AtomicU64 = AtomicU64::new(1);

#[component]
pub fn App() -> Element {
    let shared = use_context::<SharedBook>();
    let startup = use_context::<StartupPath>();
    // Signals are reactive state: reading one inside a component subscribes
    // it, and writing re-renders the subscribers. They are `Copy`, so closures
    // can capture them by value freely.
    let mut reader = use_signal(|| None::<Reader>);
    let mut error = use_signal(|| None::<String>);
    let bridge = use_hook(Bridge::start);

    // `use_callback` gives a `Copy` handle to a closure, so the same function
    // can be used from several event handlers below.
    let open = use_callback(move |path: PathBuf| match open_book(&shared, &path) {
        Ok(opened) => {
            set_window_title(opened.contents.metadata.title.as_deref());
            reader.set(Some(opened));
            error.set(None);
        }
        Err(e) => error.set(Some(format!(
            "Could not open {}: {}",
            path.display(),
            e.full_message()
        ))),
    });

    use_hook(move || {
        if let Some(path) = startup.0 {
            open(path);
        }
    });

    // Handle messages from the page for as long as the app runs.
    let events_bridge = bridge.clone();
    let scroll_bridge = bridge.clone();
    use_future(move || {
        let mut bridge = events_bridge.clone();
        let scroll_bridge = scroll_bridge.clone();
        async move {
            while let Some(event) = bridge.next_event().await {
                let mut guard = reader.write();
                let Some(r) = guard.as_mut() else {
                    continue;
                };
                let scroll = match event {
                    PageEvent::Turn { forward: true } => {
                        r.next();
                        None
                    }
                    PageEvent::Turn { forward: false } => {
                        r.turn_back();
                        None
                    }
                    PageEvent::Link { path, hash } => r.follow_link(&path, &hash),
                    PageEvent::Scroll { path, fraction } => {
                        r.on_scroll(&path, fraction);
                        None
                    }
                    PageEvent::Ready { path } => r.on_ready(&path),
                };
                if let Some(scroll) = scroll {
                    scroll_bridge.scroll(scroll);
                }
            }
        }
    });

    let pick_file = move |_| {
        // `spawn` runs an async task on the UI thread's executor; the dialog
        // doesn't block rendering while it is open.
        spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Open EPUB")
                .add_filter("EPUB books", &["epub"])
                .pick_file()
                .await;
            if let Some(file) = picked {
                open(file.path().to_path_buf());
            }
        });
    };

    // Clone a snapshot for this render, so no borrow of the signal is held
    // while the event handlers below write to it.
    let snapshot = reader();

    rsx! {
        div { class: "app",
            header { class: "toolbar",
                button { class: "open", onclick: pick_file, "Open EPUB" }
                if let Some(r) = &snapshot {
                    BookHeader { reader: r.clone() }
                    div { class: "nav",
                        button {
                            disabled: !r.has_prev(),
                            title: "Previous (←)",
                            onclick: move |_| {
                                if let Some(r) = reader.write().as_mut() {
                                    r.prev();
                                }
                            },
                            "‹ Prev"
                        }
                        button {
                            disabled: !r.has_next(),
                            title: "Next (→)",
                            onclick: move |_| {
                                if let Some(r) = reader.write().as_mut() {
                                    r.next();
                                }
                            },
                            "Next ›"
                        }
                    }
                }
            }
            if let Some(message) = error() {
                div { class: "error", role: "alert",
                    span { "{message}" }
                    button { onclick: move |_| error.set(None), "Dismiss" }
                }
            }
            match &snapshot {
                None => rsx! {
                    main { class: "empty",
                        p { "Open an EPUB to start reading." }
                        button { class: "open", onclick: pick_file, "Open EPUB" }
                    }
                },
                Some(r) => {
                    let current = r.contents.toc_entry_for(r.index).and_then(|e| e.target.clone());
                    let on_select = move |target: Href| {
                        let mut guard = reader.write();
                        let Some(r) = guard.as_mut() else { return };
                        match r.contents.spine_index_of(&target.path) {
                            Some(index) => {
                                if let Some(scroll) = r.go_to(index, target.fragment) {
                                    bridge.scroll(scroll);
                                }
                            }
                            None => error.set(Some(format!("{} is not part of the reading order", target.path))),
                        }
                    };
                    rsx! {
                        div { class: "body",
                            nav { class: "sidebar", aria_label: "Table of contents",
                                if r.contents.toc.is_empty() {
                                    p { class: "muted", "This book has no table of contents." }
                                } else {
                                    TocList { entries: r.contents.toc.clone(), current, on_select }
                                }
                            }
                            main { class: "page",
                                iframe { id: "chapter", title: "Chapter", src: r.chapter_url() }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Title, author, current chapter and progress.
#[component]
fn BookHeader(reader: Reader) -> Element {
    let meta = &reader.contents.metadata;
    let title = meta.title.as_deref().unwrap_or("Untitled");
    let authors = meta.authors.join(", ");
    let chapter = reader.chapter_title().unwrap_or("");
    let percent = (reader.progress() * 100.0).round();
    let position = format!("{} / {}", reader.index + 1, reader.contents.spine.len());

    rsx! {
        div { class: "book",
            div { class: "title",
                "{title}"
                if !authors.is_empty() {
                    span { class: "muted", " — {authors}" }
                }
            }
            div { class: "chapter", "{chapter}" }
        }
        div { class: "progress", title: "Section {position}",
            span { "{percent}%" }
            div { class: "bar", div { class: "fill", style: "width: {percent}%" } }
        }
    }
}

/// One level of the table of contents; nested levels render recursively.
#[component]
fn TocList(
    entries: Vec<TocEntry>,
    current: Option<Href>,
    on_select: EventHandler<Href>,
) -> Element {
    rsx! {
        ul {
            for (i, entry) in entries.into_iter().enumerate() {
                li { key: "{i}",
                    match entry.target.clone() {
                        Some(target) => {
                            let class = if current.as_ref() == Some(&target) { "current" } else { "" };
                            rsx! {
                                button {
                                    class,
                                    onclick: move |_| on_select.call(target.clone()),
                                    "{entry.label}"
                                }
                            }
                        }
                        None => rsx! { span { "{entry.label}" } },
                    }
                    if !entry.children.is_empty() {
                        TocList { entries: entry.children, current: current.clone(), on_select }
                    }
                }
            }
        }
    }
}

fn open_book(shared: &SharedBook, path: &Path) -> Result<Reader, EpubError> {
    let book = Book::open(path)?;
    for warning in book.warnings() {
        eprintln!("justread: {}: {warning}", path.display());
    }
    let id = NEXT_BOOK_ID.fetch_add(1, Ordering::Relaxed);
    let reader = Reader::new(id, book.contents().clone());
    *shared.lock() = Some(OpenBook { id, book });
    Ok(reader)
}

fn set_window_title(book_title: Option<&str>) {
    let title = match book_title {
        Some(t) => format!("{t} — justread"),
        None => "justread".to_owned(),
    };
    dioxus::desktop::window().set_title(&title);
}
