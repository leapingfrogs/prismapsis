//! The hero: spin input and bounce movement. Simulation only; visuals live in
//! `render`.

use bevy::prelude::*;

use crate::events::GameEvent;
use crate::level::{CurrentLevel, Solid};
use crate::physics::{self, Body, BounceTuning};
use crate::powerup::{Abilities, PowerUp};

/// Collision half-size of the hero, in world units.
pub const HERO_RADIUS: f32 = 24.0;

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

/// Horizontal direction the hero last moved: 1 right, -1 left. Kept when
/// the hero bounces in place, so it still has a direction to fire in.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Facing(pub i8);

impl Default for Facing {
    fn default() -> Self {
        Self(1)
    }
}

impl Facing {
    /// Unit vector in the facing direction.
    #[must_use]
    pub fn direction(self) -> Vec2 {
        Vec2::new(f32::from(self.0), 0.0)
    }
}

/// Seconds of invulnerability remaining after losing a life.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Invulnerable(pub f32);

/// The hero appears at the level's start.
pub fn spawn_hero(mut commands: Commands, level: Res<CurrentLevel>) {
    let start = level.0.start;
    commands.spawn((
        Name::new("Hero"),
        Hero,
        Position(start),
        PreviousPosition(start),
        Velocity::default(),
        Spin::default(),
        Facing::default(),
        Invulnerable::default(),
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

/// Vertical input for anti-gravity hovering: 1 up, -1 down, 0 neither.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HoverInput(pub i8);

/// Up/W and Down/S (or the d-pad) steer the hero while it has anti-gravity.
pub fn hover_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut hover: ResMut<HoverInput>,
) {
    let pad_held = |button| gamepads.iter().any(|pad: &Gamepad| pad.pressed(button));
    let up = keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || pad_held(GamepadButton::DPadUp);
    let down =
        keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || pad_held(GamepadButton::DPadDown);
    let direction = i8::from(up) - i8::from(down);
    if hover.0 != direction {
        hover.0 = direction;
    }
}

/// Fixed-timestep movement against all solids: bouncing normally, or
/// hovering with anti-gravity.
pub fn move_hero(
    time: Res<Time>,
    tuning: Res<BounceTuning>,
    abilities: Res<Abilities>,
    hover: Res<HoverInput>,
    mut events: MessageWriter<GameEvent>,
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
        let dt = time.delta_secs();
        // Hovering rests on floors rather than bouncing, so only real bounces
        // count.
        let contacts = if abilities.has(PowerUp::AntiGravity) {
            let mut contacts =
                physics::hover_step(&mut body, &mut spin.0, hover.0, &tuning, &solids, dt);
            contacts.floor = false;
            contacts
        } else {
            physics::step(&mut body, &mut spin.0, &tuning, &solids, dt)
        };
        pos.0 = body.pos;
        if contacts.floor {
            events.write(GameEvent::Bounced {
                at: body.pos,
                spin: spin.0,
            });
        }
        if contacts.wall {
            events.write(GameEvent::WallBump { at: body.pos });
        }
        vel.0 = body.vel;
    }
}

/// Track the direction of horizontal travel; unchanged while not moving
/// horizontally.
pub fn update_facing(mut heroes: Query<(&Velocity, &mut Facing), With<Hero>>) {
    for (vel, mut facing) in &mut heroes {
        let moving = if vel.0.x > 0.0 {
            1
        } else if vel.0.x < 0.0 {
            -1
        } else {
            continue;
        };
        if facing.0 != moving {
            facing.0 = moving;
        }
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
            .insert_resource(CurrentLevel(crate::level::test_room()))
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

    fn sim_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(16),
            ))
            .add_plugins(crate::SimulationPlugin);
        app.update();
        app
    }

    fn hero_y(app: &mut App) -> f32 {
        let world = app.world_mut();
        world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0
            .y
    }

    #[test]
    fn anti_gravity_lets_up_and_down_steer_the_hero() {
        let mut app = sim_app();
        app.world_mut()
            .resource_mut::<Abilities>()
            .grant(PowerUp::AntiGravity);
        app.update();
        // Hovering in place: no fall.
        let start = hero_y(&mut app);
        for _ in 0..30 {
            app.update();
        }
        assert!((hero_y(&mut app) - start).abs() < 1e-3);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowUp);
        for _ in 0..30 {
            app.update();
        }
        let raised = hero_y(&mut app);
        assert!(raised > start + 100.0, "up should raise the hero");

        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::ArrowUp);
        keys.press(KeyCode::KeyS);
        for _ in 0..30 {
            app.update();
        }
        assert!(
            hero_y(&mut app) < raised - 100.0,
            "down should lower the hero"
        );
    }
}
