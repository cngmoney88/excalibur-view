//! Lining markups up, spacing them out, flipping them, and deciding what sits
//! in front of what.
//!
//! All of it works on whatever is selected, and all of it is one undo step, so
//! a row of twelve bolt tags lined up by mistake goes back with one Ctrl+Z
//! rather than twelve.
//!
//! Nothing here ever changes a measurement's *value* by accident: moving a
//! markup moves its points, and the measurement is re-read from the points it
//! now has. A length dragged to line up with another is a different length,
//! and it says so — silently keeping the old number would be the worse lie.

use crate::app::App;

/// Which edge things line up on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    Left,
    Middle,
    Right,
    Top,
    Centre,
    Bottom,
}

impl Edge {
    pub fn from_command(id: &str) -> Option<Edge> {
        Some(match id {
            "Align.Left" => Edge::Left,
            "Align.Center" => Edge::Middle,
            "Align.Right" => Edge::Right,
            "Align.Top" => Edge::Top,
            "Align.Middle" => Edge::Centre,
            "Align.Bottom" => Edge::Bottom,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Edge::Left => "left",
            Edge::Middle => "middle",
            Edge::Right => "right",
            Edge::Top => "top",
            Edge::Centre => "centre",
            Edge::Bottom => "bottom",
        }
    }

}

/// Which dimension is made the same.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Same {
    Width,
    Height,
    Both,
}

impl Same {
    pub fn from_command(id: &str) -> Option<Same> {
        Some(match id {
            "Align.Width" => Same::Width,
            "Align.Height" => Same::Height,
            "Align.Size" => Same::Both,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Same::Width => "the same width",
            Same::Height => "the same height",
            Same::Both => "the same size",
        }
    }
}

/// The box a run of points sits in.
pub fn bounds(points: &[[f64; 2]]) -> Option<[f64; 4]> {
    let first = points.first()?;
    let mut out = [first[0], first[1], first[0], first[1]];
    for p in points {
        out[0] = out[0].min(p[0]);
        out[1] = out[1].min(p[1]);
        out[2] = out[2].max(p[0]);
        out[3] = out[3].max(p[1]);
    }
    Some(out)
}

/// How far each box has to move to line its edge up with `to`.
///
/// In PDF space, where y goes up: "top" is the larger y. Getting this the
/// wrong way round lines a row of tags up on the bottom when somebody asked
/// for the top, which is the kind of thing nobody reports and everybody works
/// around.
pub fn shift_for(edge: Edge, area: [f64; 4], to: f64) -> [f64; 2] {
    match edge {
        Edge::Left => [to - area[0], 0.0],
        Edge::Right => [to - area[2], 0.0],
        Edge::Middle => [to - (area[0] + area[2]) * 0.5, 0.0],
        Edge::Bottom => [0.0, to - area[1]],
        Edge::Top => [0.0, to - area[3]],
        Edge::Centre => [0.0, to - (area[1] + area[3]) * 0.5],
    }
}

/// Where the edge everything lines up on is.
///
/// The outermost one, which is what somebody means: aligning left puts
/// everything on the leftmost, not on the average of them.
pub fn edge_of(edge: Edge, boxes: &[[f64; 4]]) -> Option<f64> {
    if boxes.is_empty() {
        return None;
    }
    Some(match edge {
        Edge::Left => boxes.iter().map(|b| b[0]).fold(f64::MAX, f64::min),
        Edge::Right => boxes.iter().map(|b| b[2]).fold(f64::MIN, f64::max),
        Edge::Middle => {
            let lo = boxes.iter().map(|b| b[0]).fold(f64::MAX, f64::min);
            let hi = boxes.iter().map(|b| b[2]).fold(f64::MIN, f64::max);
            (lo + hi) * 0.5
        }
        Edge::Bottom => boxes.iter().map(|b| b[1]).fold(f64::MAX, f64::min),
        Edge::Top => boxes.iter().map(|b| b[3]).fold(f64::MIN, f64::max),
        Edge::Centre => {
            let lo = boxes.iter().map(|b| b[1]).fold(f64::MAX, f64::min);
            let hi = boxes.iter().map(|b| b[3]).fold(f64::MIN, f64::max);
            (lo + hi) * 0.5
        }
    })
}

/// Where each box has to end up for the gaps between them to be equal.
///
/// The two on the ends stay where they are — they are what defines the run —
/// and everything between them is spread out evenly. Comes back in the same
/// order it went in.
pub fn spread(boxes: &[[f64; 4]], across: bool) -> Vec<[f64; 2]> {
    let n = boxes.len();
    if n < 3 {
        return vec![[0.0, 0.0]; n];
    }
    let low = |b: &[f64; 4]| if across { b[0] } else { b[1] };
    let high = |b: &[f64; 4]| if across { b[2] } else { b[3] };

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| {
        low(&boxes[*a])
            .partial_cmp(&low(&boxes[*b]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let first = &boxes[order[0]];
    let last = &boxes[order[n - 1]];
    let span = high(last) - low(first);
    let filled: f64 = order.iter().map(|i| high(&boxes[*i]) - low(&boxes[*i])).sum();
    let gap = (span - filled) / (n as f64 - 1.0);

    let mut shifts = vec![[0.0, 0.0]; n];
    let mut at = low(first);
    for (place, index) in order.iter().enumerate() {
        let wide = high(&boxes[*index]) - low(&boxes[*index]);
        if place > 0 && place < n - 1 {
            let move_by = at - low(&boxes[*index]);
            shifts[*index] = if across {
                [move_by, 0.0]
            } else {
                [0.0, move_by]
            };
        }
        at += wide + gap;
    }
    shifts
}

/// Turning a point about a line, for a flip.
pub fn mirrored(point: [f64; 2], about: f64, across: bool) -> [f64; 2] {
    if across {
        [2.0 * about - point[0], point[1]]
    } else {
        [point[0], 2.0 * about - point[1]]
    }
}

/// How much a box has to be scaled to match another's size, and where that
/// leaves its corner.
pub fn resize(area: [f64; 4], to: [f64; 4], same: Same) -> ([f64; 2], [f64; 2]) {
    let wide = (area[2] - area[0]).max(1e-9);
    let tall = (area[3] - area[1]).max(1e-9);
    let x = match same {
        Same::Width | Same::Both => (to[2] - to[0]) / wide,
        Same::Height => 1.0,
    };
    let y = match same {
        Same::Height | Same::Both => (to[3] - to[1]) / tall,
        Same::Width => 1.0,
    };
    // Grown about its own top left corner, which is where somebody's eye is.
    ([x, y], [area[0], area[3]])
}

impl App {
    /// The selection, with each one's box in PDF space.
    fn picked_with_boxes(&self) -> Vec<(usize, [f64; 4])> {
        let Some(doc) = self.doc() else {
            return Vec::new();
        };
        doc.selection()
            .into_iter()
            .filter_map(|index| {
                let mark = doc.marks.get(index)?;
                let area = bounds(&mark.markup.points())?;
                Some((index, area))
            })
            .collect()
    }

    /// Moves every markup in the selection by its own amount, as one step.
    fn move_each(&mut self, what: &str, shifts: Vec<(usize, [f64; 2])>) {
        if shifts.is_empty() {
            self.status = "Select the markups first.".into();
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        doc.checkpoint_named(what);
        let mut moved = 0usize;
        for (index, by) in shifts {
            if by[0].abs() < 1e-9 && by[1].abs() < 1e-9 {
                continue;
            }
            if let Some(mark) = doc.marks.get_mut(index) {
                mark.markup.move_by(by[0], by[1]);
                mark.changed = true;
                moved += 1;
            }
        }
        doc.dirty = true;
        // A measurement measures where it now is, not where it was.
        let pages: Vec<u32> = doc.selection().iter().filter_map(|i| doc.marks.get(*i)).map(|m| m.page).collect();
        let _ = pages;
        doc.remeasure_selection();
        self.status = format!(
            "{moved} markup{} moved.",
            if moved == 1 { "" } else { "s" }
        );
    }

    pub fn align_selection(&mut self, edge: Edge) {
        let picked = self.picked_with_boxes();
        if picked.len() < 2 {
            self.status = "Select two or more markups to line up.".into();
            return;
        }
        let boxes: Vec<[f64; 4]> = picked.iter().map(|(_, b)| *b).collect();
        let Some(to) = edge_of(edge, &boxes) else { return };
        let shifts: Vec<(usize, [f64; 2])> = picked
            .iter()
            .map(|(index, area)| (*index, shift_for(edge, *area, to)))
            .collect();
        let count = shifts.len();
        self.move_each(&format!("Align {}", edge.name()), shifts);
        self.status = format!(
            "{count} markups lined up on the {}. Any measurement among them now \
             reads what it measures where it is.",
            edge.name()
        );
    }

    pub fn spread_selection(&mut self, across: bool) {
        let picked = self.picked_with_boxes();
        if picked.len() < 3 {
            self.status = "Select three or more markups to space out.".into();
            return;
        }
        let boxes: Vec<[f64; 4]> = picked.iter().map(|(_, b)| *b).collect();
        let shifts = spread(&boxes, across);
        let moves: Vec<(usize, [f64; 2])> = picked
            .iter()
            .zip(shifts)
            .map(|((index, _), by)| (*index, by))
            .collect();
        self.move_each(
            if across {
                "Space out across"
            } else {
                "Space out down"
            },
            moves,
        );
        self.status = format!(
            "The gaps between them are even now. The two on the {} did not move.",
            if across { "ends" } else { "top and bottom" }
        );
    }

    /// Puts the selection in the middle of the sheet.
    pub fn centre_on_sheet(&mut self) {
        let picked = self.picked_with_boxes();
        if picked.is_empty() {
            self.status = "Select something first.".into();
            return;
        }
        let Some(doc) = self.doc() else { return };
        let frame = doc.frame();
        let middle = [
            (frame.area[0] + frame.area[2]) * 0.5,
            (frame.area[1] + frame.area[3]) * 0.5,
        ];
        let boxes: Vec<[f64; 4]> = picked.iter().map(|(_, b)| *b).collect();
        let whole = [
            boxes.iter().map(|b| b[0]).fold(f64::MAX, f64::min),
            boxes.iter().map(|b| b[1]).fold(f64::MAX, f64::min),
            boxes.iter().map(|b| b[2]).fold(f64::MIN, f64::max),
            boxes.iter().map(|b| b[3]).fold(f64::MIN, f64::max),
        ];
        let by = [
            middle[0] - (whole[0] + whole[2]) * 0.5,
            middle[1] - (whole[1] + whole[3]) * 0.5,
        ];
        let shifts: Vec<(usize, [f64; 2])> =
            picked.iter().map(|(index, _)| (*index, by)).collect();
        self.move_each("Centre on the sheet", shifts);
        self.status = "Put in the middle of the sheet.".into();
    }

    /// Turns the selection over, about the middle of what is selected.
    pub fn flip_selection(&mut self, across: bool) {
        let picked = self.picked_with_boxes();
        if picked.is_empty() {
            self.status = "Select something first.".into();
            return;
        }
        let boxes: Vec<[f64; 4]> = picked.iter().map(|(_, b)| *b).collect();
        let whole = [
            boxes.iter().map(|b| b[0]).fold(f64::MAX, f64::min),
            boxes.iter().map(|b| b[1]).fold(f64::MAX, f64::min),
            boxes.iter().map(|b| b[2]).fold(f64::MIN, f64::max),
            boxes.iter().map(|b| b[3]).fold(f64::MIN, f64::max),
        ];
        let about = if across {
            (whole[0] + whole[2]) * 0.5
        } else {
            (whole[1] + whole[3]) * 0.5
        };

        let indices: Vec<usize> = picked.iter().map(|(i, _)| *i).collect();
        let Some(doc) = self.doc_mut() else { return };
        doc.checkpoint_named(if across { "Flip across" } else { "Flip down" });
        let mut flipped = 0usize;
        for index in &indices {
            let Some(mark) = doc.marks.get_mut(*index) else { continue };
            if mirror_markup(&mut mark.markup, about, across) {
                mark.changed = true;
                flipped += 1;
            }
        }
        doc.dirty = true;
        doc.remeasure_selection();
        self.status = format!(
            "{flipped} markup{} turned over.",
            if flipped == 1 { "" } else { "s" }
        );
    }

    /// Makes everything the same size as the one whose properties are shown.
    pub fn same_size(&mut self, same: Same) {
        let picked = self.picked_with_boxes();
        if picked.len() < 2 {
            self.status = "Select two or more markups, and the first one is the one \
                           the rest are matched to."
                .into();
            return;
        }
        let to = picked[0].1;
        let rest: Vec<(usize, [f64; 4])> = picked[1..].to_vec();
        let Some(doc) = self.doc_mut() else { return };
        doc.checkpoint_named(&format!("Make {}", same.name()));
        let mut changed = 0usize;
        for (index, area) in rest {
            let (scale, about) = resize(area, to, same);
            let Some(mark) = doc.marks.get_mut(index) else { continue };
            if scale_markup(&mut mark.markup, scale, about) {
                mark.changed = true;
                changed += 1;
            }
        }
        doc.dirty = true;
        doc.remeasure_selection();
        self.status = format!(
            "{changed} markup{} made {}. Any measurement among them reads what it \
             measures now.",
            if changed == 1 { "" } else { "s" },
            same.name()
        );
    }

    /// Moves the selection in the list, which is what decides what is in front.
    pub fn reorder_selection(&mut self, to_front: bool, all_the_way: bool) {
        let picked = {
            let Some(doc) = self.doc() else { return };
            doc.selection()
        };
        if picked.is_empty() {
            self.status = "Select something first.".into();
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        doc.checkpoint_named(if to_front {
            "Bring forward"
        } else {
            "Send backward"
        });

        // Taken out and put back, so several moved at once keep their order
        // among themselves.
        let mut taken: Vec<crate::sheet::Mark> = Vec::new();
        let mut kept: Vec<crate::sheet::Mark> = Vec::new();
        let chosen: std::collections::HashSet<usize> = picked.iter().copied().collect();
        for (at, mark) in doc.marks.drain(..).enumerate() {
            if chosen.contains(&at) {
                taken.push(mark);
            } else {
                kept.push(mark);
            }
        }
        let count = taken.len();
        let put_at = if all_the_way {
            if to_front {
                kept.len()
            } else {
                0
            }
        } else {
            // One step: past the next one that is on the same sheet.
            let first = picked.iter().copied().min().unwrap_or(0);
            let step = if to_front { first + 1 } else { first.saturating_sub(1) };
            step.min(kept.len())
        };
        let mut out = kept;
        for (offset, mark) in taken.into_iter().enumerate() {
            out.insert((put_at + offset).min(out.len()), mark);
        }
        doc.marks = out;
        // The indices have all moved, so what was held is found again by
        // where it went.
        doc.selected = Some(put_at.min(doc.marks.len().saturating_sub(1)));
        doc.also = (put_at + 1..put_at + count).collect();
        for index in doc.selection() {
            if let Some(mark) = doc.marks.get_mut(index) {
                mark.changed = true;
            }
        }
        doc.dirty = true;
        self.status = format!(
            "{count} markup{} moved {}.",
            if count == 1 { "" } else { "s" },
            match (to_front, all_the_way) {
                (true, true) => "to the front",
                (true, false) => "forward one",
                (false, true) => "to the back",
                (false, false) => "back one",
            }
        );
    }
}

/// Turns one markup over about a line.
fn mirror_markup(markup: &mut annot::Markup, about: f64, across: bool) -> bool {
    use annot::Subtype;
    let points = markup.points();
    if points.is_empty() {
        return false;
    }
    let flipped: Vec<[f64; 2]> = points.iter().map(|p| mirrored(*p, about, across)).collect();
    match markup.subtype() {
        Subtype::Line => {
            if flipped.len() < 2 {
                return false;
            }
            markup.set_line(flipped[0], flipped[flipped.len() - 1]);
        }
        Subtype::Ink => {
            let strokes: Vec<Vec<[f64; 2]>> = markup
                .ink()
                .iter()
                .map(|s| s.iter().map(|p| mirrored(*p, about, across)).collect())
                .collect();
            markup.set_ink(&strokes);
        }
        Subtype::Square
        | Subtype::Circle
        | Subtype::FreeText
        | Subtype::Stamp
        | Subtype::Text
        | Subtype::Link
        | Subtype::Highlight
        | Subtype::Underline
        | Subtype::StrikeOut
        | Subtype::Squiggly => {
            let Some(area) = bounds(&flipped) else { return false };
            markup.set_box(area);
        }
        _ => {
            markup.set_vertices(&flipped);
        }
    }
    markup.dict.remove("AP");
    true
}

/// Grows or shrinks one markup about a corner.
fn scale_markup(markup: &mut annot::Markup, scale: [f64; 2], about: [f64; 2]) -> bool {
    use annot::Subtype;
    let put = |p: [f64; 2]| {
        [
            about[0] + (p[0] - about[0]) * scale[0],
            about[1] + (p[1] - about[1]) * scale[1],
        ]
    };
    let points = markup.points();
    if points.is_empty() {
        return false;
    }
    match markup.subtype() {
        Subtype::Line => {
            let moved: Vec<[f64; 2]> = points.iter().map(|p| put(*p)).collect();
            if moved.len() < 2 {
                return false;
            }
            markup.set_line(moved[0], moved[moved.len() - 1]);
        }
        Subtype::Ink => {
            let strokes: Vec<Vec<[f64; 2]>> = markup
                .ink()
                .iter()
                .map(|s| s.iter().map(|p| put(*p)).collect())
                .collect();
            markup.set_ink(&strokes);
        }
        Subtype::Square
        | Subtype::Circle
        | Subtype::FreeText
        | Subtype::Stamp
        | Subtype::Text
        | Subtype::Link
        | Subtype::Highlight
        | Subtype::Underline
        | Subtype::StrikeOut
        | Subtype::Squiggly => {
            let moved: Vec<[f64; 2]> = points.iter().map(|p| put(*p)).collect();
            let Some(area) = bounds(&moved) else { return false };
            markup.set_box(area);
        }
        _ => {
            let moved: Vec<[f64; 2]> = points.iter().map(|p| put(*p)).collect();
            markup.set_vertices(&moved);
        }
    }
    markup.dict.remove("AP");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lining_up_on_the_left_goes_to_the_leftmost_one() {
        // Not the average of them, which is what somebody would get if the
        // edge were worked out the obvious wrong way.
        let boxes = [[100.0, 0.0, 200.0, 50.0], [40.0, 0.0, 90.0, 50.0], [300.0, 0.0, 380.0, 50.0]];
        assert_eq!(edge_of(Edge::Left, &boxes), Some(40.0));
        assert_eq!(edge_of(Edge::Right, &boxes), Some(380.0));
    }

    #[test]
    fn the_top_in_a_pdf_is_the_larger_y() {
        // PDF y goes up. Getting this the wrong way round lines a row of tags
        // up on the bottom when somebody asked for the top.
        let boxes = [[0.0, 100.0, 10.0, 200.0], [0.0, 400.0, 10.0, 500.0]];
        assert_eq!(edge_of(Edge::Top, &boxes), Some(500.0));
        assert_eq!(edge_of(Edge::Bottom, &boxes), Some(100.0));
    }

    #[test]
    fn a_shift_puts_the_edge_exactly_where_it_was_asked_for() {
        let area = [100.0, 200.0, 180.0, 260.0];
        assert_eq!(shift_for(Edge::Left, area, 40.0), [-60.0, 0.0]);
        assert_eq!(shift_for(Edge::Right, area, 400.0), [220.0, 0.0]);
        assert_eq!(shift_for(Edge::Top, area, 500.0), [0.0, 240.0]);
        assert_eq!(shift_for(Edge::Bottom, area, 0.0), [0.0, -200.0]);
        assert_eq!(shift_for(Edge::Middle, area, 140.0), [0.0, 0.0]);
    }

    #[test]
    fn spacing_out_leaves_the_two_on_the_ends_where_they_were() {
        // They are what defines the run; moving them would move the whole row.
        let boxes = [
            [0.0, 0.0, 10.0, 10.0],
            [15.0, 0.0, 25.0, 10.0],
            [100.0, 0.0, 110.0, 10.0],
        ];
        let shifts = spread(&boxes, true);
        assert_eq!(shifts[0], [0.0, 0.0]);
        assert_eq!(shifts[2], [0.0, 0.0]);
        // The gaps are 40 either side: 110 across, 30 of it filled, two gaps.
        assert!((shifts[1][0] - 35.0).abs() < 0.001, "{shifts:?}");
        let middle_now = 15.0 + shifts[1][0];
        assert!((middle_now - 10.0 - 40.0).abs() < 0.001, "gap before it");
        assert!((100.0 - (middle_now + 10.0) - 40.0).abs() < 0.001, "gap after it");
    }

    #[test]
    fn spacing_fewer_than_three_moves_nothing() {
        let boxes = [[0.0, 0.0, 10.0, 10.0], [50.0, 0.0, 60.0, 10.0]];
        assert_eq!(spread(&boxes, true), vec![[0.0, 0.0], [0.0, 0.0]]);
    }

    #[test]
    fn spacing_works_whatever_order_they_were_selected_in() {
        let boxes = [
            [100.0, 0.0, 110.0, 10.0],
            [0.0, 0.0, 10.0, 10.0],
            [15.0, 0.0, 25.0, 10.0],
        ];
        let shifts = spread(&boxes, true);
        // The ends of the run, whichever order they came in, stay put.
        assert_eq!(shifts[0], [0.0, 0.0]);
        assert_eq!(shifts[1], [0.0, 0.0]);
    }

    #[test]
    fn a_flip_is_its_own_opposite() {
        let point = [37.0, 91.0];
        let once = mirrored(point, 100.0, true);
        assert_eq!(mirrored(once, 100.0, true), point);
        let once = mirrored(point, 100.0, false);
        assert_eq!(mirrored(once, 100.0, false), point);
    }

    #[test]
    fn making_two_the_same_width_leaves_the_height_alone() {
        let area = [0.0, 0.0, 50.0, 30.0];
        let to = [0.0, 0.0, 100.0, 90.0];
        let (scale, _) = resize(area, to, Same::Width);
        assert_eq!(scale, [2.0, 1.0]);
        let (scale, _) = resize(area, to, Same::Height);
        assert_eq!(scale, [1.0, 3.0]);
        let (scale, _) = resize(area, to, Same::Both);
        assert_eq!(scale, [2.0, 3.0]);
    }

    #[test]
    fn a_markup_with_no_size_at_all_does_not_divide_by_nothing() {
        let (scale, _) = resize([10.0, 10.0, 10.0, 10.0], [0.0, 0.0, 50.0, 50.0], Same::Both);
        assert!(scale[0].is_finite() && scale[1].is_finite());
    }

    #[test]
    fn every_align_command_names_an_edge() {
        for id in [
            "Align.Left", "Align.Center", "Align.Right", "Align.Top", "Align.Middle",
            "Align.Bottom",
        ] {
            assert!(Edge::from_command(id).is_some(), "{id}");
        }
        assert!(Edge::from_command("Align.Width").is_none());
    }
}
