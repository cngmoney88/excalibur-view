//! PDF text strings.
//!
//! A string in a PDF is bytes, not UTF-8. It is either UTF-16 with a byte order
//! mark or PDFDocEncoding, which is Latin-1 with a handful of typographic
//! characters slotted into the control range. Reading one as UTF-8 turns a
//! degree sign into a replacement character, which is how a measurement ends up
//! reading `90.00?` instead of `90.00°`.

/// The PDFDocEncoding characters that differ from Latin-1, at 0x18..0x20 and
/// 0x80..0xA0.
const LOW: [char; 8] = ['\u{2d8}', '\u{2c7}', '\u{2c6}', '\u{2d9}', '\u{2dd}', '\u{2db}', '\u{2da}', '\u{2dc}'];
const HIGH: [char; 32] = [
    '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', '\u{2014}', '\u{2013}', '\u{192}', '\u{2044}',
    '\u{2039}', '\u{203a}', '\u{2212}', '\u{2030}', '\u{201e}', '\u{201c}', '\u{201d}', '\u{2018}',
    '\u{2019}', '\u{201a}', '\u{2122}', '\u{fb01}', '\u{fb02}', '\u{141}', '\u{152}', '\u{160}',
    '\u{178}', '\u{17d}', '\u{131}', '\u{142}', '\u{153}', '\u{161}', '\u{17e}', '\u{fffd}',
];

pub fn decode(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        return utf16(&bytes[2..], true);
    }
    // A few writers use little endian despite the specification.
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        return utf16(&bytes[2..], false);
    }
    bytes
        .iter()
        .map(|b| match b {
            0x18..=0x1F => LOW[(*b - 0x18) as usize],
            0x80..=0x9F => HIGH[(*b - 0x80) as usize],
            other => *other as char,
        })
        .collect()
}

fn utf16(bytes: &[u8], big_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| {
            if big_endian {
                u16::from_be_bytes([c[0], c[1]])
            } else {
                u16::from_le_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Cleans a string that came out of another program: trailing NUL bytes,
/// and the invisible direction marks Revit puts in sheet numbers.
pub fn tidy(text: &str) -> String {
    text.chars()
        .filter(|c| {
            *c != '\0' && !matches!(*c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{feff}')
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Encodes a string the narrow way when every character fits, and as UTF-16
/// with a byte order mark when one does not.
pub fn encode(text: &str) -> Vec<u8> {
    let mut narrow = Vec::with_capacity(text.len());
    let mut fits = true;
    for ch in text.chars() {
        match ch {
            '\u{0}'..='\u{17}' | '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => {
                narrow.push(ch as u8)
            }
            _ => match HIGH.iter().position(|c| *c == ch) {
                Some(at) => narrow.push(0x80 + at as u8),
                None => {
                    fits = false;
                    break;
                }
            },
        }
    }
    if fits {
        return narrow;
    }
    let mut out = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_degree_sign_reads_as_a_degree_sign() {
        assert_eq!(decode(&[0xB0]), "\u{b0}");
        assert_eq!(decode(b"90.00\xb0"), "90.00\u{b0}");
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(decode(b"W12x26"), "W12x26");
        assert_eq!(decode(b"20'-6 1/2\""), "20'-6 1/2\"");
    }

    #[test]
    fn utf16_with_a_byte_order_mark_is_recognised() {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in "Fox West Théâtre".encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(decode(&bytes), "Fox West Théâtre");
    }

    #[test]
    fn the_typographic_characters_in_the_control_range_are_not_control_codes() {
        assert_eq!(decode(&[0x8B]), "\u{2030}", "per mille, not a control code");
        assert_eq!(decode(&[0x92]), "\u{2122}", "trademark, not a control code");
        assert_eq!(decode(&[0x8D]), "\u{201c}");
    }

    #[test]
    fn writing_and_reading_come_back_to_the_same_text() {
        for text in [
            "W12x26",
            "20'-6 1/2\"",
            "90.00\u{b0}",
            "ABBREVIATIONS, SYMBOLS & SCHEDULES",
            "Théâtre",
            "\u{4e2d}\u{6587}",
        ] {
            assert_eq!(decode(&encode(text)), text, "{text}");
        }
    }

    #[test]
    fn the_rubbish_revit_leaves_on_a_sheet_number_is_cleaned_off() {
        assert_eq!(tidy("A0.01\0"), "A0.01");
        assert_eq!(tidy("D2.00\u{200f}\0"), "D2.00");
        assert_eq!(tidy("  S-200  "), "S-200");
        assert_eq!(tidy("ABBREVIATIONS, SYMBOLS & SCHEDULES"), "ABBREVIATIONS, SYMBOLS & SCHEDULES");
    }

    #[test]
    fn something_outside_the_narrow_encoding_goes_out_as_utf16() {
        let bytes = encode("\u{4e2d}");
        assert_eq!(&bytes[..2], &[0xFE, 0xFF]);
    }
}
