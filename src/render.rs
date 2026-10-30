//! Presentation: draws the simulation and moves the camera. Nothing here feeds
//! back into game logic, so it can later be swapped for a 3D presentation.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::hero::{HERO_RADIUS, Hero, Position, PreviousPosition, Spin};
use crate::level::{RoomBounds, Solid};
use crate::physics::BounceTuning;
use crate::prism::{Prism, PrismFrozen};
use std::f32::consts::FRAC_PI_2;

/// Vertical extent of the view in world units, regardless of window size.
pub const DESIGN_HEIGHT: f32 = 1080.0;

/// How quickly the camera catches up with the hero (per second).
const CAMERA_FOLLOW_RATE: f32 = 6.0;

const SOLID_COLOUR: Color = Color::srgb(0.32, 0.34, 0.4);
const HERO_COLOUR: Color = Color::srgb(0.92, 0.92, 0.96);
const HERO_MARK_COLOUR: Color = Color::srgb(0.35, 0.75, 0.95);

#[derive(Debug, Default)]
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera).add_systems(
            Update,
            (
                draw_solids,
                draw_hero,
                draw_prism,
                toggle_fullscreen,
                sync_positions,
                spin_hero_visual,
                follow_hero.after(sync_positions),
                orient_prism.after(sync_positions),
                tint_frozen_prism,
            ),
        );
    }
}

/// Size of the prism triangle (centre to tip, world units).
const PRISM_SIZE: f32 = 20.0;
const PRISM_COLOUR: Color = Color::srgba(0.8, 0.92, 1.0, 0.7);
const PRISM_FROZEN_COLOUR: Color = Color::srgba(1.0, 1.0, 1.0, 0.95);

/// Materials for the prism's normal and frozen states.
#[derive(Resource, Debug)]
struct PrismMaterials {
    normal: Handle<ColorMaterial>,
    frozen: Handle<ColorMaterial>,
}

fn draw_prism(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    prisms: Query<(Entity, &Position), Added<Prism>>,
) {
    for (entity, pos) in &prisms {
        let prism_materials = PrismMaterials {
            normal: materials.add(PRISM_COLOUR),
            frozen: materials.add(PRISM_FROZEN_COLOUR),
        };
        // Equilateral triangle with its tip along +y; `orient_prism` turns
        // the tip to face away from the hero.
        let triangle = Triangle2d::new(
            Vec2::new(0.0, PRISM_SIZE),
            Vec2::from_angle(210f32.to_radians()) * PRISM_SIZE,
            Vec2::from_angle(330f32.to_radians()) * PRISM_SIZE,
        );
        commands.entity(entity).insert((
            Mesh2d(meshes.add(triangle)),
            MeshMaterial2d(prism_materials.normal.clone()),
            Transform::from_translation(pos.0.extend(2.0)),
        ));
        commands.insert_resource(prism_materials);
    }
}

/// Point the prism's tip outward from the hero, using drawn (interpolated)
/// positions so it turns smoothly.
fn orient_prism(
    heroes: Query<&Transform, (With<Hero>, Without<Prism>)>,
    mut prisms: Query<&mut Transform, (With<Prism>, Without<Hero>)>,
) {
    let Ok(hero) = heroes.single() else {
        return;
    };
    for mut transform in &mut prisms {
        let outward = (transform.translation - hero.translation).truncate();
        if outward != Vec2::ZERO {
            transform.rotation = Quat::from_rotation_z(outward.to_angle() - FRAC_PI_2);
        }
    }
}

/// Brighten the prism while it is held in place.
fn tint_frozen_prism(
    frozen: Res<PrismFrozen>,
    materials: Option<Res<PrismMaterials>>,
    mut prisms: Query<&mut MeshMaterial2d<ColorMaterial>, With<Prism>>,
) {
    let Some(materials) = materials else {
        return;
    };
    if !frozen.is_changed() && !materials.is_added() {
        return;
    }
    let handle = if frozen.0 {
        &materials.frozen
    } else {
        &materials.normal
    };
    for mut material in &mut prisms {
        material.0 = handle.clone();
    }
}

/// Marks the main gameplay camera.
#[derive(Component, Debug)]
pub struct MainCamera;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Main Camera"),
        MainCamera,
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: DESIGN_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}

fn draw_solids(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    solids: Query<(Entity, &Solid), Added<Solid>>,
) {
    if solids.is_empty() {
        return;
    }
    let material = materials.add(SOLID_COLOUR);
    for (entity, solid) in &solids {
        let size = solid.0.size();
        commands.entity(entity).insert((
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(material.clone()),
            Transform::from_translation(solid.0.center().extend(0.0)),
        ));
    }
}

fn draw_hero(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    heroes: Query<(Entity, &Position), Added<Hero>>,
) {
    for (entity, pos) in &heroes {
        // An off-centre mark makes the spin visible.
        let mark = commands
            .spawn((
                Mesh2d(meshes.add(circle_mesh(HERO_RADIUS * 0.3))),
                MeshMaterial2d(materials.add(HERO_MARK_COLOUR)),
                Transform::from_xyz(0.0, HERO_RADIUS * 0.5, 0.1),
            ))
            .id();
        commands
            .entity(entity)
            .insert((
                Mesh2d(meshes.add(circle_mesh(HERO_RADIUS))),
                MeshMaterial2d(materials.add(HERO_COLOUR)),
                Transform::from_translation(pos.0.extend(1.0)),
            ))
            .add_child(mark);
    }
}

/// Segments per circle: enough to look round at 4K, where the hero is about
/// 100 pixels across.
const CIRCLE_SEGMENTS: u32 = 64;

fn circle_mesh(radius: f32) -> Mesh {
    Circle::new(radius)
        .mesh()
        .resolution(CIRCLE_SEGMENTS)
        .build()
}

/// F11 switches between a window and borderless fullscreen at the monitor's
/// native resolution.
fn toggle_fullscreen(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::F11) {
        return;
    }
    for mut window in &mut windows {
        window.mode = toggled_mode(window.mode);
    }
}

/// The window mode F11 switches to from `mode`.
#[must_use]
pub const fn toggled_mode(mode: WindowMode) -> WindowMode {
    match mode {
        WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        _ => WindowMode::Windowed,
    }
}

/// Place each moving entity between its last two fixed-step positions,
/// according to how far the current frame is into the next fixed step.
/// Without this, motion judders whenever the frame rate and the 64 Hz
/// simulation rate don't line up.
fn sync_positions(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&Position, Option<&PreviousPosition>, &mut Transform)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (pos, prev, mut transform) in &mut query {
        let drawn = interpolate(prev.map_or(pos.0, |p| p.0), pos.0, alpha);
        transform.translation.x = drawn.x;
        transform.translation.y = drawn.y;
    }
}

/// Blend from `previous` to `current` by `alpha` (clamped to 0..=1).
#[must_use]
pub fn interpolate(previous: Vec2, current: Vec2, alpha: f32) -> Vec2 {
    previous.lerp(current, alpha.clamp(0.0, 1.0))
}

/// Rotate the hero as if rolling at its spin speed (clockwise when moving
/// right).
fn spin_hero_visual(
    time: Res<Time>,
    tuning: Res<BounceTuning>,
    mut heroes: Query<(&Spin, &mut Transform), With<Hero>>,
) {
    for (spin, mut transform) in &mut heroes {
        let angular_speed = -tuning.horizontal_speed(spin.0) / HERO_RADIUS;
        transform.rotate_z(angular_speed * time.delta_secs());
    }
}

/// Camera x eases towards the hero, kept inside the room; y shows the full
/// room height.
fn follow_hero(
    time: Res<Time>,
    bounds: Option<Res<RoomBounds>>,
    heroes: Query<&Transform, With<Hero>>,
    mut cameras: Query<(&mut Transform, &Projection), (With<MainCamera>, Without<Hero>)>,
) {
    // Follow the smoothed (drawn) position, not the raw fixed-step one.
    let (Some(bounds), Ok(hero)) = (bounds, heroes.single()) else {
        return;
    };
    let hero_x = hero.translation.x;
    for (mut transform, projection) in &mut cameras {
        let half_width = match projection {
            Projection::Orthographic(ortho) => ortho.area.width() / 2.0,
            _ => 0.0,
        };
        let target_x = camera_target_x(hero_x, half_width, bounds.0.min.x, bounds.0.max.x);
        let blend = 1.0 - (-CAMERA_FOLLOW_RATE * time.delta_secs()).exp();
        transform.translation.x += (target_x - transform.translation.x) * blend;
        transform.translation.y = bounds.0.center().y;
    }
}

/// Camera x that centres on `hero_x` without showing beyond the room edges.
/// If the view is wider than the room, the room is centred.
#[must_use]
pub fn camera_target_x(hero_x: f32, half_width: f32, min_x: f32, max_x: f32) -> f32 {
    let (lo, hi) = (min_x + half_width, max_x - half_width);
    if lo > hi {
        f32::midpoint(min_x, max_x)
    } else {
        hero_x.clamp(lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::hero::HERO_SPAWN;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn camera_target_clamps_to_room() {
        assert!((camera_target_x(500.0, 960.0, 0.0, 4800.0) - 960.0).abs() < f32::EPSILON);
        assert!((camera_target_x(2000.0, 960.0, 0.0, 4800.0) - 2000.0).abs() < f32::EPSILON);
        assert!((camera_target_x(4700.0, 960.0, 0.0, 4800.0) - 3840.0).abs() < f32::EPSILON);
        assert!((camera_target_x(10.0, 960.0, 0.0, 1000.0) - 500.0).abs() < f32::EPSILON);
    }

    fn presentation_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins((SimulationPlugin, PresentationPlugin));
        app
    }

    #[test]
    fn world_gets_visuals_and_transforms_follow_simulation() {
        let mut app = presentation_app();
        for _ in 0..30 {
            app.update();
        }
        let world = app.world_mut();

        let undrawn = world
            .query_filtered::<&Solid, Without<Mesh2d>>()
            .iter(world)
            .count();
        assert_eq!(undrawn, 0);

        let (pos, prev, transform) = world
            .query_filtered::<(&Position, &PreviousPosition, &Transform), With<Hero>>()
            .single(world)
            .unwrap();
        assert_ne!(pos.0, HERO_SPAWN, "hero should have moved");
        // Drawn position lies on the segment between the last two steps.
        let drawn = transform.translation.truncate();
        let segment = pos.0 - prev.0;
        let along = (drawn - prev.0).dot(segment) / segment.length_squared().max(f32::EPSILON);
        assert!(
            (-1e-3..=1.0 + 1e-3).contains(&along),
            "drawn {drawn} outside step"
        );
        assert!((drawn - interpolate(prev.0, pos.0, along)).length() < 1e-2);
    }

    #[test]
    fn toggled_mode_switches_between_window_and_fullscreen() {
        let fullscreen = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
        assert_eq!(toggled_mode(WindowMode::Windowed), fullscreen);
        assert_eq!(toggled_mode(fullscreen), WindowMode::Windowed);
    }

    #[test]
    fn f11_toggles_the_primary_window() {
        let mut app = presentation_app();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F11);
        app.update();

        let mode = app.world().get::<Window>(window).unwrap().mode;
        assert_eq!(
            mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
    }

    #[test]
    fn interpolate_blends_and_clamps() {
        let (a, b) = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
        assert_eq!(interpolate(a, b, 0.0), a);
        assert_eq!(interpolate(a, b, 0.5), Vec2::new(5.0, 10.0));
        assert_eq!(interpolate(a, b, 1.0), b);
        assert_eq!(interpolate(a, b, 2.0), b);
        assert_eq!(interpolate(a, b, -1.0), a);
    }

    #[test]
    fn spinning_hero_rotates_and_camera_stays_in_room() {
        let mut app = presentation_app();
        app.update();
        app.world_mut()
            .query_filtered::<&mut Spin, With<Hero>>()
            .single_mut(app.world_mut())
            .unwrap()
            .0 = 5;
        for _ in 0..60 {
            app.update();
        }
        let world = app.world_mut();

        let hero_rotation = world
            .query_filtered::<&Transform, With<Hero>>()
            .single(world)
            .unwrap()
            .rotation;
        assert_ne!(hero_rotation, Quat::IDENTITY);

        let camera = world
            .query_filtered::<&Transform, With<MainCamera>>()
            .single(world)
            .unwrap();
        let bounds = world.resource::<RoomBounds>().0;
        assert!(camera.translation.x >= bounds.min.x && camera.translation.x <= bounds.max.x);
        assert!((camera.translation.y - bounds.center().y).abs() < f32::EPSILON);
    }
}
