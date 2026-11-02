//! Art for the light the player works with, painted offline (`just art`)
//! in white and greys and tinted in the game: the beam's glow and the exit,
//! a doorway that fills with light once white is restored.

use super::paint::Canvas;
use bevy::math::Vec2;

/// The beam's profile: a strip across the beam, stretched along it.
pub const BEAM_PATH: &str = "art/beam.png";
/// The exit's frame, drawn as a nine-slice so it fits any doorway.
pub const EXIT_FRAME_PATH: &str = "art/exit_frame.png";
/// The light filling the exit's doorway, stretched to fit.
pub const EXIT_LIGHT_PATH: &str = "art/exit_light.png";

/// Pixels across the beam's profile.
pub const BEAM_PX: u32 = 32;

/// The exit frame's size in pixels, and the border kept unstretched round
/// it when sliced.
pub const FRAME_PX: u32 = 64;
pub const FRAME_BORDER_PX: f32 = 24.0;

/// How thick the frame's posts and lintel are, in pixels (half that in
/// world units).
pub const FRAME_THICKNESS_PX: f32 = 16.0;

/// The light's size in pixels.
const LIGHT_PX: (u32, u32) = (32, 64);

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of a pixel lies inside, from its signed distance to the edge
/// in pixels (negative inside).
fn cover(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Paint every pixel of a `width` × `height` canvas from its centre.
fn paint(width: u32, height: u32, pixel: impl Fn(f32, f32) -> [f32; 4]) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    for y in 0..height {
        for x in 0..width {
            #[allow(clippy::cast_precision_loss)]
            canvas.set(x, y, pixel(x as f32 + 0.5, y as f32 + 0.5));
        }
    }
    canvas
}

/// Across the beam: a hot core that fades into a soft glow. One pixel
/// along, as it's stretched to the beam's length.
#[must_use]
pub fn beam() -> Canvas {
    #[allow(clippy::cast_precision_loss)]
    let half = BEAM_PX as f32 / 2.0;
    paint(1, BEAM_PX, |_, y| {
        let t = (y - half).abs() / half;
        let core = 1.0 - smoothstep(0.1, 0.3, t);
        let glow = (1.0 - t).powi(3) * 0.45;
        [1.0, 1.0, 1.0, core.max(glow)]
    })
}

/// Signed distance to a box from `min` to `max` with its top corners
/// rounded by `radius` (y down, so the top is `min.y`).
fn arch(p: Vec2, min: Vec2, max: Vec2, radius: f32) -> f32 {
    let inside = (p.x - min.x)
        .min(max.x - p.x)
        .min(p.y - min.y)
        .min(max.y - p.y);
    let corner = Vec2::new(
        p.x.clamp(min.x + radius, max.x - radius),
        p.y.max(min.y + radius),
    );
    if p.y < min.y + radius && (p.x < min.x + radius || p.x > max.x - radius) {
        p.distance(corner) - radius
    } else {
        -inside
    }
}

/// The doorway's frame: two posts and a lintel of pale stone round an
/// arched opening, lit from the top left.
///
/// Its inner edge is bright where the light spills over it. The bottom is
/// open, as the doorway stands on the floor.
#[must_use]
pub fn exit_frame() -> Canvas {
    #[allow(clippy::cast_precision_loss)]
    let size = FRAME_PX as f32;
    let t = FRAME_THICKNESS_PX;
    paint(FRAME_PX, FRAME_PX, |x, y| {
        let p = Vec2::new(x, y);
        // Run past the bottom so neither shape closes there.
        let outer = arch(p, Vec2::ZERO, Vec2::new(size, size * 2.0), 14.0);
        let inner = arch(p, Vec2::splat(t), Vec2::new(size - t, size * 2.0), 8.0);
        let distance = outer.max(-inner);
        let alpha = cover(distance);
        if alpha <= 0.0 {
            return [0.0; 4];
        }
        let mut value = 0.62;
        // The outside is lit on top and to the left and shaded to the right;
        // the opening's edge is bright where the light spills over it.
        let near_outer = -outer < 3.0;
        let near_inner = inner < 3.0;
        if near_outer {
            value = if y < 10.0 || x < size / 2.0 { 0.9 } else { 0.4 };
        } else if near_inner {
            value = 1.0;
        }
        [value, value, value, alpha]
    })
}

/// The light in the doorway: even across, brightest along the floor and
/// fading upwards, with thin rays rising through it.
#[must_use]
pub fn exit_light() -> Canvas {
    let (width, height) = LIGHT_PX;
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (width as f32, height as f32);
    paint(width, height, |x, y| {
        let across = 1.0 - ((x / w - 0.5).abs() * 2.0).powi(8);
        let rise = 0.35 + 0.65 * (y / h).powf(1.5);
        let ray = (0.5 + 0.5 * (x / w * std::f32::consts::TAU * 4.0).cos()).powi(6);
        let alpha = (across * (rise + 0.3 * ray * (1.0 - rise))).clamp(0.0, 1.0);
        [1.0, 1.0, 1.0, alpha]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_images_are_up_to_date() {
        for (path, canvas) in [
            (BEAM_PATH, beam()),
            (EXIT_FRAME_PATH, exit_frame()),
            (EXIT_LIGHT_PATH, exit_light()),
        ] {
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
    fn the_beam_is_solid_in_the_middle_and_fades_out() {
        let beam = beam();
        assert!(beam.get(0, BEAM_PX / 2)[3] > 0.99);
        assert!(beam.get(0, 0)[3] < 0.05);
    }

    #[test]
    fn the_frame_is_open_in_the_middle_and_at_the_bottom() {
        let frame = exit_frame();
        let mid = FRAME_PX / 2;
        assert!(frame.get(mid, mid)[3] < 0.01, "opening");
        assert!(frame.get(mid, 4)[3] > 0.99, "lintel");
        assert!(frame.get(4, FRAME_PX - 1)[3] > 0.99, "post");
        assert!(frame.get(mid, FRAME_PX - 1)[3] < 0.01, "no sill");
    }
}
