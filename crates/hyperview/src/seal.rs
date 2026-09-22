//! Sealing a set, and saying what a seal is and is not.
//!
//! An engineer's seal on a shop drawing is two things that get spoken about as
//! one, and a program that blurs them is a program that lets somebody believe
//! they have something they do not.
//!
//! **The seal on the sheet** is a picture: the embossed stamp, and usually a
//! signature and a date beside it. It is what the plan reviewer looks for and
//! what gets printed. Placing one is what this module does, and it does it
//! properly — as a real image on the page, on the sheets chosen, in a new file.
//!
//! **A digital signature** is something else entirely: a cryptographic seal
//! over the file's bytes, made with a certificate, which a reader checks and
//! which proves the file has not been altered since. Hyperview does **not** do
//! that yet, and does not pretend to. A picture of a seal is not a signature,
//! and an implementation of PDF signing that has not been checked against the
//! readers a plan reviewer actually uses is worse than none — it would produce
//! files that look signed and might not be.
//!
//! What is offered in the meantime is the honest half of it: a **fingerprint**
//! of the file as issued, which anybody can recompute to show that the set in
//! their hands is the set that was sent. That is not a signature — it proves
//! nothing about who made it — and it is described that way wherever it
//! appears.

use std::path::{Path, PathBuf};

/// Where a seal goes on a sheet, in the sheet's own points, measured from the
/// bottom-left as a PDF does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    /// How wide to draw it. The height follows from the picture's proportions,
    /// because a seal squashed out of round is a seal somebody will query.
    pub width: f32,
    /// A corner to sit in, worked out per sheet. `None` means the x and y
    /// above are the answer — which is what a seal placed by hand on one
    /// drawing wants, while a stamp put on a whole batch of different paper
    /// sizes wants the corner.
    pub corner: Option<crate::stamps::Spot>,
}

impl Default for Placement {
    fn default() -> Placement {
        // Bottom right, above where a title block's own text usually sits.
        Placement {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            corner: None,
        }
    }
}

impl Placement {
    /// The box this fills, given the picture's own proportions.
    pub fn box_for(&self, picture: (u32, u32)) -> [f32; 4] {
        let (pw, ph) = (picture.0.max(1) as f32, picture.1.max(1) as f32);
        let height = self.width * (ph / pw);
        [self.x, self.y, self.x + self.width, self.y + height]
    }

    /// A sensible spot on a sheet of this size, for somebody who has not chosen
    /// one: in from the bottom-right corner, clear of the very edge.
    pub fn suggested(page: (f32, f32), width: f32) -> Placement {
        let margin = (page.0 * 0.02).clamp(18.0, 72.0);
        Placement {
            x: (page.0 - width - margin).max(0.0),
            y: margin,
            width,
            corner: None,
        }
    }

    /// A placement that finds its own spot on whatever sheet it lands on.
    pub fn at(spot: crate::stamps::Spot, width: f32) -> Placement {
        Placement {
            x: 0.0,
            y: 0.0,
            width,
            corner: Some(spot),
        }
    }

    /// Where this actually sits on one sheet, once the paper size and the
    /// picture's proportions are known.
    pub fn on(&self, page: (f32, f32), picture: (u32, u32)) -> Placement {
        let Some(spot) = self.corner else {
            return *self;
        };
        use crate::stamps::Spot;
        let (pw, ph) = (picture.0.max(1) as f32, picture.1.max(1) as f32);
        let height = self.width * (ph / pw);
        let margin = (page.0 * 0.02).clamp(18.0, 72.0);
        let x = match spot {
            Spot::TopLeft | Spot::BottomLeft => margin,
            Spot::TopMiddle | Spot::BottomMiddle => (page.0 - self.width) * 0.5,
            Spot::TopRight | Spot::BottomRight => (page.0 - self.width - margin).max(0.0),
        };
        let y = match spot {
            Spot::TopLeft | Spot::TopMiddle | Spot::TopRight => {
                (page.1 - height - margin).max(0.0)
            }
            _ => margin,
        };
        Placement {
            x,
            y,
            width: self.width,
            corner: None,
        }
    }
}

/// What a seal job is.
#[derive(Clone, Debug, PartialEq)]
pub struct Sealing {
    pub source: PathBuf,
    /// The picture: a PNG or JPEG of the seal, and usually the signature with
    /// it. Somebody's own file, not something this program invents.
    pub picture: PathBuf,
    pub pages: Vec<u32>,
    pub placement: Placement,
    pub to: PathBuf,
}

/// The fingerprint of a file as issued.
///
/// Plain SHA-256 over the bytes, written in groups so somebody can read it down
/// a telephone. Not a signature: it says two files are the same file, and
/// nothing at all about who made either.
pub fn fingerprint(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| format!("could not read that file: {e}"))?;
    let digest = Sha256::digest(&bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok(in_groups(&hex))
}

/// Four characters at a time, which is how a person reads a long string back
/// without losing their place.
pub fn in_groups(hex: &str) -> String {
    hex.as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

/// What to say next to a fingerprint, every time one is shown.
pub const WHAT_A_FINGERPRINT_IS: &str = "\
This is a fingerprint of the file, not a signature. Two files with the same \
fingerprint are the same file, byte for byte. It says nothing about who made \
the file or who sealed it — anybody can compute one. Send it separately from \
the drawings if you want somebody to be able to check what they received is \
what you sent.";

/// What to say about digital signing, until there is one.
pub const NO_DIGITAL_SIGNATURE: &str = "\
Excalibur View places the seal on the sheet. It does not yet add a cryptographic \
digital signature to the file, and it will not pretend to: a picture of a seal \
is not a signature, and signing that has not been checked against the readers a \
plan reviewer actually uses would produce files that look signed and might not \
be. If a submittal requires a digitally signed PDF, seal it here and sign it \
with whatever your engineer already uses.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seal_keeps_its_proportions() {
        // A seal squashed out of round is a seal somebody queries.
        let placement = Placement {
            x: 100.0,
            y: 50.0,
            width: 200.0,
            corner: None,
        };
        let box_ = placement.box_for((400, 300));
        assert_eq!(box_[0], 100.0);
        assert_eq!(box_[1], 50.0);
        assert_eq!(box_[2], 300.0);
        // 200 wide from a 4:3 picture is 150 tall.
        assert_eq!(box_[3], 200.0);
    }

    #[test]
    fn a_square_seal_stays_square() {
        let box_ = Placement { x: 0.0, y: 0.0, width: 180.0, corner: None }.box_for((512, 512));
        assert_eq!(box_[2] - box_[0], box_[3] - box_[1]);
    }

    #[test]
    fn the_suggested_spot_is_inside_the_sheet() {
        for page in [(3024.0f32, 2160.0f32), (612.0, 792.0), (200.0, 200.0)] {
            let placement = Placement::suggested(page, 180.0);
            assert!(placement.x >= 0.0, "{page:?}");
            assert!(placement.y >= 0.0);
            let box_ = placement.box_for((512, 512));
            assert!(box_[2] <= page.0 + 0.01, "{page:?} gave {box_:?}");
        }
    }

    #[test]
    fn a_fingerprint_is_readable_down_a_telephone() {
        let grouped = in_groups("0123456789abcdef0123456789abcdef");
        assert_eq!(grouped, "0123 4567 89ab cdef 0123 4567 89ab cdef");
        // And it is still the same string underneath.
        assert_eq!(grouped.replace(' ', "").len(), 32);
    }

    #[test]
    fn the_same_file_fingerprints_the_same_and_a_changed_one_does_not() {
        let dir = std::env::temp_dir().join("hyperview-fingerprint-test");
        std::fs::create_dir_all(&dir).unwrap();
        let one = dir.join("a.pdf");
        let two = dir.join("b.pdf");
        std::fs::write(&one, b"the set as issued").unwrap();
        std::fs::write(&two, b"the set as issued").unwrap();
        assert_eq!(fingerprint(&one).unwrap(), fingerprint(&two).unwrap());

        std::fs::write(&two, b"the set as issued.").unwrap();
        assert_ne!(fingerprint(&one).unwrap(), fingerprint(&two).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fingerprint_of_something_that_is_not_there_says_so() {
        assert!(fingerprint(Path::new("/no/such/file")).is_err());
    }

    #[test]
    fn the_program_says_plainly_that_a_seal_is_not_a_signature() {
        // This is the sentence that stops somebody submitting a sealed-looking
        // file believing it is digitally signed.
        assert!(WHAT_A_FINGERPRINT_IS.contains("not a signature"));
        assert!(NO_DIGITAL_SIGNATURE.contains("does not yet"));
        assert!(NO_DIGITAL_SIGNATURE.contains("is not a signature"));
    }
}

// ---- the window ------------------------------------------------------------

use crate::app::App;
use egui::RichText;

impl App {
    /// Fingerprints the drawing as it is on disk, and says what that is worth.
    pub fn show_fingerprint(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "Open a drawing first.".into();
            return;
        };
        let path = doc.path.clone();
        // Saved first, so the fingerprint is of what is actually in the file
        // rather than of the file as it was before somebody's last three
        // markups. A fingerprint of a stale file is worse than none.
        let unsaved = doc.dirty;
        if unsaved {
            self.save_now();
        }
        match fingerprint(&path) {
            Ok(text) => {
                self.fingerprint = Some((
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    text,
                ));
            }
            Err(why) => self.error = Some(why),
        }
    }

    pub fn fingerprint_window(&mut self, ctx: &egui::Context) {
        let Some((name, text)) = self.fingerprint.clone() else {
            return;
        };
        let theme = self.chrome.theme;
        let mut open = true;
        let mut copy = false;

        egui::Window::new("Fingerprint")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_width(700.0)
            .show(ctx, |ui| {
                ui.set_min_width(460.0);
                ui.label(RichText::new(&name).strong().size(12.0));
                ui.add_space(10.0);
                let mut shown = text.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut shown)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(WHAT_A_FINGERPRINT_IS)
                        .color(theme.faint)
                        .size(10.0),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Copy").clicked() {
                        copy = true;
                    }
                });
            });

        if copy {
            ctx.copy_text(text);
            self.status = "The fingerprint is on the clipboard.".into();
        }
        if !open {
            self.fingerprint = None;
        }
    }
}
