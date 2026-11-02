//! The player's settings, kept between runs in a small TOML file: the
//! master, music and effects volumes.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::screens::Screen;
use crate::sound::SoundSettings;

/// What's saved. Missing values take their defaults, so older files still
/// load.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub master: f32,
    pub music: f32,
    pub effects: f32,
}

impl Default for Saved {
    fn default() -> Self {
        Self::from(&SoundSettings::default())
    }
}

impl From<&SoundSettings> for Saved {
    fn from(settings: &SoundSettings) -> Self {
        Self {
            master: settings.master,
            music: settings.music,
            effects: settings.effects,
        }
    }
}

impl Saved {
    /// Set these volumes, each kept within 0 to 1.
    pub const fn apply(self, settings: &mut SoundSettings) {
        settings.master = self.master.clamp(0.0, 1.0);
        settings.music = self.music.clamp(0.0, 1.0);
        settings.effects = self.effects.clamp(0.0, 1.0);
    }
}

/// Where settings are kept: `prismapsis/settings.toml` in the user's
/// configuration directory (`$XDG_CONFIG_HOME`, `~/.config` or
/// `%APPDATA%`), if there is one.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    let var = |name| std::env::var_os(name).filter(|v| !v.is_empty());
    let dir = var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|home| Path::new(&home).join(".config")))
        .or_else(|| var("APPDATA").map(PathBuf::from))?;
    Some(dir.join("prismapsis").join("settings.toml"))
}

/// The settings in `path`, or `None` if it can't be read. A file with
/// mistakes is ignored rather than stopping the game.
#[must_use]
pub fn load(path: &Path) -> Option<Saved> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}

/// Write `saved` to `path`, making its directory if need be.
///
/// # Errors
///
/// If the directory or file can't be written.
pub fn save(path: &Path, saved: Saved) -> Result<(), String> {
    let text = toml::to_string(&saved).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The settings file, if settings are kept; tests and tools run without.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsFile(pub Option<PathBuf>);

/// Loads the settings at start and saves them on leaving the settings
/// screen.
#[derive(Debug, Default)]
pub struct SettingsPlugin {
    pub path: Option<PathBuf>,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SettingsFile(self.path.clone()))
            .add_systems(Startup, load_settings)
            .add_systems(OnExit(Screen::Settings), save_settings);
    }
}

fn load_settings(file: Res<SettingsFile>, mut settings: ResMut<SoundSettings>) {
    if let Some(saved) = file.0.as_deref().and_then(load) {
        saved.apply(&mut settings);
    }
}

fn save_settings(file: Res<SettingsFile>, settings: Res<SoundSettings>) {
    if let Some(path) = &file.0
        && let Err(problem) = save(path, Saved::from(&*settings))
    {
        warn!("couldn't save settings: {problem}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("prismapsis-settings-{}-{name}", std::process::id()))
            .join("settings.toml")
    }

    #[test]
    fn settings_save_and_load_back() {
        let path = temp_file("round-trip");
        let saved = Saved {
            master: 0.5,
            music: 0.2,
            effects: 0.9,
        };
        save(&path, saved).unwrap();
        assert_eq!(load(&path), Some(saved));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_values_take_defaults_and_bad_files_are_ignored() {
        let path = temp_file("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "music = 0.1\n").unwrap();
        let loaded = load(&path).unwrap();
        assert!((loaded.music - 0.1).abs() < f32::EPSILON);
        assert!((loaded.master - Saved::default().master).abs() < f32::EPSILON);
        std::fs::write(&path, "music = \"loud\"\n").unwrap();
        assert_eq!(load(&path), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(load(&path), None);
    }

    #[test]
    fn applying_keeps_volumes_in_range() {
        let mut settings = SoundSettings::default();
        Saved {
            master: 2.0,
            music: -1.0,
            effects: 0.3,
        }
        .apply(&mut settings);
        assert!((settings.master - 1.0).abs() < f32::EPSILON);
        assert!(settings.music.abs() < f32::EPSILON);
        assert!((settings.effects - 0.3).abs() < f32::EPSILON);
        // The beam's dev setting isn't the player's.
        assert!((settings.beam - SoundSettings::default().beam).abs() < f32::EPSILON);
    }

    #[test]
    fn the_settings_file_lives_in_the_config_directory() {
        if let Some(path) = default_path() {
            assert!(path.ends_with("prismapsis/settings.toml"));
        }
    }

    #[test]
    fn settings_load_at_start_and_save_on_leaving_the_settings_screen() {
        use bevy::state::app::StatesPlugin;
        let path = temp_file("plugin");
        save(
            &path,
            Saved {
                master: 0.3,
                ..Saved::default()
            },
        )
        .unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_resource::<SoundSettings>()
            .insert_state(Screen::Settings)
            .add_plugins(SettingsPlugin {
                path: Some(path.clone()),
            });
        app.update();
        assert!((app.world().resource::<SoundSettings>().master - 0.3).abs() < f32::EPSILON);

        app.world_mut().resource_mut::<SoundSettings>().music = 0.25;
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Title);
        app.update();
        assert!((load(&path).unwrap().music - 0.25).abs() < f32::EPSILON);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
