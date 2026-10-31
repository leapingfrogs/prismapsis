//! Level geometry. For now a single hand-built test room for tuning movement
//! and trying out colour gating.

use bevy::prelude::*;

use crate::light::{ExitGate, Hidden, Hue, Phantom, Primary, Shade};
use crate::physics::Aabb;
use crate::powerup::{Placed, PowerUp, pickup_bundle};

/// Static geometry the hero bounces off.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Solid(pub Aabb);

/// Background decoration: no collision, there to show colour returning.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Scenery(pub Aabb);

/// World-space extent of the current room, used to bound the camera.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RoomBounds(pub Aabb);

/// Size of the test room in world units. Its height matches the design
/// resolution, so the whole room is visible vertically.
pub const TEST_ROOM_SIZE: Vec2 = Vec2::new(4800.0, 1080.0);

/// Thickness of the boundary floor, ceiling and walls.
const BORDER: f32 = 80.0;

const fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Aabb {
    Aabb::new(Vec2::new(x0, y0), Vec2::new(x1, y1))
}

/// Named solids making up the test room, with the hue each takes on as
/// colour returns.
///
/// Each feature exercises one movement rule: walls reverse, the overhang caps
/// the bounce, the step and shelf test landing on raised floors, and the
/// floating platform needs a higher apex.
#[must_use]
pub fn test_room() -> Vec<(&'static str, Aabb, Hue)> {
    let (w, h) = (TEST_ROOM_SIZE.x, TEST_ROOM_SIZE.y);
    vec![
        ("Floor", rect(0.0, 0.0, w, BORDER), Hue::Green),
        ("Ceiling", rect(0.0, h - BORDER, w, h), Hue::Blue),
        ("Left wall", rect(0.0, BORDER, BORDER, h - BORDER), Hue::Red),
        (
            "Right wall",
            rect(w - BORDER, BORDER, w, h - BORDER),
            Hue::Red,
        ),
        (
            "Low overhang",
            rect(1300.0, 260.0, 1900.0, 320.0),
            Hue::Yellow,
        ),
        ("Step", rect(2400.0, BORDER, 2800.0, 240.0), Hue::Cyan),
        ("Shelf", rect(3400.0, BORDER, 3900.0, 380.0), Hue::Magenta),
        (
            "Floating platform",
            rect(4100.0, 600.0, 4500.0, 640.0),
            Hue::White,
        ),
    ]
}

/// Phantom platforms, one per hue except white.
///
/// The low ones are reachable from the floor; the yellow and magenta ones
/// sit above the red and green, so reaching them takes two restored colours;
/// the blue one is a step up to the floating platform.
#[must_use]
pub fn test_phantoms() -> Vec<(&'static str, Aabb, Hue)> {
    vec![
        ("Red phantom", rect(600.0, 330.0, 1000.0, 360.0), Hue::Red),
        (
            "Yellow phantom",
            rect(750.0, 640.0, 1100.0, 670.0),
            Hue::Yellow,
        ),
        (
            "Cyan phantom",
            rect(2000.0, 330.0, 2300.0, 360.0),
            Hue::Cyan,
        ),
        (
            "Green phantom",
            rect(3000.0, 330.0, 3300.0, 360.0),
            Hue::Green,
        ),
        (
            "Magenta phantom",
            rect(3050.0, 640.0, 3300.0, 670.0),
            Hue::Magenta,
        ),
        (
            "Blue phantom",
            rect(4150.0, 330.0, 4450.0, 360.0),
            Hue::Blue,
        ),
    ]
}

/// Pickups hidden until their hue returns, each waiting above the phantom
/// platform of the same hue.
#[must_use]
pub fn test_hidden_pickups() -> Vec<(PowerUp, Vec2, Hue)> {
    vec![
        (PowerUp::Piercing, Vec2::new(925.0, 740.0), Hue::Yellow),
        (PowerUp::RapidFire, Vec2::new(3175.0, 740.0), Hue::Magenta),
    ]
}

/// Background shapes that colour in as the spectrum returns.
#[must_use]
pub fn test_scenery() -> Vec<(Aabb, Hue)> {
    vec![
        (rect(100.0, 80.0, 900.0, 260.0), Hue::Green),
        (rect(1050.0, 80.0, 1250.0, 820.0), Hue::Blue),
        (rect(1450.0, 500.0, 1750.0, 900.0), Hue::Magenta),
        (rect(1950.0, 80.0, 2600.0, 200.0), Hue::Yellow),
        (rect(2900.0, 80.0, 3050.0, 900.0), Hue::Cyan),
        (rect(3500.0, 450.0, 3800.0, 950.0), Hue::Red),
        (rect(3950.0, 80.0, 4600.0, 300.0), Hue::Green),
        (rect(4250.0, 760.0, 4450.0, 960.0), Hue::Yellow),
        (rect(500.0, 880.0, 2800.0, 930.0), Hue::Cyan),
        (rect(2800.0, 880.0, 4700.0, 930.0), Hue::Magenta),
    ]
}

/// The exit, against the right wall.
#[must_use]
pub fn test_exit() -> Aabb {
    rect(4620.0, BORDER, TEST_ROOM_SIZE.x - BORDER, 400.0)
}

/// Lens turrets placed in the test room, with their colours: one at the
/// far end of the step, one at the far end of the shelf, leaving room to
/// land.
#[must_use]
pub fn test_lenses() -> Vec<(Vec2, Primary)> {
    vec![
        (Vec2::new(2740.0, 270.0), Primary::Red),
        (Vec2::new(3850.0, 410.0), Primary::Blue),
    ]
}

pub fn spawn_test_room(mut commands: Commands) {
    commands.insert_resource(RoomBounds(Aabb::new(Vec2::ZERO, TEST_ROOM_SIZE)));
    for (name, aabb, hue) in test_room() {
        commands.spawn((Name::new(name), Solid(aabb), Shade(hue)));
    }
    for (name, aabb, hue) in test_phantoms() {
        commands.spawn((Name::new(name), Phantom { hue, aabb }));
    }
    for (aabb, hue) in test_scenery() {
        commands.spawn((Name::new("Scenery"), Scenery(aabb), Shade(hue)));
    }
    commands.spawn((Name::new("Exit"), ExitGate(test_exit())));
    spawn_hidden_pickups(&mut commands);
}

/// The room's hidden pickups; also respawned when the room resets.
pub fn spawn_hidden_pickups(commands: &mut Commands) {
    for (power, at, hue) in test_hidden_pickups() {
        commands.spawn((pickup_bundle(power, at), Placed, Hidden(hue)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside_room(aabb: &Aabb) -> bool {
        let bounds = Aabb::new(Vec2::ZERO, TEST_ROOM_SIZE);
        aabb.min.cmpge(bounds.min).all() && aabb.max.cmple(bounds.max).all()
    }

    fn apart(a: &Aabb, b: &Aabb) -> bool {
        a.max.x <= b.min.x || b.max.x <= a.min.x || a.max.y <= b.min.y || b.max.y <= a.min.y
    }

    #[test]
    fn test_room_solids_lie_inside_the_room() {
        for (name, solid, _) in test_room() {
            assert!(inside_room(&solid), "{name} outside room");
            assert!(solid.size().cmpgt(Vec2::ZERO).all(), "{name} has no area");
        }
        assert!(inside_room(&test_exit()));
        for (aabb, _) in test_scenery() {
            assert!(inside_room(&aabb));
        }
    }

    #[test]
    fn phantoms_are_inside_the_room_and_clear_of_solids() {
        let phantoms = test_phantoms();
        let mut hues: Vec<Hue> = phantoms.iter().map(|p| p.2).collect();
        hues.dedup();
        assert_eq!(hues.len(), 6, "one per hue except white");
        for (name, phantom, _) in &phantoms {
            assert!(inside_room(phantom), "{name} outside room");
            for (solid_name, solid, _) in test_room() {
                assert!(apart(phantom, &solid), "{name} overlaps {solid_name}");
            }
        }
    }

    #[test]
    fn hidden_pickups_wait_above_their_phantom() {
        for (power, at, hue) in test_hidden_pickups() {
            let (_, platform, _) = test_phantoms().into_iter().find(|p| p.2 == hue).unwrap();
            assert!(
                at.x > platform.min.x && at.x < platform.max.x && at.y > platform.max.y,
                "{power:?} not above its platform"
            );
        }
    }

    #[test]
    fn test_lenses_sit_in_open_space_inside_the_room() {
        // Lens radius, kept local so the level doesn't depend on enemies.
        let radius = 30.0;
        for (center, _) in test_lenses() {
            let lens = Aabb::from_center(center, Vec2::splat(radius));
            assert!(inside_room(&lens), "lens at {center} outside room");
            for (solid_name, solid, _) in test_room() {
                assert!(
                    apart(&lens, &solid),
                    "lens at {center} overlaps {solid_name}"
                );
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
        let hidden = world.query::<&Hidden>().iter(world).count();
        assert_eq!(hidden, test_hidden_pickups().len());
    }
}
