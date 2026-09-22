//! Takeoff: measuring markups and totalling them.
//!
//! The one rule everything here is built around: **nothing is estimated.**
//! Every quantity traces back to points somebody clicked, measured against a
//! scale somebody set. A markup on a sheet with no scale is reported and left
//! out of the totals; it is never counted as zero.

pub mod columns;
pub mod doubles;
pub mod export;
pub mod fill;
pub mod geometry;
pub mod grid;
pub mod revision;
pub mod row;
pub mod shoplist;
pub mod snap;
pub mod summary;
pub mod symbols;
pub mod user;

pub use columns::Column;
pub use row::{measure, read_document, Row, WeightColumns};
pub use summary::{summarise, Group, Summary};
