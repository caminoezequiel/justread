# justread

A lightweight cross-platform desktop EPUB reader (Linux, macOS, Windows) in Rust.
The owner is learning Rust with this project: write idiomatic Rust and briefly
explain new language concepts when introducing them.

## Layout

Cargo workspace (`Cargo.toml` at the root, edition 2024, resolver 3):

- `epub-core/`: library. Parses EPUBs and serves their resources. No UI dependencies.
  - `book.rs`: `Book<R: Read + Seek>`, the public entry point (`Book::open`, `contents`, `read_bytes`, `media_type`, `warnings`), and `Contents` (metadata, spine, TOC; cloneable, no file handle) with `spine_index_of`, `toc_entry_for`/`chapter_title`, and size-weighted `progress`.
  - `opf.rs`: `META-INF/container.xml` → OPF path; OPF metadata/manifest/spine.
  - `toc.rs`: EPUB3 `nav.xhtml` (preferred), EPUB2 `toc.ncx` fallback.
  - `href.rs`: `Href::resolve(from_doc, href)`: percent-decoding, `#fragment`, `..`, and rejecting paths that escape the archive or remote URLs.
  - `xml.rs`: `roxmltree` helpers (match by local name, DTD allowed, BOM stripped).
  - `error.rs`: `EpubError` (`thiserror`) and `Result<T>`.
  - `examples/info.rs`: CLI that prints metadata, spine and TOC.
- `reader-app/`: Dioxus 0.7 desktop binary. `reader-app [book.epub]`.
  - `main.rs`: window config, registers the `epub` custom protocol, provides `SharedBook` and `StartupPath` as contexts.
  - `protocol.rs`: serves `epub://book/<book-id>/<path-in-zip>` (Windows: `http://epub.book/...`) from the open book with the right Content-Type; injects `<script src="/__justread/reader.js">` into spine documents only; serves malformed XHTML as `text/html`; error pages instead of panics. `SharedBook = Arc<Mutex<Option<OpenBook>>>`.
  - `reader.js`: runs inside each chapter; postMessage bridge to the app (keys, in-book link clicks, scroll fraction, ready; receives `scroll_to`).
  - `bridge.rs`: the app-side half (one long-running `document::eval`), `PageEvent` / `Command` serde enums.
  - `reader.rs`: `Reader` state and navigation rules (next/prev, go_to, follow_link, on_ready, on_scroll); unit tested, no UI code.
  - `app.rs`: components (`App`, `BookHeader`, recursive `TocList`); `style.css` is inlined via `with_custom_head`.

## How an EPUB is read

- Never extract to disk; zip entries are read on demand via `ZipArchive::by_name`.
- `container.xml` `rootfile/@full-path` → OPF. Manifest hrefs resolve relative to the OPF's folder; nav/NCX hrefs relative to the nav/NCX file.
- All paths stored in `epub-core` types are decoded zip entry names (no leading `/`, no `%xx`).
- Problems that still leave a readable book (bad manifest href, broken TOC, missing `mimetype`) go into `Book::warnings()`. Only an unreadable container/OPF or empty spine is an error.
- Rendering: the current spine item is shown in an iframe at `epub://book/<book-id>/<path>`, so relative CSS/image/font links resolve natively. It is a separate scheme because Dioxus blocks every `dioxus://` navigation after the first page load, iframes included (`dioxus-desktop` `webview.rs`). The book id changes on every open (no stale cache, no cross-book reads).
- The iframe is cross-origin to the app, so the app never touches its DOM: everything goes through `reader.js` ⇄ `bridge.rs` postMessage. Fragments are scrolled by the chapter itself after it reports `ready` (or immediately if the document is already loaded).

## Commands

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p epub-core --example info -- path/to/book.epub
cargo run -p reader-app -- path/to/book.epub   # or: dx serve -p reader-app --platform desktop
```

All three checks (fmt, clippy with `-D warnings`, tests) must pass before every commit.

Verifying the UI on Linux: start the app with `WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9334` and drive it through WebKit's remote inspector websocket (`/socket/1/1/WebPage`, commands wrapped in `Target.sendMessageToTarget`; `Runtime.evaluate` for DOM/state, `Page.snapshotRect` for screenshots). Avoid port 9222 (often a Chrome debugging port). Cross-origin iframe internals are not reachable this way; verify them through their effects (progress, current chapter). `dx serve` renames the binary (`reader-app-<hash>`), so stop it by PID.

## Conventions

- No `unwrap()`/`expect()` outside tests; a malformed book must never panic.
- `thiserror` errors in `epub-core`; `anyhow` in binaries/examples. Error variants that wrap a source don't repeat it in their message.
- Dependency versions are pinned exactly (`=x.y.z`). Check docs.rs or the crate source for the pinned version before using an API (Dioxus changes a lot between versions).
- Prefer integration tests against real fixture EPUBs (`epub-core/tests/`); small unit tests only for pure helpers like `href.rs`.
  - `tests/gutenberg.rs`: real public-domain books; `tests/broken.rs`: hand-made broken books; `tests/common/mod.rs`: shared helpers.
  - Fixtures and their sources are listed in `epub-core/tests/fixtures/README.md`. Hand-made fixtures are built from `tests/fixtures/hand-made/<name>/` by `hand-made/build.sh`; edit the folder, rerun the script, commit both.
  - Never commit commercial or copyrighted books as fixtures.
- Small modules, simple code, short comments explaining Rust concepts where they first appear.
- Linux build deps for the Dioxus app: `libwebkit2gtk-4.1-dev build-essential libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev pkg-config`.
