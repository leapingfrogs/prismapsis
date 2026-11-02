//! Draws walls and scenery from the theme's tilesets.
//!
//! Each is one quad per cell corner, picked by which of the four cells
//! around it are filled, drawn through a shader that holds back each hue
//! until it has been restored.

use super::backdrops::{BACKDROP_LAYERS, BACKDROP_PX, BACKDROP_UNITS_PER_PX, PARALLAX};
use super::features::{FEATURE_LAYERS, Feature, FeatureCells};
use super::figures::Sheet;
use super::filtered;
use super::paint::{Kind, Theme, linear, theme_index, themes};
use super::phantoms::Glass;
use super::sprites::{CELL_SIZE, Figure, POST_REACH, PX_PER_UNIT};
use super::{BOTTOM_LEFT, BOTTOM_RIGHT, LAYERS, TOP_LEFT, TOP_RIGHT, layer, tint, variant_at};
use crate::level::{Campaign, CurrentLevel, LevelGeometry, Scenery, Solid};
use crate::light::{Hue, Phantom, Shade};
use crate::physics::{Aabb, Traits};
use crate::platforms::{BREAK_TICKS, Breakable, Changing, Crumbling, Moving};
use crate::render::MainCamera;
use crate::spectrum_view::Wash;
use crate::spectrum_view::hue_colour;
use bevy::asset::{RenderAssetUsages, embedded_asset, embedded_path};
use bevy::ecs::system::SystemParam;
use bevy::image::{
    CompressedImageFormats, ImageAddressMode, ImageSampler, ImageSamplerDescriptor, ImageType,
};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

/// Each theme's walls and panels, painted by `just art`, in campaign order.
const TILESETS: [[&[u8]; 2]; 3] = [
    [
        include_bytes!("../../assets/art/quarry/walls.png"),
        include_bytes!("../../assets/art/quarry/panels.png"),
    ],
    [
        include_bytes!("../../assets/art/caverns/walls.png"),
        include_bytes!("../../assets/art/caverns/panels.png"),
    ],
    [
        include_bytes!("../../assets/art/observatory/walls.png"),
        include_bytes!("../../assets/art/observatory/panels.png"),
    ],
];

/// Each theme's features (walls with traits), painted by `just art`.
const FEATURES: [&[u8]; 3] = [
    include_bytes!("../../assets/art/quarry/features.png"),
    include_bytes!("../../assets/art/caverns/features.png"),
    include_bytes!("../../assets/art/observatory/features.png"),
];

/// Each theme's backdrop, painted by `just art`.
const BACKDROPS: [&[u8]; 3] = [
    include_bytes!("../../assets/art/quarry/backdrop.png"),
    include_bytes!("../../assets/art/caverns/backdrop.png"),
    include_bytes!("../../assets/art/observatory/backdrop.png"),
];

/// The backdrop's layers are drawn behind the scenery panels, far behind
/// near.
const BACKDROP_DEPTH: [f32; 2] = [-3.0, -2.0];

/// How much bigger than the view the backdrop is drawn, so its edges never
/// show.
const BACKDROP_MARGIN: f32 = 1.1;

/// Phantom platforms, painted by `just art` and tinted with their hues.
const PHANTOMS: &[u8] = include_bytes!("../../assets/art/phantoms.png");

/// Features are drawn just in front of the walls they sit against.
const FEATURE_DEPTH: f32 = 0.1;

/// Scenery is drawn behind the walls.
const PANEL_DEPTH: f32 = -1.0;

#[derive(Debug, Default)]
pub struct TilesPlugin;

impl Plugin for TilesPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "world.wesl");
        app.add_plugins(Material2dPlugin::<WorldMaterial>::default())
            .init_resource::<ThemeChoice>()
            .add_systems(Startup, load_tiles)
            .add_systems(
                Update,
                (
                    draw_wall_tiles,
                    draw_panel_tiles,
                    draw_feature_tiles,
                    wash_tiles,
                    tint_background,
                    theme_backdrop,
                    draw_changing,
                    show_changing,
                    theme_changing,
                ),
            )
            .add_systems(Startup, spawn_backdrop.after(load_tiles))
            .add_systems(
                PostUpdate,
                scroll_backdrop.before(TransformSystems::Propagate),
            );
    }
}

/// Theme art drawn through the colour filter.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WorldMaterial {
    /// How far red, green and blue have been restored, 0 to 1.
    #[uniform(0)]
    pub wash: Vec4,
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    pub tiles: Handle<Image>,
}

impl Material2d for WorldMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            bevy::asset::AssetPath::from_path_buf(embedded_path!("world.wesl"))
                .with_source("embedded"),
        )
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// The materials walls, scenery and features are drawn with: each theme's
/// walls and panels, then its features and backdrop, in campaign order.
#[derive(Resource, Debug)]
pub struct Tilesets(pub Vec<[Handle<WorldMaterial>; 4]>);

impl Tilesets {
    /// The material for `kind` in theme `theme`.
    fn material(&self, theme: usize, kind: Kind) -> Handle<WorldMaterial> {
        self.0[theme][kind as usize].clone()
    }

    /// The material for features in theme `theme`.
    fn features(&self, theme: usize) -> Handle<WorldMaterial> {
        self.0[theme][2].clone()
    }

    /// The material for theme `theme`'s backdrop.
    fn backdrop(&self, theme: usize) -> Handle<WorldMaterial> {
        self.0[theme][3].clone()
    }
}

/// The material phantoms are drawn with. Phantoms keep their colour, so
/// it shows every hue in full.
#[derive(Resource, Debug)]
pub struct PhantomTiles(pub Handle<WorldMaterial>);

/// A theme picked in the dev panel to preview, in place of the campaign's.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ThemeChoice(pub Option<usize>);

/// The theme of the level being played; levels outside a campaign use the
/// first.
fn current_theme(campaign: Option<&Campaign>) -> usize {
    campaign.map_or(0, |c| theme_index(c.current))
}

/// The theme being drawn: the dev panel's choice, or the campaign's.
#[derive(SystemParam)]
struct ThemeInPlay<'w> {
    campaign: Option<Res<'w, Campaign>>,
    choice: Res<'w, ThemeChoice>,
}

impl ThemeInPlay<'_> {
    fn index(&self) -> usize {
        self.choice
            .0
            .unwrap_or_else(|| current_theme(self.campaign.as_deref()))
    }

    fn theme(&self) -> Theme {
        themes()[self.index()]
    }

    fn changed(&self) -> bool {
        self.choice.is_changed() || self.campaign.as_ref().is_some_and(Res::is_changed)
    }
}

/// The entity holding the wall tiles' mesh.
#[derive(Component, Debug)]
pub struct WallMesh;

/// The entity holding the features' mesh.
#[derive(Component, Debug)]
pub struct FeatureMesh;

/// The entity holding the scenery panels' mesh.
#[derive(Component, Debug)]
pub struct PanelMesh;

/// Decode a tileset whose `layers` tiles are stacked top to bottom into an
/// array texture, one layer per tile.
///
/// # Errors
/// If the image can't be decoded or isn't a stack of `layers` tiles.
pub fn tileset_image(png: &[u8], layers: u16) -> Result<Image, String> {
    let mut image = Image::from_buffer(
        png,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        RenderAssetUsages::RENDER_WORLD,
    )
    .map_err(|e| e.to_string())?;
    image
        .reinterpret_stacked_2d_as_array(layers.into())
        .map_err(|e| e.to_string())?;
    Ok(image)
}

/// Without images (headless tests), walls and scenery still get a mesh and
/// material, just no tileset.
fn load_tiles(
    mut commands: Commands,
    mut images: Option<ResMut<Assets<Image>>>,
    mut materials: ResMut<Assets<WorldMaterial>>,
) {
    let mut load = |name: &str, png: &[u8], layers: u16, repeat: bool| {
        let image = tileset_image(png, layers).map(|mut image| {
            if repeat {
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                });
            }
            image
        });
        let tiles = match (images.as_mut(), image) {
            (Some(images), Ok(image)) => images.add(image),
            (None, _) => Handle::default(),
            (_, Err(problem)) => {
                error!("{name} tileset: {problem}");
                Handle::default()
            }
        };
        materials.add(WorldMaterial {
            wash: Vec4::ZERO,
            tiles,
        })
    };
    let themes = TILESETS
        .iter()
        .zip(FEATURES)
        .zip(BACKDROPS)
        .map(|((&[walls, panels], features), backdrop)| {
            [
                load(Kind::Walls.name(), walls, LAYERS, false),
                load(Kind::Panels.name(), panels, LAYERS, false),
                load("features", features, FEATURE_LAYERS, false),
                load("backdrop", backdrop, BACKDROP_LAYERS, true),
            ]
        })
        .collect();
    commands.insert_resource(Tilesets(themes));
    let phantoms = load("phantoms", PHANTOMS, super::phantoms::PHANTOM_LAYERS, false);
    if let Some(mut material) = materials.get_mut(&phantoms) {
        material.wash = Vec4::ONE;
    }
    commands.insert_resource(PhantomTiles(phantoms));
}

/// Which cells are filled, and the tint each gives the art.
#[derive(Debug, Clone, PartialEq)]
pub struct Cells {
    cols: i32,
    rows: i32,
    /// What's beyond the room: plain wall for walls, so the room's border
    /// has no edge drawn against nothing; empty for scenery.
    outside: Option<[f32; 3]>,
    /// Row-major from the bottom-left; `None` is open.
    filled: Vec<Option<[f32; 3]>>,
}

impl Cells {
    /// Rasterise `areas` (cell-aligned boxes in a room of `size`, with an
    /// optional hue each) onto the level grid, as `kind` art.
    #[must_use]
    pub fn new(
        kind: Kind,
        theme: &Theme,
        size: Vec2,
        tile: f32,
        areas: &[(Aabb, Option<Hue>)],
    ) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let cell = |v: f32| (v / tile).round() as i32;
        let (cols, rows) = (cell(size.x), cell(size.y));
        let mut filled = vec![None; usize::try_from(cols * rows).unwrap_or(0)];
        for &(aabb, hue) in areas {
            let tint = tint(hue, theme.hue_tint(kind));
            for row in cell(aabb.min.y).max(0)..cell(aabb.max.y).min(rows) {
                for col in cell(aabb.min.x).max(0)..cell(aabb.max.x).min(cols) {
                    if let Ok(i) = usize::try_from(row * cols + col) {
                        filled[i] = Some(tint);
                    }
                }
            }
        }
        Self {
            cols,
            rows,
            outside: (kind == Kind::Walls).then_some([1.0; 3]),
            filled,
        }
    }

    /// The cell's tint if it's filled.
    fn get(&self, col: i32, row: i32) -> Option<[f32; 3]> {
        if col < 0 || row < 0 || col >= self.cols || row >= self.rows {
            return self.outside;
        }
        usize::try_from(row * self.cols + col)
            .ok()
            .and_then(|i| self.filled[i])
    }

    /// The tint at a cell's centre: its own, or for an open cell its first
    /// filled neighbour's, so the colour doesn't fade out towards edges.
    fn tint(&self, col: i32, row: i32) -> [f32; 3] {
        self.get(col, row)
            .or_else(|| {
                [(0, -1), (0, 1), (-1, 0), (1, 0)]
                    .iter()
                    .find_map(|(dc, dr)| self.get(col + dc, row + dr))
            })
            .unwrap_or([1.0; 3])
    }

    /// Which of the four cells around corner `(x, y)` are filled.
    #[must_use]
    pub fn mask(&self, x: i32, y: i32) -> u8 {
        [
            (TOP_LEFT, x - 1, y),
            (TOP_RIGHT, x, y),
            (BOTTOM_LEFT, x - 1, y - 1),
            (BOTTOM_RIGHT, x, y - 1),
        ]
        .iter()
        .filter(|&&(_, col, row)| self.get(col, row).is_some())
        .fold(0, |mask, &(bit, ..)| mask | bit)
    }

    /// A mesh with a quad on every corner that touches a wall. Each quad's
    /// corners sit on cell centres and carry that cell's tint, with the
    /// tileset layer in the alpha channel.
    #[must_use]
    pub fn mesh(&self, tile: f32) -> Mesh {
        let (mut positions, mut uvs, mut colours, mut indices) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for y in 0..=self.rows {
            for x in 0..=self.cols {
                let mask = self.mask(x, y);
                if mask == 0 {
                    continue;
                }
                let layer = f32::from(layer(mask, variant_at(x, y)));
                let base = u32::try_from(positions.len()).unwrap_or(0);
                // Top-left, top-right, bottom-left, bottom-right.
                for (dx, dy, u, v) in [
                    (-1, 0, 0.0, 0.0),
                    (0, 0, 1.0, 0.0),
                    (-1, -1, 0.0, 1.0),
                    (0, -1, 1.0, 1.0),
                ] {
                    let (col, row) = (x + dx, y + dy);
                    #[allow(clippy::cast_precision_loss)]
                    let centre = Vec2::new(col as f32 + 0.5, row as f32 + 0.5) * tile;
                    positions.push([centre.x, centre.y, 0.0]);
                    uvs.push([u, v]);
                    let [r, g, b] = self.tint(col, row);
                    colours.push([r, g, b, layer]);
                }
                indices.extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(indices))
    }
}

/// Rebuild the wall tiles whenever walls are spawned (a new or reloaded
/// level). Phantoms draw themselves.
fn draw_wall_tiles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tilesets: Option<Res<Tilesets>>,
    theme: ThemeInPlay,
    level: Res<CurrentLevel>,
    added: Query<(), (Added<Solid>, Without<Phantom>, Without<Changing>)>,
    walls: Query<(&Solid, Option<&Shade>, Option<&Traits>), (Without<Phantom>, Without<Changing>)>,
    old: Query<Entity, With<WallMesh>>,
) {
    let Some(tilesets) = tilesets else { return };
    if added.is_empty() && !theme.choice.is_changed() {
        return;
    }
    // Walls with traits draw as features instead.
    let walls: Vec<_> = walls
        .iter()
        .filter(|(.., traits)| traits.is_none_or(|&t| Feature::of(t).is_none()))
        .map(|(s, shade, _)| (s.0, shade.map(|s| s.0)))
        .collect();
    let level = &level.0;
    let cells = Cells::new(Kind::Walls, &theme.theme(), level.size, level.tile, &walls);
    replace_mesh(
        &mut commands,
        &old,
        (
            Name::new("Wall tiles"),
            WallMesh,
            Mesh2d(meshes.add(cells.mesh(level.tile))),
            MeshMaterial2d(tilesets.material(theme.index(), Kind::Walls)),
            Transform::default(),
        ),
    );
}

/// Rebuild the features whenever walls are spawned.
fn draw_feature_tiles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tilesets: Option<Res<Tilesets>>,
    theme: ThemeInPlay,
    level: Res<CurrentLevel>,
    added: Query<(), (Added<Solid>, Without<Phantom>, Without<Changing>)>,
    walls: Query<(&Solid, Option<&Shade>, &Traits), (Without<Phantom>, Without<Changing>)>,
    old: Query<Entity, With<FeatureMesh>>,
) {
    let Some(tilesets) = tilesets else { return };
    if added.is_empty() && !theme.choice.is_changed() {
        return;
    }
    let walls: Vec<_> = walls
        .iter()
        .map(|(s, shade, &traits)| (s.0, shade.map(|s| s.0), traits))
        .collect();
    let level = &level.0;
    let cells = FeatureCells::new(level.size, level.tile, &walls);
    for entity in &old {
        commands.entity(entity).despawn();
    }
    if cells.is_empty() {
        return;
    }
    commands.spawn((
        LevelGeometry,
        Name::new("Features"),
        FeatureMesh,
        Mesh2d(meshes.add(cells.mesh())),
        MeshMaterial2d(tilesets.features(theme.index())),
        Transform::from_xyz(0.0, 0.0, FEATURE_DEPTH),
    ));
}

/// Rebuild the scenery panels whenever scenery is spawned.
fn draw_panel_tiles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tilesets: Option<Res<Tilesets>>,
    theme: ThemeInPlay,
    level: Res<CurrentLevel>,
    added: Query<(), Added<Scenery>>,
    scenery: Query<(&Scenery, Option<&Shade>)>,
    old: Query<Entity, With<PanelMesh>>,
) {
    let Some(tilesets) = tilesets else { return };
    if added.is_empty() && !theme.choice.is_changed() {
        return;
    }
    let scenery: Vec<_> = scenery
        .iter()
        .map(|(s, shade)| (s.0, shade.map(|s| s.0)))
        .collect();
    // With no scenery there's nothing to draw, and Bevy warns about empty meshes.
    if scenery.is_empty() {
        for entity in &old {
            commands.entity(entity).despawn();
        }
        return;
    }
    let level = &level.0;
    let cells = Cells::new(
        Kind::Panels,
        &theme.theme(),
        level.size,
        level.tile,
        &scenery,
    );
    replace_mesh(
        &mut commands,
        &old,
        (
            Name::new("Scenery panels"),
            PanelMesh,
            Mesh2d(meshes.add(cells.mesh(level.tile))),
            MeshMaterial2d(tilesets.material(theme.index(), Kind::Panels)),
            Transform::from_xyz(0.0, 0.0, PANEL_DEPTH),
        ),
    );
}

/// One of the backdrop's layers, by index: 0 far, 1 near.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackdropLayer(pub usize);

/// The backdrop's layers, which stay for the whole game and follow the
/// camera; `scroll_backdrop` sizes them to the view.
fn spawn_backdrop(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tilesets: Res<Tilesets>,
    theme: ThemeInPlay,
) {
    for (layer, depth) in BACKDROP_DEPTH.into_iter().enumerate() {
        commands.spawn((
            Name::new(format!("Backdrop layer {layer}")),
            BackdropLayer(layer),
            Mesh2d(meshes.add(backdrop_quad(layer, Vec2::ZERO, Vec2::ONE))),
            MeshMaterial2d(tilesets.backdrop(theme.index())),
            Transform::from_xyz(0.0, 0.0, depth),
        ));
    }
}

/// A quad for backdrop `layer`, `size` across, centred on the camera at
/// `camera`. Its texture is placed so the layer moves `PARALLAX` as far as
/// the level does when the camera moves.
#[must_use]
pub fn backdrop_quad(layer: usize, camera: Vec2, size: Vec2) -> Mesh {
    #[allow(clippy::cast_precision_loss)]
    let tile = BACKDROP_PX as f32 * BACKDROP_UNITS_PER_PX;
    let parallax = PARALLAX[layer.min(PARALLAX.len() - 1)];
    let half = size / 2.0;
    #[allow(clippy::cast_precision_loss)]
    let alpha = layer as f32;
    let corners = [
        Vec2::new(-half.x, half.y),
        Vec2::new(half.x, half.y),
        Vec2::new(-half.x, -half.y),
        Vec2::new(half.x, -half.y),
    ];
    let positions: Vec<[f32; 3]> = corners.iter().map(|c| [c.x, c.y, 0.0]).collect();
    // Texture v runs down the image while world y runs up.
    let uvs: Vec<[f32; 2]> = corners
        .iter()
        .map(|&c| {
            let at = camera * parallax + c;
            [at.x / tile, -at.y / tile]
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0, 1.0, 1.0, alpha]; 4])
    .with_inserted_indices(Indices::U32(vec![0, 2, 1, 1, 2, 3]))
}

/// Keep the backdrop behind the view, scrolled for the camera's position.
fn scroll_backdrop(
    mut meshes: ResMut<Assets<Mesh>>,
    cameras: Query<(&Transform, &Projection), (With<MainCamera>, Without<BackdropLayer>)>,
    mut layers: Query<(&BackdropLayer, &Mesh2d, &mut Transform)>,
    mut last: Local<Option<(Vec2, Vec2)>>,
) {
    let Ok((camera, Projection::Orthographic(projection))) = cameras.single() else {
        return;
    };
    let at = camera.translation.truncate();
    let size = projection.area.size() * BACKDROP_MARGIN;
    let changed = *last != Some((at, size));
    *last = Some((at, size));
    for (layer, mesh, mut transform) in &mut layers {
        transform.translation.x = at.x;
        transform.translation.y = at.y;
        if changed && let Some(mut mesh) = meshes.get_mut(&mesh.0) {
            *mesh = backdrop_quad(layer.0, at, size);
        }
    }
}

/// Draw the backdrop of the theme in play.
fn theme_backdrop(
    tilesets: Option<Res<Tilesets>>,
    theme: ThemeInPlay,
    mut layers: Query<&mut MeshMaterial2d<WorldMaterial>, With<BackdropLayer>>,
) {
    let Some(tilesets) = tilesets else { return };
    if !theme.changed() {
        return;
    }
    let wanted = tilesets.backdrop(theme.index());
    for mut material in &mut layers {
        if material.0 != wanted {
            material.0 = wanted.clone();
        }
    }
}

/// A changing platform drawn from the theme's features, so a new theme
/// redraws it.
#[derive(Component, Debug)]
pub struct ChangingFeature;

/// The rail a moving platform runs along.
#[derive(Component, Debug)]
pub struct Rail;

/// How the rail behind a moving platform looks: a dim rod.
const RAIL_COLOUR: Color = Color::srgb(0.4, 0.42, 0.48);

/// How far a worn platform or a damaged block shakes (world units), and
/// how fast (radians per second).
const SHAKE: f32 = 1.2;
const SHAKE_RATE: f32 = 55.0;

/// Give each changing platform its own mesh: crumbling and moving ones from
/// the theme's features, breakable blocks as cracked crystal in their
/// colour. A moving platform also gets a rail along its track.
#[allow(clippy::type_complexity)]
fn draw_changing(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tilesets: Option<Res<Tilesets>>,
    glass: Option<Res<PhantomTiles>>,
    sheet: Option<Res<Sheet>>,
    theme: ThemeInPlay,
    level: Res<CurrentLevel>,
    added: Query<
        (
            Entity,
            &Traits,
            Option<&Shade>,
            Option<&Crumbling>,
            Option<&Breakable>,
            Option<&Moving>,
        ),
        Added<Changing>,
    >,
) {
    let (Some(tilesets), Some(glass)) = (tilesets, glass) else {
        return;
    };
    let level = &level.0;
    for (entity, &traits, shade, crumbling, breakable, moving) in &added {
        let mut drawn = commands.entity(entity);
        drawn.insert((
            Transform::from_xyz(0.0, 0.0, FEATURE_DEPTH),
            Visibility::default(),
        ));
        if let Some(block) = breakable {
            let [r, g, b, _] = hue_colour(block.colour.into()).to_linear().to_f32_array();
            let mesh = super::phantoms::mesh(&block.aabb, level.tile, Glass::Cracked, [r, g, b]);
            drawn.insert((Mesh2d(meshes.add(mesh)), MeshMaterial2d(glass.0.clone())));
            continue;
        }
        let Some(aabb) = crumbling
            .map(|c| c.aabb)
            .or_else(|| moving.map(|m| m.0.aabb))
        else {
            continue;
        };
        let cells = FeatureCells::new(
            level.size,
            level.tile,
            &[(aabb, shade.map(|s| s.0), traits)],
        );
        drawn.insert((
            ChangingFeature,
            Mesh2d(meshes.add(cells.mesh())),
            MeshMaterial2d(tilesets.features(theme.index())),
        ));
        if let (Some(moving), Some(sheet)) = (moving, sheet.as_deref()) {
            commands.spawn((LevelGeometry, Rail, rail(sheet, &moving.0)));
        }
    }
}

/// A rod along a moving platform's whole track, behind it.
fn rail(sheet: &Sheet, mover: &crate::level::format::Mover) -> impl Bundle {
    let start = mover.aabb_at_offset(mover.low).center();
    let end = mover.aabb_at_offset(mover.high).center();
    let length = start.distance(end) + mover.aabb.size().dot(mover.axis.abs());
    let mut sprite = sheet.sprite(Figure::Post, RAIL_COLOUR);
    let inset = (CELL_SIZE / 2.0 - POST_REACH + 2.0) * PX_PER_UNIT;
    sprite.rect = sprite
        .rect
        .map(|r| Rect::new(r.min.x, r.min.y + inset, r.max.x, r.max.y - inset));
    sprite.custom_size = Some(Vec2::new(CELL_SIZE, length));
    // The post stands upright; turn it to lie along a track across.
    let turn = if mover.axis.x.abs() > 0.5 {
        std::f32::consts::FRAC_PI_2
    } else {
        0.0
    };
    (
        sprite,
        Transform::from_translation(start.midpoint(end).extend(FEATURE_DEPTH - 0.05))
            .with_rotation(Quat::from_rotation_z(turn)),
    )
}

/// Move each moving platform's mesh with it; hide crumbled platforms and
/// broken blocks; shake worn platforms and damaged blocks.
#[allow(clippy::type_complexity)]
fn show_changing(
    time: Res<Time>,
    mut platforms: Query<
        (
            Option<&Solid>,
            Option<&Moving>,
            Option<&Crumbling>,
            Option<&Breakable>,
            &mut Transform,
            &mut Visibility,
        ),
        With<Changing>,
    >,
) {
    let wobble = (time.elapsed_secs() * SHAKE_RATE).sin() * SHAKE;
    for (solid, moving, crumbling, breakable, mut transform, mut visibility) in &mut platforms {
        visibility.set_if_neq(if solid.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let mut offset = match (solid, moving) {
            (Some(solid), Some(moving)) => solid.0.min - moving.0.aabb.min,
            _ => Vec2::ZERO,
        };
        let worn = crumbling.is_some_and(|c| c.left < c.landings)
            || breakable.is_some_and(|b| b.health < BREAK_TICKS);
        if worn {
            offset.x += wobble;
        }
        transform.translation.x = offset.x;
        transform.translation.y = offset.y;
    }
}

/// Redraw changing platforms in a newly chosen theme.
fn theme_changing(
    tilesets: Option<Res<Tilesets>>,
    theme: ThemeInPlay,
    mut platforms: Query<&mut MeshMaterial2d<WorldMaterial>, With<ChangingFeature>>,
) {
    let Some(tilesets) = tilesets else { return };
    if !theme.changed() {
        return;
    }
    let wanted = tilesets.features(theme.index());
    for mut material in &mut platforms {
        if material.0 != wanted {
            material.0 = wanted.clone();
        }
    }
}

/// Swap the old mesh entities for a new one, part of the level's geometry.
fn replace_mesh<Q: bevy::ecs::query::QueryFilter>(
    commands: &mut Commands,
    old: &Query<Entity, Q>,
    mesh: impl Bundle,
) {
    for entity in old {
        commands.entity(entity).despawn();
    }
    commands.spawn((LevelGeometry, mesh));
}

fn wash_tiles(
    wash: Res<Wash>,
    tilesets: Option<Res<Tilesets>>,
    mut materials: ResMut<Assets<WorldMaterial>>,
) {
    let Some(tilesets) = tilesets else { return };
    if !wash.is_changed() && !tilesets.is_added() {
        return;
    }
    let [r, g, b] = wash.0;
    for handle in tilesets.0.iter().flatten() {
        if let Some(mut material) = materials.get_mut(handle) {
            material.wash = Vec4::new(r, g, b, 0.0);
        }
    }
}

/// The theme's background colour, through the same filter as its art.
fn tint_background(wash: Res<Wash>, theme: ThemeInPlay, clear: Option<ResMut<ClearColor>>) {
    let Some(mut clear) = clear else { return };
    if !wash.is_changed() && !theme.changed() {
        return;
    }
    clear.0 = background(theme.index(), wash.0);
}

/// A theme's background colour with `wash` of each primary restored.
#[must_use]
pub fn background(theme: usize, wash: [f32; 3]) -> Color {
    let [r, g, b] = filtered(linear(themes()[theme].backdrop), wash);
    Color::linear_rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::SOLID;

    fn cells() -> Cells {
        // A 4×3 room: a floor along the bottom and one red block above it.
        let walls = [
            (Aabb::new(Vec2::ZERO, Vec2::new(160.0, 40.0)), None),
            (
                Aabb::new(Vec2::new(40.0, 40.0), Vec2::new(80.0, 80.0)),
                Some(Hue::Red),
            ),
        ];
        Cells::new(
            Kind::Walls,
            &themes()[0],
            Vec2::new(160.0, 120.0),
            40.0,
            &walls,
        )
    }

    #[test]
    fn corners_see_the_four_cells_around_them() {
        let cells = cells();
        // On the floor's top edge: wall below, open above.
        assert_eq!(cells.mask(3, 1), BOTTOM_LEFT | BOTTOM_RIGHT);
        // Beside the block: the block is above-left as well.
        assert_eq!(cells.mask(2, 1), BOTTOM_LEFT | BOTTOM_RIGHT | TOP_LEFT);
        // Top of the block: wall only below-right.
        assert_eq!(cells.mask(1, 2), BOTTOM_RIGHT);
        assert_eq!(cells.mask(3, 2), 0);
        // Outside the room counts as wall.
        assert_eq!(cells.mask(0, 0), SOLID);
        assert_eq!(
            cells.mask(4, 3),
            SOLID & !BOTTOM_LEFT,
            "open cell below-left"
        );
    }

    #[test]
    fn tints_follow_wall_hues_and_spread_into_open_cells() {
        let cells = cells();
        assert_eq!(cells.tint(0, 0), [1.0; 3]);
        let red = cells.tint(1, 1);
        assert!(red[0] > red[1] && red[0] > red[2]);
        // Open above the block: borrows the block's tint.
        assert_eq!(cells.tint(1, 2), red);
        // Open cells with no wall beside them stay plain.
        assert_eq!(cells.tint(2, 2), [1.0; 3]);
    }

    #[test]
    fn the_mesh_has_a_quad_per_wall_corner() {
        let cells = cells();
        let corners = (0..=3)
            .flat_map(|y| (0..=4).map(move |x| (x, y)))
            .filter(|&(x, y)| cells.mask(x, y) != 0)
            .count();
        let mesh = cells.mesh(40.0);
        assert_eq!(mesh.count_vertices(), corners * 4);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colours)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("no colours");
        };
        // Every quad carries one layer on all four corners.
        for quad in colours.chunks(4) {
            assert!(
                quad.iter()
                    .all(|c| (c[3] - quad[0][3]).abs() < f32::EPSILON)
            );
            assert!(quad[0][3] < f32::from(LAYERS));
        }
    }

    #[test]
    fn a_chosen_theme_redraws_the_level_and_background_in_it() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ColorMaterial>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(ClearColor(crate::BACKGROUND))
            .add_plugins((crate::SimulationPlugin, crate::render::PresentationPlugin));
        for _ in 0..3 {
            app.update();
        }
        let walls_material = |app: &mut App| {
            let world = app.world_mut();
            world
                .query_filtered::<&MeshMaterial2d<WorldMaterial>, With<WallMesh>>()
                .single(world)
                .map_or_else(|e| panic!("{e}"), |m| m.0.clone())
        };
        let tileset =
            |app: &App, theme: usize| app.world().resource::<Tilesets>().0[theme][0].clone();
        assert_eq!(walls_material(&mut app), tileset(&app, 0));
        app.world_mut().resource_mut::<ThemeChoice>().0 = Some(2);
        app.update();
        app.update();
        assert_eq!(walls_material(&mut app), tileset(&app, 2));
        let wash = app.world().resource::<Wash>().0;
        assert_eq!(app.world().resource::<ClearColor>().0, background(2, wash));
    }

    #[test]
    fn the_campaign_position_picks_the_theme_and_its_background() {
        let mut campaign = Campaign::built_in();
        assert_eq!(current_theme(None), 0);
        assert_eq!(current_theme(Some(&campaign)), 0);
        campaign.current = 3;
        assert_eq!(current_theme(Some(&campaign)), 1);
        // The background starts grey and takes the theme's colour.
        let grey = background(1, [0.0; 3]).to_linear();
        assert!((grey.red - grey.blue).abs() < 1e-6);
        assert_ne!(background(1, [1.0; 3]), background(0, [1.0; 3]));
    }

    #[test]
    fn scenery_has_no_edge_at_the_room_border_and_shows_its_hue() {
        let blue = [(Aabb::new(Vec2::ZERO, Vec2::splat(40.0)), Some(Hue::Blue))];
        let size = Vec2::new(160.0, 120.0);
        let panels = Cells::new(Kind::Panels, &themes()[0], size, 40.0, &blue);
        // Outside the room is empty for scenery, unlike walls.
        assert_eq!(panels.mask(0, 0), TOP_RIGHT);
        assert_eq!(panels.mask(4, 3), 0);
        let walls = Cells::new(Kind::Walls, &themes()[0], size, 40.0, &blue);
        assert_eq!(walls.mask(0, 0), SOLID);
        // Pale glass takes its hue more strongly than stone.
        let (glass, stone) = (panels.tint(0, 0), walls.tint(0, 0));
        assert!(glass[2] / glass[0] > stone[2] / stone[0]);
    }

    #[test]
    fn the_committed_tileset_loads_as_layers() {
        assert_eq!(TILESETS.len(), themes().len(), "a tileset for every theme");
        assert_eq!(FEATURES.len(), themes().len(), "features for every theme");
        let sets = TILESETS
            .iter()
            .flatten()
            .map(|png| (png, LAYERS))
            .chain(FEATURES.iter().map(|png| (png, FEATURE_LAYERS)));
        for (png, layers) in sets {
            let image = tileset_image(png, layers).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(
                image.texture_descriptor.size.depth_or_array_layers,
                u32::from(layers)
            );
        }
        assert!(tileset_image(b"not a png", LAYERS).is_err());
    }
}
