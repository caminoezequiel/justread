# justread

A lightweight cross-platform desktop EPUB reader (Linux, macOS, Windows) in Rust.
The owner is learning Rust with this project: write idiomatic Rust and briefly
explain new language concepts when introducing them.

## Layout

Cargo workspace (`Cargo.toml` at the root, edition 2024, resolver 3):

- `epub-core/`: library. Parses EPUBs and serves their resources. No UI dependencies.
  - `book.rs`: `Book<R: Read + Seek>`, the public entry point (`Book::open`, `metadata`, `spine`, `toc`, `read_bytes`, `warnings`).
  - `opf.rs`: `META-INF/container.xml` → OPF path; OPF metadata/manifest/spine.
  - `toc.rs`: EPUB3 `nav.xhtml` (preferred), EPUB2 `toc.ncx` fallback.
  - `href.rs`: `Href::resolve(from_doc, href)`: percent-decoding, `#fragment`, `..`, and rejecting paths that escape the archive or remote URLs.
  - `xml.rs`: `roxmltree` helpers (match by local name, DTD allowed, BOM stripped).
  - `error.rs`: `EpubError` (`thiserror`) and `Result<T>`.
  - `examples/info.rs`: CLI that prints metadata, spine and TOC.
- `reader-app/`: Dioxus desktop binary (milestone 3+; placeholder for now).

## How an EPUB is read

- Never extract to disk; zip entries are read on demand via `ZipArchive::by_name`.
- `container.xml` `rootfile/@full-path` → OPF. Manifest hrefs resolve relative to the OPF's folder; nav/NCX hrefs relative to the nav/NCX file.
- All paths stored in `epub-core` types are decoded zip entry names (no leading `/`, no `%xx`).
- Problems that still leave a readable book (bad manifest href, broken TOC, missing `mimetype`) go into `Book::warnings()`. Only an unreadable container/OPF or empty spine is an error.
- Planned rendering: the current spine item in an iframe at `/book/<path-in-zip>`, served by a Dioxus asset handler from `Book::read_bytes`, so relative CSS/image/font links resolve without rewriting HTML.

## Commands

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p epub-core --example info -- path/to/book.epub
```

All three checks (fmt, clippy with `-D warnings`, tests) must pass before every commit.

## Conventions

- No `unwrap()`/`expect()` outside tests; a malformed book must never panic.
- `thiserror` errors in `epub-core`; `anyhow` in binaries/examples. Error variants that wrap a source don't repeat it in their message.
- Dependency versions are pinned exactly (`=x.y.z`). Check docs.rs or the crate source for the pinned version before using an API (Dioxus changes a lot between versions).
- Prefer integration tests against real fixture EPUBs (`epub-core/tests/`); small unit tests only for pure helpers like `href.rs`.
- Small modules, simple code, short comments explaining Rust concepts where they first appear.
- Linux build deps for the Dioxus app: `libwebkit2gtk-4.1-dev build-essential libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev pkg-config`.
