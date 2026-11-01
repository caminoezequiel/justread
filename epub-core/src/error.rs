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

/// Shorthand used throughout the crate: `Result<T>` means `Result<T, EpubError>`.
pub type Result<T> = std::result::Result<T, EpubError>;
