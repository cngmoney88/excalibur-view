//! Making a scanned drawing searchable.
//!
//! Half the sets a fabricator receives are scans: somebody photographed a
//! drawing, or printed a PDF and scanned it back in, and what arrives is a
//! picture of a drawing with no text in it at all. Search finds nothing, the
//! sheet list has no sheet numbers, and slip-sheeting cannot match anything.
//! OCR is what turns that back into a drawing this program can work with.
//!
//! **What it does not do is change the drawing.** The recognised words are
//! added as an *invisible* text layer sitting exactly over the marks they were
//! read from. The picture on the page is untouched — what prints is what was
//! always there — and what changes is that the words underneath can now be
//! found, selected and read. That is also why a wrong reading is survivable: it
//! makes a search miss, not a drawing wrong.
//!
//! And it never affects a quantity. OCR feeds search and sheet numbers. Every
//! measurement in this program still comes from somebody clicking on the
//! drawing.

use std::path::Path;

/// One word an engine recognised, and where it was.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    /// In the pixels of the image given to the engine: left, top, right,
    /// bottom, with top at zero.
    pub area: [f32; 4],
    /// Nought to one. Used to drop the noise a scanner leaves, never to change
    /// what a word says.
    pub confidence: f32,
}

impl Word {
    pub fn width(&self) -> f32 {
        (self.area[2] - self.area[0]).max(0.0)
    }

    pub fn height(&self) -> f32 {
        (self.area[3] - self.area[1]).max(0.0)
    }
}

/// Anything that can read words off a picture.
///
/// A trait because the answer differs per platform and neither answer should
/// leak into the rest of the program: Windows has an OCR engine built into it,
/// and everywhere else there is Tesseract if somebody installed it.
pub trait Engine: Send {
    /// Reads a greyscale image. `width` and `height` are in pixels; `grey` is
    /// one byte per pixel, row by row from the top.
    fn read(&self, grey: &[u8], width: u32, height: u32) -> Result<Vec<Word>, String>;

    /// What to tell somebody about where the reading came from.
    fn name(&self) -> String;
}

/// Below this, a "word" is a speck on the glass.
pub const SURE_ENOUGH: f32 = 0.30;

/// Words worth keeping.
///
/// Three tests, and each one exists because of something a scanner does. A
/// reading nobody is confident about is usually dirt. A word with no letters or
/// digits in it is a mark on the page that happened to look like punctuation. A
/// word taller than it is wide by a factor of ten is the edge of a table being
/// read as a row of Is.
pub fn worth_keeping(words: Vec<Word>) -> Vec<Word> {
    words
        .into_iter()
        .filter(|word| {
            if word.confidence < SURE_ENOUGH {
                return false;
            }
            let trimmed = word.text.trim();
            if trimmed.is_empty() {
                return false;
            }
            if !trimmed.chars().any(|c| c.is_alphanumeric()) {
                return false;
            }
            let (w, h) = (word.width(), word.height());
            if w <= 0.0 || h <= 0.0 {
                return false;
            }
            if h > w * 10.0 {
                return false;
            }
            true
        })
        .collect()
}

/// A word placed on the page, in the page's own points with y up from the
/// bottom — which is what a PDF text object needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub text: String,
    pub x: f32,
    pub y: f32,
    /// The point size that makes this word about as wide as the marks it was
    /// read from.
    pub size: f32,
}

/// Turns recognised words into text objects' worth of placement.
///
/// The size is worked out from the width rather than taken from the height,
/// because what matters is that a word selects over the marks it belongs to: a
/// person dragging across "S-201" on a scan should get "S-201", not half of it
/// and a space.
pub fn place(words: &[Word], image: (u32, u32), page: (f32, f32)) -> Vec<Placed> {
    let (iw, ih) = (image.0.max(1) as f32, image.1.max(1) as f32);
    let across = page.0 / iw;
    let down = page.1 / ih;
    words
        .iter()
        .filter_map(|word| {
            let wide_in_points = word.width() * across;
            if wide_in_points <= 0.0 || word.text.trim().is_empty() {
                return None;
            }
            // The size that makes this text that wide, using the same estimate
            // the stamping code uses, so both agree about how wide Helvetica is.
            let at_one_point = crate::stamps::about_as_wide(word.text.trim(), 1.0).max(0.01);
            let size = (wide_in_points / at_one_point).clamp(1.0, 400.0);
            Some(Placed {
                text: word.text.trim().to_string(),
                x: word.area[0] * across,
                // Image space counts down from the top; a PDF counts up from
                // the bottom. The baseline sits at the bottom of the word.
                y: page.1 - word.area[3] * down,
                size,
            })
        })
        .collect()
}

// ---- Tesseract, where somebody has it -------------------------------------

/// Tesseract, driven as a command.
///
/// Through its command-line rather than by linking it, because linking it means
/// shipping it, and a drawing viewer that is forty megabytes larger for
/// something most sets do not need is the wrong trade. On Windows the engine
/// built into the operating system is used instead and nothing has to be
/// installed at all.
pub struct Tesseract;

impl Tesseract {
    /// Whether it is on this machine.
    pub fn available() -> bool {
        std::process::Command::new("tesseract")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Engine for Tesseract {
    fn name(&self) -> String {
        "Tesseract".into()
    }

    fn read(&self, grey: &[u8], width: u32, height: u32) -> Result<Vec<Word>, String> {
        let scratch = std::env::temp_dir().join(format!(
            "hyperview-ocr-{:016x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        let picture = scratch.join("page.pgm");
        write_pgm(&picture, grey, width, height)?;

        // TSV, because it carries a box and a confidence per word. `--psm 11`
        // is "sparse text": a drawing is not a page of prose and the default
        // layout analysis tries to find columns that are not there.
        let out = std::process::Command::new("tesseract")
            .arg(&picture)
            .arg("stdout")
            .args(["--psm", "11", "-c", "tessedit_create_tsv=1", "tsv"])
            .output();
        let _ = std::fs::remove_dir_all(&scratch);
        let out = out.map_err(|e| format!("tesseract could not be run: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "tesseract refused: {}",
                String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")
            ));
        }
        Ok(read_tsv(&String::from_utf8_lossy(&out.stdout)))
    }
}

/// A greyscale image, written the simplest way anything reads one.
fn write_pgm(to: &Path, grey: &[u8], width: u32, height: u32) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::File::create(to).map_err(|e| e.to_string())?;
    write!(file, "P5\n{width} {height}\n255\n").map_err(|e| e.to_string())?;
    file.write_all(grey).map_err(|e| e.to_string())?;
    Ok(())
}

/// Tesseract's word-per-row output.
pub fn read_tsv(tsv: &str) -> Vec<Word> {
    let mut out = Vec::new();
    for line in tsv.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        // level, page, block, para, line, word, left, top, width, height, conf, text
        if cells.len() < 12 {
            continue;
        }
        let text = cells[11].trim();
        if text.is_empty() {
            continue;
        }
        let (Ok(left), Ok(top), Ok(width), Ok(height)) = (
            cells[6].parse::<f32>(),
            cells[7].parse::<f32>(),
            cells[8].parse::<f32>(),
            cells[9].parse::<f32>(),
        ) else {
            continue;
        };
        // Tesseract reports confidence out of a hundred, and -1 for a row that
        // is a block rather than a word.
        let confidence = cells[10].parse::<f32>().unwrap_or(-1.0);
        if confidence < 0.0 {
            continue;
        }
        out.push(Word {
            text: text.to_string(),
            area: [left, top, left + width, top + height],
            confidence: confidence / 100.0,
        });
    }
    out
}


// ---- the recogniser built into Windows -------------------------------------

/// Windows' own text recogniser.
///
/// Present on every Windows 10 and 11 machine, needs nothing installed, and is
/// good at exactly the kind of text a drawing has — short, upright, in a
/// title block — which is the text that matters here. That it is already there
/// is the point: "make this scan searchable" should be a button, not a support
/// call about installing something.
#[cfg(windows)]
pub struct WindowsOcr;

#[cfg(windows)]
impl WindowsOcr {
    /// Whether this machine has a recogniser for any language.
    ///
    /// A Windows install with no OCR language pack has the API and no engine
    /// behind it, and asking it to read something then fails in a way that
    /// reads like a bug rather than like a missing language pack.
    pub fn available() -> bool {
        use windows::Media::Ocr::OcrEngine;
        // The call fails outright when no language pack is installed, which is
        // exactly the question being asked.
        OcrEngine::TryCreateFromUserProfileLanguages().is_ok()
    }

    /// Greyscale bytes into the bitmap the API wants.
    fn bitmap(
        grey: &[u8],
        width: u32,
        height: u32,
    ) -> Result<windows::Graphics::Imaging::SoftwareBitmap, String> {
        use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};

        // Grey expanded to BGRA, because that is the format the recogniser
        // takes and converting here is cheaper than converting twice inside it.
        let mut bgra = Vec::with_capacity(grey.len() * 4);
        for value in grey {
            bgra.extend_from_slice(&[*value, *value, *value, 255]);
        }
        let bitmap = SoftwareBitmap::Create(
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
        )
        .map_err(|e| format!("Windows could not make a bitmap: {e}"))?;
        let bitmap = SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8)
            .unwrap_or(bitmap);
        let _ = BitmapAlphaMode::Ignore;

        {
            use windows::Graphics::Imaging::BitmapBufferAccessMode;
            let buffer = bitmap
                .LockBuffer(BitmapBufferAccessMode::Write)
                .map_err(|e| format!("Windows would not lend its bitmap: {e}"))?;
            let reference = buffer
                .CreateReference()
                .map_err(|e| format!("Windows would not lend its bitmap: {e}"))?;
            use windows::core::Interface;
            use windows::Win32::System::WinRT::IMemoryBufferByteAccess;
            let access: IMemoryBufferByteAccess = reference
                .cast()
                .map_err(|e| format!("Windows would not lend its bitmap: {e}"))?;
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut capacity: u32 = 0;
            unsafe {
                access
                    .GetBuffer(&mut data, &mut capacity)
                    .map_err(|e| format!("Windows would not lend its bitmap: {e}"))?;
                if data.is_null() {
                    return Err("Windows gave back an empty bitmap".into());
                }
                let room = (capacity as usize).min(bgra.len());
                std::ptr::copy_nonoverlapping(bgra.as_ptr(), data, room);
            }
        }
        Ok(bitmap)
    }
}

#[cfg(windows)]
impl Engine for WindowsOcr {
    fn name(&self) -> String {
        "the text recogniser built into Windows".into()
    }

    fn read(&self, grey: &[u8], width: u32, height: u32) -> Result<Vec<Word>, String> {
        use windows::Media::Ocr::OcrEngine;

        if width == 0 || height == 0 {
            return Ok(Vec::new());
        }
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|e| format!("Windows has no text recogniser set up: {e}"))?;
        let bitmap = WindowsOcr::bitmap(grey, width, height)?;
        let result = engine
            .RecognizeAsync(&bitmap)
            .and_then(|task| task.get())
            .map_err(|e| format!("Windows could not read the sheet: {e}"))?;

        let mut out = Vec::new();
        let lines = result
            .Lines()
            .map_err(|e| format!("Windows gave back nothing readable: {e}"))?;
        for line in lines {
            let Ok(words) = line.Words() else { continue };
            for word in words {
                let (Ok(text), Ok(box_)) = (word.Text(), word.BoundingRect()) else {
                    continue;
                };
                let text = text.to_string();
                if text.trim().is_empty() {
                    continue;
                }
                out.push(Word {
                    text,
                    area: [
                        box_.X,
                        box_.Y,
                        box_.X + box_.Width,
                        box_.Y + box_.Height,
                    ],
                    // Windows does not report a confidence per word. Treated as
                    // sure rather than as unknown, because the alternative is
                    // throwing away every word it found; the shape tests in
                    // `worth_keeping` still drop the obvious rubbish.
                    confidence: 1.0,
                });
            }
        }
        Ok(out)
    }
}

/// Whatever engine this machine has.
///
/// Windows' own first, because it is already there and needs nothing. Tesseract
/// after it, for a Linux box or a machine where somebody has installed it
/// deliberately.
pub fn engine_here() -> Option<Box<dyn Engine>> {
    #[cfg(windows)]
    {
        if WindowsOcr::available() {
            return Some(Box::new(WindowsOcr));
        }
    }
    if Tesseract::available() {
        return Some(Box::new(Tesseract));
    }
    None
}

/// What to tell somebody when there is none.
pub const NO_ENGINE: &str = "There is no text recognition available on this computer. \
    On Windows, Excalibur View uses the recogniser built into Windows itself. Elsewhere it \
    uses Tesseract, which has to be installed separately.";

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, area: [f32; 4], confidence: f32) -> Word {
        Word {
            text: text.into(),
            area,
            confidence,
        }
    }

    #[test]
    fn a_reading_nobody_is_sure_about_is_dropped() {
        let kept = worth_keeping(vec![
            word("S-201", [10.0, 10.0, 90.0, 30.0], 0.94),
            word("s", [4.0, 200.0, 8.0, 206.0], 0.05),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "S-201");
    }

    #[test]
    fn a_mark_that_is_not_a_word_is_dropped() {
        // A speck read as punctuation, which every scan produces.
        let kept = worth_keeping(vec![
            word(".", [4.0, 4.0, 8.0, 8.0], 0.9),
            word("---", [4.0, 4.0, 40.0, 8.0], 0.9),
            word("3", [4.0, 4.0, 12.0, 20.0], 0.9),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "3");
    }

    #[test]
    fn a_table_edge_read_as_letters_is_dropped() {
        // Ten times taller than wide: a rule, not a word.
        let kept = worth_keeping(vec![word("l", [10.0, 10.0, 12.0, 400.0], 0.8)]);
        assert!(kept.is_empty());
    }

    #[test]
    fn a_word_of_no_size_is_dropped_rather_than_dividing_by_nothing() {
        let kept = worth_keeping(vec![word("S-201", [10.0, 10.0, 10.0, 10.0], 0.9)]);
        assert!(kept.is_empty());
    }

    #[test]
    fn a_word_lands_where_it_was_read_from() {
        // A 1000x500 image of a 2000x1000 point page: everything doubles.
        let words = vec![word("S-201", [100.0, 50.0, 300.0, 90.0], 0.9)];
        let placed = place(&words, (1000, 500), (2000.0, 1000.0));
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].x, 200.0);
        // The bottom of the word in image space is 90; the page is 1000 points
        // tall and the image 500 pixels, so that is 180 down from the top and
        // 820 up from the bottom.
        assert_eq!(placed[0].y, 820.0);
    }

    #[test]
    fn a_word_is_sized_to_cover_the_marks_it_was_read_from() {
        let words = vec![word("S-201", [0.0, 0.0, 200.0, 40.0], 0.9)];
        let placed = place(&words, (1000, 500), (1000.0, 500.0));
        // Whatever the size works out to, writing the word at it should come
        // out about as wide as the box it came from — that is the whole point.
        let wide = crate::stamps::about_as_wide("S-201", placed[0].size);
        assert!((wide - 200.0).abs() < 2.0, "{wide} should be about 200");
    }

    #[test]
    fn tesseracts_own_output_is_read_the_way_it_writes_it() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   5\t1\t1\t1\t1\t1\t100\t50\t200\t40\t96.2\tS-201\n\
                   4\t1\t1\t1\t1\t0\t0\t0\t10\t10\t-1\t\n\
                   5\t1\t1\t1\t1\t2\t400\t50\t300\t40\t88.0\tFRAMING\n";
        let words = read_tsv(tsv);
        assert_eq!(words.len(), 2, "the -1 row is not a word");
        assert_eq!(words[0].text, "S-201");
        assert_eq!(words[0].area, [100.0, 50.0, 300.0, 90.0]);
        assert!((words[0].confidence - 0.962).abs() < 0.001);
        assert_eq!(words[1].text, "FRAMING");
    }

    #[test]
    fn output_that_is_not_what_was_expected_produces_no_words_rather_than_nonsense() {
        assert!(read_tsv("").is_empty());
        assert!(read_tsv("something else entirely").is_empty());
        assert!(read_tsv("a\tb\tc\n1\t2\t3\n").is_empty());
    }
}
