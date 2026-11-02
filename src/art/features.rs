//! Art for walls with traits: one-way platforms, springs, mud and mirrors,
//! painted offline (`just art`) into a tileset per theme.
//!
//! Features are drawn one quad per cell rather than on the dual grid, as
//! platforms and pads are thinner than a cell: each quad covers the part of
//! its cell the feature fills, from the top down. A cell's tile depends on
//! which of its four neighbours hold the same feature, so ends and edges
//! are drawn where a run stops.

use super::paint::{Canvas, Rgb, Theme};
use super::{TILE_PX, tint};
use crate::light::Hue;
use crate::physics::{Aabb, Traits};
use bevy::asset::RenderAssetUsages;
use bevy::math::Vec2;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use std::f32::consts::TAU;

/// A kind of feature, each with its own 16 tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Platform,
    Spring,
    Mud,
    Mirror,
    /// A ledge that gives way after a few landings.
    Crumbling,
}

/// Neighbours holding the same feature, as mask bits.
pub const ABOVE: u8 = 1;
pub const BELOW: u8 = 2;
pub const LEFT: u8 = 4;
pub const RIGHT: u8 = 8;

/// Layers in a features tileset: 16 neighbour masks per feature.
#[allow(clippy::cast_possible_truncation)]
pub const FEATURE_LAYERS: u16 = 16 * Feature::ALL.len() as u16;

/// How strongly a cell's hue tints feature art, as for walls.
const HUE_TINT: f32 = 0.6;

impl Feature {
    pub const ALL: [Self; 5] = [
        Self::Platform,
        Self::Spring,
        Self::Mud,
        Self::Mirror,
        Self::Crumbling,
    ];

    /// How a wall with `traits` is drawn, or `None` for a plain wall. A
    /// mirror shows as one whatever else it does, then bounce, then one-way.
    #[must_use]
    pub fn of(traits: Traits) -> Option<Self> {
        if traits.mirror {
            Some(Self::Mirror)
        } else if traits.crumble > 0 {
            Some(Self::Crumbling)
        } else if traits.bounce > 1.0 {
            Some(Self::Spring)
        } else if traits.bounce < 1.0 {
            Some(Self::Mud)
        } else if traits.one_way {
            Some(Self::Platform)
        } else {
            None
        }
    }

    /// The tileset layer for this feature with neighbours `mask`.
    #[must_use]
    pub fn layer(self, mask: u8) -> u16 {
        let index = Self::ALL.iter().position(|&f| f == self).unwrap_or(0);
        u16::try_from(index).unwrap_or(0) * 16 + u16::from(mask & 15)
    }

    /// How a pixel of a tile is painted, in tile pixels from the top left.
    fn paint(self, theme: &Theme, x: f32, y: f32, mask: u8) -> [f32; 4] {
        match self {
            Self::Platform => platform(theme, x, y, mask),
            Self::Spring => spring(theme, x, y, mask),
            Self::Mud => mud(theme, x, y, mask),
            Self::Mirror => mirror(theme, x, y, mask),
            Self::Crumbling => crumbling(theme, x, y, mask),
        }
    }
}

/// A theme's features tileset: every tile stacked top to bottom, in layer
/// order.
#[must_use]
pub fn tileset(theme: &Theme) -> Canvas {
    let mut canvas = Canvas::new(TILE_PX, TILE_PX * u32::from(FEATURE_LAYERS));
    for feature in Feature::ALL {
        for mask in 0..16 {
            let top = u32::from(feature.layer(mask)) * TILE_PX;
            for y in 0..TILE_PX {
                for x in 0..TILE_PX {
                    #[allow(clippy::cast_precision_loss)]
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    canvas.set(x, top + y, feature.paint(theme, px, py, mask));
                }
            }
        }
    }
    canvas
}

/// The features tileset's path under the assets folder.
#[must_use]
pub fn tileset_path(theme: &Theme) -> String {
    format!("art/{}/features.png", theme.name)
}

#[allow(clippy::cast_precision_loss)]
const SIZE: f32 = TILE_PX as f32;

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn scale(c: Rgb, by: f32) -> Rgb {
    c.map(|v| (v * by).clamp(0.0, 1.0))
}

const fn with_alpha([r, g, b]: Rgb, alpha: f32) -> [f32; 4] {
    [r, g, b, alpha]
}

/// How much of a pixel lies inside, from its signed distance to the edge
/// in pixels (negative inside).
fn cover(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Distance inside an end that isn't joined to a neighbour, rounded at the
/// corners by `radius`, for a shape `height` pixels tall from the top.
/// Positive outside.
fn rounded_ends(x: f32, y: f32, mask: u8, height: f32, radius: f32) -> f32 {
    let left = if mask & LEFT == 0 { radius } else { -SIZE };
    let right = if mask & RIGHT == 0 {
        SIZE - radius
    } else {
        SIZE * 2.0
    };
    let cx = x.clamp(left, right);
    let cy = y.clamp(radius, height - radius);
    let corner = Vec2::new(x - cx, y - cy).length() - radius;
    corner.max(y - height).max(-y)
}

/// A one-way platform: a slim stone ledge with faint chevrons pointing up
/// through it.
fn platform(theme: &Theme, x: f32, y: f32, mask: u8) -> [f32; 4] {
    // Twice `PLATFORM_HEIGHT` in pixels.
    let height = 20.0;
    let inside = cover(rounded_ends(x, y, mask, height, 5.0));
    let [light, mid, dark] = theme.stone;
    let mut colour = lerp(mid, scale(dark, 0.8), smoothstep(height - 6.0, height, y));
    colour = lerp(light, colour, smoothstep(1.0, 4.0, y));
    // A seam where each tile's ledge meets the next.
    if mask & LEFT != 0 && x < 1.5 {
        colour = lerp(colour, theme.mortar, 0.6);
    }
    // Chevrons, every half tile.
    let across = (x.rem_euclid(SIZE / 2.0) - SIZE / 4.0).abs();
    let chevron = ((y - 6.0 - across * 0.6).abs() - 1.2).max(across - 7.0);
    colour = lerp(colour, scale(light, 1.15), cover(chevron) * 0.5);
    with_alpha(colour, inside)
}

/// A crumbling ledge: the platform's stone, darker and split by cracks,
/// with chips missing from its underside.
fn crumbling(theme: &Theme, x: f32, y: f32, mask: u8) -> [f32; 4] {
    // Twice `PLATFORM_HEIGHT` in pixels.
    let height = 20.0;
    // A ragged underside, chipped away in places.
    let chip = (x * 0.31).sin() * 2.0 + (x * 0.73).sin() * 1.5;
    let bottom = height - 1.5 + chip.min(0.0) * 1.5;
    let inside = cover(rounded_ends(x, y, mask, bottom.max(8.0), 4.0));
    let [light, mid, dark] = theme.stone;
    let mut colour = lerp(
        scale(mid, 0.8),
        scale(dark, 0.65),
        smoothstep(height - 8.0, height, y),
    );
    colour = lerp(scale(light, 0.9), colour, smoothstep(1.0, 4.0, y));
    // Cracks running down through the ledge, two to a tile.
    for start in [SIZE * 0.3, SIZE * 0.78] {
        let line = start + (y * 0.5).sin() * 3.0 + y * 0.25;
        colour = lerp(colour, theme.mortar, cover((x - line).abs() - 0.9) * 0.9);
    }
    with_alpha(colour, inside)
}

/// A spring pad: a lit plate on two coils over a dark base.
fn spring(theme: &Theme, x: f32, y: f32, mask: u8) -> [f32; 4] {
    // Twice `PAD_HEIGHT` in pixels.
    let height = 40.0;
    let metal: Rgb = [0.66, 0.68, 0.74];
    let [plate_dark, plate_light] = theme.crystal[0];
    let plate = 10.0;
    let base = 32.0;
    if y < plate {
        let inside = cover(rounded_ends(x, y, mask, plate, 4.0));
        let lit = lerp(plate_light, plate_dark, smoothstep(2.0, plate, y));
        return with_alpha(lit, inside);
    }
    if y >= base {
        let inside = cover(rounded_ends(x, y - base, mask, height - base, 3.0));
        return with_alpha(scale(metal, 0.45 + 0.1 * (height - y) / 8.0), inside);
    }
    // Two coils per tile, zigzagging three times between plate and base.
    let along = (y - plate) / (base - plate);
    let zig = (along * 3.0).fract();
    let swing = (zig * 2.0 - 1.0).abs() * 2.0 - 1.0;
    let centre = if x < SIZE / 2.0 {
        SIZE / 4.0
    } else {
        SIZE * 0.75
    };
    // Measured square to the wire, which runs at a slant.
    let reach = 11.0;
    let slope = 4.0 * reach * 3.0 / (base - plate);
    let wire = (x - (centre + swing * reach)).abs() / slope.hypot(1.0) - 1.6;
    // The wire's far half is in shade.
    let front = zig < 0.5;
    let colour = scale(metal, if front { 1.05 } else { 0.7 });
    with_alpha(colour, cover(wire))
}

/// Mud: a dark, mottled sludge with a wet, wavy top and a few bubbles.
fn mud(theme: &Theme, x: f32, y: f32, mask: u8) -> [f32; 4] {
    let wet: Rgb = lerp([0.42, 0.3, 0.19], theme.mortar, 0.25);
    let phase = |period: f32, at: f32| (at / period * TAU).sin();
    // Mottling that repeats every tile, so neighbours join.
    let mottle = 0.5 * phase(SIZE, x + 9.0 * phase(SIZE / 2.0, y))
        + 0.5 * phase(SIZE / 2.0, y + 7.0 * phase(SIZE, x));
    let mut colour = scale(wet, 0.85 + 0.12 * mottle);
    let mut edge = f32::MIN;
    if mask & ABOVE == 0 {
        let surface = 7.0 + 2.5 * phase(SIZE / 2.0, x) + 1.5 * phase(SIZE, x + 17.0);
        edge = edge.max(surface - y);
        colour = lerp(scale(wet, 1.45), colour, smoothstep(0.0, 5.0, y - surface));
        // Bubbles just under the surface.
        for (bx, by, r) in [(18.0, 15.0, 3.0), (52.0, 19.0, 2.2), (63.0, 13.0, 1.6)] {
            let ring = (Vec2::new(x - bx, y - by).length() - r).abs() - 0.7;
            colour = lerp(colour, scale(wet, 1.6), cover(ring) * 0.8);
        }
    }
    for (side, distance) in [(LEFT, x), (RIGHT, SIZE - x)] {
        if mask & side == 0 {
            edge = edge.max(2.5 + 1.5 * phase(SIZE / 4.0, y) - distance);
        }
    }
    if mask & BELOW == 0 {
        colour = lerp(colour, scale(wet, 0.6), smoothstep(SIZE - 10.0, SIZE, y));
    }
    with_alpha(colour, cover(edge))
}

/// A mirror: silvered glass with bright streaks across it, framed in the
/// theme's stone wherever it doesn't join another mirror.
fn mirror(theme: &Theme, x: f32, y: f32, mask: u8) -> [f32; 4] {
    let silver: Rgb = lerp([0.66, 0.72, 0.8], theme.glass[1], 0.25);
    // A gentle shimmer that repeats every tile, so stacked mirrors join.
    let mut colour = scale(silver, 1.0 + 0.05 * (y / SIZE * TAU).cos());
    // Streaks along the diagonal, repeating every tile.
    let diagonal = (x + y).rem_euclid(SIZE);
    let streak = (-((diagonal - 22.0) / 3.0).powi(2)).exp()
        + 0.6 * (-((diagonal - 34.0) / 1.5).powi(2)).exp();
    colour = lerp(colour, [1.0; 3], streak * 0.7);
    let frame = 6.0;
    let [light, mid, dark] = theme.stone;
    let sides = [
        (ABOVE, y, light),
        (LEFT, x, light),
        (BELOW, SIZE - y, dark),
        (RIGHT, SIZE - x, mid),
    ];
    for (side, distance, stone) in sides {
        if mask & side == 0 {
            if distance < frame {
                colour = stone;
            } else if distance < frame + 1.5 {
                colour = scale(colour, 0.55);
            }
        }
    }
    with_alpha(colour, 1.0)
}

/// Which cells hold which feature, and the tint each gives the art.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureCells {
    cols: i32,
    rows: i32,
    tile: f32,
    /// Row-major from the bottom-left: the feature, its tint and how far
    /// down from the cell's top it reaches.
    cells: Vec<Option<(Feature, [f32; 3], f32)>>,
}

impl FeatureCells {
    /// Place each wall with traits from `walls` (in a room of `size`) on
    /// the level grid; plain walls are left out.
    #[must_use]
    pub fn new(size: Vec2, tile: f32, walls: &[(Aabb, Option<Hue>, Traits)]) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let cols = (size.x / tile).round() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let rows = (size.y / tile).round() as i32;
        let mut cells = vec![None; usize::try_from(cols * rows).unwrap_or(0)];
        for &(aabb, hue, traits) in walls {
            let Some(feature) = Feature::of(traits) else {
                continue;
            };
            let tint = tint(hue, HUE_TINT);
            #[allow(clippy::cast_possible_truncation)]
            let span =
                |from: f32, to: f32| ((from / tile).floor() as i32, (to / tile).ceil() as i32);
            let (left, right) = span(aabb.min.x, aabb.max.x);
            let (bottom, top) = span(aabb.min.y, aabb.max.y);
            for row in bottom.max(0)..top.min(rows) {
                #[allow(clippy::cast_precision_loss)]
                let cell_top = (row + 1) as f32 * tile;
                let depth = (cell_top - aabb.min.y).min(tile);
                for col in left.max(0)..right.min(cols) {
                    if let Ok(i) = usize::try_from(row * cols + col) {
                        cells[i] = Some((feature, tint, depth));
                    }
                }
            }
        }
        Self {
            cols,
            rows,
            tile,
            cells,
        }
    }

    fn get(&self, col: i32, row: i32) -> Option<(Feature, [f32; 3], f32)> {
        if col < 0 || row < 0 || col >= self.cols || row >= self.rows {
            return None;
        }
        usize::try_from(row * self.cols + col)
            .ok()
            .and_then(|i| self.cells[i])
    }

    /// Which neighbours of a cell hold the same feature.
    #[must_use]
    pub fn mask(&self, col: i32, row: i32) -> u8 {
        let Some((feature, ..)) = self.get(col, row) else {
            return 0;
        };
        [
            (ABOVE, col, row + 1),
            (BELOW, col, row - 1),
            (LEFT, col - 1, row),
            (RIGHT, col + 1, row),
        ]
        .iter()
        .filter(|&&(_, c, r)| self.get(c, r).is_some_and(|(f, ..)| f == feature))
        .fold(0, |mask, &(bit, ..)| mask | bit)
    }

    /// Whether any cell holds a feature.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(Option::is_none)
    }

    /// A quad for each feature cell, covering the part of the cell it fills
    /// with the matching part of its tile. Like wall tiles, each corner
    /// carries the cell's tint, with the tileset layer in the alpha channel.
    #[must_use]
    pub fn mesh(&self) -> Mesh {
        let (mut positions, mut uvs, mut colours, mut indices) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for row in 0..self.rows {
            for col in 0..self.cols {
                let Some((feature, [r, g, b], depth)) = self.get(col, row) else {
                    continue;
                };
                let layer = f32::from(feature.layer(self.mask(col, row)));
                let base = u32::try_from(positions.len()).unwrap_or(0);
                #[allow(clippy::cast_precision_loss)]
                let (left, top) = (col as f32 * self.tile, (row + 1) as f32 * self.tile);
                let reach = depth / self.tile;
                for (dx, down, u, v) in [
                    (0.0, 0.0, 0.0, 0.0),
                    (1.0, 0.0, 1.0, 0.0),
                    (0.0, depth, 0.0, reach),
                    (1.0, depth, 1.0, reach),
                ] {
                    positions.push([left + dx * self.tile, top - down, 0.0]);
                    uvs.push([u, v]);
                    colours.push([r, g, b, layer]);
                }
                indices.extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(indices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::paint::themes;
    use crate::level::format::{MUD, PAD_HEIGHT, PLATFORM_HEIGHT, SPRING};
    use bevy::mesh::VertexAttributeValues;

    fn traits(one_way: bool, bounce: f32, mirror: bool) -> Traits {
        Traits {
            one_way,
            bounce,
            mirror,
            ..Traits::default()
        }
    }

    #[test]
    fn committed_feature_tilesets_are_up_to_date() {
        for theme in themes() {
            let path = format!(
                "{}/assets/{}",
                env!("CARGO_MANIFEST_DIR"),
                tileset_path(&theme)
            );
            let committed = std::fs::read(&path).unwrap_or_default();
            let painted = tileset(&theme).png().unwrap_or_default();
            assert!(
                committed == painted,
                "{path} is out of date; run `just art`"
            );
        }
    }

    #[test]
    fn traits_pick_the_feature_drawn() {
        assert_eq!(Feature::of(Traits::default()), None);
        assert_eq!(
            Feature::of(traits(true, 1.0, false)),
            Some(Feature::Platform)
        );
        assert_eq!(
            Feature::of(traits(true, SPRING, false)),
            Some(Feature::Spring)
        );
        assert_eq!(Feature::of(traits(false, MUD, false)), Some(Feature::Mud));
        assert_eq!(Feature::of(traits(false, MUD, true)), Some(Feature::Mirror));
        let layers: std::collections::BTreeSet<_> = Feature::ALL
            .into_iter()
            .flat_map(|f| (0..16).map(move |m| f.layer(m)))
            .collect();
        assert_eq!(layers.len(), usize::from(FEATURE_LAYERS));
    }

    #[test]
    fn slim_features_fill_only_the_top_of_their_tiles() {
        let theme = themes()[0];
        let alpha = |feature: Feature, depth: f32| feature.paint(&theme, 40.0, depth, 0)[3];
        let platform = PLATFORM_HEIGHT * 2.0;
        assert!(alpha(Feature::Platform, platform - 2.0) > 0.99);
        assert!(alpha(Feature::Platform, platform + 2.0) < 0.01);
        let pad = PAD_HEIGHT * 2.0;
        assert!(alpha(Feature::Spring, 4.0) > 0.99);
        assert!(alpha(Feature::Spring, pad + 2.0) < 0.01);
        assert!(alpha(Feature::Mud, 60.0) > 0.99);
        assert!(alpha(Feature::Mirror, 60.0) > 0.99);
    }

    #[test]
    fn runs_of_a_feature_join_up_and_end_cleanly() {
        let theme = themes()[1];
        // A joined edge is filled to the side; an end's corner is rounded off.
        let platform = |x, mask| Feature::Platform.paint(&theme, x, 1.0, mask)[3];
        assert!(platform(0.5, LEFT) > 0.99);
        assert!(platform(0.5, 0) < 0.5);
        let mirror = |x, mask| Feature::Mirror.paint(&theme, x, 40.0, mask);
        assert_ne!(
            mirror(2.0, 0),
            mirror(2.0, LEFT),
            "unjoined sides are framed"
        );
    }

    #[test]
    fn feature_quads_cover_what_each_wall_fills() {
        let walls = [
            // A plain wall and a slim one-way platform in the second row.
            (
                Aabb::new(Vec2::ZERO, Vec2::new(160.0, 40.0)),
                None,
                Traits::default(),
            ),
            (
                Aabb::new(Vec2::new(40.0, 70.0), Vec2::new(120.0, 80.0)),
                Some(Hue::Red),
                traits(true, 1.0, false),
            ),
        ];
        let cells = FeatureCells::new(Vec2::new(160.0, 120.0), 40.0, &walls);
        assert_eq!(cells.mask(1, 1), RIGHT);
        assert_eq!(cells.mask(2, 1), LEFT);
        assert_eq!(cells.mask(0, 0), 0, "plain walls aren't features");
        let mesh = cells.mesh();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        assert_eq!(positions.len(), 8, "a quad per platform cell");
        let ys: Vec<f32> = positions.iter().map(|p| p[1]).collect();
        assert!(
            ys.iter()
                .all(|&y| (y - 80.0).abs() < 1e-4 || (y - 70.0).abs() < 1e-4)
        );
        assert!(!cells.is_empty());
    }
}
