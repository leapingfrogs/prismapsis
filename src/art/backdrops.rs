//! Backdrops: two layers behind each theme's level, painted offline
//! (`just art`) and scrolled more slowly than the level as the camera moves,
//! so they feel far away.
//!
//! The far layer fills the view: the quarry's distant cliff of blocks, the
//! caverns' rock with glowing fungus, the observatory's night sky, the
//! foundry's furnace hall and the glacier's aurora. The near layer holds
//! props with clear space between them: stone pillars, hanging
//! stalactites, a colonnade of arches, girders and chains, ice spires. Both tile in every direction, so any
//! level size is covered, and both are dark and soft, so the level stands in
//! front of them. They go through the same colour filter as the walls, so
//! they start grey and their colours return with the level's.

use super::mix;
use super::paint::{Canvas, Rgb, Theme, WallStyle};

/// Pixels across and down each layer's tile.
pub const BACKDROP_PX: u32 = 512;

/// World units per backdrop pixel: a quarter of the walls' resolution, as
/// the backdrop is soft and far away, and so each tile spans most of a
/// screen.
pub const BACKDROP_UNITS_PER_PX: f32 = 2.0;

/// Layers per theme, far then near.
pub const BACKDROP_LAYERS: u16 = 2;

/// How far each layer moves with the level as the camera moves, far then
/// near: 0 would stay fixed on screen, 1 would move with the walls.
pub const PARALLAX: [f32; 2] = [0.2, 0.45];

/// A theme's backdrop, its layers stacked top to bottom, far first.
#[must_use]
pub fn backdrop(theme: &Theme) -> Canvas {
    let mut canvas = Canvas::new(BACKDROP_PX, BACKDROP_PX * u32::from(BACKDROP_LAYERS));
    for layer in 0..u32::from(BACKDROP_LAYERS) {
        for y in 0..BACKDROP_PX {
            for x in 0..BACKDROP_PX {
                #[allow(clippy::cast_precision_loss)]
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let pixel = match (theme.walls, layer) {
                    (WallStyle::Blocks, 0) => cliff(theme, px, py),
                    (WallStyle::Blocks, _) => pillars(theme, px, py),
                    (WallStyle::Rock, 0) => cave(theme, px, py),
                    (WallStyle::Rock, _) => stalactites(theme, px, py),
                    (WallStyle::Marble, 0) => sky(theme, px, py),
                    (WallStyle::Marble, _) => colonnade(theme, px, py),
                    (WallStyle::Plates, 0) => furnace_hall(theme, px, py),
                    (WallStyle::Plates, _) => girders(theme, px, py),
                    (WallStyle::Ice, 0) => aurora(theme, px, py),
                    (WallStyle::Ice, _) => ice_spires(theme, px, py),
                };
                canvas.set(x, layer * BACKDROP_PX + y, pixel);
            }
        }
    }
    canvas
}

/// The backdrop's path under the assets folder.
#[must_use]
pub fn backdrop_path(theme: &Theme) -> String {
    format!("art/{}/backdrop.png", theme.name)
}

#[allow(clippy::cast_precision_loss)]
const SIZE: f32 = BACKDROP_PX as f32;

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn scale(c: Rgb, by: f32) -> Rgb {
    c.map(|v| (v * by).clamp(0.0, 1.0))
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

const fn opaque([r, g, b]: Rgb) -> [f32; 4] {
    [r, g, b, 1.0]
}

/// A number from 0 to 1 for lattice point `(x, y)` of a pattern `seed`.
#[allow(clippy::cast_precision_loss)]
fn hash(seed: u32, x: i64, y: i64) -> f32 {
    let key = (u64::from(seed) << 48)
        ^ ((x.cast_unsigned() & 0xff_ffff) << 24)
        ^ (y.cast_unsigned() & 0xff_ffff);
    (mix(key) % 100_000) as f32 / 100_000.0
}

/// Smooth noise from 0 to 1 that repeats every `period` lattice cells, so
/// the tile wraps; `cell` is the lattice spacing in pixels.
#[allow(clippy::cast_possible_truncation)]
fn noise(seed: u32, x: f32, y: f32, cell: f32) -> f32 {
    let period = (SIZE / cell).round() as i64;
    let (gx, gy) = (x / cell, y / cell);
    let (x0, y0) = (gx.floor(), gy.floor());
    let (tx, ty) = (gx - x0, gy - y0);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let at = |dx: i64, dy: i64| {
        let ix = (x0 as i64 + dx).rem_euclid(period);
        let iy = (y0 as i64 + dy).rem_euclid(period);
        hash(seed, ix, iy)
    };
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// A few octaves of `noise`, from 0 to 1.
fn fbm(seed: u32, x: f32, y: f32, cell: f32) -> f32 {
    let mut total = 0.0;
    let mut weight = 0.5;
    let mut size = cell;
    let mut sum = 0.0;
    for octave in 0..4 {
        total += noise(seed + octave, x, y, size) * weight;
        sum += weight;
        weight *= 0.5;
        size /= 2.0;
    }
    total / sum
}

/// Distance across the tile's wrap: the shorter way round.
fn wrapped(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(SIZE);
    d.min(SIZE - d)
}

/// The quarry's far wall: a cliff of big staggered blocks in the dark, a
/// few with blue crystal glinting in them.
fn cliff(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let (width, height) = (256.0, 128.0);
    let row = (y / height).floor();
    let shift = if row.rem_euclid(2.0) < 1.0 {
        0.0
    } else {
        width / 2.0
    };
    let column = ((x + shift) / width).floor().rem_euclid(SIZE / width);
    let (bx, by) = ((x + shift).rem_euclid(width), y.rem_euclid(height));
    #[allow(clippy::cast_possible_truncation)]
    let (ci, ri) = (column as i64, row as i64);
    let block = hash(theme.seed * 7, ci, ri);
    let grain = fbm(theme.seed * 7 + 1, x, y, 64.0);
    let stone = lerp(
        theme.backdrop,
        scale(theme.stone[2], 0.3),
        0.35 + 0.25 * block,
    );
    let mut colour = scale(stone, 0.75 + 0.45 * grain);
    // Faint joints between the blocks, and a lit top edge on each.
    let edge = bx.min(width - bx).min(by).min(height - by);
    colour = lerp(scale(colour, 0.7), colour, smoothstep(1.0, 3.0, edge));
    if by < 3.0 && edge >= 1.0 {
        colour = scale(colour, 1.12);
    }
    // Crystal in about one block in twelve.
    if block > 0.92 {
        let d = (bx - width / 2.0).abs() / 10.0 + (by - height / 2.0).abs() / 16.0;
        colour = lerp(
            colour,
            scale(theme.crystal[0][1], 0.6),
            cover((d - 1.0) * 10.0),
        );
    }
    opaque(colour)
}

/// The quarry's props: two stone pillars a tile, with capitals and moss
/// spilling from their tops, and nothing between them.
fn pillars(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let mut best = [0.0; 4];
    for (centre, width) in [(110.0, 44.0), (370.0, 56.0)] {
        let across = (x - centre) / (width / 2.0);
        let capital = y.rem_euclid(SIZE) < 26.0;
        let half = if capital { 1.35 } else { 1.0 };
        let inside = cover((across.abs() - half) * width / 2.0);
        if inside <= 0.0 {
            continue;
        }
        let lit = 0.75 - 0.35 * across.clamp(-1.0, 1.0);
        let grain = fbm(theme.seed * 11, x, y, 32.0);
        let mut colour = scale(theme.stone[2], 0.3 * lit * (0.85 + 0.3 * grain));
        if capital {
            colour = scale(colour, 1.25);
        }
        // Moss hanging from the capital.
        let drip = 26.0 + 30.0 * noise(theme.seed * 13, x, 0.0, 16.0);
        if (26.0..drip).contains(&y) {
            colour = lerp(colour, scale(theme.moss[0], 0.4), 0.8);
        }
        best = [colour[0], colour[1], colour[2], inside];
    }
    best
}

/// The caverns' far wall: rough rock in strata, freckled with glowing
/// fungus and the odd glint of crystal.
fn cave(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let rock = fbm(theme.seed * 7, x, y, 128.0);
    // Strata: gentle horizontal bands, bent a little by the rock.
    let bend = fbm(theme.seed * 7 + 9, x, y, 128.0) * 40.0;
    let strata = 0.5 + 0.5 * ((y + bend) / SIZE * std::f32::consts::TAU * 4.0).sin();
    let base = lerp(
        theme.backdrop,
        scale(theme.stone[2], 0.45),
        0.3 + 0.5 * rock,
    );
    let mut colour = scale(base, 0.9 + 0.15 * strata);
    // Fungus: soft glowing specks in clusters.
    let cluster = noise(theme.seed * 17, x, y, 64.0);
    let (cx, cy) = ((x / 16.0).floor(), (y / 16.0).floor());
    #[allow(clippy::cast_possible_truncation)]
    let speck = hash(theme.seed * 19, cx as i64, cy as i64);
    if cluster > 0.6 && speck > 0.75 {
        let d = (x.rem_euclid(16.0) - 8.0).hypot(y.rem_euclid(16.0) - 8.0);
        let glow = (-(d / 2.5).powi(2)).exp();
        colour = lerp(
            colour,
            scale(theme.moss[1], 0.7),
            glow * (cluster - 0.6) * 2.5,
        );
    }
    if speck > 0.995 {
        let d = (x.rem_euclid(16.0) - 8.0).abs() / 3.0 + (y.rem_euclid(16.0) - 8.0).abs() / 5.0;
        colour = lerp(
            colour,
            scale(theme.crystal[1][1], 0.55),
            cover((d - 1.0) * 4.0),
        );
    }
    opaque(colour)
}

/// The caverns' props: stalactites hanging from the dark above and a
/// glowing drop of fungus at each tip.
fn stalactites(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let mut best = [0.0; 4];
    for (index, centre) in (0i64..).zip([70.0f32, 200.0, 290.0, 430.0]) {
        let size = hash(theme.seed * 23, index, 0);
        let length = 120.0 + 220.0 * size;
        let width = 26.0 + 30.0 * size;
        let top = SIZE * hash(theme.seed * 29, index, 1);
        let down = (y - top).rem_euclid(SIZE);
        if down > length + 8.0 {
            continue;
        }
        let taper = 1.0 - down / length;
        let ragged = 1.0 + 0.25 * (noise(theme.seed * 31, x, y, 16.0) - 0.5);
        let half = width / 2.0 * taper.max(0.0) * ragged;
        let across = wrapped(x, centre);
        let inside = if down <= length {
            cover(across - half)
        } else {
            0.0
        };
        // The glowing tip.
        let tip = (-(across.hypot(down - length) / 4.0).powi(2)).exp();
        let lit = 0.8 - 0.3 * ((x - centre) / width).clamp(-1.0, 1.0);
        let rock = scale(theme.stone[2], 0.32 * lit);
        let colour = lerp(rock, scale(theme.moss[1], 0.75), tip);
        let alpha = inside.max(tip);
        if alpha > best[3] {
            best = [colour[0], colour[1], colour[2], alpha];
        }
    }
    best
}

/// The foundry's far wall: a smoky hall with big pipes running up it and
/// the glow of furnaces low down, breaking through in a few hot windows.
fn furnace_hall(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let smoke = fbm(theme.seed * 7, x, y, 128.0);
    let glow = 0.5 + 0.5 * (y / SIZE * std::f32::consts::TAU).sin();
    let mut colour = lerp(
        theme.backdrop,
        scale(theme.glass[0], 0.35),
        (0.25 + 0.35 * smoke) * glow,
    );
    // Pipes: two a tile, lit on their left.
    for (centre, half) in [(140.0f32, 26.0f32), (390.0, 18.0)] {
        let across = (x - centre) / half;
        if across.abs() < 1.0 {
            let lit = 0.6 - 0.35 * across;
            colour = scale(theme.stone[2], 0.3 * lit);
            // A flange every half tile.
            if (y.rem_euclid(SIZE / 2.0) - 20.0).abs() < 5.0 {
                colour = scale(colour, 1.4);
            }
        }
    }
    // Hot windows: a few small glowing slots.
    let (cx, cy) = ((x / 64.0).floor(), (y / 64.0).floor());
    #[allow(clippy::cast_possible_truncation)]
    let window = hash(theme.seed * 43, cx as i64, cy as i64);
    let (lx, ly) = (x.rem_euclid(64.0) - 32.0, y.rem_euclid(64.0) - 32.0);
    if window > 0.9 {
        // An arched furnace mouth, hottest low down, with a soft glow round it.
        let mouth = lx.abs().max(-ly) < 9.0 && lx.hypot(ly.min(0.0)) < 9.0 && ly < 10.0;
        let halo = (-(lx.hypot(ly) / 18.0).powi(2)).exp() * 0.5;
        colour = lerp(colour, scale(theme.glass[1], 0.5), halo);
        if mouth {
            colour = scale(theme.crystal[0][1], 0.45 + 0.25 * (ly + 9.0) / 19.0);
        }
    }
    opaque(colour)
}

/// The foundry's props: iron girders with cross-bracing between them and
/// chains hanging in the gaps.
fn girders(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let iron = |lit: f32| scale(theme.stone[2], 0.28 * lit);
    // Two girders a tile, each an I-beam lit on its left.
    for centre in [100.0f32, 356.0] {
        let across = (x - centre).abs();
        if across < 18.0 {
            let flange = across > 13.0;
            let lit = if x < centre { 1.0 } else { 0.7 };
            let web = if flange { 1.1 } else { 0.8 };
            return [
                iron(lit * web)[0],
                iron(lit * web)[1],
                iron(lit * web)[2],
                1.0,
            ];
        }
    }
    // Cross-bracing between the pair, an X every half tile.
    let local = x - 118.0;
    if (0.0..220.0).contains(&local) {
        let along = y.rem_euclid(SIZE / 2.0);
        let slope = (SIZE / 2.0) / 220.0;
        let brace = (along - local * slope)
            .abs()
            .min((along - (220.0 - local) * slope).abs());
        if brace < 3.0 {
            let c = iron(0.85);
            return [c[0], c[1], c[2], cover(brace - 2.5)];
        }
    }
    // A chain hanging in the outer gap.
    let chain = x - 470.0;
    if chain.abs() < 5.0 {
        let link = y.rem_euclid(16.0) - 8.0;
        let ring = (chain.hypot(link * 0.8) - 4.0).abs();
        let c = iron(1.2);
        return [c[0], c[1], c[2], cover(ring - 1.0)];
    }
    [0.0; 4]
}

/// The glacier's sky: deep blue with stars and curtains of aurora rippling
/// across it, green with violet edges.
fn aurora(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let tau = std::f32::consts::TAU;
    let mut colour = theme.backdrop;
    // Two curtains a tile, waving across and drifting down.
    for (offset, phase) in [(0.0f32, 0.0f32), (SIZE / 2.0, 1.7)] {
        let wave = (x / SIZE * tau * 2.0 + phase).sin() * 40.0
            + (x / SIZE * tau * 5.0 + phase).sin() * 12.0;
        let centre = offset + SIZE / 4.0 + wave;
        let d = wrapped(y, centre);
        let rays = 0.8 + 0.2 * (x / SIZE * tau * 24.0 + phase).sin();
        let core = (-(d / 26.0).powi(2)).exp() * rays;
        let fringe = (-(d / 50.0).powi(2)).exp() * 0.5;
        colour = lerp(colour, scale(theme.crystal[1][1], 0.35), fringe * 0.35);
        colour = lerp(colour, [0.35, 0.95, 0.6], core * 0.2);
    }
    let (cx, cy) = ((x / 8.0).floor(), (y / 8.0).floor());
    #[allow(clippy::cast_possible_truncation)]
    let star = hash(theme.seed * 37, cx as i64, cy as i64);
    if star > 0.98 {
        let d = (x.rem_euclid(8.0) - 4.0).hypot(y.rem_euclid(8.0) - 4.0);
        colour = lerp(colour, [0.9, 0.95, 1.0], (-(d / 1.2).powi(2)).exp() * 0.8);
    }
    opaque(colour)
}

/// The glacier's props: jagged spires of ice rising from below and
/// hanging from above, pale and a little see-through.
fn ice_spires(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let mut best = [0.0; 4];
    for (index, centre) in (0i64..).zip([60.0f32, 180.0, 300.0, 440.0]) {
        let size = hash(theme.seed * 23, index, 0);
        let width = 30.0 + 40.0 * size;
        let height = 160.0 + 200.0 * size;
        // Rising from the tile's foot, which meets the next tile's head.
        let up = (SIZE - y).rem_euclid(SIZE);
        let ragged = 1.0 + 0.3 * (noise(theme.seed * 31, x, y, 16.0) - 0.5);
        let half = width / 2.0 * (1.0 - up / height).max(0.0) * ragged;
        let across = wrapped(x, centre);
        let inside = if up < height {
            cover(across - half)
        } else {
            0.0
        };
        if inside > best[3] {
            let lit = 0.85 - 0.3 * ((x - centre) / width).clamp(-1.0, 1.0);
            let facet = if (x - centre).rem_euclid(14.0) < 7.0 {
                1.0
            } else {
                0.85
            };
            let colour = scale(theme.stone[1], 0.32 * lit * facet);
            best = [colour[0], colour[1], colour[2], inside * 0.9];
        }
    }
    best
}

/// The observatory's sky: deep blue with faint drifts of nebula and
/// scattered stars, a few of them gold.
fn sky(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let cloud = fbm(theme.seed * 7, x, y, 256.0);
    let mut colour = lerp(
        theme.backdrop,
        scale(theme.crystal[1][1], 0.35),
        smoothstep(0.45, 0.8, cloud) * 0.6,
    );
    let (cx, cy) = ((x / 8.0).floor(), (y / 8.0).floor());
    #[allow(clippy::cast_possible_truncation)]
    let star = hash(theme.seed * 37, cx as i64, cy as i64);
    if star > 0.975 {
        let d = (x.rem_euclid(8.0) - 4.0).hypot(y.rem_euclid(8.0) - 4.0);
        let bright = (star - 0.975) * 40.0;
        let shade = if star > 0.997 {
            theme.moss[1]
        } else {
            [0.85, 0.88, 1.0]
        };
        colour = lerp(
            colour,
            shade,
            (-(d / 1.2).powi(2)).exp() * (0.3 + 0.7 * bright),
        );
    }
    opaque(colour)
}

/// The observatory's props: an arcade of marble, its round-arched openings
/// edged in gold with the sky showing through, storey on storey.
fn colonnade(theme: &Theme, x: f32, y: f32) -> [f32; 4] {
    let span = SIZE / 2.0;
    // Across the opening from its middle, and how wide it is.
    let dx = x.rem_euclid(span) - span / 2.0;
    let radius = span / 2.0 - 24.0;
    // A band of stone along the top of each storey, then the arch.
    let spring = 40.0 + radius;
    let rise = (spring - y).max(0.0);
    let to_arch = dx.hypot(rise) - radius;
    // Inside the opening is negative.
    let opening = to_arch.max(dx.abs() - radius);
    let inside = cover(-opening);
    if inside <= 0.0 {
        return [0.0; 4];
    }
    let lit = 0.75 - 0.3 * (dx / (span / 2.0)).clamp(-1.0, 1.0);
    let veins = fbm(theme.seed * 41, x, y, 64.0);
    // Gold round the arch.
    let colour = if opening < 3.0 && y < spring + 2.0 {
        scale(theme.moss[0], 0.55)
    } else {
        scale(theme.stone[2], 0.3 * lit * (0.85 + 0.3 * veins))
    };
    [colour[0], colour[1], colour[2], inside]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::paint::themes;

    #[test]
    fn committed_backdrops_are_up_to_date() {
        for theme in themes() {
            let path = format!(
                "{}/assets/{}",
                env!("CARGO_MANIFEST_DIR"),
                backdrop_path(&theme)
            );
            let committed = std::fs::read(&path).unwrap_or_default();
            let painted = backdrop(&theme).png().unwrap_or_default();
            assert!(
                committed == painted,
                "{path} is out of date; run `just art`"
            );
        }
    }

    /// The largest jump between a tile's edge and the opposite edge,
    /// against the largest jump between neighbours inside it.
    fn seam_and_step(canvas: &Canvas, layer: u32) -> (f32, f32) {
        let top = layer * BACKDROP_PX;
        let at = |x: u32, y: u32| {
            let [r, g, b, a] = canvas.get(x, top + y);
            (r + g + b) * a
        };
        let last = BACKDROP_PX - 1;
        let mut seam = 0.0f32;
        let mut step = 0.0f32;
        for i in 0..BACKDROP_PX {
            seam = seam.max((at(0, i) - at(last, i)).abs());
            seam = seam.max((at(i, 0) - at(i, last)).abs());
            for j in 0..last {
                step = step.max((at(j, i) - at(j + 1, i)).abs());
                step = step.max((at(i, j) - at(i, j + 1)).abs());
            }
        }
        (seam, step)
    }

    #[test]
    fn backdrops_tile_without_seams() {
        for theme in themes() {
            let canvas = backdrop(&theme);
            for layer in 0..u32::from(BACKDROP_LAYERS) {
                let (seam, step) = seam_and_step(&canvas, layer);
                assert!(
                    seam <= step + 1e-3,
                    "{} layer {layer}: {seam} > {step}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn the_far_layer_is_solid_and_dark_and_the_near_one_has_gaps() {
        for theme in themes() {
            let canvas = backdrop(&theme);
            let far = &canvas.pixels[..(BACKDROP_PX * BACKDROP_PX) as usize];
            let near = &canvas.pixels[(BACKDROP_PX * BACKDROP_PX) as usize..];
            assert!(far.iter().all(|p| (p[3] - 1.0).abs() < f32::EPSILON));
            #[allow(clippy::cast_precision_loss)]
            let mean =
                far.iter().map(|p| p[0] + p[1] + p[2]).sum::<f32>() / (far.len() as f32 * 3.0);
            assert!(mean < 0.25, "{}: far layer too bright ({mean})", theme.name);
            assert!(near.iter().any(|p| p[3] < 0.01), "{}: no gaps", theme.name);
        }
    }
}
