//! Enemies, their shots, and what happens when they reach the hero.
//!
//! Motes arrive in timed waves from beyond the screen edges and drift across
//! in sine-wave formations. Lenses are placed in the level and fire slow orbs
//! at the hero. The prism always blocks orbs. Touching an enemy or an orb
//! costs a life; losing every life resets the room. Simulation only; visuals
//! live in `render`.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::beam::{BeamEnergy, damage_enemies};
use crate::hero::{
    Facing, HERO_RADIUS, HERO_SPAWN, Hero, Invulnerable, Position, PreviousPosition, Spin, Velocity,
};
use crate::level::{RoomBounds, Solid, test_lenses};
use crate::physics::{Aabb, ray_aabb};
use crate::powerup::{Abilities, DropRng, Pickup, PowerUpTuning, pickup_bundle, roll_drop};
use crate::prism::{PRISM_RADIUS, Prism, orbit_prism};

/// Distance from the hero at which waves appear: just beyond the edge of a
/// 16:9 view, which is 1920 units wide.
pub const SPAWN_DISTANCE: f32 = 1150.0;

/// Gap between motes in a wave (world units).
const MOTE_SPACING: f32 = 80.0;

/// Wave heights cycle through these, so successive waves vary.
const WAVE_HEIGHTS: [f32; 4] = [220.0, 380.0, 540.0, 300.0];

/// Motes older than this are removed even if still on their way.
const MOTE_MAX_AGE: f32 = 20.0;

/// Orbs older than this are removed.
const ORB_MAX_AGE: f32 = 8.0;

pub const MOTE_RADIUS: f32 = 14.0;
pub const LENS_RADIUS: f32 = 30.0;
pub const ORB_RADIUS: f32 = 10.0;

/// How long an enemy flashes after a beam hit (seconds).
pub const HIT_FLASH: f32 = 0.15;

/// How long a defeat burst lasts (seconds).
pub const BURST_LIFETIME: f32 = 0.35;

/// Radius within which enemies and orbs hurt the hero: smaller than the
/// hero itself, so near misses feel like near misses.
pub const HERO_HURT_RADIUS: f32 = HERO_RADIUS * 0.7;

/// Lives at the start and after a room reset.
pub const STARTING_LIVES: u8 = 3;

/// Tunable enemy parameters. Adjusted live from the dev tuning panel.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct EnemyTuning {
    /// Beam damage ticks to defeat a mote.
    pub mote_health: u32,
    /// Beam damage ticks to defeat a lens.
    pub lens_health: u32,
    /// Horizontal mote speed (world units/s).
    pub mote_speed: f32,
    /// Height of the mote sine wave either side of its line.
    pub mote_amplitude: f32,
    /// Seconds per mote sine-wave cycle.
    pub mote_period: f32,
    /// Motes per wave.
    pub wave_size: u32,
    /// Seconds between waves.
    pub wave_interval: f32,
    /// Seconds between a lens's shots.
    pub lens_fire_interval: f32,
    /// How close the hero must be for a lens to fire.
    pub lens_range: f32,
    /// Orb speed (world units/s).
    pub orb_speed: f32,
    /// Invulnerability after losing a life (seconds).
    pub respawn_invulnerability: f32,
}

impl Default for EnemyTuning {
    fn default() -> Self {
        Self {
            mote_health: 1,
            lens_health: 12,
            mote_speed: 160.0,
            mote_amplitude: 45.0,
            mote_period: 1.2,
            wave_size: 3,
            wave_interval: 10.0,
            lens_fire_interval: 3.0,
            lens_range: 700.0,
            orb_speed: 180.0,
            respawn_invulnerability: 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum EnemyKind {
    Mote,
    Lens,
}

impl EnemyKind {
    #[must_use]
    pub const fn radius(self) -> f32 {
        match self {
            Self::Mote => MOTE_RADIUS,
            Self::Lens => LENS_RADIUS,
        }
    }
}

/// Anything the beam can damage and the hero must avoid.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Enemy {
    pub kind: EnemyKind,
    /// Beam damage ticks remaining.
    pub health: u32,
    /// Seconds of hit flash remaining.
    pub flash: f32,
}

impl Enemy {
    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.kind.radius()
    }
}

/// Mote movement state: drifts horizontally along `base_y` in a sine wave.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Mote {
    pub base_y: f32,
    /// -1 travels left, 1 travels right.
    pub dir: f32,
    pub phase: f32,
    pub age: f32,
}

/// Lens turret state.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Lens {
    /// Seconds until it can fire again.
    pub cooldown: f32,
}

/// A slow enemy shot.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Orb {
    pub vel: Vec2,
    pub age: f32,
}

/// A short-lived effect where an enemy was defeated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Burst {
    pub radius: f32,
    pub age: f32,
}

/// Lives remaining.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lives(pub u8);

impl Default for Lives {
    fn default() -> Self {
        Self(STARTING_LIVES)
    }
}

/// Enemies defeated since the room was last reset.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Kills {
    pub motes: u32,
    pub lenses: u32,
}

/// Set to reset the room on the next fixed step: enemies respawn, lives and
/// energy refill.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ResetRoom(pub bool);

/// Times the waves.
#[derive(Resource, Debug)]
pub struct WaveTimer {
    pub timer: Timer,
    /// Waves spawned so far; picks the side and height of the next one.
    pub count: u32,
}

impl Default for WaveTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(EnemyTuning::default().wave_interval, TimerMode::Repeating),
            count: 0,
        }
    }
}

/// Vertical offset of a mote from its line after `age` seconds.
#[must_use]
pub fn mote_offset(age: f32, amplitude: f32, period: f32, phase: f32) -> f32 {
    amplitude * (TAU * age / period.max(f32::EPSILON) + phase).sin()
}

/// One mote of a wave: start position, travel direction and sine phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoteSpawn {
    pub start: Vec2,
    pub dir: f32,
    pub phase: f32,
}

/// Where wave number `index` appears around a hero at `hero_x`. Waves
/// alternate sides, starting from the right, and cycle through heights.
#[must_use]
pub fn wave_spawns(hero_x: f32, index: u32, count: u32) -> Vec<MoteSpawn> {
    let side = if index.is_multiple_of(2) { 1.0 } else { -1.0 };
    let height = WAVE_HEIGHTS[index as usize % WAVE_HEIGHTS.len()];
    (0..count)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let i = i as f32;
            MoteSpawn {
                start: Vec2::new(hero_x + side * (SPAWN_DISTANCE + i * MOTE_SPACING), height),
                dir: -side,
                phase: i * 0.6,
            }
        })
        .collect()
}

/// Whether nothing solid lies between `from` and `to`.
#[must_use]
pub fn line_of_sight(from: Vec2, to: Vec2, solids: &[Aabb]) -> bool {
    let span = to - from;
    let distance = span.length();
    if distance <= f32::EPSILON {
        return true;
    }
    let dir = span / distance;
    solids
        .iter()
        .all(|solid| ray_aabb(from, dir, solid).is_none_or(|t| t >= distance))
}

/// Whether two circles overlap.
#[must_use]
pub fn circles_overlap(a: Vec2, a_radius: f32, b: Vec2, b_radius: f32) -> bool {
    a.distance_squared(b) < (a_radius + b_radius).powi(2)
}

fn circle_hits_solid(center: Vec2, radius: f32, solid: &Aabb) -> bool {
    let closest = center.clamp(solid.min, solid.max);
    closest.distance_squared(center) < radius * radius
}

#[derive(Debug, Default)]
pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnemyTuning>()
            .register_type::<EnemyTuning>()
            .init_resource::<Lives>()
            .init_resource::<Kills>()
            .init_resource::<ResetRoom>()
            .init_resource::<WaveTimer>()
            .add_systems(Startup, spawn_placed_enemies)
            .add_systems(
                FixedUpdate,
                (
                    (spawn_waves, move_motes, update_lenses, move_orbs)
                        .chain()
                        .after(orbit_prism)
                        .before(damage_enemies),
                    (defeat_enemies, hero_hits, reset_room)
                        .chain()
                        .after(damage_enemies),
                    (age_bursts, fade_enemy_flash, tick_invulnerability),
                ),
            );
    }
}

fn lens_bundle(center: Vec2, tuning: &EnemyTuning) -> impl Bundle {
    (
        Name::new("Lens"),
        Enemy {
            kind: EnemyKind::Lens,
            health: tuning.lens_health,
            flash: 0.0,
        },
        Lens {
            cooldown: tuning.lens_fire_interval,
        },
        Position(center),
    )
}

fn spawn_placed_enemies(mut commands: Commands, tuning: Res<EnemyTuning>) {
    for center in test_lenses() {
        commands.spawn(lens_bundle(center, &tuning));
    }
}

fn spawn_waves(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<EnemyTuning>,
    mut waves: ResMut<WaveTimer>,
    heroes: Query<&Position, With<Hero>>,
) {
    let interval = std::time::Duration::from_secs_f32(tuning.wave_interval.max(0.5));
    if waves.timer.duration() != interval {
        waves.timer.set_duration(interval);
    }
    waves.timer.tick(time.delta());
    if !waves.timer.just_finished() {
        return;
    }
    let Ok(hero) = heroes.single() else {
        return;
    };
    let index = waves.count;
    waves.count += 1;
    for spawn in wave_spawns(hero.0.x, index, tuning.wave_size) {
        commands.spawn((
            Name::new("Mote"),
            Enemy {
                kind: EnemyKind::Mote,
                health: tuning.mote_health,
                flash: 0.0,
            },
            Mote {
                base_y: spawn.start.y,
                dir: spawn.dir,
                phase: spawn.phase,
                age: 0.0,
            },
            Position(spawn.start),
            PreviousPosition(spawn.start),
        ));
    }
}

/// Drift motes along their sine waves; remove them once they've crossed the
/// room or grown too old. Motes pass through solids.
fn move_motes(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<EnemyTuning>,
    bounds: Option<Res<RoomBounds>>,
    mut motes: Query<(Entity, &mut Mote, &mut Position, &mut PreviousPosition)>,
) {
    let dt = time.delta_secs();
    for (entity, mut mote, mut pos, mut prev) in &mut motes {
        mote.age += dt;
        prev.0 = pos.0;
        pos.0.x += mote.dir * tuning.mote_speed * dt;
        pos.0.y = mote.base_y
            + mote_offset(
                mote.age,
                tuning.mote_amplitude,
                tuning.mote_period,
                mote.phase,
            );

        let left_room = bounds.as_ref().is_some_and(|b| {
            (mote.dir < 0.0 && pos.0.x < b.0.min.x - SPAWN_DISTANCE)
                || (mote.dir > 0.0 && pos.0.x > b.0.max.x + SPAWN_DISTANCE)
        });
        if left_room || mote.age > MOTE_MAX_AGE {
            commands.entity(entity).despawn();
        }
    }
}

/// Lenses fire an orb at the hero when it's in range and in view.
fn update_lenses(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<EnemyTuning>,
    solids: Query<&Solid>,
    heroes: Query<&Position, With<Hero>>,
    mut lenses: Query<(&Position, &mut Lens), Without<Hero>>,
) {
    let Ok(hero) = heroes.single() else {
        return;
    };
    let solids: Vec<_> = solids.iter().map(|s| s.0).collect();
    for (pos, mut lens) in &mut lenses {
        lens.cooldown -= time.delta_secs();
        if lens.cooldown > 0.0 {
            continue;
        }
        let to_hero = hero.0 - pos.0;
        if to_hero.length() > tuning.lens_range || !line_of_sight(pos.0, hero.0, &solids) {
            continue;
        }
        let dir = to_hero.normalize_or(Vec2::X);
        let start = pos.0 + dir * (LENS_RADIUS + ORB_RADIUS);
        commands.spawn((
            Name::new("Orb"),
            Orb {
                vel: dir * tuning.orb_speed,
                age: 0.0,
            },
            Position(start),
            PreviousPosition(start),
        ));
        lens.cooldown = tuning.lens_fire_interval;
    }
}

/// Move orbs; solids and the prism stop them.
fn move_orbs(
    mut commands: Commands,
    time: Res<Time>,
    solids: Query<&Solid>,
    prisms: Query<&Position, (With<Prism>, Without<Orb>)>,
    mut orbs: Query<(Entity, &mut Orb, &mut Position, &mut PreviousPosition)>,
) {
    let dt = time.delta_secs();
    let prism = prisms.single().ok().map(|p| p.0);
    for (entity, mut orb, mut pos, mut prev) in &mut orbs {
        orb.age += dt;
        prev.0 = pos.0;
        pos.0 += orb.vel * dt;
        let blocked_by_prism =
            prism.is_some_and(|p| circles_overlap(pos.0, ORB_RADIUS, p, PRISM_RADIUS));
        let hit_solid = solids
            .iter()
            .any(|s| circle_hits_solid(pos.0, ORB_RADIUS, &s.0));
        if blocked_by_prism || hit_solid || orb.age > ORB_MAX_AGE {
            commands.entity(entity).despawn();
        }
    }
}

/// Remove enemies with no health left, leaving a burst and sometimes a
/// power-up.
fn defeat_enemies(
    mut commands: Commands,
    mut kills: ResMut<Kills>,
    mut rng: ResMut<DropRng>,
    abilities: Res<Abilities>,
    powerup_tuning: Res<PowerUpTuning>,
    enemies: Query<(Entity, &Enemy, &Position)>,
) {
    for (entity, enemy, pos) in &enemies {
        if enemy.health > 0 {
            continue;
        }
        commands.entity(entity).despawn();
        if let Some(power) = roll_drop(&mut rng, *abilities, powerup_tuning.drop_chance) {
            commands.spawn(pickup_bundle(power, pos.0));
        }
        commands.spawn((
            Name::new("Burst"),
            Burst {
                radius: enemy.radius() * 2.5,
                age: 0.0,
            },
            Position(pos.0),
        ));
        match enemy.kind {
            EnemyKind::Mote => kills.motes += 1,
            EnemyKind::Lens => kills.lenses += 1,
        }
    }
}

/// Touching an enemy or orb costs a life and every ability, and respawns the
/// hero; with no lives left, the room resets.
fn hero_hits(
    mut commands: Commands,
    tuning: Res<EnemyTuning>,
    mut lives: ResMut<Lives>,
    mut abilities: ResMut<Abilities>,
    mut reset: ResMut<ResetRoom>,
    enemies: Query<(&Position, &Enemy), Without<Hero>>,
    orbs: Query<(Entity, &Position), (With<Orb>, Without<Hero>)>,
    mut heroes: Query<
        (
            &mut Position,
            &mut PreviousPosition,
            &mut Velocity,
            &mut Spin,
            &mut Facing,
            &mut Invulnerable,
        ),
        With<Hero>,
    >,
) {
    let Ok((mut pos, mut prev, mut vel, mut spin, mut facing, mut invulnerable)) =
        heroes.single_mut()
    else {
        return;
    };
    if invulnerable.0 > 0.0 {
        return;
    }
    let touched_enemy = enemies
        .iter()
        .any(|(p, e)| circles_overlap(pos.0, HERO_HURT_RADIUS, p.0, e.radius()));
    let touched_orb = orbs
        .iter()
        .find(|(_, p)| circles_overlap(pos.0, HERO_HURT_RADIUS, p.0, ORB_RADIUS));
    if !touched_enemy && touched_orb.is_none() {
        return;
    }
    if let Some((orb, _)) = touched_orb {
        commands.entity(orb).despawn();
    }

    lives.0 = lives.0.saturating_sub(1);
    *abilities = Abilities::default();
    if lives.0 == 0 {
        reset.0 = true;
    }
    pos.0 = HERO_SPAWN;
    prev.0 = HERO_SPAWN;
    *vel = Velocity::default();
    *spin = Spin::default();
    *facing = Facing::default();
    invulnerable.0 = tuning.respawn_invulnerability;
}

/// Clear and repopulate the room, refilling lives and energy and removing
/// pickups and abilities.
pub fn reset_room(
    mut commands: Commands,
    tuning: Res<EnemyTuning>,
    mut reset: ResMut<ResetRoom>,
    mut lives: ResMut<Lives>,
    mut kills: ResMut<Kills>,
    mut energy: ResMut<BeamEnergy>,
    mut waves: ResMut<WaveTimer>,
    mut abilities: ResMut<Abilities>,
    clear: Query<Entity, Or<(With<Enemy>, With<Orb>, With<Burst>, With<Pickup>)>>,
) {
    if !reset.0 {
        return;
    }
    reset.0 = false;
    for entity in &clear {
        commands.entity(entity).despawn();
    }
    for center in test_lenses() {
        commands.spawn(lens_bundle(center, &tuning));
    }
    *lives = Lives::default();
    *kills = Kills::default();
    *energy = BeamEnergy::default();
    *waves = WaveTimer::default();
    *abilities = Abilities::default();
}

fn age_bursts(mut commands: Commands, time: Res<Time>, mut bursts: Query<(Entity, &mut Burst)>) {
    for (entity, mut burst) in &mut bursts {
        burst.age += time.delta_secs();
        if burst.age >= BURST_LIFETIME {
            commands.entity(entity).despawn();
        }
    }
}

fn fade_enemy_flash(time: Res<Time>, mut enemies: Query<&mut Enemy>) {
    for mut enemy in &mut enemies {
        if enemy.flash > 0.0 {
            enemy.flash = (enemy.flash - time.delta_secs()).max(0.0);
        }
    }
}

fn tick_invulnerability(time: Res<Time>, mut heroes: Query<&mut Invulnerable>) {
    for mut invulnerable in &mut heroes {
        if invulnerable.0 > 0.0 {
            invulnerable.0 = (invulnerable.0 - time.delta_secs()).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::f32::consts::FRAC_PI_2;
    use std::time::Duration;

    #[test]
    fn mote_offset_follows_a_sine_wave() {
        assert!(mote_offset(0.0, 60.0, 1.2, 0.0).abs() < 1e-4);
        assert!((mote_offset(0.3, 60.0, 1.2, 0.0) - 60.0).abs() < 1e-3);
        assert!((mote_offset(0.0, 60.0, 1.2, FRAC_PI_2) - 60.0).abs() < 1e-3);
    }

    #[test]
    fn waves_alternate_sides_and_head_towards_the_hero() {
        let first = wave_spawns(1000.0, 0, 5);
        assert_eq!(first.len(), 5);
        assert!(
            first
                .iter()
                .all(|m| m.start.x > 1000.0 + SPAWN_DISTANCE - 1.0)
        );
        assert!(first.iter().all(|m| (m.dir + 1.0).abs() < f32::EPSILON));

        let second = wave_spawns(1000.0, 1, 5);
        assert!(
            second
                .iter()
                .all(|m| m.start.x < 1000.0 - SPAWN_DISTANCE + 1.0)
        );
        assert!(second.iter().all(|m| (m.dir - 1.0).abs() < f32::EPSILON));
        assert!((first[0].start.y - second[0].start.y).abs() > 1.0);
    }

    #[test]
    fn line_of_sight_is_blocked_by_solids() {
        let wall = Aabb::new(Vec2::new(100.0, -50.0), Vec2::new(120.0, 50.0));
        assert!(!line_of_sight(Vec2::ZERO, Vec2::new(200.0, 0.0), &[wall]));
        assert!(line_of_sight(Vec2::ZERO, Vec2::new(90.0, 0.0), &[wall]));
        assert!(line_of_sight(Vec2::ZERO, Vec2::new(200.0, 200.0), &[wall]));
        assert!(line_of_sight(Vec2::ZERO, Vec2::ZERO, &[wall]));
    }

    #[test]
    fn circle_overlap_and_solid_contact() {
        assert!(circles_overlap(
            Vec2::ZERO,
            10.0,
            Vec2::new(15.0, 0.0),
            10.0
        ));
        assert!(!circles_overlap(
            Vec2::ZERO,
            10.0,
            Vec2::new(25.0, 0.0),
            10.0
        ));
        let solid = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        assert!(circle_hits_solid(Vec2::new(13.0, 5.0), 5.0, &solid));
        assert!(!circle_hits_solid(Vec2::new(20.0, 5.0), 5.0, &solid));
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

    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
        }
    }

    /// Test frames (16 ms each) until just after the first wave arrives.
    fn frames_until_first_wave() -> usize {
        let frames = (EnemyTuning::default().wave_interval / 0.016).ceil();
        // Small positive whole number.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = frames as usize;
        frames + 10
    }

    #[test]
    fn placed_lenses_spawn_with_the_room() {
        let mut app = sim_app();
        assert_eq!(count::<Lens>(&mut app), test_lenses().len());
    }

    #[test]
    fn waves_arrive_on_the_timer() {
        let mut app = sim_app();
        assert_eq!(count::<Mote>(&mut app), 0);
        // Just over one wave interval.
        run(&mut app, frames_until_first_wave());
        assert_eq!(
            count::<Mote>(&mut app),
            EnemyTuning::default().wave_size as usize
        );
    }

    fn hero_position(app: &mut App) -> Vec2 {
        let world = app.world_mut();
        world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0
    }

    fn spawn_mote_at(app: &mut App, at: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                Enemy {
                    kind: EnemyKind::Mote,
                    health: EnemyTuning::default().mote_health,
                    flash: 0.0,
                },
                Position(at),
            ))
            .id()
    }

    #[test]
    fn beam_defeats_a_mote_leaving_a_burst() {
        let mut app = sim_app();
        // A stationary mote straight ahead of the hero at its current height.
        let hero = hero_position(&mut app);
        let mote = spawn_mote_at(&mut app, hero + Vec2::new(300.0, 0.0));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        // The hero keeps bouncing, so allow a few passes through the line.
        for _ in 0..240 {
            app.update();
            if app.world().get_entity(mote).is_err() {
                break;
            }
        }
        assert!(
            app.world().get_entity(mote).is_err(),
            "mote should be defeated"
        );
        assert_eq!(app.world().resource::<Kills>().motes, 1);
    }

    #[test]
    fn touching_an_enemy_costs_a_life_and_respawns_the_hero() {
        let mut app = sim_app();
        let hero = hero_position(&mut app);
        spawn_mote_at(&mut app, hero);
        app.update();
        assert_eq!(app.world().resource::<Lives>().0, STARTING_LIVES - 1);
        let world = app.world_mut();
        let invulnerable = world
            .query_filtered::<&Invulnerable, With<Hero>>()
            .single(world)
            .unwrap()
            .0;
        assert!(invulnerable > 0.0);
        assert!(hero_position(&mut app).distance(HERO_SPAWN) < 50.0);
    }

    #[test]
    fn losing_every_life_resets_the_room() {
        let mut app = sim_app();
        run(&mut app, frames_until_first_wave());
        assert!(count::<Mote>(&mut app) > 0);
        app.insert_resource(Lives(1));
        let hero = hero_position(&mut app);
        spawn_mote_at(&mut app, hero);
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Lives>().0, STARTING_LIVES);
        assert_eq!(count::<Mote>(&mut app), 0);
        assert_eq!(count::<Lens>(&mut app), test_lenses().len());
    }

    #[test]
    fn lens_fires_at_a_hero_in_range_and_view() {
        let mut app = sim_app();
        // A lens right next to the hero's bounce path, in open space.
        let hero = hero_position(&mut app);
        app.world_mut().spawn((
            Enemy {
                kind: EnemyKind::Lens,
                health: 20,
                flash: 0.0,
            },
            Lens { cooldown: 0.0 },
            Position(hero + Vec2::new(500.0, 0.0)),
        ));
        app.update();
        assert!(count::<Orb>(&mut app) >= 1);
    }

    #[test]
    fn the_prism_blocks_orbs() {
        let mut app = sim_app();
        let prism = {
            let world = app.world_mut();
            world
                .query_filtered::<&Position, With<Prism>>()
                .single(world)
                .unwrap()
                .0
        };
        // An orb about to reach the prism from the right.
        let start = prism + Vec2::new(PRISM_RADIUS + ORB_RADIUS + 2.0, 0.0);
        app.world_mut().spawn((
            Orb {
                vel: Vec2::new(-400.0, 0.0),
                age: 0.0,
            },
            Position(start),
            PreviousPosition(start),
        ));
        app.update();
        assert_eq!(count::<Orb>(&mut app), 0);
    }

    #[test]
    fn an_orb_hitting_the_hero_costs_a_life_and_is_used_up() {
        let mut app = sim_app();
        let hero = hero_position(&mut app);
        app.world_mut().spawn((
            Orb {
                vel: Vec2::ZERO,
                age: 0.0,
            },
            Position(hero),
            PreviousPosition(hero),
        ));
        app.update();
        assert_eq!(app.world().resource::<Lives>().0, STARTING_LIVES - 1);
        assert_eq!(count::<Orb>(&mut app), 0);
    }

    #[test]
    fn invulnerability_protects_then_wears_off() {
        let mut app = sim_app();
        let hero = hero_position(&mut app);
        spawn_mote_at(&mut app, hero);
        app.update();
        assert_eq!(app.world().resource::<Lives>().0, STARTING_LIVES - 1);
        // Another enemy at the respawn point: ignored while invulnerable.
        spawn_mote_at(&mut app, HERO_SPAWN);
        run(&mut app, 30);
        assert_eq!(app.world().resource::<Lives>().0, STARTING_LIVES - 1);
        // Once it wears off, the mote waiting at the respawn point costs
        // another life (allow a second for the bounce to reach it).
        let wear_off = (EnemyTuning::default().respawn_invulnerability / 0.016).ceil();
        // Small positive whole number.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let wear_off = wear_off as usize;
        run(&mut app, wear_off + 60);
        assert_eq!(app.world().resource::<Lives>().0, STARTING_LIVES - 2);
    }

    #[test]
    fn defeated_lenses_count_and_bursts_fade_away() {
        let mut app = sim_app();
        let lens = {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<Lens>>()
                .iter(world)
                .next()
                .unwrap()
        };
        app.world_mut().get_mut::<Enemy>(lens).unwrap().health = 0;
        app.update();
        assert_eq!(app.world().resource::<Kills>().lenses, 1);
        assert_eq!(count::<Burst>(&mut app), 1);
        // Bursts last BURST_LIFETIME (0.35 s).
        run(&mut app, 30);
        assert_eq!(count::<Burst>(&mut app), 0);
    }

    #[test]
    fn reset_room_flag_restores_everything() {
        let mut app = sim_app();
        app.insert_resource(Lives(1));
        app.insert_resource(Kills {
            motes: 4,
            lenses: 1,
        });
        app.insert_resource(ResetRoom(true));
        app.update();
        assert_eq!(*app.world().resource::<Lives>(), Lives::default());
        assert_eq!(*app.world().resource::<Kills>(), Kills::default());
        assert!(!app.world().resource::<ResetRoom>().0);
    }
}
