//! Icons, drawn rather than shipped.
//!
//! Each icon is a short path string in a 100 by 100 box. Drawing them as
//! vectors means they stay sharp at any display scale and weigh nothing, and
//! writing our own means none of Bluebeam's artwork is copied — the shapes are
//! the ordinary visual shorthand of the trade, drawn fresh.
//!
//! The path language is a small subset of SVG: `M x y`, `L x y`,
//! `C x1 y1 x2 y2 x y`, `Z` to close.

use egui::{epaint::PathShape, Color32, Pos2, Rect, Stroke, Vec2};

/// One subpath and whether it is filled or stroked.
pub type Stroke1 = (&'static str, bool);
pub type Glyph = &'static [Stroke1];

/// Turns a path string into points, mapped into `into`.
fn trace(d: &str, into: Rect) -> Vec<Vec<Pos2>> {
    let scale = into.width().min(into.height()) / 100.0;
    let origin = into.center() - Vec2::splat(50.0 * scale);
    let place = |x: f32, y: f32| Pos2::new(origin.x + x * scale, origin.y + y * scale);

    let mut out: Vec<Vec<Pos2>> = Vec::new();
    let mut current: Vec<Pos2> = Vec::new();
    let mut numbers: Vec<f32> = Vec::new();
    let mut command = ' ';

    let flush = |command: char, numbers: &mut Vec<f32>, current: &mut Vec<Pos2>| {
        match command {
            'M' => {
                if numbers.len() >= 2 {
                    current.push(place(numbers[0], numbers[1]));
                }
            }
            'L' => {
                for pair in numbers.chunks(2) {
                    if pair.len() == 2 {
                        current.push(place(pair[0], pair[1]));
                    }
                }
            }
            'C' => {
                for six in numbers.chunks(6) {
                    if six.len() < 6 {
                        continue;
                    }
                    let from = *current.last().unwrap_or(&place(six[0], six[1]));
                    let a = place(six[0], six[1]);
                    let b = place(six[2], six[3]);
                    let to = place(six[4], six[5]);
                    // Flatten the curve; icons are small, so a dozen steps is
                    // smoother than anything the eye can pick out.
                    for step in 1..=12 {
                        let t = step as f32 / 12.0;
                        let u = 1.0 - t;
                        let x = u * u * u * from.x
                            + 3.0 * u * u * t * a.x
                            + 3.0 * u * t * t * b.x
                            + t * t * t * to.x;
                        let y = u * u * u * from.y
                            + 3.0 * u * u * t * a.y
                            + 3.0 * u * t * t * b.y
                            + t * t * t * to.y;
                        current.push(Pos2::new(x, y));
                    }
                }
            }
            _ => {}
        }
        numbers.clear();
    };

    for token in d.split_whitespace() {
        match token {
            "M" | "L" | "C" => {
                flush(command, &mut numbers, &mut current);
                if token == "M" && !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                command = token.chars().next().unwrap();
            }
            "Z" => {
                flush(command, &mut numbers, &mut current);
                // Close the shape by coming back to where it started. Without
                // this a box is drawn with three sides.
                if let Some(first) = current.first().copied() {
                    if current.len() > 2 && *current.last().unwrap() != first {
                        current.push(first);
                    }
                }
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                command = ' ';
            }
            number => {
                if let Ok(v) = number.parse::<f32>() {
                    numbers.push(v);
                }
            }
        }
    }
    flush(command, &mut numbers, &mut current);
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// How heavy an icon's lines are, in the 100 by 100 box.
const LINE_WEIGHT: f32 = 7.5;

/// Draws a glyph into a rectangle.
///
/// Lines have round ends and round corners. Square-ended strokes at a heavy
/// weight were what made the old icons read as somebody else's; a rounded
/// line is softer and is ours.
pub fn draw(painter: &egui::Painter, glyph: Glyph, into: Rect, colour: Color32, weight: f32) {
    let width = (into.width().min(into.height()) / 100.0 * LINE_WEIGHT).max(1.0) * weight;
    for (d, filled) in glyph {
        for points in trace(d, into) {
            if points.len() < 2 {
                continue;
            }
            if *filled {
                painter.add(PathShape {
                    points,
                    closed: true,
                    fill: colour,
                    stroke: Stroke::NONE.into(),
                });
                continue;
            }
            let rounds = round_points(&points);
            painter.add(PathShape {
                points,
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(width, colour).into(),
            });
            for at in rounds {
                painter.circle_filled(at, width / 2.0, colour);
            }
        }
    }
}

/// Where a stroked run needs a round cap or a round corner: both ends of an
/// open run, and every corner that turns enough to show its square edge. The
/// gentle bends of a flattened curve are left alone.
fn round_points(points: &[Pos2]) -> Vec<Pos2> {
    let n = points.len();
    let mut out = Vec::new();
    if n < 2 {
        return out;
    }
    let closed = n > 2 && (points[0] - points[n - 1]).length() < 0.01;
    let turns = |a: Pos2, b: Pos2, c: Pos2| {
        let (u, v) = ((b - a).normalized(), (c - b).normalized());
        // More than about 20 degrees.
        u != Vec2::ZERO && v != Vec2::ZERO && u.dot(v) < 0.94
    };
    if closed {
        if turns(points[n - 2], points[0], points[1]) {
            out.push(points[0]);
        }
    } else {
        out.push(points[0]);
        out.push(points[n - 1]);
    }
    for i in 1..n - 1 {
        if turns(points[i - 1], points[i], points[i + 1]) {
            out.push(points[i]);
        }
    }
    out
}

/// How a tile behind a tool's icon looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    /// Waiting to be picked up.
    Rest,
    /// Under the pointer.
    Hover,
    /// Being clicked.
    Pressed,
    /// The tool in your hand.
    Active,
    /// Chosen, such as the panel that is open.
    Chosen,
    /// Not available here, such as a markup tool with no drawing open.
    Off,
}

/// The accent tiles' blue, top and bottom of the gradient.
const TILE_TOP: Color32 = Color32::from_rgb(58, 111, 184);
const TILE_BOTTOM: Color32 = Color32::from_rgb(43, 84, 143);
/// How much of the blue a tile shows while it waits, on a dark bar and on a
/// light one. A full-strength tile on every tool would turn a toolbar into a
/// wall of blue; resting tiles are the blue laid thinly over the bar, and the
/// tile comes up to full strength under the pointer and in your hand.
const REST_ON_DARK: f32 = 0.55;
const REST_ON_LIGHT: f32 = 0.16;
/// A glyph sitting on a full-strength tile.
pub const ON_TILE: Color32 = Color32::from_rgb(234, 242, 255);

/// Mixes two opaque colours: `t` of the way from `a` to `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let c = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(c(a.r(), b.r()), c(a.g(), b.g()), c(a.b(), b.b()))
}

/// True for a light bar, where a thin blue tile needs a dark glyph on it.
fn is_light(behind: Color32) -> bool {
    let (r, g, b) = (behind.r() as f32, behind.g() as f32, behind.b() as f32);
    0.299 * r + 0.587 * g + 0.114 * b > 150.0
}

/// The colours a tile is painted in, top and bottom, over `behind`.
pub fn tile_colours(look: Tile, behind: Color32) -> (Color32, Color32) {
    let thin = |t: f32| (mix(behind, TILE_TOP, t), mix(behind, TILE_BOTTOM, t));
    let light = is_light(behind);
    match look {
        Tile::Rest => thin(if light { REST_ON_LIGHT } else { REST_ON_DARK }),
        Tile::Hover | Tile::Chosen => (TILE_TOP, TILE_BOTTOM),
        Tile::Pressed => (mix(TILE_TOP, Color32::BLACK, 0.20), mix(TILE_BOTTOM, Color32::BLACK, 0.20)),
        Tile::Active => (mix(TILE_TOP, Color32::WHITE, 0.12), TILE_TOP),
        Tile::Off => thin(if light { 0.08 } else { 0.28 }),
    }
}

/// The colour for a glyph on a tile of this look. `plain` is the colour the
/// glyph would be without a tile, and `faint` the one for something that
/// cannot be used; a thin tile on a light bar keeps them, because white on
/// pale blue cannot be read.
pub fn glyph_on_tile(look: Tile, behind: Color32, plain: Color32, faint: Color32) -> Color32 {
    let light = is_light(behind);
    match look {
        Tile::Rest if light => plain,
        Tile::Off if light => faint,
        Tile::Off => {
            let (top, bottom) = tile_colours(look, behind);
            mix(mix(top, bottom, 0.5), ON_TILE, 0.40)
        }
        _ => ON_TILE,
    }
}

/// Paints a rounded tile with a soft top-to-bottom gradient.
///
/// The edge is drawn by egui's own anti-aliased rounded rectangle; the
/// gradient is laid over it a fraction inside, so the tile's outline stays
/// smooth while its face shades.
pub fn tile(painter: &egui::Painter, rect: Rect, look: Tile, behind: Color32) {
    let (top, bottom) = tile_colours(look, behind);
    let radius = (rect.width().min(rect.height()) * 0.25).round();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(radius.clamp(0.0, 255.0) as u8),
        mix(top, bottom, 0.5),
    );
    let face = rect.shrink(0.75);
    let r = (radius - 0.75).max(0.0);
    let shade = |p: Pos2| mix(top, bottom, (p.y - face.top()) / face.height().max(1.0));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(face.center(), shade(face.center()));
    let corners = [
        (Pos2::new(face.right() - r, face.top() + r), -90.0f32),
        (Pos2::new(face.right() - r, face.bottom() - r), 0.0),
        (Pos2::new(face.left() + r, face.bottom() - r), 90.0),
        (Pos2::new(face.left() + r, face.top() + r), 180.0),
    ];
    const STEPS: usize = 6;
    for (centre, from) in corners {
        for step in 0..=STEPS {
            let a = (from + 90.0 * step as f32 / STEPS as f32).to_radians();
            let p = centre + Vec2::new(a.cos(), a.sin()) * r;
            mesh.colored_vertex(p, shade(p));
        }
    }
    let rim = (mesh.vertices.len() - 1) as u32;
    for i in 0..rim {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % rim);
    }
    painter.add(egui::Shape::mesh(mesh));
}

// --- the glyphs -----------------------------------------------------------
// Everything sits inside 10..90 so icons of different shapes look the same
// weight beside each other on a toolbar.

pub const POINTER: Glyph = &[("M 30 12 L 30 78 L 46 62 L 58 88 L 70 82 L 58 56 L 78 56 Z", true)];
pub const HAND: Glyph = &[(
    "M 34 74 L 34 30 C 34 22 46 22 46 30 L 46 52 L 46 24 C 46 16 58 16 58 24 \
     L 58 52 L 58 28 C 58 20 70 20 70 28 L 70 56 C 70 78 58 86 46 86 C 36 86 30 80 26 70 \
     L 20 56 C 17 49 27 44 31 51 Z",
    false,
)];
pub const MAGNIFIER: Glyph = &[
    ("M 44 16 C 60 16 74 30 74 46 C 74 62 60 76 44 76 C 28 76 14 62 14 46 C 14 30 28 16 44 16 Z", false),
    ("M 66 68 L 88 88", false),
];
pub const LASSO: Glyph = &[
    ("M 50 16 C 74 16 90 28 90 44 C 90 60 74 72 50 72 C 26 72 10 60 10 44 C 10 28 26 16 50 16 Z", false),
    ("M 34 68 L 34 84 C 34 92 44 92 44 86", false),
];
pub const RULE: Glyph = &[
    ("M 10 34 L 90 34 L 90 66 L 10 66 Z", false),
    ("M 26 34 L 26 48 M 42 34 L 42 54 M 58 34 L 58 48 M 74 34 L 74 54", false),
];
pub const LENGTH: Glyph = &[
    ("M 14 76 L 86 24", false),
    ("M 8 66 L 24 86 M 76 14 L 92 34", false),
];
pub const POLYLENGTH: Glyph = &[("M 12 78 L 38 40 L 60 62 L 88 18", false)];
pub const AREA: Glyph = &[
    ("M 14 78 L 14 30 L 52 30 L 52 52 L 86 52 L 86 78 Z", false),
    ("M 26 60 L 40 60 M 26 68 L 60 68", false),
];
pub const PERIMETER: Glyph = &[("M 14 22 L 86 22 L 86 78 L 14 78 Z", false)];
pub const VOLUME: Glyph = &[
    ("M 20 34 L 56 18 L 90 34 L 90 68 L 56 84 L 20 68 Z", false),
    ("M 20 34 L 56 50 L 90 34 M 56 50 L 56 84", false),
];
pub const COUNT: Glyph = &[
    ("M 26 34 C 34 34 34 46 26 46 C 18 46 18 34 26 34 Z", true),
    ("M 26 62 C 34 62 34 74 26 74 C 18 74 18 62 26 62 Z", true),
    ("M 50 40 L 86 40 M 50 68 L 86 68", false),
];
pub const ANGLE: Glyph = &[
    ("M 16 82 L 16 18 L 84 82 Z", false),
    ("M 16 58 C 30 58 40 68 42 82", false),
];
pub const RADIUS: Glyph = &[
    ("M 50 86 C 26 86 10 68 10 50 C 10 30 26 14 50 14", false),
    ("M 50 50 L 86 34", false),
];
pub const DIAMETER: Glyph = &[
    ("M 50 12 C 72 12 88 30 88 50 C 88 70 72 88 50 88 C 28 88 12 70 12 50 C 12 30 28 12 50 12 Z", false),
    ("M 18 68 L 82 32", false),
];
pub const FILL: Glyph = &[
    ("M 22 46 L 52 16 L 82 46 L 52 76 Z", false),
    ("M 84 58 C 92 70 92 82 84 82 C 76 82 76 70 84 58 Z", true),
];
pub const CALIBRATE: Glyph = &[
    ("M 12 70 L 88 70", false),
    ("M 12 58 L 12 82 M 88 58 L 88 82", false),
    ("M 32 24 L 68 24 M 50 12 L 50 40", false),
];
pub const LINE: Glyph = &[("M 14 84 L 86 16", false)];
pub const ARROW: Glyph = &[
    ("M 14 84 L 78 22", false),
    ("M 88 12 L 60 20 L 80 40 Z", true),
];
pub const ARC: Glyph = &[("M 14 82 C 14 30 50 14 86 18", false)];
pub const POLYLINE: Glyph = &[
    ("M 12 78 L 38 40 L 60 62 L 88 18", false),
    ("M 12 78 C 18 78 18 86 12 86 C 6 86 6 78 12 78 Z", true),
    ("M 88 18 C 94 18 94 26 88 26 C 82 26 82 18 88 18 Z", true),
];
pub const RECTANGLE: Glyph = &[("M 14 26 L 86 26 L 86 74 L 14 74 Z", false)];
pub const ELLIPSE: Glyph = &[(
    "M 50 24 C 72 24 88 36 88 50 C 88 64 72 76 50 76 C 28 76 12 64 12 50 C 12 36 28 24 50 24 Z",
    false,
)];
pub const POLYGON: Glyph = &[("M 50 12 L 88 40 L 74 84 L 26 84 L 12 40 Z", false)];
pub const CLOUD: Glyph = &[(
    "M 22 68 C 12 68 12 52 24 52 C 22 34 46 28 52 42 C 62 32 82 40 80 54 C 92 56 90 68 80 68 Z",
    false,
)];
pub const TEXT_BOX: Glyph = &[
    ("M 12 22 L 88 22 L 88 78 L 12 78 Z", false),
    ("M 32 38 L 68 38 M 50 38 L 50 64", false),
];
pub const CALLOUT: Glyph = &[
    ("M 30 22 L 88 22 L 88 60 L 30 60 Z", false),
    ("M 30 42 L 8 78", false),
];
pub const NOTE: Glyph = &[
    ("M 14 20 L 86 20 L 86 64 L 46 64 L 26 84 L 26 64 L 14 64 Z", false),
];
pub const PEN: Glyph = &[
    ("M 18 82 L 26 60 L 70 16 L 84 30 L 40 74 Z", false),
    ("M 64 22 L 78 36", false),
];
pub const HIGHLIGHT: Glyph = &[
    ("M 20 62 L 58 24 L 76 42 L 38 80 L 20 80 Z", false),
    ("M 14 88 L 86 88", false),
];
pub const STAMP: Glyph = &[
    ("M 30 46 C 30 30 42 30 42 20 C 42 12 58 12 58 20 C 58 30 70 30 70 46 Z", false),
    ("M 18 58 L 82 58 L 82 72 L 18 72 Z", false),
    ("M 12 84 L 88 84", false),
];
pub const IMAGE: Glyph = &[
    ("M 12 24 L 88 24 L 88 76 L 12 76 Z", false),
    ("M 12 66 L 36 44 L 54 62 L 68 50 L 88 68", false),
    ("M 32 38 C 38 38 38 46 32 46 C 26 46 26 38 32 38 Z", true),
];
pub const SNAPSHOT: Glyph = &[
    ("M 10 30 L 30 30 M 10 30 L 10 48 M 90 30 L 70 30 M 90 30 L 90 48", false),
    ("M 10 70 L 30 70 M 10 70 L 10 52 M 90 70 L 70 70 M 90 70 L 90 52", false),
];
pub const UNDERLINE: Glyph = &[
    ("M 28 16 L 28 50 C 28 70 72 70 72 50 L 72 16", false),
    ("M 20 84 L 80 84", false),
];
pub const STRIKE: Glyph = &[
    ("M 28 16 L 28 50 C 28 70 72 70 72 50 L 72 16", false),
    ("M 14 50 L 86 50", false),
];
pub const SQUIGGLY: Glyph = &[
    ("M 28 12 L 28 44 C 28 62 72 62 72 44 L 72 12", false),
    ("M 14 74 C 26 60 38 88 50 74 C 62 60 74 88 86 74", false),
];
pub const OPEN: Glyph = &[
    ("M 10 74 L 10 26 L 40 26 L 48 36 L 84 36 L 84 74 Z", false),
];
pub const SAVE: Glyph = &[
    ("M 16 16 L 74 16 L 84 26 L 84 84 L 16 84 Z", false),
    ("M 32 16 L 32 42 L 68 42 L 68 16", false),
    ("M 32 84 L 32 60 L 68 60 L 68 84", false),
];
pub const PRINT: Glyph = &[
    ("M 28 36 L 28 14 L 72 14 L 72 36", false),
    ("M 14 36 L 86 36 L 86 68 L 72 68 L 72 86 L 28 86 L 28 68 L 14 68 Z", false),
];
pub const SEARCH: Glyph = MAGNIFIER;
pub const ROTATE_RIGHT: Glyph = &[
    ("M 22 62 C 26 78 40 88 56 88 C 76 88 90 72 90 54 C 90 36 76 20 56 20 L 30 20", false),
    ("M 44 8 L 26 20 L 44 32 Z", true),
];
pub const ROTATE_LEFT: Glyph = &[
    ("M 78 62 C 74 78 60 88 44 88 C 24 88 10 72 10 54 C 10 36 24 20 44 20 L 70 20", false),
    ("M 56 8 L 74 20 L 56 32 Z", true),
];
pub const UNDO: Glyph = &[
    ("M 24 44 L 62 44 C 82 44 88 58 88 70 C 88 80 82 86 74 88", false),
    ("M 38 28 L 18 44 L 38 60 Z", true),
];
pub const REDO: Glyph = &[
    ("M 76 44 L 38 44 C 18 44 12 58 12 70 C 12 80 18 86 26 88", false),
    ("M 62 28 L 82 44 L 62 60 Z", true),
];
pub const FIT_PAGE: Glyph = &[
    ("M 26 20 L 74 20 L 74 80 L 26 80 Z", false),
    ("M 10 34 L 10 12 L 32 12 M 90 66 L 90 88 L 68 88", false),
];
pub const FIT_WIDTH: Glyph = &[
    ("M 30 22 L 70 22 L 70 78 L 30 78 Z", false),
    ("M 16 50 L 4 50 M 84 50 L 96 50", false),
    ("M 10 44 L 4 50 L 10 56 M 90 44 L 96 50 L 90 56", false),
];
pub const ACTUAL_SIZE: Glyph = &[
    ("M 20 16 L 80 16 L 80 84 L 20 84 Z", false),
    ("M 32 42 L 38 38 L 38 66", false),
    ("M 50 46 C 54 46 54 52 50 52 C 46 52 46 46 50 46 Z", true),
    ("M 50 58 C 54 58 54 64 50 64 C 46 64 46 58 50 58 Z", true),
    ("M 62 42 L 68 38 L 68 66", false),
];
pub const SPLIT_VERTICAL: Glyph = &[
    ("M 12 18 L 88 18 L 88 82 L 12 82 Z", false),
    ("M 50 18 L 50 82", false),
];
pub const SPLIT_HORIZONTAL: Glyph = &[
    ("M 12 18 L 88 18 L 88 82 L 12 82 Z", false),
    ("M 12 50 L 88 50", false),
];
pub const FIRST: Glyph = &[("M 70 18 L 34 50 L 70 82 Z", true), ("M 26 18 L 26 82", false)];
pub const LAST: Glyph = &[("M 30 18 L 66 50 L 30 82 Z", true), ("M 74 18 L 74 82", false)];
pub const PREVIOUS: Glyph = &[("M 66 18 L 30 50 L 66 82 Z", true)];
pub const NEXT: Glyph = &[("M 34 18 L 70 50 L 34 82 Z", true)];
pub const FLATTEN: Glyph = &[
    ("M 16 30 L 84 30 M 22 50 L 78 50 M 28 70 L 72 70", false),
    ("M 50 78 L 50 90", false),
];
pub const LINK: Glyph = &[
    ("M 40 60 L 60 40", false),
    ("M 34 46 L 22 58 C 8 72 28 92 42 78 L 54 66", false),
    ("M 66 54 L 78 42 C 92 28 72 8 58 22 L 46 34", false),
];
pub const ATTACH: Glyph = &[(
    "M 68 30 L 36 62 C 28 70 40 82 48 74 L 80 42 C 92 30 74 12 62 24 L 28 58 C 12 74 36 98 52 82",
    false,
)];
pub const ERASER: Glyph = &[
    ("M 30 76 L 12 58 L 54 16 L 84 46 L 54 76 Z", false),
    ("M 30 76 L 84 76", false),
];
pub const CUT: Glyph = &[
    ("M 28 14 L 66 66 M 72 14 L 34 66", false),
    ("M 26 70 C 36 70 36 86 26 86 C 16 86 16 70 26 70 Z", false),
    ("M 74 70 C 84 70 84 86 74 86 C 64 86 64 70 74 70 Z", false),
];
pub const COPY: Glyph = &[
    ("M 26 12 L 74 12 L 74 64 L 26 64 Z", false),
    ("M 14 30 L 14 88 L 62 88 L 62 76", false),
];
pub const PASTE: Glyph = &[
    ("M 18 20 L 82 20 L 82 88 L 18 88 Z", false),
    ("M 36 20 L 36 10 L 64 10 L 64 20", false),
];
pub const DELETE: Glyph = &[
    ("M 24 28 L 76 28 L 70 88 L 30 88 Z", false),
    ("M 14 28 L 86 28 M 38 28 L 38 16 L 62 16 L 62 28", false),
];
pub const PANEL: Glyph = &[
    ("M 12 18 L 88 18 L 88 82 L 12 82 Z", false),
    ("M 38 18 L 38 82", false),
];
pub const GRID: Glyph = &[
    ("M 12 18 L 88 18 L 88 82 L 12 82 Z", false),
    ("M 12 40 L 88 40 M 12 61 L 88 61 M 40 18 L 40 82", false),
];
pub const THUMBNAILS: Glyph = &[
    ("M 14 14 L 44 14 L 44 46 L 14 46 Z", false),
    ("M 56 14 L 86 14 L 86 46 L 56 46 Z", false),
    ("M 14 56 L 44 56 L 44 86 L 14 86 Z", false),
    ("M 56 56 L 86 56 L 86 86 L 56 86 Z", false),
];
pub const CHEST: Glyph = &[
    ("M 12 36 L 88 36 L 88 84 L 12 84 Z", false),
    ("M 12 36 L 24 18 L 76 18 L 88 36", false),
    ("M 40 36 L 40 56 L 60 56 L 60 36", false),
];
pub const LAYERS: Glyph = &[
    ("M 50 12 L 90 34 L 50 56 L 10 34 Z", false),
    ("M 18 48 L 50 66 L 82 48 M 18 64 L 50 82 L 82 64", false),
];
pub const BOOKMARK: Glyph = &[("M 28 12 L 72 12 L 72 88 L 50 68 L 28 88 Z", false)];
pub const PROPERTIES: Glyph = &[
    ("M 20 28 L 80 28 M 20 50 L 80 50 M 20 72 L 80 72", false),
    ("M 36 20 L 36 36 M 62 42 L 62 58 M 44 64 L 44 80", false),
];
pub const SIGN: Glyph = &[
    ("M 12 72 C 30 72 34 30 46 30 C 58 30 50 72 64 72 C 74 72 78 58 88 58", false),
    ("M 12 86 L 88 86", false),
];
pub const FLAG: Glyph = &[
    ("M 26 12 L 26 88", false),
    ("M 26 18 L 78 18 L 66 38 L 78 58 L 26 58", false),
];
pub const INVERT: Glyph = &[
    ("M 50 12 C 72 12 88 30 88 50 C 88 70 72 88 50 88 C 28 88 12 70 12 50 C 12 30 28 12 50 12 Z", false),
    ("M 50 12 C 72 12 88 30 88 50 C 88 70 72 88 50 88 Z", true),
];


// Text and appearance, which need their own shapes rather than a letter.

pub const BOLD: Glyph = &[
    ("M 32 14 L 32 86", false),
    ("M 32 14 L 58 14 C 78 14 78 48 58 48 L 32 48", false),
    ("M 32 48 L 62 48 C 84 48 84 86 62 86 L 32 86", false),
];
pub const ITALIC: Glyph = &[("M 40 14 L 74 14 M 26 86 L 60 86 M 58 14 L 42 86", false)];
pub const UNDERLINE_TEXT: Glyph = &[
    ("M 28 14 L 28 48 C 28 72 72 72 72 48 L 72 14", false),
    ("M 20 86 L 80 86", false),
];
pub const STRIKE_TEXT: Glyph = &[
    ("M 30 20 C 30 8 70 8 70 22 C 70 40 30 38 30 58 C 30 76 72 76 70 62", false),
    ("M 12 48 L 88 48", false),
];
pub const ALIGN_LEFT: Glyph = &[("M 14 22 L 86 22 M 14 40 L 58 40 M 14 58 L 86 58 M 14 76 L 58 76", false)];
pub const ALIGN_CENTRE: Glyph = &[("M 14 22 L 86 22 M 28 40 L 72 40 M 14 58 L 86 58 M 28 76 L 72 76", false)];
pub const ALIGN_RIGHT: Glyph = &[("M 14 22 L 86 22 M 42 40 L 86 40 M 14 58 L 86 58 M 42 76 L 86 76", false)];
pub const SPELL: Glyph = &[
    ("M 14 66 L 32 24 L 50 66 M 21 52 L 43 52", false),
    ("M 60 80 L 72 92 L 92 62", false),
];
pub const TO_FRONT: Glyph = &[
    ("M 20 20 L 62 20 L 62 62 L 20 62 Z", false),
    ("M 40 40 L 82 40 L 82 82 L 40 82 Z", true),
];
pub const TO_BACK: Glyph = &[
    ("M 40 40 L 82 40 L 82 82 L 40 82 Z", false),
    ("M 20 20 L 62 20 L 62 62 L 20 62 Z", true),
];
pub const PALETTE: Glyph = &[
    ("M 50 14 C 76 14 90 32 90 50 C 90 64 78 66 70 66 C 60 66 56 74 62 80 C 68 88 60 90 50 90 C 26 90 10 72 10 50 C 10 30 26 14 50 14 Z", false),
    ("M 30 46 C 36 46 36 56 30 56 C 24 56 24 46 30 46 Z", true),
    ("M 50 32 C 56 32 56 42 50 42 C 44 42 44 32 50 32 Z", true),
    ("M 70 42 C 76 42 76 52 70 52 C 64 52 64 42 70 42 Z", true),
];
pub const OPACITY: Glyph = &[
    ("M 50 10 C 72 28 86 42 86 58 C 86 76 70 90 50 90 C 30 90 14 76 14 58 C 14 42 28 28 50 10 Z", false),
    ("M 50 10 C 72 28 86 42 86 58 C 86 76 70 90 50 90 Z", true),
];
pub const HATCH: Glyph = &[
    ("M 14 22 L 86 22 L 86 78 L 14 78 Z", false),
    ("M 24 78 L 66 22 M 44 78 L 86 22 M 14 60 L 42 22", false),
];
pub const LINE_WIDTH: Glyph = &[
    ("M 14 26 L 86 26", false),
    ("M 14 48 L 86 48 M 14 50 L 86 50", false),
    ("M 14 72 L 86 72 M 14 76 L 86 76 M 14 68 L 86 68", false),
];
pub const LINE_STYLE: Glyph = &[
    ("M 14 30 L 86 30", false),
    ("M 14 52 L 34 52 M 46 52 L 66 52 M 78 52 L 86 52", false),
    ("M 14 74 L 22 74 M 32 74 L 40 74 M 50 74 L 58 74 M 68 74 L 76 74 M 84 74 L 86 74", false),
];
pub const LINE_END: Glyph = &[
    ("M 12 50 L 66 50", false),
    ("M 88 50 L 62 38 L 62 62 Z", true),
];
pub const CROSSHAIR: Glyph = &[
    ("M 50 10 L 50 90 M 10 50 L 90 50", false),
    ("M 50 34 C 60 34 66 42 66 50 C 66 58 60 66 50 66 C 40 66 34 58 34 50 C 34 42 40 34 50 34 Z", false),
];


// --- documents -------------------------------------------------------------

/// A sheet of paper, the base of most of the document icons.
const SHEET: &str = "M 26 12 L 62 12 L 74 26 L 74 88 L 26 88 Z";

pub const COMBINE: Glyph = &[
    ("M 14 18 L 44 18 L 44 62 L 14 62 Z", false),
    ("M 56 38 L 86 38 L 86 82 L 56 82 Z", false),
    ("M 46 70 L 54 70 M 50 66 L 50 74", false),
];
pub const INSERT_PAGES: Glyph = &[
    ("M 16 14 L 46 14 L 46 86 L 16 86 Z", false),
    ("M 78 14 L 78 86 M 62 14 L 62 86", false),
    ("M 54 50 L 70 50 M 62 42 L 62 58", false),
];
pub const EXTRACT_PAGES: Glyph = &[
    ("M 16 14 L 52 14 L 52 86 L 16 86 Z", false),
    ("M 62 50 L 88 50", false),
    ("M 88 50 L 74 42 L 74 58 Z", true),
];
pub const SPLIT_DOC: Glyph = &[
    ("M 20 14 L 44 14 L 44 86 L 20 86 Z", false),
    ("M 56 14 L 80 14 L 80 86 L 56 86 Z", false),
    ("M 50 10 L 50 34 M 50 44 L 50 58 M 50 68 L 50 90", false),
];
pub const SLIP_SHEET: Glyph = &[
    ("M 14 26 L 54 26 L 54 82 L 14 82 Z", false),
    ("M 40 14 L 86 14 L 86 66 L 40 66", false),
    ("M 62 34 L 76 46 L 62 58", false),
];
pub const DELETE_PAGES: Glyph = &[
    (SHEET, false),
    ("M 38 44 L 62 68 M 62 44 L 38 68", false),
];
pub const HEADER_FOOTER: Glyph = &[
    (SHEET, false),
    ("M 34 24 L 66 24 M 34 78 L 66 78", false),
];
pub const SHRINK: Glyph = &[
    (SHEET, false),
    ("M 38 58 L 38 42 L 62 42 L 62 58 Z", false),
    ("M 32 68 L 68 68", false),
];
pub const NUMBER_PAGES: Glyph = &[
    (SHEET, false),
    ("M 44 72 L 50 68 L 50 84 M 44 84 L 56 84", false),
];
/// A circle of arrows around a downward one: look for a newer version.
pub const CHECK_UPDATES: Glyph = &[
    ("M 82 42 C 78 24 62 12 44 14 C 26 16 12 32 14 52", false),
    ("M 88 26 L 82 44 L 66 36", false),
    ("M 18 58 C 22 76 38 88 56 86 C 74 84 88 68 86 48", false),
    ("M 12 74 L 18 56 L 34 64", false),
    ("M 50 32 L 50 62", false),
    ("M 50 70 L 40 56 L 60 56 Z", true),
];
pub const RECENT: Glyph = &[
    ("M 50 12 C 71 12 88 29 88 50 C 88 71 71 88 50 88 C 29 88 12 71 12 50 C 12 29 29 12 50 12 Z", false),
    ("M 50 28 L 50 52 L 68 62", false),
];
pub const REVERT: Glyph = &[
    ("M 22 34 C 36 16 64 14 78 30 C 92 46 88 72 68 82 C 52 90 34 84 26 70", false),
    ("M 12 20 L 24 38 L 42 30", false),
];
pub const CLOSE: Glyph = &[
    (SHEET, false),
    ("M 40 44 L 60 64 M 60 44 L 40 64", false),
];
pub const CLOSE_ALL: Glyph = &[
    ("M 14 10 L 42 10 L 52 20 L 52 70 L 14 70 Z", false),
    ("M 48 30 L 76 30 L 86 40 L 86 90 L 48 90 Z", false),
    ("M 58 52 L 76 70 M 76 52 L 58 70", false),
];
pub const SUMMARY: Glyph = &[
    (SHEET, false),
    ("M 36 34 L 64 34 M 36 48 L 64 48 M 36 62 L 52 62", false),
    ("M 36 74 L 64 74", false),
];
pub const REVISION_COST: Glyph = &[
    // Two issues of a sheet, and an arrow between them going up.
    ("M 8 24 L 38 24 L 38 84 L 8 84 Z", false),
    ("M 62 24 L 92 24 L 92 84 L 62 84 Z", false),
    ("M 44 60 L 56 60", false),
    ("M 50 74 L 50 40 M 42 50 L 50 40 L 58 50", false),
];
pub const DOUBLES: Glyph = &[
    // Two of the same thing, one over the other, with a question over them.
    ("M 16 40 L 56 40 L 56 80 L 16 80 Z", false),
    ("M 30 26 L 70 26 L 70 66 L 30 66 Z", false),
    ("M 74 14 C 74 6 88 6 88 14 C 88 21 81 20 81 28", false),
    ("M 81 36 L 81 38", false),
];
pub const SHOP_LIST: Glyph = &[
    // A list of lengths over the stick they get cut out of.
    ("M 14 18 L 18 18 M 28 18 L 86 18", false),
    ("M 14 32 L 18 32 M 28 32 L 74 32", false),
    // The stick.
    ("M 8 56 L 92 56 L 92 74 L 8 74 Z", false),
    // Where the saw goes.
    ("M 36 48 L 36 82 M 62 48 L 62 82", false),
];
pub const OCR: Glyph = &[
    (SHEET, false),
    ("M 36 68 L 44 40 L 52 68 M 39 58 L 49 58", false),
    ("M 64 44 C 70 40 70 52 64 52 L 64 68", false),
];
pub const COMPARE: Glyph = &[
    ("M 12 16 L 44 16 L 44 84 L 12 84 Z", false),
    ("M 56 16 L 88 16 L 88 84 L 56 84 Z", false),
    ("M 64 38 L 80 38 M 64 54 L 80 54 M 64 68 L 74 68", false),
    ("M 20 38 L 36 38 M 20 54 L 30 54", false),
];
pub const OVERLAY: Glyph = &[
    ("M 14 14 L 62 14 L 62 62 L 14 62 Z", false),
    ("M 38 38 L 86 38 L 86 86 L 38 86 Z", false),
];
pub const FINGERPRINT: Glyph = &[
    ("M 50 16 C 70 16 86 32 86 52 C 86 62 84 72 80 82", false),
    ("M 50 30 C 62 30 72 40 72 52 C 72 64 70 74 66 84", false),
    ("M 50 44 C 55 44 58 48 58 52 C 58 66 55 76 51 86", false),
    ("M 14 52 C 14 32 26 20 40 16", false),
    ("M 24 76 C 30 68 34 60 34 52 C 34 44 40 38 46 36", false),
];
pub const REPAIR: Glyph = &[
    (SHEET, false),
    ("M 36 40 L 50 54 L 40 64 L 34 58 Z", false),
    ("M 52 52 L 68 68", false),
];
pub const UNFLATTEN: Glyph = &[
    ("M 16 68 L 84 68", false),
    ("M 50 60 L 50 20", false),
    ("M 50 14 L 40 30 L 60 30 Z", true),
];
pub const APPLY_REDACTION: Glyph = &[
    (SHEET, false),
    ("M 34 40 L 66 40 L 66 52 L 34 52 Z", true),
    ("M 34 62 L 56 62 L 56 72 L 34 72 Z", true),
];
pub const SECURITY: Glyph = &[
    ("M 30 46 L 30 32 C 30 20 40 12 50 12 C 60 12 70 20 70 32 L 70 46", false),
    ("M 22 46 L 78 46 L 78 88 L 22 88 Z", false),
];
pub const PAGE_LABELS: Glyph = &[
    (SHEET, false),
    ("M 36 40 L 64 40 M 36 56 L 64 56", false),
    ("M 36 72 L 50 72", false),
];
pub const EXPORT: Glyph = &[
    (SHEET, false),
    ("M 50 74 L 50 36", false),
    ("M 50 30 L 40 48 L 60 48 Z", true),
];

// --- editing ---------------------------------------------------------------

pub const OFFSET: Glyph = &[
    ("M 14 20 L 52 20 L 52 58 L 14 58 Z", false),
    ("M 34 40 L 86 40 L 86 88 L 34 88 Z", false),
];
pub const MULTIPLY_COPIES: Glyph = &[
    ("M 12 12 L 40 12 L 40 40 L 12 40 Z", false),
    ("M 58 12 L 86 12 L 86 40 L 58 40 Z", false),
    ("M 12 58 L 40 58 L 40 86 L 12 86 Z", false),
    ("M 58 58 L 86 58 L 86 86 L 58 86 Z", false),
];
pub const SELECT_ALL: Glyph = &[
    ("M 12 24 L 12 12 L 24 12 M 76 12 L 88 12 L 88 24 M 88 76 L 88 88 L 76 88 \
      M 24 88 L 12 88 L 12 76", false),
    ("M 40 12 L 60 12 M 88 40 L 88 60 M 60 88 L 40 88 M 12 60 L 12 40", false),
    ("M 32 32 L 68 32 L 68 68 L 32 68 Z", false),
];
pub const PASTE_IN_PLACE: Glyph = &[
    ("M 24 20 L 76 20 L 76 88 L 24 88 Z", false),
    ("M 40 12 L 60 12 L 60 28 L 40 28 Z", false),
    ("M 38 50 L 62 50 M 50 40 L 50 60", false),
    ("M 36 70 L 64 70", false),
];
pub const UNDO_HISTORY: Glyph = &[
    ("M 22 34 C 36 16 64 14 78 30 C 92 46 88 72 68 82 C 52 90 34 84 26 70", false),
    ("M 12 20 L 24 38 L 42 30", false),
    ("M 56 40 L 56 56 L 68 62", false),
];
pub const FORMAT_PAINTER: Glyph = &[
    ("M 36 14 L 64 14 L 64 34 L 36 34 Z", false),
    ("M 30 34 L 70 34 L 70 50 L 30 50 Z", false),
    ("M 44 50 L 56 50 L 56 88 L 44 88 Z", false),
];
pub const SNAP: Glyph = &[
    ("M 28 78 L 28 42 C 28 26 40 16 50 16 C 60 16 72 26 72 42 L 72 78", false),
    ("M 28 62 L 44 62 M 56 62 L 72 62", false),
];

// --- laying out ------------------------------------------------------------

pub const ALIGN_TOP: Glyph = &[
    ("M 12 14 L 88 14", false),
    ("M 26 22 L 42 22 L 42 86 L 26 86 Z", false),
    ("M 58 22 L 74 22 L 74 58 L 58 58 Z", false),
];
pub const ALIGN_MIDDLE: Glyph = &[
    ("M 12 50 L 88 50", false),
    ("M 26 18 L 42 18 L 42 82 L 26 82 Z", false),
    ("M 58 32 L 74 32 L 74 68 L 58 68 Z", false),
];
pub const ALIGN_BOTTOM: Glyph = &[
    ("M 12 86 L 88 86", false),
    ("M 26 14 L 42 14 L 42 78 L 26 78 Z", false),
    ("M 58 42 L 74 42 L 74 78 L 58 78 Z", false),
];
pub const SAME_WIDTH: Glyph = &[
    ("M 18 22 L 82 22 L 82 42 L 18 42 Z", false),
    ("M 18 58 L 82 58 L 82 78 L 18 78 Z", false),
    ("M 18 12 L 18 88 M 82 12 L 82 88", false),
];
pub const SAME_HEIGHT: Glyph = &[
    ("M 22 18 L 42 18 L 42 82 L 22 82 Z", false),
    ("M 58 18 L 78 18 L 78 82 L 58 82 Z", false),
    ("M 12 18 L 88 18 M 12 82 L 88 82", false),
];
pub const SAME_SIZE: Glyph = &[
    ("M 16 16 L 56 16 L 56 56 L 16 56 Z", false),
    ("M 44 44 L 84 44 L 84 84 L 44 84 Z", false),
];
pub const CENTRE_ON_SHEET: Glyph = &[
    ("M 12 12 L 88 12 L 88 88 L 12 88 Z", false),
    ("M 36 36 L 64 36 L 64 64 L 36 64 Z", false),
    ("M 50 12 L 50 36 M 50 64 L 50 88 M 12 50 L 36 50 M 64 50 L 88 50", false),
];
pub const SPACE_ACROSS: Glyph = &[
    ("M 12 24 L 24 24 L 24 76 L 12 76 Z", false),
    ("M 44 24 L 56 24 L 56 76 L 44 76 Z", false),
    ("M 76 24 L 88 24 L 88 76 L 76 76 Z", false),
];
pub const SPACE_DOWN: Glyph = &[
    ("M 24 12 L 76 12 L 76 24 L 24 24 Z", false),
    ("M 24 44 L 76 44 L 76 56 L 24 56 Z", false),
    ("M 24 76 L 76 76 L 76 88 L 24 88 Z", false),
];
pub const FLIP_ACROSS: Glyph = &[
    ("M 50 10 L 50 90", false),
    ("M 42 26 L 16 50 L 42 74 Z", false),
    ("M 58 26 L 84 50 L 58 74 Z", true),
];
pub const FLIP_DOWN: Glyph = &[
    ("M 10 50 L 90 50", false),
    ("M 26 42 L 50 16 L 74 42 Z", false),
    ("M 26 58 L 50 84 L 74 58 Z", true),
];
pub const FORWARD_ONE: Glyph = &[
    // The one in front, solid, over the one behind.
    ("M 14 14 L 58 14 L 58 58 L 14 58 Z", false),
    ("M 42 42 L 86 42 L 86 86 L 42 86 Z", true),
];
pub const BACKWARD_ONE: Glyph = &[
    // The same pair the other way round: the solid one has gone behind.
    ("M 14 14 L 58 14 L 58 58 L 14 58 Z", true),
    ("M 42 42 L 86 42 L 86 86 L 42 86 Z", false),
];

// --- text ------------------------------------------------------------------

pub const SUPERSCRIPT: Glyph = &[
    ("M 16 82 L 40 30 L 64 82 M 26 64 L 54 64", false),
    ("M 70 34 L 70 16 L 88 16 L 88 34", false),
];
pub const SUBSCRIPT: Glyph = &[
    ("M 16 70 L 40 18 L 64 70 M 26 52 L 54 52", false),
    ("M 70 88 L 70 70 L 88 70 L 88 88", false),
];
pub const TYPEWRITER: Glyph = &[
    ("M 20 40 L 80 40 M 50 40 L 50 76", false),
    ("M 38 76 L 62 76", false),
    ("M 14 22 L 86 22", false),
];
pub const KEYBOARD: Glyph = &[
    ("M 10 28 L 90 28 L 90 72 L 10 72 Z", false),
    ("M 24 40 L 32 40 M 42 40 L 50 40 M 60 40 L 68 40", false),
    ("M 34 58 L 66 58", false),
];
pub const INFO: Glyph = &[
    ("M 50 12 C 71 12 88 29 88 50 C 88 71 71 88 50 88 C 29 88 12 71 12 50 C 12 29 29 12 50 12 Z", false),
    ("M 50 44 L 50 70", false),
    ("M 50 28 L 50 34", false),
];

// --- batch -----------------------------------------------------------------

/// The stack that marks a batch icon: a whole list of sets rather than one.
const STACK: &str = "M 10 26 L 10 90 L 74 90";

pub const BATCH_COMBINE: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 44 42 L 72 42 M 58 28 L 58 56", false)];
pub const BATCH_SPLIT: Glyph = &[(STACK, false), ("M 26 10 L 54 10 L 54 74 L 26 74 Z", false), ("M 66 10 L 90 10 L 90 74 L 66 74 Z", false)];
pub const BATCH_STAMP: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 36 20 L 80 20 M 36 64 L 80 64", false)];
pub const BATCH_ROTATE: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 44 52 C 44 34 62 28 74 36", false), ("M 80 26 L 78 42 L 64 38 Z", true)];
pub const BATCH_FLATTEN: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 36 60 L 80 60 M 58 22 L 58 52", false), ("M 58 56 L 50 42 L 66 42 Z", true)];
pub const BATCH_SHRINK: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 46 52 L 46 32 L 70 32 L 70 52 Z", false)];
pub const BATCH_OCR: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 40 60 L 50 26 L 60 60 M 44 48 L 56 48", false), ("M 72 32 C 80 28 80 44 72 44 L 72 60", false)];
pub const BATCH_SEAL: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 58 24 C 68 24 76 32 76 42 C 76 52 68 60 58 60 C 48 60 40 52 40 42 C 40 32 48 24 58 24 Z", false)];
pub const BATCH_SLIP: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 44 42 L 68 42", false), ("M 74 42 L 60 34 L 60 50 Z", true)];
pub const BATCH_COMPARE: Glyph = &[(STACK, false), ("M 26 10 L 54 10 L 54 74 L 26 74 Z", false), ("M 66 10 L 90 10 L 90 74 L 66 74 Z", false), ("M 72 30 L 84 30 M 72 46 L 84 46", false)];
pub const BATCH_CROP: Glyph = &[(STACK, false), ("M 38 10 L 38 62 L 90 62", false), ("M 26 26 L 78 26 L 78 78", false)];
pub const BATCH_PRINT: Glyph = &[(STACK, false), ("M 38 10 L 78 10 L 78 28 L 38 28 Z", false), ("M 26 28 L 90 28 L 90 58 L 26 58 Z", false), ("M 38 46 L 78 46 L 78 74 L 38 74 Z", false)];
pub const BATCH_SUMMARY: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 38 26 L 78 26 M 38 40 L 78 40 M 38 54 L 62 54", false)];
pub const BATCH_OVERLAY: Glyph = &[(STACK, false), ("M 26 10 L 66 10 L 66 50 L 26 50 Z", false), ("M 50 34 L 90 34 L 90 74 L 50 74 Z", false)];
pub const BATCH_SECURITY: Glyph = &[(STACK, false), ("M 26 10 L 90 10 L 90 74 L 26 74 Z", false), ("M 48 40 L 48 32 C 48 24 54 20 58 20 C 62 20 68 24 68 32 L 68 40", false), ("M 44 40 L 72 40 L 72 64 L 44 64 Z", false)];

// --- the rest --------------------------------------------------------------

pub const CLOUD_POLYGON: Glyph = &[
    // Scallops round a shape with straight sides, which is what tells this
    // apart from the box cloud beside it on the toolbar.
    (
        "M 50 14 C 52 11 57 10 60 12 C 64 15 64 19 62 23 C 64 19 69 19 72 21 C 76 23 76 28 74 31 C 76 28 81 27 84 30 C 88 32 88 37 86 40 C 90 41 92 46 91 50 C 90 54 85 56 81 55 C 85 56 88 60 86 64 C 85 68 81 71 77 69 C 81 71 83 75 82 79 C 80 83 76 85 72 84 C 72 88 69 91 65 91 C 61 91 57 88 57 84 C 57 88 54 91 50 91 C 46 91 43 88 43 84 C 43 88 39 91 35 91 C 31 91 28 88 28 84 C 24 85 20 83 18 79 C 17 75 19 71 23 69 C 19 71 15 68 14 64 C 12 60 15 56 19 55 C 15 56 10 54 9 50 C 8 46 10 41 14 40 C 12 37 12 32 16 30 C 19 27 24 28 26 31 C 24 28 24 23 28 21 C 31 19 36 19 38 23 C 36 19 36 15 40 12 C 43 10 48 11 50 14 Z",
        false,
    ),
];
pub const DIMENSION: Glyph = &[
    ("M 20 30 L 20 70 M 80 30 L 80 70", false),
    ("M 20 50 L 80 50", false),
    ("M 20 50 L 34 43 L 34 57 Z", true),
    ("M 80 50 L 66 43 L 66 57 Z", true),
];
pub const REDACTION: Glyph = &[
    ("M 14 22 L 86 22 L 86 40 L 14 40 Z", true),
    ("M 14 52 L 60 52 L 60 70 L 14 70 Z", true),
    ("M 14 80 L 50 80", false),
];
pub const SPACE: Glyph = &[
    // A room in plan, with a doorway in one wall: a named area of the job.
    ("M 14 18 L 86 18 L 86 82 L 14 82 Z", false),
    ("M 14 44 L 14 58", false),
    ("M 52 18 L 52 50 L 86 50", false),
];
pub const SKETCH: Glyph = &[
    ("M 16 76 L 40 34 L 64 58 L 84 24", false),
    ("M 16 88 L 84 88", false),
    ("M 16 84 L 16 92 M 84 84 L 84 92", false),
];
pub const ELLIPSE_CUTOUT: Glyph = &[
    ("M 12 20 L 88 20 L 88 80 L 12 80 Z", false),
    ("M 50 34 C 62 34 70 42 70 50 C 70 58 62 66 50 66 C 38 66 30 58 30 50 C 30 42 38 34 50 34 Z", false),
    ("M 34 64 L 66 36", false),
];
pub const AREA_CUTOUT: Glyph = &[
    ("M 12 20 L 88 20 L 88 80 L 12 80 Z", false),
    ("M 34 36 L 66 36 L 66 64 L 34 64 Z", false),
    ("M 34 64 L 66 36", false),
];
pub const SHOW_GRID: Glyph = &[
    ("M 12 12 L 88 12 L 88 88 L 12 88 Z", false),
    ("M 37 12 L 37 88 M 63 12 L 63 88 M 12 37 L 88 37 M 12 63 L 88 63", false),
];
pub const SNAP_TO_GRID: Glyph = &[
    ("M 12 12 L 88 12 L 88 88 L 12 88 Z", false),
    ("M 37 12 L 37 88 M 63 12 L 63 88 M 12 37 L 88 37 M 12 63 L 88 63", false),
    ("M 63 37 C 69 37 74 42 74 48 C 74 54 69 59 63 59 C 57 59 52 54 52 48 C 52 42 57 37 63 37 Z", true),
];
pub const FULL_SCREEN: Glyph = &[
    ("M 12 34 L 12 12 L 34 12 M 66 12 L 88 12 L 88 34 \
      M 88 66 L 88 88 L 66 88 M 34 88 L 12 88 L 12 66", false),
];
pub const PAGE_SINGLE: Glyph = &[("M 32 12 L 68 12 L 68 88 L 32 88 Z", false)];
pub const PAGE_CONTINUOUS: Glyph = &[
    ("M 32 6 L 68 6 L 68 44 L 32 44 Z", false),
    ("M 32 56 L 68 56 L 68 94 L 32 94 Z", false),
];
pub const PAGE_FACING: Glyph = &[
    ("M 8 20 L 46 20 L 46 80 L 8 80 Z", false),
    ("M 54 20 L 92 20 L 92 80 L 54 80 Z", false),
];
pub const PAGE_FACING_CONTINUOUS: Glyph = &[
    ("M 8 8 L 46 8 L 46 46 L 8 46 Z", false),
    ("M 54 8 L 92 8 L 92 46 L 54 46 Z", false),
    ("M 8 54 L 46 54 L 46 92 L 8 92 Z", false),
    ("M 54 54 L 92 54 L 92 92 L 54 92 Z", false),
];
pub const VIEW_BACK: Glyph = &[
    ("M 24 36 C 40 18 66 20 78 36 C 90 52 86 76 66 84", false),
    ("M 12 24 L 26 42 L 44 34", false),
];
pub const VIEW_FORWARD: Glyph = &[
    ("M 76 36 C 60 18 34 20 22 36 C 10 52 14 76 34 84", false),
    ("M 88 24 L 74 42 L 56 34", false),
];
pub const FORM_FIELD: Glyph = &[
    ("M 12 32 L 88 32 L 88 68 L 12 68 Z", false),
    ("M 26 42 L 26 58", false),
];
pub const CHECK_BOX: Glyph = &[
    ("M 16 16 L 84 16 L 84 84 L 16 84 Z", false),
    ("M 32 52 L 46 66 L 70 34", false),
];
pub const RADIO_BUTTON: Glyph = &[
    ("M 50 12 C 71 12 88 29 88 50 C 88 71 71 88 50 88 C 29 88 12 71 12 50 C 12 29 29 12 50 12 Z", false),
    ("M 50 32 C 60 32 68 40 68 50 C 68 60 60 68 50 68 C 40 68 32 60 32 50 C 32 40 40 32 50 32 Z", true),
];
pub const LIST_BOX: Glyph = &[
    ("M 16 16 L 84 16 L 84 84 L 16 84 Z", false),
    ("M 28 34 L 72 34 M 28 50 L 72 50 M 28 66 L 72 66", false),
];
pub const COMBO_BOX: Glyph = &[
    ("M 12 32 L 88 32 L 88 68 L 12 68 Z", false),
    ("M 64 44 L 76 56 L 88 44", false),
];
pub const PUSH_BUTTON: Glyph = &[
    ("M 14 30 L 86 30 L 86 70 L 14 70 Z", false),
    ("M 34 50 L 66 50", false),
];
pub const FORM_EDITOR: Glyph = &[
    ("M 14 18 L 86 18 L 86 82 L 14 82 Z", false),
    ("M 26 36 L 60 36 M 26 52 L 74 52 M 26 68 L 48 68", false),
];

/// Every glyph with its name, for looking at the set as a whole.
pub const CONTACT_SHEET: &[(&str, Glyph)] = &[
    ("pointer", POINTER),
    ("hand", HAND),
    ("magnifier", MAGNIFIER),
    ("lasso", LASSO),
    ("rule", RULE),
    ("length", LENGTH),
    ("polylength", POLYLENGTH),
    ("area", AREA),
    ("perimeter", PERIMETER),
    ("volume", VOLUME),
    ("count", COUNT),
    ("angle", ANGLE),
    ("radius", RADIUS),
    ("diameter", DIAMETER),
    ("fill", FILL),
    ("calibrate", CALIBRATE),
    ("line", LINE),
    ("arrow", ARROW),
    ("arc", ARC),
    ("polyline", POLYLINE),
    ("rectangle", RECTANGLE),
    ("ellipse", ELLIPSE),
    ("polygon", POLYGON),
    ("cloud", CLOUD),
    ("text box", TEXT_BOX),
    ("callout", CALLOUT),
    ("note", NOTE),
    ("pen", PEN),
    ("highlight", HIGHLIGHT),
    ("stamp", STAMP),
    ("image", IMAGE),
    ("snapshot", SNAPSHOT),
    ("underline", UNDERLINE),
    ("strike", STRIKE),
    ("squiggly", SQUIGGLY),
    ("open", OPEN),
    ("save", SAVE),
    ("print", PRINT),
    ("search", SEARCH),
    ("rotate right", ROTATE_RIGHT),
    ("rotate left", ROTATE_LEFT),
    ("undo", UNDO),
    ("redo", REDO),
    ("fit page", FIT_PAGE),
    ("fit width", FIT_WIDTH),
    ("actual size", ACTUAL_SIZE),
    ("split vertical", SPLIT_VERTICAL),
    ("split horizontal", SPLIT_HORIZONTAL),
    ("first", FIRST),
    ("last", LAST),
    ("previous", PREVIOUS),
    ("next", NEXT),
    ("flatten", FLATTEN),
    ("link", LINK),
    ("attach", ATTACH),
    ("eraser", ERASER),
    ("cut", CUT),
    ("copy", COPY),
    ("paste", PASTE),
    ("delete", DELETE),
    ("panel", PANEL),
    ("grid", GRID),
    ("thumbnails", THUMBNAILS),
    ("chest", CHEST),
    ("layers", LAYERS),
    ("bookmark", BOOKMARK),
    ("properties", PROPERTIES),
    ("sign", SIGN),
    ("flag", FLAG),
    ("invert", INVERT),
    ("bold", BOLD),
    ("italic", ITALIC),
    ("underline text", UNDERLINE_TEXT),
    ("strike text", STRIKE_TEXT),
    ("align left", ALIGN_LEFT),
    ("align centre", ALIGN_CENTRE),
    ("align right", ALIGN_RIGHT),
    ("spell", SPELL),
    ("to front", TO_FRONT),
    ("to back", TO_BACK),
    ("palette", PALETTE),
    ("opacity", OPACITY),
    ("hatch", HATCH),
    ("line width", LINE_WIDTH),
    ("line style", LINE_STYLE),
    ("line end", LINE_END),
    ("crosshair", CROSSHAIR),
    ("combine", COMBINE),
    ("insert pages", INSERT_PAGES),
    ("extract pages", EXTRACT_PAGES),
    ("split doc", SPLIT_DOC),
    ("slip sheet", SLIP_SHEET),
    ("delete pages", DELETE_PAGES),
    ("header footer", HEADER_FOOTER),
    ("shrink", SHRINK),
    ("number pages", NUMBER_PAGES),
    ("recent", RECENT),
    ("revert", REVERT),
    ("close", CLOSE),
    ("close all", CLOSE_ALL),
    ("summary", SUMMARY),
    ("ocr", OCR),
    ("compare", COMPARE),
    ("overlay", OVERLAY),
    ("fingerprint", FINGERPRINT),
    ("repair", REPAIR),
    ("unflatten", UNFLATTEN),
    ("apply redaction", APPLY_REDACTION),
    ("security", SECURITY),
    ("page labels", PAGE_LABELS),
    ("export", EXPORT),
    ("offset", OFFSET),
    ("multiply copies", MULTIPLY_COPIES),
    ("select all", SELECT_ALL),
    ("paste in place", PASTE_IN_PLACE),
    ("undo history", UNDO_HISTORY),
    ("format painter", FORMAT_PAINTER),
    ("snap", SNAP),
    ("align top", ALIGN_TOP),
    ("align middle", ALIGN_MIDDLE),
    ("align bottom", ALIGN_BOTTOM),
    ("same width", SAME_WIDTH),
    ("same height", SAME_HEIGHT),
    ("same size", SAME_SIZE),
    ("centre on sheet", CENTRE_ON_SHEET),
    ("space across", SPACE_ACROSS),
    ("space down", SPACE_DOWN),
    ("flip across", FLIP_ACROSS),
    ("flip down", FLIP_DOWN),
    ("forward one", FORWARD_ONE),
    ("backward one", BACKWARD_ONE),
    ("superscript", SUPERSCRIPT),
    ("subscript", SUBSCRIPT),
    ("typewriter", TYPEWRITER),
    ("keyboard", KEYBOARD),
    ("info", INFO),
    ("batch combine", BATCH_COMBINE),
    ("batch split", BATCH_SPLIT),
    ("batch stamp", BATCH_STAMP),
    ("batch rotate", BATCH_ROTATE),
    ("batch flatten", BATCH_FLATTEN),
    ("batch shrink", BATCH_SHRINK),
    ("batch ocr", BATCH_OCR),
    ("batch seal", BATCH_SEAL),
    ("batch slip", BATCH_SLIP),
    ("batch compare", BATCH_COMPARE),
    ("batch crop", BATCH_CROP),
    ("batch print", BATCH_PRINT),
    ("batch summary", BATCH_SUMMARY),
    ("batch overlay", BATCH_OVERLAY),
    ("batch security", BATCH_SECURITY),
    ("cloud polygon", CLOUD_POLYGON),
    ("dimension", DIMENSION),
    ("redaction", REDACTION),
    ("space", SPACE),
    ("sketch", SKETCH),
    ("ellipse cutout", ELLIPSE_CUTOUT),
    ("area cutout", AREA_CUTOUT),
    ("show grid", SHOW_GRID),
    ("snap to grid", SNAP_TO_GRID),
    ("full screen", FULL_SCREEN),
    ("page single", PAGE_SINGLE),
    ("page continuous", PAGE_CONTINUOUS),
    ("page facing", PAGE_FACING),
    ("page facing continuous", PAGE_FACING_CONTINUOUS),
    ("view back", VIEW_BACK),
    ("view forward", VIEW_FORWARD),
    ("form field", FORM_FIELD),
    ("check box", CHECK_BOX),
    ("radio button", RADIO_BUTTON),
    ("list box", LIST_BOX),
    ("combo box", COMBO_BOX),
    ("push button", PUSH_BUTTON),
    ("form editor", FORM_EDITOR),
];

/// A letter, for a command whose icon has not been drawn yet. Honest about
/// being a placeholder rather than showing a wrong picture.
pub fn letter(painter: &egui::Painter, text: &str, into: Rect, colour: Color32) {
    let ch: String = text
        .rsplit('.')
        .next()
        .unwrap_or(text)
        .chars()
        .take(2)
        .collect();
    painter.text(
        into.center(),
        egui::Align2::CENTER_CENTER,
        ch,
        egui::FontId::proportional(into.height() * 0.5),
        colour,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_of(points: &[Vec<Pos2>]) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for run in points {
            for p in run {
                b.0 = b.0.min(p.x);
                b.1 = b.1.min(p.y);
                b.2 = b.2.max(p.x);
                b.3 = b.3.max(p.y);
            }
        }
        b
    }

    #[test]
    fn a_path_traces_into_the_rectangle_it_was_given() {
        let into = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(100.0));
        let runs = trace("M 0 0 L 100 100", into);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0][0], Pos2::new(10.0, 20.0));
        assert_eq!(runs[0][1], Pos2::new(110.0, 120.0));
    }

    #[test]
    fn a_second_move_starts_a_second_run() {
        let into = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let runs = trace("M 0 0 L 50 50 M 60 60 L 90 90", into);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].len(), 2);
    }

    #[test]
    fn a_curve_is_flattened_into_something_drawable() {
        let into = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let runs = trace("M 0 0 C 0 50 50 100 100 100", into);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].len() > 5, "a curve needs more than its endpoints");
        let last = *runs[0].last().unwrap();
        assert!((last.x - 100.0).abs() < 0.01 && (last.y - 100.0).abs() < 0.01);
    }

    #[test]
    fn every_glyph_stays_inside_its_box() {
        let into = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let all = CONTACT_SHEET;
        for (name, glyph) in all {
            let mut runs = Vec::new();
            for (d, _) in *glyph {
                runs.extend(trace(d, into));
            }
            assert!(!runs.is_empty(), "{name} drew nothing");
            let b = box_of(&runs);
            assert!(
                b.0 >= -6.0 && b.1 >= -6.0 && b.2 <= 106.0 && b.3 <= 106.0,
                "{name} spills out of its box: {b:?}"
            );
            // And it should fill enough of the box to look like a sibling of
            // the icons beside it. Some shapes are honestly wide and flat — a
            // ruler, a squiggle — so only the longer side has to reach.
            let (wide, tall) = (b.2 - b.0, b.3 - b.1);
            assert!(
                wide.max(tall) > 55.0 && wide.min(tall) > 12.0,
                "{name} is too small for its box: {wide} by {tall}"
            );
        }
    }

    #[test]
    fn a_line_gets_round_ends_and_a_box_gets_round_corners() {
        let into = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let line = &trace("M 10 10 L 90 90", into)[0];
        assert_eq!(round_points(line), vec![line[0], line[1]]);

        let square = &trace("M 10 10 L 90 10 L 90 90 L 10 90 Z", into)[0];
        let rounds = round_points(square);
        assert_eq!(rounds.len(), 4, "one for each corner, the first counted once: {rounds:?}");
    }

    #[test]
    fn the_gentle_bends_of_a_curve_are_left_alone() {
        let into = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let arc = &trace("M 14 82 C 14 30 50 14 86 18", into)[0];
        assert!(arc.len() > 10);
        assert_eq!(round_points(arc).len(), 2, "only its two ends");
    }

    fn luminance(c: Color32) -> f32 {
        let f = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn a_glyph_on_a_tile_can_be_read_in_both_palettes() {
        let dark_bar = Color32::from_rgb(28, 37, 50);
        let light_bar = Color32::from_rgb(246, 248, 250);
        let navy = Color32::from_rgb(43, 68, 97);
        for bar in [dark_bar, light_bar] {
            for look in [Tile::Rest, Tile::Hover, Tile::Pressed, Tile::Active, Tile::Chosen] {
                let (top, bottom) = tile_colours(look, bar);
                let glyph = glyph_on_tile(look, bar, navy, Color32::GRAY);
                // 3:1 is the contrast asked of icons and other things that
                // are not text; every tile clears it top and bottom.
                for face in [top, bottom] {
                    assert!(
                        contrast(glyph, face) >= 3.0,
                        "{look:?} on {bar:?}: {glyph:?} against {face:?} is only {:.1}:1",
                        contrast(glyph, face)
                    );
                }
            }
        }
    }

    #[test]
    fn a_resting_tile_is_quieter_than_one_under_the_pointer() {
        let bar = Color32::from_rgb(28, 37, 50);
        let (rest, _) = tile_colours(Tile::Rest, bar);
        let (hover, _) = tile_colours(Tile::Hover, bar);
        assert!(contrast(rest, bar) < contrast(hover, bar));
        assert!(contrast(rest, bar) > 1.3, "but it still shows as a tile");
    }

    #[test]
    fn rubbish_in_a_path_does_not_panic() {
        let into = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let _ = trace("M oops L 10", into);
        let _ = trace("", into);
        let _ = trace("Z Z Z", into);
    }
}
