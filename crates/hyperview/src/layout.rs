//! How the sheets are laid out in the window.
//!
//! Revu offers four arrangements and people use all of them: one sheet at a
//! time for marking up, a continuous scroll for reading a specification,
//! facing pages for a drawing set bound like a book, and facing pages
//! scrolling for going through one.
//!
//! The arrangement is a way of putting *other* sheets on the screen beside the
//! one being worked on. The sheet being worked on is still `doc.page` and
//! still uses `doc.view`, so every tool, every measurement and every bit of
//! painting goes on exactly as it did — which is the same trick the split view
//! uses, and the reason neither one needed changes spread through the program.

/// How the sheets sit in the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Layout {
    /// One sheet, on its own.
    #[default]
    Single,
    /// Every sheet, one under the other.
    Continuous,
    /// Two sheets side by side, like an open book.
    Facing,
    /// Facing pages, scrolling.
    FacingContinuous,
}

impl Layout {
    pub const ALL: &'static [Layout] = &[
        Layout::Single,
        Layout::Continuous,
        Layout::Facing,
        Layout::FacingContinuous,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Layout::Single => "Single Page",
            Layout::Continuous => "Continuous",
            Layout::Facing => "Side by Side",
            Layout::FacingContinuous => "Side by Side Continuous",
        }
    }

    pub fn from_command(id: &str) -> Option<Layout> {
        Some(match id {
            "View.Single" => Layout::Single,
            "View.Continuous" | "View.ScrollingPages" => Layout::Continuous,
            "View.SideBySide" => Layout::Facing,
            "View.SideBySideContinuous" => Layout::FacingContinuous,
            _ => return None,
        })
    }

    pub fn scrolls(self) -> bool {
        matches!(self, Layout::Continuous | Layout::FacingContinuous)
    }

    pub fn faces(self) -> bool {
        matches!(self, Layout::Facing | Layout::FacingContinuous)
    }
}

/// The gap between sheets, in the sheet's own points. Wide enough that two
/// drawings never look like one drawing with a line down it.
pub const GAP: f64 = 18.0;

/// One sheet to draw, and where it sits relative to the sheet being worked on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub page: u32,
    /// In the sheet space of the sheet being worked on: right and down.
    pub at: [f64; 2],
}

/// Which sheets to draw and where, for one arrangement.
///
/// `sizes` are every sheet's size in order. The sheet being worked on always
/// comes back at the origin, so an arrangement can be changed without the view
/// jumping.
pub fn placed(layout: Layout, page: u32, sizes: &[(f64, f64)], reach: f64) -> Vec<Placed> {
    let count = sizes.len() as u32;
    if count == 0 {
        return Vec::new();
    }
    let page = page.min(count - 1);
    let size_of = |p: u32| sizes.get(p as usize).copied().unwrap_or((612.0, 792.0));

    match layout {
        Layout::Single => vec![Placed { page, at: [0.0, 0.0] }],

        Layout::Facing => {
            // A drawing set opens like a book: sheet one on its own on the
            // right, then two and three facing, and so on. So the left-hand
            // sheet of a spread is always an odd-numbered one.
            let mut out = Vec::new();
            let left = if page == 0 { 0 } else { page - ((page + 1) % 2) };
            if left == page {
                out.push(Placed { page, at: [0.0, 0.0] });
                if page + 1 < count && page > 0 {
                    let (w, _) = size_of(page);
                    out.push(Placed {
                        page: page + 1,
                        at: [w + GAP, 0.0],
                    });
                }
            } else {
                let (w, _) = size_of(left);
                out.push(Placed {
                    page,
                    at: [0.0, 0.0],
                });
                out.push(Placed {
                    page: left,
                    at: [-(w + GAP), 0.0],
                });
            }
            out
        }

        Layout::Continuous | Layout::FacingContinuous => {
            // Only what could be on the screen. A thousand-sheet specification
            // must not cost a thousand entries every frame.
            let mut out = vec![Placed { page, at: [0.0, 0.0] }];
            let step = if layout.faces() { 2 } else { 1 };
            if layout.faces() {
                let beside = if page % 2 == 1 { page + 1 } else { page.wrapping_sub(1) };
                if beside < count && page > 0 {
                    let (w, _) = size_of(page.min(beside));
                    let across = if beside > page { w + GAP } else { -(w + GAP) };
                    out.push(Placed {
                        page: beside,
                        at: [across, 0.0],
                    });
                }
            }

            // Downwards.
            let mut down = size_of(page).1 + GAP;
            let mut at = page + step;
            while at < count && down <= reach {
                out.push(Placed {
                    page: at,
                    at: [0.0, down],
                });
                if layout.faces() && at + 1 < count {
                    let (w, _) = size_of(at);
                    out.push(Placed {
                        page: at + 1,
                        at: [w + GAP, down],
                    });
                }
                down += size_of(at).1 + GAP;
                at += step;
            }

            // Upwards.
            let mut up = 0.0;
            let mut at = page as i64 - step as i64;
            while at >= 0 && up <= reach {
                let above = at as u32;
                up += size_of(above).1 + GAP;
                out.push(Placed {
                    page: above,
                    at: [0.0, -up],
                });
                if layout.faces() && above + 1 < count && above + 1 != page {
                    let (w, _) = size_of(above);
                    out.push(Placed {
                        page: above + 1,
                        at: [w + GAP, -up],
                    });
                }
                at -= step as i64;
            }
            out
        }
    }
}

/// Which sheet is mostly in the window, so it becomes the one being worked on.
///
/// Scrolling in a continuous view has to change which sheet the tools draw on,
/// or somebody scrolls to the next sheet and marks up the one they have just
/// scrolled away from.
pub fn mostly_showing(placed: &[Placed], sizes: &[(f64, f64)], window: [f64; 4]) -> Option<u32> {
    let mut best: Option<(u32, f64)> = None;
    for one in placed {
        let (w, h) = sizes.get(one.page as usize).copied().unwrap_or((612.0, 792.0));
        let sheet = [one.at[0], one.at[1], one.at[0] + w, one.at[1] + h];
        let across = (sheet[2].min(window[2]) - sheet[0].max(window[0])).max(0.0);
        let down = (sheet[3].min(window[3]) - sheet[1].max(window[1])).max(0.0);
        let showing = across * down;
        if showing <= 0.0 {
            continue;
        }
        if best.map(|(_, most)| showing > most).unwrap_or(true) {
            best = Some((one.page, showing));
        }
    }
    best.map(|(page, _)| page)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters(n: usize) -> Vec<(f64, f64)> {
        vec![(612.0, 792.0); n]
    }

    #[test]
    fn one_sheet_at_a_time_shows_one_sheet() {
        let out = placed(Layout::Single, 3, &letters(10), 5000.0);
        assert_eq!(out, vec![Placed { page: 3, at: [0.0, 0.0] }]);
    }

    #[test]
    fn the_sheet_being_worked_on_is_always_at_the_origin() {
        // Otherwise changing the arrangement would throw the view somewhere
        // else, and somebody would lose their place mid-takeoff.
        for layout in Layout::ALL {
            for page in [0u32, 1, 2, 7] {
                let out = placed(*layout, page, &letters(10), 3000.0);
                let first = out.iter().find(|p| p.page == page).expect("it is there");
                assert_eq!(first.at, [0.0, 0.0], "{layout:?} on sheet {page}");
            }
        }
    }

    #[test]
    fn a_continuous_view_stacks_the_sheets_with_a_gap_between_them() {
        let out = placed(Layout::Continuous, 0, &letters(4), 2000.0);
        let second = out.iter().find(|p| p.page == 1).expect("the next sheet");
        assert_eq!(second.at, [0.0, 792.0 + GAP]);
        let third = out.iter().find(|p| p.page == 2).expect("and the one after");
        assert_eq!(third.at, [0.0, (792.0 + GAP) * 2.0]);
    }

    #[test]
    fn a_continuous_view_only_lays_out_what_could_be_on_the_screen() {
        // A thousand-sheet specification must not cost a thousand entries
        // every time the window is drawn.
        let out = placed(Layout::Continuous, 500, &letters(1000), 1600.0);
        assert!(out.len() <= 8, "{} sheets laid out", out.len());
    }

    #[test]
    fn a_set_opens_like_a_book() {
        // Sheet one on its own, then two and three facing. Anything else and
        // the sheets that belong together end up on opposite spreads.
        let out = placed(Layout::Facing, 0, &letters(6), 0.0);
        assert_eq!(out.len(), 1, "the first sheet is on its own");

        let out = placed(Layout::Facing, 1, &letters(6), 0.0);
        let pages: Vec<u32> = out.iter().map(|p| p.page).collect();
        assert!(pages.contains(&1) && pages.contains(&2), "{pages:?}");

        let out = placed(Layout::Facing, 2, &letters(6), 0.0);
        let pages: Vec<u32> = out.iter().map(|p| p.page).collect();
        assert!(pages.contains(&1) && pages.contains(&2), "{pages:?}");
    }

    #[test]
    fn the_sheet_mostly_in_the_window_is_the_one_that_gets_worked_on() {
        let sizes = letters(4);
        let out = placed(Layout::Continuous, 0, &sizes, 3000.0);
        // Looking at the very top: sheet one.
        assert_eq!(mostly_showing(&out, &sizes, [0.0, 0.0, 612.0, 700.0]), Some(0));
        // Scrolled down past it: sheet two.
        assert_eq!(
            mostly_showing(&out, &sizes, [0.0, 900.0, 612.0, 1600.0]),
            Some(1)
        );
    }

    #[test]
    fn a_window_showing_no_sheet_at_all_names_none() {
        let sizes = letters(1);
        let out = placed(Layout::Single, 0, &sizes, 0.0);
        assert_eq!(
            mostly_showing(&out, &sizes, [5000.0, 5000.0, 5600.0, 5700.0]),
            None
        );
    }
}
