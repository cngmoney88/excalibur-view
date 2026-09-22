//! The Excalibur mark: a shield, cross-braced like a steel frame, with a sword
//! on it. Drawn rather than pasted, so it stays crisp at any size and can take
//! the palette's own colours.

use egui::{pos2, Color32, Painter, Pos2, Rect, Stroke};

use crate::chrome::Theme;

/// The shield outline, in a 100 × 110 box with its origin at the top left.
/// A heater shield: square shoulders, straight sides, and a point.
fn shield(box_: Rect) -> Vec<Pos2> {
    let at = |x: f64, y: f64| -> Pos2 {
        pos2(
            box_.left() + box_.width() * (x as f32 / 100.0),
            box_.top() + box_.height() * (y as f32 / 110.0),
        )
    };
    let mut points = vec![at(6.0, 4.0), at(94.0, 4.0), at(94.0, 52.0)];
    // The taper down to the point, as a curve rather than a straight cut.
    for step in 0..=18 {
        let t = step as f64 / 18.0;
        // Quadratic from the right shoulder through the flank to the tip.
        let (x0, y0) = (94.0, 52.0);
        let (cx, cy) = (92.0, 86.0);
        let (x1, y1) = (50.0, 107.0);
        let u = 1.0 - t;
        points.push(at(
            u * u * x0 + 2.0 * u * t * cx + t * t * x1,
            u * u * y0 + 2.0 * u * t * cy + t * t * y1,
        ));
    }
    for step in 0..=18 {
        let t = step as f64 / 18.0;
        let (x0, y0) = (50.0, 107.0);
        let (cx, cy) = (8.0, 86.0);
        let (x1, y1) = (6.0, 52.0);
        let u = 1.0 - t;
        points.push(at(
            u * u * x0 + 2.0 * u * t * cx + t * t * x1,
            u * u * y0 + 2.0 * u * t * cy + t * t * y1,
        ));
    }
    points
}

/// Clips a shape against a convex outline, Sutherland–Hodgman. The shield is
/// convex — square shoulders, straight flanks, a curve in to the point — so
/// this is exact, and it is how the bracing stops at the shield's edge instead
/// of running off it.
fn clip(subject: &[Pos2], outline: &[Pos2]) -> Vec<Pos2> {
    // Which side of an edge counts as inside depends on which way the outline
    // was wound. Work it out rather than assume it, so redrawing the shield
    // cannot silently turn the clip inside out and erase everything.
    let mut twice_area = 0.0f32;
    for i in 0..outline.len() {
        let a = outline[i];
        let b = outline[(i + 1) % outline.len()];
        twice_area += a.x * b.y - b.x * a.y;
    }
    let sign = if twice_area >= 0.0 { 1.0f32 } else { -1.0 };

    let mut out = subject.to_vec();
    for i in 0..outline.len() {
        if out.is_empty() {
            return out;
        }
        let a = outline[i];
        let b = outline[(i + 1) % outline.len()];
        let side = |p: Pos2| {
            sign * ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x))
        };
        let inside = |p: Pos2| side(p) >= 0.0;
        let cross = |p: Pos2, q: Pos2| -> Pos2 {
            let (dx, dy) = (q.x - p.x, q.y - p.y);
            let (ex, ey) = (b.x - a.x, b.y - a.y);
            let denominator = ex * dy - ey * dx;
            if denominator.abs() < 1e-6 {
                return p;
            }
            let t = (ex * (p.y - a.y) - ey * (p.x - a.x)) / denominator;
            pos2(p.x - dx * t, p.y - dy * t)
        };
        let mut next = Vec::with_capacity(out.len() + 2);
        for k in 0..out.len() {
            let current = out[k];
            let previous = out[(k + out.len() - 1) % out.len()];
            if inside(current) {
                if !inside(previous) {
                    next.push(cross(previous, current));
                }
                next.push(current);
            } else if inside(previous) {
                next.push(cross(previous, current));
            }
        }
        out = next;
    }
    out
}

/// A thick line as a quad, so it can be clipped like any other shape.
fn bar(from: Pos2, to: Pos2, width: f32) -> Vec<Pos2> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (nx, ny) = (-dy / length * width * 0.5, dx / length * width * 0.5);
    vec![
        pos2(from.x + nx, from.y + ny),
        pos2(to.x + nx, to.y + ny),
        pos2(to.x - nx, to.y - ny),
        pos2(from.x - nx, from.y - ny),
    ]
}

/// One filled shape of the mark. Everything the mark is made of is a polygon,
/// which is what lets the same description be painted by egui on screen and
/// scan-filled into a window icon without the two ever drifting apart.
pub struct Piece {
    pub points: Vec<Pos2>,
    pub fill: Color32,
}

fn circle(centre: Pos2, radius: f32) -> Vec<Pos2> {
    (0..40)
        .map(|i| {
            let a = i as f32 / 40.0 * std::f32::consts::TAU;
            pos2(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect()
}

fn rectangle(a: Pos2, b: Pos2) -> Vec<Pos2> {
    vec![a, pos2(b.x, a.y), b, pos2(a.x, b.y)]
}

/// Fits the shield's own proportion inside a box, centred, so an awkward
/// rectangle does not stretch it.
fn fitted(into: Rect) -> Rect {
    let ratio = 100.0 / 110.0;
    let (w, h) = if into.width() / into.height() > ratio {
        (into.height() * ratio, into.height())
    } else {
        (into.width(), into.width() / ratio)
    };
    Rect::from_center_size(into.center(), egui::vec2(w, h))
}

/// Every piece of the mark, back to front.
pub fn pieces(into: Rect, dark: bool) -> Vec<Piece> {
    let box_ = fitted(into);
    let at = |x: f64, y: f64| -> Pos2 {
        pos2(
            box_.left() + box_.width() * (x as f32 / 100.0),
            box_.top() + box_.height() * (y as f32 / 110.0),
        )
    };
    let unit = box_.width() / 100.0;

    // The shield's own navy is nearly the colour of the dark chrome it sits
    // on, so on dark the whole mark is lifted a step. The shape and the
    // relationships stay the same; only the level moves.
    let navy = if dark {
        Color32::from_rgb(28, 38, 54)
    } else {
        Color32::from_rgb(20, 28, 38)
    };
    let steel = if dark {
        Color32::from_rgb(58, 88, 124)
    } else {
        Color32::from_rgb(43, 68, 97)
    };
    let rim = if dark {
        Color32::from_rgb(88, 120, 156)
    } else {
        Color32::from_rgb(20, 28, 38)
    };
    let silver = Color32::from_rgb(207, 218, 230);
    let bright = Color32::from_rgb(240, 244, 248);

    let outline = shield(box_);
    let mut out = Vec::new();

    // The rim is the shield drawn slightly larger underneath, rather than a
    // stroke, so it rasterises the same way everything else does.
    out.push(Piece {
        points: grown(&outline, unit * 2.0),
        fill: rim,
    });
    out.push(Piece {
        points: outline.clone(),
        fill: navy,
    });

    // Cross bracing, the way a braced bay is drawn on a framing elevation, cut
    // off at the shield's edge rather than allowed to run past it.
    for shape in [
        bar(at(2.0, 18.0), at(98.0, 68.0), unit * 8.0),
        bar(at(98.0, 18.0), at(2.0, 68.0), unit * 8.0),
        bar(at(0.0, 16.0), at(100.0, 16.0), unit * 5.0),
    ] {
        let inside = clip(&shape, &outline);
        if inside.len() > 2 {
            out.push(Piece {
                points: inside,
                fill: steel,
            });
        }
    }

    // The blade, tapered, with a highlight down one side of the fuller so it
    // reads as steel rather than as a white stripe.
    out.push(Piece {
        points: vec![
            at(50.0, 11.0),
            at(57.5, 26.0),
            at(57.5, 60.0),
            at(42.5, 60.0),
            at(42.5, 26.0),
        ],
        fill: silver,
    });
    out.push(Piece {
        points: vec![at(50.0, 13.5), at(55.0, 27.0), at(55.0, 60.0), at(50.0, 60.0)],
        fill: bright,
    });
    // Crossguard, grip, pommel.
    out.push(Piece {
        points: rectangle(at(23.0, 57.0), at(77.0, 68.0)),
        fill: silver,
    });
    out.push(Piece {
        points: rectangle(at(23.0, 57.0), at(77.0, 61.5)),
        fill: bright,
    });
    out.push(Piece {
        points: rectangle(at(45.0, 68.0), at(55.0, 85.0)),
        fill: silver,
    });
    let centre = at(50.0, 91.0);
    out.push(Piece {
        points: circle(centre, unit * 8.0),
        fill: silver,
    });
    out.push(Piece {
        points: circle(centre, unit * 3.5),
        fill: navy,
    });
    out
}

/// Pushes a closed outline outwards from its own centre. Good enough for a rim
/// on a convex shape, and it keeps the rim a polygon like everything else.
fn grown(outline: &[Pos2], by: f32) -> Vec<Pos2> {
    let n = outline.len() as f32;
    let cx = outline.iter().map(|p| p.x).sum::<f32>() / n;
    let cy = outline.iter().map(|p| p.y).sum::<f32>() / n;
    outline
        .iter()
        .map(|p| {
            let (dx, dy) = (p.x - cx, p.y - cy);
            let d = (dx * dx + dy * dy).sqrt().max(1e-6);
            pos2(p.x + dx / d * by, p.y + dy / d * by)
        })
        .collect()
}

/// Paints the mark inside a rectangle.
pub fn draw(painter: &Painter, into: Rect, theme: Theme) {
    for piece in pieces(into, theme.is_dark()) {
        painter.add(egui::epaint::PathShape {
            points: piece.points,
            closed: true,
            fill: piece.fill,
            stroke: Stroke::NONE.into(),
        });
    }
}

/// The mark as pixels: RGBA, `size` square, transparent outside the shield.
///
/// This is what goes on the window and the taskbar. It is scan-filled from the
/// same pieces the screen draws, so the icon can never drift from the mark.
pub fn raster(size: usize) -> Vec<u8> {
    // Four samples per pixel each way: enough to take the jaggies off a shield
    // at 16 pixels, cheap enough to do at start-up.
    const OVER: usize = 4;
    let big = size * OVER;
    let mut buffer = vec![0u8; big * big * 4];
    let into = Rect::from_min_size(
        pos2(0.0, 0.0),
        egui::vec2(big as f32, big as f32),
    );
    for piece in pieces(into, false) {
        fill(&mut buffer, big, &piece.points, piece.fill);
    }

    // Average each block back down.
    let mut out = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut total = [0u32; 4];
            for sy in 0..OVER {
                for sx in 0..OVER {
                    let i = ((y * OVER + sy) * big + x * OVER + sx) * 4;
                    for c in 0..4 {
                        total[c] += buffer[i + c] as u32;
                    }
                }
            }
            let i = (y * size + x) * 4;
            for c in 0..4 {
                out[i + c] = (total[c] / (OVER * OVER) as u32) as u8;
            }
        }
    }
    out
}

/// Scanline fill of one polygon into an RGBA buffer, non-zero winding.
fn fill(buffer: &mut [u8], width: usize, points: &[Pos2], colour: Color32) {
    if points.len() < 3 {
        return;
    }
    let height = buffer.len() / (width * 4);
    let top = points.iter().fold(f32::MAX, |m, p| m.min(p.y)).floor().max(0.0) as usize;
    let bottom = (points.iter().fold(f32::MIN, |m, p| m.max(p.y)).ceil() as usize).min(height);
    let [r, g, b, a] = [colour.r(), colour.g(), colour.b(), colour.a()];
    for y in top..bottom {
        let scan = y as f32 + 0.5;
        let mut crossings: Vec<f32> = Vec::new();
        for i in 0..points.len() {
            let p = points[i];
            let q = points[(i + 1) % points.len()];
            if (p.y <= scan) == (q.y <= scan) {
                continue;
            }
            let t = (scan - p.y) / (q.y - p.y);
            crossings.push(p.x + t * (q.x - p.x));
        }
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in crossings.chunks(2) {
            let [from, to] = match pair {
                [from, to] => [*from, *to],
                _ => continue,
            };
            let start = from.round().max(0.0) as usize;
            let end = (to.round() as usize).min(width);
            for x in start..end {
                let i = (y * width + x) * 4;
                buffer[i] = r;
                buffer[i + 1] = g;
                buffer[i + 2] = b;
                buffer[i + 3] = a;
            }
        }
    }
}

/// The mark plus the program's name, for an empty window or an About box.
pub fn wordmark(painter: &Painter, at: Pos2, height: f32, theme: Theme) -> Rect {
    let shield_box = Rect::from_min_size(at, egui::vec2(height * 100.0 / 110.0, height));
    draw(painter, shield_box, theme);
    let name = painter.layout_no_wrap(
        "Excalibur".to_string(),
        egui::FontId::proportional(height * 0.40),
        theme.text,
    );
    let sub = painter.layout_no_wrap(
        "VIEW".to_string(),
        egui::FontId::proportional(height * 0.20),
        theme.faint,
    );
    let left = shield_box.right() + height * 0.22;
    let block = name.size().y + sub.size().y + height * 0.06;
    let top = shield_box.center().y - block / 2.0;
    painter.galley(pos2(left, top), name.clone(), theme.text);
    painter.galley(
        pos2(left + 2.0, top + name.size().y + height * 0.06),
        sub.clone(),
        theme.faint,
    );
    Rect::from_min_max(
        shield_box.left_top(),
        pos2(left + name.size().x.max(sub.size().x), shield_box.bottom()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shield_is_a_closed_shape_with_a_point_at_the_bottom() {
        let outline = shield(Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 110.0)));
        assert!(outline.len() > 20);
        let lowest = outline
            .iter()
            .cloned()
            .fold(f32::MIN, |low, p| low.max(p.y));
        let tip: Vec<&Pos2> = outline.iter().filter(|p| p.y > lowest - 0.5).collect();
        assert_eq!(tip.len(), 2, "the bottom is a point, not a flat");
        assert!((tip[0].x - 50.0).abs() < 1.0, "and it is in the middle");
    }

    #[test]
    fn the_bracing_is_cut_off_at_the_edge_of_the_shield() {
        let box_ = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 110.0));
        let outline = shield(box_);
        // A brace drawn well past both edges comes back inside them.
        let long = bar(pos2(-40.0, 20.0), pos2(140.0, 60.0), 8.0);
        let inside = clip(&long, &outline);
        assert!(inside.len() > 2, "it should survive the clip");
        for p in &inside {
            assert!(p.x >= -0.5 && p.x <= 100.5, "{p:?} escaped sideways");
        }
    }

    #[test]
    fn the_mark_keeps_its_shape_in_a_box_of_the_wrong_proportion() {
        let squashed = fitted(Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(400.0, 100.0)));
        let ratio = squashed.width() / squashed.height();
        assert!((ratio - 100.0 / 110.0).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn the_icon_has_a_shield_in_the_middle_and_nothing_in_the_corners() {
        let size = 64usize;
        let pixels = raster(size);
        let alpha = |x: usize, y: usize| pixels[(y * size + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0, "the top left corner is outside the shield");
        assert_eq!(alpha(size - 1, 0), 0, "and so is the top right");
        assert!(alpha(size / 2, size / 3) > 200, "but the middle is solid");
        // And the blade is lighter than the field it sits on.
        let at = |x: usize, y: usize| pixels[(y * size + x) * 4];
        assert!(
            at(size / 2, size / 4) > at(size / 6, size / 2),
            "the sword should be brighter than the shield"
        );
    }
}
