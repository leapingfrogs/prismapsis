// No console window beside the game on Windows, except in dev builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use bevy::prelude::*;
use prismapsis::{GamePlugin, level, primary_window, settings};

/// Flags: `--fullscreen` or `--windowed` to choose for this run, and
/// `--level <file>` to play a level file instead of the built-in test room.
fn main() -> AppExit {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    let flag = if has("--windowed") {
        Some(false)
    } else if has("--fullscreen") {
        Some(true)
    } else {
        None
    };
    let store = settings::default_store();
    let saved = store
        .as_ref()
        .and_then(settings::Store::load)
        .and_then(|saved| saved.fullscreen);
    let fullscreen = settings::opens_fullscreen(flag, saved, !cfg!(debug_assertions));
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
            GamePlugin {
                level,
                settings: store,
            },
        ))
        .run()
}
