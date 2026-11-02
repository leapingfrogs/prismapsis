//! Art for the heads-up display, painted offline (`just art`) in white and
//! greys and tinted in the game: a glass tube that the energy bar and the
//! spectrum gauges fill with light.

use super::paint::Canvas;
use bevy::math::Vec2;

/// The empty glass tube, drawn as a nine-slice so it fits any length.
pub const TUBE_PATH: &str = "art/hud_tube.png";
/// The light filling a tube, stretched to fit.
pub const FILL_PATH: &str = "art/hud_fill.png";

/// The tube's size in pixels, and the border kept unstretched round it.
pub const TUBE_PX: (u32, u32) = (64, 32);
pub const TUBE_BORDER_PX: f32 = 15.0;

/// Pixels down the fill's profile.
const FILL_PX: u32 = 32;

/// Pixels along the fill, which is the same all the way along. UI images
/// need some width to draw.
const FILL_WIDTH_PX: u32 = 8;

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of a pixel lies inside, from its signed distance to the edge
/// in pixels (negative inside).
fn cover(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Signed distance to a capsule filling a `width` × `height` box.
fn capsule(p: Vec2, width: f32, height: f32) -> f32 {
    let radius = height / 2.0;
    let x = p.x.clamp(radius, width - radius);
    p.distance(Vec2::new(x, radius)) - radius
}

/// A glass tube lying on its side: a lit rim round dark, faintly clear
/// glass, with a highlight along the top and a reflection low down.
#[must_use]
pub fn tube() -> Canvas {
    let (width, height) = TUBE_PX;
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (width as f32, height as f32);
    let mut canvas = Canvas::new(width, height);
    for y in 0..height {
        for x in 0..width {
            #[allow(clippy::cast_precision_loss)]
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let distance = capsule(p, w, h);
            let outside = cover(distance);
            if outside <= 0.0 {
                continue;
            }
            // The rim: bright on top, dimmer below, as lit from above.
            let rim = 1.0 - smoothstep(1.5, 3.0, -distance);
            let rim_value = 0.55 + 0.45 * (1.0 - p.y / h);
            // The glass: dark and mostly see-through, so the fill shows.
            let mut value: f32 = 0.1;
            let mut alpha: f32 = 0.3;
            // A highlight just inside the top of the glass.
            let top = 1.0 - smoothstep(0.0, 2.0, (p.y - h * 0.22).abs());
            let highlight = top * (1.0 - smoothstep(-8.0, -4.0, distance));
            value += highlight * 0.85;
            alpha = alpha.max(highlight * 0.7);
            if rim > 0.0 {
                value = value + (rim_value - value) * rim;
                alpha = alpha + (1.0 - alpha) * rim;
            }
            canvas.set(x, y, [value, value, value, alpha * outside]);
        }
    }
    canvas
}

/// The light in a tube, across it: bright through the middle and a little
/// dimmer at the edges, like liquid light, stretched along the tube.
#[must_use]
pub fn fill() -> Canvas {
    let mut canvas = Canvas::new(FILL_WIDTH_PX, FILL_PX);
    #[allow(clippy::cast_precision_loss)]
    let half = FILL_PX as f32 / 2.0;
    for y in 0..FILL_PX {
        #[allow(clippy::cast_precision_loss)]
        let t = ((y as f32 + 0.5) - half).abs() / half;
        let value = 1.0 - 0.35 * t * t;
        for x in 0..FILL_WIDTH_PX {
            canvas.set(x, y, [value, value, value, 1.0]);
        }
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_images_are_up_to_date() {
        for (path, canvas) in [(TUBE_PATH, tube()), (FILL_PATH, fill())] {
            let path = format!("{}/assets/{path}", env!("CARGO_MANIFEST_DIR"));
            let committed = std::fs::read(&path).unwrap_or_default();
            let painted = canvas.png().unwrap_or_default();
            assert!(
                committed == painted,
                "{path} is out of date; run `just art`"
            );
        }
    }

    #[test]
    fn the_tube_is_rounded_with_a_lit_rim_and_clear_glass() {
        let tube = tube();
        let (width, height) = TUBE_PX;
        // Rounded ends: the corners are empty.
        assert!(tube.get(0, 0)[3] < 0.01);
        // The rim along the top is solid and bright.
        let rim = tube.get(width / 2, 1);
        assert!(rim[3] > 0.9 && rim[0] > 0.8);
        // The middle is see-through, so the fill shows.
        assert!(tube.get(width / 2, height * 2 / 3)[3] < 0.6);
    }
}
