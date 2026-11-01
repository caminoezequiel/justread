//! Reading EPUB files straight from the zip, without extracting them.
//!
//! ```no_run
//! let book = epub_core::Book::open("book.epub")?;
//! println!("{:?}", book.metadata().title);
//! # Ok::<(), epub_core::EpubError>(())
//! ```

mod book;
mod error;
mod href;
mod opf;
mod toc;
mod xml;

// Re-export the public API at the crate root so users write
// `epub_core::Book` rather than `epub_core::book::Book`.
pub use book::{Book, Contents, SpineItem};
pub use error::{EpubError, Result};
pub use href::Href;
pub use opf::Metadata;
pub use toc::{TocEntry, flatten};
pub use xml::is_well_formed as is_well_formed_xml;
