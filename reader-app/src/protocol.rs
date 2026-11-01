//! The `epub://` custom protocol: serves files straight out of the open book.
//!
//! The chapter iframe loads `epub://book/<book-id>/<path-in-zip>`, so relative
//! links to CSS, images and fonts resolve against that URL and come back here.
//! The book id changes every time a book is opened, so a stale iframe can
//! never read from the wrong book and the webview never reuses a cached file.
//!
//! The app itself is served by Dioxus from `dioxus://`, which blocks iframe
//! navigation after the first page load; hence a scheme of our own.

use std::borrow::Cow;
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex, MutexGuard};

use dioxus::desktop::wry::http::{Request, Response, StatusCode, header};
use epub_core::{Book, EpubError};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

pub const SCHEME: &str = "epub";

/// WebView2 (Windows) exposes custom schemes as `http://<scheme>.<host>`.
#[cfg(not(windows))]
const ORIGIN: &str = "epub://book";
#[cfg(windows)]
const ORIGIN: &str = "http://epub.book";

/// Files of our own, served next to the book (see `reader.js`).
const INTERNAL_PREFIX: &str = "__justread/";
const READER_JS: &str = include_str!("reader.js");
/// Self-closing `<link/>` so the tag is valid in XHTML as well as HTML.
const INJECTED_TAG: &str = r#"<link rel="stylesheet" href="/__justread/reader.css"/><script src="/__justread/reader.js"></script>"#;

/// Characters that must be escaped inside one URL path segment.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

pub struct OpenBook {
    pub id: u64,
    pub book: Book<BufReader<File>>,
}

/// The open book, shared by the UI (which opens books) and the protocol
/// handler (which reads from them).
///
/// `Arc` ("atomically reference counted") lets several owners hold the same
/// value; `Mutex` makes sure only one of them uses it at a time, which
/// `read_bytes(&mut self)` needs. `#[derive(Clone)]` clones the `Arc`, i.e. it
/// hands out another pointer to the same book, not a copy of it.
#[derive(Clone, Default)]
pub struct SharedBook(Arc<Mutex<Option<OpenBook>>>);

impl SharedBook {
    pub fn lock(&self) -> MutexGuard<'_, Option<OpenBook>> {
        // A mutex is "poisoned" if a thread panicked while holding it. Our
        // data is still usable then, so take it rather than propagate a panic.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The stylesheet added to every chapter (generated from the reader's
/// preferences), shared like [`SharedBook`] so the UI can update it.
#[derive(Clone, Default)]
pub struct SharedStyle(Arc<Mutex<String>>);

impl SharedStyle {
    pub fn new(css: String) -> Self {
        SharedStyle(Arc::new(Mutex::new(css)))
    }

    pub fn set(&self, css: String) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = css;
    }

    fn get(&self) -> String {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// URL of `path` (a zip entry) inside book `id`.
pub fn url(id: u64, path: &str) -> String {
    let encoded: Vec<String> = path
        .split('/')
        .map(|segment| utf8_percent_encode(segment, SEGMENT).to_string())
        .collect();
    format!("{ORIGIN}/{id}/{}", encoded.join("/"))
}

/// Inverse of [`url`] for the path part: `/7/OEBPS/ch%201.xhtml` -> `(7, "OEBPS/ch 1.xhtml")`.
pub fn parse_url_path(url_path: &str) -> Option<(u64, String)> {
    let (id, rest) = url_path.trim_start_matches('/').split_once('/')?;
    let id = id.parse().ok()?;
    let path = percent_decode_str(rest).decode_utf8().ok()?.into_owned();
    Some((id, path))
}

/// Answer one request from the webview. Never panics: every failure becomes
/// an HTTP error response that shows up in the iframe.
pub fn handle(
    shared: &SharedBook,
    style: &SharedStyle,
    request: &Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let url_path = request.uri().path();
    if let Some(name) = url_path
        .trim_start_matches('/')
        .strip_prefix(INTERNAL_PREFIX)
    {
        let mut response = match name {
            "reader.js" => respond(
                StatusCode::OK,
                "text/javascript",
                READER_JS.as_bytes().to_vec(),
            ),
            "reader.css" => respond(StatusCode::OK, "text/css", style.get().into_bytes()),
            _ => return not_found(url_path),
        };
        // The stylesheet changes with the preferences; never let the webview
        // reuse an old copy for the next chapter.
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        return response;
    }
    let Some((id, path)) = parse_url_path(url_path) else {
        return not_found(url_path);
    };

    let mut guard = shared.lock();
    // `as_mut()` turns `&mut Option<OpenBook>` into `Option<&mut OpenBook>`.
    let Some(open) = guard.as_mut().filter(|open| open.id == id) else {
        return not_found(url_path);
    };
    let bytes = match open.book.read_bytes(&path) {
        Ok(bytes) => bytes,
        Err(EpubError::MissingFile(_)) => return not_found(&path),
        Err(e) => {
            return error_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "This part of the book could not be read",
                &e.full_message(),
            );
        }
    };

    let media_type = open
        .book
        .media_type(&path)
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| guess_media_type(&path));
    if is_document(media_type) {
        let (media_type, bytes) = prepare_document(media_type, bytes);
        return respond(StatusCode::OK, media_type, bytes);
    }
    respond(StatusCode::OK, media_type, bytes)
}

fn is_document(media_type: &str) -> bool {
    matches!(media_type, "application/xhtml+xml" | "text/html")
}

/// Add the bridge script and reader stylesheet to a chapter, and fall back to HTML parsing for
/// "XHTML" that isn't well-formed XML (the browser would otherwise show an
/// XML error page instead of the chapter).
fn prepare_document(media_type: &str, bytes: Vec<u8>) -> (&'static str, Vec<u8>) {
    let Ok(mut text) = String::from_utf8(bytes.clone()) else {
        return ("text/html", bytes); // not UTF-8: let the browser cope
    };
    let media_type =
        if media_type == "application/xhtml+xml" && epub_core::is_well_formed_xml(&text) {
            "application/xhtml+xml"
        } else {
            "text/html"
        };
    // ASCII lowercasing keeps byte offsets identical, so positions found in
    // `lower` are valid in `text`.
    let lower = text.to_ascii_lowercase();
    let position = lower
        .find("</head>")
        .or_else(|| lower.find("</body>"))
        .unwrap_or(text.len());
    text.insert_str(position, INJECTED_TAG);
    (media_type, text.into_bytes())
}

fn guess_media_type(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase());
    match extension.as_deref() {
        Some("xhtml" | "xht") => "application/xhtml+xml",
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn not_found(what: &str) -> Response<Cow<'static, [u8]>> {
    error_page(
        StatusCode::NOT_FOUND,
        "This part of the book is missing",
        what,
    )
}

/// A small readable page shown inside the iframe instead of the chapter.
fn error_page(status: StatusCode, heading: &str, detail: &str) -> Response<Cow<'static, [u8]>> {
    let body = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><style>\
         body {{ font-family: system-ui, sans-serif; color: #444; background: white; margin: 3em; }}\
         code {{ color: #8a1f1f; }}</style>{INJECTED_TAG}</head>\
         <body><h2>{}</h2><p><code>{}</code></p></body></html>",
        escape_html(heading),
        escape_html(detail)
    );
    respond(status, "text/html; charset=utf-8", body.into_bytes())
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn respond(status: StatusCode, media_type: &str, body: Vec<u8>) -> Response<Cow<'static, [u8]>> {
    let mut response = Response::new(Cow::Owned(body));
    *response.status_mut() = status;
    if let Ok(value) = media_type.parse() {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_round_trip() {
        let url = url(7, "OEBPS/text/Chapter 1#?.xhtml");
        assert!(
            url.ends_with("/7/OEBPS/text/Chapter%201%23%3F.xhtml"),
            "{url}"
        );
        let path = url.trim_start_matches(ORIGIN);
        assert_eq!(
            parse_url_path(path),
            Some((7, "OEBPS/text/Chapter 1#?.xhtml".to_owned()))
        );
    }

    #[test]
    fn injects_script_and_falls_back_to_html() {
        let xhtml = b"<html><head><title>t</title></head><body/></html>".to_vec();
        let (media_type, out) = prepare_document("application/xhtml+xml", xhtml);
        assert_eq!(media_type, "application/xhtml+xml");
        assert!(String::from_utf8_lossy(&out).contains(&format!("{INJECTED_TAG}</head>")));

        let broken = b"<html><HEAD></HEAD><body><script async defer></script>".to_vec();
        let (media_type, out) = prepare_document("application/xhtml+xml", broken);
        assert_eq!(media_type, "text/html");
        assert!(String::from_utf8_lossy(&out).contains(&format!("{INJECTED_TAG}</HEAD>")));
    }
}
