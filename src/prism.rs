//! The prism companion, which orbits the hero.
//!
//! It holds its place on the orbit while the freeze button is held, and can
//! then be nudged around the orbit for precise positioning. Simulation only;
//! visuals live in `render`.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::hero::{Hero, Position, PreviousPosition, move_hero};

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

#[derive(Debug, Default)]
pub struct PrismPlugin;

impl Plugin for PrismPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrismTuning>()
            .register_type::<PrismTuning>()
            .init_resource::<PrismFrozen>()
            .init_resource::<PrismNudge>()
            .add_systems(Startup, spawn_prism)
            .add_systems(Update, prism_input)
            .add_systems(FixedUpdate, orbit_prism.after(move_hero));
    }
}

fn spawn_prism(mut commands: Commands, tuning: Res<PrismTuning>) {
    let start = crate::hero::HERO_SPAWN + orbit_offset(0.0, tuning.orbit_radius);
    commands.spawn((
        Name::new("Prism"),
        Prism::default(),
        Position(start),
        PreviousPosition(start),
    ));
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
    heroes: Query<&Position, (With<Hero>, Without<Prism>)>,
    mut prisms: Query<(&mut Prism, &mut Position, &mut PreviousPosition), Without<Hero>>,
) {
    let Ok(hero) = heroes.single() else {
        return;
    };
    let rate = orbit_rate(&tuning, frozen.0, nudge.0);
    for (mut prism, mut pos, mut prev) in &mut prisms {
        prism.angle = advance_angle(prism.angle, rate, time.delta_secs());
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
}
