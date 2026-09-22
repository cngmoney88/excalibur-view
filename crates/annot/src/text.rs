//! How words on a markup are set: the font, its size, its weight, and where
//! they sit in the box.
//!
//! A text box whose words are not drawn into the appearance stream shows as an
//! empty rectangle in Acrobat, in a browser and on a general contractor's
//! screen — the same failure as a measurement whose number is not drawn in.
//! So the words go in, with the font somebody chose, wrapped to the box and
//! lined up the way they asked.

use pdf::{Dict, Name, Object};

/// One of the fonts every PDF reader has without being sent one.
///
/// Only the base fourteen. A markup that named a font off somebody's machine
/// would arrive at the general contractor set in whatever their machine had
/// instead, which is how a note that fitted its box stops fitting it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Family {
    #[default]
    Helvetica,
    Times,
    Courier,
}

impl Family {
    pub const ALL: &'static [Family] = &[Family::Helvetica, Family::Times, Family::Courier];

    pub fn name(self) -> &'static str {
        match self {
            Family::Helvetica => "Helvetica",
            Family::Times => "Times",
            Family::Courier => "Courier",
        }
    }

    /// What the PDF calls it, with the weight and slant applied.
    pub fn base_font(self, bold: bool, italic: bool) -> &'static str {
        match (self, bold, italic) {
            (Family::Helvetica, false, false) => "Helvetica",
            (Family::Helvetica, true, false) => "Helvetica-Bold",
            (Family::Helvetica, false, true) => "Helvetica-Oblique",
            (Family::Helvetica, true, true) => "Helvetica-BoldOblique",
            (Family::Times, false, false) => "Times-Roman",
            (Family::Times, true, false) => "Times-Bold",
            (Family::Times, false, true) => "Times-Italic",
            (Family::Times, true, true) => "Times-BoldItalic",
            (Family::Courier, false, false) => "Courier",
            (Family::Courier, true, false) => "Courier-Bold",
            (Family::Courier, false, true) => "Courier-Oblique",
            (Family::Courier, true, true) => "Courier-BoldOblique",
        }
    }

    /// The short name it goes under in the appearance stream's resources.
    pub fn resource_name(self, bold: bool, italic: bool) -> String {
        let letters = match (bold, italic) {
            (false, false) => "",
            (true, false) => "B",
            (false, true) => "I",
            (true, true) => "BI",
        };
        format!(
            "{}{letters}",
            match self {
                Family::Helvetica => "Helv",
                Family::Times => "TiRo",
                Family::Courier => "Cour",
            }
        )
    }

    pub fn from_base_font(name: &str) -> Family {
        if name.starts_with("Times") {
            Family::Times
        } else if name.starts_with("Courier") {
            Family::Courier
        } else {
            Family::Helvetica
        }
    }
}

/// Where the words sit across the box.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Across {
    #[default]
    Left,
    Middle,
    Right,
}

impl Across {
    pub fn from_q(q: i64) -> Across {
        match q {
            1 => Across::Middle,
            2 => Across::Right,
            _ => Across::Left,
        }
    }
    pub fn q(self) -> i64 {
        match self {
            Across::Left => 0,
            Across::Middle => 1,
            Across::Right => 2,
        }
    }
}

/// Where the words sit down the box.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Down {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Everything about how the words are set.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Setting {
    pub family: Family,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub colour: [f32; 3],
    pub across: Across,
    pub down: Down,
    /// Raised or lowered, for the odd note that needs a square foot.
    pub raised: i8,
}

impl Default for Setting {
    fn default() -> Setting {
        Setting {
            family: Family::Helvetica,
            size: 12.0,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            colour: [0.0, 0.0, 0.0],
            across: Across::Left,
            down: Down::Top,
            raised: 0,
        }
    }
}

impl Setting {
    /// Reads how a markup's words are set, from the entries a PDF keeps it in.
    ///
    /// `/DA` is the one every reader agrees on: a scrap of content stream
    /// naming a font, a size and a colour. `/DS` is Revu's richer version and
    /// is read where it says something `/DA` does not.
    pub fn of(markup: &crate::Markup) -> Setting {
        let mut out = Setting {
            colour: markup.colour(),
            ..Setting::default()
        };
        let da = markup.text_of("DA");
        let words: Vec<&str> = da.split_whitespace().collect();
        for (at, word) in words.iter().enumerate() {
            if *word == "Tf" && at >= 2 {
                if let Some(name) = words[at - 2].strip_prefix('/') {
                    out.family = family_of_resource(name);
                    out.bold = name.ends_with('B') || name.ends_with("BI");
                    out.italic = name.ends_with('I');
                }
                if let Ok(size) = words[at - 1].parse::<f64>() {
                    if size > 0.0 {
                        out.size = size.clamp(2.0, 400.0);
                    }
                }
            }
            if *word == "g" && at >= 1 {
                if let Ok(grey) = words[at - 1].parse::<f32>() {
                    out.colour = [grey; 3];
                }
            }
            if *word == "rg" && at >= 3 {
                let read = |s: &str| s.parse::<f32>().ok();
                if let (Some(r), Some(g), Some(b)) =
                    (read(words[at - 3]), read(words[at - 2]), read(words[at - 1]))
                {
                    out.colour = [r, g, b];
                }
            }
        }

        // Revu's style string, where it has one.
        let ds = markup.text_of("DS");
        for part in ds.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("font-size:") {
                if let Ok(size) = rest.trim().trim_end_matches("pt").trim().parse::<f64>() {
                    out.size = size.clamp(2.0, 400.0);
                }
            }
            if let Some(rest) = part.strip_prefix("font-weight:") {
                out.bold = rest.trim() == "bold";
            }
            if let Some(rest) = part.strip_prefix("font-style:") {
                out.italic = rest.trim() == "italic";
            }
            if let Some(rest) = part.strip_prefix("font-family:") {
                out.family = Family::from_base_font(rest.trim().trim_matches('"'));
            }
            if let Some(rest) = part.strip_prefix("text-decoration:") {
                out.underline = rest.contains("underline");
                out.strike = rest.contains("line-through");
            }
        }

        if let Some(q) = markup.dict.get("Q").and_then(|o| o.as_i64()) {
            out.across = Across::from_q(q);
        }
        out
    }

    /// Writes the setting back into the entries a reader will look at.
    pub fn onto(&self, markup: &mut crate::Markup) {
        let resource = self.family.resource_name(self.bold, self.italic);
        let colour = format!(
            "{} {} {} rg",
            pdf::write::number(self.colour[0] as f64),
            pdf::write::number(self.colour[1] as f64),
            pdf::write::number(self.colour[2] as f64)
        );
        markup.dict.set(
            Name::new("DA"),
            Object::text(&format!(
                "/{resource} {} Tf {colour}",
                pdf::write::number(self.size)
            )),
        );
        markup.dict.set(Name::new("Q"), Object::Int(self.across.q()));
        let mut decoration = Vec::new();
        if self.underline {
            decoration.push("underline");
        }
        if self.strike {
            decoration.push("line-through");
        }
        markup.dict.set(
            Name::new("DS"),
            Object::text(&format!(
                "font-family:\"{}\";font-size:{}pt;font-weight:{};font-style:{};text-decoration:{}",
                self.family.base_font(false, false),
                pdf::write::number(self.size),
                if self.bold { "bold" } else { "normal" },
                if self.italic { "italic" } else { "normal" },
                if decoration.is_empty() {
                    "none".to_string()
                } else {
                    decoration.join(" ")
                }
            )),
        );
        // The appearance was drawn with the old setting, so it has to go.
        markup.dict.remove("AP");
    }

    /// One font dictionary for this setting, to hang in the resources.
    pub fn font_dict(&self) -> (String, Dict) {
        let name = self.family.resource_name(self.bold, self.italic);
        let mut dict = Dict::new();
        dict.set("Type", Object::name("Font"));
        dict.set("Subtype", Object::name("Type1"));
        dict.set(
            "BaseFont",
            Object::name(self.family.base_font(self.bold, self.italic)),
        );
        dict.set("Encoding", Object::name("WinAnsiEncoding"));
        (name, dict)
    }
}

fn family_of_resource(name: &str) -> Family {
    let stem = name
        .trim_end_matches("BI")
        .trim_end_matches('B')
        .trim_end_matches('I');
    match stem {
        "TiRo" | "Times" => Family::Times,
        "Cour" | "Courier" => Family::Courier,
        _ => Family::Helvetica,
    }
}

/// Breaks a run of words into lines that fit a width.
///
/// Words that are themselves too long for the box are broken rather than
/// allowed to run out past the edge, because a note that runs off the sheet is
/// a note nobody reads.
pub fn wrap(text: &str, width: f64, setting: &Setting) -> Vec<String> {
    let mut out = Vec::new();
    if width <= 0.0 {
        return text.lines().map(|l| l.to_string()).collect();
    }
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if crate::content::width_of(&candidate, setting.size, setting.bold) <= width
                || line.is_empty()
            {
                line = candidate;
            } else {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            }
            // A single word wider than the box gets broken.
            while crate::content::width_of(&line, setting.size, setting.bold) > width
                && line.chars().count() > 1
            {
                let mut cut = line.chars().count() - 1;
                while cut > 1
                    && crate::content::width_of(
                        &line.chars().take(cut).collect::<String>(),
                        setting.size,
                        setting.bold,
                    ) > width
                {
                    cut -= 1;
                }
                let head: String = line.chars().take(cut).collect();
                let tail: String = line.chars().skip(cut).collect();
                out.push(head);
                line = tail;
            }
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Markup, Subtype};

    #[test]
    fn a_setting_written_and_read_back_is_the_same_setting() {
        let mut markup = Markup::new(Subtype::FreeText);
        let setting = Setting {
            family: Family::Times,
            size: 18.0,
            bold: true,
            italic: false,
            underline: true,
            strike: false,
            colour: [1.0, 0.0, 0.0],
            across: Across::Middle,
            down: Down::Top,
            raised: 0,
        };
        setting.onto(&mut markup);
        let back = Setting::of(&markup);
        assert_eq!(back.family, Family::Times);
        assert_eq!(back.size, 18.0);
        assert!(back.bold);
        assert!(!back.italic);
        assert!(back.underline);
        assert_eq!(back.across, Across::Middle);
        assert_eq!(back.colour, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn a_markup_from_revu_is_read_the_way_revu_wrote_it() {
        // /DA is what every reader agrees on, and it is what a Revu tool
        // carries.
        let mut markup = Markup::new(Subtype::FreeText);
        markup.set("DA", pdf::Object::text("/HelvB 14 Tf 1 0 0 rg"));
        let setting = Setting::of(&markup);
        assert_eq!(setting.size, 14.0);
        assert!(setting.bold);
        assert_eq!(setting.colour, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn every_font_is_one_a_reader_already_has() {
        // A markup naming a font off somebody's machine arrives set in
        // whatever the other machine had instead.
        for family in Family::ALL {
            for bold in [false, true] {
                for italic in [false, true] {
                    let name = family.base_font(bold, italic);
                    assert!(
                        [
                            "Helvetica", "Helvetica-Bold", "Helvetica-Oblique",
                            "Helvetica-BoldOblique", "Times-Roman", "Times-Bold",
                            "Times-Italic", "Times-BoldItalic", "Courier",
                            "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
                        ]
                        .contains(&name),
                        "{name} is not one of the base fourteen"
                    );
                }
            }
        }
    }

    #[test]
    fn words_wrap_to_the_box_they_are_in() {
        let setting = Setting::default();
        let lines = wrap(
            "FIELD VERIFY ALL DIMENSIONS BEFORE FABRICATION SEE DETAIL 3 ON S-501",
            120.0,
            &setting,
        );
        assert!(lines.len() > 3, "{lines:?}");
        for line in &lines {
            assert!(
                crate::content::width_of(line, setting.size, false) <= 121.0,
                "{line:?} is too wide"
            );
        }
    }

    #[test]
    fn a_word_too_long_for_the_box_is_broken_rather_than_left_hanging_out() {
        let setting = Setting::default();
        let lines = wrap("WWWWWWWWWWWWWWWWWWWWWWWWWWWWWW", 40.0, &setting);
        assert!(lines.len() > 1, "{lines:?}");
        for line in &lines {
            assert!(crate::content::width_of(line, setting.size, false) <= 41.0);
        }
    }

    #[test]
    fn line_breaks_somebody_typed_are_kept() {
        let lines = wrap("FIRST\nSECOND", 400.0, &Setting::default());
        assert_eq!(lines, vec!["FIRST".to_string(), "SECOND".to_string()]);
    }
}
