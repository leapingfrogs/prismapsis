use bevy::prelude::*;
use prismapsis::{GamePlugin, primary_window};

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(primary_window()),
                ..default()
            }),
            GamePlugin,
        ))
        .run()
}
