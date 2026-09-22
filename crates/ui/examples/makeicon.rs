//! Writes the Excalibur mark out as a Windows .ico.
//!
//! Generated from the same vector the program draws on screen rather than
//! drawn separately, so the file in Explorer, the taskbar button and the About
//! box can never disagree about what the mark looks like. Run it when the mark
//! changes:
//!
//! ```text
//!   cargo run -p ui --example makeicon -- assets/hyperview.ico
//! ```

use std::io::Write;

/// The sizes Windows actually asks for. Each is scan-filled from the vector at
/// that size rather than resampled from one big one, because a shield with a
/// sword through it at 16 pixels needs the geometry to land on whole pixels,
/// and a downscaled 256 turns it into grey mush.
const SIZES: &[u32] = &[16, 20, 24, 32, 40, 48, 64, 128, 256];

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hyperview.ico".to_string());

    let mut images: Vec<(u32, Vec<u8>)> = Vec::new();
    for &size in SIZES {
        let rgba = ui::mark::raster(size as usize);
        images.push((size, png(size, size, &rgba)));
    }

    let mut file = std::fs::File::create(&out).expect("could not create the icon");
    // ICONDIR
    file.write_all(&0u16.to_le_bytes()).unwrap(); // reserved
    file.write_all(&1u16.to_le_bytes()).unwrap(); // 1 = icon
    file.write_all(&(images.len() as u16).to_le_bytes()).unwrap();

    // Every entry's data follows the whole directory, so the first offset is
    // past all of them.
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        // 256 is written as 0 in a byte, which is the format saying "256".
        let byte = if *size >= 256 { 0u8 } else { *size as u8 };
        file.write_all(&[byte, byte, 0, 0]).unwrap(); // w, h, colours, reserved
        file.write_all(&1u16.to_le_bytes()).unwrap(); // colour planes
        file.write_all(&32u16.to_le_bytes()).unwrap(); // bits per pixel
        file.write_all(&(data.len() as u32).to_le_bytes()).unwrap();
        file.write_all(&offset.to_le_bytes()).unwrap();
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        file.write_all(data).unwrap();
    }
    println!("{out}: {} sizes", images.len());
}

/// A PNG, written by hand.
///
/// Windows has taken PNG-compressed icon entries since Vista, and a PNG is both
/// smaller and simpler than the upside-down BMP-with-a-mask the older format
/// wants. Written here rather than pulled in as a dependency because it is
/// forty lines and this runs once when the mark changes.
fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA, no interlace
    chunk(&mut out, b"IHDR", &ihdr);

    // Each row carries a filter byte. Nought — "no filter" — because these are
    // tiny and the compression is not the point.
    let mut raw = Vec::with_capacity((width * height * 4 + height) as usize);
    for y in 0..height {
        raw.push(0u8);
        let from = (y * width * 4) as usize;
        raw.extend_from_slice(&rgba[from..from + (width * 4) as usize]);
    }
    chunk(&mut out, b"IDAT", &deflate_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc::new();
    crc.eat(kind);
    crc.eat(data);
    out.extend_from_slice(&crc.done().to_be_bytes());
}

/// A zlib stream that does not compress at all.
///
/// Stored blocks are a legal deflate stream, every decoder reads them, and the
/// largest icon here is a quarter of a megabyte before compression. Trading a
/// dependency for that is not worth it.
fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib header, no compression
    let mut rest = data;
    while !rest.is_empty() {
        let take = rest.len().min(65535);
        let last = if take == rest.len() { 1u8 } else { 0u8 };
        out.push(last);
        out.extend_from_slice(&(take as u16).to_le_bytes());
        out.extend_from_slice(&(!(take as u16)).to_le_bytes());
        out.extend_from_slice(&rest[..take]);
        rest = &rest[take..];
    }
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

struct Crc(u32);

impl Crc {
    fn new() -> Crc {
        Crc(0xffff_ffff)
    }
    fn eat(&mut self, data: &[u8]) {
        for byte in data {
            let mut c = (self.0 ^ *byte as u32) & 0xff;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            self.0 = c ^ (self.0 >> 8);
        }
    }
    fn done(self) -> u32 {
        self.0 ^ 0xffff_ffff
    }
}
