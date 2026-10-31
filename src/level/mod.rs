//! Levels: building the current level's layout into the world, the room's
//! bounds, and checkpoints. The file format lives in `format`; `preview`
//! draws a level in a terminal.

pub mod format;
pub mod preview;
#[cfg(feature = "dev")]
pub mod reload;

use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

pub use format::{Level, LevelError, parse};

use crate::enemy::{EnemyTuning, ResetRoom, reset_room};
use crate::events::GameEvent;
use crate::hero::{
    Facing, HERO_RADIUS, Hero, Position, PreviousPosition, Spin, Velocity, move_hero,
};
use crate::light::{ExitGate, Hidden, LevelsCompleted, LightTuning, Phantom, Shade, reach_exit};
use crate::physics::Aabb;
use crate::powerup::{Placed, pickup_bundle};

/// Height of the view in world units, whatever the window size.
pub const VIEW_HEIGHT: f32 = 1080.0;

/// The test room, built into the game.
pub const TEST_ROOM: &str = include_str!("../../assets/levels/test_room.toml");

/// The test room's file in the source tree, watched for edits in dev builds.
pub const TEST_ROOM_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/assets/levels/test_room.toml");

/// The levels played in order, built into the game: each file's name in
/// `assets/levels` and its contents.
const CAMPAIGN: &[(&str, &str)] = &[
    ("test_room.toml", TEST_ROOM),
    ("tower.toml", include_str!("../../assets/levels/tower.toml")),
];

/// Static geometry the hero bounces off.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Solid(pub Aabb);

/// Background decoration: no collision, there to show colour returning.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Scenery(pub Aabb);

/// Marks everything built from the level's layout, so it can be rebuilt.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct LevelGeometry;

/// Touching a checkpoint moves the respawn point to it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Checkpoint {
    pub aabb: Aabb,
    pub active: bool,
}

/// World-space extent of the current room, used to bound the camera.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RoomBounds(pub Aabb);

/// The level being played.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CurrentLevel(pub Level);

/// A level in the campaign, with the file it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignLevel {
    pub path: PathBuf,
    pub level: Level,
}

/// The levels to play in order. Finishing the last one starts the first
/// again.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Campaign {
    pub levels: Vec<CampaignLevel>,
    pub current: usize,
}

impl Campaign {
    /// The campaign built into the game.
    ///
    /// # Panics
    ///
    /// If an embedded level is invalid, which a test rules out.
    #[must_use]
    pub fn built_in() -> Self {
        let levels = CAMPAIGN
            .iter()
            .map(|&(file, source)| match parse(source) {
                Ok(level) => CampaignLevel {
                    path: Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("assets/levels")
                        .join(file),
                    level,
                },
                // A test parses them, so only a broken build gets here.
                Err(errors) => panic!("built-in level {file} is invalid: {errors:?}"),
            })
            .collect();
        Self { levels, current: 0 }
    }

    /// A campaign of one level, played over and over.
    #[must_use]
    pub fn single(level: Level, path: PathBuf) -> Self {
        Self {
            levels: vec![CampaignLevel { path, level }],
            current: 0,
        }
    }

    /// The level being played.
    #[must_use]
    pub fn level(&self) -> &CampaignLevel {
        &self.levels[self.current]
    }

    /// Move on to the next level, back to the first after the last.
    pub const fn advance(&mut self) {
        self.current = (self.current + 1) % self.levels.len();
    }
}

/// Where the hero comes back after losing a life.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RespawnPoint(pub Vec2);

/// The built-in test room.
///
/// # Panics
///
/// If the embedded file is invalid, which a test rules out.
#[must_use]
pub fn test_room() -> Level {
    match parse(TEST_ROOM) {
        Ok(level) => level,
        // A test parses it, so only a broken build gets here.
        Err(errors) => panic!("the built-in test room is invalid: {errors:?}"),
    }
}

/// Read and parse a level file.
///
/// # Errors
///
/// Returns every problem with the file, one per line.
pub fn load(path: &Path) -> Result<Level, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&source).map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("{}: {e}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Bottom of the view when it follows a hero at `hero_y`, keeping inside the
/// room. Rooms shorter than the view show from their floor.
#[must_use]
pub fn view_bottom(hero_y: f32, room: &Aabb) -> f32 {
    let highest = (room.max.y - VIEW_HEIGHT).max(room.min.y);
    (hero_y - VIEW_HEIGHT / 2.0).clamp(room.min.y, highest)
}

#[derive(Debug, Default)]
pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Campaign>() {
            app.insert_resource(Campaign::built_in());
        }
        if !app.world().contains_resource::<CurrentLevel>() {
            let level = app.world().resource::<Campaign>().level().level.clone();
            app.insert_resource(CurrentLevel(level));
        }
        let start = app.world().resource::<CurrentLevel>().0.start;
        app.insert_resource(RespawnPoint(start))
            .register_type::<Checkpoint>()
            .add_systems(Startup, spawn_level)
            .add_systems(
                FixedUpdate,
                (
                    touch_checkpoints.after(move_hero),
                    (
                        next_level,
                        rebuild_level.run_if(resource_changed::<CurrentLevel>),
                    )
                        .chain()
                        .after(reach_exit)
                        .before(reset_room),
                ),
            );
    }
}

/// Spawn the level's walls, phantoms, scenery, exits and checkpoints, and
/// set the room's bounds.
pub fn spawn_geometry(commands: &mut Commands, level: &Level) {
    commands.insert_resource(RoomBounds(Aabb::new(Vec2::ZERO, level.size)));
    for &(aabb, hue) in &level.walls {
        let mut wall = commands.spawn((Name::new("Wall"), LevelGeometry, Solid(aabb)));
        if let Some(hue) = hue {
            wall.insert(Shade(hue));
        }
    }
    for &(aabb, hue) in &level.phantoms {
        commands.spawn((Name::new("Phantom"), LevelGeometry, Phantom { hue, aabb }));
    }
    for &(aabb, hue) in &level.scenery {
        let mut scenery = commands.spawn((Name::new("Scenery"), LevelGeometry, Scenery(aabb)));
        if let Some(hue) = hue {
            scenery.insert(Shade(hue));
        }
    }
    for &aabb in &level.exits {
        commands.spawn((Name::new("Exit"), LevelGeometry, ExitGate(aabb)));
    }
    for &aabb in &level.checkpoints {
        commands.spawn((
            Name::new("Checkpoint"),
            LevelGeometry,
            Checkpoint {
                aabb,
                active: false,
            },
        ));
    }
}

/// The level's placed pickups; respawned whenever the room resets.
pub fn spawn_pickups(commands: &mut Commands, level: &Level) {
    for pickup in &level.pickups {
        let mut entity = commands.spawn((pickup_bundle(pickup.power, pickup.at), Placed));
        if let Some(hue) = pickup.hidden {
            entity.insert(Hidden(hue));
        }
    }
}

/// The level the world's layout was last built from.
#[derive(Resource, Debug, Clone, PartialEq)]
struct BuiltLevel(Level);

fn spawn_level(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut enemy: ResMut<EnemyTuning>,
    mut light: ResMut<LightTuning>,
) {
    spawn_geometry(&mut commands, &level.0);
    commands.insert_resource(BuiltLevel(level.0.clone()));
    spawn_pickups(&mut commands, &level.0);
    level.0.apply_tuning(&mut enemy, &mut light);
}

/// Finishing a level moves on to the next in the campaign.
fn next_level(
    completed: Res<LevelsCompleted>,
    mut seen: Local<u32>,
    mut campaign: ResMut<Campaign>,
    mut level: ResMut<CurrentLevel>,
) {
    if completed.0 == *seen {
        return;
    }
    *seen = completed.0;
    campaign.advance();
    level.set_if_neq(CurrentLevel(campaign.level().level.clone()));
}

/// Replace the layout after the level changes (the next level, or an edit),
/// then reset the room, which repopulates it and puts the hero at the new
/// start.
fn rebuild_level(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut built: ResMut<BuiltLevel>,
    mut reset: ResMut<ResetRoom>,
    mut enemy: ResMut<EnemyTuning>,
    mut light: ResMut<LightTuning>,
    geometry: Query<Entity, With<LevelGeometry>>,
) {
    if built.0 == level.0 {
        return;
    }
    built.0.clone_from(&level.0);
    for entity in &geometry {
        commands.entity(entity).despawn();
    }
    spawn_geometry(&mut commands, &level.0);
    level.0.apply_tuning(&mut enemy, &mut light);
    reset.0 = true;
}

/// Where the hero respawns at a checkpoint: its centre, lifted clear of the
/// floor if the checkpoint is shorter than the hero.
fn checkpoint_respawn(aabb: &Aabb) -> Vec2 {
    let centre = aabb.center();
    Vec2::new(centre.x, centre.y.max(aabb.min.y + HERO_RADIUS))
}

fn touch_checkpoints(
    mut events: MessageWriter<GameEvent>,
    mut respawn: ResMut<RespawnPoint>,
    heroes: Query<&Position, With<Hero>>,
    mut checkpoints: Query<&mut Checkpoint>,
) {
    let Ok(hero) = heroes.single() else {
        return;
    };
    let hero_box = Aabb::from_center(hero.0, Vec2::splat(HERO_RADIUS));
    let Some(reached) = checkpoints
        .iter()
        .find(|c| !c.active && c.aabb.overlaps(&hero_box))
        .map(|c| c.aabb)
    else {
        return;
    };
    for mut checkpoint in &mut checkpoints {
        checkpoint.active = checkpoint.aabb == reached;
    }
    respawn.0 = checkpoint_respawn(&reached);
    events.write(GameEvent::CheckpointReached { at: respawn.0 });
}

/// What restarting the level needs, for `enemy::reset_room`.
#[derive(SystemParam, Debug)]
pub struct LevelRestart<'w, 's> {
    pub level: Res<'w, CurrentLevel>,
    respawn: ResMut<'w, RespawnPoint>,
    checkpoints: Query<'w, 's, &'static mut Checkpoint>,
    heroes: Query<
        'w,
        's,
        (
            &'static mut Position,
            &'static mut PreviousPosition,
            &'static mut Velocity,
            &'static mut Spin,
            &'static mut Facing,
        ),
        With<Hero>,
    >,
}

impl LevelRestart<'_, '_> {
    /// Put the hero back at the start, clear the checkpoints and replace the
    /// level's pickups.
    pub fn restart(&mut self, commands: &mut Commands) {
        let start = self.level.0.start;
        self.respawn.0 = start;
        for mut checkpoint in &mut self.checkpoints {
            checkpoint.set_if_neq(Checkpoint {
                active: false,
                ..*checkpoint
            });
        }
        for (mut pos, mut prev, mut vel, mut spin, mut facing) in &mut self.heroes {
            // Move both so the hero snaps rather than sweeping across.
            pos.0 = start;
            prev.0 = start;
            *vel = Velocity::default();
            *spin = Spin::default();
            *facing = Facing::default();
        }
        spawn_pickups(commands, &self.level.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::enemy::LENS_RADIUS;
    use crate::light::Hue;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn inside(aabb: &Aabb, level: &Level) -> bool {
        let bounds = Aabb::new(Vec2::ZERO, level.size);
        aabb.min.cmpge(bounds.min).all() && aabb.max.cmple(bounds.max).all()
    }

    #[test]
    fn every_level_file_is_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/levels");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "toml") {
                load(&path).unwrap();
                count += 1;
            }
        }
        assert!(count >= 1);
        assert_eq!(load(Path::new(TEST_ROOM_PATH)), Ok(test_room()));
    }

    #[test]
    fn loading_reports_the_file_with_each_problem() {
        let missing = load(Path::new("no/such/level.toml")).unwrap_err();
        assert!(missing.starts_with("no/such/level.toml: "));
        let dir = std::env::temp_dir().join(format!("prismapsis-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.toml");
        std::fs::write(&path, "name = \"Bad\"\nmap = '''\n#\n'''\n").unwrap();
        let problems = load(&path).unwrap_err();
        assert_eq!(problems.lines().count(), 2, "{problems}");
        assert!(
            problems
                .lines()
                .all(|l| l.starts_with(&*path.to_string_lossy()))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_room_layout_is_sound() {
        let level = test_room();
        let walls: Vec<Aabb> = level.walls.iter().map(|w| w.0).collect();
        let all = walls
            .iter()
            .chain(level.phantoms.iter().map(|p| &p.0))
            .chain(level.scenery.iter().map(|s| &s.0))
            .chain(&level.exits)
            .chain(&level.checkpoints);
        for aabb in all {
            assert!(inside(aabb, &level), "{aabb:?} outside the room");
        }
        let hues: std::collections::BTreeSet<_> = level
            .phantoms
            .iter()
            .map(|p| format!("{:?}", p.1))
            .collect();
        assert_eq!(hues.len(), 6, "one phantom per hue except white");
        for (phantom, _) in &level.phantoms {
            assert!(walls.iter().all(|w| !w.overlaps(phantom)));
        }
        for (center, _) in &level.lenses {
            let lens = Aabb::from_center(*center, Vec2::splat(LENS_RADIUS));
            assert!(walls.iter().all(|w| !w.overlaps(&lens)), "lens at {center}");
        }
        // Hidden pickups wait above the phantom of their hue.
        for pickup in &level.pickups {
            let hue = pickup.hidden.unwrap();
            let platform = level.phantoms.iter().find(|p| p.1 == hue).unwrap().0;
            let at = pickup.at;
            assert!(at.x > platform.min.x && at.x < platform.max.x && at.y > platform.max.y);
        }
        assert_eq!(level.lenses.len(), 2);
        assert_eq!(level.pickups.len(), 2);
        assert_eq!(level.exits.len(), 1);
        assert_eq!(level.checkpoints.len(), 2);
    }

    #[test]
    fn view_bottom_follows_inside_tall_rooms() {
        let short = Aabb::new(Vec2::ZERO, Vec2::new(4800.0, 1080.0));
        assert!(view_bottom(900.0, &short).abs() < f32::EPSILON);
        let tall = Aabb::new(Vec2::ZERO, Vec2::new(1920.0, 3000.0));
        assert!(view_bottom(100.0, &tall).abs() < f32::EPSILON);
        assert!((view_bottom(1500.0, &tall) - 960.0).abs() < f32::EPSILON);
        assert!((view_bottom(2900.0, &tall) - 1920.0).abs() < f32::EPSILON);
    }

    fn sim_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins(SimulationPlugin);
        app.update();
        app
    }

    fn count<T: Component>(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query_filtered::<(), With<T>>().iter(world).count()
    }

    #[test]
    fn spawning_builds_the_level_and_its_bounds() {
        let mut app = sim_app();
        let level = test_room();
        // Phantoms turn solid later; at the start only walls are.
        assert_eq!(count::<Solid>(&mut app), level.walls.len());
        assert_eq!(count::<Phantom>(&mut app), level.phantoms.len());
        assert_eq!(count::<Checkpoint>(&mut app), level.checkpoints.len());
        assert_eq!(count::<Hidden>(&mut app), level.pickups.len());
        assert_eq!(app.world().resource::<RoomBounds>().0.size(), level.size);
    }

    fn hero(app: &mut App) -> Vec2 {
        let world = app.world_mut();
        world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0
    }

    fn put_hero_at(app: &mut App, at: Vec2) {
        let world = app.world_mut();
        let mut hero = world
            .query_filtered::<&mut Position, With<Hero>>()
            .single_mut(world)
            .unwrap();
        hero.0 = at;
    }

    #[test]
    fn checkpoints_move_the_respawn_point_until_the_room_resets() {
        let mut app = sim_app();
        let start = test_room().start;
        assert_eq!(app.world().resource::<RespawnPoint>().0, start);
        let checkpoint = test_room().checkpoints[0];
        put_hero_at(&mut app, checkpoint.center());
        app.update();
        let respawn = app.world().resource::<RespawnPoint>().0;
        assert_eq!(respawn, checkpoint_respawn(&checkpoint));
        let world = app.world_mut();
        let active: Vec<bool> = world
            .query::<&Checkpoint>()
            .iter(world)
            .map(|c| c.active)
            .collect();
        assert_eq!(active.iter().filter(|&&a| a).count(), 1);

        app.insert_resource(ResetRoom(true));
        app.update();
        assert_eq!(app.world().resource::<RespawnPoint>().0, start);
        assert!(hero(&mut app).distance(start) < 30.0);
        let world = app.world_mut();
        assert!(world.query::<&Checkpoint>().iter(world).all(|c| !c.active));
    }

    fn finish_level(app: &mut App) {
        let exit = app.world().resource::<CurrentLevel>().0.exits[0].center();
        let mut spectrum = app.world_mut().resource_mut::<crate::light::Spectrum>();
        for primary in crate::light::Primary::ALL {
            spectrum.restore(primary, 10.0);
        }
        put_hero_at(app, exit);
        app.update();
        app.update();
    }

    #[test]
    fn finishing_a_level_moves_on_through_the_campaign() {
        let mut app = sim_app();
        let campaign = Campaign::built_in();
        assert!(campaign.levels.len() >= 2);
        assert_eq!(campaign.level().level, test_room());

        finish_level(&mut app);
        let tower = &campaign.levels[1].level;
        assert_eq!(app.world().resource::<CurrentLevel>().0, *tower);
        assert_eq!(app.world().resource::<RoomBounds>().0.size(), tower.size);
        assert_eq!(count::<Solid>(&mut app), tower.walls.len());
        assert!(hero(&mut app).distance(tower.start) < 30.0);
        assert_eq!(app.world().resource::<RespawnPoint>().0, tower.start);

        // After the last level, back to the first.
        for _ in 1..campaign.levels.len() {
            finish_level(&mut app);
        }
        assert_eq!(app.world().resource::<CurrentLevel>().0, test_room());
    }

    #[test]
    fn a_single_level_campaign_repeats_it() {
        let mut campaign = Campaign::single(test_room(), PathBuf::from("x.toml"));
        campaign.advance();
        assert_eq!(campaign.current, 0);
    }

    #[test]
    fn short_checkpoints_respawn_the_hero_clear_of_the_floor() {
        let tile = Aabb::new(Vec2::new(0.0, 80.0), Vec2::new(40.0, 120.0));
        assert!((checkpoint_respawn(&tile).y - (80.0 + HERO_RADIUS)).abs() < f32::EPSILON);
    }

    #[test]
    fn changing_the_level_rebuilds_the_room() {
        let mut app = sim_app();
        let mut level = test_room();
        level.walls.truncate(3);
        level.phantoms.clear();
        level.start = Vec2::new(300.0, 500.0);
        level.walls[0].1 = Some(Hue::Cyan);
        app.world_mut().resource_mut::<CurrentLevel>().0 = level;
        app.update();
        app.update();
        assert_eq!(count::<Solid>(&mut app), 3);
        assert_eq!(count::<Phantom>(&mut app), 0);
        assert!(hero(&mut app).distance(Vec2::new(300.0, 500.0)) < 30.0);
    }
}
