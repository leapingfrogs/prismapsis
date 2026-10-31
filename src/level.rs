//! Level geometry. For now a single hand-built test room for tuning movement.

use bevy::prelude::*;

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

/// Centres of the lens turrets placed in the test room: one at the far end
/// of the step, one at the far end of the shelf, leaving room to land.
#[must_use]
pub fn test_lenses() -> Vec<Vec2> {
    vec![Vec2::new(2740.0, 270.0), Vec2::new(3850.0, 410.0)]
}

pub fn spawn_test_room(mut commands: Commands) {
    commands.insert_resource(RoomBounds(Aabb::new(Vec2::ZERO, TEST_ROOM_SIZE)));
    for (name, aabb) in test_room() {
        commands.spawn((Name::new(name), Solid(aabb)));
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
    fn test_lenses_sit_in_open_space_inside_the_room() {
        // Lens radius, kept local so the level doesn't depend on enemies.
        let radius = 30.0;
        let bounds = Aabb::new(Vec2::ZERO, TEST_ROOM_SIZE);
        for center in test_lenses() {
            let lens = Aabb::from_center(center, Vec2::splat(radius));
            assert!(
                lens.min.cmpge(bounds.min).all() && lens.max.cmple(bounds.max).all(),
                "lens at {center} outside room"
            );
            for (solid_name, solid) in test_room() {
                let apart = lens.max.x <= solid.min.x
                    || solid.max.x <= lens.min.x
                    || lens.max.y <= solid.min.y
                    || solid.max.y <= lens.min.y;
                assert!(apart, "lens at {center} overlaps {solid_name}");
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
        assert_eq!(world.resource::<RoomBounds>().0.size(), TEST_ROOM_SIZE);
    }
}
