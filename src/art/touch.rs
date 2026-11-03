//! The touch buttons' symbols, painted offline (`just art`) in white and
//! tinted in the game. Exo 2 has no arrows, stars or locks, so they are
//! drawn here rather than set as text.

use super::paint::Canvas;
use bevy::math::{Rect, Vec2};

/// The symbols, side by side in one strip.
pub const GLYPHS_PATH: &str = "art/touch_glyphs.png";

/// Each symbol's square cell, in pixels: sharp on the biggest button at 4K.
pub const GLYPH_PX: u32 = 128;

/// Half the width of a stroke, as a fraction of half a cell.
const STROKE: f32 = 0.11;

/// How far the open lock's shackle is raised, as a fraction of half a cell.
const LIFT: f32 = 0.2;

/// A symbol on a touch button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Left,
    Right,
    Up,
    Down,
    /// A burst, for fire.
    Fire,
    /// Two bars.
    Pause,
    /// An open lock, for hold while the prism isn't locked.
    Unlocked,
    /// A closed lock, for hold while it is.
    Locked,
    /// A ring with rays all round, for the light nova.
    Nova,
}

impl Glyph {
    pub const ALL: [Self; 9] = [
        Self::Left,
        Self::Right,
        Self::Up,
        Self::Down,
        Self::Fire,
        Self::Pause,
        Self::Unlocked,
        Self::Locked,
        Self::Nova,
    ];

    /// Its place in the strip.
    fn index(self) -> u32 {
        Self::ALL
            .iter()
            .position(|&glyph| glyph == self)
            .map_or(0, |i| u32::try_from(i).unwrap_or(0))
    }

    /// Where it sits in the strip, in pixels from the top left.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn rect(self) -> Rect {
        let corner = Vec2::new((self.index() * GLYPH_PX) as f32, 0.0);
        Rect::from_corners(corner, corner + Vec2::splat(GLYPH_PX as f32))
    }

    /// Signed distance to its shape at `p`, with the cell running from -1
    /// to 1 each way and y down.
    fn distance(self, p: Vec2) -> f32 {
        match self {
            Self::Right => arrow(p),
            Self::Left => arrow(Vec2::new(-p.x, p.y)),
            Self::Up => arrow(Vec2::new(-p.y, p.x)),
            Self::Down => arrow(Vec2::new(p.y, -p.x)),
            Self::Fire => burst(p),
            Self::Pause => bars(p),
            Self::Unlocked => lock(p, LIFT),
            Self::Locked => lock(p, 0.0),
            Self::Nova => nova(p),
        }
    }
}

/// How much of a pixel lies inside, from its signed distance to the edge
/// in pixels (negative inside).
fn cover(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Distance from `p` to the segment from `a` to `b`.
fn segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let along = (p - a).dot(b - a) / (b - a).length_squared();
    (p - a.lerp(b, along.clamp(0.0, 1.0))).length()
}

/// Signed distance to a box centred on `centre` with half its size `half`
/// and corners rounded by `radius`.
fn rounded_box(p: Vec2, centre: Vec2, half: Vec2, radius: f32) -> f32 {
    let q = (p - centre).abs() - half + Vec2::splat(radius);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - radius
}

/// An arrow pointing right: a shaft and two strokes back from the tip.
fn arrow(p: Vec2) -> f32 {
    let tip = Vec2::new(0.6, 0.0);
    [
        segment(p, Vec2::new(-0.6, 0.0), tip),
        segment(p, tip, Vec2::new(0.15, -0.45)),
        segment(p, tip, Vec2::new(0.15, 0.45)),
    ]
    .into_iter()
    .fold(f32::MAX, f32::min)
        - STROKE
}

/// A burst: eight rays round a bright middle, long and short in turn.
fn burst(p: Vec2) -> f32 {
    let rays = (0..8u8)
        .map(|k| {
            let direction = Vec2::from_angle(f32::from(k) * std::f32::consts::FRAC_PI_4);
            let length = if k % 2 == 0 { 0.85 } else { 0.6 };
            segment(p, direction * 0.3, direction * length) - STROKE * 0.8
        })
        .fold(f32::MAX, f32::min);
    rays.min(p.length() - 0.22)
}

/// A ring with rays all round it, long and short in turn.
fn nova(p: Vec2) -> f32 {
    let ring = (p.length() - 0.3).abs() - STROKE * 0.8;
    let rays = (0..8u8)
        .map(|k| {
            let direction = Vec2::from_angle(f32::from(k) * std::f32::consts::FRAC_PI_4);
            let length = if k % 2 == 0 { 0.84 } else { 0.7 };
            segment(p, direction * 0.52, direction * length) - STROKE * 0.7
        })
        .fold(f32::MAX, f32::min);
    ring.min(rays)
}

/// Two upright bars.
fn bars(p: Vec2) -> f32 {
    let half = Vec2::new(0.14, 0.6);
    rounded_box(p, Vec2::new(-0.3, 0.0), half, 0.06).min(rounded_box(
        p,
        Vec2::new(0.3, 0.0),
        half,
        0.06,
    ))
}

/// A padlock with a keyhole, its shackle raised by `lift`: closed at
/// none, and open, with its left leg out of the body, when raised.
fn lock(p: Vec2, lift: f32) -> f32 {
    let body = rounded_box(p, Vec2::new(0.0, 0.3), Vec2::new(0.55, 0.38), 0.1);
    let keyhole = ((p - Vec2::new(0.0, 0.22)).length() - 0.1)
        .min(segment(p, Vec2::new(0.0, 0.22), Vec2::new(0.0, 0.45)) - 0.045);
    let body = body.max(-keyhole);
    // The shackle: a half circle on two legs, the left one short when open.
    let radius = 0.3;
    let centre = Vec2::new(0.0, -0.3 - lift);
    let arc = if p.y <= centre.y {
        ((p - centre).length() - radius).abs()
    } else {
        f32::MAX
    };
    let foot = if lift > 0.0 { centre.y + 0.15 } else { 0.0 };
    let left_leg = segment(p, centre - Vec2::new(radius, 0.0), Vec2::new(-radius, foot));
    let right_leg = segment(p, centre + Vec2::new(radius, 0.0), Vec2::new(radius, 0.0));
    let shackle = arc.min(left_leg).min(right_leg) - STROKE * 0.8;
    body.min(shackle)
}

/// Every symbol in white, side by side.
#[must_use]
pub fn glyphs() -> Canvas {
    let count = u32::try_from(Glyph::ALL.len()).unwrap_or(0);
    let mut canvas = Canvas::new(GLYPH_PX * count, GLYPH_PX);
    #[allow(clippy::cast_precision_loss)]
    let half = GLYPH_PX as f32 / 2.0;
    for glyph in Glyph::ALL {
        let left = glyph.index() * GLYPH_PX;
        for y in 0..GLYPH_PX {
            for x in 0..GLYPH_PX {
                #[allow(clippy::cast_precision_loss)]
                let p = (Vec2::new(x as f32, y as f32) + 0.5 - half) / half;
                let alpha = cover(glyph.distance(p) * half);
                if alpha > 0.0 {
                    canvas.set(left + x, y, [1.0, 1.0, 1.0, alpha]);
                }
            }
        }
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_image_is_up_to_date() {
        let path = format!("{}/assets/{GLYPHS_PATH}", env!("CARGO_MANIFEST_DIR"));
        let committed = std::fs::read(&path).unwrap_or_default();
        let painted = glyphs().png().unwrap_or_default();
        assert!(
            committed == painted,
            "{path} is out of date; run `just art`"
        );
    }

    /// The alpha at a point in a glyph's cell, given from -1 to 1.
    fn alpha(canvas: &Canvas, glyph: Glyph, x: f32, y: f32) -> f32 {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let at = |v: f32| (f32::midpoint(v, 1.0) * GLYPH_PX as f32) as u32;
        canvas.get(glyph.index() * GLYPH_PX + at(x), at(y))[3]
    }

    #[test]
    fn arrows_point_their_way_and_locks_open_and_close() {
        let canvas = glyphs();
        // Each arrow's head has strokes back from its tip on both sides.
        assert!(alpha(&canvas, Glyph::Right, 0.2, -0.4) > 0.5);
        assert!(alpha(&canvas, Glyph::Right, -0.2, -0.4) < 0.01);
        assert!(alpha(&canvas, Glyph::Left, -0.2, -0.4) > 0.5);
        assert!(alpha(&canvas, Glyph::Up, -0.4, -0.2) > 0.5);
        assert!(alpha(&canvas, Glyph::Up, -0.4, 0.2) < 0.01);
        assert!(alpha(&canvas, Glyph::Down, -0.4, 0.2) > 0.5);
        // Pause has a gap between its bars.
        assert!(alpha(&canvas, Glyph::Pause, 0.0, 0.0) < 0.01);
        assert!(alpha(&canvas, Glyph::Pause, 0.3, 0.0) > 0.99);
        // The open lock's left leg stops short of the body.
        assert!(alpha(&canvas, Glyph::Locked, -0.3, -0.15) > 0.5);
        assert!(alpha(&canvas, Glyph::Unlocked, -0.3, -0.15) < 0.01);
        // A ray from the burst.
        assert!(alpha(&canvas, Glyph::Fire, 0.7, 0.0) > 0.5);
        // Nothing reaches the cells' edges, so neighbours don't bleed.
        for glyph in Glyph::ALL {
            for i in 0..GLYPH_PX {
                let x = glyph.index() * GLYPH_PX;
                assert!(canvas.get(x, i)[3] < 0.01, "{glyph:?}");
                assert!(canvas.get(x + i, 0)[3] < 0.01, "{glyph:?}");
                assert!(canvas.get(x + GLYPH_PX - 1, i)[3] < 0.01, "{glyph:?}");
                assert!(canvas.get(x + i, GLYPH_PX - 1)[3] < 0.01, "{glyph:?}");
            }
        }
    }
}
