use bevy::prelude::*;
use prismapsis::{GamePlugin, primary_window};

fn main() -> AppExit {
    let fullscreen = std::env::args().any(|arg| arg == "--fullscreen");
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(primary_window(fullscreen)),
                ..default()
            }),
            GamePlugin,
        ))
        .run()
}
