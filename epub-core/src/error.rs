use thiserror::Error;

/// Everything that can go wrong while reading an EPUB.
///
/// Library code returns this concrete enum (via `thiserror`) so callers can
/// match on the failure; the app wraps it in `anyhow::Error` for display.
///
/// Variants that wrap another error expose it through `source()` and do not
/// repeat it in their message, so `anyhow`'s "Caused by:" chain shows each
/// cause exactly once.
#[derive(Debug, Error)]
pub enum EpubError {
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    #[error("not a valid zip archive")]
    Zip(#[from] zip::result::ZipError),

    #[error("file not found inside the EPUB: {0}")]
    MissingFile(String),

    #[error("{0} is not valid UTF-8")]
    NotUtf8(String),

    #[error("invalid XML in {path}")]
    Xml {
        path: String,
        #[source]
        source: roxmltree::Error,
    },

    #[error("invalid EPUB structure: {0}")]
    Invalid(String),

    #[error("unsupported or unsafe href: {0:?}")]
    BadHref(String),
}

impl EpubError {
    /// This error and all of its causes on one line, e.g.
    /// `invalid XML in OPS/nav.xhtml: unexpected end of stream`.
    pub fn full_message(&self) -> String {
        let mut message = self.to_string();
        // `source()` comes from the `std::error::Error` trait and returns
        // `Option<&dyn Error>`: a *trait object*, i.e. "some type that
        // implements Error", whose concrete type is only known at runtime.
        let mut source = std::error::Error::source(self);
        while let Some(cause) = source {
            message.push_str(": ");
            message.push_str(&cause.to_string());
            source = cause.source();
        }
        message
    }
}

/// Shorthand used throughout the crate: `Result<T>` means `Result<T, EpubError>`.
pub type Result<T> = std::result::Result<T, EpubError>;
