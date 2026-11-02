//! The player's settings, kept between runs as a few lines of TOML: the
//! master, music and effects volumes. The desktop game keeps them in a
//! file, and the browser in its local storage.

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

/// The key the browser keeps settings under in its local storage.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "prismapsis.settings";

/// Where settings are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Store {
    /// A TOML file.
    File(PathBuf),
    /// The browser's local storage, holding the same TOML.
    #[cfg(target_arch = "wasm32")]
    Browser,
}

/// Where the game keeps settings: the browser's local storage on the web,
/// otherwise the file at [`default_path`], if there is one.
#[must_use]
#[cfg(target_arch = "wasm32")]
pub const fn default_store() -> Option<Store> {
    Some(Store::Browser)
}

/// Where the game keeps settings: the browser's local storage on the web,
/// otherwise the file at [`default_path`], if there is one.
#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub fn default_store() -> Option<Store> {
    default_path().map(Store::File)
}

impl Store {
    /// The settings kept here, or `None` if there are none or they can't
    /// be read. Settings with mistakes are ignored rather than stopping the
    /// game.
    #[must_use]
    pub fn load(&self) -> Option<Saved> {
        toml::from_str(&self.read()?).ok()
    }

    /// Keep `saved` here.
    ///
    /// # Errors
    ///
    /// If they can't be written.
    pub fn save(&self, saved: Saved) -> Result<(), String> {
        let text = toml::to_string(&saved).map_err(|e| e.to_string())?;
        self.write(&text)
    }

    /// The text kept here, if any.
    fn read(&self) -> Option<String> {
        match self {
            Self::File(path) => std::fs::read_to_string(path).ok(),
            #[cfg(target_arch = "wasm32")]
            Self::Browser => local_storage()?.get_item(STORAGE_KEY).ok()?,
        }
    }

    /// Keep `text` here, making a file's directory if need be.
    fn write(&self, text: &str) -> Result<(), String> {
        match self {
            Self::File(path) => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
            }
            #[cfg(target_arch = "wasm32")]
            Self::Browser => local_storage()
                .ok_or("no local storage")?
                .set_item(STORAGE_KEY, text)
                .map_err(|e| format!("local storage: {e:?}")),
        }
    }
}

/// The browser's local storage, if the page may use it.
#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// Where settings are kept, if they are; tests and tools run without.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsStore(pub Option<Store>);

/// Loads the settings at start and saves them on leaving the settings
/// screen.
#[derive(Debug, Default)]
pub struct SettingsPlugin {
    pub store: Option<Store>,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SettingsStore(self.store.clone()))
            .add_systems(Startup, load_settings)
            .add_systems(OnExit(Screen::Settings), save_settings);
    }
}

fn load_settings(store: Res<SettingsStore>, mut settings: ResMut<SoundSettings>) {
    if let Some(saved) = store.0.as_ref().and_then(Store::load) {
        saved.apply(&mut settings);
    }
}

fn save_settings(store: Res<SettingsStore>, settings: Res<SoundSettings>) {
    if let Some(store) = &store.0
        && let Err(problem) = store.save(Saved::from(&*settings))
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
        let store = Store::File(path.clone());
        let saved = Saved {
            master: 0.5,
            music: 0.2,
            effects: 0.9,
        };
        store.save(saved).unwrap();
        assert_eq!(store.load(), Some(saved));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_values_take_defaults_and_bad_files_are_ignored() {
        let path = temp_file("partial");
        let store = Store::File(path.clone());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "music = 0.1\n").unwrap();
        let loaded = store.load().unwrap();
        assert!((loaded.music - 0.1).abs() < f32::EPSILON);
        assert!((loaded.master - Saved::default().master).abs() < f32::EPSILON);
        std::fs::write(&path, "music = \"loud\"\n").unwrap();
        assert_eq!(store.load(), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(store.load(), None);
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
        if let Some(Store::File(path)) = default_store() {
            assert!(path.ends_with("prismapsis/settings.toml"));
        }
    }

    #[test]
    fn settings_load_at_start_and_save_on_leaving_the_settings_screen() {
        use bevy::state::app::StatesPlugin;
        let path = temp_file("plugin");
        let store = Store::File(path.clone());
        store
            .save(Saved {
                master: 0.3,
                ..Saved::default()
            })
            .unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_resource::<SoundSettings>()
            .insert_state(Screen::Settings)
            .add_plugins(SettingsPlugin {
                store: Some(store.clone()),
            });
        app.update();
        assert!((app.world().resource::<SoundSettings>().master - 0.3).abs() < f32::EPSILON);

        app.world_mut().resource_mut::<SoundSettings>().music = 0.25;
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Title);
        app.update();
        assert!((store.load().unwrap().music - 0.25).abs() < f32::EPSILON);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
