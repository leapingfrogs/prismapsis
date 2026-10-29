//! Core game library. Everything testable lives here; `main.rs` only wires it
//! into a windowed Bevy app.

use bevy::prelude::*;
use bevy::window::WindowResolution;

/// Title shown on the game window.
pub const WINDOW_TITLE: &str = "Prismapsis";

/// Logical window size in pixels (16:9).
pub const WINDOW_SIZE: (u32, u32) = (1280, 720);

/// Background colour for the initial empty scene.
pub const BACKGROUND: Color = Color::srgb(0.05, 0.05, 0.1);

/// Top-level plugin for all game systems.
#[derive(Debug, Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(BACKGROUND))
            .add_systems(Startup, spawn_camera);
    }
}

/// Configuration for the primary window.
#[must_use]
pub fn primary_window() -> Window {
    Window {
        title: WINDOW_TITLE.to_owned(),
        resolution: WindowResolution::new(WINDOW_SIZE.0, WINDOW_SIZE.1),
        ..default()
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Name::new("Main Camera"), Camera2d));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_spawns_single_camera_and_sets_clear_colour() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, GamePlugin));
        app.update();

        let world = app.world_mut();
        let cameras = world.query::<&Camera2d>().iter(world).count();
        assert_eq!(cameras, 1);
        assert_eq!(world.resource::<ClearColor>().0, BACKGROUND);
    }

    #[test]
    fn primary_window_uses_configured_title_and_size() {
        let window = primary_window();
        assert_eq!(window.title, WINDOW_TITLE);
        assert_eq!(window.resolution.physical_width(), WINDOW_SIZE.0);
        assert_eq!(window.resolution.physical_height(), WINDOW_SIZE.1);
    }
}
