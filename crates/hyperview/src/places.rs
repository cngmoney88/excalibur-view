//! Where you have been, and the grid.
//!
//! Going back to where you were is one of the things people miss most when
//! they move off Revu: on a set of forty sheets, jumping to a detail and then
//! back to the plan you were reading is something somebody does two hundred
//! times a day.

use crate::app::App;

/// A place on the drawing: which sheet, and what part of it was on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub page: u32,
    pub offset: egui::Vec2,
    pub zoom: f32,
}

impl Place {
    /// Whether two places are near enough that going between them would not
    /// look like going anywhere.
    pub fn about_the_same(&self, other: &Place) -> bool {
        self.page == other.page
            && (self.zoom - other.zoom).abs() < 0.01
            && (self.offset - other.offset).length() < 8.0
    }
}

/// How many places back it is possible to go. Deep enough for a morning's
/// jumping about, shallow enough not to be a second undo history.
pub const REMEMBERED_PLACES: usize = 40;

impl App {
    fn here(&self) -> Option<Place> {
        let doc = self.doc()?;
        Some(Place {
            page: doc.page,
            offset: doc.view.offset,
            zoom: doc.view.zoom,
        })
    }

    /// Remembers where the view is, before it is about to move somewhere else.
    ///
    /// Called by the things that jump — a search hit, a sheet clicked in the
    /// list, a markup double-clicked — rather than by panning, because
    /// remembering every pan would make going back mean nothing.
    pub fn remember_place(&mut self) {
        let Some(here) = self.here() else { return };
        if self.been.last().map(|p| p.about_the_same(&here)).unwrap_or(false) {
            return;
        }
        self.been.push(here);
        if self.been.len() > REMEMBERED_PLACES {
            self.been.remove(0);
        }
        // Going somewhere new means there is no forward to go to.
        self.going_back.clear();
    }

    pub fn go_back_a_view(&mut self) {
        let Some(place) = self.been.pop() else {
            self.status = "You have not gone anywhere yet.".into();
            return;
        };
        if let Some(here) = self.here() {
            self.going_back.push(here);
        }
        self.go_to_place(place);
        self.status = format!("Back to sheet {}.", place.page + 1);
    }

    pub fn go_forward_a_view(&mut self) {
        let Some(place) = self.going_back.pop() else {
            self.status = "There is nowhere forward to go.".into();
            return;
        };
        if let Some(here) = self.here() {
            self.been.push(here);
        }
        self.go_to_place(place);
        self.status = format!("Forward to sheet {}.", place.page + 1);
    }

    fn go_to_place(&mut self, place: Place) {
        let Some(doc) = self.doc_mut() else { return };
        doc.page = place.page.min(doc.pages.len().saturating_sub(1) as u32);
        doc.view.offset = place.offset;
        doc.view.zoom = place.zoom;
        doc.view.fit_requested = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(page: u32, x: f32, zoom: f32) -> Place {
        Place {
            page,
            offset: egui::vec2(x, 0.0),
            zoom,
        }
    }

    #[test]
    fn two_places_a_few_pixels_apart_are_the_same_place() {
        assert!(place(2, 100.0, 1.0).about_the_same(&place(2, 103.0, 1.0)));
        assert!(!place(2, 100.0, 1.0).about_the_same(&place(3, 100.0, 1.0)));
        assert!(!place(2, 100.0, 1.0).about_the_same(&place(2, 400.0, 1.0)));
        assert!(!place(2, 100.0, 1.0).about_the_same(&place(2, 100.0, 4.0)));
    }
}

// ---- the grid --------------------------------------------------------------

/// Where the grid lines fall across part of a sheet, in the sheet's own points.
///
/// The spacing comes from the sheet's scale, so a grid on a quarter-inch plan
/// is a foot apart on the building, not a foot apart on the paper. A sheet
/// with no scale gets a grid an inch apart on the paper and says so, rather
/// than a grid at a spacing nobody can name.
pub fn grid_lines(
    area: [f64; 4],
    spacing_points: f64,
    most: usize,
) -> (Vec<f64>, Vec<f64>) {
    if spacing_points <= 0.5 {
        return (Vec::new(), Vec::new());
    }
    let across = ((area[2] - area[0]) / spacing_points).ceil() as usize;
    let down = ((area[3] - area[1]) / spacing_points).ceil() as usize;
    // A grid too fine to see is a grey wash that hides the drawing under it.
    if across + down > most {
        return (Vec::new(), Vec::new());
    }
    let first = |from: f64| (from / spacing_points).ceil() * spacing_points;
    let mut xs = Vec::new();
    let mut x = first(area[0]);
    while x <= area[2] {
        xs.push(x);
        x += spacing_points;
    }
    let mut ys = Vec::new();
    let mut y = first(area[1]);
    while y <= area[3] {
        ys.push(y);
        y += spacing_points;
    }
    (xs, ys)
}

/// The nearest grid line to a point, when it is near enough to pull onto.
pub fn on_grid(at: [f64; 2], spacing_points: f64, reach: f64) -> Option<[f64; 2]> {
    if spacing_points <= 0.5 {
        return None;
    }
    let near = |v: f64| (v / spacing_points).round() * spacing_points;
    let pulled = [near(at[0]), near(at[1])];
    let away = ((pulled[0] - at[0]).powi(2) + (pulled[1] - at[1]).powi(2)).sqrt();
    (away <= reach).then_some(pulled)
}

#[cfg(test)]
mod grid_tests {
    use super::*;

    #[test]
    fn the_lines_fall_on_round_multiples_of_the_spacing() {
        let (xs, ys) = grid_lines([5.0, 5.0, 45.0, 25.0], 10.0, 1000);
        assert_eq!(xs, vec![10.0, 20.0, 30.0, 40.0]);
        assert_eq!(ys, vec![10.0, 20.0]);
    }

    #[test]
    fn a_grid_too_fine_to_see_is_not_drawn_at_all() {
        // Otherwise zooming out turns a plan into a grey wash.
        let (xs, ys) = grid_lines([0.0, 0.0, 3024.0, 2160.0], 0.75, 4000);
        assert!(xs.is_empty() && ys.is_empty());
    }

    #[test]
    fn a_point_near_a_line_is_pulled_onto_it_and_one_far_off_is_left_alone() {
        assert_eq!(on_grid([19.0, 41.0], 10.0, 3.0), Some([20.0, 40.0]));
        assert_eq!(on_grid([15.0, 15.0], 10.0, 3.0), None);
    }
}
