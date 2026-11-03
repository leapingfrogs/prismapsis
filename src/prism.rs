//! The prism companion, which orbits the hero.
//!
//! It holds its place on the orbit while the freeze button is held, and can
//! then be nudged around the orbit for precise positioning. Extra prisms
//! from pickups join it in a ring, always evenly spaced round the hero, and
//! held and nudged together. Simulation only; visuals live in `render`.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

use crate::hero::{Hero, Position, PreviousPosition, move_hero};
use crate::powerup::Abilities;

/// Size of the prism, centre to tip (world units). Beams treat it as a
/// circle of this radius, which is simpler and more forgiving to aim with
/// than the exact triangle.
pub const PRISM_RADIUS: f32 = 20.0;

/// Tunable orbit parameters. Adjusted live from the dev tuning panel.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct PrismTuning {
    /// Distance from the hero's centre (world units).
    pub orbit_radius: f32,
    /// Orbit speed in revolutions per second; negative orbits clockwise.
    pub orbit_speed: f32,
    /// Speed of fine adjustment while frozen (revolutions per second).
    pub nudge_speed: f32,
}

impl Default for PrismTuning {
    fn default() -> Self {
        Self {
            orbit_radius: 90.0,
            orbit_speed: 0.6,
            nudge_speed: 0.25,
        }
    }
}

/// The prism and its current angle on the orbit (radians, 0 = right of the
/// hero, increasing anticlockwise).
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Prism {
    pub angle: f32,
}

/// Whether the player is holding the prism in place this frame.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PrismFrozen(pub bool);

/// Fine adjustment requested while frozen: +1 anticlockwise, -1 clockwise.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PrismNudge(pub i8);

/// Orbit rate (revolutions per second) for this step: the normal orbit, or
/// only the player's nudge while frozen.
#[must_use]
pub fn orbit_rate(tuning: &PrismTuning, frozen: bool, nudge: i8) -> f32 {
    if frozen {
        f32::from(nudge) * tuning.nudge_speed
    } else {
        tuning.orbit_speed
    }
}

/// Next orbit angle, wrapped to `0..TAU`.
#[must_use]
pub fn advance_angle(angle: f32, revs_per_sec: f32, dt: f32) -> f32 {
    (angle + revs_per_sec * TAU * dt).rem_euclid(TAU)
}

/// Offset from the hero's centre for an orbit angle and radius.
#[must_use]
pub fn orbit_offset(angle: f32, radius: f32) -> Vec2 {
    Vec2::from_angle(angle) * radius
}

/// How fast a prism glides round the orbit to its place in the ring
/// (revolutions per second), after one joins or breaks.
pub const GLIDE_SPEED: f32 = 0.5;

/// The ring of prisms' own angle: where its first place is. The prisms'
/// places are spread evenly round from it.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct PrismRing(pub f32);

/// Signed turn from angle `from` to angle `to`, the short way round, in
/// `-PI..PI`.
#[must_use]
pub fn turn(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

/// The place in the ring for each of the prisms at `angles`: keeping them
/// in order round the orbit, the evenly spaced places from `ring` that
/// move them least. Returns each prism's place angle.
#[must_use]
pub fn ring_places(ring: f32, angles: &[f32]) -> Vec<f32> {
    let n = angles.len();
    if n == 0 {
        return Vec::new();
    }
    #[allow(clippy::cast_precision_loss)]
    let step = TAU / n as f32;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let from_ring = |i: usize| (angles[i] - ring).rem_euclid(TAU);
        from_ring(a).total_cmp(&from_ring(b))
    });
    #[allow(clippy::cast_precision_loss)]
    let place = |rank: usize, shift: usize| ring + ((rank + shift) % n) as f32 * step;
    let cost = |shift: usize| -> f32 {
        order
            .iter()
            .enumerate()
            .map(|(rank, &i)| turn(angles[i], place(rank, shift)).abs())
            .sum()
    };
    let shift = (0..n)
        .min_by(|&a, &b| cost(a).total_cmp(&cost(b)))
        .unwrap_or(0);
    let mut places = vec![0.0; n];
    for (rank, &i) in order.iter().enumerate() {
        places[i] = place(rank, shift);
    }
    places
}

/// Where a new prism joins: in the middle of the widest gap between the
/// prisms at `angles`, or opposite the only one.
#[must_use]
pub fn joining_angle(angles: &[f32]) -> f32 {
    let mut sorted: Vec<f32> = angles.iter().map(|a| a.rem_euclid(TAU)).collect();
    sorted.sort_by(f32::total_cmp);
    let Some(&first) = sorted.first() else {
        return 0.0;
    };
    let gaps = sorted
        .iter()
        .copied()
        .zip(sorted.iter().copied().skip(1).chain([first + TAU]))
        .map(|(a, b)| (b - a, a));
    let (gap, start) = gaps.fold(
        (0.0, first),
        |best, gap| if gap.0 > best.0 { gap } else { best },
    );
    (start + gap / 2.0).rem_euclid(TAU)
}

#[derive(Debug, Default)]
pub struct PrismPlugin;

impl Plugin for PrismPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrismTuning>()
            .register_type::<PrismTuning>()
            .init_resource::<PrismFrozen>()
            .init_resource::<PrismNudge>()
            .init_resource::<PrismRing>()
            .add_systems(Startup, spawn_prism)
            .add_systems(Update, prism_input.run_if(crate::pause::running))
            .add_systems(
                FixedUpdate,
                (fill_ring, orbit_prism).chain().after(move_hero),
            );
    }
}

fn prism_bundle(angle: f32, at: Vec2) -> impl Bundle {
    (
        Name::new("Prism"),
        Prism { angle },
        Position(at),
        PreviousPosition(at),
    )
}

fn spawn_prism(
    mut commands: Commands,
    tuning: Res<PrismTuning>,
    level: Res<crate::level::CurrentLevel>,
) {
    let start = level.0.start + orbit_offset(0.0, tuning.orbit_radius);
    commands.spawn(prism_bundle(0.0, start));
}

/// Keep as many prisms as the hero's abilities say: new ones join in the
/// widest gap, and extra ones go (from the end of the ring) when they're
/// lost all at once, as on losing a life.
fn fill_ring(
    mut commands: Commands,
    abilities: Res<Abilities>,
    tuning: Res<PrismTuning>,
    heroes: Query<&Position, (With<Hero>, Without<Prism>)>,
    prisms: Query<(Entity, &Prism)>,
) {
    let Ok(hero) = heroes.single() else {
        return;
    };
    let wanted = usize::from(abilities.prisms());
    let mut angles: Vec<f32> = prisms.iter().map(|(_, p)| p.angle).collect();
    while angles.len() < wanted {
        let angle = joining_angle(&angles);
        angles.push(angle);
        let at = hero.0 + orbit_offset(angle, tuning.orbit_radius);
        commands.spawn(prism_bundle(angle, at));
    }
    for (entity, _) in prisms.iter().skip(wanted) {
        commands.entity(entity).despawn();
    }
}

/// Hold Shift (gamepad left trigger) to freeze the prism; while
/// frozen, Q (gamepad West) nudges it anticlockwise and E (gamepad East)
/// clockwise.
fn prism_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut frozen: ResMut<PrismFrozen>,
    mut nudge: ResMut<PrismNudge>,
) {
    let pad_held = |button| gamepads.iter().any(|pad| pad.pressed(button));
    let held = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
        || pad_held(GamepadButton::LeftTrigger2);
    let anticlockwise = keys.pressed(KeyCode::KeyQ) || pad_held(GamepadButton::West);
    let clockwise = keys.pressed(KeyCode::KeyE) || pad_held(GamepadButton::East);
    let direction = i8::from(anticlockwise) - i8::from(clockwise);

    // Only write on change so change detection stays meaningful.
    if frozen.0 != held {
        frozen.0 = held;
    }
    if nudge.0 != direction {
        nudge.0 = direction;
    }
}

/// Advance the orbit and place the prism relative to the hero's new
/// position. Runs after the hero moves so the two stay in step.
pub fn orbit_prism(
    time: Res<Time>,
    tuning: Res<PrismTuning>,
    frozen: Res<PrismFrozen>,
    nudge: Res<PrismNudge>,
    mut ring: ResMut<PrismRing>,
    heroes: Query<&Position, (With<Hero>, Without<Prism>)>,
    mut prisms: Query<(&mut Prism, &mut Position, &mut PreviousPosition), Without<Hero>>,
) {
    let Ok(hero) = heroes.single() else {
        return;
    };
    let dt = time.delta_secs();
    let rate = orbit_rate(&tuning, frozen.0, nudge.0);
    ring.0 = advance_angle(ring.0, rate, dt);
    let angles: Vec<f32> = prisms
        .iter()
        .map(|(prism, ..)| advance_angle(prism.angle, rate, dt))
        .collect();
    let places = ring_places(ring.0, &angles);
    let glide = GLIDE_SPEED * TAU * dt;
    for ((mut prism, mut pos, mut prev), (angle, place)) in
        prisms.iter_mut().zip(angles.into_iter().zip(places))
    {
        prism.angle = (angle + turn(angle, place).clamp(-glide, glide)).rem_euclid(TAU);
        prev.0 = pos.0;
        pos.0 = hero.0 + orbit_offset(prism.angle, tuning.orbit_radius);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::f32::consts::{FRAC_PI_2, PI};
    use std::time::Duration;

    #[test]
    fn angle_advances_by_speed_and_wraps() {
        let quarter = advance_angle(0.0, 0.25, 1.0);
        assert!((quarter - FRAC_PI_2).abs() < 1e-5);
        let wrapped = advance_angle(1.5 * PI, 0.5, 1.0);
        assert!((wrapped - 0.5 * PI).abs() < 1e-5);
        let backwards = advance_angle(0.0, -0.25, 1.0);
        assert!((backwards - 1.5 * PI).abs() < 1e-5);
    }

    #[test]
    fn orbit_rate_is_orbit_speed_or_nudge_while_frozen() {
        let tuning = PrismTuning::default();
        assert!((orbit_rate(&tuning, false, 1) - tuning.orbit_speed).abs() < f32::EPSILON);
        assert!(orbit_rate(&tuning, true, 0).abs() < f32::EPSILON);
        assert!((orbit_rate(&tuning, true, 1) - tuning.nudge_speed).abs() < f32::EPSILON);
        assert!((orbit_rate(&tuning, true, -1) + tuning.nudge_speed).abs() < f32::EPSILON);
    }

    #[test]
    fn orbit_offset_points_along_angle() {
        assert!((orbit_offset(0.0, 90.0) - Vec2::new(90.0, 0.0)).length() < 1e-4);
        assert!((orbit_offset(FRAC_PI_2, 90.0) - Vec2::new(0.0, 90.0)).length() < 1e-4);
    }

    fn sim_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins(SimulationPlugin);
        app
    }

    fn hero_and_prism(app: &mut App) -> (Vec2, Vec2, f32) {
        let world = app.world_mut();
        let hero = world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0;
        let (prism, pos) = world.query::<(&Prism, &Position)>().single(world).unwrap();
        (hero, pos.0, prism.angle)
    }

    #[test]
    fn prism_orbits_the_moving_hero_at_its_radius() {
        let mut app = sim_app();
        let radius = PrismTuning::default().orbit_radius;
        let mut angles = Vec::new();
        for _ in 0..120 {
            app.update();
            let (hero, prism, angle) = hero_and_prism(&mut app);
            assert!(((prism - hero).length() - radius).abs() < 1e-3);
            angles.push(angle);
        }
        assert!(
            angles
                .windows(2)
                .any(|w| (w[0] - w[1]).abs() > f32::EPSILON)
        );
    }

    #[test]
    fn holding_freeze_keeps_the_prism_angle() {
        let mut app = sim_app();
        for _ in 0..10 {
            app.update();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.update();
        let (_, _, before) = hero_and_prism(&mut app);
        for _ in 0..60 {
            app.update();
        }
        let (hero, prism, after) = hero_and_prism(&mut app);
        assert!(app.world().resource::<PrismFrozen>().0);
        assert!((before - after).abs() < f32::EPSILON);
        // Still travels with the hero while frozen.
        assert!(((prism - hero).length() - PrismTuning::default().orbit_radius).abs() < 1e-3);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::ShiftLeft);
        for _ in 0..10 {
            app.update();
        }
        assert!(!app.world().resource::<PrismFrozen>().0);
        let (_, _, resumed) = hero_and_prism(&mut app);
        assert!((resumed - after).abs() > f32::EPSILON);
    }

    /// Signed angular change from `from` to `to`, in `-PI..PI`.
    fn angle_delta(from: f32, to: f32) -> f32 {
        (to - from + PI).rem_euclid(TAU) - PI
    }

    #[test]
    fn nudging_while_frozen_moves_the_prism_both_ways() {
        let mut app = sim_app();
        app.update();
        let hold = |app: &mut App, keys: &[KeyCode]| {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.release_all();
            for &key in keys {
                input.press(key);
            }
        };

        hold(&mut app, &[KeyCode::ShiftLeft]);
        app.update();
        let (_, _, start) = hero_and_prism(&mut app);

        hold(&mut app, &[KeyCode::ShiftLeft, KeyCode::KeyQ]);
        for _ in 0..30 {
            app.update();
        }
        let (_, _, after_q) = hero_and_prism(&mut app);
        assert!(
            angle_delta(start, after_q) > 0.1,
            "Q should turn anticlockwise"
        );

        hold(&mut app, &[KeyCode::ShiftLeft, KeyCode::KeyE]);
        for _ in 0..30 {
            app.update();
        }
        let (_, _, after_e) = hero_and_prism(&mut app);
        assert!(
            angle_delta(after_q, after_e) < -0.1,
            "E should turn clockwise"
        );
    }

    #[test]
    fn nudge_keys_do_nothing_while_orbiting() {
        let mut app = sim_app();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        let (_, _, start) = hero_and_prism(&mut app);
        for _ in 0..30 {
            app.update();
        }
        let (_, _, end) = hero_and_prism(&mut app);
        // Still orbiting anticlockwise at the normal speed.
        assert!(angle_delta(start, end) > 0.1);
    }

    #[test]
    fn places_keep_the_ring_in_order_and_move_prisms_least() {
        // One prism keeps the ring's place.
        assert_eq!(ring_places(1.0, &[1.0]), vec![1.0]);
        // Two near each other go to opposite sides, each moving a little.
        let places = ring_places(0.0, &[0.1, 0.4]);
        assert!(turn(places[0], 0.0).abs() < 1e-5);
        assert!(turn(places[1], PI).abs() < 1e-5);
        // Already evenly spaced but just behind the ring: they stay put.
        let behind = [TAU - 0.01, PI - 0.01];
        let places = ring_places(0.0, &behind);
        for (place, angle) in places.iter().zip(behind) {
            assert!(turn(angle, *place).abs() < 0.02);
        }
    }

    #[test]
    fn a_new_prism_joins_in_the_widest_gap() {
        assert!(turn(joining_angle(&[0.0]), PI).abs() < 1e-5);
        assert!(turn(joining_angle(&[0.0, 1.0]), 1.0 + (TAU - 1.0) / 2.0).abs() < 1e-5);
    }

    fn ring(app: &mut App) -> Vec<f32> {
        let world = app.world_mut();
        let hero = world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0;
        let mut angles: Vec<f32> = world
            .query::<(&Prism, &Position)>()
            .iter(world)
            .map(|(_, p)| (p.0 - hero).to_angle().rem_euclid(TAU))
            .collect();
        angles.sort_by(f32::total_cmp);
        angles
    }

    /// Whether `angles` (sorted) are evenly spaced round the orbit.
    fn evenly_spaced(angles: &[f32]) -> bool {
        #[allow(clippy::cast_precision_loss)]
        let step = TAU / angles.len() as f32;
        angles
            .iter()
            .zip(angles.iter().cycle().skip(1))
            .all(|(&a, &b)| (turn(a, b).rem_euclid(TAU) - step).abs() < 1e-3)
    }

    #[test]
    fn prisms_join_and_leave_the_ring_evenly_spaced() {
        use crate::powerup::{MAX_PRISMS, PowerUp};
        let mut app = sim_app();
        app.update();
        for _ in 0..10 {
            app.world_mut()
                .resource_mut::<Abilities>()
                .grant(PowerUp::Prism);
        }
        // Well after gliding into place, there are the most and evenly
        // spaced; held still, they stay that way.
        for _ in 0..90 {
            app.update();
        }
        let angles = ring(&mut app);
        assert_eq!(angles.len(), usize::from(MAX_PRISMS));
        assert!(evenly_spaced(&angles), "{angles:?}");

        // Losing two leaves three, which spread out again.
        let mut abilities = app.world_mut().resource_mut::<Abilities>();
        abilities.revoke(PowerUp::Prism);
        abilities.revoke(PowerUp::Prism);
        for _ in 0..90 {
            app.update();
        }
        let angles = ring(&mut app);
        assert_eq!(angles.len(), 3);
        assert!(evenly_spaced(&angles), "{angles:?}");
    }
}
