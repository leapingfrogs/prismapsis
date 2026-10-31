//! Presentation for light and colour.
//!
//! Shows the world washing from grey into colour, phantom platforms, the exit gate, the spectrum gauges and the
//! level-complete banner. Nothing here feeds back into game logic.

use bevy::prelude::*;

use crate::level::{Scenery, Solid};
use crate::light::{
    ExitGate, Hue, LevelsCompleted, LightTuning, Phantom, Primary, Shade, Spectrum,
};

/// Display colour of each hue.
#[must_use]
pub const fn hue_colour(hue: Hue) -> Color {
    match hue {
        Hue::Red => Color::srgb(0.92, 0.28, 0.28),
        Hue::Green => Color::srgb(0.3, 0.85, 0.38),
        Hue::Blue => Color::srgb(0.3, 0.48, 0.98),
        Hue::Yellow => Color::srgb(0.96, 0.86, 0.3),
        Hue::Cyan => Color::srgb(0.3, 0.88, 0.92),
        Hue::Magenta => Color::srgb(0.88, 0.36, 0.9),
        Hue::White => Color::srgb(0.95, 0.95, 0.97),
    }
}

/// Greyscale versions of solids and scenery, before colour returns.
const SOLID_GREY: Color = Color::srgb(0.32, 0.34, 0.4);
const SCENERY_GREY: Color = Color::srgb(0.1, 0.1, 0.13);

/// How strongly restored hues show on solids and on scenery (scenery stays
/// dim so it reads as background).
const SOLID_STRENGTH: f32 = 0.6;
const SCENERY_STRENGTH: f32 = 0.28;

/// The background once the whole spectrum is back.
const BACKGROUND_RESTORED: Color = Color::srgb(0.06, 0.06, 0.15);

/// How fast a restored colour floods in (fraction per second).
const WASH_RATE: f32 = 0.6;

/// How far each primary has washed back into the world, from 0 to 1. Eases
/// towards the spectrum, so restoring a colour floods it in over a couple
/// of seconds.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct Wash(pub [f32; 3]);

impl Wash {
    /// How much of `hue` shows: limited by its least-washed primary.
    #[must_use]
    pub fn of(&self, hue: Hue) -> f32 {
        hue.primaries()
            .iter()
            .map(|p| self.0[p.index()])
            .fold(1.0, f32::min)
    }
}

/// Colour of a shaded solid or piece of scenery given how washed its hue is.
#[must_use]
pub fn shaded_colour(hue: Hue, wash: f32, scenery: bool) -> Color {
    let (grey, strength) = if scenery {
        (SCENERY_GREY, SCENERY_STRENGTH)
    } else {
        (SOLID_GREY, SOLID_STRENGTH)
    };
    let full = Color::BLACK.mix(&hue_colour(hue), strength);
    grey.mix(&full, wash.clamp(0.0, 1.0))
}

#[derive(Debug, Default)]
pub struct SpectrumViewPlugin;

impl Plugin for SpectrumViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wash>()
            .add_systems(Startup, (spawn_gauges, spawn_banner))
            .add_systems(
                Update,
                (
                    advance_wash,
                    draw_scenery,
                    draw_phantoms,
                    draw_exit,
                    (wash_world, show_phantoms, animate_exit, tint_background).after(advance_wash),
                    update_gauges,
                    show_banner,
                ),
            );
    }
}

fn advance_wash(time: Res<Time>, spectrum: Res<Spectrum>, mut wash: ResMut<Wash>) {
    let step = WASH_RATE * time.delta_secs();
    let mut next = *wash;
    for primary in Primary::ALL {
        let target = if spectrum.restored[primary.index()] {
            1.0
        } else {
            0.0
        };
        let level = &mut next.0[primary.index()];
        *level += (target - *level).clamp(-step, step);
    }
    if next != *wash {
        *wash = next;
    }
}

fn draw_scenery(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    scenery: Query<(Entity, &Scenery, &Shade), Added<Scenery>>,
) {
    for (entity, scenery, shade) in &scenery {
        let size = scenery.0.size();
        commands.entity(entity).insert((
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(materials.add(shaded_colour(shade.0, 0.0, true))),
            Transform::from_translation(scenery.0.center().extend(-1.0)),
        ));
    }
}

/// Recolour shaded solids and scenery as colour washes back in.
fn wash_world(
    wash: Res<Wash>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    shaded: Query<(&Shade, Has<Scenery>, &MeshMaterial2d<ColorMaterial>)>,
    added: Query<(), Added<MeshMaterial2d<ColorMaterial>>>,
) {
    if !wash.is_changed() && added.is_empty() {
        return;
    }
    for (shade, scenery, material) in &shaded {
        if let Some(mut material) = materials.get_mut(&material.0) {
            let colour = shaded_colour(shade.0, wash.of(shade.0), scenery);
            if material.color != colour {
                material.color = colour;
            }
        }
    }
}

/// Outline thickness of phantom platforms and the exit frame.
const OUTLINE: f32 = 4.0;

/// Four thin rectangles tracing the edge of a `size` box centred on the
/// parent, as (offset, size) pairs.
fn outline_parts(size: Vec2) -> [(Vec2, Vec2); 4] {
    let half = size / 2.0;
    [
        (Vec2::new(0.0, half.y), Vec2::new(size.x, OUTLINE)),
        (Vec2::new(0.0, -half.y), Vec2::new(size.x, OUTLINE)),
        (Vec2::new(-half.x, 0.0), Vec2::new(OUTLINE, size.y)),
        (Vec2::new(half.x, 0.0), Vec2::new(OUTLINE, size.y)),
    ]
}

fn spawn_outline(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    size: Vec2,
    material: &Handle<ColorMaterial>,
) {
    for (offset, part) in outline_parts(size) {
        let child = commands
            .spawn((
                Mesh2d(meshes.add(Rectangle::new(part.x, part.y))),
                MeshMaterial2d(material.clone()),
                Transform::from_translation(offset.extend(0.05)),
            ))
            .id();
        commands.entity(parent).add_child(child);
    }
}

/// A phantom's fill when it is only an outline.
const PHANTOM_FILL_ALPHA: f32 = 0.1;

/// Phantoms are a coloured outline with a faint fill, which turns solid
/// when their hue is restored.
fn draw_phantoms(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    phantoms: Query<(Entity, &Phantom), Added<Phantom>>,
) {
    for (entity, phantom) in &phantoms {
        let size = phantom.aabb.size();
        let colour = hue_colour(phantom.hue);
        commands.entity(entity).insert((
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(materials.add(colour.with_alpha(PHANTOM_FILL_ALPHA))),
            Transform::from_translation(phantom.aabb.center().extend(0.2)),
        ));
        let outline = materials.add(colour.with_alpha(0.7));
        spawn_outline(&mut commands, &mut meshes, entity, size, &outline);
    }
}

fn show_phantoms(
    mut materials: ResMut<Assets<ColorMaterial>>,
    phantoms: Query<(&Phantom, Has<Solid>, &MeshMaterial2d<ColorMaterial>)>,
) {
    for (phantom, solid, material) in &phantoms {
        let alpha = if solid { 1.0 } else { PHANTOM_FILL_ALPHA };
        let colour = hue_colour(phantom.hue).with_alpha(alpha);
        // Only touch the asset on a change, so it isn't re-uploaded each frame.
        if materials
            .get(&material.0)
            .is_some_and(|m| m.color != colour)
            && let Some(mut material) = materials.get_mut(&material.0)
        {
            material.color = colour;
        }
    }
}

const EXIT_CLOSED: Color = Color::srgb(0.25, 0.25, 0.3);

/// Materials for the exit's frame and inner glow.
#[derive(Component, Debug)]
struct ExitMaterials {
    frame: Handle<ColorMaterial>,
    glow: Handle<ColorMaterial>,
}

fn draw_exit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    gates: Query<(Entity, &ExitGate), Added<ExitGate>>,
) {
    for (entity, gate) in &gates {
        let size = gate.0.size();
        let frame = materials.add(EXIT_CLOSED);
        let glow = materials.add(EXIT_CLOSED.with_alpha(0.15));
        commands.entity(entity).insert((
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(glow.clone()),
            Transform::from_translation(gate.0.center().extend(0.3)),
            ExitMaterials {
                frame: frame.clone(),
                glow,
            },
        ));
        spawn_outline(&mut commands, &mut meshes, entity, size, &frame);
    }
}

/// Grey while closed; once white is restored, the frame shines and the
/// doorway pulses.
fn animate_exit(
    time: Res<Time>,
    wash: Res<Wash>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    gates: Query<&ExitMaterials>,
) {
    let open = wash.of(Hue::White);
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 3.0).sin();
    let white = hue_colour(Hue::White);
    for gate in &gates {
        if let Some(mut frame) = materials.get_mut(&gate.frame) {
            frame.color = EXIT_CLOSED.mix(&white, open);
        }
        if let Some(mut glow) = materials.get_mut(&gate.glow) {
            let alpha = 0.15 + open * (0.35 + 0.3 * pulse);
            glow.color = EXIT_CLOSED.mix(&white, open).with_alpha(alpha);
        }
    }
}

fn tint_background(wash: Res<Wash>, clear: Option<ResMut<ClearColor>>) {
    let Some(mut clear) = clear else {
        return;
    };
    if !wash.is_changed() {
        return;
    }
    clear.0 = crate::BACKGROUND.mix(&BACKGROUND_RESTORED, wash.of(Hue::White));
}

/// The fill of one primary's gauge in the HUD.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaugeFill(pub Primary);

fn spawn_gauges(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Spectrum gauges"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Vw(1.5),
                top: Val::Vh(10.0),
                column_gap: Val::Vw(0.5),
                ..default()
            },
        ))
        .with_children(|row| {
            for primary in Primary::ALL {
                row.spawn((
                    Node {
                        width: Val::Vw(5.0),
                        height: Val::Vh(1.2),
                        padding: UiRect::all(Val::Vh(0.2)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                ))
                .with_children(|gauge| {
                    gauge.spawn((
                        GaugeFill(primary),
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(hue_colour(primary.into())),
                    ));
                });
            }
        });
}

/// Gauges fill with light and glow at full strength once restored.
fn update_gauges(
    spectrum: Res<Spectrum>,
    tuning: Res<LightTuning>,
    mut fills: Query<(&GaugeFill, &mut Node, &mut BackgroundColor)>,
) {
    if !spectrum.is_changed() && !tuning.is_changed() {
        return;
    }
    for (fill, mut node, mut background) in &mut fills {
        let primary = fill.0;
        node.width = Val::Percent(spectrum.fill(primary, tuning.gauge_capacity) * 100.0);
        let colour = hue_colour(primary.into());
        background.0 = if spectrum.restored[primary.index()] {
            colour
        } else {
            colour.with_alpha(0.55)
        };
    }
}

/// How long the level-complete banner shows (seconds).
const BANNER_TIME: f32 = 3.0;

/// The level-complete banner, and how long it has left.
#[derive(Component, Debug, Default)]
pub struct Banner(pub f32);

fn spawn_banner(mut commands: Commands) {
    commands.spawn((
        Name::new("Level complete banner"),
        Banner::default(),
        Text::new("Spectrum restored!"),
        TextFont {
            font_size: FontSize::Vh(6.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Vh(40.0),
            justify_self: JustifySelf::Center,
            ..default()
        },
        Visibility::Hidden,
    ));
}

fn show_banner(
    time: Res<Time>,
    completed: Res<LevelsCompleted>,
    mut banners: Query<(&mut Banner, &mut Visibility)>,
) {
    for (mut banner, mut visibility) in &mut banners {
        if completed.is_changed() && completed.0 > 0 {
            banner.0 = BANNER_TIME;
        }
        banner.0 = (banner.0 - time.delta_secs()).max(0.0);
        visibility.set_if_neq(if banner.0 > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn wash_is_limited_by_the_least_washed_primary() {
        let wash = Wash([1.0, 0.5, 0.0]);
        assert!((wash.of(Hue::Red) - 1.0).abs() < f32::EPSILON);
        assert!((wash.of(Hue::Yellow) - 0.5).abs() < f32::EPSILON);
        assert!(wash.of(Hue::White).abs() < f32::EPSILON);
    }

    #[test]
    fn shading_runs_from_grey_to_colour() {
        assert_eq!(shaded_colour(Hue::Red, 0.0, false), SOLID_GREY);
        assert_eq!(shaded_colour(Hue::Red, 0.0, true), SCENERY_GREY);
        let red = shaded_colour(Hue::Red, 1.0, false).to_srgba();
        assert!(red.red > red.green && red.red > red.blue);
        let blue = shaded_colour(Hue::Blue, 1.0, true).to_srgba();
        assert!(blue.blue > blue.red);
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(ClearColor(crate::BACKGROUND))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins((SimulationPlugin, crate::render::PresentationPlugin));
        app.update();
        app
    }

    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
        }
    }

    fn colour_of<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> Vec<Color> {
        let handles: Vec<_> = {
            let world = app.world_mut();
            world
                .query_filtered::<&MeshMaterial2d<ColorMaterial>, F>()
                .iter(world)
                .map(|m| m.0.clone())
                .collect()
        };
        let materials = app.world().resource::<Assets<ColorMaterial>>();
        handles
            .iter()
            .filter_map(|h| materials.get(h).map(|m| m.color))
            .collect()
    }

    #[test]
    fn restoring_a_colour_floods_it_into_the_world() {
        let mut app = app();
        run(&mut app, 2);
        let grey = colour_of::<(With<Shade>, With<Solid>)>(&mut app);
        assert!(grey.iter().all(|&c| c == SOLID_GREY));
        for primary in Primary::ALL {
            app.world_mut()
                .resource_mut::<Spectrum>()
                .restore(primary, 10.0);
        }
        run(&mut app, 10);
        let partway = app.world().resource::<Wash>().0[0];
        assert!(partway > 0.0 && partway < 1.0, "should flood in gradually");
        run(&mut app, 200);
        assert_eq!(app.world().resource::<Wash>().0, [1.0; 3]);
        let coloured = colour_of::<(With<Shade>, With<Solid>)>(&mut app);
        assert!(coloured.iter().all(|&c| c != SOLID_GREY));
        assert_eq!(app.world().resource::<ClearColor>().0, BACKGROUND_RESTORED);
    }

    #[test]
    fn phantoms_fill_in_when_solid() {
        let mut app = app();
        run(&mut app, 2);
        let alphas = |app: &mut App| -> Vec<f32> {
            colour_of::<With<Phantom>>(app)
                .iter()
                .map(Alpha::alpha)
                .collect()
        };
        assert!(alphas(&mut app).iter().all(|&a| a < 0.5));
        app.world_mut()
            .resource_mut::<Spectrum>()
            .restore(Primary::Green, 10.0);
        run(&mut app, 3);
        let filled = alphas(&mut app)
            .iter()
            .filter(|&&a| (a - 1.0).abs() < f32::EPSILON)
            .count();
        assert_eq!(filled, 1, "only the green phantom fills");
        // Solid phantoms keep their own look rather than the solids'.
        let solid_phantom_meshes = colour_of::<(With<Phantom>, With<Solid>)>(&mut app);
        assert!(solid_phantom_meshes.iter().all(|&c| c != SOLID_GREY));
    }

    #[test]
    fn gauges_show_light_and_the_banner_shows_on_completion() {
        let mut app = app();
        app.world_mut().resource_mut::<Spectrum>().gauges[2] = 5.0;
        app.update();
        let width = {
            let world = app.world_mut();
            world
                .query::<(&GaugeFill, &Node)>()
                .iter(world)
                .find(|(g, _)| g.0 == Primary::Blue)
                .unwrap()
                .1
                .width
        };
        assert_eq!(width, Val::Percent(50.0));

        let banner_shown = |app: &mut App| {
            let world = app.world_mut();
            *world
                .query_filtered::<&Visibility, With<Banner>>()
                .single(world)
                .unwrap()
                == Visibility::Inherited
        };
        assert!(!banner_shown(&mut app));
        app.world_mut().resource_mut::<LevelsCompleted>().0 += 1;
        app.update();
        assert!(banner_shown(&mut app));
        run(&mut app, 200);
        assert!(!banner_shown(&mut app));
    }

    #[test]
    fn the_exit_glows_once_white_returns() {
        let mut app = app();
        run(&mut app, 2);
        let frame = |app: &mut App| {
            let handle = {
                let world = app.world_mut();
                world
                    .query::<&ExitMaterials>()
                    .single(world)
                    .unwrap()
                    .frame
                    .clone()
            };
            app.world()
                .resource::<Assets<ColorMaterial>>()
                .get(&handle)
                .unwrap()
                .color
        };
        assert_eq!(frame(&mut app), EXIT_CLOSED);
        for primary in Primary::ALL {
            app.world_mut()
                .resource_mut::<Spectrum>()
                .restore(primary, 10.0);
        }
        run(&mut app, 200);
        assert_eq!(frame(&mut app), hue_colour(Hue::White));
    }
}
