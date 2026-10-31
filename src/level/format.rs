//! The level file format.
//!
//! A level is a TOML file whose layout is drawn as an ASCII grid, with a
//! legend saying what each character places; see "Level format" in
//! `docs/design.md`. Parsing is pure, so the game, the `levels` tool and the
//! tests share it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use bevy::math::Vec2;
use serde::Deserialize;

use crate::enemy::EnemyTuning;
use crate::hero::HERO_RADIUS;
use crate::light::{Hue, LightTuning, Primary};
use crate::physics::Aabb;
use crate::powerup::PowerUp;

/// Size of one grid character in world units, unless a level sets `tile`.
pub const DEFAULT_TILE: f32 = 40.0;

/// A colour as written in a level file: a hue, or grey for things that never
/// colour in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tint {
    Grey,
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    White,
}

impl Tint {
    #[must_use]
    pub const fn hue(self) -> Option<Hue> {
        match self {
            Self::Grey => None,
            Self::Red => Some(Hue::Red),
            Self::Green => Some(Hue::Green),
            Self::Blue => Some(Hue::Blue),
            Self::Yellow => Some(Hue::Yellow),
            Self::Cyan => Some(Hue::Cyan),
            Self::Magenta => Some(Hue::Magenta),
            Self::White => Some(Hue::White),
        }
    }

    const fn primary(self) -> Option<Primary> {
        match self {
            Self::Red => Some(Primary::Red),
            Self::Green => Some(Primary::Green),
            Self::Blue => Some(Primary::Blue),
            _ => None,
        }
    }
}

/// What a grid character places.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tile {
    Empty,
    /// Solid, shaded by its hue; `height` makes it thinner than a tile.
    Wall {
        hue: Option<Hue>,
        height: Option<f32>,
    },
    /// Solid once its hue is restored.
    Phantom {
        hue: Hue,
        height: Option<f32>,
    },
    /// Background decoration with no collision.
    Scenery(Option<Hue>),
    Lens(Primary),
    /// A pickup that stays put, hidden until `hidden` is restored if set.
    Pickup {
        power: PowerUp,
        hidden: Option<Hue>,
    },
    Start,
    Exit,
    Checkpoint,
}

/// The legend every level starts from; a level's own legend adds to or
/// overrides it.
#[must_use]
pub fn default_legend() -> BTreeMap<char, Tile> {
    let mut legend = BTreeMap::from([
        (' ', Tile::Empty),
        ('.', Tile::Empty),
        (
            '#',
            Tile::Wall {
                hue: None,
                height: None,
            },
        ),
        ('@', Tile::Start),
        ('X', Tile::Exit),
        ('K', Tile::Checkpoint),
    ]);
    let hues = [
        ('R', Hue::Red),
        ('G', Hue::Green),
        ('B', Hue::Blue),
        ('Y', Hue::Yellow),
        ('C', Hue::Cyan),
        ('M', Hue::Magenta),
        ('W', Hue::White),
    ];
    for (ch, hue) in hues {
        let wall = Tile::Wall {
            hue: Some(hue),
            height: None,
        };
        let phantom = Tile::Phantom { hue, height: None };
        legend.insert(ch, wall);
        legend.insert(ch.to_ascii_lowercase(), phantom);
    }
    legend
}

/// A grid of characters, one per tile, read top row first. Short lines are
/// padded with empty space.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grid {
    rows: Vec<Vec<char>>,
    width: usize,
}

impl Grid {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let rows: Vec<Vec<char>> = text.lines().map(|line| line.chars().collect()).collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        Self { rows, width }
    }

    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> usize {
        self.rows.len()
    }

    /// The character at a cell; a space beyond the end of a line.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> char {
        self.rows
            .get(row)
            .and_then(|r| r.get(col))
            .copied()
            .unwrap_or(' ')
    }

    fn cells(&self) -> impl Iterator<Item = (usize, usize, char)> + '_ {
        (0..self.height())
            .flat_map(move |row| (0..self.width).map(move |col| (row, col, self.get(row, col))))
    }
}

/// A rectangle of grid cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    pub col: usize,
    pub row: usize,
    pub cols: usize,
    pub rows: usize,
}

/// Cover every `ch` in the grid with as few rectangles as a greedy sweep
/// finds: runs along each row, extended down while the next row matches the
/// whole run. Without `vertical`, each row's runs stay separate.
#[must_use]
pub fn merge_cells(grid: &Grid, ch: char, vertical: bool) -> Vec<CellRect> {
    let (width, height) = (grid.width(), grid.height());
    let mut used = vec![vec![false; width]; height];
    let mut rects = Vec::new();
    for row in 0..height {
        for col in 0..width {
            if used[row][col] || grid.get(row, col) != ch {
                continue;
            }
            let free = |used: &[Vec<bool>], r: usize, c: usize| !used[r][c] && grid.get(r, c) == ch;
            let mut end = col + 1;
            while end < width && free(&used, row, end) {
                end += 1;
            }
            let mut bottom = row + 1;
            while vertical && bottom < height && (col..end).all(|c| free(&used, bottom, c)) {
                bottom += 1;
            }
            for r in &mut used[row..bottom] {
                r[col..end].fill(true);
            }
            rects.push(CellRect {
                col,
                row,
                cols: end - col,
                rows: bottom - row,
            });
        }
    }
    rects
}

/// Optional light tuning for one level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightSettings {
    pub gauge_capacity: Option<f32>,
    pub mote_light: Option<f32>,
    pub lens_light: Option<f32>,
}

/// The level's mote waves.
#[derive(Debug, Clone, PartialEq)]
pub struct Waves {
    /// Colours of successive waves, repeating.
    pub colours: Vec<Primary>,
    pub interval: Option<f32>,
    pub size: Option<u32>,
}

impl Default for Waves {
    fn default() -> Self {
        Self {
            colours: Primary::ALL.to_vec(),
            interval: None,
            size: None,
        }
    }
}

/// A pickup placed in the level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedPickup {
    pub power: PowerUp,
    pub at: Vec2,
    pub hidden: Option<Hue>,
}

/// A parsed, valid level, in world units with the origin at the bottom left.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub name: String,
    pub tile: f32,
    pub size: Vec2,
    pub start: Vec2,
    pub walls: Vec<(Aabb, Option<Hue>)>,
    pub phantoms: Vec<(Aabb, Hue)>,
    pub scenery: Vec<(Aabb, Option<Hue>)>,
    pub exits: Vec<Aabb>,
    pub checkpoints: Vec<Aabb>,
    pub lenses: Vec<(Vec2, Primary)>,
    pub pickups: Vec<PlacedPickup>,
    pub waves: Waves,
    pub light: LightSettings,
    /// The layout as written, for tools that show it.
    pub map: Grid,
    pub background: Grid,
    pub legend: BTreeMap<char, Tile>,
}

impl Level {
    /// Set the tuning a level can override: to the level's value, or the
    /// default where it gives none, so one level's settings don't carry into
    /// the next. Other tuning is left as it is.
    pub fn apply_tuning(&self, enemy: &mut EnemyTuning, light: &mut LightTuning) {
        let (enemy_default, light_default) = (EnemyTuning::default(), LightTuning::default());
        enemy.wave_interval = self.waves.interval.unwrap_or(enemy_default.wave_interval);
        enemy.wave_size = self.waves.size.unwrap_or(enemy_default.wave_size);
        let settings = self.light;
        light.gauge_capacity = settings
            .gauge_capacity
            .unwrap_or(light_default.gauge_capacity);
        light.mote_light = settings.mote_light.unwrap_or(light_default.mote_light);
        light.lens_light = settings.lens_light.unwrap_or(light_default.lens_light);
    }

    /// What the character at a cell places, if it's in the legend.
    #[must_use]
    pub fn tile(&self, ch: char) -> Option<Tile> {
        self.legend.get(&ch).copied()
    }
}

/// Something wrong with a level file.
#[derive(Debug, Clone, PartialEq)]
pub enum LevelError {
    /// Not valid TOML, or a field of the wrong type.
    Syntax(String),
    LegendKey(String),
    LegendEntry {
        key: char,
        problem: String,
    },
    BadTile(f32),
    EmptyMap,
    UnknownChar {
        grid: &'static str,
        ch: char,
        row: usize,
        col: usize,
    },
    /// Only walls and scenery can go in the background.
    NotScenery {
        ch: char,
        row: usize,
        col: usize,
    },
    BackgroundTooBig,
    Starts(usize),
    NoExit,
    Waves(String),
    Light(String),
}

impl fmt::Display for LevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(message) => write!(f, "{}", message.trim_end()),
            Self::LegendKey(key) => write!(f, "legend key {key:?} must be one character"),
            Self::LegendEntry { key, problem } => write!(f, "legend {key:?}: {problem}"),
            Self::BadTile(tile) => write!(f, "tile must be a positive size, not {tile}"),
            Self::EmptyMap => write!(f, "the map is empty"),
            Self::UnknownChar { grid, ch, row, col } => write!(
                f,
                "{grid} row {}, column {}: {ch:?} is not in the legend",
                row + 1,
                col + 1
            ),
            Self::NotScenery { ch, row, col } => write!(
                f,
                "background row {}, column {}: {ch:?} is not a wall or scenery",
                row + 1,
                col + 1
            ),
            Self::BackgroundTooBig => write!(f, "the background is bigger than the map"),
            Self::Starts(0) => write!(f, "the map has no start"),
            Self::Starts(count) => write!(f, "the map has {count} starts; it needs one"),
            Self::NoExit => write!(f, "the map has no exit"),
            Self::Waves(problem) => write!(f, "waves: {problem}"),
            Self::Light(problem) => write!(f, "light: {problem}"),
        }
    }
}

impl std::error::Error for LevelError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LevelFile {
    name: String,
    tile: Option<f32>,
    map: String,
    background: Option<String>,
    #[serde(default)]
    legend: BTreeMap<String, toml::Value>,
    #[serde(default)]
    waves: WavesFile,
    #[serde(default)]
    light: LightSettings,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WavesFile {
    colours: Option<Vec<Tint>>,
    interval: Option<f32>,
    size: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Marker {
    Empty,
    Start,
    Exit,
    Checkpoint,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TileFile {
    wall: Option<Tint>,
    phantom: Option<Tint>,
    scenery: Option<Tint>,
    lens: Option<Tint>,
    pickup: Option<PowerUp>,
    hidden: Option<Tint>,
    height: Option<f32>,
}

/// Turn one legend entry into a tile.
fn legend_tile(value: toml::Value, tile_size: f32) -> Result<Tile, String> {
    if value.is_str() {
        let marker: Marker = value.try_into().map_err(|e| e.to_string())?;
        return Ok(match marker {
            Marker::Empty => Tile::Empty,
            Marker::Start => Tile::Start,
            Marker::Exit => Tile::Exit,
            Marker::Checkpoint => Tile::Checkpoint,
        });
    }
    let spec: TileFile = value.try_into().map_err(|e| e.to_string())?;
    let kinds = [
        spec.wall.is_some(),
        spec.phantom.is_some(),
        spec.scenery.is_some(),
        spec.lens.is_some(),
        spec.pickup.is_some(),
    ];
    if kinds.iter().filter(|&&k| k).count() != 1 {
        return Err("give exactly one of wall, phantom, scenery, lens or pickup".to_owned());
    }
    if spec.hidden.is_some() && spec.pickup.is_none() {
        return Err("only a pickup can be hidden".to_owned());
    }
    if spec.height.is_some() && spec.wall.is_none() && spec.phantom.is_none() {
        return Err("only walls and phantoms have a height".to_owned());
    }
    if let Some(height) = spec.height
        && !(height > 0.0 && height <= tile_size)
    {
        return Err(format!(
            "height must be above 0 and at most the tile size ({tile_size})"
        ));
    }
    let coloured = |tint: Tint| {
        tint.hue()
            .ok_or_else(|| "needs a colour, not grey".to_owned())
    };
    Ok(if let Some(tint) = spec.wall {
        Tile::Wall {
            hue: tint.hue(),
            height: spec.height,
        }
    } else if let Some(tint) = spec.phantom {
        Tile::Phantom {
            hue: coloured(tint)?,
            height: spec.height,
        }
    } else if let Some(tint) = spec.scenery {
        Tile::Scenery(tint.hue())
    } else if let Some(tint) = spec.lens {
        Tile::Lens(
            tint.primary()
                .ok_or_else(|| "a lens must be red, green or blue".to_owned())?,
        )
    } else if let Some(power) = spec.pickup {
        let hidden = spec.hidden.map(coloured).transpose()?;
        Tile::Pickup { power, hidden }
    } else {
        unreachable!("exactly one kind is set")
    })
}

/// Grid placement in world units: rows count down from the top.
struct Layout {
    tile: f32,
    rows: usize,
}

impl Layout {
    // Grid sizes are far below f32's exact-integer limit.
    #[allow(clippy::cast_precision_loss)]
    fn aabb(&self, rect: CellRect, height: Option<f32>) -> Aabb {
        let top = (self.rows - rect.row) as f32 * self.tile;
        let bottom = height.map_or((self.rows - rect.row - rect.rows) as f32 * self.tile, |h| {
            top - h
        });
        Aabb::new(
            Vec2::new(rect.col as f32 * self.tile, bottom),
            Vec2::new((rect.col + rect.cols) as f32 * self.tile, top),
        )
    }

    fn centre(&self, row: usize, col: usize) -> Vec2 {
        self.aabb(
            CellRect {
                col,
                row,
                cols: 1,
                rows: 1,
            },
            None,
        )
        .center()
    }
}

fn positive(
    errors: &mut Vec<LevelError>,
    wrap: fn(String) -> LevelError,
    name: &str,
    value: Option<f32>,
) {
    if let Some(value) = value
        && !(value > 0.0 && value.is_finite())
    {
        errors.push(wrap(format!("{name} must be positive, not {value}")));
    }
}

fn read_legend(
    entries: BTreeMap<String, toml::Value>,
    tile: f32,
    errors: &mut Vec<LevelError>,
) -> BTreeMap<char, Tile> {
    let mut legend = default_legend();
    for (key, value) in entries {
        let mut chars = key.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            errors.push(LevelError::LegendKey(key));
            continue;
        };
        match legend_tile(value, tile) {
            Ok(entry) => {
                legend.insert(ch, entry);
            }
            Err(problem) => errors.push(LevelError::LegendEntry { key: ch, problem }),
        }
    }
    legend
}

fn read_waves(file: WavesFile, errors: &mut Vec<LevelError>) -> Waves {
    let tints = file
        .colours
        .unwrap_or_else(|| vec![Tint::Red, Tint::Green, Tint::Blue]);
    let mut colours = Vec::new();
    let mut bad_colour = false;
    for tint in tints {
        if let Some(primary) = tint.primary() {
            colours.push(primary);
        } else {
            bad_colour = true;
            errors.push(LevelError::Waves(format!(
                "wave colours must be red, green or blue, not {tint:?}"
            )));
        }
    }
    if colours.is_empty() && !bad_colour {
        errors.push(LevelError::Waves(
            "give at least one wave colour".to_owned(),
        ));
    }
    positive(errors, LevelError::Waves, "interval", file.interval);
    if file.size == Some(0) {
        errors.push(LevelError::Waves("size must be at least 1".to_owned()));
    }
    Waves {
        colours,
        interval: file.interval,
        size: file.size,
    }
}

fn check_light(light: &LightSettings, errors: &mut Vec<LevelError>) {
    for (name, value) in [
        ("gauge_capacity", light.gauge_capacity),
        ("mote_light", light.mote_light),
        ("lens_light", light.lens_light),
    ] {
        positive(errors, LevelError::Light, name, value);
    }
}

/// Things placed at single cells, and which characters the background uses.
#[derive(Default)]
struct Placements {
    starts: Vec<Vec2>,
    lenses: Vec<(Vec2, Primary)>,
    pickups: Vec<PlacedPickup>,
    background_chars: BTreeSet<char>,
}

fn place(
    legend: &BTreeMap<char, Tile>,
    map: &Grid,
    background: &Grid,
    layout: &Layout,
    errors: &mut Vec<LevelError>,
) -> Placements {
    // Report each unknown character once, where it first appears.
    let mut unknown = BTreeSet::new();
    let mut lookup = |errors: &mut Vec<LevelError>, grid: &'static str, row, col, ch| {
        let tile = legend.get(&ch).copied();
        if tile.is_none() && unknown.insert((grid, ch)) {
            errors.push(LevelError::UnknownChar { grid, ch, row, col });
        }
        tile
    };
    let mut found = Placements::default();
    for (row, col, ch) in map.cells() {
        let at = layout.centre(row, col);
        match lookup(errors, "map", row, col, ch) {
            Some(Tile::Start) => {
                // The hero is bigger than a tile, so lift it clear of the floor.
                let lifted = at.y - layout.tile / 2.0 + HERO_RADIUS;
                found.starts.push(Vec2::new(at.x, at.y.max(lifted)));
            }
            Some(Tile::Lens(colour)) => found.lenses.push((at, colour)),
            Some(Tile::Pickup { power, hidden }) => {
                found.pickups.push(PlacedPickup { power, at, hidden });
            }
            _ => {}
        }
    }
    for (row, col, ch) in background.cells() {
        match lookup(errors, "background", row, col, ch) {
            None | Some(Tile::Empty) => {}
            Some(Tile::Wall { .. } | Tile::Scenery(_)) => {
                found.background_chars.insert(ch);
            }
            Some(_) => errors.push(LevelError::NotScenery { ch, row, col }),
        }
    }
    found
}

/// Areas covered by runs of the same character, merged into rectangles.
#[derive(Default)]
struct Areas {
    walls: Vec<(Aabb, Option<Hue>)>,
    phantoms: Vec<(Aabb, Hue)>,
    scenery: Vec<(Aabb, Option<Hue>)>,
    exits: Vec<Aabb>,
    checkpoints: Vec<Aabb>,
}

fn merge_areas(
    legend: &BTreeMap<char, Tile>,
    map: &Grid,
    background: &Grid,
    background_chars: &BTreeSet<char>,
    layout: &Layout,
) -> Areas {
    let mut areas = Areas::default();
    for (&ch, &entry) in legend {
        // Thin tiles only merge along their row.
        let (vertical, height) = match entry {
            Tile::Wall { height, .. } | Tile::Phantom { height, .. } => (height.is_none(), height),
            _ => (true, None),
        };
        let rects = || {
            merge_cells(map, ch, vertical)
                .into_iter()
                .map(|r| layout.aabb(r, height))
        };
        match entry {
            Tile::Wall { hue, .. } => areas.walls.extend(rects().map(|a| (a, hue))),
            Tile::Phantom { hue, .. } => areas.phantoms.extend(rects().map(|a| (a, hue))),
            Tile::Scenery(hue) => areas.scenery.extend(rects().map(|a| (a, hue))),
            Tile::Exit => areas.exits.extend(rects()),
            Tile::Checkpoint => areas.checkpoints.extend(rects()),
            _ => {}
        }
        if background_chars.contains(&ch)
            && let Tile::Wall { hue, .. } | Tile::Scenery(hue) = entry
        {
            let rects = merge_cells(background, ch, true);
            areas
                .scenery
                .extend(rects.into_iter().map(|r| (layout.aabb(r, None), hue)));
        }
    }
    areas
}

/// Parse and check a level file, reporting every problem found.
///
/// # Errors
///
/// Returns the problems if the file isn't a valid level.
pub fn parse(source: &str) -> Result<Level, Vec<LevelError>> {
    let file: LevelFile =
        toml::from_str(source).map_err(|e| vec![LevelError::Syntax(e.to_string())])?;
    let mut errors = Vec::new();

    let tile = file.tile.unwrap_or(DEFAULT_TILE);
    let tile = if tile > 0.0 && tile.is_finite() {
        tile
    } else {
        errors.push(LevelError::BadTile(tile));
        DEFAULT_TILE
    };
    let legend = read_legend(file.legend, tile, &mut errors);
    let waves = read_waves(file.waves, &mut errors);
    check_light(&file.light, &mut errors);

    let map = Grid::parse(&file.map);
    let background = Grid::parse(file.background.as_deref().unwrap_or_default());
    if map.width() == 0 || map.height() == 0 {
        errors.push(LevelError::EmptyMap);
        return Err(errors);
    }
    if background.width() > map.width() || background.height() > map.height() {
        errors.push(LevelError::BackgroundTooBig);
    }
    let layout = Layout {
        tile,
        rows: map.height(),
    };
    let placed = place(&legend, &map, &background, &layout, &mut errors);
    let areas = merge_areas(
        &legend,
        &map,
        &background,
        &placed.background_chars,
        &layout,
    );

    if placed.starts.len() != 1 {
        errors.push(LevelError::Starts(placed.starts.len()));
    }
    if areas.exits.is_empty() {
        errors.push(LevelError::NoExit);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // Grid sizes are far below f32's exact-integer limit.
    #[allow(clippy::cast_precision_loss)]
    let size = Vec2::new(map.width() as f32, map.height() as f32) * tile;
    Ok(Level {
        name: file.name,
        tile,
        size,
        start: placed.starts[0],
        walls: areas.walls,
        phantoms: areas.phantoms,
        scenery: areas.scenery,
        exits: areas.exits,
        checkpoints: areas.checkpoints,
        lenses: placed.lenses,
        pickups: placed.pickups,
        waves,
        light: file.light,
        map,
        background,
        legend,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(map: &str, extra: &str) -> Result<Level, Vec<LevelError>> {
        parse(&format!("name = \"Test\"\nmap = '''\n{map}'''\n{extra}"))
    }

    const BOX: &str = "\
######
#@  X#
######
";

    #[test]
    fn grids_pad_short_lines() {
        let grid = Grid::parse("ab\nc\n");
        assert_eq!((grid.width(), grid.height()), (2, 2));
        assert_eq!(grid.get(1, 1), ' ');
        assert_eq!(grid.get(5, 5), ' ');
    }

    #[test]
    fn merging_covers_runs_and_extends_down() {
        let grid = Grid::parse("##.\n##.\n#..\n");
        let rects = merge_cells(&grid, '#', true);
        assert_eq!(
            rects,
            vec![
                CellRect {
                    col: 0,
                    row: 0,
                    cols: 2,
                    rows: 2
                },
                CellRect {
                    col: 0,
                    row: 2,
                    cols: 1,
                    rows: 1
                },
            ]
        );
        assert_eq!(merge_cells(&grid, '#', false).len(), 3);
    }

    #[test]
    fn a_small_level_lays_out_bottom_up() {
        let level = level(BOX, "").unwrap();
        assert_eq!(level.size, Vec2::new(240.0, 120.0));
        assert_eq!(level.start, Vec2::new(60.0, 40.0 + HERO_RADIUS));
        assert_eq!(
            level.exits,
            vec![Aabb::new(Vec2::new(160.0, 40.0), Vec2::new(200.0, 80.0))]
        );
        // Top row, then the two side walls, then the bottom row.
        assert_eq!(level.walls.len(), 4);
        assert!(level.walls.iter().all(|(_, hue)| hue.is_none()));
        assert_eq!(level.waves, Waves::default());
    }

    #[test]
    fn default_legend_colours_walls_and_phantoms() {
        let level = level("RRRRRR\nR@ g X\nRRRRRR\n", "").unwrap();
        assert!(level.walls.iter().all(|(_, hue)| *hue == Some(Hue::Red)));
        assert_eq!(level.phantoms.len(), 1);
        assert_eq!(level.phantoms[0].1, Hue::Green);
    }

    #[test]
    fn legend_entries_place_things() {
        let level = level(
            "######\n#@lpX#\n#=  K#\n######\n",
            r#"background = '''
 ss
'''
[legend]
"l" = { lens = "blue" }
"p" = { pickup = "spread", hidden = "cyan" }
"=" = { wall = "yellow", height = 10 }
"s" = { scenery = "magenta" }
[waves]
colours = ["blue"]
interval = 4
[light]
gauge_capacity = 5
"#,
        )
        .unwrap();
        assert_eq!(level.lenses, vec![(Vec2::new(100.0, 100.0), Primary::Blue)]);
        assert_eq!(
            level.pickups,
            vec![PlacedPickup {
                power: PowerUp::Spread,
                at: Vec2::new(140.0, 100.0),
                hidden: Some(Hue::Cyan),
            }]
        );
        let thin = level
            .walls
            .iter()
            .find(|(_, hue)| *hue == Some(Hue::Yellow))
            .unwrap()
            .0;
        assert_eq!(
            thin,
            Aabb::new(Vec2::new(40.0, 70.0), Vec2::new(80.0, 80.0))
        );
        assert_eq!(
            level.scenery,
            vec![(
                Aabb::new(Vec2::new(40.0, 120.0), Vec2::new(120.0, 160.0)),
                Some(Hue::Magenta)
            )]
        );
        assert_eq!(level.checkpoints.len(), 1);

        let mut enemy = EnemyTuning {
            wave_size: 9,
            ..EnemyTuning::default()
        };
        let mut light = LightTuning::default();
        level.apply_tuning(&mut enemy, &mut light);
        assert!((enemy.wave_interval - 4.0).abs() < f32::EPSILON);
        // Unset overrides go back to the default.
        assert_eq!(enemy.wave_size, EnemyTuning::default().wave_size);
        assert!((light.gauge_capacity - 5.0).abs() < f32::EPSILON);
        assert_eq!(level.waves.colours, vec![Primary::Blue]);
    }

    #[test]
    fn background_walls_become_scenery() {
        let level = level(BOX, "background = '''\nGG\n'''\n").unwrap();
        assert_eq!(level.scenery.len(), 1);
        assert_eq!(level.scenery[0].1, Some(Hue::Green));
    }

    fn problems(map: &str, extra: &str) -> Vec<String> {
        level(map, extra)
            .unwrap_err()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn layout_problems_are_all_reported() {
        assert_eq!(
            problems("#?#\n#?#\n", ""),
            vec![
                "map row 1, column 2: '?' is not in the legend",
                "the map has no start",
                "the map has no exit",
            ]
        );
        assert_eq!(
            problems("@@X\n", ""),
            vec!["the map has 2 starts; it needs one"]
        );
        assert_eq!(problems("", ""), vec!["the map is empty"]);
        assert_eq!(
            problems(BOX, "background = '''\n X\n'''\n"),
            vec!["background row 1, column 2: 'X' is not a wall or scenery"]
        );
        assert_eq!(
            problems(BOX, "background = '''\n#######\n'''\n"),
            vec!["the background is bigger than the map"]
        );
    }

    #[test]
    fn legend_and_setting_problems_are_reported() {
        let extra = r#"tile = -1
[legend]
"ab" = "start"
"l" = { lens = "yellow" }
"w" = { wall = "red", phantom = "red" }
"h" = { pickup = "spread", height = 5 }
"x" = { lens = "red", hidden = "red" }
"t" = { wall = "red", height = 50 }
"g" = { phantom = "grey" }
"m" = "nowhere"
[waves]
colours = ["cyan"]
interval = 0
size = 0
[light]
mote_light = -1
"#;
        let found = problems(BOX, extra);
        let expected = [
            "tile must be a positive size",
            "legend key \"ab\"",
            "legend 'l': a lens must be red, green or blue",
            "legend 'w': give exactly one",
            "legend 'h': only walls and phantoms have a height",
            "legend 'x': only a pickup can be hidden",
            "legend 't': height must be above 0",
            "legend 'g': needs a colour, not grey",
            "legend 'm': unknown variant",
            "waves: wave colours must be red, green or blue",
            "waves: interval must be positive",
            "waves: size must be at least 1",
            "light: mote_light must be positive",
        ];
        for text in expected {
            assert!(
                found.iter().any(|f| f.starts_with(text)),
                "missing {text:?} in {found:#?}"
            );
        }
        assert_eq!(found.len(), expected.len(), "{found:#?}");
    }

    #[test]
    fn syntax_errors_and_empty_wave_lists_are_reported() {
        assert!(problems(BOX, "name = 1")[0].contains("duplicate key"));
        assert_eq!(
            problems(BOX, "[waves]\ncolours = []\n"),
            vec!["waves: give at least one wave colour"]
        );
    }
}
