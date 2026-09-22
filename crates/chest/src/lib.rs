//! Tool chests: Excalibur View's own `.evtools` files (see `native`), and
//! Revu profiles and tool sets read in on the way to becoming one.
//!
//! A `.bpx` profile carries a firm's whole way of working: which toolbars are
//! up, which panels are open, which columns the markups list shows, and the
//! tool chest itself. A `.btx` is one tool set on its own.
//!
//! Both are UTF-8 XML in which several leaf values are zlib streams written as
//! lowercase hex. The interesting one is `Raw`, which decompresses to a literal
//! PDF annotation dictionary — so a tool is not a description of a markup, it
//! *is* the markup, ready to be stamped onto a page.

pub mod native;
pub mod profile;
pub mod toolset;
pub mod xml;

pub use profile::{CustomColumn, Panels, Profile, ToolBar};
pub use annot::Kind;
pub use toolset::{Tool, ToolSet};

use std::io::Read;

/// Decodes one of the zlib-over-hex values these files are full of.
pub fn unpack(hex: &str) -> Option<Vec<u8>> {
    let hex = hex.trim();
    if hex.is_empty() || hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let raw = hex.as_bytes();
    for pair in raw.chunks(2) {
        let hi = value(pair[0])?;
        let lo = value(pair[1])?;
        bytes.push(hi * 16 + lo);
    }
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(&bytes[..])
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

/// The same, as text.
pub fn unpack_text(hex: &str) -> Option<String> {
    unpack(hex).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_value_comes_back_as_text() {
        // The Title of his General Measurements set, straight out of the file.
        assert_eq!(
            unpack_text("789c734fcd4b2d4acc51f04d4d2c2e2d4acd4dcd2b2906004f2d07d8").as_deref(),
            Some("General Measurements")
        );
    }

    #[test]
    fn rubbish_is_refused_rather_than_guessed_at() {
        assert!(unpack("").is_none());
        assert!(unpack("abc").is_none(), "odd length");
        assert!(unpack("zzzz").is_none(), "not hex");
        assert!(unpack("deadbeef").is_none(), "hex, but not zlib");
    }
}
