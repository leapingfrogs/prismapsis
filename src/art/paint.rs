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

/// How a theme's walls are built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallStyle {
    /// Staggered blocks with straight edges, grass on top.
    Blocks,
    /// Rough rock with worn edges, cracks and strata, glowing fungus on top
    /// and stalactites underneath.
    Rock,
    /// Veined marble blocks with gold trim on top and carved dentils
    /// underneath.
    Marble,
    /// Riveted iron plates with glowing seams, a copper rail on top and an
    /// iron lip underneath.
    Plates,
    /// Blocks of ice with snow on top and icicles underneath.
    Ice,
}

/// How a theme's scenery panels are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelStyle {
    /// A stone frame round leaded glass.
    Leaded,
    /// A rough rind round agate bands, with sparkling crystal inside.
    Geode,
    /// A gilt frame round stained glass in many colours.
    Stained,
    /// An iron frame round a grating with a furnace glowing behind it.
    Grating,
    /// An ice frame round frosted glass.
    Frost,
}

/// What a theme's walls and scenery are made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// The folder under `assets/art` its tilesets go in.
    pub name: &'static str,
    pub seed: u32,
    pub walls: WallStyle,
    pub panels: PanelStyle,
    /// Wall colours, light to dark; the darkest is kept for variants, as
    /// any pattern in the plain tile repeats with every tile.
    pub stone: [Rgb; 3],
    /// Mortar between blocks, or cracks in rock.
    pub mortar: Rgb,
    /// Moss or fungus on top surfaces, dark and light.
    pub moss: [Rgb; 2],
    /// Two kinds of crystal set into the walls, each dark and light.
    pub crystal: [[Rgb; 2]; 2],
    /// Scenery panels' glass or agate, kept pale so a panel's own hue tints
    /// it; panes and bands vary between the two.
    pub glass: [Rgb; 2],
    /// Stained glass colours, for stained panels.
    pub panes: &'static [Rgb],
    /// Behind the level.
    pub backdrop: Rgb,
}

impl Theme {
    /// How strongly a cell's hue tints this theme's `kind` of art: pale
    /// glass takes its hue strongly, stained glass only a little, so its own
    /// colours still show.
    #[must_use]
    pub const fn hue_tint(&self, kind: Kind) -> f32 {
        match (kind, self.panels) {
            (Kind::Walls, _) => 0.6,
            (Kind::Panels, PanelStyle::Stained) => 0.35,
            (Kind::Panels, _) => 0.9,
        }
    }
}

/// How many campaign levels share a theme.
pub const LEVELS_PER_THEME: usize = 3;

/// Every theme, in campaign order.
#[must_use]
pub fn themes() -> Vec<Theme> {
    let blue = [[0.25, 0.45, 0.9], [0.75, 0.9, 1.0]];
    vec![
        Theme {
            name: "quarry",
            seed: 1,
            walls: WallStyle::Blocks,
            panels: PanelStyle::Leaded,
            stone: [[0.86, 0.66, 0.46], [0.72, 0.5, 0.36], [0.6, 0.33, 0.27]],
            mortar: [0.3, 0.23, 0.22],
            moss: [[0.3, 0.55, 0.2], [0.58, 0.8, 0.3]],
            crystal: [blue, blue],
            glass: [[0.33, 0.3, 0.27], [0.42, 0.44, 0.47]],
            panes: &[],
            backdrop: [0.07, 0.07, 0.12],
        },
        Theme {
            name: "caverns",
            seed: 2,
            walls: WallStyle::Rock,
            panels: PanelStyle::Geode,
            stone: [[0.5, 0.5, 0.6], [0.36, 0.36, 0.47], [0.24, 0.22, 0.33]],
            mortar: [0.13, 0.12, 0.18],
            moss: [[0.12, 0.38, 0.4], [0.5, 1.0, 0.85]],
            crystal: [
                [[0.1, 0.6, 0.72], [0.7, 1.0, 1.0]],
                [[0.62, 0.16, 0.66], [1.0, 0.68, 1.0]],
            ],
            glass: [[0.3, 0.33, 0.38], [0.48, 0.53, 0.58]],
            panes: &[],
            backdrop: [0.03, 0.08, 0.09],
        },
        Theme {
            name: "observatory",
            seed: 3,
            walls: WallStyle::Marble,
            panels: PanelStyle::Stained,
            stone: [[0.93, 0.91, 0.88], [0.8, 0.79, 0.8], [0.56, 0.54, 0.62]],
            mortar: [0.5, 0.48, 0.54],
            moss: [[0.7, 0.5, 0.16], [1.0, 0.86, 0.42]],
            crystal: [
                [[0.7, 0.5, 0.16], [1.0, 0.86, 0.42]],
                [[0.14, 0.2, 0.58], [0.4, 0.52, 0.95]],
            ],
            glass: [[0.6, 0.55, 0.4], [0.9, 0.85, 0.65]],
            panes: &[
                [0.62, 0.14, 0.16],
                [0.72, 0.48, 0.1],
                [0.2, 0.5, 0.2],
                [0.12, 0.45, 0.5],
                [0.16, 0.22, 0.6],
                [0.42, 0.18, 0.55],
            ],
            backdrop: [0.06, 0.05, 0.16],
        },
        Theme {
            name: "foundry",
            seed: 4,
            walls: WallStyle::Plates,
            panels: PanelStyle::Grating,
            stone: [[0.44, 0.46, 0.5], [0.34, 0.36, 0.4], [0.22, 0.23, 0.27]],
            mortar: [0.12, 0.12, 0.14],
            moss: [[0.55, 0.32, 0.12], [0.95, 0.62, 0.25]],
            crystal: [
                [[0.85, 0.3, 0.05], [1.0, 0.75, 0.3]],
                [[0.1, 0.5, 0.6], [0.5, 0.95, 1.0]],
            ],
            glass: [[0.55, 0.25, 0.1], [0.95, 0.6, 0.3]],
            panes: &[],
            backdrop: [0.07, 0.05, 0.05],
        },
        Theme {
            name: "glacier",
            seed: 5,
            walls: WallStyle::Ice,
            panels: PanelStyle::Frost,
            stone: [[0.66, 0.8, 0.9], [0.46, 0.6, 0.74], [0.3, 0.4, 0.54]],
            mortar: [0.3, 0.42, 0.55],
            moss: [[0.82, 0.87, 0.92], [0.98, 0.99, 1.0]],
            crystal: [
                [[0.2, 0.55, 0.75], [0.6, 0.9, 1.0]],
                [[0.45, 0.3, 0.75], [0.8, 0.7, 1.0]],
            ],
            glass: [[0.3, 0.36, 0.44], [0.52, 0.6, 0.68]],
            panes: &[],
            backdrop: [0.03, 0.06, 0.12],
        },
    ]
}

/// The theme for a level at `position` in the campaign: a few levels each,
/// staying on the last theme after they run out.
#[must_use]
pub fn theme_index(position: usize) -> usize {
    (position / LEVELS_PER_THEME).min(themes().len() - 1)
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
/// Depth of the agate bands inside a geode.
const AGATE: f64 = 22.0;
/// Depth of the gold trim along marble tops.
const TRIM: f64 = 4.0;
/// Glass panes are diamonds; this is the spacing of their diagonal leads
/// along each axis, which must divide the tile so panes repeat every tile.
const PANE: f64 = 20.0;
const LEAD: f64 = 1.2;

struct Painter<'a> {
    theme: &'a Theme,
    grain: Fbm<Perlin>,
    moss: Fbm<Perlin>,
    /// Rock's worn edges and cracks.
    wear: Fbm<Perlin>,
}

/// Where crystals sit in a cluster, as (x, y, size) from the tile centre.
const CLUSTER: [(f64, f64, f64); 3] = [(-6.0, 4.0, 10.0), (8.0, -2.0, 13.0), (2.0, 12.0, 7.0)];
const CLUSTER_MIRRORED: [(f64, f64, f64); 4] = [
    (7.0, 5.0, 12.0),
    (-8.0, -3.0, 14.0),
    (-1.0, 13.0, 8.0),
    (12.0, -10.0, 7.0),
];

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

const fn translucent([r, g, b]: Rgb, alpha: f32) -> [f32; 4] {
    [r, g, b, alpha]
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
            wear: Fbm::<Perlin>::new(theme.seed.wrapping_add(2)).set_octaves(3),
        }
    }

    /// How far a rough edge is worn back from the cell's edge, in pixels.
    fn worn(&self, x: f64, y: f64, rough: bool) -> f64 {
        if rough {
            f64::from(Self::periodic(&self.wear, x, y, 9.0) + 0.6).max(0.0) * 7.0
        } else {
            0.0
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
                let rough = match kind {
                    Kind::Walls => self.theme.walls == WallStyle::Rock,
                    Kind::Panels => self.theme.panels == PanelStyle::Geode,
                };
                // Distance in from the (worn) edge; open space is below zero.
                let edge = if mask & quarter_bit(x, y) == 0 {
                    -1.0
                } else {
                    distance_to(x, y, mask, false) - self.worn(x, y, rough)
                };
                let pixel = match (kind, edge >= 0.0) {
                    (Kind::Walls, false) => self.open(x, y, mask),
                    (Kind::Walls, true) => opaque(self.solid(x, y, mask, edge, variant)),
                    (Kind::Panels, false) => [0.0; 4],
                    (Kind::Panels, true) => opaque(self.panel(x, y, mask, edge, variant)),
                };
                canvas.set(px, top + py, pixel);
            }
        }
    }

    /// Open space beside a wall, by the theme's style.
    fn open(&self, x: f64, y: f64, mask: u8) -> [f32; 4] {
        match self.theme.walls {
            WallStyle::Blocks => self.grass_and_shadow(x, y, mask),
            WallStyle::Rock => self.fungus_and_stalactites(x, y, mask),
            WallStyle::Marble => self.dentils_and_shadow(x, y, mask),
            WallStyle::Plates => self.lip_and_shadow(x, y, mask),
            WallStyle::Ice => self.icicles(x, y, mask),
        }
    }

    /// Under a ledge: a dark iron lip with rivets, then a soft shadow. Above
    /// a floor, nothing.
    #[allow(clippy::cast_possible_truncation)]
    fn lip_and_shadow(&self, x: f64, y: f64, mask: u8) -> [f32; 4] {
        if mask & bit_across(x, y) == 0 || y < CENTRE {
            return [0.0; 4];
        }
        let down = y - CENTRE;
        if down < 4.0 {
            let rivet = (x.rem_euclid(16.0) - 8.0).hypot(down - 2.0) < 1.3;
            let shade = if rivet {
                0.9
            } else {
                0.55 - 0.05 * down as f32
            };
            return opaque(scale(self.theme.stone[2], shade));
        }
        let depth = ((down - 4.0) / 16.0).min(1.0);
        [0.0, 0.0, 0.0, (0.5 * (1.0 - depth).powi(2)) as f32]
    }

    /// Icicles hanging from a ledge, clear at their tips, then a faint
    /// shadow. Above a floor, nothing.
    #[allow(clippy::cast_possible_truncation)]
    fn icicles(&self, x: f64, y: f64, mask: u8) -> [f32; 4] {
        let theme = self.theme;
        if mask & bit_across(x, y) == 0 || y < CENTRE {
            return [0.0; 4];
        }
        let column = (x / 6.0).floor();
        let roll = f64::from(unit(theme.seed, column as i64, 17));
        let (length, width) = (3.0 + 22.0 * roll * roll, 1.2 + 1.8 * roll);
        let down = y - CENTRE;
        let dx = x - (column * 6.0 + 3.0);
        if down < length && dx.abs() < width * (1.0 - down / length) {
            let lit = if dx < 0.0 { 1.1 } else { 0.85 };
            let clear = (0.95 - 0.5 * down / length) as f32;
            return translucent(scale(theme.stone[0], lit), clear);
        }
        let depth = (down / 16.0).min(1.0);
        [0.0, 0.0, 0.0, (0.3 * (1.0 - depth).powi(2)) as f32]
    }

    /// Under a ledge: a row of carved dentils, then a soft shadow. Above a
    /// floor, nothing.
    #[allow(clippy::cast_possible_truncation)]
    fn dentils_and_shadow(&self, x: f64, y: f64, mask: u8) -> [f32; 4] {
        if mask & bit_across(x, y) == 0 || y < CENTRE {
            return [0.0; 4];
        }
        let down = y - CENTRE;
        if down < 5.0 && x.rem_euclid(10.0) < 6.0 {
            let lit = if x.rem_euclid(10.0) < 1.5 { 1.1 } else { 0.85 };
            return opaque(scale(
                self.theme.stone[1],
                lit * (0.95 - 0.04 * down as f32),
            ));
        }
        let depth = (down / 16.0).min(1.0);
        [0.0, 0.0, 0.0, (0.5 * (1.0 - depth).powi(2)) as f32]
    }

    /// Glowing fungus growing up from a floor, or stalactites and shadow
    /// under a ledge. Rock is worn back from its cell, so both start a
    /// little inside it.
    #[allow(clippy::cast_possible_truncation)]
    fn fungus_and_stalactites(&self, x: f64, y: f64, mask: u8) -> [f32; 4] {
        let theme = self.theme;
        let solid = |y: f64| mask & quarter_bit(x, y) != 0;
        let (above, below) = (solid(CENTRE - 1.0), solid(CENTRE + 1.0));
        if below && !above && y < CENTRE + 4.0 {
            // A few mushrooms along the floor, a stem and a glowing cap.
            let column = (x / 10.0).floor();
            let roll = f64::from(unit(theme.seed, column as i64, 11));
            if roll > 0.5 {
                return [0.0; 4];
            }
            let (height, cap) = (3.0 + 16.0 * roll, 2.0 + 5.0 * roll);
            let up = CENTRE + 3.0 - y;
            let dx = x - (column * 10.0 + 5.0);
            let reach = dx.hypot(up - height);
            if reach < cap && up >= height - 1.0 {
                return opaque(theme.moss[1]);
            }
            if dx.abs() < 1.0 && (0.0..height).contains(&up) {
                return opaque(lerp(theme.moss[0], theme.moss[1], 0.4));
            }
            if reach < cap + 4.0 {
                return translucent(theme.moss[1], (0.3 * (1.0 - (reach - cap) / 4.0)) as f32);
            }
            return [0.0; 4];
        }
        if above && !below && y > CENTRE - 6.0 {
            // Stalactites hanging from the ledge, lit on their left.
            let column = (x / 8.0).floor();
            let roll = f64::from(unit(theme.seed, column as i64, 13));
            let (length, width) = (4.0 + 24.0 * roll * roll, 1.5 + 2.5 * roll);
            let down = y - (CENTRE - 5.0);
            let dx = x - (column * 8.0 + 4.0);
            if down < length && dx.abs() < width * (1.0 - down / length) {
                let fade = (0.95 - 0.4 * down / length) as f32;
                let lit = if dx < 0.0 { 1.15 } else { 0.85 };
                return opaque(scale(theme.stone[1], fade * lit));
            }
            let depth = ((y - CENTRE) / 16.0).clamp(0.0, 1.0);
            return [0.0, 0.0, 0.0, (0.5 * (1.0 - depth).powi(2)) as f32];
        }
        [0.0; 4]
    }

    /// Open space: grass poking up from a floor below, or the shadow under a
    /// ledge above. Otherwise clear.
    fn grass_and_shadow(&self, x: f64, y: f64, mask: u8) -> [f32; 4] {
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

    /// A wall pixel `edge` pixels in from its surface.
    fn solid(&self, x: f64, y: f64, mask: u8, edge: f64, variant: u16) -> Rgb {
        let theme = self.theme;
        let rock = theme.walls == WallStyle::Rock;
        let mut colour = self
            .decal(x, y, variant)
            .unwrap_or_else(|| match theme.walls {
                WallStyle::Blocks => self.stone(x, y),
                WallStyle::Rock => self.rock(x, y),
                WallStyle::Marble => self.marble(x, y),
                WallStyle::Plates => self.plates(x, y),
                WallStyle::Ice => self.ice(x, y),
            });

        // Deeper inside a wall is darker.
        #[allow(clippy::cast_possible_truncation)]
        let depth = ((edge - BEVEL) / (DEPTH - BEVEL)).clamp(0.0, 1.0) as f32;
        colour = scale(colour, 1.0 - 0.22 * depth);

        let top = y >= CENTRE && mask & bit_across(x, y) == 0;
        if top && matches!(theme.walls, WallStyle::Marble | WallStyle::Plates) {
            // A band of gold trim along top surfaces, brightest at the edge.
            let below_surface = y - CENTRE;
            if below_surface < TRIM {
                #[allow(clippy::cast_possible_truncation)]
                let lit = (1.0 - below_surface / TRIM) as f32;
                colour = lerp(theme.moss[0], theme.moss[1], lit);
            } else if below_surface < TRIM + 1.0 {
                colour = scale(theme.moss[0], 0.6);
            }
        } else if top {
            // Moss along top surfaces: open space directly above.
            let below_surface = if rock {
                edge.max(y - CENTRE)
            } else {
                y - CENTRE
            };
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
    #[allow(clippy::cast_possible_truncation)]
    fn decal(&self, x: f64, y: f64, variant: u16) -> Option<Rgb> {
        let theme = self.theme;
        let (cx, cy) = (x - CENTRE, y - CENTRE);
        match (theme.walls, variant) {
            // A porthole: a ring of rivets round dark glass with the
            // furnace glowing through it.
            (WallStyle::Plates, 1) => {
                let radius = cx.hypot(cy);
                if radius < 12.0 {
                    let glow = (1.0 - radius / 12.0) as f32;
                    Some(lerp(
                        scale(theme.mortar, 1.5),
                        theme.crystal[0][1],
                        glow * glow,
                    ))
                } else if radius < 17.0 {
                    let angle = cy.atan2(cx).rem_euclid(TAU / 8.0) - TAU / 16.0;
                    let rivet = (radius - 14.5).abs() < 1.6 && (angle * radius).abs() < 1.6;
                    Some(scale(theme.stone[2], if rivet { 1.6 } else { 0.9 }))
                } else {
                    None
                }
            }
            // A vent: slots with the furnace glowing behind them.
            (WallStyle::Plates, 2) => {
                let inside = cx.abs() < 18.0 && cy.abs() < 14.0;
                inside.then(|| {
                    if (cy + 14.0).rem_euclid(7.0) < 3.0 {
                        lerp(
                            theme.crystal[0][0],
                            theme.crystal[0][1],
                            (1.0 - cx.abs() / 18.0) as f32,
                        )
                    } else {
                        scale(theme.stone[2], 0.7)
                    }
                })
            }
            // A gold rosette round a lapis centre.
            (WallStyle::Marble, 1) => {
                let radius = cx.hypot(cy);
                let petals = (cy.atan2(cx) * 8.0).cos() * 3.0;
                if radius < 6.0 {
                    Some(theme.crystal[1][usize::from(radius < 3.0)])
                } else if radius < 14.0 + petals {
                    Some(lerp(
                        theme.crystal[0][0],
                        theme.crystal[0][1],
                        0.5 + 0.5 * (-cx - cy) as f32 / 14.0,
                    ))
                } else {
                    None
                }
            }
            // A lapis diamond inlaid in a gold border.
            (WallStyle::Marble, 2) => {
                let reach = cx.abs() + cy.abs();
                if reach < 13.0 {
                    Some(theme.crystal[1][usize::from(cx + cy < 0.0)])
                } else if reach < 16.0 {
                    Some(theme.crystal[0][1])
                } else {
                    None
                }
            }
            (_, 1) => Self::cluster(cx, cy, theme.crystal[0], &CLUSTER),
            (WallStyle::Rock | WallStyle::Ice, 2) => {
                Self::cluster(cx, cy, theme.crystal[1], &CLUSTER_MIRRORED)
            }
            // A darker block with a crack running down and across it. The
            // block is the one wholly inside the tile, on the second row.
            (WallStyle::Blocks, 2) => {
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
            (_, 3) => {
                let crystal = theme.crystal[usize::from(theme.walls == WallStyle::Rock)];
                let wave = (x / 9.0).sin() * 5.0;
                let seam = (cy - wave).abs();
                (seam < 2.2 && cx.abs() < 26.0)
                    .then(|| if seam < 0.9 { crystal[1] } else { crystal[0] })
            }
            _ => None,
        }
    }

    /// A cluster of crystals at `spots`, lit on their upper-left faces.
    fn cluster(cx: f64, cy: f64, crystal: [Rgb; 2], spots: &[(f64, f64, f64)]) -> Option<Rgb> {
        spots.iter().find_map(|&(ox, oy, size)| {
            let (dx, dy) = (cx - ox, cy - oy);
            let reach = dx.abs() + dy.abs() * 0.55;
            (reach < size * 0.55).then(|| {
                let base = if dx + dy < 0.0 {
                    crystal[1]
                } else {
                    crystal[0]
                };
                if reach > size * 0.45 {
                    scale(base, 0.7)
                } else {
                    base
                }
            })
        })
    }

    /// Iron plates, staggered and joined by dark seams, with a
    /// rivet in each corner lit from the top left and a brushed grain.
    #[allow(clippy::cast_possible_truncation)]
    fn plates(&self, x: f64, y: f64) -> Rgb {
        let theme = self.theme;
        let size = f64::from(TILE_PX) / 2.0;
        // Plates are a tile wide and half a tile tall, staggered by row.
        let row = (y / size).floor();
        let shift = if row.rem_euclid(2.0) == 0.0 {
            0.0
        } else {
            size
        };
        let wide = f64::from(TILE_PX);
        let (lx, ly) = ((x + shift).rem_euclid(wide), y.rem_euclid(size));
        if lx < 1.5 || ly < 1.5 {
            return theme.mortar;
        }
        let plate = (((x + shift) / wide).floor() as i64, row as i64);
        let mut colour = lerp(
            theme.stone[0],
            theme.stone[1],
            unit(theme.seed, plate.0.rem_euclid(2), plate.1.rem_euclid(2)),
        );
        // Brushed: fine streaks running across.
        colour = scale(
            colour,
            1.0 + 0.06 * Self::periodic(&self.grain, x * 0.2, y * 3.0, 6.0),
        );
        for (rx, ry) in [
            (6.0, 6.0),
            (wide - 6.0, 6.0),
            (6.0, size - 6.0),
            (wide - 6.0, size - 6.0),
        ] {
            let (dx, dy) = (lx - rx, ly - ry);
            if dx.hypot(dy) < 2.2 {
                let lit = (dx * LIGHT[0] + dy * LIGHT[1]) / 2.2;
                return scale(theme.stone[0], (1.0 + 0.4 * lit) as f32);
            }
        }
        // Each plate is lit along its top and left, shaded at its bottom.
        if ly < 3.5 || lx < 3.5 {
            colour = scale(colour, 1.1);
        } else if ly > size - 3.0 || lx > wide - 3.0 {
            colour = scale(colour, 0.82);
        }
        colour
    }

    /// Ice: big blocks with soft joints, clouded depths and fine
    /// fractures, brighter towards each block's top.
    #[allow(clippy::cast_possible_truncation)]
    fn ice(&self, x: f64, y: f64) -> Rgb {
        let theme = self.theme;
        let row = (y / 40.0).floor();
        let offset = if row.rem_euclid(2.0) == 0.0 {
            0.0
        } else {
            40.0
        };
        let along = (x + offset).rem_euclid(f64::from(TILE_PX));
        let (lx, ly) = (along % 80.0, y % 40.0);
        if lx < 1.2 || ly < 1.2 {
            return lerp(theme.stone[1], theme.mortar, 0.5);
        }
        let cloud = Self::periodic(&self.grain, x, y, 4.0) * 0.5 + 0.5;
        let mut colour = lerp(theme.stone[1], theme.stone[0], 0.3 + 0.5 * cloud);
        // Fractures: bright where they catch the light.
        if Self::periodic(&self.wear, x, y, 3.0).abs() < 0.012 {
            colour = lerp(colour, theme.moss[1], 0.6);
        }
        scale(colour, 1.08 - 0.12 * (ly / 40.0) as f32)
    }

    /// Polished marble: tall staggered blocks with fine joints, faint veins,
    /// and a sheen towards each block's top.
    #[allow(clippy::cast_possible_truncation)]
    fn marble(&self, x: f64, y: f64) -> Rgb {
        let theme = self.theme;
        let row = (y / 40.0).floor();
        let offset = if row.rem_euclid(2.0) == 0.0 {
            0.0
        } else {
            20.0
        };
        let along = (x + offset).rem_euclid(f64::from(TILE_PX));
        let (lx, ly) = (along % 40.0, y % 40.0);
        if lx < 1.5 || ly < 1.5 {
            return theme.mortar;
        }
        let block = ((along / 40.0).floor() as i64, row.rem_euclid(2.0) as i64);
        let mut colour = lerp(
            theme.stone[0],
            theme.stone[1],
            0.6 * unit(theme.seed, block.0, block.1),
        );
        let vein = Self::periodic(&self.wear, x, y, 3.0).abs();
        if vein < 0.025 {
            colour = lerp(colour, theme.stone[2], 0.35 * (1.0 - vein / 0.025));
        }
        scale(colour, 1.06 - 0.08 * (ly / 40.0) as f32)
    }

    /// Rough rock: strata warped by noise, grain, and a network of cracks.
    #[allow(clippy::cast_possible_truncation)]
    fn rock(&self, x: f64, y: f64) -> Rgb {
        let theme = self.theme;
        if Self::periodic(&self.moss, x, y, 3.0).abs() < 0.012 {
            return theme.mortar;
        }
        // Four bands a tile, so they repeat with it.
        let warp = f64::from(Self::periodic(&self.grain, x, y, 3.0)) * 12.0;
        let band = ((y + warp) / f64::from(TILE_PX) * TAU * 4.0).sin() as f32;
        let mut colour = lerp(theme.stone[1], theme.stone[0], 0.5 + 0.35 * band);
        colour = scale(colour, 1.0 + 0.14 * Self::periodic(&self.grain, x, y, 14.0));
        colour
    }

    /// A panel: a raised stone frame round its edge and leaded glass set
    /// back inside it, shaded under the frame and glowing towards the
    /// middle.
    #[allow(clippy::cast_possible_truncation)]
    fn panel(&self, x: f64, y: f64, mask: u8, edge: f64, variant: u16) -> Rgb {
        let facing = Self::facing(x, y, mask);
        if edge < FRAME {
            let rim = (1.0 - edge / FRAME) as f32;
            let grain = Self::periodic(&self.grain, x, y, 8.0);
            let body = match self.theme.panels {
                PanelStyle::Leaded | PanelStyle::Grating | PanelStyle::Frost => self.theme.stone[1],
                PanelStyle::Geode => self.rock(x, y),
                PanelStyle::Stained => scale(self.theme.moss[1], 1.6),
            };
            let frame = scale(
                body,
                0.45 * (1.0 + 0.6 * rim * facing) * (1.0 + 0.1 * grain),
            );
            return if edge < 1.5 { scale(frame, 0.6) } else { frame };
        }
        let depth = ((edge - FRAME) / (DEPTH - FRAME)).clamp(0.0, 1.0) as f32;
        let inside = match self.theme.panels {
            PanelStyle::Leaded => self.glass(x, y, variant),
            PanelStyle::Geode => self.geode(x, y, edge - FRAME, variant),
            PanelStyle::Stained => self.stained(x, y, variant),
            PanelStyle::Grating => self.grating(x, y, variant),
            PanelStyle::Frost => self.frost(x, y, variant),
        };
        let mut colour = scale(inside, 0.8 + 0.2 * depth);
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

    /// Inside a geode, `inner` pixels in from its rind: agate bands that
    /// follow the panel's shape, then sparkling crystal in the hollow, with
    /// a motif in each variant kept clear of the tile's edges.
    #[allow(clippy::cast_possible_truncation)]
    fn geode(&self, x: f64, y: f64, inner: f64, variant: u16) -> Rgb {
        let theme = self.theme;
        let (cx, cy) = (x - CENTRE, y - CENTRE);
        let radius = cx.hypot(cy);
        let wobble = f64::from(Self::periodic(&self.wear, x, y, 6.0)) * 2.0;
        let agate = |depth: f64| {
            let band = (0.5 + 0.5 * ((depth + wobble) * 0.55).sin()) as f32;
            scale(
                lerp(
                    scale(theme.glass[0], 0.65),
                    scale(theme.glass[1], 1.2),
                    band,
                ),
                1.0 - (0.25 * depth / AGATE) as f32,
            )
        };
        if inner < AGATE {
            return agate(inner);
        }
        match variant {
            // A cluster of bright crystal points.
            1 => {
                if let Some(c) = Self::cluster(cx, cy, theme.glass, &CLUSTER) {
                    return scale(c, 1.4);
                }
            }
            // A ring of agate round a knot in the middle.
            2 if radius < 24.0 => return agate(24.0 - radius),
            // A dark hollow with a lit lower rim.
            3 if radius < 14.0 => {
                let rim = if radius > 12.0 && cy > 0.0 { 2.5 } else { 0.5 };
                return scale(theme.mortar, rim);
            }
            _ => {}
        }
        // The hollow: dim crystal, softly shaded, with a few glints.
        let (column, row) = ((x / 8.0).floor(), (y / 8.0).floor());
        #[allow(clippy::cast_possible_truncation)]
        let roll = unit(
            theme.seed.wrapping_add(4),
            column.rem_euclid(10.0) as i64,
            row.rem_euclid(10.0) as i64,
        );
        let (dx, dy) = (x - column * 8.0 - 4.0, y - row * 8.0 - 4.0);
        if roll > 0.94 && dx.abs() * dy.abs() < 1.0 && dx.abs() + dy.abs() < 3.5 {
            return scale(theme.glass[1], 1.5);
        }
        let shade = Self::periodic(&self.grain, x, y, 5.0) * 0.5 + 0.5;
        lerp(
            scale(theme.glass[0], 0.5),
            scale(theme.glass[1], 0.7),
            shade,
        )
    }

    /// Stained glass: irregular panes in the theme's colours between leads,
    /// with a rose window, a star or a crescent moon in the variants.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn stained(&self, x: f64, y: f64, variant: u16) -> Rgb {
        let theme = self.theme;
        let (cx, cy) = (x - CENTRE, y - CENTRE);
        let radius = cx.hypot(cy);
        let lead = scale(theme.mortar, 0.35);
        let pane = |pick: f32| {
            let i = (pick * theme.panes.len() as f32) as usize;
            theme.panes[i.min(theme.panes.len() - 1)]
        };
        match variant {
            // A rose window: a ring of petals round a gold centre.
            1 if radius < 28.0 => {
                let angle = cy.atan2(cx).rem_euclid(TAU);
                let petal = (angle / (TAU / 12.0)).floor();
                let within = angle - (petal + 0.5) * TAU / 12.0;
                if (radius - 27.0).abs() < LEAD || (radius - 11.0).abs() < LEAD {
                    return lead;
                }
                if radius < 11.0 {
                    return theme.glass[1];
                }
                if (radius * within.sin()).abs() > radius * (TAU / 24.0).sin() - LEAD {
                    return lead;
                }
                return pane(unit(theme.seed, petal as i64 % 3, 5));
            }
            // An eight-pointed gold star.
            2 => {
                let angle = cy.atan2(cx);
                let point = 7.0 + 15.0 * (angle * 4.0).cos().abs().powi(6);
                if radius < point - LEAD {
                    return lerp(
                        theme.glass[0],
                        theme.glass[1],
                        (1.0 - radius / point) as f32,
                    );
                }
                if radius < point + LEAD * 0.5 {
                    return lead;
                }
            }
            // A pale crescent moon.
            3 if radius < 17.0 => {
                let shadow = (cx - 7.0).hypot(cy + 4.0);
                if radius > 15.5 || (shadow - 14.0).abs() < LEAD {
                    return lead;
                }
                if shadow > 14.0 {
                    return theme.glass[1];
                }
            }
            _ => {}
        }
        // Panes are the cells round jittered points, four by four a tile,
        // wrapping so they repeat with it.
        let (gx, gy) = ((x / 20.0).floor(), (y / 20.0).floor());
        let mut near = [(f64::MAX, (0, 0)); 2];
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cell_x, cell_y) = (gx + f64::from(dx), gy + f64::from(dy));
                let id = (cell_x.rem_euclid(4.0) as i64, cell_y.rem_euclid(4.0) as i64);
                let jitter = |salt| 3.0 + 14.0 * f64::from(unit(theme.seed, id.0 * 7 + salt, id.1));
                let (px, py) = (cell_x * 20.0 + jitter(1), cell_y * 20.0 + jitter(2));
                let d = (x - px).hypot(y - py);
                if d < near[0].0 {
                    near[1] = near[0];
                    near[0] = (d, id);
                } else if d < near[1].0 {
                    near[1] = (d, id);
                }
            }
        }
        if near[1].0 - near[0].0 < LEAD * 1.4 {
            return lead;
        }
        let (id_x, id_y) = near[0].1;
        let mut colour = pane(unit(theme.seed.wrapping_add(6), id_x, id_y));
        colour = scale(colour, 1.0 + 0.12 * Self::periodic(&self.grain, x, y, 6.0));
        // Glass is brighter at each pane's heart.
        scale(colour, 1.15 - 0.02 * near[0].0.min(10.0) as f32)
    }

    /// A grating of crossed iron bars with a furnace glowing behind, a cog
    /// or a fan in the variants, and bars missing in the last.
    #[allow(clippy::cast_possible_truncation)]
    fn grating(&self, x: f64, y: f64, variant: u16) -> Rgb {
        let theme = self.theme;
        let (cx, cy) = (x - CENTRE, y - CENTRE);
        let radius = cx.hypot(cy);
        let flicker = Self::periodic(&self.grain, x, y, 3.0) * 0.5 + 0.5;
        let glow = scale(
            lerp(theme.glass[0], theme.glass[1], 0.4 + 0.6 * flicker),
            0.6,
        );
        let iron = scale(theme.stone[2], 0.6);
        match variant {
            // A cog: teeth round a hub with a hole.
            1 if radius < 26.0 => {
                let teeth = if (cy.atan2(cx) * 10.0).sin() > 0.0 {
                    25.0
                } else {
                    20.0
                };
                if radius < 5.0 {
                    return glow;
                }
                if radius < teeth {
                    let lit = (cx * LIGHT[0] + cy * LIGHT[1]) / radius.max(1.0);
                    return scale(iron, (1.1 + 0.3 * lit) as f32);
                }
            }
            // A fan: four curved blades in a ring.
            2 if radius < 27.0 => {
                if radius > 24.0 || radius < 4.0 {
                    return iron;
                }
                let blade = (cy.atan2(cx) * 2.0 + radius * 0.08).sin();
                if blade > 0.35 {
                    return scale(iron, 1.2);
                }
                return glow;
            }
            _ => {}
        }
        // Bars every 10 pixels each way, lit along their upper-left edges;
        // the last variant has some missing near the middle.
        let (bx, by) = (x.rem_euclid(10.0), y.rem_euclid(10.0));
        let missing = variant == 3 && radius < 20.0 && unit(theme.seed, (x / 10.0) as i64, 1) < 0.5;
        if !missing && (bx < 3.0 || by < 3.0) {
            let lit = if bx < 1.0 || by < 1.0 { 1.35 } else { 1.0 };
            return scale(iron, lit);
        }
        glow
    }

    /// Frosted glass: pale panes with ferns of frost spreading over them, a
    /// snowflake, a clear patch or a crack in the variants.
    #[allow(clippy::cast_possible_truncation)]
    fn frost(&self, x: f64, y: f64, variant: u16) -> Rgb {
        let theme = self.theme;
        let (cx, cy) = (x - CENTRE, y - CENTRE);
        let radius = cx.hypot(cy);
        let fern = Self::periodic(&self.wear, x, y, 7.0).abs();
        let frosted = 1.0 - (fern / 0.12).min(1.0);
        let mut colour = lerp(theme.glass[0], theme.glass[1], 0.35 + 0.6 * frosted);
        match variant {
            // A six-armed snowflake with side branches.
            1 if radius < 24.0 => {
                let angle = cy.atan2(cx).rem_euclid(TAU / 6.0);
                let arm = radius * angle.min(TAU / 6.0 - angle).sin();
                let branch = ((radius - 12.0).abs() < 6.0)
                    && (arm - (radius - 12.0).abs() * 0.6).abs() < 1.2;
                if arm < 1.3 || branch || radius < 3.0 {
                    colour = scale(theme.glass[1], 1.25);
                }
            }
            // A clear patch, rubbed free of frost.
            2 if radius < 18.0 => {
                colour = lerp(
                    scale(theme.glass[0], 0.8),
                    colour,
                    (radius / 18.0).powi(3) as f32,
                );
            }
            // A crack across the pane.
            3 => {
                let wobble = f64::from(Self::periodic(&self.grain, x, y, 5.0)) * 4.0;
                if (cy + cx * 0.4 + wobble).abs() < 0.9 && cx.abs() < 26.0 {
                    colour = scale(theme.mortar, 1.2);
                }
            }
            _ => {}
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
/// Walls ('#'), scenery panels ('o', or a hue's letter in lower case for a
/// panel of that hue) and features (the default legend's '=', '^', '~'
/// and '%') are laid out from `map`, with cells outside it counting as
/// wall, and drawn at several stages of restoring colour, one above the
/// other: none, red, red and green, then all.
#[must_use]
pub fn preview(theme: &Theme, map: &[&str]) -> Canvas {
    const STAGES: [[f32; 3]; 4] = [[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0; 3]];
    let walls = tileset(theme, Kind::Walls);
    let panels = tileset(theme, Kind::Panels);
    let features = super::features::tileset(theme);
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
    let feature_at = |c, r| {
        use super::features::Feature;
        match cell(c, r)? {
            '=' => Some(Feature::Platform),
            '^' => Some(Feature::Spring),
            '~' => Some(Feature::Mud),
            '%' => Some(Feature::Mirror),
            _ => None,
        }
    };
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
                        let tint =
                            super::tint(own.and_then(panel_hue), theme.hue_tint(Kind::Panels));
                        over = [0, 1, 2].map(|i| over[i] * tint[i]);
                    }
                    let over = filtered(over, *wash);
                    colour = [0, 1, 2].map(|i| colour[i] + (over[i] - colour[i]) * a);
                }
                let (col, row) = (i64::from(px / TILE_PX), i64::from(py / TILE_PX));
                if let Some(feature) = feature_at(col, row) {
                    use super::features::{ABOVE, BELOW, LEFT, RIGHT};
                    let mask = [(ABOVE, 0, -1), (BELOW, 0, 1), (LEFT, -1, 0), (RIGHT, 1, 0)]
                        .iter()
                        .filter(|&&(_, dc, dr)| feature_at(col + dc, row + dr) == Some(feature))
                        .fold(0, |mask, &(bit, ..)| mask | bit);
                    let top = u32::from(feature.layer(mask)) * TILE_PX;
                    let [r, g, b, a] = features.get(px % TILE_PX, top + py % TILE_PX);
                    let over = filtered(linear([r, g, b]), *wash);
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
        for theme in themes() {
            panels_sit_back_in(&theme);
        }
    }

    fn panels_sit_back_in(theme: &Theme) {
        // Average brightness of the filled parts of every tile.
        let brightness = |kind| {
            let tiles = tileset(theme, kind);
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
        assert!(p < w * 0.75, "{}: {p} {w}", theme.name);
        // Open space around a panel is clear: no grass or shadows.
        let panels = tileset(theme, Kind::Panels);
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
    fn themes_last_a_few_levels_each_in_order() {
        let positions: Vec<usize> = (0..7).map(theme_index).collect();
        assert_eq!(positions, [0, 0, 0, 1, 1, 1, 2.min(themes().len() - 1)]);
        assert_eq!(theme_index(100), themes().len() - 1);
        let names: Vec<_> = themes().iter().map(|t| t.name).collect();
        assert_eq!(&names[..2], ["quarry", "caverns"]);
    }

    #[test]
    fn marble_tops_are_trimmed_in_gold() {
        let observatory = themes()[2];
        let tiles = tileset(&observatory, Kind::Walls);
        let floor = u32::from(BOTTOM_LEFT | BOTTOM_RIGHT) * TILE_PX;
        let gold = crate::art::hue_degrees(observatory.moss[1]);
        let [r, g, b, _] = tiles.get(TILE_PX / 3, floor + TILE_PX / 2 + 2);
        assert!((crate::art::hue_degrees([r, g, b]) - gold).abs() < 10.0);
        // Deeper in, it's pale marble again.
        let [r, g, b, _] = tiles.get(TILE_PX / 3, floor + TILE_PX - 10);
        assert!(r.max(g).max(b) - r.min(g).min(b) < 0.15);
    }

    #[test]
    fn rock_is_worn_and_holds_both_crystals() {
        let caverns = themes()[1];
        let tiles = tileset(&caverns, Kind::Walls);
        // Along a floor's surface, some of the solid side is worn away.
        let floor = u32::from(BOTTOM_LEFT | BOTTOM_RIGHT) * TILE_PX;
        let worn = (0..TILE_PX)
            .filter(|&x| tiles.get(x, floor + TILE_PX / 2 + 1)[3] < 1.0)
            .count();
        assert!(worn > 5 && worn < TILE_PX as usize, "{worn} worn");
        // Variants 1 and 2 hold the two kinds of crystal.
        // Crystal is shaded like the rest of the wall, so look for strong
        // colour of its hue.
        let holds = |layer: u16, colour: Rgb| {
            let hue = crate::art::hue_degrees(colour);
            tile(&tiles, layer).iter().any(|&[r, g, b, _]| {
                let strong = r.max(g).max(b) - r.min(g).min(b) > 0.3;
                strong && (crate::art::hue_degrees([r, g, b]) - hue).abs() < 15.0
            })
        };
        assert!(holds(16, caverns.crystal[0][0]));
        assert!(holds(17, caverns.crystal[1][0]));
    }

    #[test]
    fn colour_conversions_round_trip() {
        for v in [0.0, 0.02, 0.5, 1.0] {
            assert!((srgb(linear([v; 3])[0]) - v).abs() < 1e-4);
        }
    }
}
