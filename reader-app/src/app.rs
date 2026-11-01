//! The Dioxus components.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use epub_core::{Book, BookKey, EpubError, Href, SearchHit, TocEntry};

use crate::bridge::{Bridge, PageEvent, Shortcut};
use crate::preferences::{LineWidth, MAX_FONT_SCALE, MIN_FONT_SCALE, Preferences, Theme};
use crate::protocol::{OpenBook, SharedBook, SharedStyle};
use crate::reader::Reader;
use crate::storage;

/// Settings decided in `main` before the window opens.
#[derive(Clone)]
pub struct Startup {
    /// Book passed on the command line, if any.
    pub book: Option<PathBuf>,
    /// Where reading progress is kept (`None`: nowhere, don't save).
    pub progress_file: Option<PathBuf>,
    /// Where preferences are kept, and their value at launch.
    pub preferences_file: Option<PathBuf>,
    pub preferences: Preferences,
}

/// Which file the open book came from, for saving its progress.
#[derive(Clone, PartialEq)]
struct OpenedFrom {
    key: BookKey,
    path: PathBuf,
}

/// A finished search: what was searched and what was found.
#[derive(Clone, PartialEq)]
struct SearchState {
    query: String,
    /// Each hit with the title of the chapter it is in.
    hits: Vec<(SearchHit, String)>,
    truncated: bool,
}

/// At most this many search hits are listed.
const SEARCH_LIMIT: usize = 300;

/// Source of unique book ids. An atomic integer can be updated through a
/// shared reference (`&`), which is all a `static` gives us.
static NEXT_BOOK_ID: AtomicU64 = AtomicU64::new(1);

#[component]
pub fn App() -> Element {
    let shared = use_context::<SharedBook>();
    let startup = use_context::<Startup>();
    // Signals are reactive state: reading one inside a component subscribes
    // it, and writing re-renders the subscribers. They are `Copy`, so closures
    // can capture them by value freely.
    let mut reader = use_signal(|| None::<Reader>);
    let mut error = use_signal(|| None::<String>);
    let mut opened_from = use_signal(|| None::<OpenedFrom>);
    let progress_file = startup.progress_file.clone();
    let mut store = use_signal(move || storage::load_progress(progress_file));
    let mut preferences = use_signal(|| startup.preferences);
    let mut settings_open = use_signal(|| false);
    let mut restyles = use_signal(|| 0_u64);
    let mut search_query = use_signal(String::new);
    let mut search = use_signal(|| None::<SearchState>);
    let mut selected_hit = use_signal(|| None::<usize>);
    let style = use_context::<SharedStyle>();
    let bridge = use_hook(Bridge::start);

    // Apply new preferences: regenerate the chapter stylesheet (used by every
    // chapter loaded from now on), tell the current chapter to reload it, and
    // save them.
    let preferences_file = startup.preferences_file.clone();
    let restyle_bridge = bridge.clone();
    let set_preferences = use_callback(move |new: Preferences| {
        if new == *preferences.peek() {
            return;
        }
        preferences.set(new);
        style.set(new.chapter_css());
        restyles += 1;
        restyle_bridge.restyle(restyles());
        if let Some(file) = &preferences_file {
            storage::save_preferences(file, &new);
        }
    });

    // `use_callback` gives a `Copy` handle to a closure, so the same function
    // can be used from several event handlers below.
    let search_book = shared.clone();
    let open = use_callback(move |path: PathBuf| match open_book(&shared, &path) {
        Ok((mut opened, key)) => {
            // Remember the absolute path, so reopening works from any directory.
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if let Some(store) = store.write().as_mut() {
                if let Some(position) = store.position(&key) {
                    opened.restore(position);
                }
                store.set_last_opened(&path);
                storage::save_progress(store);
            }
            set_window_title(opened.contents.metadata.title.as_deref());
            opened_from.set(Some(OpenedFrom { key, path }));
            search.set(None);
            search_query.set(String::new());
            reader.set(Some(opened));
            error.set(None);
        }
        Err(e) => error.set(Some(format!(
            "Could not open {}: {}",
            path.display(),
            e.full_message()
        ))),
    });

    // Search the open book (synchronously: a book's text is small) and list
    // the hits with their chapter titles.
    let find_bridge = bridge.clone();
    let run_search = use_callback(move |query: String| {
        selected_hit.set(None);
        find_bridge.clear_find();
        if let Some(r) = reader.write().as_mut() {
            r.end_search();
        }
        if query.trim().is_empty() {
            search.set(None);
            return;
        }
        let Some(results) = search_book
            .lock()
            .as_mut()
            .map(|open| open.book.search(&query, SEARCH_LIMIT))
        else {
            return;
        };
        for path in &results.skipped {
            eprintln!("justread: search skipped {path} (missing or not well-formed)");
        }
        let hits = {
            let reader = reader.peek();
            let title = |index: usize| {
                reader
                    .as_ref()
                    .and_then(|r| r.contents.chapter_title(index))
                    .unwrap_or("")
                    .to_owned()
            };
            results
                .hits
                .into_iter()
                .map(|hit| {
                    let chapter = title(hit.spine_index);
                    (hit, chapter)
                })
                .collect()
        };
        search.set(Some(SearchState {
            query,
            hits,
            truncated: results.truncated,
        }));
    });

    // Show hit `i` of the current search in the chapter.
    let pick_bridge = bridge.clone();
    let show_hit = use_callback(move |i: usize| {
        let state = search.peek();
        let Some((hit, _)) = state.as_ref().and_then(|s| s.hits.get(i)) else {
            return;
        };
        let query = state.as_ref().map(|s| s.query.clone()).unwrap_or_default();
        selected_hit.set(Some(i));
        if let Some(r) = reader.write().as_mut()
            && let Some(scroll) = r.show_match(hit.spine_index, &query, hit.occurrence)
        {
            pick_bridge.scroll(scroll);
        }
    });

    // Back (`true`) or forward through the jump history.
    let history_bridge = bridge.clone();
    let go_history = use_callback(move |back: bool| {
        let mut guard = reader.write();
        let Some(r) = guard.as_mut() else { return };
        let scroll = if back { r.go_back() } else { r.go_forward() };
        if let Some(scroll) = scroll {
            history_bridge.scroll(scroll);
        }
    });

    // On launch: the book from the command line, else the last one read
    // (if it still exists).
    use_hook(move || {
        let last = || {
            let store = store.peek();
            let path = store.as_ref()?.last_opened()?.to_path_buf();
            path.exists().then_some(path)
        };
        if let Some(path) = startup.book.or_else(last) {
            open(path);
        }
    });

    // Save the position whenever it changes. An effect re-runs every time a
    // signal it *reads* changes (here `reader` and `opened_from`); writing
    // `store` doesn't subscribe to it, so this can't loop.
    use_effect(move || {
        let reader = reader.read();
        let opened_from = opened_from.read();
        let (Some(r), Some(from)) = (reader.as_ref(), opened_from.as_ref()) else {
            return;
        };
        let position = r.position();
        let mut store = store.write();
        let Some(store) = store.as_mut() else {
            return;
        };
        if store.position(&from.key) != Some(position) {
            let title = r.contents.metadata.title.as_deref();
            store.set_position(&from.key, &from.path, title, position);
            storage::save_progress(store);
        }
    });

    // Keep the highlighted TOC entry visible in the sidebar. `use_memo`
    // recomputes only when `reader` changes and notifies only when the result
    // differs, so the effect below runs once per new entry, not on every
    // scroll event.
    let current_target =
        use_memo(move || reader.read().as_ref().and_then(Reader::current_toc_target));
    use_effect(move || {
        if current_target.read().is_some() {
            // After the re-render that moved the highlight; scroll only if the
            // entry is out of view, so paging through a chapter doesn't
            // jiggle the sidebar.
            document::eval(
                "requestAnimationFrame(() => {
                   const b = document.querySelector('.sidebar button.current');
                   const s = document.querySelector('.sidebar');
                   if (!b || !s) return;
                   const r = b.getBoundingClientRect(), v = s.getBoundingClientRect();
                   if (r.top < v.top || r.bottom > v.bottom) b.scrollIntoView({ block: 'center' });
                 })",
            );
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
                if let PageEvent::Shortcut { action } = event {
                    let current = *preferences.peek();
                    match action {
                        Shortcut::Bigger => set_preferences(current.bigger()),
                        Shortcut::Smaller => set_preferences(current.smaller()),
                        Shortcut::ResetSize => set_preferences(Preferences {
                            font_scale: 100,
                            ..current
                        }),
                        Shortcut::Find => {
                            document::eval("document.getElementById('search')?.select()");
                        }
                        Shortcut::Back => go_history(true),
                        Shortcut::Forward => go_history(false),
                        Shortcut::Escape => {
                            settings_open.set(false);
                            if search.peek().is_some() {
                                run_search(String::new());
                                search_query.set(String::new());
                            }
                        }
                    }
                    continue;
                }
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
                    PageEvent::Link {
                        path,
                        hash,
                        from,
                        fraction,
                        section,
                    } => {
                        // Exactly where the link was clicked, so Back
                        // returns there.
                        r.on_scroll(&from, fraction, section);
                        r.follow_link(&path, &hash)
                    }
                    PageEvent::Scroll {
                        path,
                        fraction,
                        section,
                    } => {
                        r.on_scroll(&path, fraction, section);
                        None
                    }
                    PageEvent::Ready { path } => {
                        // Sections first: the scroll that may follow then
                        // already reports the right one.
                        if r.is_current(&path) {
                            scroll_bridge.track_sections(r.section_ids());
                        }
                        r.on_ready(&path)
                    }
                    PageEvent::FindFailed => {
                        error.set(Some("Couldn't locate that match on the page.".to_owned()));
                        None
                    }
                    // Handled above, before borrowing the reader.
                    PageEvent::Shortcut { .. } => None,
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
    let current_preferences = preferences();

    rsx! {
        // `data-theme` switches the app's own colors (see style.css).
        div { class: "app", "data-theme": current_preferences.theme.name(),
            header { class: "toolbar",
                button { class: "open", onclick: pick_file, "Open EPUB" }
                if let Some(r) = &snapshot {
                    BookHeader { reader: r.clone() }
                    div { class: "nav",
                        button {
                            disabled: !r.can_go_back(),
                            title: "Back to where you jumped from (Alt+←)",
                            onclick: move |_| go_history(true),
                            "← Back"
                        }
                        button {
                            disabled: !r.can_go_forward(),
                            title: "Forward (Alt+→)",
                            onclick: move |_| go_history(false),
                            "→"
                        }
                        span { class: "divider" }
                        button {
                            disabled: !r.has_prev(),
                            title: "Previous chapter",
                            onclick: move |_| {
                                if let Some(r) = reader.write().as_mut() {
                                    r.prev();
                                }
                            },
                            "‹ Prev"
                        }
                        button {
                            disabled: !r.has_next(),
                            title: "Next chapter",
                            onclick: move |_| {
                                if let Some(r) = reader.write().as_mut() {
                                    r.next();
                                }
                            },
                            "Next ›"
                        }
                    }
                }
                button {
                    class: "settings-toggle",
                    title: "Reading settings",
                    "aria-expanded": settings_open(),
                    onclick: move |_| settings_open.toggle(),
                    "Aa"
                }
                if settings_open() {
                    Settings { preferences: current_preferences, on_change: set_preferences }
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
                    let current = r.current_toc_target();
                    let on_select = move |target: Href| {
                        let mut guard = reader.write();
                        let Some(r) = guard.as_mut() else { return };
                        match r.contents.spine_index_of(&target.path) {
                            Some(index) => {
                                if let Some(scroll) = r.jump(index, target.fragment) {
                                    bridge.scroll(scroll);
                                }
                            }
                            None => error.set(Some(format!("{} is not part of the reading order", target.path))),
                        }
                    };
                    rsx! {
                        div { class: "body",
                            nav { class: "sidebar", aria_label: "Table of contents and search",
                                div { class: "search",
                                    input {
                                        id: "search",
                                        r#type: "search",
                                        placeholder: "Search in book (Ctrl+F)",
                                        value: "{search_query}",
                                        oninput: move |e| search_query.set(e.value()),
                                        onkeydown: move |e| {
                                            if e.key() == Key::Enter {
                                                run_search(search_query());
                                            }
                                        },
                                    }
                                }
                                match search() {
                                    Some(state) => rsx! {
                                        SearchResultsList {
                                            state,
                                            selected: selected_hit(),
                                            on_pick: show_hit,
                                            on_close: move |_| {
                                                search_query.set(String::new());
                                                run_search(String::new());
                                            },
                                        }
                                    },
                                    None if r.contents.toc.is_empty() => rsx! {
                                        p { class: "muted", "This book has no table of contents." }
                                    },
                                    None => rsx! {
                                        TocList { entries: r.contents.toc.clone(), current, on_select }
                                    },
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

/// Search results, shown in the sidebar instead of the table of contents.
#[component]
fn SearchResultsList(
    state: SearchState,
    selected: Option<usize>,
    on_pick: Callback<usize>,
    on_close: EventHandler<()>,
) -> Element {
    let count = state.hits.len();
    let summary = match (count, state.truncated) {
        (0, _) => format!("No matches for “{}”", state.query),
        (_, true) => format!("First {count} matches for “{}”", state.query),
        (1, false) => format!("1 match for “{}”", state.query),
        (_, false) => format!("{count} matches for “{}”", state.query),
    };
    rsx! {
        div { class: "results-header",
            span { "{summary}" }
            button { title: "Close search (Esc)", onclick: move |_| on_close.call(()), "✕" }
        }
        ul { class: "hits",
            for (i, (hit, chapter)) in state.hits.into_iter().enumerate() {
                li { key: "{i}",
                    button {
                        class: if selected == Some(i) { "current" } else { "" },
                        onclick: move |_| on_pick.call(i),
                        if !chapter.is_empty() {
                            span { class: "where", "{chapter}" }
                        }
                        span { class: "snippet",
                            "{hit.before}"
                            mark { "{hit.matched}" }
                            "{hit.after}"
                        }
                    }
                }
            }
        }
    }
}

/// The reading-settings panel: theme, text size and line width.
#[component]
fn Settings(preferences: Preferences, on_change: Callback<Preferences>) -> Element {
    let p = preferences;
    rsx! {
        div { class: "settings", role: "dialog", aria_label: "Reading settings",
            div { class: "row",
                span { class: "label", "Theme" }
                div { class: "choices",
                    for theme in Theme::ALL {
                        button {
                            key: "{theme.name()}",
                            class: "swatch {theme.name()}",
                            "aria-pressed": p.theme == theme,
                            onclick: move |_| on_change.call(Preferences { theme, ..p }),
                            "{theme.label()}"
                        }
                    }
                }
            }
            div { class: "row",
                span { class: "label", "Text size" }
                div { class: "choices",
                    button {
                        title: "Smaller text",
                        disabled: p.font_scale <= MIN_FONT_SCALE,
                        onclick: move |_| on_change.call(p.smaller()),
                        "A−"
                    }
                    button {
                        class: "value",
                        title: "Reset to the book's size",
                        onclick: move |_| on_change.call(Preferences { font_scale: 100, ..p }),
                        "{p.font_scale}%"
                    }
                    button {
                        title: "Larger text",
                        disabled: p.font_scale >= MAX_FONT_SCALE,
                        onclick: move |_| on_change.call(p.bigger()),
                        "A+"
                    }
                }
            }
            div { class: "row",
                span { class: "label", "Line width" }
                div { class: "choices",
                    for width in LineWidth::ALL {
                        button {
                            key: "{width.label()}",
                            "aria-pressed": p.line_width == width,
                            onclick: move |_| on_change.call(Preferences { line_width: width, ..p }),
                            "{width.label()}"
                        }
                    }
                }
            }
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

/// Open the book at `path`, make it the one served over `epub://`, and
/// return its reading state plus its identity for saving progress.
fn open_book(shared: &SharedBook, path: &Path) -> Result<(Reader, BookKey), EpubError> {
    let book = Book::open(path)?;
    let key = BookKey::from_file(path)?;
    for warning in book.warnings() {
        eprintln!("justread: {}: {warning}", path.display());
    }
    let id = NEXT_BOOK_ID.fetch_add(1, Ordering::Relaxed);
    let reader = Reader::new(id, book.contents().clone());
    *shared.lock() = Some(OpenBook { id, book });
    Ok((reader, key))
}

fn set_window_title(book_title: Option<&str>) {
    let title = match book_title {
        Some(t) => format!("{t} — justread"),
        None => "justread".to_owned(),
    };
    dioxus::desktop::window().set_title(&title);
}
