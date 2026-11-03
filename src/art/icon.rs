//! The game's icon, painted offline (`just art`): the logo's prism on dark
//! glass, splitting a white beam into red, green and blue.
//!
//! Two versions: a square tile for the web, which home screens crop to
//! their own shape, and a rounded one with a lit rim for the desktop and the
//! browser tab. Strokes keep a minimum width, so the smallest sizes still
//! show a prism.

use super::menu::{RAYS, cover, over, segment};
use super::paint::Canvas;
use bevy::math::Vec2;

/// The square icon's sizes: the iPhone home screen, then Android's two.
pub const TILE_SIZES: [u32; 3] = [180, 192, 512];
/// The rounded icon's sizes, smallest first; all go in the Windows icon.
pub const ROUNDED_SIZES: [u32; 4] = [16, 32, 48, 256];
/// The Windows icon, which `build.rs` embeds in the program.
pub const ICO_PATH: &str = "icon/prismapsis.ico";

/// The rounded icon's corner radius, as a share of its size.
const CORNER: f32 = 0.2;

#[must_use]
pub fn tile_path(size: u32) -> String {
    format!("icon/tile_{size}.png")
}

#[must_use]
pub fn rounded_path(size: u32) -> String {
    format!("icon/rounded_{size}.png")
}

/// Every size of the icon, with where each is kept under `assets`.
#[must_use]
pub fn images() -> Vec<(String, Canvas)> {
    let tiles = TILE_SIZES
        .into_iter()
        .map(|size| (tile_path(size), icon(size, false)));
    let rounded = ROUNDED_SIZES
        .into_iter()
        .map(|size| (rounded_path(size), icon(size, true)));
    tiles.chain(rounded).collect()
}

/// A soft ray from `from` to `to`, keeping `tail` of its strength at `to`.
fn ray(p: Vec2, from: Vec2, to: Vec2, colour: [f32; 3], width: f32, tail: f32) -> [f32; 4] {
    let distance = segment(p, from, to);
    let along = ((p - from).dot(to - from) / (to - from).length_squared()).clamp(0.0, 1.0);
    let core = cover(distance - width);
    let glow = (-(distance / (width * 3.5)).powi(2)).exp() * 0.55;
    let alpha = core.max(glow) * (1.0 + (tail - 1.0) * along);
    let white = core * 0.15;
    let channel = |i: usize| colour[i] + (1.0 - colour[i]) * white;
    [channel(0), channel(1), channel(2), alpha]
}

/// How far `p` lies inside a rounded square of `size` (negative outside).
fn rounded_inset(p: Vec2, size: f32) -> f32 {
    let radius = size * CORNER;
    let corner = p.clamp(Vec2::splat(radius), Vec2::splat(size - radius));
    radius - p.distance(corner)
}

/// The icon at `size` pixels, `rounded` or filling the square.
#[must_use]
pub fn icon(size: u32, rounded: bool) -> Canvas {
    #[allow(clippy::cast_precision_loss)]
    let s = size as f32;
    let at = |x: f32, y: f32| Vec2::new(x, y) * s;
    // The prism, a little left of centre to leave room for the colours, and
    // within the middle 80% circle that home screens never crop.
    let apex = at(0.42, 0.2);
    let base_left = at(0.18, 0.72);
    let base_right = at(0.66, 0.72);
    let corners = [apex, base_right, base_left];
    let enter = apex.lerp(base_left, 0.55);
    let leave = apex.lerp(base_right, 0.5);
    let width = (s * 0.018).max(0.8);
    let rim_width = (s * 0.017).max(0.75);
    let mut canvas = Canvas::new(size, size);
    for y in 0..size {
        for x in 0..size {
            #[allow(clippy::cast_precision_loss)]
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            // Dark glass, lit from above and glowing faintly behind the prism.
            let glow = (-(p.distance(at(0.45, 0.55)) / (s * 0.42)).powi(2)).exp();
            let depth = 1.0 - p.y / s;
            let mut pixel = [
                0.04 + 0.05 * depth + 0.06 * glow,
                0.04 + 0.05 * depth + 0.07 * glow,
                0.09 + 0.1 * depth + 0.14 * glow,
                1.0,
            ];
            pixel = over(
                ray(
                    p,
                    at(-0.05, enter.y / s + 0.06),
                    enter,
                    [1.0; 3],
                    width,
                    1.0,
                ),
                pixel,
            );
            for (colour, drop) in RAYS.iter().zip([-0.14, 0.09, 0.32]) {
                let end = at(1.05, leave.y / s + drop);
                pixel = over(ray(p, leave, end, *colour, width, 0.55), pixel);
            }
            let edges = || corners.iter().zip(corners.iter().cycle().skip(1));
            let inside = edges()
                .map(|(&a, &b)| {
                    let edge = b - a;
                    (p - a).perp_dot(edge) / edge.length()
                })
                .fold(f32::MIN, f32::max);
            if inside < 0.5 {
                let glass = [0.75, 0.85, 1.0, 0.2 * cover(inside)];
                let path = (1.0 - segment(p, enter, leave) / (width * 2.5)).clamp(0.0, 1.0);
                pixel = over(
                    [1.0, 1.0, 1.0, path * 0.85 * cover(inside)],
                    over(glass, pixel),
                );
            }
            let rim = edges()
                .map(|(&a, &b)| segment(p, a, b))
                .fold(f32::MAX, f32::min);
            let lit = 0.78 + 0.22 * (1.0 - p.x / s);
            pixel = over([lit, lit, 1.0, cover(rim - rim_width)], pixel);
            if rounded {
                let inset = rounded_inset(p, s);
                let edge = (s * 0.012).max(0.6);
                let light =
                    (1.0 - (inset / edge - 0.5).abs()).clamp(0.0, 1.0) * (0.2 + 0.5 * depth);
                pixel = over([0.75, 0.8, 1.0, light], pixel);
                pixel[3] *= cover(-inset);
            }
            canvas.set(x, y, pixel);
        }
    }
    canvas
}

/// The rounded sizes as a Windows icon: a directory of PNG images.
///
/// # Errors
/// If an image can't be encoded.
#[allow(clippy::cast_possible_truncation)]
pub fn ico() -> Result<Vec<u8>, png::EncodingError> {
    const HEADER: usize = 6;
    const ENTRY: usize = 16;
    let images = ROUNDED_SIZES
        .into_iter()
        .map(|size| icon(size, true).png().map(|bytes| (size, bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut bytes = vec![0, 0, 1, 0];
    bytes.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = HEADER + ENTRY * images.len();
    for (size, image) in &images {
        // A size of 256 is written as 0.
        let side = (*size % 256) as u8;
        bytes.extend_from_slice(&[side, side, 0, 0]);
        // One colour plane, 32 bits per pixel.
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&32_u16.to_le_bytes());
        bytes.extend_from_slice(&(image.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += image.len();
    }
    for (_, image) in &images {
        bytes.extend_from_slice(image);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_images_are_up_to_date() {
        let assets = format!("{}/assets", env!("CARGO_MANIFEST_DIR"));
        for (path, canvas) in images() {
            let committed = std::fs::read(format!("{assets}/{path}")).unwrap_or_default();
            assert!(
                committed == canvas.png().unwrap_or_default(),
                "{path} is out of date; run `just art`"
            );
        }
        let committed = std::fs::read(format!("{assets}/{ICO_PATH}")).unwrap_or_default();
        assert!(
            committed == ico().unwrap_or_default(),
            "{ICO_PATH} is out of date; run `just art`"
        );
    }

    #[test]
    fn a_tile_fills_its_square_for_home_screens_to_crop() {
        let tile = icon(64, false);
        for (x, y) in [(0, 0), (63, 0), (0, 63), (63, 63)] {
            assert!(tile.get(x, y)[3] > 0.99, "opaque at {x}, {y}");
        }
    }

    #[test]
    fn the_rounded_icon_has_clear_corners() {
        let rounded = icon(64, true);
        assert!(rounded.get(0, 0)[3] < 0.01);
        assert!(rounded.get(32, 32)[3] > 0.99);
    }

    #[test]
    fn colours_leave_the_prism_even_at_the_smallest_size() {
        let small = icon(16, true);
        let column: Vec<[f32; 4]> = (0..16).map(|y| small.get(14, y)).collect();
        assert!(column.iter().any(|p| p[0] > p[2] + 0.2), "some red");
        assert!(column.iter().any(|p| p[2] > p[0] + 0.2), "some blue");
    }

    #[test]
    fn the_windows_icon_lists_each_size() {
        let bytes = ico().unwrap_or_default();
        assert_eq!(&bytes[..4], &[0, 0, 1, 0]);
        assert_eq!(usize::from(bytes[4]), ROUNDED_SIZES.len());
        let entry = |i: usize| &bytes[6 + 16 * i..6 + 16 * (i + 1)];
        assert_eq!(entry(0)[0], 16);
        assert_eq!(entry(3)[0], 0, "256 is written as 0");
        let offset = |i: usize| {
            let e = entry(i);
            u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize
        };
        // PNG's signature starts every image.
        assert_eq!(&bytes[offset(3) + 1..offset(3) + 4], b"PNG");
    }
}
