# justread

A lightweight, cross-platform desktop EPUB reader for Linux, macOS and Windows,
written in Rust. It aims to be a fast, simple alternative to Calibre's viewer:
open a book, read it comfortably, and pick up exactly where you left off.

## Objective

justread has two goals:

1. **A small, pleasant reader.** It does a few things well: renders chapters
   faithfully, navigates by table of contents, remembers your position, and
   offers comfortable reading themes. There is no library management, no
   conversion and no editing.
2. **Learning Rust.** The project is also a hands-on way to learn idiomatic
   Rust: ownership and borrowing, `Result` and `?`, traits and generics,
   closures, error handling with `thiserror`/`anyhow`, workspaces and testing.
   The EPUB format is parsed by hand, not with an existing EPUB crate, to
   understand how it works.

## How it works

An EPUB is a zip file of XHTML, CSS, images and fonts, plus a few XML files
that describe them. justread never extracts the book to disk; it reads each
entry from the zip when it needs it:

1. `META-INF/container.xml` points to the **OPF** package file.
2. The OPF has the **metadata** (title, author), the **manifest** (every file
   in the book, with its media type) and the **spine** (reading order).
3. The **table of contents** comes from the EPUB3 `nav.xhtml`, or the EPUB2
   `toc.ncx` as a fallback.
4. In the app, each chapter is shown in an iframe. The iframe loads it from a
   custom URL (`/book/<path-in-zip>`) that returns the zip entry directly, so
   the chapter's relative links to stylesheets, images and fonts work without
   rewriting any HTML.

Broken or unusual books are handled gracefully: bad links, missing files and
invalid XML produce clear errors or warnings, never a crash.

## Project layout

| Crate | Kind | Purpose |
|---|---|---|
| [`epub-core`](epub-core/) | library | EPUB parsing, resource access, TOC, reading-progress persistence. No UI code. |
| [`reader-app`](reader-app/) | binary | The desktop UI, built with [Dioxus](https://dioxuslabs.com/) (renders through the OS webview). |

## Status

The project is built in milestones:

- [x] **1. Core parsing:** open an EPUB, read title, author, spine and TOC; `info` CLI.
- [ ] **2. Tests:** integration tests against real Project Gutenberg books and a deliberately broken EPUB.
- [ ] **3. Reader UI:** open a file, render chapters, next/previous (buttons and arrow keys), TOC sidebar, progress.
- [ ] **4. Remember position:** per-book reading position saved and restored; reopen the last book on launch.
- [ ] **5. Reading comfort:** light, dark and sepia themes, font size, line width.
- [ ] **6. Packaging:** installable bundles for each OS and CI builds on GitHub Actions.

## Getting started

Install Rust with [rustup](https://rustup.rs/). On Linux, the desktop UI also
needs the WebKitGTK development packages (Ubuntu/Debian):

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev pkg-config
```

Print a book's metadata, reading order and table of contents:

```bash
cargo run -p epub-core --example info -- path/to/book.epub
```

Run the checks (all must pass before every commit):

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

See [CLAUDE.md](CLAUDE.md) for architecture details and coding conventions.
