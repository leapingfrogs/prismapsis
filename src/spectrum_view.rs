//! Presentation for light and colour.
//!
//! Shows the world washing from grey into colour, phantom platforms, the exit gate, the spectrum gauges and the
//! level-complete banner. Nothing here feeds back into game logic.

use bevy::prelude::*;

use crate::art::figures::GlowArt;
use crate::art::glow;
use crate::art::tiles::PhantomTiles;
use crate::hud::{self, HudArt};
use crate::level::{CurrentLevel, Solid};
use crate::light::{ExitGate, Hue, LevelsCompleted, LightTuning, Phantom, Primary, Spectrum};
use crate::screens::{Screen, in_play};

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
                    draw_phantoms,
                    draw_exit,
                    (show_phantoms, animate_exit).after(advance_wash),
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

/// Which art a phantom's mesh shows: waiting, or solid.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhantomLook {
    pub solid: bool,
}

/// A phantom's mesh: ghost tiles while it waits, crystal once solid,
/// tinted with its hue.
fn phantom_mesh(phantom: &Phantom, tile: f32, solid: bool) -> Mesh {
    let [r, g, b, _] = hue_colour(phantom.hue).to_linear().to_f32_array();
    let glass = if solid {
        crate::art::phantoms::Glass::Crystal
    } else {
        crate::art::phantoms::Glass::Ghost
    };
    crate::art::phantoms::mesh(&phantom.aabb, tile, glass, [r, g, b])
}

/// Phantoms are drawn from the phantom tileset, one mesh each.
fn draw_phantoms(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tiles: Option<Res<PhantomTiles>>,
    level: Res<CurrentLevel>,
    phantoms: Query<(Entity, &Phantom, Has<Solid>), Added<Phantom>>,
) {
    let Some(tiles) = tiles else { return };
    for (entity, phantom, solid) in &phantoms {
        commands.entity(entity).insert((
            Mesh2d(meshes.add(phantom_mesh(phantom, level.0.tile, solid))),
            MeshMaterial2d(tiles.0.clone()),
            Transform::from_xyz(0.0, 0.0, 0.2),
            PhantomLook { solid },
        ));
    }
}

/// Swap a phantom's art when it turns solid, or back.
fn show_phantoms(
    mut meshes: ResMut<Assets<Mesh>>,
    level: Res<CurrentLevel>,
    mut phantoms: Query<(&Phantom, Has<Solid>, &mut PhantomLook, &mut Mesh2d)>,
) {
    for (phantom, solid, mut look, mut mesh) in &mut phantoms {
        if look.solid == solid {
            continue;
        }
        look.solid = solid;
        meshes.remove(&mesh.0);
        mesh.0 = meshes.add(phantom_mesh(phantom, level.0.tile, solid));
    }
}

/// The exit's frame tint while closed: pale stone in the dark.
const EXIT_CLOSED: Color = Color::srgb(0.55, 0.56, 0.62);

/// How strongly the doorway glows while closed.
const EXIT_CLOSED_GLOW: f32 = 0.08;

/// How much brighter than white the open exit's light burns.
const EXIT_LIGHT: f32 = 1.5;

/// The light filling an exit's doorway.
#[derive(Component, Debug)]
struct ExitLight;

/// The frame's art is twice the world's scale, so its corners are drawn
/// at half size.
const FRAME_SCALE: f32 = 0.5;

/// The exit is a doorway: a frame sliced to fit its size, with light
/// behind it filling the opening.
fn draw_exit(
    mut commands: Commands,
    art: Option<Res<GlowArt>>,
    gates: Query<(Entity, &ExitGate), Added<ExitGate>>,
) {
    let art = art.map(|a| a.clone()).unwrap_or_default();
    for (entity, gate) in &gates {
        let size = gate.0.size();
        let thickness = glow::FRAME_THICKNESS_PX * FRAME_SCALE;
        let opening = Vec2::new(size.x - 2.0 * thickness, size.y - thickness);
        commands.entity(entity).insert((
            Sprite {
                image: art.exit_frame.clone(),
                color: EXIT_CLOSED,
                custom_size: Some(size),
                image_mode: SpriteImageMode::Sliced(TextureSlicer {
                    border: BorderRect::all(glow::FRAME_BORDER_PX),
                    max_corner_scale: FRAME_SCALE,
                    ..default()
                }),
                ..default()
            },
            Transform::from_translation(gate.0.center().extend(0.3)),
            children![(
                ExitLight,
                Sprite {
                    image: art.exit_light.clone(),
                    color: Color::WHITE.with_alpha(EXIT_CLOSED_GLOW),
                    custom_size: Some(opening),
                    ..default()
                },
                Transform::from_xyz(0.0, -thickness / 2.0, -0.01),
            )],
        ));
    }
}

/// Dim while closed; once white is restored, the frame shines and the
/// doorway's light pulses.
fn animate_exit(
    time: Res<Time>,
    wash: Res<Wash>,
    mut frames: Query<&mut Sprite, (With<ExitGate>, Without<ExitLight>)>,
    mut lights: Query<&mut Sprite, With<ExitLight>>,
) {
    let open = wash.of(Hue::White);
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 3.0).sin();
    let white = hue_colour(Hue::White);
    for mut frame in &mut frames {
        frame.color = EXIT_CLOSED.mix(&white, open);
    }
    for mut light in &mut lights {
        let alpha = EXIT_CLOSED_GLOW + open * (0.55 + 0.35 * pulse);
        // Once open, the light burns brighter than white, so it blooms.
        light.color = crate::render::glowing(white, 1.0 + open * EXIT_LIGHT).with_alpha(alpha);
    }
}

/// The fill of one primary's gauge in the HUD.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaugeFill(pub Primary);

/// A glass tube for each primary under the energy bar, filling with that
/// colour's light.
fn spawn_gauges(mut commands: Commands, art: Res<HudArt>) {
    commands
        .spawn((
            Name::new("Spectrum gauges"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Vw(1.5),
                top: Val::Vh(crate::render::HUD_GAUGES_TOP),
                column_gap: Val::Vw(0.5),
                ..default()
            },
        ))
        .with_children(|row| {
            for primary in Primary::ALL {
                row.spawn(hud::tube(
                    &art,
                    Val::Vw(5.0),
                    2.2,
                    hue_colour(primary.into()),
                    GaugeFill(primary),
                ));
            }
        });
}

/// Gauges fill with light and glow at full strength once restored.
fn update_gauges(
    spectrum: Res<Spectrum>,
    tuning: Res<LightTuning>,
    mut fills: Query<(&GaugeFill, &mut Node, &mut ImageNode)>,
) {
    if !spectrum.is_changed() && !tuning.is_changed() {
        return;
    }
    for (fill, mut node, mut image) in &mut fills {
        let primary = fill.0;
        node.width = Val::Percent(spectrum.fill(primary, tuning.gauge_capacity) * 100.0);
        let colour = hue_colour(primary.into());
        image.color = if spectrum.restored[primary.index()] {
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

/// The name of the level the banner leads into.
#[derive(Component, Debug)]
pub struct BannerLevel;

/// "Spectrum restored!" in shimmering spectrum letters on a glass panel,
/// with the next level's name below.
fn spawn_banner(mut commands: Commands, art: Option<Res<HudArt>>) {
    let art = art.as_deref().cloned().unwrap_or_default();
    commands
        .spawn((
            Name::new("Level complete banner"),
            Banner::default(),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Vh(34.0),
                justify_self: JustifySelf::Center,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::VMin(1.0),
                padding: UiRect::axes(Val::VMin(7.0), Val::VMin(3.0)),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|banner| {
            banner.spawn(hud::panel(&art));
            hud::spectrum_text(banner, "Spectrum restored!", 7.0);
            banner.spawn((
                BannerLevel,
                Text::default(),
                TextFont {
                    font_size: FontSize::VMin(3.5),
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.87, 0.93)),
            ));
        });
}

/// Show the banner when a level is completed, naming the next level. A
/// menu screen hides it.
fn show_banner(
    time: Res<Time>,
    completed: Res<LevelsCompleted>,
    level: Option<Res<CurrentLevel>>,
    screen: Option<Res<State<Screen>>>,
    mut banners: Query<(&mut Banner, &mut Visibility)>,
    mut names: Query<&mut Text, With<BannerLevel>>,
) {
    let playing = in_play(screen);
    for (mut banner, mut visibility) in &mut banners {
        if !playing {
            banner.0 = 0.0;
        }
        if completed.is_changed() && completed.0 > 0 {
            banner.0 = BANNER_TIME;
            let next = level.as_ref().map_or("", |l| l.0.name.as_str());
            for mut name in &mut names {
                next.clone_into(&mut name.0);
            }
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

    /// How far each theme tileset's art has been told colour is restored;
    /// `w`, how strongly it takes light, is left out.
    fn washes(app: &App) -> Vec<Vec3> {
        let materials = app
            .world()
            .resource::<Assets<crate::art::tiles::WorldMaterial>>();
        let tilesets = app.world().resource::<crate::art::tiles::Tilesets>();
        tilesets
            .0
            .iter()
            .flatten()
            .filter_map(|handle| materials.get(handle).map(|m| m.wash.truncate()))
            .collect()
    }

    #[test]
    fn restoring_a_colour_floods_it_into_the_world() {
        let mut app = app();
        run(&mut app, 2);
        let tilesets = 4 * crate::art::paint::themes().len();
        assert_eq!(
            washes(&app),
            vec![Vec3::ZERO; tilesets],
            "walls and scenery"
        );
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
        assert_eq!(washes(&app), vec![Vec3::ONE; tilesets]);
        let restored = crate::art::tiles::background(0, [1.0; 3]);
        assert_eq!(app.world().resource::<ClearColor>().0, restored);
    }

    #[test]
    fn phantoms_turn_to_crystal_when_solid() {
        let mut app = app();
        run(&mut app, 2);
        let solid = |app: &mut App| -> Vec<bool> {
            let world = app.world_mut();
            world
                .query::<&PhantomLook>()
                .iter(world)
                .map(|look| look.solid)
                .collect()
        };
        let looks = solid(&mut app);
        assert!(!looks.is_empty() && looks.iter().all(|&s| !s));
        app.world_mut()
            .resource_mut::<Spectrum>()
            .restore(Primary::Green, 10.0);
        run(&mut app, 3);
        let filled = solid(&mut app).iter().filter(|&&s| s).count();
        assert_eq!(filled, 1, "only the green phantom turns solid");
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
            let world = app.world_mut();
            world
                .query_filtered::<&Sprite, With<ExitGate>>()
                .single(world)
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
