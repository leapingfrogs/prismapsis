//! Paints the sprite sheet for the hero, enemies, orbs, pickups and the
//! prism, offline (`just art`).
//!
//! Every figure is drawn in white and greys, so the game can tint one
//! sheet to any colour. Figures sit in equal cells, left to right and top
//! to bottom, with each animation's frames side by side. Coordinates are in
//! world units from a cell's centre, with y pointing up.

use super::paint::Canvas;
use crate::enemy::{LENS_RADIUS, MOTE_RADIUS, ORB_RADIUS};
use crate::hero::HERO_RADIUS;
use crate::powerup::{PICKUP_RADIUS, PowerUp};
use crate::prism::PRISM_RADIUS;
use bevy::math::{Rect, Vec2};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Pixels per world unit: 2, to stay sharp at 4K.
pub const PX_PER_UNIT: f32 = 2.0;

/// Width and height of a cell in pixels.
pub const CELL_PX: u32 = 128;

/// Cells across the sheet.
pub const COLUMNS: u32 = 8;

/// A cell's size in world units.
#[allow(clippy::cast_precision_loss)]
pub const CELL_SIZE: f32 = CELL_PX as f32 / PX_PER_UNIT;

/// The sheet's path under the assets folder.
pub const SHEET_PATH: &str = "art/sprites.png";

/// Something drawn from the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Figure {
    /// The hero's markings, which turn as it spins.
    HeroSurface,
    /// Shading and a highlight laid over the hero, which stay put.
    HeroShade,
    Prism,
    /// A pickup's glassy shell, tinted by its power-up.
    Bubble,
    /// The white mark inside a pickup.
    Glyph(PowerUp),
    Mote,
    Lens,
    Orb,
    /// A checkpoint's crystal lamp, lit by tinting.
    Lamp,
    /// A checkpoint's post, stretched to its height.
    Post,
    /// The sparkle when an enemy is defeated, spreading over its frames.
    Burst,
    /// A droplet of colour flying to the hero.
    Droplet,
}

impl Figure {
    /// Every figure, in sheet order.
    pub const ALL: [Self; 19] = [
        Self::HeroSurface,
        Self::HeroShade,
        Self::Prism,
        Self::Bubble,
        Self::Glyph(PowerUp::AntiGravity),
        Self::Glyph(PowerUp::RearShot),
        Self::Glyph(PowerUp::Spread),
        Self::Glyph(PowerUp::RapidFire),
        Self::Glyph(PowerUp::Piercing),
        Self::Glyph(PowerUp::AirSteering),
        Self::Glyph(PowerUp::Prism),
        Self::Glyph(PowerUp::LightNova),
        Self::Mote,
        Self::Lens,
        Self::Orb,
        Self::Lamp,
        Self::Post,
        Self::Burst,
        Self::Droplet,
    ];

    /// Animation frames.
    #[must_use]
    pub const fn frames(self) -> u32 {
        match self {
            Self::Mote => 6,
            Self::Lens | Self::Orb => 4,
            Self::Burst => BURST_FRAMES,
            _ => 1,
        }
    }

    /// The cell holding the first frame.
    #[must_use]
    pub fn first_cell(self) -> u32 {
        Self::ALL
            .iter()
            .take_while(|&&figure| figure != self)
            .map(|figure| figure.frames())
            .sum()
    }

    /// Where a frame sits in the sheet, in pixels from the top left.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn rect(self, frame: u32) -> Rect {
        let cell = self.first_cell() + frame % self.frames();
        let corner = Vec2::new((cell % COLUMNS) as f32, (cell / COLUMNS) as f32) * CELL_PX as f32;
        Rect::from_corners(corner, corner + Vec2::splat(CELL_PX as f32))
    }

    /// How a point in its cell is painted, as grey and alpha.
    fn paint(self, frame: u32, p: Vec2) -> Grey {
        #[allow(clippy::cast_precision_loss)]
        let phase = frame as f32 / self.frames() as f32;
        match self {
            Self::HeroSurface => hero_surface(p),
            Self::HeroShade => hero_shade(p),
            Self::Prism => prism(p),
            Self::Bubble => bubble(p),
            Self::Glyph(power) => glyph(power, p),
            Self::Mote => mote(p, phase),
            Self::Lens => lens(p, frame),
            Self::Orb => orb(p, phase),
            Self::Lamp => lamp(p),
            Self::Post => post(p),
            Self::Burst => burst(p, phase),
            Self::Droplet => droplet(p),
        }
    }
}

/// Cells used by all the figures.
#[must_use]
pub fn cells() -> u32 {
    Figure::ALL.iter().map(|figure| figure.frames()).sum()
}

/// The whole sheet.
#[must_use]
pub fn sheet() -> Canvas {
    let rows = cells().div_ceil(COLUMNS);
    let mut canvas = Canvas::new(COLUMNS * CELL_PX, rows * CELL_PX);
    for figure in Figure::ALL {
        for frame in 0..figure.frames() {
            let corner = figure.rect(frame).min;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (left, top) = (corner.x as u32, corner.y as u32);
            for y in 0..CELL_PX {
                for x in 0..CELL_PX {
                    let Grey(grey, alpha) = figure.paint(frame, cell_point(x, y));
                    canvas.set(left + x, top + y, [grey, grey, grey, alpha]);
                }
            }
        }
    }
    canvas
}

/// The centre of pixel `(x, y)` in a cell, in world units from its centre.
#[allow(clippy::cast_precision_loss)]
fn cell_point(x: u32, y: u32) -> Vec2 {
    let half = CELL_PX as f32 / 2.0;
    Vec2::new(x as f32 + 0.5 - half, half - y as f32 - 0.5) / PX_PER_UNIT
}

/// A grey level and an alpha, each 0 to 1 (straight alpha).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Grey(f32, f32);

impl Grey {
    /// `self` laid over `under`.
    fn over(self, under: Self) -> Self {
        let alpha = self.1 + under.1 * (1.0 - self.1);
        if alpha <= f32::EPSILON {
            // Keep a colour in clear pixels so filtering doesn't darken edges.
            return Self(self.0, 0.0);
        }
        let grey = (self.0 * self.1 + under.0 * under.1 * (1.0 - self.1)) / alpha;
        Self(grey, alpha)
    }

    /// With its alpha scaled by `by`.
    fn fade(self, by: f32) -> Self {
        Self(self.0, self.1 * by.clamp(0.0, 1.0))
    }
}

/// How much of a pixel a shape covers, from the signed distance to its edge
/// (negative inside), blurred over one pixel.
fn cover(distance: f32) -> f32 {
    (0.5 - distance * PX_PER_UNIT).clamp(0.0, 1.0)
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed distance to a regular polygon with `sides` corners on a circle of
/// `radius`, one corner pointing up.
fn polygon(p: Vec2, sides: u32, radius: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let step = TAU / sides as f32;
    let apothem = radius * (step / 2.0).cos();
    (0..sides)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let angle = FRAC_PI_2 + step * (k as f32 + 0.5);
            p.dot(Vec2::from_angle(angle)) - apothem
        })
        .fold(f32::MIN, f32::max)
}

/// Signed distance to a convex polygon with corners anticlockwise.
fn convex(p: Vec2, corners: &[Vec2]) -> f32 {
    corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .map(|(&a, &b)| {
            let outward = (b - a).perp().normalize() * -1.0;
            (p - a).dot(outward)
        })
        .fold(f32::MIN, f32::max)
}

/// Distance from `p` to the segment from `a` to `b`.
fn segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let along = (p - a).dot(b - a) / (b - a).length_squared();
    (p - a.lerp(b, along.clamp(0.0, 1.0))).length()
}

/// Light falls from the top left, towards the viewer.
const LIGHT: [f32; 3] = [-0.45, 0.6, 0.66];

fn light() -> bevy::math::Vec3 {
    bevy::math::Vec3::from_array(LIGHT).normalize()
}

/// The hero is a white ball with three grey bands swirling out from a hub,
/// so its spin shows.
fn hero_surface(p: Vec2) -> Grey {
    let radius = HERO_RADIUS;
    let r = p.length() / radius;
    let angle = p.y.atan2(p.x);
    let swirl = (3.0 * angle + 2.4 * r).sin();
    let band = smoothstep(0.2, 0.45, swirl);
    let seam = (1.0 - (swirl - 0.32).abs() * 6.0).clamp(0.0, 1.0) * 0.12;
    let mut grey = 0.97 - band * 0.24 - seam;
    // A small hub where the bands meet.
    let hub = p.length() - radius * 0.2;
    grey = grey * (1.0 - cover(hub)) + 0.66 * cover(hub);
    let rim = cover(hub.abs() - 0.6);
    grey -= rim * 0.15;
    Grey(grey, cover(p.length() - radius))
}

/// Shade and a highlight for the hero as a lit sphere: black where it
/// turns from the light, white where the light glints.
fn hero_shade(p: Vec2) -> Grey {
    let radius = HERO_RADIUS;
    // Clamped, so the shading stays defined (and clear) outside the ball.
    let flat = (p / radius).clamp_length_max(1.0);
    let depth = (1.0 - flat.length_squared()).max(0.0).sqrt();
    let normal = flat.extend(depth);
    let lit = normal.dot(light()).max(0.0);
    let shadow = Grey(0.0, (1.0 - lit).powf(1.6) * 0.62);
    let half = (light() + bevy::math::Vec3::Z).normalize();
    let facing = normal.dot(half).max(0.0);
    let glint = Grey(1.0, facing.powf(48.0) * 0.95 + facing.powf(6.0) * 0.12);
    glint.over(shadow).fade(cover(p.length() - radius))
}

/// A glass triangle with a bright rim and lines to its corners.
fn prism(p: Vec2) -> Grey {
    let radius = PRISM_RADIUS;
    let inside = polygon(p, 3, radius);
    let corners: Vec<Vec2> = (0..3)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let angle = FRAC_PI_2 + TAU / 3.0 * k as f32;
            Vec2::from_angle(angle) * radius
        })
        .collect();
    // Thicker glass shows near the edges.
    let depth = (-inside / (radius * 0.5)).clamp(0.0, 1.0);
    let glass = Grey(0.8 + 0.15 * p.y / radius, 0.78 - 0.3 * depth);
    let facets = corners
        .iter()
        .map(|&corner| segment(p, Vec2::ZERO, corner))
        .fold(f32::MAX, f32::min);
    let lines = Grey(1.0, cover(facets - 0.45) * 0.7);
    // A streak of light inside the upper left edge.
    let streak = segment(
        p,
        corners[0].lerp(corners[1], 0.18) * 0.7,
        corners[0].lerp(corners[1], 0.62) * 0.7,
    );
    let shine = Grey(1.0, cover(streak - 0.9) * 0.7);
    let rim = Grey(1.0, cover(inside.abs() - 1.0));
    rim.over(shine.over(lines.over(glass)))
        .fade(cover(inside - 0.8))
}

/// A pickup's shell: a dark glass bead in a lit rim, with a gleam.
fn bubble(p: Vec2) -> Grey {
    let radius = PICKUP_RADIUS;
    let r = p.length();
    let inner = radius * 0.82;
    let depth = Grey(0.08 + 0.12 * (-p.y / inner).max(0.0), 0.88);
    let toward = p
        .normalize_or_zero()
        .dot(Vec2::new(LIGHT[0], LIGHT[1]).normalize());
    let ring = Grey(
        0.78 + 0.22 * toward,
        cover((r - f32::midpoint(inner, radius)).abs() - (radius - inner) / 2.0),
    );
    let angle = p.y.atan2(p.x);
    let arc = smoothstep(1.7, 2.0, angle) * (1.0 - smoothstep(2.6, 2.9, angle));
    let gleam = Grey(1.0, cover((r - inner * 0.8).abs() - 1.0) * arc * 0.4);
    gleam.over(ring.over(depth)).fade(cover(r - radius))
}

/// A simple shape in a pickup's glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphPart {
    /// A triangle of this size, pointing along +x before rotation.
    Arrowhead(f32),
    /// A bar of this length and width, along +x before rotation.
    Bar(f32, f32),
    /// A ring of this outer radius.
    Ring(f32),
}

/// Line width of a glyph's bars and rings.
const STROKE: f32 = 3.0;

/// The shapes that make up each power-up's glyph, with where they sit
/// (offset and rotation in radians) inside the pickup.
#[must_use]
pub fn glyph_parts(power: PowerUp) -> Vec<(GlyphPart, Vec2, f32)> {
    let s = PICKUP_RADIUS * 0.5;
    match power {
        // An arrow rising above a floor line: hovering.
        PowerUp::AntiGravity => vec![
            (
                GlyphPart::Arrowhead(s * 0.8),
                Vec2::new(0.0, s * 0.35),
                FRAC_PI_2,
            ),
            (
                GlyphPart::Bar(s * 1.6, STROKE),
                Vec2::new(0.0, -s * 0.7),
                0.0,
            ),
        ],
        // A double-headed arrow: firing both ways.
        PowerUp::RearShot => vec![
            (GlyphPart::Bar(s * 1.2, STROKE), Vec2::ZERO, 0.0),
            (GlyphPart::Arrowhead(s * 0.6), Vec2::new(s * 0.7, 0.0), 0.0),
            (GlyphPart::Arrowhead(s * 0.6), Vec2::new(-s * 0.7, 0.0), PI),
        ],
        // Three rays fanning out.
        PowerUp::Spread => [-0.45f32, 0.0, 0.45]
            .into_iter()
            .map(|angle| {
                let offset = Vec2::from_angle(angle) * s * 0.7 - Vec2::new(s * 0.6, 0.0);
                (GlyphPart::Bar(s * 1.4, STROKE), offset, angle)
            })
            .collect(),
        // Fast-forward: two arrowheads in a row.
        PowerUp::RapidFire => vec![
            (
                GlyphPart::Arrowhead(s * 0.7),
                Vec2::new(-s * 0.35, 0.0),
                0.0,
            ),
            (GlyphPart::Arrowhead(s * 0.7), Vec2::new(s * 0.45, 0.0), 0.0),
        ],
        // A beam passing through a ring.
        PowerUp::Piercing => vec![
            (GlyphPart::Ring(s * 0.55), Vec2::ZERO, 0.0),
            (GlyphPart::Bar(s * 1.9, STROKE), Vec2::ZERO, 0.0),
        ],
        // Arrows right above and left below: turning round.
        PowerUp::AirSteering => vec![
            (
                GlyphPart::Bar(s * 1.2, STROKE),
                Vec2::new(-s * 0.2, s * 0.4),
                0.0,
            ),
            (
                GlyphPart::Arrowhead(s * 0.6),
                Vec2::new(s * 0.5, s * 0.4),
                0.0,
            ),
            (
                GlyphPart::Bar(s * 1.2, STROKE),
                Vec2::new(s * 0.2, -s * 0.4),
                0.0,
            ),
            (
                GlyphPart::Arrowhead(s * 0.6),
                Vec2::new(-s * 0.5, -s * 0.4),
                PI,
            ),
        ],
        // A ring with rays all round: a burst of light.
        PowerUp::LightNova => std::iter::once((GlyphPart::Ring(s * 0.4), Vec2::ZERO, 0.0))
            .chain((0..8u8).map(|k| {
                let angle = f32::from(k) * PI / 4.0;
                let length = if k % 2 == 0 { s * 0.5 } else { s * 0.3 };
                let offset = Vec2::from_angle(angle) * (s * 0.62 + length / 2.0);
                (GlyphPart::Bar(length, STROKE), offset, angle)
            }))
            .collect(),
        // A prism's outline: a triangle point up.
        PowerUp::Prism => {
            let r = s * 0.95;
            let side = r * 3f32.sqrt() + STROKE * 0.6;
            vec![
                (GlyphPart::Bar(side, STROKE), Vec2::new(0.0, -r * 0.5), 0.0),
                (
                    GlyphPart::Bar(side, STROKE),
                    Vec2::from_angle(PI / 6.0) * r * 0.5,
                    2.0 * PI / 3.0,
                ),
                (
                    GlyphPart::Bar(side, STROKE),
                    Vec2::from_angle(5.0 * PI / 6.0) * r * 0.5,
                    PI / 3.0,
                ),
            ]
        }
    }
}

/// Signed distance to one glyph part, in its own frame.
fn glyph_part(part: GlyphPart, p: Vec2) -> f32 {
    match part {
        GlyphPart::Arrowhead(size) => convex(
            p,
            &[
                Vec2::new(size * 0.6, 0.0),
                Vec2::new(-size * 0.4, size * 0.55),
                Vec2::new(-size * 0.4, -size * 0.55),
            ],
        ),
        GlyphPart::Bar(length, width) => {
            let outside = p.abs() - Vec2::new(length, width) / 2.0;
            outside.max(Vec2::ZERO).length() + outside.x.max(outside.y).min(0.0)
        }
        GlyphPart::Ring(radius) => (p.length() - (radius - STROKE / 2.0)).abs() - STROKE / 2.0,
    }
}

/// A power-up's mark in white, with a soft dark edge so it reads on any
/// tint.
fn glyph(power: PowerUp, p: Vec2) -> Grey {
    let distance = glyph_parts(power)
        .into_iter()
        .map(|(part, offset, angle)| glyph_part(part, Vec2::from_angle(-angle).rotate(p - offset)))
        .fold(f32::MAX, f32::min);
    let halo = Grey(0.0, (1.0 - distance / 2.0).clamp(0.0, 1.0) * 0.35);
    Grey(1.0, cover(distance)).over(halo)
}

/// A spark: a white core in a soft glow, with twinkling rays that turn a
/// quarter turn over the loop (`phase`, 0 to 1).
fn mote(p: Vec2, phase: f32) -> Grey {
    let radius = MOTE_RADIUS;
    let r = p.length();
    let glow = Grey(0.9, (-(r / (radius * 0.8)).powi(2)).exp() * 0.85);
    let mut rays = 0.0f32;
    for k in 0..8u8 {
        let long = k % 2 == 0;
        let angle = f32::from(k) * PI / 4.0 + phase * FRAC_PI_2;
        let flicker = (phase * TAU + f32::from(k) * 1.7).sin();
        let length = radius
            * if long {
                1.6 + 0.25 * flicker
            } else {
                0.95 + 0.15 * flicker
            };
        let dir = Vec2::from_angle(angle);
        let along = p.dot(dir);
        if along > 0.0 {
            let across = p.perp_dot(dir).abs();
            let fall = (1.0 - along / length).max(0.0);
            rays = rays.max((-across / 0.8).exp() * fall);
        }
    }
    let core = Grey(1.0, cover(r - radius * 0.42));
    core.over(Grey(1.0, rays).over(glow))
}

/// A hexagonal crystal turret: lit facets round a flat table, with an eye
/// at its centre. Frame 0 rests; the others sweep a glint across it.
fn lens(p: Vec2, frame: u32) -> Grey {
    let radius = LENS_RADIUS;
    let outer = polygon(p, 6, radius);
    let table = polygon(p, 6, radius * 0.55);
    // Crown facets lean out towards the edge they belong to.
    let angle = (p.x.atan2(p.y) / (TAU / 6.0)).floor() + 0.5;
    let lean = Vec2::from_angle(FRAC_PI_2 - angle * TAU / 6.0);
    let lit = lean.dot(Vec2::new(LIGHT[0], LIGHT[1]).normalize());
    let crown = 0.58 + 0.3 * lit;
    let mut grey = if table < 0.0 { 0.86 } else { crown };
    // Lines along the table's edge and out to the corners.
    let spokes = (0..6)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let dir = Vec2::from_angle(FRAC_PI_2 + TAU / 6.0 * k as f32);
            segment(p, dir * radius * 0.55, dir * radius)
        })
        .fold(f32::MAX, f32::min);
    grey += 0.12 * cover(spokes - 0.35).max(cover(table.abs() - 0.35));
    // The eye: a dark iris with a bright pupil.
    let eye = p.length();
    grey = grey * (1.0 - cover(eye - radius * 0.24)) + 0.22 * cover(eye - radius * 0.24);
    grey = grey.max(cover(eye - radius * 0.09));
    // A dark outline round the crystal.
    grey -= 0.3 * cover(outer.abs() - 0.9);
    if frame > 0 {
        #[allow(clippy::cast_precision_loss)]
        let centre = radius * (frame as f32 - 2.0) * 0.6;
        let across = (p.x + p.y) / std::f32::consts::SQRT_2 - centre;
        grey += (-(across / 3.0).powi(2)).exp() * 0.45;
    }
    Grey(grey.clamp(0.0, 1.0), cover(outer))
}

/// Frames in a burst.
pub const BURST_FRAMES: u32 = 5;

/// How far a burst's sparkle reaches at its widest, in world units from
/// the cell's centre.
pub const BURST_REACH: f32 = 30.0;

/// A checkpoint lamp: a long crystal with lit facets and a bright heart,
/// held in a small cradle on top of its post. Drawn in the top half of the
/// cell, so the cell's centre sits on the post.
fn lamp(p: Vec2) -> Grey {
    // The crystal: pointed top and bottom, widest a little below centre.
    let centre = Vec2::new(0.0, 15.0);
    let q = p - centre;
    let (half_width, half_height) = (9.0, 14.0);
    let shape = q.x.abs() / half_width + q.y.abs() / half_height - 1.0;
    let inside = cover(shape * half_width);
    // Facets: lit on the upper left, shaded on the lower right, with a
    // ridge down the middle.
    let facet = if q.x < 0.0 { 0.78 } else { 0.55 } + if q.y > 0.0 { 0.12 } else { -0.05 };
    let ridge = 1.0 - (q.x.abs() / 0.8).min(1.0);
    let heart = (-(q.length() / 3.5).powi(2)).exp();
    let crystal = Grey((facet + 0.2 * ridge + 0.5 * heart).min(1.0), inside);
    // The cradle: a short cup under the crystal.
    let cup = Vec2::new(p.x, p.y - 3.5);
    let cradle_shape = (cup.x.abs() - 5.0 + cup.y.max(0.0) * 0.4).max(cup.y.abs() - 2.0);
    let cradle = Grey(
        0.45 + 0.2 * f32::from(u8::from(p.x < 0.0)),
        cover(cradle_shape),
    );
    crystal.over(cradle)
}

/// How far up and down a checkpoint post runs from its cell's centre; drawn
/// from inside this, it stretches to any height.
pub const POST_REACH: f32 = 28.0;

/// A checkpoint post: a slim rod lit from the left, the same all the way
/// up so it stretches to any height.
fn post(p: Vec2) -> Grey {
    let half = 2.0;
    let across = (p.x / half).clamp(-1.0, 1.0);
    let lit = 0.7 - 0.3 * across;
    Grey(
        lit,
        cover(p.x.abs() - half).min(cover(p.y.abs() - POST_REACH)),
    )
}

/// A burst of sparks: eight rays and eight dots that fly out from the
/// centre and fade over the frames (`phase`, 0 to 1).
fn burst(p: Vec2, phase: f32) -> Grey {
    let reach = BURST_REACH * (0.35 + 0.65 * phase);
    let fade = 1.0 - phase * 0.6;
    let mut sparks = Grey(1.0, 0.0);
    for k in 0..8u8 {
        let angle = f32::from(k) * PI / 4.0 + phase * 0.4;
        let dir = Vec2::from_angle(angle);
        let along = p.dot(dir);
        let across = p.perp_dot(dir).abs();
        // A streak just inside the edge, shortening as it goes.
        let length = 8.0 * (1.0 - phase) + 2.0;
        let from = reach - length;
        let on_ray = along > from && along < reach;
        if on_ray {
            let fall = 1.0 - (reach - along) / length;
            sparks = Grey(1.0, sparks.1.max(cover(across - 0.9) * (0.4 + 0.6 * fall)));
        }
        // A dot between each pair of rays, a little behind them.
        let between = Vec2::from_angle(angle + PI / 8.0) * reach * 0.75;
        let dot = cover(p.distance(between) - 1.6 * (1.0 - phase * 0.5));
        sparks = Grey(1.0, sparks.1.max(dot));
    }
    // A soft flash at the centre that fades fast.
    let flash = (-(p.length() / 7.0).powi(2)).exp() * (1.0 - phase).powi(2);
    sparks.over(Grey(1.0, flash)).fade(fade)
}

/// A droplet of light: a bright bead in a soft glow.
fn droplet(p: Vec2) -> Grey {
    let r = p.length();
    let glow = Grey(1.0, (-(r / 6.0).powi(2)).exp() * 0.6);
    Grey(1.0, cover(r - 3.0)).over(glow)
}

/// A glowing shot with two bright arcs circling it over the loop
/// (`phase`, 0 to 1).
fn orb(p: Vec2, phase: f32) -> Grey {
    let radius = ORB_RADIUS;
    let r = p.length();
    let halo = Grey(
        0.85,
        (-(r / radius - 0.7).max(0.0).powi(2) * 3.0).exp() * 0.55,
    );
    let shade = 1.0 - 0.25 * (r / radius).powi(2);
    let lit = (p / radius).dot(Vec2::new(LIGHT[0], LIGHT[1])).max(0.0);
    let body = Grey((shade + 0.2 * lit).min(1.0), cover(r - radius * 0.72));
    let angle = p.y.atan2(p.x);
    let arcs = (2.0 * (angle - phase * PI)).cos().max(0.0).powi(3);
    let ring = Grey(1.0, cover((r - radius * 0.92).abs() - 0.8) * arcs);
    ring.over(body.over(halo))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(figure: Figure, frame: u32) -> Vec<(Vec2, Grey)> {
        (0..CELL_PX)
            .flat_map(|y| (0..CELL_PX).map(move |x| cell_point(x, y)))
            .map(|p| (p, figure.paint(frame, p)))
            .collect()
    }

    #[test]
    fn the_committed_sheet_is_up_to_date() {
        let path = format!("{}/assets/{SHEET_PATH}", env!("CARGO_MANIFEST_DIR"));
        let committed = std::fs::read(&path).unwrap_or_default();
        let painted = sheet().png().unwrap_or_default();
        assert!(
            committed == painted,
            "{path} is out of date; run `just art`"
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn figures_fill_the_sheet_in_order_without_overlapping() {
        let mut seen = Vec::new();
        for figure in Figure::ALL {
            for frame in 0..figure.frames() {
                let rect = figure.rect(frame);
                assert!(!seen.contains(&rect), "{figure:?} frame {frame}");
                seen.push(rect);
            }
        }
        assert_eq!(seen.len(), cells() as usize);
        // Frames wrap round.
        assert_eq!(Figure::Mote.rect(6), Figure::Mote.rect(0));
        let canvas = sheet();
        assert!(seen.iter().all(|rect| rect.max.x <= canvas.width as f32
            && rect.max.y <= canvas.height as f32));
    }

    #[test]
    fn figures_are_grey_and_clear_at_their_cell_edges() {
        let canvas = sheet();
        assert!(canvas.pixels.iter().flatten().all(|c| c.is_finite()));
        for [r, g, b, _] in &canvas.pixels {
            assert!((r - g).abs() < f32::EPSILON && (g - b).abs() < f32::EPSILON);
        }
        // A clear border, so linear filtering never picks up a neighbour.
        for figure in Figure::ALL {
            for frame in 0..figure.frames() {
                for (p, Grey(_, alpha)) in pixels(figure, frame) {
                    if p.abs().max_element() > CELL_SIZE / 2.0 - 2.0 {
                        assert!(alpha < 0.01, "{figure:?} at {p}");
                    }
                }
            }
        }
    }

    #[test]
    fn bodies_match_their_size_in_the_game() {
        let solid = |figure, at: Vec2| figure_alpha(figure, at) > 0.99;
        let clear = |figure, at: Vec2| figure_alpha(figure, at) < 0.01;
        for (figure, radius) in [
            (Figure::HeroSurface, HERO_RADIUS),
            (Figure::Bubble, PICKUP_RADIUS),
            (Figure::Lens, LENS_RADIUS),
        ] {
            assert!(solid(figure, Vec2::new(0.0, radius * 0.9)), "{figure:?}");
            assert!(
                clear(figure, Vec2::new(radius + 1.5, radius + 1.5)),
                "{figure:?}"
            );
        }
        assert!(solid(Figure::Mote, Vec2::ZERO));
        assert!(solid(Figure::Orb, Vec2::ZERO));
    }

    fn figure_alpha(figure: Figure, at: Vec2) -> f32 {
        figure.paint(0, at).1
    }

    #[test]
    fn the_hero_is_lit_from_the_top_left() {
        let shade = |at: Vec2| Figure::HeroShade.paint(0, at * HERO_RADIUS);
        let glint = shade(Vec2::new(-0.3, 0.4));
        let dark = shade(Vec2::new(0.6, -0.6));
        assert!(glint.0 > 0.9 && glint.1 > 0.5, "{glint:?}");
        assert!(dark.0 < 0.1 && dark.1 > 0.4, "{dark:?}");
    }

    #[test]
    fn animations_change_from_frame_to_frame() {
        for figure in Figure::ALL.into_iter().filter(|f| f.frames() > 1) {
            let frames: Vec<_> = (0..figure.frames()).map(|f| pixels(figure, f)).collect();
            for (i, a) in frames.iter().enumerate() {
                for b in &frames[i + 1..] {
                    assert_ne!(a, b, "{figure:?}");
                }
            }
        }
    }

    #[test]
    fn every_power_up_has_a_distinct_glyph() {
        let glyphs: Vec<_> = PowerUp::ALL
            .into_iter()
            .chain([PowerUp::Prism])
            .map(glyph_parts)
            .collect();
        assert!(glyphs.iter().all(|parts| !parts.is_empty()));
        for (i, a) in glyphs.iter().enumerate() {
            for b in &glyphs[i + 1..] {
                assert_ne!(a, b);
            }
        }
        let drawn: Vec<_> = PowerUp::ALL
            .into_iter()
            .chain([PowerUp::Prism])
            .map(|power| pixels(Figure::Glyph(power), 0))
            .collect();
        for (i, a) in drawn.iter().enumerate() {
            for b in &drawn[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
