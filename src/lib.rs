//! Core game library. Everything testable lives here; `main.rs` only wires it
//! into a windowed Bevy app.
//!
//! Game logic (`SimulationPlugin`) is kept separate from how it is drawn
//! (`PresentationPlugin`), so logic runs headless in tests and a 3D
//! presentation can be added later without touching the rules.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};

pub mod beam;
#[cfg(feature = "dev")]
pub mod dev_tools;
pub mod enemy;
pub mod events;
pub mod hero;
pub mod level;
pub mod light;
pub mod physics;
pub mod powerup;
pub mod prism;
pub mod render;
pub mod sound;
pub mod spectrum_view;

/// Title shown on the game window.
pub const WINDOW_TITLE: &str = "Prismapsis";

/// Logical window size in pixels (16:9).
pub const WINDOW_SIZE: (u32, u32) = (1280, 720);

/// Background colour behind the level.
pub const BACKGROUND: Color = Color::srgb(0.05, 0.05, 0.1);

/// Simulation rate. A multiple of common 60/120 Hz displays, so frames get a
/// whole number of steps; rendering interpolates for any other rate.
pub const PHYSICS_HZ: f64 = 120.0;

/// Top-level plugin for the whole game.
#[derive(Debug, Default)]
pub struct GamePlugin {
    /// A level to play over and over instead of the built-in campaign, with
    /// the file it came from (watched for edits in dev builds).
    pub level: Option<(level::Level, std::path::PathBuf)>,
}

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        if let Some((level, path)) = &self.level {
            app.insert_resource(level::Campaign::single(level.clone(), path.clone()));
        }
        app.insert_resource(ClearColor(BACKGROUND))
            // Different drops each run; tests keep the fixed default seed.
            .insert_resource(powerup::DropRng::seeded(time_seed()))
            .add_plugins((
                SimulationPlugin,
                render::PresentationPlugin,
                sound::SoundPlugin,
            ));
        #[cfg(feature = "dev")]
        app.add_plugins((dev_tools::DevToolsPlugin, level::reload::LevelReloadPlugin));
    }
}

/// Game rules and state, with no rendering dependencies.
#[derive(Debug, Default)]
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ))
            .add_message::<events::GameEvent>()
            .init_resource::<physics::BounceTuning>()
            .register_type::<physics::BounceTuning>()
            .add_systems(Startup, hero::spawn_hero)
            .init_resource::<hero::HoverInput>()
            .add_systems(Update, (hero::spin_input, hero::hover_input))
            .add_systems(
                FixedUpdate,
                (hero::move_hero, hero::update_facing.after(hero::move_hero)),
            )
            .add_plugins((
                level::LevelPlugin,
                prism::PrismPlugin,
                beam::BeamPlugin,
                enemy::EnemyPlugin,
                powerup::PowerUpPlugin,
                light::LightPlugin,
            ));
    }
}

/// A seed from the clock, for unpredictable drops between runs.
fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        // Only the low bits matter for a seed.
        .map_or(1, |d| {
            #[allow(clippy::cast_possible_truncation)]
            let nanos = d.as_nanos() as u64;
            nanos
        })
}

/// Configuration for the primary window. With `fullscreen`, the game starts
/// borderless at the monitor's native resolution; otherwise it opens a
/// `WINDOW_SIZE` window.
#[must_use]
pub fn primary_window(fullscreen: bool) -> Window {
    Window {
        title: WINDOW_TITLE.to_owned(),
        resolution: WindowResolution::new(WINDOW_SIZE.0, WINDOW_SIZE.1),
        mode: if fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        },
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use hero::{HERO_RADIUS, Hero, Position};
    use std::time::Duration;

    #[test]
    fn primary_window_uses_configured_title_and_size() {
        let window = primary_window(false);
        assert_eq!(window.title, WINDOW_TITLE);
        assert_eq!(window.resolution.physical_width(), WINDOW_SIZE.0);
        assert_eq!(window.resolution.physical_height(), WINDOW_SIZE.1);
        assert_eq!(window.mode, WindowMode::Windowed);
    }

    #[test]
    fn primary_window_can_start_fullscreen() {
        assert_eq!(
            primary_window(true).mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
    }

    #[test]
    fn simulation_runs_at_physics_rate() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SimulationPlugin));
        let step = app.world().resource::<Time<Fixed>>().timestep();
        assert!((step.as_secs_f64() - 1.0 / PHYSICS_HZ).abs() < 1e-9);
    }

    #[test]
    fn hero_falls_and_keeps_bouncing_inside_the_room() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins(SimulationPlugin);

        let mut lowest = f32::MAX;
        let mut highest = f32::MIN;
        // ~10 seconds of simulated time.
        for _ in 0..600 {
            app.update();
            let world = app.world_mut();
            let pos = world
                .query_filtered::<&Position, With<Hero>>()
                .single(world)
                .unwrap();
            lowest = lowest.min(pos.0.y);
            highest = highest.max(pos.0.y);
        }

        let floor_top = 80.0;
        let tuning = physics::BounceTuning::default();
        assert!(
            lowest >= floor_top + HERO_RADIUS - 1.0,
            "fell through floor: {lowest}"
        );
        // After the first landing, bounces peak at the apex above the floor.
        let apex = floor_top + HERO_RADIUS + tuning.apex_height;
        assert!(highest <= apex + 10.0, "rose above apex: {highest}");
        assert!(highest >= apex - 10.0, "never reached apex: {highest}");
    }
}
