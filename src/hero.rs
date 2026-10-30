//! The hero: spin input and bounce movement. Simulation only; visuals live in
//! `render`.

use bevy::prelude::*;

use crate::level::Solid;
use crate::physics::{self, Body, BounceTuning};

/// Collision half-size of the hero, in world units.
pub const HERO_RADIUS: f32 = 24.0;

/// Where the hero appears in the test room.
pub const HERO_SPAWN: Vec2 = Vec2::new(400.0, 400.0);

#[derive(Component, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Hero;

/// Simulation position. Kept separate from `Transform` so game logic does
/// not depend on how (2D or 3D) it is presented.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Position(pub Vec2);

/// Position at the start of the latest fixed step. Rendering blends between
/// this and `Position` so motion is smooth at any frame rate.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct PreviousPosition(pub Vec2);

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Velocity(pub Vec2);

/// Current spin step, from `-MAX_SPIN` to `MAX_SPIN`.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Spin(pub i8);

pub fn spawn_hero(mut commands: Commands) {
    commands.spawn((
        Name::new("Hero"),
        Hero,
        Position(HERO_SPAWN),
        PreviousPosition(HERO_SPAWN),
        Velocity::default(),
        Spin::default(),
    ));
}

/// Spin change from one frame's input: -1, 0 or +1.
#[must_use]
pub fn spin_delta(left: bool, right: bool) -> i8 {
    i8::from(right) - i8::from(left)
}

/// Step spin with left/right (arrows or A/D) or the gamepad d-pad/bumpers.
pub fn spin_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut heroes: Query<&mut Spin, With<Hero>>,
) {
    let pad_pressed = |buttons: &[GamepadButton]| {
        gamepads
            .iter()
            .any(|pad| buttons.iter().any(|&b| pad.just_pressed(b)))
    };
    let left = keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA])
        || pad_pressed(&[GamepadButton::DPadLeft, GamepadButton::LeftTrigger]);
    let right = keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD])
        || pad_pressed(&[GamepadButton::DPadRight, GamepadButton::RightTrigger]);

    let delta = spin_delta(left, right);
    if delta == 0 {
        return;
    }
    for mut spin in &mut heroes {
        spin.0 = physics::clamp_spin(spin.0 + delta);
    }
}

/// Fixed-timestep bounce movement against all solids.
pub fn move_hero(
    time: Res<Time>,
    tuning: Res<BounceTuning>,
    solids: Query<&Solid>,
    mut heroes: Query<
        (
            &mut Position,
            &mut PreviousPosition,
            &mut Velocity,
            &mut Spin,
        ),
        With<Hero>,
    >,
) {
    let solids: Vec<_> = solids.iter().map(|s| s.0).collect();
    for (mut pos, mut prev, mut vel, mut spin) in &mut heroes {
        prev.0 = pos.0;
        let mut body = Body {
            pos: pos.0,
            vel: vel.0,
            half: HERO_RADIUS,
        };
        physics::step(&mut body, &mut spin.0, &tuning, &solids, time.delta_secs());
        pos.0 = body.pos;
        vel.0 = body.vel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spin_delta_combines_directions() {
        assert_eq!(spin_delta(false, false), 0);
        assert_eq!(spin_delta(true, false), -1);
        assert_eq!(spin_delta(false, true), 1);
        assert_eq!(spin_delta(true, true), 0);
    }

    fn input_app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Startup, spawn_hero)
            .add_systems(Update, spin_input);
        app.update();
        app
    }

    fn press(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    fn hero_spin(app: &mut App) -> i8 {
        let world = app.world_mut();
        world
            .query_filtered::<&Spin, With<Hero>>()
            .single(world)
            .unwrap()
            .0
    }

    #[test]
    fn keys_step_spin_and_clamp_at_limits() {
        let mut app = input_app();
        press(&mut app, KeyCode::ArrowRight);
        press(&mut app, KeyCode::KeyD);
        assert_eq!(hero_spin(&mut app), 2);

        for _ in 0..10 {
            press(&mut app, KeyCode::ArrowLeft);
        }
        assert_eq!(hero_spin(&mut app), -physics::MAX_SPIN);
    }
}
