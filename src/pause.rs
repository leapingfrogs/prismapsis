//! Pausing with P (gamepad Start).
//!
//! Pausing stops the game clock, so the fixed-step simulation and anything
//! timed by `Time` hold still until P is pressed again. Play inputs are
//! ignored meanwhile, so nothing changes on resume. Menu screens stop the
//! clock the same way, and P does nothing while one shows.

use bevy::prelude::*;

use crate::screens::{Screen, in_play};

#[derive(Debug, Default)]
pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            toggle_pause
                .after(bevy::input::InputSystems)
                .run_if(in_play),
        );
    }
}

/// Run condition for play inputs: true while playing, unless paused.
#[must_use]
pub fn running(time: Res<Time<Virtual>>, screen: Option<Res<State<Screen>>>) -> bool {
    !time.is_paused() && in_play(screen)
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut time: ResMut<Time<Virtual>>,
) {
    let pressed = keys.just_pressed(KeyCode::KeyP)
        || gamepads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::Start));
    if !pressed {
        return;
    }
    if time.is_paused() {
        time.unpause();
    } else {
        time.pause();
    }
}

/// The "Paused" overlay, shown while the clock is stopped.
#[derive(Component, Debug)]
pub struct PauseOverlay;

pub fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        Name::new("Pause overlay"),
        PauseOverlay,
        Text::new("Paused\nP to resume"),
        TextLayout::justify(Justify::Center),
        TextFont {
            font_size: FontSize::Vh(6.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Vh(40.0),
            justify_self: JustifySelf::Center,
            ..default()
        },
        Visibility::Hidden,
    ));
}

pub fn show_overlay(
    time: Res<Time<Virtual>>,
    screen: Option<Res<State<Screen>>>,
    mut overlays: Query<&mut Visibility, With<PauseOverlay>>,
) {
    let shown = time.is_paused() && in_play(screen);
    for mut visibility in &mut overlays {
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::hero::{Hero, Position, Spin};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins(SimulationPlugin)
            .add_systems(Startup, spawn_overlay)
            .add_systems(Update, show_overlay);
        app
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    fn hero(app: &mut App) -> (Vec2, i8) {
        let world = app.world_mut();
        let (pos, spin) = world
            .query_filtered::<(&Position, &Spin), With<Hero>>()
            .single(world)
            .unwrap();
        (pos.0, spin.0)
    }

    fn overlay_shown(app: &mut App) -> bool {
        let world = app.world_mut();
        *world
            .query_filtered::<&Visibility, With<PauseOverlay>>()
            .single(world)
            .unwrap()
            == Visibility::Inherited
    }

    #[test]
    fn p_freezes_the_game_and_ignores_play_inputs_until_pressed_again() {
        let mut app = app();
        for _ in 0..10 {
            app.update();
        }
        tap(&mut app, KeyCode::KeyP);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        assert!(overlay_shown(&mut app));

        let before = hero(&mut app);
        tap(&mut app, KeyCode::ArrowRight);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(hero(&mut app), before);

        tap(&mut app, KeyCode::KeyP);
        assert!(!overlay_shown(&mut app));
        for _ in 0..10 {
            app.update();
        }
        assert_ne!(hero(&mut app).0, before.0);
        assert_eq!(hero(&mut app).1, before.1, "spin taken while paused");
    }
}
