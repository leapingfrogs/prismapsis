//! Draws walls and scenery from the theme's tilesets.
//!
//! Each is one quad per cell corner, picked by which of the four cells
//! around it are filled, drawn through a shader that holds back each hue
//! until it has been restored.

use super::features::{FEATURE_LAYERS, Feature, FeatureCells};
use super::filtered;
use super::paint::{Kind, Theme, linear, theme_index, themes};
use super::{BOTTOM_LEFT, BOTTOM_RIGHT, LAYERS, TOP_LEFT, TOP_RIGHT, layer, tint, variant_at};
use crate::level::{Campaign, CurrentLevel, LevelGeometry, Scenery, Solid};
use crate::light::{Hue, Phantom, Shade};
use crate::physics::{Aabb, Traits};
use crate::spectrum_view::Wash;
use bevy::asset::{RenderAssetUsages, embedded_asset, embedded_path};
use bevy::ecs::system::SystemParam;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
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
                ),
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
/// walls and panels, then its features, in campaign order.
#[derive(Resource, Debug)]
pub struct Tilesets(pub Vec<[Handle<WorldMaterial>; 3]>);

impl Tilesets {
    /// The material for `kind` in theme `theme`.
    fn material(&self, theme: usize, kind: Kind) -> Handle<WorldMaterial> {
        self.0[theme][kind as usize].clone()
    }

    /// The material for features in theme `theme`.
    fn features(&self, theme: usize) -> Handle<WorldMaterial> {
        self.0[theme][2].clone()
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
    let mut load = |name: &str, png: &[u8], layers: u16| {
        let tiles = match (images.as_mut(), tileset_image(png, layers)) {
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
        .map(|(&[walls, panels], features)| {
            [
                load(Kind::Walls.name(), walls, LAYERS),
                load(Kind::Panels.name(), panels, LAYERS),
                load("features", features, FEATURE_LAYERS),
            ]
        })
        .collect();
    commands.insert_resource(Tilesets(themes));
    let phantoms = load("phantoms", PHANTOMS, super::phantoms::PHANTOM_LAYERS);
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
    added: Query<(), (Added<Solid>, Without<Phantom>)>,
    walls: Query<(&Solid, Option<&Shade>, Option<&Traits>), Without<Phantom>>,
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
    added: Query<(), (Added<Solid>, Without<Phantom>)>,
    walls: Query<(&Solid, Option<&Shade>, &Traits), Without<Phantom>>,
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
