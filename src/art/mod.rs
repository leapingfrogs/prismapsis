//! Generated art, painted offline by `just art`: themed tilesets drawn with
//! a filter that holds back each hue until it has been restored, and a grey
//! sprite sheet tinted in the game.
//!
//! Walls are drawn on a dual grid: each tile sits on a corner where four
//! level cells meet and shows which of them are solid, so 16 tiles cover
//! every edge and corner.

pub mod backdrops;
pub mod features;
pub mod figures;
pub mod glow;
pub mod hud;
pub mod icon;
pub mod menu;
pub mod paint;
pub mod phantoms;
pub mod sprites;
pub mod tiles;
pub mod touch;

/// Tile size in pixels: 2× the 40-unit cell, to stay sharp at 4K.
pub const TILE_PX: u32 = 80;

/// Extra solid-interior tiles, so long walls don't visibly repeat.
pub const VARIANTS: u16 = 3;

/// Layers in a wall tileset: one per corner mask, then the variants.
pub const LAYERS: u16 = 16 + VARIANTS;

/// Which of a tile's four cells are solid, as mask bits.
pub const TOP_LEFT: u8 = 1;
pub const TOP_RIGHT: u8 = 2;
pub const BOTTOM_LEFT: u8 = 4;
pub const BOTTOM_RIGHT: u8 = 8;
pub const SOLID: u8 = 15;

/// The tileset layer for a corner mask: a fully solid tile uses `variant`
/// (0 for the plain one, up to `VARIANTS`).
#[must_use]
pub fn layer(mask: u8, variant: u16) -> u16 {
    if mask == SOLID && variant > 0 {
        15 + variant.min(VARIANTS)
    } else {
        u16::from(mask & SOLID)
    }
}

/// Which solid tile to use at a corner: mostly plain, with variants
/// scattered by a hash of the position so the pattern doesn't repeat.
#[must_use]
pub fn variant_at(x: i32, y: i32) -> u16 {
    let hash = mix((u64::from(x.cast_unsigned()) << 32) | u64::from(y.cast_unsigned()));
    // Roughly one tile in four is a variant.
    let roll = hash % 12;
    if roll < u64::from(VARIANTS) {
        u16::try_from(roll).map_or(0, |r| r + 1)
    } else {
        0
    }
}

/// Scramble the bits of `z` (the `SplitMix64` finaliser).
#[must_use]
pub const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// How much colour a hue may show, given how far each primary has been
/// restored (`wash`, 0 to 1).
///
/// Pure red follows red; yellow follows the less restored of red and green;
/// hues in between blend. Matches the shader.
#[must_use]
pub fn allowance(hue_degrees: f32, wash: [f32; 3]) -> f32 {
    let [red, green, blue] = wash;
    let anchors = [
        red,
        red.min(green),
        green,
        green.min(blue),
        blue,
        blue.min(red),
        red,
    ];
    let x = hue_degrees.rem_euclid(360.0) / 60.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = (x.floor() as usize).min(5);
    #[allow(clippy::cast_precision_loss)]
    let t = x - i as f32;
    anchors[i] + (anchors[i + 1] - anchors[i]) * t
}

/// The hue of a colour in degrees (0 red, 120 green, 240 blue).
#[must_use]
pub fn hue_degrees([r, g, b]: [f32; 3]) -> f32 {
    let max = r.max(g).max(b);
    let range = max - r.min(g).min(b);
    if range <= f32::EPSILON {
        return 0.0;
    }
    let sector = if (max - r).abs() <= f32::EPSILON {
        ((g - b) / range).rem_euclid(6.0)
    } else if (max - g).abs() <= f32::EPSILON {
        (b - r) / range + 2.0
    } else {
        (r - g) / range + 4.0
    };
    sector * 60.0
}

/// How a cell's hue tints the theme's art, as a linear multiplier: towards
/// the hue's colour by `strength`, or untouched without a hue.
#[must_use]
pub fn tint(hue: Option<crate::light::Hue>, strength: f32) -> [f32; 3] {
    hue.map_or([1.0; 3], |hue| {
        let colour = crate::spectrum_view::hue_colour(hue).to_linear();
        [colour.red, colour.green, colour.blue].map(|c| 1.0 + (c - 1.0) * strength)
    })
}

/// Relative luminance weights for linear RGB.
pub const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// A linear colour with its unrestored hues pulled towards grey, as the
/// shader draws it.
#[must_use]
pub fn filtered(colour: [f32; 3], wash: [f32; 3]) -> [f32; 3] {
    let grey = colour[0] * LUMA[0] + colour[1] * LUMA[1] + colour[2] * LUMA[2];
    let keep = allowance(hue_degrees(colour), wash);
    colour.map(|c| grey + (c - grey) * keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn layers_cover_every_mask_and_variant() {
        let mut seen: Vec<u16> = (0..16).map(|m| layer(m, 0)).collect();
        seen.extend((1..=VARIANTS).map(|v| layer(SOLID, v)));
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, (0..LAYERS).collect::<Vec<_>>());
        // Variants only apply to solid tiles.
        assert_eq!(layer(TOP_LEFT, 2), u16::from(TOP_LEFT));
    }

    #[test]
    fn variants_are_scattered_but_fixed() {
        let picks: Vec<u16> = (0..200).map(|x| variant_at(x, 7)).collect();
        assert_eq!(
            picks,
            (0..200).map(|x| variant_at(x, 7)).collect::<Vec<_>>()
        );
        let plain = picks.iter().filter(|&&v| v == 0).count();
        assert!((100..190).contains(&plain), "{plain} plain of 200");
        assert!(picks.iter().all(|&v| v <= VARIANTS));
        assert!((1..=VARIANTS).all(|v| picks.contains(&v)));
    }

    #[test]
    fn hues_are_measured_round_the_wheel() {
        assert!(close(hue_degrees([1.0, 0.0, 0.0]), 0.0));
        assert!(close(hue_degrees([1.0, 1.0, 0.0]), 60.0));
        assert!(close(hue_degrees([0.0, 1.0, 0.0]), 120.0));
        assert!(close(hue_degrees([0.0, 1.0, 1.0]), 180.0));
        assert!(close(hue_degrees([0.0, 0.0, 1.0]), 240.0));
        assert!(close(hue_degrees([1.0, 0.0, 1.0]), 300.0));
        assert!(close(hue_degrees([0.5, 0.5, 0.5]), 0.0));
    }

    #[test]
    fn secondaries_wait_for_both_primaries() {
        let red = [1.0, 0.0, 0.0];
        assert!(close(allowance(0.0, red), 1.0));
        assert!(close(allowance(60.0, red), 0.0), "yellow needs green too");
        assert!(close(allowance(30.0, red), 0.5), "orange is half way");
        assert!(close(allowance(60.0, [1.0, 1.0, 0.0]), 1.0));
        assert!(close(allowance(300.0, [1.0, 0.0, 1.0]), 1.0));
        assert!(close(allowance(359.0, [0.0; 3]), 0.0));
    }

    #[test]
    fn filter_greys_what_is_not_restored() {
        let green = [0.1, 0.8, 0.1];
        let grey = filtered(green, [0.0; 3]);
        assert!(close(grey[0], grey[1]) && close(grey[1], grey[2]));
        for wash in [[0.0, 1.0, 0.0], [1.0; 3]] {
            let shown = filtered(green, wash);
            assert!((0..3).all(|i| close(shown[i], green[i])), "{shown:?}");
        }
        // Restoring red alone leaves green grey.
        let still = filtered(green, [1.0, 0.0, 0.0]);
        assert!(close(still[0], still[1]));
    }
}
