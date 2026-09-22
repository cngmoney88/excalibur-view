//! Checking the spelling of what somebody typed on a drawing.
//!
//! Not a general English spell checker: those flag every part number, every
//! grid reference and every shape designation on a structural drawing, and a
//! checker that cries wolf on "W12x26" is one people turn off in the first
//! minute.
//!
//! What this checks against instead is **the drawing set\'s own words**, plus a
//! short list of ordinary English kept here. A word typed in a markup that
//! appears nowhere on the drawings and is not ordinary English is worth
//! looking at — and the suggestion offered is the nearest word that *is* on
//! the drawing, which is nearly always the one that was meant. Type
//! "RENFORCING" over a sheet that says REINFORCING and it says so, and says
//! what the sheet says.
//!
//! Numbers, part numbers, anything with a digit in it, and anything under
//! three letters are left alone, because those are never spelling mistakes on
//! a drawing.

use std::collections::BTreeSet;

/// A word worth looking at, and what the drawing says instead.
#[derive(Clone, Debug, PartialEq)]
pub struct Doubt {
    /// Which markup, by its place in the list.
    pub markup: usize,
    pub page: u32,
    pub word: String,
    /// The nearest word on the drawing, when there is one near enough to be
    /// worth offering.
    pub perhaps: Option<String>,
}

/// Ordinary English, and the words a drawing office uses. Short on purpose:
/// this is the list that keeps a checker from flagging "the", not a
/// dictionary.
pub const ORDINARY: &[&str] = &[
    "about", "above", "across", "after", "again", "against", "aisc", "all",
    "almost", "alone", "along", "already", "also", "although", "always", "among",
    "an", "anchor", "anchors", "and", "angle", "angles", "another", "any",
    "anybody", "anyone", "anything", "anyway", "apply", "approve", "approved", "approximate",
    "area", "areas", "around", "as", "aside", "ask", "astm", "at",
    "attach", "attached", "available", "away", "awd", "aws", "back", "base",
    "based", "baseplate", "basic", "be", "beam", "beams", "bearing", "because",
    "become", "been", "before", "begin", "behind", "being", "below", "beneath",
    "bent", "beside", "best", "better", "between", "beyond", "bolt", "bolts",
    "both", "bottom", "box", "boxes", "bracing", "bracket", "brackets", "build",
    "building", "buildings", "built", "but", "by", "call", "called", "camber",
    "can", "cannot", "cantilever", "capacity", "center", "centre", "certain", "change",
    "changed", "channel", "channels", "check", "chord", "clear", "clearance", "clip",
    "clips", "close", "closed", "collar", "column", "columns", "come", "coming",
    "complete", "completed", "concrete", "condition", "conditions", "confirm", "connection", "connections",
    "consider", "construction", "contact", "continue", "continuous", "contract", "contractor", "coordinate",
    "coordinated", "coping", "copy", "corbel", "corner", "corners", "correct", "could",
    "countersink", "coupler", "course", "cover", "covered", "crane", "cross", "curb",
    "current", "cut", "cuts", "date", "dates", "day", "days", "deck",
    "decking", "deep", "deflection", "depth", "design", "designed", "detail", "detailed",
    "details", "diameter", "diaphragm", "did", "differ", "different", "dimension", "dimensions",
    "direct", "direction", "discuss", "do", "does", "done", "door", "doors",
    "double", "dowel", "dowels", "down", "drain", "drawing", "drawings", "drill",
    "drilled", "drop", "due", "during", "each", "east", "edge", "edges",
    "either", "elevation", "elevations", "else", "embed", "embeds", "end", "ends",
    "engineer", "engineering", "enough", "entire", "equal", "equipment", "erect", "erection",
    "erector", "etc", "even", "every", "everything", "exact", "example", "except",
    "existing", "expansion", "exterior", "extra", "fabricate", "fabricated", "fabrication", "face",
    "faces", "far", "fasten", "fastener", "fasteners", "few", "field", "fill",
    "filled", "fillet", "final", "find", "finish", "finished", "fire", "first",
    "fit", "fits", "five", "fix", "fixed", "flange", "flanges", "flashing",
    "flat", "floor", "floors", "flute", "follow", "following", "foot", "footing",
    "footings", "for", "force", "forces", "form", "forms", "found", "foundation",
    "four", "frame", "framed", "framing", "from", "front", "full", "furnish",
    "further", "galvanized", "gauge", "general", "get", "girder", "girders", "girt",
    "girts", "give", "given", "go", "grade", "grades", "grating", "grid",
    "grids", "ground", "group", "grout", "guard", "gusset", "gussets", "had",
    "half", "hand", "handrail", "hanger", "hangers", "has", "haunch", "have",
    "he", "head", "headed", "header", "heads", "height", "heights", "held",
    "help", "her", "here", "high", "higher", "hole", "holes", "hollow",
    "horizontal", "hot", "hour", "how", "however", "hss", "hvac", "if",
    "impact", "in", "inch", "inches", "include", "included", "including", "indicate",
    "indicated", "information", "inside", "install", "installation", "installed", "interior", "into",
    "is", "it", "its", "itself", "jamb", "job", "joint", "joints",
    "joist", "joists", "just", "keep", "kept", "key", "kicker", "kind",
    "knee", "know", "ladder", "large", "larger", "last", "later", "lateral",
    "layout", "least", "leave", "ledger", "left", "length", "less", "let",
    "level", "levels", "light", "like", "limit", "line", "lines", "lintel",
    "list", "load", "loads", "local", "locate", "located", "location", "locations",
    "long", "look", "low", "lower", "made", "main", "maintain", "major",
    "make", "makes", "making", "manufacturer", "many", "mark", "marked", "mason",
    "masonry", "material", "materials", "maximum", "may", "me", "mean", "means",
    "measure", "measured", "measurement", "meet", "member", "members", "metal", "meter",
    "method", "middle", "might", "minimum", "miscellaneous", "miter", "mm", "modify",
    "moment", "more", "most", "mount", "mounted", "move", "much", "must",
    "nailer", "name", "near", "need", "needed", "nelson", "new", "next",
    "no", "north", "not", "note", "noted", "notes", "nothing", "now",
    "number", "numbers", "nut", "nuts", "of", "off", "offset", "on",
    "once", "one", "only", "ope", "open", "opening", "openings", "opposite",
    "or", "order", "other", "others", "otherwise", "out", "outrigger", "outside",
    "over", "overall", "own", "paint", "painted", "pair", "panel", "panels",
    "parapet", "part", "parts", "pass", "pedestal", "penetration", "per", "perimeter",
    "pier", "piers", "pintle", "pipe", "place", "placed", "plan", "plank",
    "plans", "plate", "plates", "please", "plumb", "plus", "ply", "point",
    "points", "position", "possible", "post", "posts", "pour", "poured", "precast",
    "prefabricated", "prepare", "prior", "project", "provide", "provided", "provision", "purlin",
    "purlins", "put", "quality", "quantity", "radius", "rail", "railing", "rebar",
    "reference", "referenced", "reinforce", "reinforced", "reinforcement", "reinforcing", "remove", "removed",
    "repair", "replace", "report", "required", "requirement", "requirements", "respond", "review",
    "reviewed", "revise", "revised", "revision", "revisions", "rib", "ribs", "right",
    "rise", "riser", "risers", "rod", "rods", "rolled", "roof", "roofs",
    "room", "rooms", "round", "row", "rows", "run", "running", "saddle",
    "sag", "sags", "said", "same", "say", "schedule", "schedules", "scope",
    "screw", "screws", "seal", "sealant", "seat", "second", "section", "sections",
    "see", "seismic", "self", "send", "set", "sets", "several", "shall",
    "shape", "shapes", "shear", "sheet", "sheets", "shim", "shims", "ship",
    "shop", "shoring", "should", "show", "shown", "side", "sides", "sill",
    "similar", "site", "six", "size", "sizes", "slab", "slabs", "sleeve",
    "sleeves", "slope", "sloped", "slotted", "small", "smaller", "snug", "so",
    "soffit", "soil", "sole", "some", "south", "space", "spaced", "spacer",
    "spacing", "span", "spandrel", "spans", "special", "specification", "specifications", "specified",
    "splice", "splices", "stair", "stairs", "standard", "standards", "start", "steel",
    "step", "steps", "stiffener", "stiffeners", "still", "stirrup", "stirrups", "stock",
    "stop", "straight", "strap", "strength", "stringer", "structural", "structure", "structures",
    "stud", "studs", "subject", "submit", "submittal", "submittals", "such", "sump",
    "support", "supported", "supports", "surcharge", "surface", "survey", "system", "systems",
    "table", "take", "taken", "tee", "temporary", "ten", "test", "testing",
    "than", "that", "the", "their", "them", "then", "there", "therefore",
    "these", "they", "thick", "thickness", "thin", "this", "those", "threaded",
    "three", "through", "throughout", "thru", "thus", "tie", "ties", "tight",
    "time", "to", "toe", "together", "tolerance", "top", "torque", "total",
    "toward", "tread", "treads", "treated", "trim", "truss", "trusses", "tube",
    "tubes", "turn", "two", "type", "types", "typical", "under", "unit",
    "units", "unless", "until", "up", "uplift", "upon", "upper", "use",
    "used", "using", "usually", "value", "verify", "vertical", "very", "view",
    "wall", "walls", "was", "washer", "washers", "way", "we", "weather",
    "web", "weep", "weld", "welded", "welding", "weldment", "welds", "well",
    "were", "west", "what", "when", "where", "whether", "which", "while",
    "who", "whole", "why", "wide", "width", "will", "wind", "window",
    "windows", "wing", "with", "within", "without", "work", "working", "would",
    "write", "written", "wt", "yield", "you", "your", "zee", "zinc",
    "zone", "zones",
];

/// Whether a word is worth checking at all.
///
/// Anything with a digit in it is a part number, a grid reference or a shape:
/// W12X26, S-201, 3/4", A992. None of those is ever a spelling mistake, and
/// flagging them is how a checker gets turned off.
pub fn worth_checking(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    letters >= 3
        && !word.chars().any(|c| c.is_ascii_digit())
        && word.chars().all(|c| c.is_alphabetic() || c == '\'' || c == '-')
}

/// Breaks a piece of text into the words worth checking.
pub fn words_in(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '-'))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// How far apart two words are, counting a swap of neighbours as one step.
///
/// Neighbour swaps matter here because the commonest typing mistake on a
/// keyboard is exactly that, and a checker that counted "TEH" as two steps
/// from "THE" would rank the right suggestion below the wrong one.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut grid = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in grid.iter_mut().enumerate().take(n + 1) {
        row[0] = i;
    }
    for j in 0..=m {
        grid[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let same = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let mut best = (grid[i - 1][j] + 1)
                .min(grid[i][j - 1] + 1)
                .min(grid[i - 1][j - 1] + same);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(grid[i - 2][j - 2] + 1);
            }
            grid[i][j] = best;
        }
    }
    grid[n][m]
}

/// The nearest word on the drawing, when one is near enough to be worth
/// offering.
///
/// "Near enough" grows with the length of the word, because one wrong letter
/// in four is a different word and one wrong letter in twelve is a typing
/// mistake.
pub fn nearest(word: &str, known: &BTreeSet<String>) -> Option<String> {
    let lower = word.to_lowercase();
    let allowed = match lower.chars().count() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    let mut best: Option<(String, usize)> = None;
    for candidate in known {
        if candidate.len().abs_diff(lower.len()) > allowed {
            continue;
        }
        let away = distance(&lower, candidate);
        if away == 0 || away > allowed {
            continue;
        }
        if best.as_ref().map(|(_, b)| away < *b).unwrap_or(true) {
            best = Some((candidate.clone(), away));
        }
    }
    best.map(|(word, _)| word)
}

/// Every word the drawing set itself uses, which is the dictionary that
/// matters.
pub fn known_from(pages: &[String]) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = ORDINARY.iter().map(|w| w.to_string()).collect();
    for page in pages {
        for word in words_in(page) {
            if worth_checking(&word) {
                out.insert(word.to_lowercase());
            }
        }
    }
    out
}

/// Looks over what somebody typed and says which words are worth a second
/// look.
pub fn check(
    markups: &[(usize, u32, String)],
    known: &BTreeSet<String>,
) -> Vec<Doubt> {
    let mut out = Vec::new();
    let mut already: BTreeSet<String> = BTreeSet::new();
    for (index, page, text) in markups {
        for word in words_in(text) {
            if !worth_checking(&word) {
                continue;
            }
            let lower = word.to_lowercase();
            if known.contains(&lower) {
                continue;
            }
            // One line per word per markup: a note with "RENFORCING" twice is
            // one thing to fix, not two.
            let seen = format!("{index}:{lower}");
            if !already.insert(seen) {
                continue;
            }
            out.push(Doubt {
                markup: *index,
                page: *page,
                perhaps: nearest(&word, known),
                word,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(words: &[&str]) -> BTreeSet<String> {
        known_from(&[words.join(" ")])
    }

    #[test]
    fn a_part_number_is_never_a_spelling_mistake() {
        // A checker that cries wolf on W12X26 is one people turn off in the
        // first minute.
        assert!(!worth_checking("W12X26"));
        assert!(!worth_checking("S-201"));
        assert!(!worth_checking("A992"));
        assert!(!worth_checking("3/4"));
        assert!(!worth_checking("4"));
        assert!(!worth_checking("OK"));
        assert!(worth_checking("REINFORCING"));
    }

    #[test]
    fn a_word_the_drawing_uses_is_not_questioned() {
        let dictionary = known(&["REINFORCING", "GALVANIZED", "SPANDREL"]);
        let found = check(
            &[(0, 0, "SEE REINFORCING AT SPANDREL".into())],
            &dictionary,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_mistyped_word_is_caught_and_the_drawings_own_spelling_offered() {
        let dictionary = known(&["REINFORCING", "GALVANIZED"]);
        let found = check(&[(3, 2, "RENFORCING AT GRID 4".into())], &dictionary);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].word, "RENFORCING");
        assert_eq!(found[0].perhaps.as_deref(), Some("reinforcing"));
        assert_eq!(found[0].markup, 3);
        assert_eq!(found[0].page, 2);
    }

    #[test]
    fn ordinary_english_is_left_alone_even_on_a_drawing_that_never_says_it() {
        let dictionary = known(&["NOTHING"]);
        let found = check(&[(0, 0, "please verify the connection before welding".into())], &dictionary);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn two_letters_swapped_counts_as_one_mistake() {
        // The commonest typing mistake there is. Counting it as two would rank
        // the right suggestion below the wrong one.
        assert_eq!(distance("teh", "the"), 1);
        assert_eq!(distance("weld", "weld"), 0);
        assert_eq!(distance("weld", "welds"), 1);
    }

    #[test]
    fn a_word_nothing_is_near_is_flagged_with_nothing_offered() {
        // Better than offering something wrong: a suggestion somebody accepts
        // without reading is a mistake this program made.
        let dictionary = known(&["REINFORCING"]);
        let found = check(&[(0, 0, "QQQQZZZXW".into())], &dictionary);
        assert_eq!(found.len(), 1);
        assert!(found[0].perhaps.is_none(), "{:?}", found[0]);
    }

    #[test]
    fn the_same_word_twice_in_one_note_is_one_thing_to_fix() {
        let dictionary = known(&["REINFORCING"]);
        let found = check(
            &[(0, 0, "RENFORCING AND RENFORCING".into())],
            &dictionary,
        );
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_long_word_is_allowed_more_slack_than_a_short_one() {
        // One wrong letter in four is a different word; one in twelve is a
        // typing mistake.
        let dictionary = known(&["SPECIFICATIONS"]);
        // Three letters out of fourteen is still the word somebody meant.
        assert_eq!(
            nearest("SPECIFCATONS", &dictionary),
            Some("specifications".to_string())
        );
        // One letter out of four is a different word, and nothing is offered.
        let short = known(&["HSST"]);
        assert!(nearest("ZZZZ", &short).is_none());
    }
}

// ---- the window ------------------------------------------------------------

use crate::app::App;

/// What is on the screen while somebody looks through the doubts.
pub struct Checking {
    pub doubts: Vec<Doubt>,
    /// Which one is being looked at.
    pub at: usize,
    /// Words told to leave alone for the rest of this session.
    pub allowed: BTreeSet<String>,
    pub said: String,
}

impl App {
    /// Reads every word somebody typed on this drawing against the drawing's
    /// own words.
    pub fn begin_spell_check(&mut self) {
        let Some(doc) = self.doc() else {
            self.status = "Open a drawing first.".into();
            return;
        };
        let path = doc.path.clone();
        let sheets = doc.pages.len();
        let typed: Vec<(usize, u32, String)> = doc
            .marks
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.gone)
            .filter_map(|(at, m)| {
                let words = format!("{} {}", m.markup.contents(), m.markup.subject());
                (!words.trim().is_empty()).then(|| (at, m.page, words))
            })
            .collect();
        if typed.is_empty() {
            self.status = "There are no words on this drawing to check.".into();
            return;
        }

        // The drawing's own words are the dictionary that matters. Reading
        // every sheet of a big set takes a moment, so it is read once and
        // said out loud that it is happening.
        let library = self.svc.library.clone();
        let mut pages = Vec::with_capacity(sheets);
        for page in 0..sheets {
            match crate::render::words_on(library.clone(), &path, page as u32) {
                Ok(words) => pages.push(words),
                Err(_) => pages.push(String::new()),
            }
        }
        let known = known_from(&pages);
        let doubts = check(&typed, &known);

        let count = doubts.len();
        self.status = if count == 0 {
            "Nothing in the markups looks misspelt.".into()
        } else {
            format!(
                "{count} word{} worth a second look.",
                if count == 1 { "" } else { "s" }
            )
        };
        self.checking = Some(Checking {
            said: format!(
                "Checked against every word on the {sheets} sheet{} of this drawing, \
                 and against ordinary English. Part numbers, grid references and \
                 anything with a digit in it are left alone.",
                if sheets == 1 { "" } else { "s" }
            ),
            doubts,
            at: 0,
            allowed: BTreeSet::new(),
        });
    }

    pub fn spell_window(&mut self, ctx: &egui::Context) {
        let Some(mut checking) = self.checking.take() else {
            return;
        };
        let theme = self.chrome.theme;
        let mut keep = true;
        let mut go_to: Option<usize> = None;
        let mut leave_alone: Option<String> = None;
        let mut put_right: Option<(usize, String, String)> = None;

        egui::Window::new("Spell Check")
            .open(&mut keep)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(&checking.said)
                        .color(theme.faint)
                        .size(10.0),
                );
                ui.add_space(10.0);

                let showing: Vec<&Doubt> = checking
                    .doubts
                    .iter()
                    .filter(|d| !checking.allowed.contains(&d.word.to_lowercase()))
                    .collect();
                if showing.is_empty() {
                    ui.label(
                        egui::RichText::new("Nothing left to look at.").size(12.0),
                    );
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for doubt in &showing {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(false, format!("“{}”", doubt.word))
                                    .clicked()
                                {
                                    go_to = Some(doubt.markup);
                                }
                                ui.label(
                                    egui::RichText::new(format!("sheet {}", doubt.page + 1))
                                        .color(theme.faint)
                                        .size(10.0),
                                );
                                match &doubt.perhaps {
                                    Some(instead) => {
                                        if ui
                                            .small_button(format!("→ {instead}"))
                                            .on_hover_text(
                                                "The drawing itself spells it this way.",
                                            )
                                            .clicked()
                                        {
                                            put_right = Some((
                                                doubt.markup,
                                                doubt.word.clone(),
                                                instead.clone(),
                                            ));
                                        }
                                    }
                                    None => {
                                        ui.label(
                                            egui::RichText::new("nothing near it")
                                                .color(theme.faint)
                                                .size(10.0),
                                        );
                                    }
                                }
                                if ui.small_button("Leave it").clicked() {
                                    leave_alone = Some(doubt.word.to_lowercase());
                                }
                            });
                        }
                    });
            });

        if let Some(word) = leave_alone {
            checking.allowed.insert(word);
        }
        if let Some(index) = go_to {
            let page = self
                .doc()
                .and_then(|d| d.marks.get(index))
                .map(|m| m.page);
            if let Some(page) = page {
                self.go_to(page);
            }
            if let Some(doc) = self.doc_mut() {
                doc.choose(Some(index));
            }
        }
        if let Some((index, was, instead)) = put_right {
            // Put in with the same case as what was typed, because a drawing
            // set in capitals stays in capitals.
            let replacement = match_case(&was, &instead);
            if let Some(doc) = self.doc_mut() {
                doc.checkpoint_named("Correct a word");
                if let Some(mark) = doc.marks.get_mut(index) {
                    let now = mark.markup.contents().replace(&was, &replacement);
                    mark.markup.set_contents(&now);
                    mark.changed = true;
                }
                doc.dirty = true;
            }
            checking
                .doubts
                .retain(|d| !(d.markup == index && d.word == was));
            self.status = format!("“{was}” put right as “{replacement}”.");
        }
        if keep {
            self.checking = Some(checking);
        }
    }
}

/// Puts a suggestion in the same case as what it replaces.
///
/// A drawing set is in capitals. A checker that corrected REINFORCING to
/// "reinforcing" would be making the note wrong in a different way.
pub fn match_case(was: &str, instead: &str) -> String {
    let letters: Vec<char> = was.chars().filter(|c| c.is_alphabetic()).collect();
    let all_caps = !letters.is_empty() && letters.iter().all(|c| c.is_uppercase());
    let first_caps = letters.first().map(|c| c.is_uppercase()).unwrap_or(false);
    if all_caps {
        return instead.to_uppercase();
    }
    if first_caps {
        let mut chars = instead.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => instead.to_string(),
        };
    }
    instead.to_string()
}

#[cfg(test)]
mod case_tests {
    use super::*;

    #[test]
    fn a_drawing_in_capitals_stays_in_capitals() {
        assert_eq!(match_case("RENFORCING", "reinforcing"), "REINFORCING");
        assert_eq!(match_case("Renforcing", "reinforcing"), "Reinforcing");
        assert_eq!(match_case("renforcing", "reinforcing"), "reinforcing");
    }
}
