//! Platforms that change: moving platforms that run along their track,
//! crumbling platforms that give way after a few landings and grow back,
//! and blocks that only a split beam of their colour can break.
//!
//! All three are walls with a `Solid` like any other, so the physics, the
//! beam and enemies treat them as walls; this module moves them, and takes
//! their `Solid` away while they're crumbled or broken.

use bevy::prelude::*;

use crate::enemy::{Burst, ResetRoom, reset_room};
use crate::events::GameEvent;
use crate::hero::{HERO_RADIUS, Hero, Position, move_hero};
use crate::level::Solid;
use crate::level::format::Mover;
use crate::light::Primary;
use crate::physics::{Aabb, Traits};

/// Seconds a crumbled platform takes to grow back.
pub const CRUMBLE_REGROW: f32 = 3.0;

/// Damage ticks from its colour's beam that break a block.
pub const BREAK_TICKS: u32 = 6;

/// How close the hero's base must be to a platform's top to have landed on
/// it.
const LANDED_TOLERANCE: f32 = 1.5;

/// A platform that changes, so it's drawn on its own rather than with the
/// level's other walls.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Changing;

/// A moving platform and the track it runs along.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Moving(pub Mover);

/// How fast a moving platform is going, for the physics.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct PlatformVelocity(pub Vec2);

/// A crumbling platform: landings left before it gives way, and while
/// crumbled, seconds until it grows back.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Crumbling {
    /// Where it stands.
    pub aabb: Aabb,
    pub landings: u8,
    pub left: u8,
    pub regrow: f32,
}

/// A block that a split beam of `colour` breaks after `BREAK_TICKS`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Breakable {
    pub aabb: Aabb,
    pub colour: Primary,
    pub health: u32,
}

/// Seconds since the room started, which sets where moving platforms are.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct PlatformClock(pub f32);

#[derive(Debug, Default)]
pub struct PlatformsPlugin;

impl Plugin for PlatformsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlatformClock>().add_systems(
            FixedUpdate,
            (
                restore_platforms.before(reset_room),
                move_platforms.before(move_hero),
                (
                    crumble_platforms,
                    announce_crumbles,
                    regrow_platforms,
                    break_blocks,
                )
                    .chain()
                    .after(move_hero),
            ),
        );
    }
}

/// The components a wall with `traits` needs if it changes.
pub fn insert_changing(wall: &mut EntityCommands, aabb: Aabb, traits: Traits) {
    if traits.crumble > 0 {
        wall.insert((
            Changing,
            Crumbling {
                aabb,
                landings: traits.crumble,
                left: traits.crumble,
                regrow: 0.0,
            },
        ));
    }
    if let Some(colour) = traits.breakable {
        wall.insert((
            Changing,
            Breakable {
                aabb,
                colour,
                health: BREAK_TICKS,
            },
        ));
    }
}

/// Put every moving platform where the clock says, with its velocity.
fn move_platforms(
    time: Res<Time>,
    mut clock: ResMut<PlatformClock>,
    mut movers: Query<(&Moving, &mut Solid, &mut PlatformVelocity)>,
) {
    clock.0 += time.delta_secs();
    for (moving, mut solid, mut velocity) in &mut movers {
        solid.0 = moving.0.aabb_at(clock.0);
        velocity.0 = moving.0.velocity(clock.0);
    }
}

/// Whether the hero at `at` has just landed on `aabb`.
fn landed_on(at: Vec2, aabb: &Aabb) -> bool {
    (at.y - HERO_RADIUS - aabb.max.y).abs() < LANDED_TOLERANCE
        && at.x + HERO_RADIUS > aabb.min.x
        && at.x - HERO_RADIUS < aabb.max.x
}

/// Each landing on a crumbling platform wears it down; the last one takes
/// it away.
fn crumble_platforms(
    mut commands: Commands,
    mut bounces: MessageReader<GameEvent>,
    mut platforms: Query<(Entity, &mut Crumbling), With<Solid>>,
) {
    for event in bounces.read() {
        let &GameEvent::Bounced { at, .. } = event else {
            continue;
        };
        for (entity, mut platform) in &mut platforms {
            if !landed_on(at, &platform.aabb) {
                continue;
            }
            platform.left = platform.left.saturating_sub(1);
            if platform.left == 0 {
                platform.regrow = CRUMBLE_REGROW;
                commands.entity(entity).remove::<Solid>();
            }
        }
    }
}

/// Say when a crumbling platform has given way. Separate from crumbling,
/// which reads game events and so can't also write them.
fn announce_crumbles(
    mut removed: RemovedComponents<Solid>,
    mut events: MessageWriter<GameEvent>,
    platforms: Query<&Crumbling>,
) {
    for entity in removed.read() {
        if let Ok(platform) = platforms.get(entity) {
            events.write(GameEvent::PlatformCrumbled {
                at: platform.aabb.center(),
            });
        }
    }
}

/// A crumbled platform grows back once its time is up and the hero is out
/// of the way.
fn regrow_platforms(
    mut commands: Commands,
    time: Res<Time>,
    heroes: Query<&Position, With<Hero>>,
    mut platforms: Query<(Entity, &mut Crumbling), Without<Solid>>,
) {
    let dt = time.delta_secs();
    for (entity, mut platform) in &mut platforms {
        platform.regrow -= dt;
        let blocked = heroes.iter().any(|hero| {
            Aabb::from_center(hero.0, Vec2::splat(HERO_RADIUS)).overlaps(&platform.aabb)
        });
        if platform.regrow <= 0.0 && !blocked {
            platform.left = platform.landings;
            commands.entity(entity).insert(Solid(platform.aabb));
        }
    }
}

/// A block with no health left breaks.
fn break_blocks(
    mut commands: Commands,
    mut events: MessageWriter<GameEvent>,
    blocks: Query<(Entity, &Breakable), With<Solid>>,
) {
    for (entity, block) in &blocks {
        if block.health == 0 {
            commands.entity(entity).remove::<Solid>();
            commands.spawn((
                Name::new("Burst"),
                Burst {
                    radius: block.aabb.size().max_element() * 0.75,
                    age: 0.0,
                    colour: Some(block.colour),
                },
                Position(block.aabb.center()),
            ));
            events.write(GameEvent::BlockBroken {
                at: block.aabb.center(),
                colour: block.colour,
            });
        }
    }
}

/// A fresh room puts every platform back as it was.
fn restore_platforms(
    mut commands: Commands,
    reset: Res<ResetRoom>,
    mut clock: ResMut<PlatformClock>,
    mut crumbling: Query<(Entity, &mut Crumbling)>,
    mut blocks: Query<(Entity, &mut Breakable)>,
) {
    if !reset.0 {
        return;
    }
    clock.0 = 0.0;
    for (entity, mut platform) in &mut crumbling {
        platform.left = platform.landings;
        platform.regrow = 0.0;
        commands.entity(entity).insert(Solid(platform.aabb));
    }
    for (entity, mut block) in &mut blocks {
        block.health = BREAK_TICKS;
        commands.entity(entity).insert(Solid(block.aabb));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mover(low: f32, high: f32) -> Mover {
        Mover {
            aabb: Aabb::new(Vec2::ZERO, Vec2::new(80.0, 10.0)),
            hue: None,
            traits: Traits::default(),
            axis: Vec2::X,
            low,
            high,
            speed: 100.0,
        }
    }

    #[test]
    fn a_mover_runs_out_to_each_end_and_back() {
        let mover = mover(-100.0, 200.0);
        assert!(mover.offset(0.0).abs() < 1e-4);
        assert!(
            (mover.offset(2.0) - 200.0).abs() < 1e-3,
            "out to the far end"
        );
        assert!((mover.offset(3.0) - 100.0).abs() < 1e-3, "on its way back");
        assert!((mover.offset(5.0) + 100.0).abs() < 1e-3, "at the near end");
        assert!(mover.offset(6.0).abs() < 1e-3, "home again");
        assert!(mover.velocity(1.0).x > 0.0 && mover.velocity(3.0).x < 0.0);
        assert_eq!(mover.stops(100.0).len(), 4);
    }

    #[test]
    fn landing_needs_the_hero_on_top() {
        let aabb = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(80.0, 10.0));
        assert!(landed_on(Vec2::new(40.0, 10.0 + HERO_RADIUS), &aabb));
        assert!(!landed_on(Vec2::new(40.0, 50.0 + HERO_RADIUS), &aabb));
        assert!(!landed_on(Vec2::new(200.0, 10.0 + HERO_RADIUS), &aabb));
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::SimulationPlugin))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f64(1.0 / crate::PHYSICS_HZ),
            ));
        app.update();
        app
    }

    /// Run `steps` frames, each one physics step.
    fn fixed(app: &mut App, steps: usize) {
        for _ in 0..steps {
            app.update();
        }
    }

    #[test]
    fn a_crumbling_platform_gives_way_then_grows_back() {
        let mut app = app();
        let aabb = Aabb::new(Vec2::new(4000.0, 4000.0), Vec2::new(4120.0, 4010.0));
        let platform = app
            .world_mut()
            .spawn((
                Solid(aabb),
                Crumbling {
                    aabb,
                    landings: 2,
                    left: 2,
                    regrow: 0.0,
                },
            ))
            .id();
        let land = |app: &mut App| {
            app.world_mut().write_message(GameEvent::Bounced {
                at: Vec2::new(4060.0, 4010.0 + HERO_RADIUS),
                spin: 0,
            });
            fixed(app, 1);
        };
        land(&mut app);
        assert!(app.world().get::<Solid>(platform).is_some());
        land(&mut app);
        assert!(app.world().get::<Solid>(platform).is_none(), "gave way");
        // Well after the regrow time (400 steps is over 3 s) it's back, with
        // its landings restored.
        fixed(&mut app, 400);
        assert!(app.world().get::<Solid>(platform).is_some());
        assert_eq!(app.world().get::<Crumbling>(platform).unwrap().left, 2);
    }

    #[test]
    fn a_block_breaks_when_out_of_health_and_a_reset_restores_it() {
        let mut app = app();
        let aabb = Aabb::new(Vec2::new(4000.0, 4000.0), Vec2::new(4040.0, 4040.0));
        let block = app
            .world_mut()
            .spawn((
                Solid(aabb),
                Breakable {
                    aabb,
                    colour: Primary::Red,
                    health: 0,
                },
            ))
            .id();
        fixed(&mut app, 1);
        assert!(app.world().get::<Solid>(block).is_none());
        app.world_mut().resource_mut::<ResetRoom>().0 = true;
        fixed(&mut app, 1);
        assert!(app.world().get::<Solid>(block).is_some());
        assert_eq!(
            app.world().get::<Breakable>(block).unwrap().health,
            BREAK_TICKS
        );
    }

    #[test]
    fn moving_platforms_follow_the_clock() {
        let mut app = app();
        let platform = mover(0.0, 400.0);
        let entity = app
            .world_mut()
            .spawn((
                Solid(platform.aabb),
                Moving(platform),
                PlatformVelocity::default(),
            ))
            .id();
        fixed(&mut app, 60);
        let moved = app.world().get::<Solid>(entity).unwrap().0.min.x;
        assert!(moved > 0.0, "moved to {moved}");
        assert!(app.world().get::<PlatformVelocity>(entity).unwrap().0.x > 0.0);
    }
}
