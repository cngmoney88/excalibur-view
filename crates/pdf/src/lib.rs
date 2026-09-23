//! Reading and writing PDF files at the object level.
//!
//! Hyperview renders pages with pdfium, but it owns the file itself here. Markup
//! fidelity and the document operations — combine, split, extract, rotate —
//! need to read and write the actual objects, and an incremental writer that
//! never rewrites a byte of the original is the only honest way to save
//! somebody's drawing set.

pub mod doc;
pub mod aes;
pub mod crypt;
pub mod filters;
pub mod repair;
pub mod object;
pub mod opening;
pub mod parse;
pub mod random;
pub mod text;
pub mod write;
pub mod xref;

pub use doc::Document;
pub use object::{Dict, Name, Object, Ref, Stream, StringKind};
pub use write::Update;
pub use parse::{Error, Reader, Result};
