//! Dynamic Fill, as the user meets it.
//!
//! Click inside a room and it works out the shape from the lines already on the
//! drawing. Where a doorway is open, draw across it first and the fill stops
//! there.
//!
//! Nothing becomes a measurement until somebody looks at the shape and says so.
//! The fill is a proposal, drawn on the sheet, and Apply is the confirmation —
//! which is the same rule as everywhere else here: a quantity traces back to
//! something a person did, and a shape found by a computer reading pixels is
//! not that until they have seen it.

use crate::render::FillOutcome;

/// A shape ready to become a measurement.
pub struct Applying<'a> {
    pub outline: &'a Vec<[f64; 2]>,
    /// Everything the outline encloses but the fill did not reach. Each one
    /// has to be cut out, or the measurement is bigger than what was filled.
    pub holes: &'a Vec<Vec<[f64; 2]>>,
    /// Square sheet points actually filled.
    pub area: f64,
    /// Square sheet points the outline covers that the fill did not.
    pub enclosed: f64,
}

/// The state of a fill in progress.
#[derive(Default)]
pub struct Filling {
    /// Boundary lines drawn to close openings, in sheet coordinates.
    pub strokes: Vec<Vec<[f64; 2]>>,
    /// The one being drawn right now.
    pub drawing: Option<Vec<[f64; 2]>>,
    /// Rises with each fill, so an answer to a click somebody has moved on
    /// from is recognised and dropped.
    pub job: u64,
    pub waiting: bool,
    /// What came back.
    pub outcome: Option<FillOutcome>,
    /// How dark a pixel has to be to count as a line.
    pub darkness: f32,
    /// The sheet it was found on, so it cannot be applied to another one.
    pub page: u32,
}

impl Filling {
    pub fn new() -> Filling {
        Filling {
            // Well below mid-grey: a drawing's lines are black, and a light
            // hatch or a screened background should not stop a fill.
            darkness: 0.55,
            ..Default::default()
        }
    }

    pub fn begin(&mut self, page: u32) -> u64 {
        self.job += 1;
        self.waiting = true;
        self.outcome = None;
        self.page = page;
        self.job
    }

    /// Takes an answer, ignoring one to a click already moved on from.
    pub fn accept(&mut self, job: u64, outcome: FillOutcome) {
        if job != self.job {
            return;
        }
        self.waiting = false;
        self.outcome = Some(outcome);
    }

    /// The shape found, if there is one to apply: the outline, what it
    /// measures, and the openings that have to come out of it.
    pub fn ready(&self) -> Option<Applying<'_>> {
        match &self.outcome {
            Some(FillOutcome::Found {
                outline,
                holes,
                area,
                enclosed,
                ..
            }) if outline.len() >= 3 => Some(Applying {
                outline,
                holes,
                area: *area,
                enclosed: *enclosed,
            }),
            _ => None,
        }
    }

    /// What to say about it, and whether that is a complaint.
    pub fn says(&self) -> Option<(String, bool)> {
        match &self.outcome {
            None if self.waiting => Some(("Looking…".into(), false)),
            None => None,
            Some(FillOutcome::Found {
                area,
                enclosed,
                not_drawn,
                resolution,
                holes,
                outline: _,
            }) => {
                let mut said = String::new();
                if !holes.is_empty() {
                    said.push_str(&format!(
                        "{} opening{} cut out",
                        holes.len(),
                        if holes.len() == 1 { "" } else { "s" }
                    ));
                    if *not_drawn > 0 {
                        said.push_str(&format!(" (and {not_drawn} too small to draw)"));
                    }
                    said.push_str(&format!(
                        ", {:.0} of {:.0} square points. ",
                        enclosed,
                        area + enclosed
                    ));
                }
                // The limit is worth saying once, plainly: this shape was read
                // off pixels, and nothing finer than a cell was visible.
                said.push_str(&format!(
                    "Traced to about {resolution:.1} points on the sheet. Check it against the \
                     drawing before applying."
                ));
                Some((said, false))
            }
            Some(FillOutcome::Escaped(why)) => Some(((*why).to_string(), true)),
            Some(FillOutcome::OnALine) => Some((
                "That point is on a line. Click inside the area you want.".into(),
                true,
            )),
            Some(FillOutcome::CouldNotLook) => Some((
                "That part of the sheet could not be read. Zoom in a little and try again."
                    .into(),
                true,
            )),
        }
    }

    pub fn clear(&mut self) {
        self.job += 1;
        self.waiting = false;
        self.outcome = None;
        self.drawing = None;
    }

    /// Forgets the boundary lines as well. Used when the tool is put down.
    pub fn reset(&mut self) {
        self.clear();
        self.strokes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_shape() -> FillOutcome {
        FillOutcome::Found {
            outline: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            holes: Vec::new(),
            area: 100.0,
            enclosed: 0.0,
            not_drawn: 0,
            resolution: 0.33,
        }
    }

    #[test]
    fn an_answer_to_a_click_already_moved_on_from_is_dropped() {
        let mut filling = Filling::new();
        let stale = filling.begin(0);
        filling.begin(0);
        filling.accept(stale, a_shape());
        assert!(filling.ready().is_none(), "the old answer must not appear");
        assert!(filling.waiting, "and it is still waiting for the new one");
    }

    #[test]
    fn a_fill_that_escaped_offers_nothing_to_apply_and_says_why() {
        let mut filling = Filling::new();
        let job = filling.begin(3);
        filling.accept(
            job,
            FillOutcome::Escaped("That area is not closed — no area has been measured."),
        );
        assert!(filling.ready().is_none(), "there is nothing to apply");
        let (said, complaint) = filling.says().unwrap();
        assert!(complaint, "it should read as a problem, not as a result");
        assert!(said.contains("not closed"));
    }

    #[test]
    fn a_shape_that_was_found_can_be_applied_and_says_how_finely_it_was_traced() {
        let mut filling = Filling::new();
        let job = filling.begin(3);
        filling.accept(job, a_shape());
        let ready = filling.ready().expect("it should be ready");
        assert_eq!(ready.outline.len(), 4);
        assert_eq!(ready.area, 100.0);
        assert_eq!(ready.enclosed, 0.0);
        let (said, complaint) = filling.says().unwrap();
        assert!(!complaint);
        assert!(said.contains("0.3 points"), "{said}");
        assert!(said.contains("Check it"), "the limit is said out loud: {said}");
    }

    #[test]
    fn openings_taken_out_are_mentioned() {
        let mut filling = Filling::new();
        let job = filling.begin(0);
        filling.accept(
            job,
            FillOutcome::Found {
                outline: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                holes: vec![vec![[2.0, 2.0], [3.0, 2.0], [3.0, 3.0]]],
                area: 90.0,
                enclosed: 10.0,
                not_drawn: 0,
                resolution: 0.33,
            },
        );
        let (said, _) = filling.says().unwrap();
        assert!(said.contains("1 opening cut out"), "{said}");
        // How much it accounts for is said, not just that there is one.
        assert!(said.contains("10 of 100"), "{said}");
    }

    #[test]
    fn clicking_on_a_line_is_a_complaint_rather_than_an_area_of_nothing() {
        let mut filling = Filling::new();
        let job = filling.begin(0);
        filling.accept(job, FillOutcome::OnALine);
        assert!(filling.ready().is_none());
        let (_, complaint) = filling.says().unwrap();
        assert!(complaint);
    }

    #[test]
    fn putting_the_tool_down_forgets_the_boundary_lines_as_well() {
        let mut filling = Filling::new();
        filling.strokes.push(vec![[0.0, 0.0], [1.0, 1.0]]);
        let job = filling.begin(0);
        filling.accept(job, a_shape());
        filling.clear();
        assert!(filling.ready().is_none());
        assert_eq!(filling.strokes.len(), 1, "clearing a fill keeps the lines");
        filling.reset();
        assert!(filling.strokes.is_empty(), "putting the tool down does not");
    }
}
