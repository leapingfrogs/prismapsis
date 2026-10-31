//! Pure bounce physics. No ECS here, so every rule is directly unit-testable.

use bevy::prelude::*;

/// Spin is set in whole steps from `-MAX_SPIN` to `MAX_SPIN`.
pub const MAX_SPIN: i8 = 5;

/// Minimum penetration (world units) that counts as an overlap. Keeps a body
/// resting exactly on a surface from registering as colliding with it.
const OVERLAP_EPSILON: f32 = 0.01;

/// Tunable movement parameters. Adjusted live from the dev tuning panel.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct BounceTuning {
    /// Downward acceleration (world units/s²).
    pub gravity: f32,
    /// Height the hero's base reaches above the surface it bounced off.
    pub apex_height: f32,
    /// Horizontal speed per spin step (world units/s).
    pub speed_per_step: f32,
    /// Whether hitting a wall also reverses the spin step (the chosen feel).
    /// When false, the next floor bounce sends the hero back towards the wall.
    pub wall_flips_spin: bool,
    /// Vertical speed while hovering with anti-gravity (world units/s).
    pub hover_speed: f32,
}

impl Default for BounceTuning {
    fn default() -> Self {
        Self {
            gravity: 2400.0,
            apex_height: 320.0,
            speed_per_step: 90.0,
            wall_flips_spin: true,
            hover_speed: 360.0,
        }
    }
}

impl BounceTuning {
    /// Upward speed that carries a body exactly to `apex_height`.
    #[must_use]
    pub fn bounce_speed(&self) -> f32 {
        (2.0 * self.gravity * self.apex_height).sqrt()
    }

    /// Horizontal speed for a spin step.
    #[must_use]
    pub fn horizontal_speed(&self, spin: i8) -> f32 {
        f32::from(spin) * self.speed_per_step
    }
}

/// Clamp a spin step to the allowed range.
#[must_use]
pub fn clamp_spin(spin: i8) -> i8 {
    spin.clamp(-MAX_SPIN, MAX_SPIN)
}

/// Axis-aligned box in world space.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    #[must_use]
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    #[must_use]
    pub fn from_center(center: Vec2, half: Vec2) -> Self {
        Self::new(center - half, center + half)
    }

    #[must_use]
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    #[must_use]
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.min.x + OVERLAP_EPSILON < other.max.x
            && other.min.x + OVERLAP_EPSILON < self.max.x
            && self.min.y + OVERLAP_EPSILON < other.max.y
            && other.min.y + OVERLAP_EPSILON < self.max.y
    }
}

/// A moving body. Collides as a square of side `2 * half` around `pos`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub pos: Vec2,
    pub vel: Vec2,
    pub half: f32,
}

impl Body {
    fn aabb(&self) -> Aabb {
        Aabb::from_center(self.pos, Vec2::splat(self.half))
    }
}

/// Which surfaces a body touched during a step.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Contacts {
    pub floor: bool,
    pub ceiling: bool,
    pub wall: bool,
}

/// Advance `body` by `dt`, bouncing off `solids` with no energy loss.
///
/// - Floor: bounce back up to the apex height; spin sets horizontal speed.
/// - Ceiling: reflect downwards, capping the bounce.
/// - Wall: reverse horizontal speed (and spin, if tuned to).
///
/// Large steps are split so a body never moves more than half its size per
/// sub-step, preventing it tunnelling through thin solids.
pub fn step(
    body: &mut Body,
    spin: &mut i8,
    tuning: &BounceTuning,
    solids: &[Aabb],
    dt: f32,
) -> Contacts {
    let mut contacts = Contacts::default();
    let max_speed = body.vel.abs().max_element().max(tuning.bounce_speed());
    let (substeps, sub_dt) = substeps(body, max_speed, dt);

    for _ in 0..substeps {
        body.vel.y -= tuning.gravity * sub_dt;

        let dx = body.vel.x * sub_dt;
        body.pos.x += dx;
        resolve_x(body, spin, tuning, solids, dx, &mut contacts);

        let dy = body.vel.y * sub_dt;
        body.pos.y += dy;
        resolve_y(body, *spin, tuning, solids, dy, &mut contacts);
    }
    contacts
}

/// Advance `body` by `dt` with anti-gravity: no gravity and no bouncing.
///
/// Spin sets horizontal speed directly and `vertical` (-1, 0 or 1) moves the
/// hero down or up at `hover_speed`. Floors and ceilings stop it; walls
/// still reverse it (and its spin, if tuned to).
pub fn hover_step(
    body: &mut Body,
    spin: &mut i8,
    vertical: i8,
    tuning: &BounceTuning,
    solids: &[Aabb],
    dt: f32,
) -> Contacts {
    let mut contacts = Contacts::default();
    body.vel = Vec2::new(
        tuning.horizontal_speed(*spin),
        f32::from(vertical) * tuning.hover_speed,
    );
    let (substeps, sub_dt) = substeps(body, body.vel.abs().max_element(), dt);

    for _ in 0..substeps {
        let dx = body.vel.x * sub_dt;
        body.pos.x += dx;
        resolve_x(body, spin, tuning, solids, dx, &mut contacts);

        let dy = body.vel.y * sub_dt;
        body.pos.y += dy;
        if dy == 0.0 {
            continue;
        }
        for solid in solids {
            if !body.aabb().overlaps(solid) {
                continue;
            }
            if dy < 0.0 {
                body.pos.y = solid.max.y + body.half;
                contacts.floor = true;
            } else {
                body.pos.y = solid.min.y - body.half;
                contacts.ceiling = true;
            }
            body.vel.y = 0.0;
        }
    }
    contacts
}

/// Split `dt` so a body moving at `max_speed` never travels more than half
/// its size per sub-step, preventing it tunnelling through thin solids.
fn substeps(body: &Body, max_speed: f32, dt: f32) -> (u32, f32) {
    let max_travel = (body.half * 0.5).max(f32::EPSILON);
    // Bounded by speed, so the cast cannot truncate meaningfully.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let substeps = ((max_speed * dt) / max_travel).ceil().max(1.0) as u32;
    #[allow(clippy::cast_precision_loss)]
    let sub_dt = dt / substeps as f32;
    (substeps, sub_dt)
}

fn resolve_x(
    body: &mut Body,
    spin: &mut i8,
    tuning: &BounceTuning,
    solids: &[Aabb],
    dx: f32,
    contacts: &mut Contacts,
) {
    if dx == 0.0 {
        return;
    }
    for solid in solids {
        if !body.aabb().overlaps(solid) {
            continue;
        }
        body.pos.x = if dx > 0.0 {
            solid.min.x - body.half
        } else {
            solid.max.x + body.half
        };
        body.vel.x = -body.vel.x;
        if tuning.wall_flips_spin {
            *spin = -*spin;
        }
        contacts.wall = true;
    }
}

fn resolve_y(
    body: &mut Body,
    spin: i8,
    tuning: &BounceTuning,
    solids: &[Aabb],
    dy: f32,
    contacts: &mut Contacts,
) {
    if dy == 0.0 {
        return;
    }
    for solid in solids {
        if !body.aabb().overlaps(solid) {
            continue;
        }
        if dy < 0.0 {
            body.pos.y = solid.max.y + body.half;
            body.vel.y = tuning.bounce_speed();
            body.vel.x = tuning.horizontal_speed(spin);
            contacts.floor = true;
        } else {
            body.pos.y = solid.min.y - body.half;
            body.vel.y = -body.vel.y;
            contacts.ceiling = true;
        }
    }
}

/// Distance along a ray to where it enters `aabb`, or `None` if it misses.
/// `dir` must be normalised. A ray starting inside the box hits at 0.
#[must_use]
pub fn ray_aabb(origin: Vec2, dir: Vec2, aabb: &Aabb) -> Option<f32> {
    // Slab method: zero direction components give infinities, which the
    // min/max below handle correctly.
    let inv = dir.recip();
    let t1 = (aabb.min - origin) * inv;
    let t2 = (aabb.max - origin) * inv;
    let t_enter = t1.min(t2).max_element();
    let t_exit = t1.max(t2).min_element();
    if t_exit < 0.0 || t_enter > t_exit {
        return None;
    }
    Some(t_enter.max(0.0))
}

/// Distance along a ray to where it enters a circle, or `None` if it misses.
/// `dir` must be normalised. A ray starting inside the circle hits at 0.
#[must_use]
pub fn ray_circle(origin: Vec2, dir: Vec2, center: Vec2, radius: f32) -> Option<f32> {
    let offset = origin - center;
    let b = offset.dot(dir);
    let c = offset.length_squared() - radius * radius;
    if c > 0.0 && b > 0.0 {
        return None;
    }
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }
    Some((-b - discriminant.sqrt()).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 64.0;

    #[test]
    fn ray_hits_box_ahead_and_misses_behind_or_beside() {
        let aabb = Aabb::new(Vec2::new(100.0, -10.0), Vec2::new(120.0, 10.0));
        let t = ray_aabb(Vec2::ZERO, Vec2::X, &aabb).unwrap();
        assert!((t - 100.0).abs() < 1e-4);
        assert!(ray_aabb(Vec2::ZERO, Vec2::NEG_X, &aabb).is_none());
        assert!(ray_aabb(Vec2::new(0.0, 50.0), Vec2::X, &aabb).is_none());
        assert_eq!(ray_aabb(Vec2::new(110.0, 0.0), Vec2::X, &aabb), Some(0.0));
    }

    #[test]
    fn ray_hits_box_diagonally() {
        let aabb = Aabb::new(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0));
        let t = ray_aabb(Vec2::ZERO, Vec2::ONE.normalize(), &aabb).unwrap();
        assert!((t - 10.0 * 2f32.sqrt()).abs() < 1e-4);
    }

    #[test]
    fn ray_hits_circle_ahead_and_misses_otherwise() {
        let t = ray_circle(Vec2::ZERO, Vec2::X, Vec2::new(100.0, 0.0), 10.0).unwrap();
        assert!((t - 90.0).abs() < 1e-4);
        assert!(ray_circle(Vec2::ZERO, Vec2::NEG_X, Vec2::new(100.0, 0.0), 10.0).is_none());
        assert!(ray_circle(Vec2::ZERO, Vec2::X, Vec2::new(100.0, 50.0), 10.0).is_none());
        assert_eq!(
            ray_circle(Vec2::new(100.0, 0.0), Vec2::X, Vec2::new(100.0, 0.0), 10.0),
            Some(0.0)
        );
    }
    const HALF: f32 = 24.0;

    fn floor() -> Aabb {
        Aabb::new(Vec2::new(-10_000.0, -100.0), Vec2::new(10_000.0, 0.0))
    }

    fn resting_body() -> Body {
        Body {
            pos: Vec2::new(0.0, HALF),
            vel: Vec2::ZERO,
            half: HALF,
        }
    }

    /// Run until the next floor contact, returning the highest base height.
    fn run_one_bounce(
        body: &mut Body,
        spin: &mut i8,
        tuning: &BounceTuning,
        solids: &[Aabb],
    ) -> f32 {
        let mut apex = f32::MIN;
        for _ in 0..10_000 {
            let contacts = step(body, spin, tuning, solids, DT);
            apex = apex.max(body.pos.y - body.half);
            if contacts.floor {
                return apex;
            }
        }
        panic!("body never landed");
    }

    #[test]
    fn spin_is_clamped_to_range() {
        assert_eq!(clamp_spin(9), MAX_SPIN);
        assert_eq!(clamp_spin(-9), -MAX_SPIN);
        assert_eq!(clamp_spin(3), 3);
    }

    #[test]
    fn bounce_speed_reaches_apex_analytically() {
        let tuning = BounceTuning::default();
        let v = tuning.bounce_speed();
        let apex = v * v / (2.0 * tuning.gravity);
        assert!((apex - tuning.apex_height).abs() < 1e-3);
    }

    #[test]
    fn bounces_return_to_fixed_apex_without_decay() {
        let tuning = BounceTuning::default();
        let mut body = resting_body();
        let mut spin = 0;
        // First contact launches the body.
        step(&mut body, &mut spin, &tuning, &[floor()], DT);
        for _ in 0..5 {
            let apex = run_one_bounce(&mut body, &mut spin, &tuning, &[floor()]);
            assert!(
                (apex - tuning.apex_height).abs() < 10.0,
                "apex {apex} should be close to {}",
                tuning.apex_height
            );
        }
    }

    #[test]
    fn apex_is_fixed_even_after_a_long_fall() {
        let tuning = BounceTuning::default();
        let mut body = Body {
            pos: Vec2::new(0.0, 2000.0),
            ..resting_body()
        };
        let mut spin = 0;
        run_one_bounce(&mut body, &mut spin, &tuning, &[floor()]);
        let apex = run_one_bounce(&mut body, &mut spin, &tuning, &[floor()]);
        assert!((apex - tuning.apex_height).abs() < 10.0);
    }

    #[test]
    fn floor_contact_sets_horizontal_speed_from_spin() {
        let tuning = BounceTuning::default();
        let mut body = resting_body();
        let mut spin = 3;
        let contacts = step(&mut body, &mut spin, &tuning, &[floor()], DT);
        assert!(contacts.floor);
        assert!((body.vel.x - 3.0 * tuning.speed_per_step).abs() < 1e-3);
    }

    #[test]
    fn zero_spin_bounces_in_place() {
        let tuning = BounceTuning::default();
        let mut body = resting_body();
        let mut spin = 0;
        for _ in 0..500 {
            step(&mut body, &mut spin, &tuning, &[floor()], DT);
        }
        assert!(body.pos.x.abs() < f32::EPSILON);
    }

    #[test]
    fn horizontal_speed_holds_while_airborne() {
        let tuning = BounceTuning::default();
        let mut body = Body {
            pos: Vec2::new(0.0, 500.0),
            vel: Vec2::new(150.0, 0.0),
            half: HALF,
        };
        let mut spin = -5;
        step(&mut body, &mut spin, &tuning, &[floor()], DT);
        assert!((body.vel.x - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn wall_reverses_horizontal_speed_and_spin_by_default() {
        let tuning = BounceTuning::default();
        let wall = Aabb::new(Vec2::new(100.0, 0.0), Vec2::new(200.0, 1000.0));
        let mut body = Body {
            pos: Vec2::new(70.0, 300.0),
            vel: Vec2::new(400.0, 0.0),
            half: HALF,
        };
        let mut spin = 4;
        let contacts = step(&mut body, &mut spin, &tuning, &[wall], DT);
        assert!(contacts.wall);
        assert!(body.vel.x < 0.0);
        assert!(body.pos.x <= wall.min.x - HALF);
        assert_eq!(spin, -4);
    }

    #[test]
    fn wall_can_be_tuned_to_keep_spin() {
        let tuning = BounceTuning {
            wall_flips_spin: false,
            ..default()
        };
        let wall = Aabb::new(Vec2::new(100.0, 0.0), Vec2::new(200.0, 1000.0));
        let mut body = Body {
            pos: Vec2::new(70.0, 300.0),
            vel: Vec2::new(400.0, 0.0),
            half: HALF,
        };
        let mut spin = 4;
        step(&mut body, &mut spin, &tuning, &[wall], DT);
        assert_eq!(spin, 4);
    }

    #[test]
    fn wall_on_the_left_pushes_body_right() {
        let tuning = BounceTuning::default();
        let wall = Aabb::new(Vec2::new(-200.0, 0.0), Vec2::new(-100.0, 1000.0));
        let mut body = Body {
            pos: Vec2::new(-70.0, 300.0),
            vel: Vec2::new(-400.0, 0.0),
            half: HALF,
        };
        let mut spin = -2;
        step(&mut body, &mut spin, &tuning, &[wall], DT);
        assert!(body.vel.x > 0.0);
        assert!(body.pos.x >= wall.max.x + HALF);
    }

    #[test]
    fn low_ceiling_caps_bounce_height_without_losing_energy() {
        let tuning = BounceTuning::default();
        let ceiling = Aabb::new(Vec2::new(-500.0, 150.0), Vec2::new(500.0, 200.0));
        let mut body = resting_body();
        let mut spin = 0;
        step(&mut body, &mut spin, &tuning, &[floor(), ceiling], DT);
        let mut hit_ceiling = false;
        for _ in 0..200 {
            let c = step(&mut body, &mut spin, &tuning, &[floor(), ceiling], DT);
            hit_ceiling |= c.ceiling;
            assert!(body.pos.y + HALF <= ceiling.min.y + 1e-3);
        }
        assert!(hit_ceiling);
        // Next floor bounce still launches at full speed (less at most one
        // step of gravity, as the bounce can happen mid-step).
        run_one_bounce(&mut body, &mut spin, &tuning, &[floor(), ceiling]);
        assert!((body.vel.y - tuning.bounce_speed()).abs() <= tuning.gravity * DT);
    }

    #[test]
    fn fast_body_does_not_tunnel_through_thin_wall() {
        let tuning = BounceTuning::default();
        let wall = Aabb::new(Vec2::new(100.0, 0.0), Vec2::new(104.0, 1000.0));
        let mut body = Body {
            pos: Vec2::new(0.0, 300.0),
            vel: Vec2::new(20_000.0, 0.0),
            half: HALF,
        };
        let mut spin = 0;
        step(&mut body, &mut spin, &tuning, &[wall], DT);
        assert!(body.pos.x < wall.min.x);
    }

    #[test]
    fn hovering_moves_vertically_without_gravity() {
        let tuning = BounceTuning::default();
        let mut body = Body {
            pos: Vec2::new(0.0, 300.0),
            vel: Vec2::new(0.0, -900.0),
            half: HALF,
        };
        let mut spin = 2;
        hover_step(&mut body, &mut spin, 0, &tuning, &[floor()], 1.0);
        // No fall, and spin alone sets horizontal speed.
        assert!((body.pos.y - 300.0).abs() < 1e-3);
        assert!((body.pos.x - tuning.horizontal_speed(2)).abs() < 1e-2);

        hover_step(&mut body, &mut spin, 1, &tuning, &[floor()], 0.5);
        assert!((body.pos.y - (300.0 + tuning.hover_speed * 0.5)).abs() < 1e-2);
    }

    #[test]
    fn hovering_stops_at_floors_and_ceilings_without_bouncing() {
        let tuning = BounceTuning::default();
        let ceiling = Aabb::new(Vec2::new(-500.0, 400.0), Vec2::new(500.0, 450.0));
        let mut body = Body {
            pos: Vec2::new(0.0, 200.0),
            vel: Vec2::ZERO,
            half: HALF,
        };
        let mut spin = 0;
        let solids = [floor(), ceiling];
        let contacts = hover_step(&mut body, &mut spin, -1, &tuning, &solids, 2.0);
        assert!(contacts.floor);
        assert!((body.pos.y - HALF).abs() < 1e-3);
        assert!(body.vel.y.abs() < f32::EPSILON);

        let contacts = hover_step(&mut body, &mut spin, 1, &tuning, &solids, 2.0);
        assert!(contacts.ceiling);
        assert!((body.pos.y - (ceiling.min.y - HALF)).abs() < 1e-3);
    }

    #[test]
    fn hovering_into_a_wall_flips_spin() {
        let tuning = BounceTuning::default();
        let wall = Aabb::new(Vec2::new(100.0, 0.0), Vec2::new(150.0, 1000.0));
        let mut body = Body {
            pos: Vec2::new(60.0, 300.0),
            vel: Vec2::ZERO,
            half: HALF,
        };
        let mut spin = 3;
        let contacts = hover_step(&mut body, &mut spin, 0, &tuning, &[wall], 0.5);
        assert!(contacts.wall);
        assert_eq!(spin, -3);
        assert!(body.pos.x <= wall.min.x - HALF);
    }

    #[test]
    fn aabb_helpers() {
        let aabb = Aabb::from_center(Vec2::new(10.0, 20.0), Vec2::new(5.0, 2.0));
        assert_eq!(aabb.center(), Vec2::new(10.0, 20.0));
        assert_eq!(aabb.size(), Vec2::new(10.0, 4.0));
    }
}
