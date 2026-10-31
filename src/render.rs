//! Presentation: draws the simulation and moves the camera. Nothing here feeds
//! back into game logic, so it can later be swapped for a 3D presentation.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::beam::{
    BeamColour, BeamEnergy, BeamLoadout, BeamTrace, BeamTuning, RayScene, trace_from_hero,
};
use crate::enemy::{
    BURST_LIFETIME, Burst, Enemy, EnemyKind, HIT_FLASH, Lives, ORB_RADIUS, Orb, STARTING_LIVES,
};
use crate::hero::{Facing, HERO_RADIUS, Hero, Invulnerable, Position, PreviousPosition, Spin};
use crate::level::{RoomBounds, Solid};
use crate::light::{Colour, Hidden, Phantom, Primary};
use crate::physics::BounceTuning;
use crate::powerup::{Abilities, PICKUP_RADIUS, Pickup, PowerUp, PowerUpTuning, pickup_shown};
use crate::prism::{PRISM_RADIUS, Prism, PrismFrozen};
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
        app.init_resource::<BeamView>()
            .add_systems(
                Startup,
                (
                    spawn_camera,
                    spawn_beam_visuals,
                    spawn_energy_bar,
                    spawn_life_pips,
                    spawn_ability_pips,
                ),
            )
            .add_systems(
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
                    draw_beam.after(sync_positions),
                    update_energy_bar,
                    draw_enemies,
                    tint_enemies,
                    draw_orbs,
                    draw_bursts,
                    animate_bursts,
                    blink_hero,
                    update_life_pips,
                ),
            )
            .add_systems(Update, (draw_pickups, animate_pickups, update_ability_pips))
            .add_plugins(crate::spectrum_view::SpectrumViewPlugin);
    }
}

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
            Vec2::new(0.0, PRISM_RADIUS),
            Vec2::from_angle(210f32.to_radians()) * PRISM_RADIUS,
            Vec2::from_angle(330f32.to_radians()) * PRISM_RADIUS,
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
    // Phantoms draw themselves, even once solid.
    solids: Query<(Entity, &Solid), (Added<Solid>, Without<Phantom>)>,
) {
    for (entity, solid) in &solids {
        let size = solid.0.size();
        // Each has its own material, so colour can wash back per solid.
        commands.entity(entity).insert((
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(materials.add(SOLID_COLOUR)),
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
                // Explicit so `blink_hero` always has it to toggle.
                Visibility::default(),
            ))
            .add_child(mark);
    }
}

/// Beam thickness (world units): the hero's white beam is thicker than the
/// split colours.
const WHITE_BEAM_WIDTH: f32 = 6.0;
const SPLIT_BEAM_WIDTH: f32 = 4.0;

/// Display colour of a beam.
#[must_use]
pub const fn beam_colour(colour: BeamColour) -> Color {
    match colour {
        BeamColour::White => Color::srgb(1.0, 1.0, 1.0),
        BeamColour::Red => Color::srgb(1.0, 0.25, 0.25),
        BeamColour::Green => Color::srgb(0.3, 1.0, 0.35),
        BeamColour::Blue => Color::srgb(0.3, 0.5, 1.0),
    }
}

/// What the drawn beam looks like this frame, for other presentation
/// (such as sound) to follow.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BeamView {
    /// Some beam crosses the prism and splits into colours.
    pub split: bool,
}

/// One of the reusable beam pieces, by index into the current trace.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeamVisual(pub usize);

/// Most pieces a trace can have: forward and rear beams, each fanned into
/// three by spread, each split into white plus three colours by the prism.
pub const MAX_BEAM_PIECES: usize = 2 * 3 * 4;

/// One material per beam colour, shared by the pieces.
#[derive(Resource, Debug)]
struct BeamMaterials {
    white: Handle<ColorMaterial>,
    red: Handle<ColorMaterial>,
    green: Handle<ColorMaterial>,
    blue: Handle<ColorMaterial>,
}

impl BeamMaterials {
    const fn get(&self, colour: BeamColour) -> &Handle<ColorMaterial> {
        match colour {
            BeamColour::White => &self.white,
            BeamColour::Red => &self.red,
            BeamColour::Green => &self.green,
            BeamColour::Blue => &self.blue,
        }
    }
}

/// Spawn the beam pieces once, hidden; `draw_beam` places, colours and
/// shows them.
fn spawn_beam_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let beam_materials = BeamMaterials {
        white: materials.add(beam_colour(BeamColour::White)),
        red: materials.add(beam_colour(BeamColour::Red)),
        green: materials.add(beam_colour(BeamColour::Green)),
        blue: materials.add(beam_colour(BeamColour::Blue)),
    };
    // A unit square, stretched to each piece's length and width.
    let mesh = meshes.add(Rectangle::new(1.0, 1.0));
    for index in 0..MAX_BEAM_PIECES {
        commands.spawn((
            Name::new(format!("Beam piece {index}")),
            BeamVisual(index),
            Mesh2d(mesh.clone()),
            MeshMaterial2d(beam_materials.white.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(beam_materials);
}

/// While firing, trace the beam from the hero's and prism's drawn
/// (interpolated) positions, so it stays attached to them on screen.
/// Damage uses the simulation's own trace in `beam`.
fn draw_beam(
    energy: Res<BeamEnergy>,
    tuning: Res<BeamTuning>,
    abilities: Res<Abilities>,
    powerups: Res<PowerUpTuning>,
    beam_materials: Option<Res<BeamMaterials>>,
    mut view: ResMut<BeamView>,
    heroes: Query<(&Transform, &Facing), (With<Hero>, Without<BeamVisual>)>,
    prisms: Query<&Transform, (With<Prism>, Without<Hero>, Without<BeamVisual>)>,
    solids: Query<&Solid>,
    enemies: Query<(&Position, &Enemy)>,
    mut visuals: Query<
        (
            &BeamVisual,
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial2d<ColorMaterial>,
        ),
        (Without<Hero>, Without<Prism>),
    >,
) {
    let Some(beam_materials) = beam_materials else {
        return;
    };
    let trace = match heroes.single() {
        Ok((hero, facing)) if energy.firing => {
            let scene = RayScene {
                solids: solids.iter().map(|s| s.0).collect(),
                targets: enemies.iter().map(|(p, e)| (p.0, e.radius())).collect(),
            };
            let prism = prisms.single().ok().map(|p| p.translation.truncate());
            trace_from_hero(
                hero.translation.truncate(),
                *facing,
                prism,
                &scene,
                tuning.split_angle,
                &BeamLoadout::from_abilities(*abilities, &powerups),
            )
        }
        _ => BeamTrace::default(),
    };
    let split = trace.segments.iter().any(|s| s.2 != BeamColour::White);
    if view.split != split {
        view.split = split;
    }
    for (visual, mut transform, mut visibility, mut material) in &mut visuals {
        let Some(&(start, end, colour)) = trace.segments.get(visual.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let wanted = beam_materials.get(colour);
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
        let width = if colour == BeamColour::White {
            WHITE_BEAM_WIDTH
        } else {
            SPLIT_BEAM_WIDTH
        };
        let span = end - start;
        *transform = Transform::from_translation(start.midpoint(end).extend(3.0))
            .with_rotation(Quat::from_rotation_z(span.to_angle()))
            .with_scale(Vec3::new(span.length(), width, 1.0));
        visibility.set_if_neq(Visibility::Visible);
    }
}

const ENERGY_COLOUR: Color = Color::srgb(0.4, 0.85, 1.0);
const ENERGY_LOW_COLOUR: Color = Color::srgb(0.9, 0.3, 0.3);

/// The filled part of the energy bar.
#[derive(Component, Debug)]
pub struct EnergyBarFill;

/// Energy bar in the top-left corner, sized relative to the window so it
/// looks the same at any resolution.
fn spawn_energy_bar(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Energy bar"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Vw(1.5),
                top: Val::Vh(2.5),
                width: Val::Vw(16.0),
                height: Val::Vh(1.6),
                padding: UiRect::all(Val::Vh(0.3)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|bar| {
            bar.spawn((
                EnergyBarFill,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(ENERGY_COLOUR),
            ));
        });
}

/// Fraction of the bar to fill, 0 to 1.
#[must_use]
pub fn energy_fraction(current: f32, max: f32) -> f32 {
    (current / max.max(f32::EPSILON)).clamp(0.0, 1.0)
}

/// Fill the bar to the current energy; red when there isn't enough to start
/// firing.
fn update_energy_bar(
    energy: Res<BeamEnergy>,
    tuning: Res<BeamTuning>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<EnergyBarFill>>,
) {
    if !energy.is_changed() && !tuning.is_changed() {
        return;
    }
    let too_low = !energy.firing && energy.current < tuning.min_energy_to_fire;
    for (mut node, mut colour) in &mut fills {
        node.width = Val::Percent(energy_fraction(energy.current, tuning.max_energy) * 100.0);
        colour.0 = if too_low {
            ENERGY_LOW_COLOUR
        } else {
            ENERGY_COLOUR
        };
    }
}

const MOTE_COLOUR: Color = Color::srgb(1.0, 0.88, 0.55);
const LENS_COLOUR: Color = Color::srgb(0.45, 0.85, 0.95);
const ENEMY_HIT_COLOUR: Color = Color::srgb(1.0, 1.0, 1.0);
const ORB_COLOUR: Color = Color::srgb(0.8, 0.5, 1.0);
const BURST_COLOUR: Color = Color::srgb(1.0, 0.95, 0.8);

/// Base colour of an enemy type, before any hit flash.
#[must_use]
pub const fn enemy_colour(kind: EnemyKind) -> Color {
    match kind {
        EnemyKind::Mote => MOTE_COLOUR,
        EnemyKind::Lens => LENS_COLOUR,
    }
}

/// An enemy's colour, showing the light it will release: motes glow in it,
/// lens crystals are tinted by it.
#[must_use]
pub fn coloured_enemy(kind: EnemyKind, colour: Option<Primary>) -> Color {
    let Some(colour) = colour else {
        return enemy_colour(kind);
    };
    let light = crate::spectrum_view::hue_colour(colour.into());
    match kind {
        EnemyKind::Mote => light,
        EnemyKind::Lens => LENS_COLOUR.mix(&light, 0.7),
    }
}

/// Motes are small round sparks; lenses are hexagonal crystals. Each gets
/// its own material so it can flash independently.
fn draw_enemies(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    enemies: Query<(Entity, &Position, &Enemy, Option<&Colour>), Added<Enemy>>,
) {
    for (entity, pos, enemy, colour) in &enemies {
        let mesh = match enemy.kind {
            EnemyKind::Mote => meshes.add(circle_mesh(enemy.radius())),
            EnemyKind::Lens => meshes.add(RegularPolygon::new(enemy.radius(), 6)),
        };
        commands.entity(entity).insert((
            Mesh2d(mesh),
            MeshMaterial2d(materials.add(coloured_enemy(enemy.kind, colour.map(|c| c.0)))),
            Transform::from_translation(pos.0.extend(0.5)),
        ));
    }
}

/// Flash enemies towards white while their hit flash lasts.
fn tint_enemies(
    mut materials: ResMut<Assets<ColorMaterial>>,
    enemies: Query<(&Enemy, Option<&Colour>, &MeshMaterial2d<ColorMaterial>), Changed<Enemy>>,
) {
    for (enemy, colour, material) in &enemies {
        if let Some(mut material) = materials.get_mut(&material.0) {
            let flash = (enemy.flash / HIT_FLASH).clamp(0.0, 1.0);
            let base = coloured_enemy(enemy.kind, colour.map(|c| c.0));
            material.color = base.mix(&ENEMY_HIT_COLOUR, flash);
        }
    }
}

fn draw_orbs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    orbs: Query<(Entity, &Position), Added<Orb>>,
) {
    for (entity, pos) in &orbs {
        commands.entity(entity).insert((
            Mesh2d(meshes.add(circle_mesh(ORB_RADIUS))),
            MeshMaterial2d(materials.add(ORB_COLOUR)),
            Transform::from_translation(pos.0.extend(2.5)),
        ));
    }
}

/// Bursts are rings drawn at unit size and scaled up as they age.
fn draw_bursts(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    bursts: Query<(Entity, &Position), Added<Burst>>,
) {
    for (entity, pos) in &bursts {
        commands.entity(entity).insert((
            Mesh2d(meshes.add(Annulus::new(0.8, 1.0))),
            MeshMaterial2d(materials.add(BURST_COLOUR)),
            Transform::from_translation(pos.0.extend(4.0)).with_scale(Vec3::ZERO),
        ));
    }
}

/// Grow each burst ring outward while fading it.
fn animate_bursts(
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut bursts: Query<(&Burst, &mut Transform, &MeshMaterial2d<ColorMaterial>)>,
) {
    for (burst, mut transform, material) in &mut bursts {
        let progress = (burst.age / BURST_LIFETIME).clamp(0.0, 1.0);
        transform.scale = Vec3::new(burst.radius, burst.radius, 1.0) * progress.max(0.05);
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.color = BURST_COLOUR.with_alpha(1.0 - progress);
        }
    }
}

const PICKUP_BACKDROP_COLOUR: Color = Color::srgba(0.05, 0.05, 0.1, 0.85);
const GLYPH_COLOUR: Color = Color::srgb(1.0, 1.0, 1.0);

/// Ring and HUD colour for each power-up.
#[must_use]
pub const fn power_up_colour(power: PowerUp) -> Color {
    match power {
        PowerUp::AntiGravity => Color::srgb(0.72, 0.5, 1.0),
        PowerUp::RearShot => Color::srgb(1.0, 0.6, 0.25),
        PowerUp::Spread => Color::srgb(0.45, 0.95, 0.55),
        PowerUp::RapidFire => Color::srgb(1.0, 0.9, 0.3),
        PowerUp::Piercing => Color::srgb(0.35, 0.75, 1.0),
    }
}

/// A simple shape in a pickup's glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphPart {
    /// A triangle of this size, pointing along +x before rotation.
    Arrowhead(f32),
    /// A bar of this length and width, along +x before rotation.
    Bar(f32, f32),
    /// A ring of this outer radius.
    Ring(f32),
}

/// The shapes that make up each power-up's glyph, with where they sit
/// (offset and rotation in radians) inside the pickup ring.
#[must_use]
pub fn glyph_parts(power: PowerUp) -> Vec<(GlyphPart, Vec2, f32)> {
    use std::f32::consts::PI;
    let s = PICKUP_RADIUS * 0.5;
    match power {
        // An arrow rising above a floor line: hovering.
        PowerUp::AntiGravity => vec![
            (
                GlyphPart::Arrowhead(s * 0.8),
                Vec2::new(0.0, s * 0.35),
                FRAC_PI_2,
            ),
            (GlyphPart::Bar(s * 1.6, 3.0), Vec2::new(0.0, -s * 0.7), 0.0),
        ],
        // A double-headed arrow: firing both ways.
        PowerUp::RearShot => vec![
            (GlyphPart::Bar(s * 1.2, 3.0), Vec2::ZERO, 0.0),
            (GlyphPart::Arrowhead(s * 0.6), Vec2::new(s * 0.7, 0.0), 0.0),
            (GlyphPart::Arrowhead(s * 0.6), Vec2::new(-s * 0.7, 0.0), PI),
        ],
        // Three rays fanning out.
        PowerUp::Spread => [-0.45f32, 0.0, 0.45]
            .into_iter()
            .map(|angle| {
                let offset = Vec2::from_angle(angle) * s * 0.7 - Vec2::new(s * 0.6, 0.0);
                (GlyphPart::Bar(s * 1.4, 3.0), offset, angle)
            })
            .collect(),
        // Fast-forward: two arrowheads in a row.
        PowerUp::RapidFire => vec![
            (
                GlyphPart::Arrowhead(s * 0.7),
                Vec2::new(-s * 0.35, 0.0),
                0.0,
            ),
            (GlyphPart::Arrowhead(s * 0.7), Vec2::new(s * 0.45, 0.0), 0.0),
        ],
        // A beam passing through a ring.
        PowerUp::Piercing => vec![
            (GlyphPart::Ring(s * 0.55), Vec2::ZERO, 0.0),
            (GlyphPart::Bar(s * 1.9, 3.0), Vec2::ZERO, 0.0),
        ],
    }
}

fn glyph_mesh(part: GlyphPart) -> Mesh {
    match part {
        GlyphPart::Arrowhead(size) => Triangle2d::new(
            Vec2::new(size * 0.6, 0.0),
            Vec2::new(-size * 0.4, size * 0.55),
            Vec2::new(-size * 0.4, -size * 0.55),
        )
        .into(),
        GlyphPart::Bar(length, width) => Rectangle::new(length, width).into(),
        GlyphPart::Ring(radius) => Annulus::new(radius - 3.0, radius).into(),
    }
}

/// Pickups are a coloured ring around a white glyph naming the power-up.
fn draw_pickups(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    pickups: Query<(Entity, &Position, &Pickup), Added<Pickup>>,
) {
    if pickups.is_empty() {
        return;
    }
    let glyph_material = materials.add(GLYPH_COLOUR);
    for (entity, pos, pickup) in &pickups {
        let backdrop = commands
            .spawn((
                Mesh2d(meshes.add(circle_mesh(PICKUP_RADIUS * 0.85))),
                MeshMaterial2d(materials.add(PICKUP_BACKDROP_COLOUR)),
                Transform::from_xyz(0.0, 0.0, -0.1),
            ))
            .id();
        commands.entity(entity).insert((
            Mesh2d(meshes.add(Annulus::new(PICKUP_RADIUS * 0.82, PICKUP_RADIUS))),
            MeshMaterial2d(materials.add(power_up_colour(pickup.power))),
            Transform::from_translation(pos.0.extend(0.8)),
            Visibility::default(),
        ));
        commands.entity(entity).add_child(backdrop);
        for (part, offset, angle) in glyph_parts(pickup.power) {
            let glyph = commands
                .spawn((
                    Mesh2d(meshes.add(glyph_mesh(part))),
                    MeshMaterial2d(glyph_material.clone()),
                    Transform::from_translation(offset.extend(0.1))
                        .with_rotation(Quat::from_rotation_z(angle)),
                ))
                .id();
            commands.entity(entity).add_child(glyph);
        }
    }
}

/// Pulse pickups gently, and blink them as they're about to vanish.
fn animate_pickups(
    tuning: Res<PowerUpTuning>,
    mut pickups: Query<(&Pickup, Has<Hidden>, &mut Transform, &mut Visibility)>,
) {
    for (pickup, hidden, mut transform, mut visibility) in &mut pickups {
        let pulse = 1.0 + 0.06 * (pickup.age * 5.0).sin();
        transform.scale = Vec3::new(pulse, pulse, 1.0);
        let wanted = if !hidden && pickup_shown(pickup.age, tuning.drop_lifetime) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
}

/// One HUD marker per power-up, shown while the hero has it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbilityPip(pub PowerUp);

fn spawn_ability_pips(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Abilities"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Vw(1.5),
                top: Val::Vh(8.0),
                column_gap: Val::Vh(0.8),
                ..default()
            },
        ))
        .with_children(|row| {
            for power in PowerUp::ALL {
                row.spawn((
                    AbilityPip(power),
                    Node {
                        width: Val::Vh(2.4),
                        height: Val::Vh(1.0),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(power_up_colour(power)),
                ));
            }
        });
}

fn update_ability_pips(abilities: Res<Abilities>, mut pips: Query<(&AbilityPip, &mut Node)>) {
    if !abilities.is_changed() {
        return;
    }
    for (pip, mut node) in &mut pips {
        node.display = if abilities.has(pip.0) {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Blink rate of the hero while invulnerable (blinks per second).
const BLINK_RATE: f32 = 10.0;

/// Whether the hero is drawn this frame: blinking while invulnerable.
#[must_use]
pub fn hero_shown(invulnerable: f32) -> bool {
    // Truncation is the point: alternate whole blink intervals.
    #[allow(clippy::cast_possible_truncation)]
    let interval = (invulnerable * BLINK_RATE) as i64;
    invulnerable <= 0.0 || interval % 2 == 0
}

fn blink_hero(mut heroes: Query<(&Invulnerable, &mut Visibility), With<Hero>>) {
    for (invulnerable, mut visibility) in &mut heroes {
        let wanted = if hero_shown(invulnerable.0) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
}

const LIFE_COLOUR: Color = Color::srgb(0.92, 0.92, 0.96);

/// One life indicator under the energy bar; index 0 is the leftmost.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifePip(pub u8);

fn spawn_life_pips(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Lives"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Vw(1.5),
                top: Val::Vh(5.0),
                column_gap: Val::Vh(0.8),
                ..default()
            },
        ))
        .with_children(|row| {
            for index in 0..STARTING_LIVES {
                row.spawn((
                    LifePip(index),
                    Node {
                        width: Val::Vh(1.6),
                        height: Val::Vh(1.6),
                        ..default()
                    },
                    BackgroundColor(LIFE_COLOUR),
                ));
            }
        });
}

/// Show one pip per remaining life.
fn update_life_pips(lives: Res<Lives>, mut pips: Query<(&LifePip, &mut Node)>) {
    if !lives.is_changed() {
        return;
    }
    for (pip, mut node) in &mut pips {
        node.display = if pip.0 < lives.0 {
            Display::Flex
        } else {
            Display::None
        };
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
    fn energy_fraction_is_clamped() {
        assert!((energy_fraction(50.0, 100.0) - 0.5).abs() < f32::EPSILON);
        assert!((energy_fraction(150.0, 100.0) - 1.0).abs() < f32::EPSILON);
        assert!(energy_fraction(-5.0, 100.0).abs() < f32::EPSILON);
    }

    fn visible_beams(app: &mut App) -> usize {
        let world = app.world_mut();
        world
            .query::<(&BeamVisual, &Visibility)>()
            .iter(world)
            .filter(|(_, v)| **v == Visibility::Visible)
            .count()
    }

    fn energy_bar_width(app: &mut App) -> Val {
        let world = app.world_mut();
        world
            .query_filtered::<&Node, With<EnergyBarFill>>()
            .single(world)
            .unwrap()
            .width
    }

    #[test]
    fn beam_shows_while_firing_and_energy_bar_drains() {
        let mut app = presentation_app();
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(visible_beams(&mut app), 0);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        for _ in 0..30 {
            app.update();
        }
        assert!(visible_beams(&mut app) >= 1, "beam should be drawn");
        let Val::Percent(filled) = energy_bar_width(&mut app) else {
            panic!("energy bar width should be a percentage");
        };
        assert!(filled < 100.0, "bar should drain while firing");

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Space);
        app.update();
        app.update();
        assert_eq!(visible_beams(&mut app), 0);
    }

    #[test]
    fn weapon_power_ups_draw_more_beams_in_their_colours() {
        let mut app = presentation_app();
        app.update();
        {
            let mut abilities = app.world_mut().resource_mut::<Abilities>();
            abilities.grant(PowerUp::RearShot);
            abilities.grant(PowerUp::Spread);
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        for _ in 0..5 {
            app.update();
        }
        // Six white beams, plus three colours if one crosses the prism.
        let shown = visible_beams(&mut app);
        assert!(shown == 6 || shown == 9, "got {shown} beam pieces");
        let white = app.world().resource::<BeamMaterials>().white.clone();
        let world = app.world_mut();
        let whites = world
            .query::<(&Visibility, &MeshMaterial2d<ColorMaterial>)>()
            .iter(world)
            .filter(|(v, m)| **v == Visibility::Visible && m.0 == white)
            .count();
        assert_eq!(whites, 6);
    }

    #[test]
    fn every_power_up_has_a_distinct_glyph() {
        let glyphs: Vec<_> = PowerUp::ALL.into_iter().map(glyph_parts).collect();
        assert!(glyphs.iter().all(|parts| !parts.is_empty()));
        for (i, a) in glyphs.iter().enumerate() {
            for b in &glyphs[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn pickups_are_drawn_and_collected_abilities_show_in_the_hud() {
        let mut app = presentation_app();
        app.update();
        let pickup = app
            .world_mut()
            .spawn(crate::powerup::pickup_bundle(
                PowerUp::Spread,
                Vec2::new(4000.0, 900.0),
            ))
            .id();
        app.update();
        let glyph_parts_drawn = app.world().get::<Children>(pickup).map_or(0, Children::len);
        assert_eq!(glyph_parts_drawn, 1 + glyph_parts(PowerUp::Spread).len());
        assert!(app.world().get::<Mesh2d>(pickup).is_some());

        let shown_pips = |app: &mut App| {
            let world = app.world_mut();
            world
                .query::<(&AbilityPip, &Node)>()
                .iter(world)
                .filter(|(_, node)| node.display != Display::None)
                .map(|(pip, _)| pip.0)
                .collect::<Vec<_>>()
        };
        assert_eq!(shown_pips(&mut app), Vec::<PowerUp>::new());
        app.world_mut()
            .resource_mut::<Abilities>()
            .grant(PowerUp::Piercing);
        app.update();
        assert_eq!(shown_pips(&mut app), vec![PowerUp::Piercing]);
    }

    #[test]
    fn pickups_blink_before_vanishing() {
        let mut app = presentation_app();
        app.update();
        let lifetime = PowerUpTuning::default().drop_lifetime;
        let pickup = app
            .world_mut()
            .spawn(crate::powerup::pickup_bundle(
                PowerUp::RapidFire,
                Vec2::new(4000.0, 900.0),
            ))
            .id();
        let mut hidden = false;
        // Small positive whole number of frames.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = ((lifetime - 0.1) / 0.016) as usize;
        for _ in 0..frames {
            app.update();
            hidden |= app.world().get::<Visibility>(pickup) == Some(&Visibility::Hidden);
        }
        assert!(hidden, "pickup should blink near the end");
    }

    #[test]
    fn hero_blinks_only_while_invulnerable() {
        assert!(hero_shown(0.0));
        assert!(hero_shown(0.05));
        assert!(!hero_shown(0.15));
        assert!(hero_shown(0.25));
    }

    #[test]
    fn enemies_orbs_and_bursts_are_drawn() {
        let mut app = presentation_app();
        app.update();
        app.world_mut().spawn((
            Enemy {
                kind: EnemyKind::Mote,
                health: 0,
                flash: HIT_FLASH,
            },
            Position(Vec2::new(1500.0, 600.0)),
        ));
        app.world_mut().spawn((
            Orb {
                vel: Vec2::ZERO,
                age: 0.0,
            },
            Position(Vec2::new(1500.0, 700.0)),
            PreviousPosition(Vec2::new(1500.0, 700.0)),
        ));
        app.update();
        app.update();
        let world = app.world_mut();
        let undrawn_enemies = world
            .query_filtered::<(), (With<Enemy>, Without<Mesh2d>)>()
            .iter(world)
            .count();
        assert_eq!(undrawn_enemies, 0);
        let drawn_orbs = world
            .query_filtered::<(), (With<Orb>, With<Mesh2d>)>()
            .iter(world)
            .count();
        assert_eq!(drawn_orbs, 1);
        // The zero-health mote was defeated, leaving a growing burst.
        let burst_scale = world
            .query_filtered::<&Transform, (With<Burst>, With<Mesh2d>)>()
            .single(world)
            .unwrap()
            .scale;
        assert!(burst_scale.x > 0.0);
    }

    #[test]
    fn losing_a_life_hides_a_pip_and_blinks_the_hero() {
        let mut app = presentation_app();
        app.update();
        let hero = {
            let world = app.world_mut();
            world
                .query_filtered::<&Position, With<Hero>>()
                .single(world)
                .unwrap()
                .0
        };
        app.world_mut().spawn((
            Enemy {
                kind: EnemyKind::Mote,
                health: 3,
                flash: 0.0,
            },
            Position(hero),
        ));
        let mut hidden_frames = 0;
        for _ in 0..20 {
            app.update();
            let world = app.world_mut();
            let visibility = *world
                .query_filtered::<&Visibility, With<Hero>>()
                .single(world)
                .unwrap();
            hidden_frames += usize::from(visibility == Visibility::Hidden);
        }
        assert!(hidden_frames > 0, "hero should blink while invulnerable");

        let world = app.world_mut();
        let shown = world
            .query::<(&LifePip, &Node)>()
            .iter(world)
            .filter(|(_, node)| node.display != Display::None)
            .count();
        assert_eq!(shown, usize::from(STARTING_LIVES - 1));
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
