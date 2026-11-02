# justread

A lightweight, cross-platform desktop EPUB and PDF reader for Linux, macOS and
Windows, written in Rust. It aims to be a fast, simple alternative to Calibre's viewer:
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

## Features

- Opens EPUB 2 and EPUB 3 books, rendered with the book's own styles, images
  and fonts.
- Opens PDF files too, shown as a scrolling document with the PDF's own
  bookmarks as the table of contents, its internal links, tap to translate,
  and the text size setting as zoom (PDF pages can't reflow, so line width
  doesn't apply to them).
- A **☰** button in the toolbar hides and shows the chapter sidebar, to give
  the text the full width.
- Table of contents sidebar (EPUB3 `nav.xhtml`, or EPUB2 `toc.ncx`), including
  jumps to sections inside a chapter; it follows along, highlighting the
  section you are reading. Links inside the book work too.
- Search the whole book (EPUB only for now); results show the chapter and
  surrounding text, and clicking one jumps to that exact spot and highlights
  it.
- Page turning with the arrow keys, continuing across chapters.
- Current chapter title and overall progress (weighted by chapter length).
- Remembers your exact place in every book, and reopens the last book on launch.
- Light, sepia and dark themes, text size and line width, applied instantly.
- **Tap to translate (English → Spanish)**, for learning English while reading:
  click a word, or select a few, to see its Spanish meanings from an offline
  dictionary. It recognises inflected forms (ran → run) and phrasal verbs and
  idioms from the words that follow ("ran out of time" → *run out*: agotar,
  quedarse sin). **Translate sentence** opens the whole sentence in Google
  Translate, in a window of its own.
- Broken or unusual books open whenever possible; problems show as clear
  messages, never a crash.

## Using justread

Start it with a book (EPUB or PDF), or without one to reopen the last book you
read (or use **Open book**):

```bash
justread path/to/book.epub
justread path/to/document.pdf
```

| Action | How |
|---|---|
| Turn a page | <kbd>→</kbd> / <kbd>←</kbd> (one screenful; at the end or start of a chapter they continue into the next or previous one) |
| Scroll | <kbd>↑</kbd> / <kbd>↓</kbd>, <kbd>Page Up</kbd> / <kbd>Page Down</kbd>, <kbd>Space</kbd>, mouse wheel |
| Next / previous chapter | **Next ›** / **‹ Prev** buttons |
| Go to a chapter or section | Click it in the sidebar |
| Show or hide the sidebar | **☰** button in the toolbar |
| Reading settings | **Aa** button: theme (light keeps the book's own colors), text size 60–200% (zoom for PDFs), line width (EPUB only) |
| Text size | <kbd>Ctrl</kbd>/<kbd>⌘</kbd> + <kbd>+</kbd> / <kbd>−</kbd>, <kbd>Ctrl</kbd>/<kbd>⌘</kbd> + <kbd>0</kbd> to reset |
| Search | <kbd>Ctrl</kbd>/<kbd>⌘</kbd> + <kbd>F</kbd>, type, <kbd>Enter</kbd>; click a result to jump to it |
| Translate a word | Click it; select text to translate a phrase. **Translate sentence ↗** opens Google Translate. Turn it off in **Aa** → Translate |
| Close the translation, search or settings | <kbd>Esc</kbd> |

**Reading position** is saved as you read, per book, in `progress.json`;
**settings** are saved in `preferences.json`. Both live in your data folder:

| OS | Folder |
|---|---|
| Linux | `~/.local/share/justread` |
| macOS | `~/Library/Application Support/justread` |
| Windows | `%LOCALAPPDATA%\justread\data` |

Books are recognised by their content (a SHA-256 of the file), so moving or
renaming a file keeps its place. To use a different folder (for example while
testing), set `JUSTREAD_DATA_DIR`.

**Translation and privacy:** the dictionary is built into the app and works
offline; nothing leaves your computer when you click a word. Only the text you
send with a **Translate ↗** button goes to Google. The Google window keeps its
cookies (for its consent screen) in `google-webview` inside the data folder.
The dictionary comes from the English Wiktionary, under CC BY-SA 4.0 (see
[reader-app/dictionary](reader-app/dictionary/README.md)).

### Known limitations

- DRM-protected books (encrypted with `META-INF/encryption.xml`) can't be read.
- Text size has no effect on books that set font sizes in fixed `px` units.
- The installers are not code-signed (see [Packaging](#packaging)).
- The dictionary is word-level: it lists every sense of a word (the most common
  first) and can't tell which one the sentence uses; Google Translate can.
- Search skips chapters that aren't well-formed XHTML (they still display),
  and result snippets can run words together where the book separates them
  only by layout (for example in an index).

## How it works

An EPUB is a zip file of XHTML, CSS, images and fonts, plus a few XML files
that describe them. (A PDF is shown by PDF.js in the same way: its pages are
drawn into one scrolling view, and its bookmarks become the table of contents.) justread never extracts the book to disk; it reads each
entry from the zip when it needs it:

1. `META-INF/container.xml` points to the **OPF** package file.
2. The OPF has the **metadata** (title, author), the **manifest** (every file
   in the book, with its media type) and the **spine** (reading order).
3. The **table of contents** comes from the EPUB3 `nav.xhtml`, or the EPUB2
   `toc.ncx` as a fallback.
4. In the app, each chapter is shown in an iframe. The iframe loads it from a
   custom URL (`epub://book/<id>/<path-in-zip>`) that returns the zip entry
   directly, so the chapter's relative links to stylesheets, images and fonts
   work without rewriting any HTML.
5. Two small files are added to each chapter: a stylesheet generated from your
   reading settings (theme, text size, line width), and a script that reports
   key presses, link clicks, scrolling, the current section and clicked words
   (with their sentence) back to the app, and finds and highlights search
   matches.
6. Search reads the text of every chapter from the zip and counts matches per
   chapter; the chapter script finds "the n-th match" in the rendered page
   using exactly the same rules, so the highlight lands on the right one.
7. Translation looks the clicked word up in a dictionary embedded in the
   binary (a compressed subset of Wiktionary). Google Translate can't be shown
   in an iframe, so it gets a separate native window.

Broken or unusual books are handled gracefully: bad links, missing files and
invalid XML produce clear errors or warnings, never a crash.

## Project layout

| Crate | Kind | Purpose |
|---|---|---|
| [`epub-core`](epub-core/) | library | EPUB parsing, resource access, TOC, reading-progress persistence. No UI code. |
| [`reader-app`](reader-app/) | binary `justread` | The desktop UI, built with [Dioxus](https://dioxuslabs.com/) (renders through the OS webview). |

See [CLAUDE.md](CLAUDE.md) for architecture details and coding conventions.

## Building from source

Install Rust with [rustup](https://rustup.rs/), plus the platform's webview
build requirements:

- **Linux** (Ubuntu/Debian): the WebKitGTK development packages:

  ```bash
  sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev pkg-config
  ```

- **macOS**: the Xcode command line tools (`xcode-select --install`).
- **Windows**: the Rust MSVC toolchain (Visual Studio Build Tools, "Desktop
  development with C++"). The WebView2 runtime is already part of Windows 10
  and 11.

Run the reader:

```bash
cargo run -p reader-app -- path/to/book.epub   # or a .pdf file
```

With hot reload during development (install the Dioxus CLI once with
`cargo install dioxus-cli --version 0.7.10 --locked`). `dx serve` doesn't pass
a book path, so it reopens the last book:

```bash
dx serve -p reader-app --platform desktop
```

Print a book's metadata, reading order and table of contents:

```bash
cargo run -p epub-core --example info -- path/to/book.epub
```

## Packaging

Installers are built with the Dioxus CLI. Install it once (the version must match
the `dioxus` crate):

```bash
cargo install dioxus-cli --version 0.7.10 --locked
```

Then, **from the repository root** on the OS you are building for (bundles can't
be cross-built), run the command for that OS. Output goes to `dist/`.

**Linux** (`.deb` and `.rpm`):

```bash
dx bundle -p reader-app --desktop --release --package-types deb --package-types rpm --out-dir dist
```

The packages install `justread` with a menu entry, icons, and "Open with" for
`.epub` and `.pdf` files. Install with `sudo apt install ./dist/justread_0.1.0_amd64.deb`
(or `sudo dnf install ./dist/justread-0.1.0-1.x86_64.rpm`).

**macOS** (`.app` and `.dmg`):

```bash
dx bundle -p reader-app --desktop --release --package-types macos --package-types dmg --out-dir dist
```

The app is not code-signed or notarized, so the first time macOS will refuse to
open it: right-click the app → **Open**, or run
`xattr -dr com.apple.quarantine /Applications/Justread.app`.

**Windows** (`.msi` and an `.exe` installer):

```bash
dx bundle -p reader-app --desktop --release --package-types msi --package-types nsis --out-dir dist
```

`dx` downloads WiX and NSIS (checksum-verified) the first time. The installers
are unsigned, so SmartScreen will warn: **More info → Run anyway**.

### Continuous integration and releases

GitHub Actions ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs the
development checks below on Linux, macOS and Windows for every push and pull
request. For pushes to `main`, tags and manual runs it also builds the
installers on each OS and attaches them to the run as artifacts
(`justread-Linux`, `justread-macOS`, `justread-Windows`).

To publish a release, open the repository on GitHub → **Actions** →
**Release** → **Run workflow** (on `main`) and pick the version:

| Choice | Example |
|---|---|
| `patch` | 0.1.0 → 0.1.1 (fixes) |
| `minor` | 0.1.0 → 0.2.0 (new features) |
| `major` | 0.4.2 → 1.0.0 |
| `current` | release the version in `Cargo.toml` as is (e.g. the first release, 0.1.0) |

The [Release workflow](.github/workflows/release.yml) sets the new version in
`reader-app/Cargo.toml`, commits it to `main` as "Release vX.Y.Z", tags it, runs
the full CI on that tag (tests and installers for every OS), and publishes a
GitHub Release with the installers attached (`.deb`, `.rpm`, `.dmg`, `.msi`,
`.exe`) and generated release notes. If any test or installer build fails,
nothing is published. It refuses a version whose tag already exists.

Pushing a version tag from your computer still works too
(`git tag v0.2.0 && git push origin v0.2.0`, after setting that version in
`Cargo.toml`); a tag that doesn't match the version fails instead of
publishing mislabelled files.

## Development checks

Run the checks (all must pass before every commit):

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The integration tests run against real public-domain books from Project
Gutenberg and a set of deliberately broken EPUBs, all in
[`epub-core/tests/fixtures`](epub-core/tests/fixtures/README.md).

## Project history

justread was built in milestones:

1. **Core parsing:** open an EPUB, read title, author, spine and TOC; `info` CLI.
2. **Tests:** integration tests against real Project Gutenberg books and hand-made broken EPUBs.
3. **Reader UI:** open a file, render chapters, page turning, TOC sidebar, progress.
4. **Remember position:** per-book reading position saved and restored; reopen the last book on launch.
5. **Reading comfort:** light, dark and sepia themes, text size, line width.
6. **Packaging:** installable bundles for each OS and CI builds on GitHub Actions.
7. **Extras:** search, sidebar that follows the current section, text-size
   shortcuts, releases from version tags, back/forward history.
8. **Learning English:** tap-to-translate with an offline dictionary and
   Google Translate for sentences.
