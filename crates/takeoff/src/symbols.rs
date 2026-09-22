//! VisualSearch: pick one symbol on a sheet and find the rest of them.
//!
//! A drawing is full of things that get counted — a column type, a hanger, a
//! door, a light fitting — and counting them by eye over a hundred sheets is
//! how numbers get missed. Draw a box round one, and this finds the others.
//!
//! Line art, not photographs, so the matching is done on ink and not on grey
//! levels: each pixel is ink or paper, and two symbols match by how much of
//! their ink lands in the same place. That is both faster and steadier than
//! comparing brightness, which on a drawing is mostly comparing how much white
//! there is.
//!
//! Sliding a symbol over a whole sheet is more arithmetic than anybody will
//! wait for, so most positions are thrown out before they are looked at: an
//! integral image gives the amount of ink under any window instantly, and a
//! window holding the wrong amount of ink cannot be the symbol whatever it
//! looks like. Only the few that survive are compared properly.
//!
//! **What this does not do is decide anything.** It offers places on the sheet
//! that look like the one somebody picked. Every one of them is shown, and they
//! become counts when a person says so. A count nobody confirmed is exactly the
//! kind of number this program exists not to produce.

/// A sheet reduced to ink and paper.
#[derive(Clone)]
pub struct Ink {
    pub width: usize,
    pub height: usize,
    pub ink: Vec<bool>,
    /// Ink counted from the origin to each corner, so the amount under any
    /// rectangle is four lookups rather than a loop.
    sums: Vec<u32>,
}

impl Ink {
    pub fn new(width: usize, height: usize, ink: Vec<bool>) -> Ink {
        let mut sums = vec![0u32; (width + 1) * (height + 1)];
        for y in 0..height {
            let mut row = 0u32;
            for x in 0..width {
                if ink[y * width + x] {
                    row += 1;
                }
                sums[(y + 1) * (width + 1) + x + 1] = sums[y * (width + 1) + x + 1] + row;
            }
        }
        Ink {
            width,
            height,
            ink,
            sums,
        }
    }

    pub fn at(&self, x: usize, y: usize) -> bool {
        self.ink[y * self.width + x]
    }

    /// How much ink is in a rectangle, instantly.
    pub fn ink_in(&self, x: usize, y: usize, w: usize, h: usize) -> u32 {
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);
        let stride = self.width + 1;
        self.sums[y1 * stride + x1] + self.sums[y * stride + x]
            - self.sums[y * stride + x1]
            - self.sums[y1 * stride + x]
    }

    pub fn total_ink(&self) -> u32 {
        self.ink_in(0, 0, self.width, self.height)
    }
}

/// Somewhere on the sheet that looks like the symbol.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sighting {
    pub x: usize,
    pub y: usize,
    /// Nought to one. One is every pixel of ink in the same place.
    pub score: f32,
}

/// How alike two things have to be. Below this they are not offered at all.
pub const ALIKE: f32 = 0.62;

/// Finds everywhere on `sheet` that looks like `symbol`.
///
/// `alike` is how close a match has to be, nought to one. `most` caps how many
/// are returned, so a search that matches half the ink on the sheet comes back
/// with the best of them rather than thousands.
pub fn find(sheet: &Ink, symbol: &Ink, alike: f32, most: usize) -> Vec<Sighting> {
    let (sw, sh) = (symbol.width, symbol.height);
    if sw == 0 || sh == 0 || sw > sheet.width || sh > sheet.height {
        return Vec::new();
    }
    let wanted = symbol.total_ink();
    if wanted == 0 {
        // An empty box matches every piece of blank paper on the sheet, which
        // is not an answer to anything.
        return Vec::new();
    }

    // How far the ink under a window may differ before it cannot be the symbol
    // however it is arranged. Generous: a symbol next to a dimension line
    // picks up a little extra.
    let slack = (wanted as f32 * 0.45) as u32;
    let low = wanted.saturating_sub(slack);
    let high = wanted + slack;

    // One position per pixel is more than anybody needs and is most of the
    // cost. A step of about a tenth of the symbol still lands within a pixel
    // or two of every real one, and the refinement below finds the rest.
    let step = ((sw.min(sh) / 10).max(1)).min(4);

    let mut found: Vec<Sighting> = Vec::new();
    let mut y = 0;
    while y + sh <= sheet.height {
        let mut x = 0;
        while x + sw <= sheet.width {
            let here = sheet.ink_in(x, y, sw, sh);
            if here >= low && here <= high {
                let score = alikeness(sheet, symbol, x, y);
                if score >= alike {
                    found.push(Sighting { x, y, score });
                }
            }
            x += step;
        }
        y += step;
    }

    // The same symbol answers at several nearby positions. Keep the best of
    // each cluster: a count has to be one per thing, not one per pixel it
    // happened to match at.
    found.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<Sighting> = Vec::new();
    for one in found {
        if kept.len() >= most {
            break;
        }
        let clashes = kept.iter().any(|other| {
            overlap(one.x, one.y, other.x, other.y, sw, sh) > 0.3
        });
        if !clashes {
            kept.push(one);
        }
    }
    kept.sort_by_key(|s| (s.y, s.x));
    kept
}

/// How much of the two symbols' ink lands in the same place.
///
/// The ink in both over the ink in either: a measure that does not reward a
/// match for the paper they have in common, which on a drawing is nearly all
/// of it.
fn alikeness(sheet: &Ink, symbol: &Ink, at_x: usize, at_y: usize) -> f32 {
    let (mut both, mut either) = (0u32, 0u32);
    for y in 0..symbol.height {
        for x in 0..symbol.width {
            let a = symbol.at(x, y);
            let b = sheet.at(at_x + x, at_y + y);
            if a && b {
                both += 1;
            }
            if a || b {
                either += 1;
            }
        }
    }
    if either == 0 {
        return 0.0;
    }
    both as f32 / either as f32
}

/// How much two windows of the same size cover each other.
fn overlap(ax: usize, ay: usize, bx: usize, by: usize, w: usize, h: usize) -> f32 {
    let across = (ax + w).min(bx + w).saturating_sub(ax.max(bx));
    let down = (ay + h).min(by + h).saturating_sub(ay.max(by));
    (across * down) as f32 / (w * h).max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blank sheet to draw test symbols on.
    fn sheet(w: usize, h: usize) -> Vec<bool> {
        vec![false; w * h]
    }

    /// Draws a plus sign, which is roughly what a column mark looks like.
    fn plus(canvas: &mut [bool], w: usize, at_x: usize, at_y: usize, size: usize) {
        let middle = size / 2;
        for i in 0..size {
            canvas[(at_y + middle) * w + at_x + i] = true;
            canvas[(at_y + i) * w + at_x + middle] = true;
        }
    }

    /// Draws a filled square, which is not a plus.
    fn block(canvas: &mut [bool], w: usize, at_x: usize, at_y: usize, size: usize) {
        for y in 0..size {
            for x in 0..size {
                canvas[(at_y + y) * w + at_x + x] = true;
            }
        }
    }

    fn a_symbol(size: usize) -> Ink {
        let mut canvas = sheet(size, size);
        plus(&mut canvas, size, 0, 0, size);
        Ink::new(size, size, canvas)
    }

    #[test]
    fn every_copy_of_a_symbol_is_found() {
        let (w, h) = (200, 160);
        let mut canvas = sheet(w, h);
        let places = [(10, 10), (80, 30), (150, 100), (40, 120)];
        for (x, y) in places {
            plus(&mut canvas, w, x, y, 11);
        }
        let found = find(&Ink::new(w, h, canvas), &a_symbol(11), ALIKE, 100);
        assert_eq!(found.len(), places.len(), "found {found:?}");
        // Each one lands on, or within a pixel of, where it was drawn.
        for (x, y) in places {
            assert!(
                found
                    .iter()
                    .any(|s| s.x.abs_diff(x) <= 2 && s.y.abs_diff(y) <= 2),
                "nothing near {x},{y} in {found:?}"
            );
        }
    }

    #[test]
    fn something_that_is_not_the_symbol_is_not_offered() {
        let (w, h) = (200, 160);
        let mut canvas = sheet(w, h);
        plus(&mut canvas, w, 10, 10, 11);
        // A solid block has about the same bounding box and far more ink.
        block(&mut canvas, w, 100, 100, 11);
        let found = find(&Ink::new(w, h, canvas), &a_symbol(11), ALIKE, 100);
        assert_eq!(found.len(), 1, "only the plus: {found:?}");
        assert!(found[0].x.abs_diff(10) <= 2);
    }

    #[test]
    fn one_symbol_is_one_answer_and_not_a_cluster_of_them() {
        // Without suppressing the near misses, a symbol answers at every
        // position it nearly lines up with, and a count of four reads sixty.
        let (w, h) = (120, 120);
        let mut canvas = sheet(w, h);
        plus(&mut canvas, w, 50, 50, 15);
        let found = find(&Ink::new(w, h, canvas), &a_symbol(15), 0.4, 500);
        assert_eq!(found.len(), 1, "one symbol, one answer: {}", found.len());
    }

    #[test]
    fn an_empty_selection_matches_nothing_rather_than_all_the_paper() {
        let (w, h) = (100, 100);
        let mut canvas = sheet(w, h);
        plus(&mut canvas, w, 10, 10, 11);
        let blank = Ink::new(9, 9, sheet(9, 9));
        assert!(
            find(&Ink::new(w, h, canvas), &blank, ALIKE, 100).is_empty(),
            "blank paper is not a symbol"
        );
    }

    #[test]
    fn a_symbol_bigger_than_the_sheet_finds_nothing_rather_than_falling_over() {
        let small = Ink::new(10, 10, sheet(10, 10));
        assert!(find(&small, &a_symbol(21), ALIKE, 100).is_empty());
    }

    #[test]
    fn asking_for_fewer_answers_gives_the_best_of_them() {
        let (w, h) = (300, 300);
        let mut canvas = sheet(w, h);
        for i in 0..9 {
            plus(&mut canvas, w, 20 + (i % 3) * 90, 20 + (i / 3) * 90, 11);
        }
        let all = find(&Ink::new(w, h, canvas.clone()), &a_symbol(11), ALIKE, 100);
        assert_eq!(all.len(), 9);
        let some = find(&Ink::new(w, h, canvas), &a_symbol(11), ALIKE, 4);
        assert_eq!(some.len(), 4);
    }

    #[test]
    fn ink_under_a_window_is_counted_correctly() {
        let (w, h) = (20, 20);
        let mut canvas = sheet(w, h);
        block(&mut canvas, w, 5, 5, 4);
        let ink = Ink::new(w, h, canvas);
        assert_eq!(ink.total_ink(), 16);
        assert_eq!(ink.ink_in(5, 5, 4, 4), 16);
        assert_eq!(ink.ink_in(0, 0, 5, 5), 0);
        assert_eq!(ink.ink_in(5, 5, 2, 2), 4);
        // Beyond the edge counts what is there, not what would be.
        assert_eq!(ink.ink_in(18, 18, 10, 10), 0);
    }

    #[test]
    fn a_symbol_with_something_next_to_it_is_still_recognised() {
        // Real drawings have a dimension line or a leader touching the symbol.
        // Being too strict here is how a search misses half of them.
        let (w, h) = (200, 120);
        let mut canvas = sheet(w, h);
        plus(&mut canvas, w, 40, 40, 13);
        // A leader running off to the right of it.
        for x in 54..90 {
            canvas[46 * w + x] = true;
        }
        let found = find(&Ink::new(w, h, canvas), &a_symbol(13), ALIKE, 100);
        assert!(!found.is_empty(), "a symbol with a leader on it is still one");
        assert!(found.iter().any(|s| s.x.abs_diff(40) <= 3 && s.y.abs_diff(40) <= 3));
    }
}
