//! Turning pictures into something a PDF can hold.
//!
//! A JPEG goes in whole: a PDF's `DCTDecode` filter *is* JPEG, so re-encoding
//! one would lose a generation of quality and gain nothing. Everything else is
//! decoded to plain samples and deflated, which is what `FlateDecode` wants.
//! Transparency travels separately, as a greyscale image the PDF calls a soft
//! mask, because that is the only way a PDF carries it.

use annot::markup::Picture;

/// Reads a picture off disk, ready to go into a file.
pub fn read(path: &std::path::Path) -> Result<Picture, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    from_bytes(&bytes, path)
}

/// The same, from bytes already in hand.
pub fn from_bytes(bytes: &[u8], named: &std::path::Path) -> Result<Picture, String> {
    let name = named
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "that picture".into());

    // A JPEG is already in the form a PDF wants. Only the plain kind, though:
    // a progressive JPEG is a different thing inside, and every reader would
    // show a grey box where the picture should be.
    if let Some((width, height, grey)) = plain_jpeg(bytes) {
        return Ok(Picture {
            width,
            height,
            data: bytes.to_vec(),
            filter: "DCTDecode",
            grey,
            mask: None,
        });
    }

    let decoded = image::load_from_memory(bytes)
        .map_err(|e| format!("{name} could not be read as a picture: {e}"))?;
    Ok(from_image(&decoded.to_rgba8()))
}

/// A picture already in memory as pixels.
pub fn from_image(rgba: &image::RgbaImage) -> Picture {
    let (width, height) = (rgba.width(), rgba.height());
    let mut colour = Vec::with_capacity((width * height * 3) as usize);
    let mut alpha = Vec::with_capacity((width * height) as usize);
    let mut any_transparent = false;
    for pixel in rgba.pixels() {
        colour.extend_from_slice(&[pixel[0], pixel[1], pixel[2]]);
        alpha.push(pixel[3]);
        if pixel[3] != 255 {
            any_transparent = true;
        }
    }
    Picture {
        width,
        height,
        data: pdf::filters::deflate(&colour),
        filter: "FlateDecode",
        grey: false,
        mask: any_transparent.then(|| pdf::filters::deflate(&alpha)),
    }
}

/// A picture from what the renderer drew, which is what a snapshot is.
pub fn from_canvas(image: &egui::ColorImage) -> Picture {
    let (width, height) = (image.width() as u32, image.height() as u32);
    let mut colour = Vec::with_capacity((width * height * 3) as usize);
    for pixel in image.pixels.iter() {
        let [r, g, b, _] = pixel.to_array();
        colour.extend_from_slice(&[r, g, b]);
    }
    Picture {
        width,
        height,
        data: pdf::filters::deflate(&colour),
        filter: "FlateDecode",
        grey: false,
        mask: None,
    }
}

/// The size of a baseline JPEG, and whether it is grey, or `None` if this is
/// not a JPEG a PDF can carry as it stands.
///
/// Reading the markers rather than decoding: a JPEG that goes in whole must
/// not be decoded at all, and the only things needed are the size and how many
/// colour channels it has.
fn plain_jpeg(bytes: &[u8]) -> Option<(u32, u32, bool)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut at = 2usize;
    while at + 3 < bytes.len() {
        if bytes[at] != 0xFF {
            at += 1;
            continue;
        }
        let marker = bytes[at + 1];
        // Padding and the standalone markers carry no length.
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if matches!(marker, 0xD8 | 0x01 | 0xD0..=0xD7) {
            at += 2;
            continue;
        }
        let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        match marker {
            // Baseline and extended sequential: what a PDF can carry as it is.
            0xC0 | 0xC1 => {
                if at + 9 >= bytes.len() {
                    return None;
                }
                let height = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]) as u32;
                let width = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]) as u32;
                let channels = bytes[at + 9];
                if width == 0 || height == 0 {
                    return None;
                }
                return match channels {
                    1 => Some((width, height, true)),
                    3 => Some((width, height, false)),
                    // CMYK in a JPEG inside a PDF needs a /Decode array that
                    // depends on who wrote it. Decoding it is the honest
                    // answer rather than guessing.
                    _ => None,
                };
            }
            // Progressive, arithmetic, lossless, hierarchical: all real JPEGs
            // and none of them something to hand a PDF reader unchanged.
            0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None,
            0xDA => return None,
            _ => at += 2 + length,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn something_that_is_not_a_jpeg_is_not_taken_for_one() {
        assert!(plain_jpeg(b"\x89PNG\r\n\x1a\n").is_none());
        assert!(plain_jpeg(b"").is_none());
        assert!(plain_jpeg(b"\xff\xd8").is_none());
    }

    #[test]
    fn a_baseline_jpeg_is_read_without_being_decoded() {
        // Start of image, then a start-of-frame saying 40 wide by 30 down in
        // three colours, which is what a photograph of a seal looks like.
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        bytes.extend_from_slice(&30u16.to_be_bytes());
        bytes.extend_from_slice(&40u16.to_be_bytes());
        bytes.push(3);
        assert_eq!(plain_jpeg(&bytes), Some((40, 30, false)));
    }

    #[test]
    fn a_progressive_jpeg_is_decoded_rather_than_handed_over_whole() {
        // Every reader would show a grey box for one of these written as
        // DCTDecode, so it has to go the long way round.
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xC2, 0x00, 0x11, 0x08]);
        bytes.extend_from_slice(&30u16.to_be_bytes());
        bytes.extend_from_slice(&40u16.to_be_bytes());
        bytes.push(3);
        assert!(plain_jpeg(&bytes).is_none());
    }

    #[test]
    fn a_picture_with_no_transparency_carries_no_mask() {
        let flat = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        let picture = from_image(&flat);
        assert_eq!((picture.width, picture.height), (4, 3));
        assert_eq!(picture.filter, "FlateDecode");
        assert!(picture.mask.is_none(), "nothing see-through, nothing to mask");
    }

    #[test]
    fn a_picture_with_transparency_carries_it_separately() {
        // A PDF has no place for a fourth channel in the picture itself, so a
        // seal scanned on a transparent background needs its own mask or it
        // arrives on a black square.
        let mut image = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        image.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        let picture = from_image(&image);
        assert!(picture.mask.is_some());
    }
}
