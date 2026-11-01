//! Paints each theme's wall tileset, offline (`just art`).
//!
//! Everything is a function of the pixel position and the theme's seed, so
//! the output is reproducible, and patterns repeat every tile so neighbours
//! join up.
//!
//! A tile is centred on the corner where four cells meet, so its quarters
//! are the inner corners of those cells, and the only surfaces it shows run
//! along its centre lines.

use super::{
    BOTTOM_LEFT, BOTTOM_RIGHT, LAYERS, SOLID, TILE_PX, TOP_LEFT, TOP_RIGHT, VARIANTS, filtered,
    layer, mix,
};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::f64::consts::TAU;

/// An sRGB colour, each channel 0 to 1.
pub type Rgb = [f32; 3];

/// What a theme's walls are made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// The folder under `assets/art` its tilesets go in.
    pub name: &'static str,
    pub seed: u32,
    /// Block colours, light to dark; the darkest is kept for variants, as
    /// any pattern in the plain tile repeats with every tile.
    pub stone: [Rgb; 3],
    pub mortar: Rgb,
    /// Moss on top surfaces, dark and light.
    pub moss: [Rgb; 2],
    /// Crystal set into some blocks, dark and light.
    pub crystal: [Rgb; 2],
    /// The glass in scenery panels, kept pale so a panel's own hue tints
    /// it; panes vary between the two.
    pub glass: [Rgb; 2],
    /// Behind the level, for previews.
    pub backdrop: Rgb,
}

/// Every theme, in campaign order.
#[must_use]
pub fn themes() -> Vec<Theme> {
    vec![Theme {
        name: "quarry",
        seed: 1,
        stone: [[0.86, 0.66, 0.46], [0.72, 0.5, 0.36], [0.6, 0.33, 0.27]],
        mortar: [0.3, 0.23, 0.22],
        moss: [[0.3, 0.55, 0.2], [0.58, 0.8, 0.3]],
        crystal: [[0.25, 0.45, 0.9], [0.75, 0.9, 1.0]],
        glass: [[0.33, 0.3, 0.27], [0.42, 0.44, 0.47]],
        backdrop: [0.07, 0.07, 0.12],
    }]
}

/// The tilesets each theme has, each a set of 16 corner tiles and the
/// solid variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Walls the hero bounces off.
    Walls,
    /// Glass panels set back behind the level, for scenery.
    Panels,
}

impl Kind {
    pub const ALL: [Self; 2] = [Self::Walls, Self::Panels];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Walls => "walls",
            Self::Panels => "panels",
        }
    }

    /// How strongly a cell's hue tints this art: panels are pale glass, so
    /// their hue shows strongly.
    #[must_use]
    pub const fn hue_tint(self) -> f32 {
        match self {
            Self::Walls => 0.6,
            Self::Panels => 0.9,
        }
    }
}

/// A straight-alpha sRGB image.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}

impl Canvas {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0.0; 4]; (width * height) as usize],
        }
    }

    const fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> [f32; 4] {
        self.pixels[self.index(x, y)]
    }

    pub fn set(&mut self, x: u32, y: u32, pixel: [f32; 4]) {
        let i = self.index(x, y);
        self.pixels[i] = pixel;
    }

    /// Encode as an 8-bit RGBA PNG.
    ///
    /// # Errors
    /// If encoding fails.
    pub fn png(&self) -> Result<Vec<u8>, png::EncodingError> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        let data: Vec<u8> = self.pixels.iter().flatten().map(|&c| to_byte(c)).collect();
        writer.write_image_data(&data)?;
        writer.finish()?;
        Ok(bytes)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_byte(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// One of the theme's tilesets: every tile stacked top to bottom, in layer
/// order.
#[must_use]
pub fn tileset(theme: &Theme, kind: Kind) -> Canvas {
    let painter = Painter::new(theme);
    let mut canvas = Canvas::new(TILE_PX, TILE_PX * u32::from(LAYERS));
    for mask in 0..=SOLID {
        painter.tile(&mut canvas, kind, mask, 0);
    }
    for variant in 1..=VARIANTS {
        painter.tile(&mut canvas, kind, SOLID, variant);
    }
    canvas
}

/// The tileset's path under the assets folder.
#[must_use]
pub fn tileset_path(theme: &Theme, kind: Kind) -> String {
    format!("art/{}/{}.png", theme.name, kind.name())
}

const CENTRE: f64 = TILE_PX as f64 / 2.0;
/// Width of the lit or shaded rim along each surface.
const BEVEL: f64 = 9.0;
/// Further from a surface than this, a wall is fully in shadow.
const DEPTH: f64 = 40.0;
/// Light falls from above and a little to the left (image y points down).
const LIGHT: [f64; 2] = [-0.41, -0.91];
/// Block size: two blocks across a tile, four rows down it.
const BLOCK: [f64; 2] = [40.0, 20.0];
const MORTAR: f64 = 2.0;
const FAR: f64 = 1e3;
/// A panel's stone frame, and the shadow it casts on the glass inside.
const FRAME: f64 = 6.0;
const INSET: f64 = 8.0;
/// Glass panes are diamonds; this is the spacing of their diagonal leads
/// along each axis, which must divide the tile so panes repeat every tile.
const PANE: f64 = 20.0;
const LEAD: f64 = 1.2;

struct Painter<'a> {
    theme: &'a Theme,
    grain: Fbm<Perlin>,
    moss: Fbm<Perlin>,
}

/// Each quarter of a tile: its mask bit and which side of the centre it's on.
const QUARTERS: [(u8, bool, bool); 4] = [
    (TOP_LEFT, true, true),
    (TOP_RIGHT, true, false),
    (BOTTOM_LEFT, false, true),
    (BOTTOM_RIGHT, false, false),
];

/// Distance from `(x, y)` to the quarter of the plane on the given side of
/// the centre: each quarter continues past the tile, as the cell does.
fn quarter_distance(x: f64, y: f64, top: bool, left: bool) -> f64 {
    let dx = if left { x - CENTRE } else { CENTRE - x }.max(0.0);
    let dy = if top { y - CENTRE } else { CENTRE - y }.max(0.0);
    dx.hypot(dy)
}

/// Distance to the nearest quarter that is (or isn't) solid.
fn distance_to(x: f64, y: f64, mask: u8, solid: bool) -> f64 {
    QUARTERS
        .iter()
        .filter(|(bit, ..)| (mask & bit != 0) == solid)
        .map(|&(_, top, left)| quarter_distance(x, y, top, left))
        .fold(FAR, f64::min)
}

fn quarter_bit(x: f64, y: f64) -> u8 {
    match (y < CENTRE, x < CENTRE) {
        (true, true) => TOP_LEFT,
        (true, false) => TOP_RIGHT,
        (false, true) => BOTTOM_LEFT,
        (false, false) => BOTTOM_RIGHT,
    }
}

/// The quarter directly above or below `(x, y)`, in the other half.
fn bit_across(x: f64, y: f64) -> u8 {
    quarter_bit(x, 2.0 * CENTRE - y)
}

const fn opaque([r, g, b]: Rgb) -> [f32; 4] {
    [r, g, b, 1.0]
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn scale(c: Rgb, by: f32) -> Rgb {
    c.map(|v| v * by)
}

/// A hash of a few integers as a fraction from 0 to 1.
#[allow(clippy::cast_precision_loss)]
fn unit(seed: u32, a: i64, b: i64) -> f32 {
    let h = mix(u64::from(seed) ^ mix(a.cast_unsigned() ^ mix(b.cast_unsigned())));
    (h >> 40) as f32 / (1u64 << 24) as f32
}

impl<'a> Painter<'a> {
    fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            grain: Fbm::<Perlin>::new(theme.seed).set_octaves(4),
            moss: Fbm::<Perlin>::new(theme.seed.wrapping_add(1)).set_octaves(3),
        }
    }

    /// Noise that repeats every tile in both directions, roughly -1 to 1,
    /// with `features` bumps across a tile.
    fn periodic(noise: &Fbm<Perlin>, x: f64, y: f64, features: f64) -> f32 {
        let r = features / TAU;
        let (ax, ay) = (x / f64::from(TILE_PX) * TAU, y / f64::from(TILE_PX) * TAU);
        #[allow(clippy::cast_possible_truncation)]
        let value = noise.get([r * ax.cos(), r * ax.sin(), r * ay.cos(), r * ay.sin()]) as f32;
        value
    }

    fn tile(&self, canvas: &mut Canvas, kind: Kind, mask: u8, variant: u16) {
        let top = u32::from(layer(mask, variant)) * TILE_PX;
        for py in 0..TILE_PX {
            for px in 0..TILE_PX {
                let (x, y) = (f64::from(px) + 0.5, f64::from(py) + 0.5);
                let open = mask & quarter_bit(x, y) == 0;
                let pixel = match (kind, open) {
                    (Kind::Walls, true) => self.open(x, y, mask),
                    (Kind::Walls, false) => opaque(self.solid(x, y, mask, variant)),
                    (Kind::Panels, true) => [0.0; 4],
                    (Kind::Panels, false) => opaque(self.panel(x, y, mask, variant)),
                };
                canvas.set(px, top + py, pixel);
            }
        }
    }

    /// Open space: grass poking up from a floor below, or the shadow under a
    /// ledge above. Otherwise clear.
    fn open(&self, x: f64, y: f64, mask: u8) -> [f32; 4] {
        let across = mask & bit_across(x, y) != 0;
        if !across {
            return [0.0; 4];
        }
        if y < CENTRE {
            // Floor below: blades two pixels wide, scattered along it.
            #[allow(clippy::cast_possible_truncation)]
            let column = (x / 2.0).floor() as i64;
            let roll = unit(self.theme.seed, column, 7);
            let height = f64::from(roll) * 30.0 - 18.0;
            if CENTRE - y < height {
                return opaque(self.theme.moss[1]);
            }
            return [0.0; 4];
        }
        // Ledge above: a soft shadow fading downwards.
        let depth = ((y - CENTRE) / 16.0).min(1.0);
        #[allow(clippy::cast_possible_truncation)]
        let alpha = (0.5 * (1.0 - depth).powi(2)) as f32;
        [0.0, 0.0, 0.0, alpha]
    }

    fn solid(&self, x: f64, y: f64, mask: u8, variant: u16) -> Rgb {
        let theme = self.theme;
        let mut colour = self
            .decal(x, y, variant)
            .unwrap_or_else(|| self.stone(x, y));

        let edge = distance_to(x, y, mask, false);
        // Deeper inside a wall is darker.
        #[allow(clippy::cast_possible_truncation)]
        let depth = ((edge - BEVEL) / (DEPTH - BEVEL)).clamp(0.0, 1.0) as f32;
        colour = scale(colour, 1.0 - 0.22 * depth);

        // Moss along top surfaces: open space directly above.
        if y >= CENTRE && mask & bit_across(x, y) == 0 {
            let below_surface = y - CENTRE;
            let reach = 6.0 + 5.0 * f64::from(Self::periodic(&self.moss, x, 0.0, 6.0) + 1.0);
            if below_surface < reach {
                let shade = Self::periodic(&self.moss, x, y, 10.0) * 0.5 + 0.5;
                #[allow(clippy::cast_possible_truncation)]
                let lit = (1.0 - below_surface / reach) as f32;
                colour = lerp(
                    theme.moss[0],
                    theme.moss[1],
                    (0.6 * lit + 0.4 * shade).min(1.0),
                );
            }
        }

        if edge < BEVEL {
            let facing = Self::facing(x, y, mask);
            #[allow(clippy::cast_possible_truncation)]
            let rim = (1.0 - edge / BEVEL) as f32;
            colour = scale(colour, 1.0 + 0.5 * rim * facing);
            if edge < 2.0 {
                colour = scale(colour, 0.6);
            }
        }
        colour
    }

    /// How squarely the nearest surface faces the light, -1 to 1.
    #[allow(clippy::cast_possible_truncation)]
    fn facing(x: f64, y: f64, mask: u8) -> f32 {
        let d = |dx: f64, dy: f64| distance_to(x + dx, y + dy, mask, false);
        // Outwards is where the distance to open space shrinks.
        let out = [d(-1.0, 0.0) - d(1.0, 0.0), d(0.0, -1.0) - d(0.0, 1.0)];
        let length = out[0].hypot(out[1]);
        if length <= f64::EPSILON {
            return 0.0;
        }
        ((out[0] * LIGHT[0] + out[1] * LIGHT[1]) / length) as f32
    }

    /// Staggered blocks with mortar between and a lit top edge each.
    fn stone(&self, x: f64, y: f64) -> Rgb {
        let theme = self.theme;
        let row = (y / BLOCK[1]).floor();
        let offset = if row.rem_euclid(2.0) == 0.0 {
            0.0
        } else {
            BLOCK[0] / 2.0
        };
        let along = (x + offset).rem_euclid(f64::from(TILE_PX));
        let (lx, ly) = (along % BLOCK[0], y % BLOCK[1]);
        if lx < MORTAR || ly < MORTAR {
            return theme.mortar;
        }
        #[allow(clippy::cast_possible_truncation)]
        let block = (
            (along / BLOCK[0]).floor() as i64,
            row.rem_euclid(f64::from(TILE_PX) / BLOCK[1]) as i64,
        );
        let pick = unit(theme.seed, block.0, block.1);
        let mut colour = lerp(theme.stone[0], theme.stone[1], pick);
        colour = scale(colour, 1.0 + 0.09 * Self::periodic(&self.grain, x, y, 8.0));
        if ly < MORTAR + 2.0 {
            colour = scale(colour, 1.12);
        } else if ly > BLOCK[1] - 3.0 || lx > BLOCK[0] - 3.0 {
            colour = scale(colour, 0.86);
        }
        colour
    }

    /// Details set into the solid variants, away from the tile's edges so
    /// they never break the join with a neighbour.
    fn decal(&self, x: f64, y: f64, variant: u16) -> Option<Rgb> {
        let theme = self.theme;
        let (cx, cy) = (x - CENTRE, y - CENTRE);
        match variant {
            // A cluster of crystals, lit on their upper-left faces.
            1 => [(-6.0, 4.0, 10.0), (8.0, -2.0, 13.0), (2.0, 12.0, 7.0)]
                .iter()
                .find_map(|&(ox, oy, size)| {
                    let (dx, dy) = (cx - ox, cy - oy);
                    let reach = dx.abs() + dy.abs() * 0.55;
                    (reach < size * 0.55).then(|| {
                        let lit = dx + dy < 0.0;
                        let base = if lit {
                            theme.crystal[1]
                        } else {
                            theme.crystal[0]
                        };
                        if reach > size * 0.45 {
                            scale(base, 0.7)
                        } else {
                            base
                        }
                    })
                }),
            // A darker block with a crack running down and across it. The
            // block is the one wholly inside the tile, on the second row.
            2 => {
                let wobble = f64::from(Self::periodic(&self.grain, x, y, 5.0)) * 5.0;
                let line = (cy - cx * 0.7 + wobble).abs();
                if line < 1.4 && cx.abs() < 22.0 && cy.abs() < 22.0 {
                    return Some(scale(theme.mortar, 0.8));
                }
                let inside = (MORTAR..BLOCK[0]).contains(&(x - BLOCK[0] / 2.0))
                    && (MORTAR..BLOCK[1]).contains(&(y - BLOCK[1]));
                inside.then(|| lerp(self.stone(x, y), theme.stone[2], 0.75))
            }
            // A thin seam of crystal winding through.
            3 => {
                let wave = (x / 9.0).sin() * 5.0;
                let seam = (cy - wave).abs();
                (seam < 2.2 && cx.abs() < 26.0).then(|| {
                    if seam < 0.9 {
                        theme.crystal[1]
                    } else {
                        theme.crystal[0]
                    }
                })
            }
            _ => None,
        }
    }

    /// A panel: a raised stone frame round its edge and leaded glass set
    /// back inside it, shaded under the frame and glowing towards the
    /// middle.
    #[allow(clippy::cast_possible_truncation)]
    fn panel(&self, x: f64, y: f64, mask: u8, variant: u16) -> Rgb {
        let edge = distance_to(x, y, mask, false);
        let facing = Self::facing(x, y, mask);
        if edge < FRAME {
            let rim = (1.0 - edge / FRAME) as f32;
            let grain = Self::periodic(&self.grain, x, y, 8.0);
            let frame = scale(
                self.theme.stone[1],
                0.45 * (1.0 + 0.6 * rim * facing) * (1.0 + 0.1 * grain),
            );
            return if edge < 1.5 { scale(frame, 0.6) } else { frame };
        }
        let depth = ((edge - FRAME) / (DEPTH - FRAME)).clamp(0.0, 1.0) as f32;
        let mut colour = scale(self.glass(x, y, variant), 0.8 + 0.2 * depth);
        // The frame shades the glass below its top edge; light catches the
        // glass above its bottom edge.
        if edge < FRAME + INSET {
            let rim = (1.0 - (edge - FRAME) / INSET) as f32;
            colour = scale(
                colour,
                1.0 - 0.5 * rim * facing.max(0.0) + 0.15 * rim * (-facing).max(0.0),
            );
        }
        colour
    }

    /// Leaded glass: staggered panes, with a motif in each variant kept
    /// clear of the tile's edges.
    fn glass(&self, x: f64, y: f64, variant: u16) -> Rgb {
        let theme = self.theme;
        let (cx, cy) = (x - CENTRE, y - CENTRE);
        let radius = cx.hypot(cy);
        let lead = scale(theme.mortar, 0.7);
        match variant {
            // A rose window: two rings with spokes between, and a bright
            // centre.
            1 if radius < 28.0 => {
                let angle = cy.atan2(cx).rem_euclid(TAU / 8.0);
                let spoke = radius * angle.min(TAU / 8.0 - angle).sin();
                if (radius - 27.0).abs() < LEAD
                    || (radius - 13.0).abs() < LEAD
                    || (radius > 13.0 && spoke < LEAD)
                {
                    return lead;
                }
                if radius < 13.0 {
                    return scale(theme.glass[1], 1.35);
                }
                #[allow(clippy::cast_possible_truncation)]
                let petal = (cy.atan2(cx).rem_euclid(TAU) / (TAU / 8.0)).floor() as i64;
                return scale(
                    lerp(
                        theme.glass[0],
                        theme.glass[1],
                        unit(theme.seed, petal % 2, 9),
                    ),
                    1.15,
                );
            }
            // A lit diamond pane.
            #[allow(clippy::cast_possible_truncation)]
            2 if cx.abs() + cy.abs() < 20.0 => {
                let reach = cx.abs() + cy.abs();
                return if reach > 20.0 - LEAD * 1.5 {
                    lead
                } else {
                    scale(theme.glass[1], (1.5 - 0.02 * reach) as f32)
                };
            }
            _ => {}
        }
        // Diamond panes between diagonal leads.
        let (u, v) = ((x + y) / PANE, (x - y) / PANE);
        let (lu, lv) = (u.rem_euclid(1.0), v.rem_euclid(1.0));
        let line = LEAD / (PANE * std::f64::consts::FRAC_1_SQRT_2);
        if lu < line || lv < line {
            return lead;
        }
        // Panes repeat every tile: a tile's shift moves both by `per_tile`.
        let per_tile = f64::from(TILE_PX) / PANE;
        #[allow(clippy::cast_possible_truncation)]
        let pane = (
            u.floor().rem_euclid(per_tile) as i64,
            v.floor().rem_euclid(per_tile) as i64,
        );
        // Some whole panes near the middle are broken out, showing the dark
        // behind.
        let middle = [
            (u.floor() + v.floor() + 1.0) * PANE / 2.0 - CENTRE,
            (u.floor() - v.floor()) * PANE / 2.0 - CENTRE,
        ];
        if variant == 3 && middle[0].hypot(middle[1]) < 24.0 {
            let broken = unit(theme.seed.wrapping_add(3), pane.0, pane.1) < 0.6;
            if broken {
                return scale(theme.mortar, 0.45);
            }
        }
        let pick = unit(theme.seed.wrapping_add(2), pane.0, pane.1);
        let mut colour = lerp(theme.glass[0], theme.glass[1], pick);
        colour = scale(colour, 1.0 + 0.08 * Self::periodic(&self.grain, x, y, 6.0));
        // A glint along each pane's upper-left edges.
        if lu < line + 0.1 {
            colour = scale(colour, 1.2);
        }
        colour
    }
}

/// A sample of a theme's art, for reviewing it.
///
/// Walls ('#') and scenery panels ('o', or a hue's letter in lower case
/// for a panel of that hue) are laid out from `map`, with cells outside it
/// counting as wall, and drawn at several stages of restoring
/// colour, one above the other: none, red, red and green, then all.
#[must_use]
pub fn preview(theme: &Theme, map: &[&str]) -> Canvas {
    const STAGES: [[f32; 3]; 4] = [[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0; 3]];
    let walls = tileset(theme, Kind::Walls);
    let panels = tileset(theme, Kind::Panels);
    let rows = map.len();
    let cols = map.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let cell = |c: i64, r: i64| {
        usize::try_from(r)
            .ok()
            .zip(usize::try_from(c).ok())
            .and_then(|(r, c)| map.get(r).and_then(|row| row.chars().nth(c)))
    };
    let is_wall = |c, r| cell(c, r).is_none_or(|ch| ch == '#');
    let panel_hue = |ch: char| {
        "rgbycmw"
            .chars()
            .zip(crate::light::Hue::ALL)
            .find_map(|(letter, hue)| (letter == ch).then_some(hue))
    };
    let is_panel = |c, r| cell(c, r).is_some_and(|ch| ch == 'o' || panel_hue(ch).is_some());
    #[allow(clippy::cast_possible_truncation)]
    let (width, panel) = (cols as u32 * TILE_PX, rows as u32 * TILE_PX);
    let mut canvas = Canvas::new(width, panel * 4);
    for (stage, wash) in STAGES.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let base = stage as u32 * panel;
        let backdrop = filtered(linear(theme.backdrop), *wash);
        for py in 0..panel {
            for px in 0..width {
                let mut colour = backdrop;
                for (tiles, solid, tinted) in [
                    (&panels, &is_panel as &dyn Fn(i64, i64) -> bool, true),
                    (&walls, &is_wall, false),
                ] {
                    let [r, g, b, a] = sample(tiles, solid, px, py);
                    let mut over = linear([r, g, b]);
                    if tinted {
                        let own = cell(i64::from(px / TILE_PX), i64::from(py / TILE_PX));
                        let tint = super::tint(own.and_then(panel_hue), Kind::Panels.hue_tint());
                        over = [0, 1, 2].map(|i| over[i] * tint[i]);
                    }
                    let over = filtered(over, *wash);
                    colour = [0, 1, 2].map(|i| colour[i] + (over[i] - colour[i]) * a);
                }
                let [r, g, b] = colour.map(srgb);
                canvas.set(px, base + py, [r, g, b, 1.0]);
            }
        }
    }
    canvas
}

/// The pixel of a tileset at `(px, py)` in a layout where `solid` says
/// which cells are filled: from the tile whose centre is the nearest cell
/// corner.
fn sample(tiles: &Canvas, solid: &dyn Fn(i64, i64) -> bool, px: u32, py: u32) -> [f32; 4] {
    let half = TILE_PX / 2;
    let (cx, cy) = (
        i64::from((px + half) / TILE_PX),
        i64::from((py + half) / TILE_PX),
    );
    let mask = [
        (TOP_LEFT, -1, -1),
        (TOP_RIGHT, 0, -1),
        (BOTTOM_LEFT, -1, 0),
        (BOTTOM_RIGHT, 0, 0),
    ]
    .iter()
    .filter(|&&(_, dc, dr)| solid(cx + dc, cy + dr))
    .fold(0, |mask, &(bit, ..)| mask | bit);
    let variant = super::variant_at(
        i32::try_from(cx).unwrap_or(0),
        i32::try_from(cy).unwrap_or(0),
    );
    let tile_top = u32::from(layer(mask, variant)) * TILE_PX;
    tiles.get((px + half) % TILE_PX, tile_top + (py + half) % TILE_PX)
}

/// sRGB to linear.
#[must_use]
pub fn linear(c: Rgb) -> Rgb {
    c.map(|v| {
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
}

/// Linear to sRGB.
#[must_use]
pub fn srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        themes()[0]
    }

    fn tile(canvas: &Canvas, layer: u16) -> Vec<[f32; 4]> {
        let top = u32::from(layer) * TILE_PX;
        (top..top + TILE_PX)
            .flat_map(|y| (0..TILE_PX).map(move |x| (x, y)))
            .map(|(x, y)| canvas.get(x, y))
            .collect()
    }

    #[test]
    fn committed_tilesets_are_up_to_date() {
        for (theme, kind) in themes().into_iter().flat_map(|t| Kind::ALL.map(|k| (t, k))) {
            let path = format!(
                "{}/assets/{}",
                env!("CARGO_MANIFEST_DIR"),
                tileset_path(&theme, kind)
            );
            let committed = std::fs::read(&path).unwrap_or_default();
            let painted = tileset(&theme, kind).png().unwrap_or_default();
            assert!(
                committed == painted,
                "{path} is out of date; run `just art`"
            );
        }
    }

    #[test]
    fn tiles_are_solid_where_their_cells_are() {
        for kind in Kind::ALL {
            let tiles = tileset(&theme(), kind);
            assert_eq!(tiles.height, TILE_PX * u32::from(LAYERS));
            let (inner, outer) = (TILE_PX / 4, TILE_PX * 3 / 4);
            for mask in 0..=SOLID {
                let top = u32::from(mask) * TILE_PX;
                for (bit, x, y) in [
                    (TOP_LEFT, inner, inner),
                    (TOP_RIGHT, outer, inner),
                    (BOTTOM_LEFT, inner, outer),
                    (BOTTOM_RIGHT, outer, outer),
                ] {
                    let alpha = tiles.get(x, top + y)[3];
                    assert_eq!(alpha >= 1.0, mask & bit != 0, "{kind:?} {mask}, bit {bit}");
                }
            }
        }
    }

    #[test]
    fn panels_sit_back_behind_walls() {
        // Average brightness of the filled parts of every tile.
        let brightness = |kind| {
            let tiles = tileset(&theme(), kind);
            let filled: Vec<f32> = tiles
                .pixels
                .iter()
                .filter(|p| p[3] >= 1.0)
                .map(|p| p[0] + p[1] + p[2])
                .collect();
            #[allow(clippy::cast_precision_loss)]
            let count = filled.len() as f32;
            filled.iter().sum::<f32>() / count
        };
        let (p, w) = (brightness(Kind::Panels), brightness(Kind::Walls));
        assert!(p < w * 0.75, "{p} {w}");
        // Open space around a panel is clear: no grass or shadows.
        let panels = tileset(&theme(), Kind::Panels);
        let floor = u32::from(BOTTOM_LEFT | BOTTOM_RIGHT) * TILE_PX;
        assert!((0..TILE_PX).all(|x| panels.get(x, floor + 20)[3] == 0.0));
    }

    #[test]
    fn neighbouring_tiles_join_up() {
        // Surfaces only run along tile centre lines, so seams depend on the
        // surface pattern repeating exactly every tile.
        let theme = theme();
        let painter = Painter::new(&theme);
        let period = f64::from(TILE_PX);
        for (x, y) in [(0.5, 0.5), (13.5, 71.5), (79.5, 40.5), (40.5, 79.5)] {
            for (dx, dy) in [(period, 0.0), (0.0, period), (-period, period)] {
                assert_eq!(painter.stone(x, y), painter.stone(x + dx, y + dy));
                let noise = |x, y| Painter::periodic(&painter.grain, x, y, 8.0);
                assert!((noise(x, y) - noise(x + dx, y + dy)).abs() < 1e-5);
                let (glass, next) = (painter.glass(x, y, 0), painter.glass(x + dx, y + dy, 0));
                assert!((0..3).all(|i| (glass[i] - next[i]).abs() < 1e-5));
            }
        }
        // Variants keep the plain tile's border.
        for kind in Kind::ALL {
            let tiles = tileset(&theme, kind);
            let plain = tile(&tiles, SOLID.into());
            for layer in 16..LAYERS {
                let variant = tile(&tiles, layer);
                let px = TILE_PX as usize;
                for i in 0..px {
                    for at in [i, (px - 1) * px + i, i * px, i * px + px - 1] {
                        assert_eq!(plain[at], variant[at], "{kind:?} layer {layer}");
                    }
                }
            }
        }
    }

    #[test]
    fn tops_are_lit_and_undersides_shaded() {
        let tiles = tileset(&theme(), Kind::Walls);
        let brightness = |p: [f32; 4]| p[0] + p[1] + p[2];
        // A floor (bottom cells solid) and a ceiling (top cells solid).
        let floor = u32::from(BOTTOM_LEFT | BOTTOM_RIGHT) * TILE_PX;
        let ceiling = u32::from(TOP_LEFT | TOP_RIGHT) * TILE_PX;
        let lit: f32 = (0..TILE_PX)
            .map(|x| brightness(tiles.get(x, floor + 43)))
            .sum();
        let shaded: f32 = (0..TILE_PX)
            .map(|x| brightness(tiles.get(x, ceiling + 36)))
            .sum();
        assert!(lit > shaded * 1.2, "{lit} vs {shaded}");
        // Under the ceiling is a shadow; above the floor is mostly clear.
        assert!(tiles.get(10, ceiling + 42)[3] > 0.2);
        let clear = (0..TILE_PX)
            .filter(|&x| tiles.get(x, floor + 20)[3] == 0.0)
            .count();
        assert!(clear > TILE_PX as usize / 2);
    }

    #[test]
    fn previews_show_colour_returning() {
        let canvas = preview(&theme(), &["#  #", "    ", "####"]);
        assert_eq!((canvas.width, canvas.height), (4 * TILE_PX, 12 * TILE_PX));
        let saturation = |stage: u32| -> f32 {
            let panel = 3 * TILE_PX;
            (0..canvas.width)
                .map(|x| {
                    let [r, g, b, _] = canvas.get(x, stage * panel + panel - 30);
                    r.max(g).max(b) - r.min(g).min(b)
                })
                .sum()
        };
        assert!(saturation(0) < 0.01);
        assert!(saturation(3) > saturation(1));
        assert!(saturation(1) > saturation(0));
    }

    #[test]
    fn colour_conversions_round_trip() {
        for v in [0.0, 0.02, 0.5, 1.0] {
            assert!((srgb(linear([v; 3])[0]) - v).abs() < 1e-4);
        }
    }
}
