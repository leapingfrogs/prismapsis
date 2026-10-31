//! Reload the level being played when its file is saved, in dev builds. The
//! file is polled, so this needs no file-watching dependency.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::prelude::*;

use super::{Campaign, CurrentLevel, load};

/// How often the level file is checked for changes (seconds).
const POLL_INTERVAL: f32 = 0.5;

/// The level file being watched, and when it last changed.
#[derive(Resource, Debug)]
pub struct LevelWatch {
    path: PathBuf,
    modified: Option<SystemTime>,
    timer: Timer,
}

impl Default for LevelWatch {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            modified: None,
            timer: Timer::from_seconds(POLL_INTERVAL, TimerMode::Repeating),
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[derive(Debug, Default)]
pub struct LevelReloadPlugin;

impl Plugin for LevelReloadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelWatch>()
            .add_systems(Update, reload_level);
    }
}

/// Replace the current level when its file changes, keeping the edit in the
/// campaign too. A file with problems is reported and the level kept as it
/// is. Moving to another level starts watching its file.
fn reload_level(
    time: Res<Time>,
    mut watch: ResMut<LevelWatch>,
    mut campaign: ResMut<Campaign>,
    mut level: ResMut<CurrentLevel>,
) {
    if !watch.timer.tick(time.delta()).just_finished() {
        return;
    }
    let path = &campaign.level().path;
    if *path != watch.path {
        // Its current contents are what's being played.
        watch.modified = modified(path);
        watch.path.clone_from(path);
        return;
    }
    let now = modified(&watch.path);
    if now.is_none() || now == watch.modified {
        return;
    }
    watch.modified = now;
    match load(&watch.path) {
        Ok(new) => {
            let current = campaign.current;
            campaign.levels[current].level = new.clone();
            if level.set_if_neq(CurrentLevel(new)) {
                info!("reloaded {}", watch.path.display());
            }
        }
        Err(problems) => error!("level not reloaded:\n{problems}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{TEST_ROOM, test_room};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn saving_the_file_replaces_the_level_unless_it_has_problems() {
        let dir = std::env::temp_dir().join(format!("prismapsis-reload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("level.toml");
        std::fs::write(&path, TEST_ROOM).unwrap();

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                300,
            )))
            .insert_resource(Campaign::single(test_room(), path.clone()))
            .insert_resource(CurrentLevel(test_room()))
            .add_plugins(LevelReloadPlugin);
        let run = |app: &mut App| {
            for _ in 0..4 {
                app.update();
            }
        };
        run(&mut app);
        assert_eq!(app.world().resource::<CurrentLevel>().0, test_room());

        let renamed = TEST_ROOM.replace("name = \"Test room\"", "name = \"Edited\"");
        let bump = |path: &PathBuf, text: &str| {
            std::fs::write(path, text).unwrap();
            // Some filesystems keep coarse times; make the change visible.
            let later = SystemTime::now() + Duration::from_secs(5);
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(later)
                .unwrap();
        };
        bump(&path, &renamed);
        run(&mut app);
        assert_eq!(app.world().resource::<CurrentLevel>().0.name, "Edited");
        // The campaign keeps the edit for the next time round.
        assert_eq!(
            app.world().resource::<Campaign>().level().level.name,
            "Edited"
        );

        bump(&path, "name = \"Broken\"\nmap = '''\n?\n'''\n");
        run(&mut app);
        assert_eq!(app.world().resource::<CurrentLevel>().0.name, "Edited");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
