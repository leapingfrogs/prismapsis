//! Level geometry. For now a single hand-built test room for tuning movement.

use bevy::prelude::*;

use crate::hero::Position;
use crate::physics::Aabb;

/// Static geometry the hero bounces off.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Solid(pub Aabb);

/// World-space extent of the current room, used to bound the camera.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RoomBounds(pub Aabb);

/// Size of the test room in world units. Its height matches the design
/// resolution, so the whole room is visible vertically.
pub const TEST_ROOM_SIZE: Vec2 = Vec2::new(4800.0, 1080.0);

/// Thickness of the boundary floor, ceiling and walls.
const BORDER: f32 = 80.0;

/// Named solids making up the test room.
///
/// Each feature exercises one movement rule: walls reverse, the overhang caps
/// the bounce, the step and shelf test landing on raised floors, and the
/// floating platform needs a higher apex.
#[must_use]
pub fn test_room() -> Vec<(&'static str, Aabb)> {
    let (w, h) = (TEST_ROOM_SIZE.x, TEST_ROOM_SIZE.y);
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Aabb::new(Vec2::new(x0, y0), Vec2::new(x1, y1));
    vec![
        ("Floor", rect(0.0, 0.0, w, BORDER)),
        ("Ceiling", rect(0.0, h - BORDER, w, h)),
        ("Left wall", rect(0.0, BORDER, BORDER, h - BORDER)),
        ("Right wall", rect(w - BORDER, BORDER, w, h - BORDER)),
        ("Low overhang", rect(1300.0, 260.0, 1900.0, 320.0)),
        ("Step", rect(2400.0, BORDER, 2800.0, 240.0)),
        ("Shelf", rect(3400.0, BORDER, 3900.0, 380.0)),
        ("Floating platform", rect(4100.0, 600.0, 4500.0, 640.0)),
    ]
}

/// A dummy that beams can hit, for testing shooting. Counts hits and flashes
/// briefly when struck. Doesn't block the hero.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Target {
    pub radius: f32,
    pub hits: u32,
    /// Seconds of hit flash remaining.
    pub flash: f32,
}

/// Radius of the test room's target dummies.
pub const TARGET_RADIUS: f32 = 28.0;

/// Target dummies placed within the hero's beam heights, plus one high up
/// that only split beams from the prism can reach.
#[must_use]
pub fn test_targets() -> Vec<(&'static str, Vec2)> {
    vec![
        ("Target A", Vec2::new(1100.0, 200.0)),
        ("Target B", Vec2::new(2150.0, 360.0)),
        ("Target C", Vec2::new(3100.0, 150.0)),
        ("Target D", Vec2::new(4300.0, 300.0)),
        ("High target", Vec2::new(2600.0, 760.0)),
    ]
}

pub fn spawn_test_room(mut commands: Commands) {
    commands.insert_resource(RoomBounds(Aabb::new(Vec2::ZERO, TEST_ROOM_SIZE)));
    for (name, aabb) in test_room() {
        commands.spawn((Name::new(name), Solid(aabb)));
    }
    for (name, center) in test_targets() {
        commands.spawn((
            Name::new(name),
            Position(center),
            Target {
                radius: TARGET_RADIUS,
                hits: 0,
                flash: 0.0,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_solids_lie_inside_the_room() {
        let bounds = Aabb::new(Vec2::ZERO, TEST_ROOM_SIZE);
        for (name, solid) in test_room() {
            assert!(
                solid.min.cmpge(bounds.min).all(),
                "{name} below/left of room"
            );
            assert!(
                solid.max.cmple(bounds.max).all(),
                "{name} above/right of room"
            );
            assert!(solid.size().cmpgt(Vec2::ZERO).all(), "{name} has no area");
        }
    }

    #[test]
    fn test_targets_are_in_open_space() {
        let bounds = Aabb::new(Vec2::ZERO, TEST_ROOM_SIZE);
        for (name, center) in test_targets() {
            let target = Aabb::from_center(center, Vec2::splat(TARGET_RADIUS));
            assert!(
                target.min.cmpge(bounds.min).all() && target.max.cmple(bounds.max).all(),
                "{name} outside room"
            );
            for (solid_name, solid) in test_room() {
                let apart = target.max.x <= solid.min.x
                    || solid.max.x <= target.min.x
                    || target.max.y <= solid.min.y
                    || solid.max.y <= target.min.y;
                assert!(apart, "{name} overlaps {solid_name}");
            }
        }
    }

    #[test]
    fn spawning_creates_solids_and_bounds() {
        let mut app = App::new();
        app.add_systems(Startup, spawn_test_room);
        app.update();

        let world = app.world_mut();
        let solids = world.query::<&Solid>().iter(world).count();
        assert_eq!(solids, test_room().len());
        let targets = world.query::<&Target>().iter(world).count();
        assert_eq!(targets, test_targets().len());
        assert_eq!(world.resource::<RoomBounds>().0.size(), TEST_ROOM_SIZE);
    }
}
