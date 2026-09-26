//! Writes the Android launcher icons, the one Play shows on the store page, and
//! the iPad app's icon.
//!
//! Scan-filled from the same vector the program draws (`ui::mark`), as the
//! Windows icon is, so there is one mark and not a second drawing of it that
//! can drift. Run it when the mark changes:
//!
//! ```text
//!   cargo run -p excalibur-view-mobile --example launcher-icons
//! ```
//!
//! An Android launcher icon is two layers, a background and a foreground, that
//! the launcher masks to its own shape — a circle, a rounded square, a
//! squircle — and may move a little for effect. So the foreground is a 108 dp
//! square with the mark in the middle, small enough that no mask cuts into it.
//! The background is a plain colour, `icon_background` in `colors.xml`.

use std::path::{Path, PathBuf};

/// The foreground layer's size, in dp.
const LAYER: u32 = 108;
/// The mark's size within it, in dp. The shield's shoulders are its farthest
/// points from the middle, at 0.64 of this; 52 puts them 33 dp out, inside the
/// 72 dp circle a round launcher shows.
const MARK: u32 = 52;
/// The screen densities Android asks for, and pixels per dp at each.
const DENSITIES: &[(&str, u32)] = &[
    ("mdpi", 2),
    ("hdpi", 3),
    ("xhdpi", 4),
    ("xxhdpi", 6),
    ("xxxhdpi", 8),
];

/// The Play store icon: 512 pixels square, opaque, and cut to a rounded
/// square by Play itself.
const STORE: u32 = 512;
const STORE_MARK: u32 = 384;
/// `icon_background` in `colors.xml`.
const BACKGROUND: [u8; 4] = [0xff, 0xff, 0xff, 0xff];

fn main() {
    let android = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mobile/android");
    let res = android.join("app/src/main/res");

    for &(density, half_pixels) in DENSITIES {
        // mdpi is one pixel per dp; the others are multiples of a half.
        let layer = LAYER * half_pixels / 2;
        let mark = MARK * half_pixels / 2;
        let image = centred(layer, mark, [0, 0, 0, 0]);
        write(
            &res.join(format!("mipmap-{density}"))
                .join("ic_launcher_foreground.png"),
            &image,
        );
    }

    let store = centred(STORE, STORE_MARK, BACKGROUND);
    write(&android.join("play/icon-512.png"), &store);

    // The iPad's: one 1024-pixel square, opaque (the App Store refuses an
    // icon with transparency in it), which iPadOS rounds and shrinks itself.
    let ios = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mobile/ios");
    let icon = centred(IOS, IOS * STORE_MARK / STORE, BACKGROUND);
    let path = ios.join("Resources/Assets.xcassets/AppIcon.appiconset/icon-1024.png");
    // No alpha channel at all, not merely an opaque one: App Store Connect
    // refuses an icon that has one.
    image::DynamicImage::ImageRgba8(icon).to_rgb8().save(&path).expect("could not write the icon");
    println!("{} (1024×1024, no alpha)", path.display());
}

/// The iPad app's icon.
const IOS: u32 = 1024;

/// A `size` square of `under`, with the mark `mark` pixels across in the middle.
fn centred(size: u32, mark: u32, under: [u8; 4]) -> image::RgbaImage {
    let mut canvas = image::RgbaImage::from_pixel(size, size, image::Rgba(under));
    let drawn = ui::mark::raster(mark as usize);
    let offset = (size - mark) / 2;
    for y in 0..mark {
        for x in 0..mark {
            let i = ((y * mark + x) * 4) as usize;
            let over = [drawn[i], drawn[i + 1], drawn[i + 2], drawn[i + 3]];
            let pixel = canvas.get_pixel_mut(x + offset, y + offset);
            pixel.0 = blend(over, pixel.0);
        }
    }
    canvas
}

/// `over` laid on `under`.
///
/// `over` comes from `ui::mark::raster`, which averages its samples colour and
/// coverage alike, so a pixel half covered by white comes out as half-strength
/// white at half opacity: premultiplied. Taken as straight alpha that would
/// be grey, and every edge would have a dark fringe. `under` and the result
/// are straight, as PNG is.
fn blend(over: [u8; 4], under: [u8; 4]) -> [u8; 4] {
    let a = over[3] as f32 / 255.0;
    let b = under[3] as f32 / 255.0;
    let out = a + b * (1.0 - a);
    if out <= 0.0 {
        return [0, 0, 0, 0];
    }
    let mut result = [0u8; 4];
    for c in 0..3 {
        let value = (over[c] as f32 + under[c] as f32 * b * (1.0 - a)) / out;
        result[c] = value.round().clamp(0.0, 255.0) as u8;
    }
    result[3] = (out * 255.0).round() as u8;
    result
}

fn write(path: &Path, image: &image::RgbaImage) {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).expect("could not make the folder");
    }
    image.save(path).expect("could not write the icon");
    println!("{} ({}×{})", path.display(), image.width(), image.height());
}
