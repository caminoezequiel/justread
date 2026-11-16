# justread

A lightweight cross-platform desktop EPUB and PDF reader (Linux, macOS, Windows) in Rust:
a parsing library (`epub-core`) and a Dioxus 0.7 desktop app (`reader-app`, binary `justread`).
The owner is learning Rust with this project: write idiomatic Rust and briefly
explain new language concepts when introducing them.

Details of every module, packaging, CI and how to verify the UI are in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): read the section for the area you are changing,
and keep it current.

## Commands

Run from the workspace root. Times are on the dev machine with a warm build cache;
the first build after a clean checkout or a dependency change takes about 70 s for the gate.

| What | Command | Time |
|---|---|---|
| Build | `cargo build --workspace --locked` | ~2 s |
| Run the app | `JUSTREAD_DATA_DIR=/tmp/justread-dev cargo run -p reader-app -- epub-core/tests/fixtures/alice-epub2.epub` | opens a window |
| Print a book's metadata, spine, TOC | `cargo run -p epub-core --example info -- epub-core/tests/fixtures/alice-epub2.epub` | ~1 s |
| **Fast gate** (fmt check + clippy + all tests, same as CI) | `scripts/gate.sh` | 1–4 s |
| Fix formatting | `cargo fmt --all` | <1 s |
| Integration tests only | `cargo test -p epub-core --test '*' --locked` | ~1 s |
| Enable the git hooks (once per clone) | `git config core.hooksPath .githooks` | — |

- There is nothing to install besides stable Rust (with `rustfmt`, `clippy`) and, on Linux, the
  packages `libwebkit2gtk-4.1-dev build-essential libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev pkg-config`.
- The integration tests (`epub-core/tests/*.rs`) read local fixture EPUBs, need no services
  and are part of the gate.
- Always set `JUSTREAD_DATA_DIR=<scratch dir>` when running the app for a test, so the real
  `~/.local/share/justread/progress.json` isn't touched.

## Architecture

Cargo workspace, edition 2024, two crates:

- `epub-core/`: library, no UI dependencies. `Book::open` parses an EPUB (container → OPF →
  spine/TOC) and serves its resources straight from the zip, never extracting to disk.
  Also search (`search.rs`) and reading progress (`progress.rs`).
- `reader-app/`: the desktop app. `reader.rs` (navigation state, no UI), `app.rs` (components),
  `protocol.rs` (serves the book to an iframe at `epub://book/<id>/<path>`), `bridge.rs` ⇄
  `reader.js` (postMessage; the app never touches the chapter's DOM), `preferences.rs`,
  `dictionary.rs`/`lookup.rs` (tap to translate), `pdf.rs` (PDF.js viewer).
- `search.rs` and `reader.js` implement the same text folding: change both together.

## Conventions

- No `unwrap()`/`expect()` outside tests; a malformed book must never panic. Problems that
  still leave a readable book become `Book::warnings()`, not errors.
- `thiserror` errors in `epub-core` (`EpubError`, `Result<T>`); `anyhow` in binaries/examples.
  Error variants that wrap a source don't repeat it in their message.
- Dependency versions are pinned exactly (`=x.y.z`). Check docs.rs or the crate source for the
  pinned version before using an API (Dioxus changes a lot between versions).
- Tests: prefer integration tests against real fixture EPUBs (`epub-core/tests/`); unit tests
  (`#[cfg(test)] mod tests` in the same file) only for pure helpers. Test names are
  sentences in snake_case (`missing_resource_is_a_clear_error`). Never commit commercial or
  copyrighted books as fixtures.
- Logic in `reader-app` goes in a UI-free module with unit tests (`reader.rs`, `history.rs`,
  `preferences.rs`); components stay thin.
- Small modules, simple code, short comments explaining Rust concepts where they first appear.

## Definition of done

A task is not done until `scripts/gate.sh` is green. The Stop hook runs it for you and
blocks once if it is red. A change to parsing, search or progress also needs an integration
test against a fixture. A change to the UI or `reader.js` must be seen working in the running
app (see "Verifying the UI" in docs/ARCHITECTURE.md), because no test covers it.

## Don't

- Don't edit `Cargo.lock` by hand (use `cargo update -p <crate>`), `.env*` files, or the
  bytes of committed fixture EPUBs (their SHA-256 is asserted; rebuild hand-made ones with
  `epub-core/tests/fixtures/hand-made/build.sh`).
- Don't disable, delete, `#[ignore]` or weaken a test, or add `#[allow(...)]`, to make the gate pass.
- Don't push, force-push or merge unless the user asks.

## Reglas aprendidas

Each time the same mistake is corrected twice in review, add one line here (or better, a test
or a hook that catches it). Keep each rule to one sentence with its reason.

