//! Markups: reading, drawing and writing PDF annotations, in the dialect
//! Bluebeam uses so that files move both ways without losing anything.

pub mod appearance;
pub mod content;
pub mod markup;
pub mod page;
pub mod measure;
pub mod name;
pub mod place;
pub mod text;
pub mod viewport;

pub use appearance::Appearance;
pub use markup::{Kind, Markup, Subtype};
pub use page::Frame;
pub use measure::Measure;

/// Formats a number for the style strings that go into `/DS` and `/RC`.
pub fn number_text(value: f64) -> String {
    pdf::write::number(value)
}
