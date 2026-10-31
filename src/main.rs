use std::path::PathBuf;

use bevy::prelude::*;
use prismapsis::{GamePlugin, level, primary_window};

/// Flags: `--fullscreen`, and `--level <file>` to play a level file instead
/// of the built-in test room.
fn main() -> AppExit {
    let args: Vec<String> = std::env::args().collect();
    let fullscreen = args.iter().any(|arg| arg == "--fullscreen");
    let level = match args.iter().position(|arg| arg == "--level") {
        None => None,
        Some(i) => {
            let Some(path) = args.get(i + 1).map(PathBuf::from) else {
                eprintln!("--level needs a level file");
                return AppExit::error();
            };
            match level::load(&path) {
                Ok(level) => Some((level, path)),
                Err(problems) => {
                    eprintln!("{problems}");
                    return AppExit::error();
                }
            }
        }
    };
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(primary_window(fullscreen)),
                ..default()
            }),
            GamePlugin { level },
        ))
        .run()
}
