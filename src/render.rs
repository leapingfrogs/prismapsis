//! Presentation: draws the simulation and moves the camera. Nothing here feeds
//! back into game logic, so it can later be swapped for a 3D presentation.

use bevy::camera::{Hdr, ScalingMode};
use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::art::figures::{Animation, Sheet, stagger};
use crate::art::sprites::{BURST_FRAMES, BURST_REACH, CELL_SIZE, Figure, POST_REACH, PX_PER_UNIT};
use crate::beam::{
    BeamColour, BeamEnergy, BeamLoadout, BeamTrace, BeamTuning, MAX_REFLECTIONS, RayScene,
    trace_from_hero,
};
use crate::enemy::{
    BURST_LIFETIME, Burst, Enemy, EnemyKind, HIT_FLASH, Lives, Orb, STARTING_LIVES,
};
use crate::events::GameEvent;
use crate::hero::{Facing, HERO_RADIUS, Hero, Invulnerable, Position, PreviousPosition, Spin};
use crate::hud::{self, HudArt};
use crate::level::{Checkpoint, RoomBounds, Solid};
use crate::light::{Colour, Hidden, Primary};
use crate::physics::{BounceTuning, Traits};
use crate::powerup::{Abilities, Pickup, PowerUp, PowerUpTuning, pickup_shown};
use crate::prism::{Prism, PrismFrozen};
use crate::screens::{Screen, in_play};
use std::f32::consts::FRAC_PI_2;

/// Vertical extent of the view in world units, regardless of window size.
pub const DESIGN_HEIGHT: f32 = crate::level::VIEW_HEIGHT;

/// How quickly the camera catches up with the hero (per second).
const CAMERA_FOLLOW_RATE: f32 = 6.0;

const HERO_COLOUR: Color = Color::srgb(0.95, 0.96, 1.0);

#[derive(Debug, Default)]
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BeamView>()
            .init_resource::<BeamPieces>()
            .add_systems(Update, gather_lights.after(draw_beam))
            .init_resource::<BloomOn>()
            .add_systems(
                Startup,
                (
                    spawn_camera,
                    spawn_beam_visuals,
                    spawn_energy_bar,
                    spawn_life_pips,
                    spawn_ability_pips,
                    crate::pause::spawn_overlay,
                ),
            )
            .add_systems(
                Update,
                (
                    apply_bloom,
                    draw_hero,
                    draw_prism,
                    toggle_fullscreen,
                    sync_positions,
                    spin_hero_visual,
                    squash_hero,
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
                    crate::pause::show_overlay,
                ),
            )
            .add_systems(
                Update,
                (
                    draw_pickups,
                    animate_pickups,
                    update_ability_pips,
                    draw_checkpoints,
                    light_checkpoints,
                ),
            )
            .add_plugins((
                crate::spectrum_view::SpectrumViewPlugin,
                crate::art::tiles::TilesPlugin,
                crate::art::figures::FiguresPlugin,
                hud::HudPlugin,
                crate::droplets::DropletsPlugin,
            ));
    }
}

const PRISM_COLOUR: Color = Color::srgba(0.8, 0.92, 1.0, 0.85);
const PRISM_FROZEN_COLOUR: Color = Color::srgb(1.0, 1.0, 1.0);

/// The prism is a glass triangle with its tip along +y; `orient_prism`
/// turns the tip to face away from the hero.
fn draw_prism(
    mut commands: Commands,
    sheet: Res<Sheet>,
    prisms: Query<(Entity, &Position), Added<Prism>>,
) {
    for (entity, pos) in &prisms {
        commands.entity(entity).insert((
            sheet.sprite(Figure::Prism, PRISM_COLOUR),
            Transform::from_translation(pos.0.extend(2.0)),
        ));
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
fn tint_frozen_prism(frozen: Res<PrismFrozen>, mut prisms: Query<&mut Sprite, With<Prism>>) {
    let colour = if frozen.0 {
        PRISM_FROZEN_COLOUR
    } else {
        PRISM_COLOUR
    };
    for mut sprite in &mut prisms {
        if sprite.color != colour {
            sprite.color = colour;
        }
    }
}

/// Marks the main gameplay camera.
#[derive(Component, Debug)]
pub struct MainCamera;

/// Whether bright light blooms: things lit brighter than white spill a
/// soft glow round them.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BloomOn(pub bool);

impl Default for BloomOn {
    fn default() -> Self {
        Self(true)
    }
}

/// Bloom only takes what's brighter than white, so the art keeps its
/// colours and only light glows.
const BLOOM: Bloom = Bloom {
    intensity: 0.3,
    prefilter: BloomPrefilter {
        threshold: 1.0,
        threshold_softness: 0.3,
    },
    composite_mode: BloomCompositeMode::Additive,
    ..Bloom::NATURAL
};

/// `colour` lit `by` times brighter, past white where `by` is over 1, so
/// it blooms.
#[must_use]
pub fn glowing(colour: Color, by: f32) -> Color {
    let linear = colour.to_linear();
    Color::LinearRgba(LinearRgba::new(
        linear.red * by,
        linear.green * by,
        linear.blue * by,
        linear.alpha,
    ))
}

/// How brightly each light glows, past white.
const BEAM_LIGHT: f32 = 1.4;
const BEAM_CORE_LIGHT: f32 = 3.0;
const MOTE_LIGHT: f32 = 1.8;
const ORB_LIGHT: f32 = 2.2;
const BURST_LIGHT: f32 = 2.0;

/// Turn bloom on or off on the camera.
fn apply_bloom(
    mut commands: Commands,
    bloom: Res<BloomOn>,
    cameras: Query<Entity, With<MainCamera>>,
) {
    if !bloom.is_changed() {
        return;
    }
    for camera in &cameras {
        if bloom.0 {
            commands.entity(camera).insert(BLOOM);
        } else {
            commands.entity(camera).remove::<Bloom>();
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Main Camera"),
        MainCamera,
        Camera2d,
        // Light brighter than white needs room above it.
        Hdr,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: DESIGN_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}

/// A checkpoint's lamp before it's reached, and once lit: lit, it burns
/// brighter than white, so it blooms.
const CHECKPOINT_IDLE: Color = Color::srgb(0.5, 0.52, 0.6);
const CHECKPOINT_LIT: Color = Color::srgb(0.85, 0.95, 1.0);
const CHECKPOINT_LIGHT: f32 = 2.4;

/// How far the lamp's figure sits below the top of the checkpoint, so its
/// crystal reaches the top; and how far down its cradle the post stops.
const LAMP_DROP: f32 = 29.0;
const POST_TOP: f32 = 26.0;

/// A checkpoint's lamp, which lights up when it's reached.
#[derive(Component, Debug)]
pub struct CheckpointLamp;

/// Checkpoints are a crystal lamp on a slim post, standing on the floor of
/// their area.
fn draw_checkpoints(
    mut commands: Commands,
    sheet: Res<Sheet>,
    checkpoints: Query<(Entity, &Checkpoint), Added<Checkpoint>>,
) {
    for (entity, checkpoint) in &checkpoints {
        let size = checkpoint.aabb.size();
        let top = size.y / 2.0;
        let post_height = (size.y - POST_TOP).max(0.0);
        let mut post = sheet.sprite(Figure::Post, CHECKPOINT_IDLE);
        // Only the middle of the post, which is the same all the way up.
        let inset = (CELL_SIZE / 2.0 - POST_REACH + 2.0) * PX_PER_UNIT;
        post.rect = post
            .rect
            .map(|r| Rect::new(r.min.x, r.min.y + inset, r.max.x, r.max.y - inset));
        post.custom_size = Some(Vec2::new(CELL_SIZE, post_height));
        commands.entity(entity).insert((
            Transform::from_translation(checkpoint.aabb.center().extend(-0.5)),
            Visibility::default(),
            children![
                (
                    post,
                    Transform::from_xyz(0.0, -top + post_height / 2.0, 0.0)
                ),
                (
                    CheckpointLamp,
                    sheet.sprite(Figure::Lamp, CHECKPOINT_IDLE),
                    Transform::from_xyz(0.0, top - LAMP_DROP, 0.1),
                ),
            ],
        ));
    }
}

fn light_checkpoints(
    checkpoints: Query<(&Checkpoint, &Children), Changed<Checkpoint>>,
    mut lamps: Query<&mut Sprite, With<CheckpointLamp>>,
) {
    for (checkpoint, children) in &checkpoints {
        for &child in children {
            if let Ok(mut lamp) = lamps.get_mut(child) {
                lamp.color = if checkpoint.active {
                    glowing(CHECKPOINT_LIT, CHECKPOINT_LIGHT)
                } else {
                    CHECKPOINT_IDLE
                };
            }
        }
    }
}

/// The hero's drawing, squashed and stretched by its bounces; its surface
/// and shading are children, so neither turns the squash.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HeroBody {
    /// Seconds since the last bounce.
    pub since_bounce: f32,
}

/// The hero's markings, which turn as it spins under its fixed shading.
#[derive(Component, Debug)]
pub struct HeroSurface;

fn draw_hero(
    mut commands: Commands,
    sheet: Res<Sheet>,
    heroes: Query<(Entity, &Position), Added<Hero>>,
) {
    for (entity, pos) in &heroes {
        let surface = commands
            .spawn((HeroSurface, sheet.sprite(Figure::HeroSurface, HERO_COLOUR)))
            .id();
        let shade = commands
            .spawn((
                sheet.sprite(Figure::HeroShade, Color::WHITE),
                Transform::from_xyz(0.0, 0.0, 0.1),
            ))
            .id();
        let body = commands
            .spawn((
                HeroBody {
                    since_bounce: SQUASH_TIME,
                },
                Transform::default(),
                Visibility::default(),
            ))
            .add_children(&[surface, shade])
            .id();
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(pos.0.extend(1.0)),
                // Explicit so `blink_hero` always has it to toggle.
                Visibility::default(),
            ))
            .add_child(body);
    }
}

/// How long a bounce's wobble lasts (seconds).
const SQUASH_TIME: f32 = 0.6;

/// How much the hero squashes on landing, as a fraction of its height.
const SQUASH: f32 = 0.24;

/// The hero's scale `since` seconds after a bounce: flattened on landing,
/// then springing tall and settling back, keeping its area.
#[must_use]
pub fn squash(since: f32) -> Vec2 {
    if since >= SQUASH_TIME {
        return Vec2::ONE;
    }
    let wobble = (since * std::f32::consts::TAU * 3.5).cos();
    let tall = 1.0 - SQUASH * (-since * 8.0).exp() * wobble;
    Vec2::new(1.0 / tall, tall)
}

/// Squash the hero when it bounces, keeping its base on the floor.
fn squash_hero(
    time: Res<Time>,
    mut events: MessageReader<GameEvent>,
    mut bodies: Query<(&mut HeroBody, &mut Transform)>,
) {
    let bounced = events
        .read()
        .any(|event| matches!(event, GameEvent::Bounced { .. }));
    for (mut body, mut transform) in &mut bodies {
        body.since_bounce = if bounced {
            0.0
        } else {
            (body.since_bounce + time.delta_secs()).min(SQUASH_TIME)
        };
        let scale = squash(body.since_bounce);
        transform.scale = scale.extend(1.0);
        transform.translation.y = -(1.0 - scale.y) * HERO_RADIUS;
    }
}

/// Beam thickness (world units): the hero's white beam is thicker than the
/// split colours.
const WHITE_BEAM_WIDTH: f32 = 6.0;
const SPLIT_BEAM_WIDTH: f32 = 4.0;

/// How far a beam's glow spreads, in beam widths across.
const BEAM_GLOW: f32 = 4.0;

/// How wide a beam's hot core is, as a fraction of its glow.
const BEAM_CORE: f32 = 0.4;

/// How far a beam's core is whitened, so it burns brighter than its glow.
const BEAM_CORE_WHITE: f32 = 0.6;

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

/// The drawn beam's pieces this frame, which light the walls.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct BeamPieces(pub Vec<crate::beam::BeamPiece>);

/// How far light from each source reaches (world units), and how strong
/// it is: the hero's soft glow, the beam, the open exit, a lit
/// checkpoint's lamp and a mote.
const HERO_LIGHT: (f32, f32) = (260.0, 0.3);
const BEAM_LIGHT_REACH: f32 = 200.0;
const BEAM_LIGHT_STRENGTH: f32 = 1.3;
const EXIT_LIGHT_REACH: (f32, f32) = (380.0, 0.9);
const LAMP_LIGHT: (f32, f32) = (220.0, 0.6);
const MOTE_LIGHT_REACH: (f32, f32) = (150.0, 0.6);

/// Beam pieces that light the walls at most, so other lights still fit.
const BEAM_LIGHTS: usize = 4;

/// How far from the hero a lamp or mote can be and still light the view.
const LIGHT_RANGE: f32 = 900.0;

/// A colour as the shader takes it: linear, times `strength`.
fn light_colour(colour: Color, strength: f32) -> Vec3 {
    let linear = colour.to_linear();
    Vec3::new(linear.red, linear.green, linear.blue) * strength
}

/// Gather the lights shining on the world this frame, most important
/// first: the beam, the hero, the open exit, the nearest lit checkpoint,
/// then the nearest motes.
#[allow(clippy::type_complexity)]
fn gather_lights(
    pieces: Res<BeamPieces>,
    wash: Res<crate::spectrum_view::Wash>,
    mut world_lights: ResMut<crate::art::tiles::WorldLights>,
    heroes: Query<&Transform, With<Hero>>,
    exits: Query<&crate::light::ExitGate>,
    checkpoints: Query<&Checkpoint>,
    motes: Query<(&Transform, &Enemy, &Colour), Without<Hero>>,
) {
    use crate::art::tiles::WorldLight;
    let point = |at: Vec2, colour: Vec3, radius: f32| WorldLight {
        from: at,
        to: at,
        colour,
        radius,
    };
    let mut lights: Vec<WorldLight> = pieces
        .0
        .iter()
        .take(BEAM_LIGHTS)
        .map(|&(from, to, colour)| WorldLight {
            from,
            to,
            colour: light_colour(beam_colour(colour), BEAM_LIGHT_STRENGTH),
            radius: BEAM_LIGHT_REACH,
        })
        .collect();
    let hero = heroes.single().ok().map(|t| t.translation.truncate());
    if let Some(at) = hero {
        lights.push(point(at, Vec3::splat(HERO_LIGHT.1), HERO_LIGHT.0));
    }
    let open = wash.of(crate::light::Hue::White);
    if open > 0.0 {
        for exit in &exits {
            let (reach, strength) = EXIT_LIGHT_REACH;
            lights.push(point(exit.0.center(), Vec3::splat(strength * open), reach));
        }
    }
    let near = |at: Vec2| hero.map_or(0.0, |h| h.distance(at));
    let lamp = checkpoints
        .iter()
        .filter(|c| c.active)
        .map(|c| Vec2::new(c.aabb.center().x, c.aabb.max.y - 20.0))
        .filter(|&at| near(at) < LIGHT_RANGE)
        .min_by(|a, b| near(*a).total_cmp(&near(*b)));
    if let Some(at) = lamp {
        lights.push(point(
            at,
            Vec3::new(0.55, 0.7, 0.9) * LAMP_LIGHT.1,
            LAMP_LIGHT.0,
        ));
    }
    let mut nearby: Vec<(f32, Vec2, Primary)> = motes
        .iter()
        .filter(|(_, enemy, _)| enemy.kind == EnemyKind::Mote)
        .map(|(t, _, colour)| {
            let at = t.translation.truncate();
            (near(at), at, colour.0)
        })
        .filter(|&(distance, ..)| distance < LIGHT_RANGE)
        .collect();
    nearby.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, at, colour) in nearby {
        let (reach, strength) = MOTE_LIGHT_REACH;
        let hue = crate::spectrum_view::hue_colour(colour.into());
        lights.push(point(at, light_colour(hue, strength), reach));
    }
    lights.truncate(crate::art::tiles::MAX_LIGHTS);
    if world_lights.0 != lights {
        world_lights.0 = lights;
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

/// Most pieces a trace can have.
///
/// Forward and rear beams, each fanned into three by spread, each split
/// into white plus three colours by the prism, each of those reflecting
/// off mirrors up to `MAX_REFLECTIONS` times.
pub const MAX_BEAM_PIECES: usize = 2 * 3 * 4 * (1 + MAX_REFLECTIONS);

/// A beam piece's hot core, drawn over its glow.
#[derive(Component, Debug)]
pub struct BeamCore;

/// Materials for each beam colour's glow and core, in `BeamColour::ALL`
/// order, shared by the pieces.
#[derive(Resource, Debug)]
struct BeamMaterials {
    glow: [Handle<ColorMaterial>; 4],
    core: [Handle<ColorMaterial>; 4],
}

impl BeamMaterials {
    fn index(colour: BeamColour) -> usize {
        BeamColour::ALL
            .iter()
            .position(|&c| c == colour)
            .unwrap_or(0)
    }
}

/// Spawn the beam pieces once, hidden; `draw_beam` places, colours and
/// shows them. Each is a soft glow with a hot core down its middle.
fn spawn_beam_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    art: Option<Res<crate::art::figures::GlowArt>>,
) {
    let texture = art.map(|a| a.beam.clone());
    let mut material = |colour: Color| {
        materials.add(ColorMaterial {
            color: colour,
            texture: texture.clone(),
            ..default()
        })
    };
    let beam_materials = BeamMaterials {
        glow: BeamColour::ALL.map(|c| material(glowing(beam_colour(c), BEAM_LIGHT))),
        core: BeamColour::ALL.map(|c| {
            material(glowing(
                beam_colour(c).mix(&Color::WHITE, BEAM_CORE_WHITE),
                BEAM_CORE_LIGHT,
            ))
        }),
    };
    // A unit square, stretched to each piece's length and width.
    let mesh = meshes.add(Rectangle::new(1.0, 1.0));
    for index in 0..MAX_BEAM_PIECES {
        commands.spawn((
            Name::new(format!("Beam piece {index}")),
            BeamVisual(index),
            Mesh2d(mesh.clone()),
            MeshMaterial2d(beam_materials.glow[0].clone()),
            Transform::default(),
            Visibility::Hidden,
            children![(
                BeamCore,
                Mesh2d(mesh.clone()),
                MeshMaterial2d(beam_materials.core[0].clone()),
                Transform::from_xyz(0.0, 0.0, 0.01).with_scale(Vec3::new(1.0, BEAM_CORE, 1.0)),
            )],
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
    mut pieces: ResMut<BeamPieces>,
    heroes: Query<(&Transform, &Facing), (With<Hero>, Without<BeamVisual>)>,
    prisms: Query<&Transform, (With<Prism>, Without<Hero>, Without<BeamVisual>)>,
    solids: Query<(&Solid, Option<&Traits>)>,
    enemies: Query<(&Position, &Enemy)>,
    mut visuals: Query<
        (
            &BeamVisual,
            &Children,
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial2d<ColorMaterial>,
        ),
        (Without<Hero>, Without<Prism>),
    >,
    mut cores: Query<&mut MeshMaterial2d<ColorMaterial>, (With<BeamCore>, Without<BeamVisual>)>,
) {
    let Some(beam_materials) = beam_materials else {
        return;
    };
    let trace = match heroes.single() {
        Ok((hero, facing)) if energy.firing => {
            let scene = RayScene::new(
                solids,
                enemies.iter().map(|(p, e)| (p.0, e.radius())).collect(),
            );
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
    if pieces.0 != trace.segments {
        pieces.0.clone_from(&trace.segments);
    }
    for (visual, children, mut transform, mut visibility, mut material) in &mut visuals {
        let Some(&(start, end, colour)) = trace.segments.get(visual.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let index = BeamMaterials::index(colour);
        if material.0 != beam_materials.glow[index] {
            material.0 = beam_materials.glow[index].clone();
            for &child in children {
                if let Ok(mut core) = cores.get_mut(child) {
                    core.0 = beam_materials.core[index].clone();
                }
            }
        }
        let width = if colour == BeamColour::White {
            WHITE_BEAM_WIDTH
        } else {
            SPLIT_BEAM_WIDTH
        };
        let span = end - start;
        *transform = Transform::from_translation(start.midpoint(end).extend(3.0))
            .with_rotation(Quat::from_rotation_z(span.to_angle()))
            .with_scale(Vec3::new(span.length(), width * BEAM_GLOW, 1.0));
        visibility.set_if_neq(Visibility::Visible);
    }
}

const ENERGY_COLOUR: Color = Color::srgb(0.4, 0.85, 1.0);
const ENERGY_LOW_COLOUR: Color = Color::srgb(0.9, 0.3, 0.3);

/// The filled part of the energy bar.
#[derive(Component, Debug)]
pub struct EnergyBarFill;

/// The energy bar: a glass tube in the top-left corner, under the lives,
/// sized relative to the window so it looks the same at any resolution.
fn spawn_energy_bar(mut commands: Commands, art: Res<HudArt>) {
    commands.spawn((
        Name::new("Energy bar"),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Vw(1.5),
            top: Val::Vh(HUD_ENERGY_TOP),
            ..default()
        },
        children![hud::tube(
            &art,
            Val::Vw(16.0),
            2.6,
            ENERGY_COLOUR,
            EnergyBarFill
        )],
    ));
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
    mut fills: Query<(&mut Node, &mut ImageNode), With<EnergyBarFill>>,
) {
    if !energy.is_changed() && !tuning.is_changed() {
        return;
    }
    let too_low = !energy.firing && energy.current < tuning.min_energy_to_fire;
    for (mut node, mut image) in &mut fills {
        node.width = Val::Percent(energy_fraction(energy.current, tuning.max_energy) * 100.0);
        image.color = if too_low {
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

/// How each enemy kind's figure animates: motes twinkle all the time, and a
/// glint crosses each lens now and then.
#[must_use]
pub const fn enemy_animation(kind: EnemyKind) -> (Figure, f32, f32) {
    match kind {
        EnemyKind::Mote => (Figure::Mote, 12.0, 0.0),
        EnemyKind::Lens => (Figure::Lens, 14.0, 3.0),
    }
}

/// Motes are twinkling sparks; lenses are hexagonal crystals with an eye.
/// An enemy's colour in play: motes are sparks of light, so they glow.
fn shown_enemy(kind: EnemyKind, colour: Option<Primary>) -> Color {
    let base = coloured_enemy(kind, colour);
    match kind {
        EnemyKind::Mote => glowing(base, MOTE_LIGHT),
        EnemyKind::Lens => base,
    }
}

fn draw_enemies(
    mut commands: Commands,
    sheet: Res<Sheet>,
    enemies: Query<(Entity, &Position, &Enemy, Option<&Colour>), Added<Enemy>>,
) {
    for (entity, pos, enemy, colour) in &enemies {
        let (figure, rate, every) = enemy_animation(enemy.kind);
        commands.entity(entity).insert((
            sheet.sprite(figure, shown_enemy(enemy.kind, colour.map(|c| c.0))),
            Animation {
                figure,
                rate,
                every,
                offset: stagger(entity, every.max(1.0)),
            },
            Transform::from_translation(pos.0.extend(0.5)),
        ));
    }
}

/// Flash enemies towards white while their hit flash lasts.
fn tint_enemies(mut enemies: Query<(&Enemy, Option<&Colour>, &mut Sprite), Changed<Enemy>>) {
    for (enemy, colour, mut sprite) in &mut enemies {
        let flash = (enemy.flash / HIT_FLASH).clamp(0.0, 1.0);
        let base = shown_enemy(enemy.kind, colour.map(|c| c.0));
        sprite.color = base.mix(&ENEMY_HIT_COLOUR, flash);
    }
}

fn draw_orbs(
    mut commands: Commands,
    sheet: Res<Sheet>,
    orbs: Query<(Entity, &Position), Added<Orb>>,
) {
    for (entity, pos) in &orbs {
        commands.entity(entity).insert((
            sheet.sprite(Figure::Orb, glowing(ORB_COLOUR, ORB_LIGHT)),
            Animation {
                figure: Figure::Orb,
                rate: 12.0,
                every: 0.0,
                offset: stagger(entity, 1.0),
            },
            Transform::from_translation(pos.0.extend(2.5)),
        ));
    }
}

/// A burst's colour: the light the enemy released, or a warm white.
fn burst_colour(colour: Option<Primary>) -> Color {
    let base = colour.map_or(BURST_COLOUR, |c| crate::spectrum_view::hue_colour(c.into()));
    glowing(base, BURST_LIGHT)
}

/// Bursts are a spray of sparks from the sprite sheet, in the colour the
/// enemy released.
fn draw_bursts(
    mut commands: Commands,
    sheet: Res<Sheet>,
    bursts: Query<(Entity, &Position, &Burst), Added<Burst>>,
) {
    for (entity, pos, burst) in &bursts {
        let mut sprite = sheet.sprite(Figure::Burst, burst_colour(burst.colour));
        sprite.custom_size = Some(Vec2::splat(CELL_SIZE * burst.radius / BURST_REACH));
        commands
            .entity(entity)
            .insert((sprite, Transform::from_translation(pos.0.extend(4.0))));
    }
}

/// Step each burst through its frames as it ages, fading it out.
fn animate_bursts(mut bursts: Query<(&Burst, &mut Sprite)>) {
    for (burst, mut sprite) in &mut bursts {
        let progress = (burst.age / BURST_LIFETIME).clamp(0.0, 1.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let frame = ((progress * BURST_FRAMES as f32) as u32).min(BURST_FRAMES - 1);
        sprite.rect = Some(Figure::Burst.rect(frame));
        sprite.color = burst_colour(burst.colour).with_alpha(1.0 - progress * progress);
    }
}

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

/// Pickups are a glass bubble in the power-up's colour round a white glyph
/// naming it.
fn draw_pickups(
    mut commands: Commands,
    sheet: Res<Sheet>,
    pickups: Query<(Entity, &Position, &Pickup), Added<Pickup>>,
) {
    for (entity, pos, pickup) in &pickups {
        let glyph = commands
            .spawn((
                sheet.sprite(Figure::Glyph(pickup.power), Color::WHITE),
                Transform::from_xyz(0.0, 0.0, 0.1),
            ))
            .id();
        commands
            .entity(entity)
            .insert((
                sheet.sprite(Figure::Bubble, power_up_colour(pickup.power)),
                Transform::from_translation(pos.0.extend(0.8)),
                Visibility::default(),
            ))
            .add_child(glyph);
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

/// The power-ups held, as small pickup bubbles with their glyphs, under
/// the spectrum gauges.
fn spawn_ability_pips(mut commands: Commands, sheet: Res<Sheet>) {
    commands
        .spawn((
            Name::new("Abilities"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Vw(1.5),
                top: Val::Vh(HUD_ABILITIES_TOP),
                column_gap: Val::Vh(0.6),
                ..default()
            },
        ))
        .with_children(|row| {
            for power in PowerUp::ALL {
                row.spawn((
                    AbilityPip(power),
                    hud::icon(&sheet, Figure::Bubble, power_up_colour(power), 3.6),
                    children![hud::overlay(&sheet, Figure::Glyph(power), Color::WHITE)],
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

const LIFE_COLOUR: Color = HERO_COLOUR;

/// Where each row of the HUD sits, from the top of the window (vh): lives,
/// then energy, the spectrum gauges and the power-ups held.
const HUD_LIVES_TOP: f32 = 2.0;
const HUD_ENERGY_TOP: f32 = 5.6;
pub const HUD_GAUGES_TOP: f32 = 9.0;
const HUD_ABILITIES_TOP: f32 = 12.2;

/// One life, drawn as a small hero; index 0 is the leftmost.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifePip(pub u8);

fn spawn_life_pips(mut commands: Commands, sheet: Res<Sheet>) {
    commands
        .spawn((
            Name::new("Lives"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Vw(1.5),
                top: Val::Vh(HUD_LIVES_TOP),
                column_gap: Val::Vh(0.5),
                ..default()
            },
        ))
        .with_children(|row| {
            for index in 0..STARTING_LIVES {
                row.spawn((
                    LifePip(index),
                    hud::icon(&sheet, Figure::HeroSurface, LIFE_COLOUR, 3.0),
                    children![hud::overlay(&sheet, Figure::HeroShade, Color::WHITE)],
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

/// Turn the hero's surface as if rolling at its spin speed (clockwise when
/// moving right).
fn spin_hero_visual(
    time: Res<Time>,
    tuning: Res<BounceTuning>,
    heroes: Query<&Spin, With<Hero>>,
    mut surfaces: Query<&mut Transform, With<HeroSurface>>,
) {
    let Ok(spin) = heroes.single() else {
        return;
    };
    let angular_speed = -tuning.horizontal_speed(spin.0) / HERO_RADIUS;
    for mut transform in &mut surfaces {
        transform.rotate_z(angular_speed * time.delta_secs());
    }
}

/// The camera eases towards the hero on both axes, kept inside the room.
/// The camera eases after the hero in play, and goes straight to it behind
/// a menu, where the stopped clock would leave it wherever it was.
fn follow_hero(
    time: Res<Time>,
    screen: Option<Res<State<Screen>>>,
    bounds: Option<Res<RoomBounds>>,
    heroes: Query<&Transform, With<Hero>>,
    mut cameras: Query<(&mut Transform, &Projection), (With<MainCamera>, Without<Hero>)>,
) {
    // Follow the smoothed (drawn) position, not the raw fixed-step one.
    let (Some(bounds), Ok(hero)) = (bounds, heroes.single()) else {
        return;
    };
    let hero = hero.translation.truncate();
    let blend = if in_play(screen) {
        1.0 - (-CAMERA_FOLLOW_RATE * time.delta_secs()).exp()
    } else {
        1.0
    };
    for (mut transform, projection) in &mut cameras {
        let half = match projection {
            Projection::Orthographic(ortho) => ortho.area.size() / 2.0,
            _ => Vec2::ZERO,
        };
        let target = Vec2::new(
            camera_target(hero.x, half.x, bounds.0.min.x, bounds.0.max.x),
            camera_target(hero.y, half.y, bounds.0.min.y, bounds.0.max.y),
        );
        let current = transform.translation.truncate();
        let next = current + (target - current) * blend;
        transform.translation.x = next.x;
        transform.translation.y = next.y;
    }
}

/// Camera position along one axis that centres on `hero` without showing
/// beyond the room's edges `min` and `max`. If the view is bigger than the
/// room on that axis, the room is centred.
#[must_use]
pub fn camera_target(hero: f32, half_view: f32, min: f32, max: f32) -> f32 {
    let (lo, hi) = (min + half_view, max - half_view);
    if lo > hi {
        f32::midpoint(min, max)
    } else {
        hero.clamp(lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::level::test_room;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn camera_target_clamps_to_room() {
        assert!((camera_target(500.0, 960.0, 0.0, 4800.0) - 960.0).abs() < f32::EPSILON);
        assert!((camera_target(2000.0, 960.0, 0.0, 4800.0) - 2000.0).abs() < f32::EPSILON);
        assert!((camera_target(4700.0, 960.0, 0.0, 4800.0) - 3840.0).abs() < f32::EPSILON);
        assert!((camera_target(10.0, 960.0, 0.0, 1000.0) - 500.0).abs() < f32::EPSILON);
        // Vertically, a one-screen room stays centred; a tall one follows.
        assert!((camera_target(900.0, 540.0, 0.0, 1080.0) - 540.0).abs() < f32::EPSILON);
        assert!((camera_target(1500.0, 540.0, 0.0, 3000.0) - 1500.0).abs() < f32::EPSILON);
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

        // Walls are drawn together as one tiled mesh, and scenery as another.
        let tiled = world
            .query_filtered::<(), (With<crate::art::tiles::WallMesh>, With<Mesh2d>)>()
            .iter(world)
            .count();
        assert_eq!(tiled, 1);
        let panels = world
            .query_filtered::<(), (With<crate::art::tiles::PanelMesh>, With<Mesh2d>)>()
            .iter(world)
            .count();
        assert_eq!(panels, 1, "scenery is drawn as panels");
        assert!(world.query::<&Solid>().iter(world).count() > 1);

        let (pos, prev, transform) = world
            .query_filtered::<(&Position, &PreviousPosition, &Transform), With<Hero>>()
            .single(world)
            .unwrap();
        assert_ne!(pos.0, test_room().start, "hero should have moved");
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
        let white = app.world().resource::<BeamMaterials>().glow[0].clone();
        let world = app.world_mut();
        let whites = world
            .query::<(&Visibility, &MeshMaterial2d<ColorMaterial>)>()
            .iter(world)
            .filter(|(v, m)| **v == Visibility::Visible && m.0 == white)
            .count();
        assert_eq!(whites, 6);
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
        let bubble = app.world().get::<Sprite>(pickup).unwrap();
        assert_eq!(bubble.rect, Some(Figure::Bubble.rect(0)));
        assert_eq!(bubble.color, power_up_colour(PowerUp::Spread));
        let glyph = app.world().get::<Children>(pickup).unwrap()[0];
        assert_eq!(
            app.world().get::<Sprite>(glyph).unwrap().rect,
            Some(Figure::Glyph(PowerUp::Spread).rect(0))
        );

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
            .query_filtered::<(), (With<Enemy>, Without<Sprite>)>()
            .iter(world)
            .count();
        assert_eq!(undrawn_enemies, 0);
        let drawn_orbs = world
            .query_filtered::<(), (With<Orb>, With<Sprite>, With<Animation>)>()
            .iter(world)
            .count();
        assert_eq!(drawn_orbs, 1);
        // The zero-health mote was defeated, leaving a burst of sparks.
        let burst = world
            .query_filtered::<&Sprite, With<Burst>>()
            .single(world)
            .unwrap();
        assert!(burst.custom_size.is_some_and(|size| size.x > 0.0));
        assert!(burst.rect.is_some());
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
    fn a_bounce_squashes_then_settles_keeping_area() {
        let landed = squash(0.0);
        assert!(landed.y < 0.8 && landed.x > 1.2, "{landed}");
        let springing = squash(0.15);
        assert!(springing.y > 1.0, "{springing}");
        for t in [0.0, 0.1, 0.3, 0.5] {
            assert!((squash(t).x * squash(t).y - 1.0).abs() < 1e-5);
        }
        assert_eq!(squash(SQUASH_TIME), Vec2::ONE);
    }

    #[test]
    fn the_hero_squashes_when_it_bounces() {
        let mut app = presentation_app();
        let mut squashed = false;
        for _ in 0..120 {
            app.update();
            let world = app.world_mut();
            let (body, transform) = world
                .query::<(&HeroBody, &Transform)>()
                .single(world)
                .unwrap();
            if body.since_bounce < 0.05 {
                squashed |= transform.scale.y < 0.9;
                // Its base stays on the floor.
                let base = transform.translation.y - HERO_RADIUS * transform.scale.y;
                assert!((base + HERO_RADIUS).abs() < 1e-3);
            }
        }
        assert!(squashed, "the hero should have bounced in two seconds");
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

        // The surface turns; the hero itself, and so its shading, doesn't.
        let surface_rotation = world
            .query_filtered::<&Transform, With<HeroSurface>>()
            .single(world)
            .unwrap()
            .rotation;
        assert_ne!(surface_rotation, Quat::IDENTITY);
        let hero_rotation = world
            .query_filtered::<&Transform, With<Hero>>()
            .single(world)
            .unwrap()
            .rotation;
        assert_eq!(hero_rotation, Quat::IDENTITY);

        let camera = world
            .query_filtered::<&Transform, With<MainCamera>>()
            .single(world)
            .unwrap();
        let bounds = world.resource::<RoomBounds>().0;
        assert!(camera.translation.x >= bounds.min.x && camera.translation.x <= bounds.max.x);
        assert!(camera.translation.y >= bounds.min.y && camera.translation.y <= bounds.max.y);
    }

    #[test]
    fn the_hero_lights_the_world_and_the_beam_adds_its_pieces() {
        let mut app = presentation_app();
        app.update();
        app.update();
        let lights = |app: &App| {
            app.world()
                .resource::<crate::art::tiles::WorldLights>()
                .0
                .clone()
        };
        let idle = lights(&app);
        assert_eq!(idle.len(), 1, "the hero's glow");
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        for _ in 0..5 {
            app.update();
        }
        let firing = lights(&app);
        assert!(firing.len() > idle.len(), "{firing:?}");
        assert!(
            firing[0].from != firing[0].to,
            "a beam lights along its length"
        );
    }
}
