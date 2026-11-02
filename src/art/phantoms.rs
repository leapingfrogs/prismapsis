//! Art for phantom platforms, painted offline (`just art`) in white and
//! greys and tinted with each phantom's hue in the game.
//!
//! A phantom waiting for its hue is a ghost: a dashed rim of light round
//! faint glass, so it reads as something not yet there. Once its hue is
//! restored it is solid crystal, bevelled and lit from the top left like
//! the walls. Like features, phantoms are drawn one quad per cell, and a
//! cell's tile depends on which of its neighbours belong to the same
//! phantom, so the rim and bevel only run round the outside.
//!
//! Breakable blocks use the same crystal, fractured with cracks, as they
//! too always show their colour.

use super::TILE_PX;
use super::features::{ABOVE, BELOW, LEFT, RIGHT};
use super::paint::Canvas;
use crate::physics::Aabb;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};

/// How a cell of glass looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glass {
    /// A phantom waiting for its hue.
    Ghost,
    /// A phantom once solid.
    Crystal,
    /// A breakable block: crystal with cracks running through it.
    Cracked,
}

impl Glass {
    pub const ALL: [Self; 3] = [Self::Ghost, Self::Crystal, Self::Cracked];
}

/// Layers in the phantom tileset: 16 neighbour masks for each look.
pub const PHANTOM_LAYERS: u16 = 48;

/// The phantom tileset's path under the assets folder.
pub const TILESET_PATH: &str = "art/phantoms.png";

#[allow(clippy::cast_precision_loss)]
const SIZE: f32 = TILE_PX as f32;

/// The tileset layer for a cell of `glass` with neighbours `mask`.
#[must_use]
pub fn layer(glass: Glass, mask: u8) -> u16 {
    let index = Glass::ALL.iter().position(|&g| g == glass).unwrap_or(0);
    u16::try_from(index).unwrap_or(0) * 16 + u16::from(mask & 15)
}

/// Every tile stacked top to bottom, in layer order.
#[must_use]
pub fn tileset() -> Canvas {
    let mut canvas = Canvas::new(TILE_PX, TILE_PX * u32::from(PHANTOM_LAYERS));
    for glass in Glass::ALL {
        for mask in 0..16 {
            let top = u32::from(layer(glass, mask)) * TILE_PX;
            for y in 0..TILE_PX {
                for x in 0..TILE_PX {
                    #[allow(clippy::cast_precision_loss)]
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let pixel = match glass {
                        Glass::Ghost => ghost(px, py, mask),
                        Glass::Crystal => crystal(px, py, mask),
                        Glass::Cracked => cracked(px, py, mask),
                    };
                    canvas.set(x, top + y, pixel);
                }
            }
        }
    }
    canvas
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of a pixel lies inside, from its signed distance to the edge
/// in pixels (negative inside).
fn cover(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Distances to the tile's open sides (those with no neighbour), in pixels:
/// left, top, right and bottom. Joined sides are far away.
fn open_sides(x: f32, y: f32, mask: u8) -> [f32; 4] {
    let far = SIZE * 4.0;
    let side = |bit: u8, distance: f32| if mask & bit == 0 { distance } else { far };
    [
        side(LEFT, x),
        side(ABOVE, y),
        side(RIGHT, SIZE - x),
        side(BELOW, SIZE - y),
    ]
}

/// Distance to the nearest open side, rounding the corner where two meet.
fn edge_distance(x: f32, y: f32, mask: u8, radius: f32) -> f32 {
    let [left, top, right, bottom] = open_sides(x, y, mask);
    let across = left.min(right);
    let down = top.min(bottom);
    // Within `radius` of a corner where two open sides meet, measure to an
    // arc instead, so the outline is rounded.
    if across < radius && down < radius {
        radius - (radius - across).hypot(radius - down)
    } else {
        across.min(down)
    }
}

/// Grey and alpha.
const fn grey(value: f32, alpha: f32) -> [f32; 4] {
    [value, value, value, alpha]
}

/// Dash length along the rim, in pixels; a tile holds a whole number, so
/// dashes run on across joined tiles.
const DASH: f32 = 20.0;

/// A phantom waiting for its hue: faint glass with slow diagonal shimmer,
/// a glow inside the rim and a dashed rim of light.
fn ghost(x: f32, y: f32, mask: u8) -> [f32; 4] {
    let edge = edge_distance(x, y, mask, 10.0);
    if edge < 0.0 {
        return grey(1.0, 0.0);
    }
    let [left, top, right, bottom] = open_sides(x, y, mask);
    // Along whichever side is nearest, so dashes follow the rim.
    let along = if left.min(right) < top.min(bottom) {
        y
    } else {
        x
    };
    let dash = smoothstep(0.1, 0.18, ((along / DASH).fract() - 0.5).abs());
    // Solid at corners, so each corner reads as a bracket.
    let corner = f32::from(left.min(right) < 12.0 && top.min(bottom) < 12.0);
    let rim = cover((edge - 4.0).abs() - 1.5) * dash.max(corner);
    let glow = (1.0 - smoothstep(0.0, 14.0, edge)) * 0.22;
    let streak = ((x + y) / 80.0).fract();
    let shimmer = smoothstep(0.0, 0.1, streak) * (1.0 - smoothstep(0.1, 0.3, streak)) * 0.07;
    let alpha = (0.08 + glow + shimmer).max(rim * 0.95);
    grey(1.0, alpha)
}

/// A restored phantom: a block of crystal with a dark outline, a bevel lit
/// from the top left, a faceted face, faint seams where its cells meet and
/// a gleam on its top-left corner.
fn crystal(x: f32, y: f32, mask: u8) -> [f32; 4] {
    let edge = edge_distance(x, y, mask, 6.0);
    if edge < 0.0 {
        return grey(0.0, 0.0);
    }
    let [left, top, right, bottom] = open_sides(x, y, mask);
    // A facet line from corner to corner splits each cell's face in two
    // lights.
    let mut value = if x > y { 0.62 } else { 0.66 };
    // Seams where cells of the block join.
    let seam = |bit: u8, distance: f32| mask & bit != 0 && distance < 1.0;
    if seam(LEFT, x) || seam(ABOVE, y) {
        value *= 0.8;
    }
    // The bevel: lit on top and left, shaded on the right and below.
    let bevel = 8.0;
    let lit = left.min(top);
    let shaded = right.min(bottom);
    if lit < bevel && lit <= shaded {
        value = 0.95;
    } else if shaded < bevel {
        value = 0.4;
    }
    // A gleam across the block's top-left corner.
    let gleam = 1.0 - smoothstep(1.5, 3.5, (x + y - 34.0).abs());
    if mask & (ABOVE | LEFT) == 0 && x < 30.0 && y < 30.0 {
        value += 0.3 * gleam;
    }
    // A dark outline gives the block a clean edge on any background.
    let outline = 1.0 - smoothstep(1.0, 2.5, edge);
    value *= 1.0 - 0.7 * outline;
    grey(value.min(1.0), 0.94)
}

/// Crystal broken by cracks: a few dark lines branching across each cell
/// from a point off its centre, lit along one edge.
fn cracked(x: f32, y: f32, mask: u8) -> [f32; 4] {
    let [value, _, _, alpha] = crystal(x, y, mask);
    if alpha <= 0.0 {
        return grey(value, alpha);
    }
    let origin = (SIZE * 0.42, SIZE * 0.55);
    let (dx, dy) = (x - origin.0, y - origin.1);
    let angle = dy.atan2(dx);
    let r = dx.hypot(dy);
    // Five crooked rays, and a ring partway out.
    let mut crack = f32::MAX;
    for k in 0..5u8 {
        let a = f32::from(k) * 1.3 + 0.4;
        let wobble = (r * 0.25 + f32::from(k)).sin() * 0.08;
        let off = (angle - a - wobble).sin().abs() * r;
        if (angle - a - wobble).cos() > 0.0 {
            crack = crack.min(off);
        }
    }
    crack = crack.min((r - SIZE * 0.22).abs() + 2.0 * ((angle * 3.0).sin().abs()));
    let line = 1.0 - smoothstep(0.6, 1.4, crack);
    let lit = 1.0 - smoothstep(1.4, 2.4, (crack - 1.6).abs());
    let shade = (value * (1.0 - 0.75 * line) + 0.25 * lit * (1.0 - line)).min(1.0);
    grey(shade, alpha)
}

/// A mesh for one phantom covering `aabb`, a quad per cell of `tile` from
/// its top-left corner.
///
/// A cell cut short by the box's edge shows the
/// matching part of its tile. Each corner carries `tint` (linear), with the
/// tileset layer in the alpha channel.
#[must_use]
pub fn mesh(aabb: &Aabb, tile: f32, glass: Glass, tint: [f32; 3]) -> Mesh {
    let size = aabb.size();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = |length: f32| ((length / tile).ceil() as u32).max(1);
    let (cols, rows) = (count(size.x), count(size.y));
    let (mut positions, mut uvs, mut colours, mut indices) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let [r, g, b] = tint;
    for row in 0..rows {
        for col in 0..cols {
            let mut mask = 0;
            if row > 0 {
                mask |= ABOVE;
            }
            if row + 1 < rows {
                mask |= BELOW;
            }
            if col > 0 {
                mask |= LEFT;
            }
            if col + 1 < cols {
                mask |= RIGHT;
            }
            let layer = f32::from(layer(glass, mask));
            #[allow(clippy::cast_precision_loss)]
            let (left, top) = (
                aabb.min.x + col as f32 * tile,
                aabb.max.y - row as f32 * tile,
            );
            let width = tile.min(aabb.max.x - left);
            let height = tile.min(top - aabb.min.y);
            let base = u32::try_from(positions.len()).unwrap_or(0);
            for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                positions.push([left + dx * width, top - dy * height, 0.0]);
                uvs.push([dx * width / tile, dy * height / tile]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn the_committed_tileset_is_up_to_date() {
        let path = format!("{}/assets/{TILESET_PATH}", env!("CARGO_MANIFEST_DIR"));
        let committed = std::fs::read(&path).unwrap_or_default();
        let painted = tileset().png().unwrap_or_default();
        assert!(
            committed == painted,
            "{path} is out of date; run `just art`"
        );
    }

    #[test]
    fn ghosts_are_faint_and_crystal_is_solid() {
        let middle = SIZE / 2.0;
        let all = ABOVE | BELOW | LEFT | RIGHT;
        assert!(ghost(middle, middle, all)[3] < 0.3);
        assert!(crystal(middle, middle, all)[3] > 0.9);
        // The ghost's rim shows only on open sides.
        let rim = |mask| ghost(SIZE / 2.0, 4.0, mask)[3];
        assert!(rim(0) > 0.8);
        assert!(rim(all) < 0.3);
    }

    #[test]
    fn a_phantom_has_a_quad_per_cell_with_its_layers() {
        let aabb = Aabb::new(Vec2::new(40.0, 0.0), Vec2::new(120.0, 120.0));
        let mesh = mesh(&aabb, 40.0, Glass::Crystal, [1.0, 0.0, 0.0]);
        assert_eq!(mesh.count_vertices(), 2 * 3 * 4);
        let Some(VertexAttributeValues::Float32x4(colours)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("no colours");
        };
        // The top-left cell is open above and to the left, solid art.
        let first = colours[0][3];
        assert!((first - f32::from(layer(Glass::Crystal, BELOW | RIGHT))).abs() < f32::EPSILON);
        assert!(colours.iter().all(|c| (16.0..32.0).contains(&c[3])));
    }

    #[test]
    fn a_thin_phantom_shows_the_top_of_its_tile() {
        let aabb = Aabb::new(Vec2::ZERO, Vec2::new(40.0, 10.0));
        let mesh = mesh(&aabb, 40.0, Glass::Ghost, [1.0; 3]);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("no uvs");
        };
        assert_eq!(uvs[3], [1.0, 0.25]);
    }
}
