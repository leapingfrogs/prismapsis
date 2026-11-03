//! Art for the title and menus, painted offline (`just art`): the logo's
//! emblem, a beam of white light split by a glass prism into colour, and
//! the glass panel menus sit on.
//!
//! The emblem is painted in its own colours, as it shows the colour the
//! game is about; the panel is white and greys, tinted in the game.

use super::paint::Canvas;
use bevy::math::Vec2;

/// The emblem above the title.
pub const EMBLEM_PATH: &str = "art/logo_emblem.png";
/// The glass panel behind menus, drawn as a nine-slice.
pub const PANEL_PATH: &str = "art/menu_panel.png";

/// The emblem's size in pixels.
pub const EMBLEM_PX: (u32, u32) = (384, 160);

/// The panel's size in pixels, and the border kept unstretched round it.
pub const PANEL_PX: u32 = 96;
pub const PANEL_BORDER_PX: f32 = 32.0;

/// The colours the prism splits the light into, top to bottom.
pub(super) const RAYS: [[f32; 3]; 3] = [[1.0, 0.32, 0.3], [0.35, 1.0, 0.45], [0.35, 0.55, 1.0]];

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of a pixel lies inside, from its signed distance to the edge
/// in pixels (negative inside).
pub(super) fn cover(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Distance from `p` to the segment from `a` to `b`.
pub(super) fn segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// `top` laid over `under`, both straight-alpha colour.
pub(super) fn over(top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
    let alpha = top[3] + under[3] * (1.0 - top[3]);
    if alpha <= f32::EPSILON {
        return [0.0; 4];
    }
    let mix = |i: usize| (top[i] * top[3] + under[i] * under[3] * (1.0 - top[3])) / alpha;
    [mix(0), mix(1), mix(2), alpha]
}

/// A soft beam of `colour` along the segment from `a` to `b`: a bright
/// core in a glow, fading towards `b` if `fade`.
fn beam(p: Vec2, from: Vec2, to: Vec2, colour: [f32; 3], fade: bool) -> [f32; 4] {
    let distance = segment(p, from, to);
    let along = ((p - from).dot(to - from) / (to - from).length_squared()).clamp(0.0, 1.0);
    let strength = if fade { 1.0 - along.powf(1.5) } else { 1.0 };
    let core = cover(distance - 2.0);
    let glow = (-(distance / 7.0).powi(2)).exp() * 0.6;
    let alpha = core.max(glow) * strength;
    let white = core * 0.6;
    let channel = |i: usize| colour[i] + (1.0 - colour[i]) * white;
    [channel(0), channel(1), channel(2), alpha]
}

/// The emblem: white light from the left meets a glass prism and leaves it
/// to the right as red, green and blue rays fanning out and fading.
#[must_use]
pub fn emblem() -> Canvas {
    let (width, height) = EMBLEM_PX;
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (width as f32, height as f32);
    // The prism: a triangle point up, a little left of centre.
    let apex = Vec2::new(w * 0.44, h * 0.12);
    let base_left = Vec2::new(w * 0.31, h * 0.86);
    let base_right = Vec2::new(w * 0.57, h * 0.86);
    let corners = [apex, base_right, base_left];
    // Where the white beam enters and the colours leave.
    let enter = apex.lerp(base_left, 0.55);
    let leave = apex.lerp(base_right, 0.5);
    let mut canvas = Canvas::new(width, height);
    for y in 0..height {
        for x in 0..width {
            #[allow(clippy::cast_precision_loss)]
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let mut pixel = beam(p, Vec2::new(4.0, enter.y + 6.0), enter, [1.0; 3], false);
            for (i, colour) in RAYS.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let spread = (i as f32 - 1.0) * 0.14 + 0.06;
                let end = leave + Vec2::new(w * 0.42, h * spread * 2.2);
                pixel = over(beam(p, leave, end, *colour, true), pixel);
            }
            // Inside the prism: faint glass with the light passing through.
            let inside = corners
                .iter()
                .zip(corners.iter().cycle().skip(1))
                .map(|(&a, &b)| {
                    let edge = b - a;
                    (p - a).perp_dot(edge) / edge.length()
                })
                .fold(f32::MIN, f32::max);
            if inside < 0.5 {
                let glass = [0.8, 0.9, 1.0, 0.18 * cover(inside)];
                let path = 1.0 - smoothstep(0.0, 6.0, segment(p, enter, leave));
                pixel = over(
                    [1.0, 1.0, 1.0, path * 0.7 * cover(inside)],
                    over(glass, pixel),
                );
            }
            // The prism's rim, brightest on its upper left.
            let rim = corners
                .iter()
                .zip(corners.iter().cycle().skip(1))
                .map(|(&a, &b)| segment(p, a, b))
                .fold(f32::MAX, f32::min);
            let lit = 0.75 + 0.25 * (1.0 - p.x / w);
            pixel = over([lit, lit, 1.0, cover(rim - 1.6)], pixel);
            canvas.set(x, y, pixel);
        }
    }
    canvas
}

/// The menu panel: dark, faintly clear glass with rounded corners, a lit
/// rim brightest along the top, and a fine highlight just inside it.
#[must_use]
pub fn panel() -> Canvas {
    #[allow(clippy::cast_precision_loss)]
    let size = PANEL_PX as f32;
    let radius = 22.0;
    let mut canvas = Canvas::new(PANEL_PX, PANEL_PX);
    for y in 0..PANEL_PX {
        for x in 0..PANEL_PX {
            #[allow(clippy::cast_precision_loss)]
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let corner = Vec2::new(
                p.x.clamp(radius, size - radius),
                p.y.clamp(radius, size - radius),
            );
            let inset = radius - p.distance(corner);
            if inset < -0.5 {
                continue;
            }
            let outside = cover(-inset);
            let rim = 1.0 - smoothstep(1.0, 2.5, inset);
            let highlight = 1.0 - smoothstep(0.6, 1.4, (inset - 5.0).abs());
            let top = 1.0 - p.y / size;
            let value = 0.08 + rim * (0.5 + 0.45 * top) + highlight * 0.25 * top;
            let alpha = (0.62 + rim * 0.38 + highlight * 0.2).min(1.0) * outside;
            canvas.set(x, y, [value, value, value + 0.03, alpha]);
        }
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_images_are_up_to_date() {
        for (path, canvas) in [(EMBLEM_PATH, emblem()), (PANEL_PATH, panel())] {
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
    fn the_emblem_splits_white_into_colours() {
        let emblem = emblem();
        let (width, height) = EMBLEM_PX;
        // White light on the left.
        let left: Vec<[f32; 4]> = (0..height).map(|y| emblem.get(20, y)).collect();
        assert!(left.iter().any(|p| p[3] > 0.5 && p[0] > 0.9 && p[2] > 0.9));
        // Some red and some blue to the right.
        let column: Vec<[f32; 4]> = (0..height).map(|y| emblem.get(width * 3 / 4, y)).collect();
        assert!(column.iter().any(|p| p[3] > 0.3 && p[0] > p[2] + 0.3));
        assert!(column.iter().any(|p| p[3] > 0.3 && p[2] > p[0] + 0.3));
    }

    #[test]
    fn the_panel_is_rounded_with_a_lit_rim() {
        let panel = panel();
        assert!(panel.get(0, 0)[3] < 0.01, "rounded corner");
        let mid = PANEL_PX / 2;
        assert!(
            panel.get(mid, 1)[0] > panel.get(mid, PANEL_PX - 2)[0],
            "lit on top"
        );
        let glass = panel.get(mid, mid);
        assert!(glass[3] > 0.5 && glass[0] < 0.2, "dark glass inside");
    }
}
