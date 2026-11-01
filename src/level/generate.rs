//! Generating levels from a seed.
//!
//! A generated level file keeps the settings it was made from in its
//! `[generated]` table, and `just levelgen` remakes the rest of the file
//! from them; see "Level generation" in `docs/design.md`.
//!
//! The route comes first. A wide level runs over rolling ground, and each
//! colour gate is a wall too high to bounce over, with two phantom steps of
//! the gate's colour in front of it. A tall level climbs platforms on
//! alternating sides, and each gate is a phantom platform the climb can't
//! skip. Lenses of a gate's colour sit before it, a checkpoint follows it,
//! and a pickup hidden until the gate's colour returns sits nearby.
//!
//! Every level is checked before it's written: it must be finishable with
//! every checkpoint in reach, and taking away any gate's phantoms must make
//! it unfinishable, so no gate can be bypassed. A layout that fails is
//! thrown away and the next one tried, in an order fixed by the seed.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::Deserialize;

use super::format::{Tint, parse};
use super::solve::{Limits, solve_within};
use crate::light::{Hue, Primary};
use crate::powerup::PowerUp;

/// Layouts tried for one level before giving up.
const ATTEMPTS: u64 = 40;

/// Rows in a wide level, including its border: one view tall.
const WIDE_ROWS: usize = 27;

/// Tiles between the tops of a tall level's platforms: a comfortable
/// fraction of the bounce.
const CLIMB: usize = 5;

/// Pickups a generated level can place. Anti-gravity would float past the
/// gates, so it only comes from drops.
const PICKUPS: [PowerUp; 4] = [
    PowerUp::RearShot,
    PowerUp::Spread,
    PowerUp::RapidFire,
    PowerUp::Piercing,
];

/// Legend characters for pickups, assigned in order of use.
const PICKUP_CHARS: &str = "456789";

/// A generated level's overall shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    /// One view tall, running sideways.
    Wide,
    /// Narrow, climbing upwards.
    Tall,
}

/// The settings a level is generated from.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Recipe {
    pub seed: u64,
    pub shape: Shape,
    /// The colour of each gate along the route, in order.
    #[serde(default)]
    pub gates: Vec<Tint>,
    /// Lenses before each gate.
    #[serde(default = "one")]
    pub lenses: usize,
    /// Seconds between mote waves, if not the default.
    pub wave_interval: Option<f32>,
    /// The layout attempt that made the file, which the generator records.
    pub attempt: Option<u64>,
}

const fn one() -> usize {
    1
}

const fn letter(hue: Hue) -> char {
    match hue {
        Hue::Red => 'R',
        Hue::Green => 'G',
        Hue::Blue => 'B',
        Hue::Yellow => 'Y',
        Hue::Cyan => 'C',
        Hue::Magenta => 'M',
        Hue::White => 'W',
    }
}

const fn hue_name(hue: Hue) -> &'static str {
    match hue {
        Hue::Red => "red",
        Hue::Green => "green",
        Hue::Blue => "blue",
        Hue::Yellow => "yellow",
        Hue::Cyan => "cyan",
        Hue::Magenta => "magenta",
        Hue::White => "white",
    }
}

const fn power_name(power: PowerUp) -> &'static str {
    match power {
        PowerUp::AntiGravity => "anti-gravity",
        PowerUp::RearShot => "rear-shot",
        PowerUp::Spread => "spread",
        PowerUp::RapidFire => "rapid-fire",
        PowerUp::Piercing => "piercing",
    }
}

/// Small, fast random numbers (xorshift64*), the same for a seed on every
/// machine.
struct Rng(u64);

/// Scramble the bits of `z` (the `SplitMix64` finaliser), so nearby seeds
/// give unrelated numbers.
const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The parts of a level that each draw their own random numbers, so that
/// changing how one is made leaves the others as they were.
#[derive(Debug, Clone, Copy)]
enum Stage {
    Layout = 1,
    Terrain,
    Ledges,
    Lenses,
    Pickups,
    Decoration,
}

impl Rng {
    const fn new(seed: u64, attempt: u64, stage: Stage) -> Self {
        // xorshift needs a non-zero state.
        Self(mix(seed ^ mix(attempt ^ mix(stage as u64))) | 1)
    }

    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number from `low` to `high`, both included.
    const fn range(&mut self, low: usize, high: usize) -> usize {
        let span = (high.saturating_sub(low) + 1) as u64;
        // Below `span`, which fits in usize.
        #[allow(clippy::cast_possible_truncation)]
        let offset = (self.next() % span) as usize;
        low + offset
    }

    const fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.range(0, items.len() - 1)]
    }

    /// True `percent` times in a hundred.
    const fn chance(&mut self, percent: usize) -> bool {
        self.range(0, 99) < percent
    }
}

/// Random numbers for each stage of one layout attempt.
struct Streams {
    layout: Rng,
    terrain: Rng,
    ledges: Rng,
    lenses: Rng,
    pickups: Rng,
    decoration: Rng,
}

impl Streams {
    const fn new(seed: u64, attempt: u64) -> Self {
        Self {
            layout: Rng::new(seed, attempt, Stage::Layout),
            terrain: Rng::new(seed, attempt, Stage::Terrain),
            ledges: Rng::new(seed, attempt, Stage::Ledges),
            lenses: Rng::new(seed, attempt, Stage::Lenses),
            pickups: Rng::new(seed, attempt, Stage::Pickups),
            decoration: Rng::new(seed, attempt, Stage::Decoration),
        }
    }
}

/// A level being laid out. Heights count tiles up from the bottom border:
/// height 0 is the first open row.
struct Draft {
    cols: usize,
    rows: usize,
    map: Vec<Vec<char>>,
    background: Vec<Vec<char>>,
    legend: BTreeMap<char, String>,
}

impl Draft {
    fn new(cols: usize, rows: usize) -> Self {
        let mut map = vec![vec![' '; cols]; rows];
        for (r, row) in map.iter_mut().enumerate() {
            if r == 0 || r == rows - 1 {
                row.fill('#');
            }
            row[0] = '#';
            row[cols - 1] = '#';
        }
        Self {
            cols,
            rows,
            map,
            background: vec![vec![' '; cols]; rows],
            legend: BTreeMap::new(),
        }
    }

    /// The highest height inside the border.
    const fn top(&self) -> usize {
        self.rows - 3
    }

    const fn row(&self, height: usize) -> usize {
        self.rows - 2 - height
    }

    fn get(&self, col: usize, height: usize) -> char {
        self.map[self.row(height)][col]
    }

    fn set(&mut self, col: usize, height: usize, ch: char) {
        let row = self.row(height);
        self.map[row][col] = ch;
    }

    /// Fill columns `cols` from height `low` up to `high`, both included.
    fn fill(&mut self, cols: std::ops::RangeInclusive<usize>, low: usize, high: usize, ch: char) {
        for col in cols {
            for height in low..=high {
                self.set(col, height, ch);
            }
        }
    }

    /// Whether the cells around `col`, `height` are all open.
    fn clear_around(&self, col: usize, height: usize) -> bool {
        (col.saturating_sub(1)..=(col + 1).min(self.cols - 1)).all(|c| {
            (height.saturating_sub(1)..=(height + 1).min(self.top())).all(|h| self.get(c, h) == ' ')
        })
    }

    fn lens(&mut self, col: usize, height: usize, colour: Primary) {
        let ch = char::from(b'1' + u8::try_from(colour.index()).unwrap_or_default());
        self.legend
            .insert(ch, format!("{{ lens = \"{}\" }}", hue_name(colour.into())));
        self.set(col, height, ch);
    }

    fn pickup(&mut self, col: usize, height: usize, power: PowerUp, hidden: Hue) {
        let entry = format!(
            "{{ pickup = \"{}\", hidden = \"{}\" }}",
            power_name(power),
            hue_name(hidden)
        );
        let ch = self
            .legend
            .iter()
            .find(|&(_, e)| *e == entry)
            .map(|(&ch, _)| ch)
            .or_else(|| PICKUP_CHARS.chars().find(|c| !self.legend.contains_key(c)));
        if let Some(ch) = ch {
            self.legend.insert(ch, entry);
            self.set(col, height, ch);
        }
    }

    /// Background shapes in random hues, behind the open space.
    fn decorate(&mut self, rng: &mut Rng) {
        let mut col = 2;
        while col + 6 < self.cols {
            if rng.chance(60) {
                let width = rng.range(1, 4);
                let tall = rng.range(2, 7);
                let low = rng.range(1, self.top().saturating_sub(tall));
                let hue = rng.pick(&[
                    Hue::Red,
                    Hue::Green,
                    Hue::Blue,
                    Hue::Yellow,
                    Hue::Cyan,
                    Hue::Magenta,
                ]);
                for c in col..col + width {
                    for h in low..low + tall {
                        let row = self.row(h);
                        self.background[row][c] = letter(hue);
                    }
                }
            }
            col += rng.range(5, 10);
        }
    }

    fn grid_text(grid: &[Vec<char>]) -> String {
        grid.iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_owned() + "\n")
            .collect()
    }

    /// The level file: the layout, then the settings it came from.
    fn text(&self, name: &str, recipe: &Recipe) -> String {
        let mut out = String::from(
            "# Generated by `just levelgen` from the [generated] settings below.\n\
             # Change those and regenerate rather than editing this file.\n",
        );
        let _ = writeln!(out, "name = {name:?}");
        let _ = write!(out, "map = '''\n{}'''\n", Self::grid_text(&self.map));
        let _ = write!(
            out,
            "background = '''\n{}'''\n",
            Self::grid_text(&self.background)
        );
        out.push_str("\n[generated]\n");
        let _ = writeln!(out, "seed = {}", recipe.seed);
        let shape = match recipe.shape {
            Shape::Wide => "wide",
            Shape::Tall => "tall",
        };
        let _ = writeln!(out, "shape = \"{shape}\"");
        let gates: Vec<String> = gate_hues(recipe)
            .unwrap_or_default()
            .into_iter()
            .map(|hue| format!("\"{}\"", hue_name(hue)))
            .collect();
        let _ = writeln!(out, "gates = [{}]", gates.join(", "));
        let _ = writeln!(out, "lenses = {}", recipe.lenses);
        if let Some(interval) = recipe.wave_interval {
            let _ = writeln!(out, "wave-interval = {interval:?}");
        }
        if let Some(attempt) = recipe.attempt {
            let _ = writeln!(out, "attempt = {attempt}");
        }
        if let Some(interval) = recipe.wave_interval {
            let _ = write!(out, "\n[waves]\ninterval = {interval:?}\n");
        }
        if !self.legend.is_empty() {
            out.push_str("\n[legend]\n");
            for (ch, entry) in &self.legend {
                let _ = writeln!(out, "\"{ch}\" = {entry}");
            }
        }
        out
    }

    /// This draft with every phantom of `hue` taken away.
    fn without(&self, hue: Hue) -> Self {
        let phantom = letter(hue).to_ascii_lowercase();
        let map = self
            .map
            .iter()
            .map(|row| {
                row.iter()
                    .map(|&ch| if ch == phantom { ' ' } else { ch })
                    .collect()
            })
            .collect();
        Self {
            map,
            background: self.background.clone(),
            legend: self.legend.clone(),
            ..*self
        }
    }
}

/// The gates' hues, if they're all colours a gate can be.
fn gate_hues(recipe: &Recipe) -> Result<Vec<Hue>, String> {
    recipe
        .gates
        .iter()
        .map(|tint| match tint.hue() {
            Some(Hue::White) | None => {
                Err("gates must be red, green, blue, yellow, cyan or magenta".to_owned())
            }
            Some(hue) => Ok(hue),
        })
        .collect()
}

/// Lenses for a gate, cycling through its primaries.
fn place_lenses(
    draft: &mut Draft,
    rng: &mut Rng,
    gate: Hue,
    count: usize,
    spots: &mut dyn FnMut(&mut Rng) -> (usize, usize),
) {
    let primaries = gate.primaries();
    let mut placed = 0;
    for _ in 0..count * 20 {
        if placed == count {
            break;
        }
        let (col, height) = spots(rng);
        if col > 0
            && col < draft.cols - 1
            && height <= draft.top()
            && draft.clear_around(col, height)
        {
            draft.lens(col, height, primaries[placed % primaries.len()]);
            placed += 1;
        }
    }
}

/// Highest the ground of a wide level rises, in tiles.
const MAX_GROUND: usize = 5;

/// Ground heights for `len` columns starting level with `height`, built
/// from short patterns that give the run a rhythm: flat stretches, stairs,
/// hops between pillars, pits that widen, and alternating steps. No step
/// is more than two tiles.
fn terrain(len: usize, mut height: usize, rng: &mut Rng) -> Vec<usize> {
    let mut out = Vec::with_capacity(len + 12);
    let push = |out: &mut Vec<usize>, height: usize, count: usize| {
        out.extend(std::iter::repeat_n(height, count));
    };
    while out.len() < len {
        match rng.range(0, 4) {
            // A flat stretch.
            0 => push(&mut out, height, rng.range(3, 6)),
            // Stairs, climbing or descending a tile at a time.
            1 => {
                let up = height < 2 || (height < MAX_GROUND - 1 && rng.chance(50));
                let width = rng.range(2, 3);
                for _ in 0..rng.range(2, 3) {
                    height = if up {
                        (height + 1).min(MAX_GROUND)
                    } else {
                        height.saturating_sub(1)
                    };
                    push(&mut out, height, width);
                }
            }
            // Hops between pillars standing up from the ground.
            2 => {
                let top = (height + 2).min(MAX_GROUND);
                let gap = rng.range(2, 3);
                for _ in 0..3 {
                    push(&mut out, height, gap);
                    push(&mut out, top, 2);
                }
                push(&mut out, height, gap);
            }
            // Pits that widen: two, three, then four tiles.
            3 => {
                if height < 2 {
                    height += 2;
                    push(&mut out, height, 2);
                }
                for width in 2..=4 {
                    push(&mut out, height, 2);
                    push(&mut out, height - 2, width);
                }
                push(&mut out, height, 2);
            }
            // Steps alternating high and low.
            _ => {
                let high = (height + 2).min(MAX_GROUND);
                let width = rng.range(2, 3);
                for _ in 0..2 {
                    push(&mut out, high, width);
                    push(&mut out, height, width);
                }
            }
        }
    }
    out.truncate(len);
    out
}

/// A one-view-tall level over patterned ground, each gate a high wall with
/// two phantom steps before it.
fn wide(recipe: &Recipe, gates: &[Hue], rngs: &mut Streams) -> Draft {
    let lengths: Vec<usize> = (0..=gates.len())
        .map(|i| {
            let base = if i < gates.len() {
                28 + 3 * recipe.lenses
            } else {
                20
            };
            base + rngs.layout.range(0, 8)
        })
        .collect();
    let cols = 2 + lengths.iter().sum::<usize>();
    let mut draft = Draft::new(cols, WIDE_ROWS);

    // Ground heights, column by column: flat where the start, gates and
    // exit need it, patterned between.
    let mut ground = vec![0; cols];
    let mut segments = Vec::new();
    let mut start = 1;
    for (i, &length) in lengths.iter().enumerate() {
        let end = start + length;
        let head = if i == 0 { 6 } else { 4 };
        let tail = if i < gates.len() { 13 } else { 7 };
        let entry = if i == 0 { 0 } else { rngs.terrain.range(0, 2) };
        let middle = terrain(length - head - tail, entry, &mut rngs.terrain);
        let exit = middle.last().copied().unwrap_or(entry);
        let heights = std::iter::repeat_n(entry, head)
            .chain(middle)
            .chain(std::iter::repeat_n(exit, tail));
        for (g, height) in ground[start..end].iter_mut().zip(heights) {
            *g = height;
        }
        segments.push((start, end));
        start = end;
    }
    for (col, &height) in ground.iter().enumerate().take(cols - 1).skip(1) {
        if height > 0 {
            draft.fill(col..=col, 0, height - 1, '#');
        }
    }

    for (i, &(start, end)) in segments.iter().enumerate() {
        let gate = gates.get(i).copied();
        // Ledges, kept well away from a gate so they can't lift the hero
        // over it.
        let rng = &mut rngs.ledges;
        let ledge_end = end - if gate.is_some() { 17 } else { 9 };
        let mut first_ledge = None;
        for _ in 0..rng.range(1, 2) {
            let width = rng.range(3, 5);
            if start + 3 + width >= ledge_end {
                break;
            }
            let left = rng.range(start + 3, ledge_end - width);
            let floor = (left - 1..=left + width)
                .map(|c| ground[c])
                .max()
                .unwrap_or(0);
            let height = floor + rng.range(4, 5);
            let free = (left - 1..=left + width)
                .all(|c| (height - 1..=height + 2).all(|h| draft.get(c, h) == ' '));
            if !free {
                continue;
            }
            let ch = if rng.chance(60) {
                '#'
            } else {
                letter(rng.pick(&Hue::ALL[..6]))
            };
            draft.fill(left..=left + width - 1, height, height, ch);
            first_ledge.get_or_insert((left + width / 2, height + 1));
        }

        if let Some(gate) = gate {
            let floor = ground[end - 1];
            // The wall's top is 12 tiles up: out of a bounce's reach. The
            // steps are 4 and 8 tiles up.
            draft.fill(end - 2..=end - 1, floor, floor + 11, letter(gate));
            let phantom = letter(gate).to_ascii_lowercase();
            draft.fill(end - 11..=end - 9, floor + 3, floor + 3, phantom);
            draft.fill(end - 6..=end - 4, floor + 7, floor + 7, phantom);

            // On a ledge if there is one, otherwise just above the ground.
            let (col, height) = first_ledge.unwrap_or((start + 5, ground[start + 5] + 1));
            draft.pickup(col, height, rngs.pickups.pick(&PICKUPS), gate);
            let spots = &mut |rng: &mut Rng| {
                let col = rng.range(start + 4, end - 15);
                (col, ground[col] + rng.range(3, 6))
            };
            place_lenses(&mut draft, &mut rngs.lenses, gate, recipe.lenses, spots);
        }
        if i > 0 {
            draft.set(start + 2, ground[start + 2], 'K');
        }
    }

    draft.set(3, ground[3], '@');
    let end = cols - 1;
    draft.fill(end - 3..=end - 2, ground[end - 2], ground[end - 2] + 2, 'X');
    draft
}

/// A narrow level climbing platforms on alternating sides, each gate a
/// phantom platform.
fn tall(recipe: &Recipe, gates: &[Hue], rngs: &mut Streams) -> Draft {
    let rng = &mut rngs.layout;
    // Each platform's gate, if it is one. The top platform comes after.
    let mut platforms: Vec<Option<Hue>> = Vec::new();
    for &gate in gates {
        for _ in 0..rng.range(2, 3) {
            platforms.push(None);
        }
        platforms.push(Some(gate));
    }
    for _ in 0..rng.range(2, 3) {
        platforms.push(None);
    }
    let width = rng.range(26, 30);
    let top_platform = 4 + CLIMB * platforms.len();
    let mut draft = Draft::new(width + 2, top_platform + 10);

    // Platform i spans `left..=right` at height 4 + CLIMB * i, on the left
    // for even i. Each is a short gap across from the one below.
    let mut spans = Vec::new();
    let mut edge = rng.range(8, 12);
    for i in 0..=platforms.len() {
        let gap = rng.range(3, 6);
        let span = if i % 2 == 0 {
            if i > 0 {
                edge = edge.saturating_sub(gap + 1).clamp(6, 13);
            }
            (1, edge)
        } else {
            edge = (edge + gap + 1).min(width - 5);
            (edge, width)
        };
        spans.push(span);
    }

    for (i, &(left, right)) in spans.iter().enumerate() {
        let height = 4 + CLIMB * i;
        let ch = match platforms.get(i) {
            Some(Some(gate)) => letter(*gate).to_ascii_lowercase(),
            _ if rngs.ledges.chance(70) => '#',
            _ => letter(rngs.ledges.pick(&Hue::ALL[..6])),
        };
        draft.fill(left..=right, height, height, ch);
        let Some(&Some(gate)) = platforms.get(i) else {
            continue;
        };
        // A checkpoint on the platform after the gate.
        let (next_left, next_right) = spans[i + 1];
        draft.set(
            usize::midpoint(next_left, next_right),
            height + CLIMB + 1,
            'K',
        );
        // A pickup at the wall end of the platform before it.
        if i > 0 {
            let (before_left, before_right) = spans[i - 1];
            let col = if before_left == 1 {
                2
            } else {
                before_right - 1
            };
            let power = rngs.pickups.pick(&PICKUPS);
            draft.pickup(col, height - CLIMB + 1, power, gate);
        }
        // Lenses in the open space below the gate.
        let below = height - CLIMB;
        let spots = &mut |rng: &mut Rng| (rng.range(2, width - 1), below + rng.range(2, 3));
        place_lenses(&mut draft, &mut rngs.lenses, gate, recipe.lenses, spots);
    }

    // The exit at the wall end of the top platform.
    let (left, right) = spans[platforms.len()];
    let col = if left == 1 { 2 } else { right - 2 };
    draft.fill(col..=col + 1, top_platform + 1, top_platform + 3, 'X');
    let start = if spans[0].0 == 1 { width - 3 } else { 3 };
    draft.set(start, 0, '@');
    draft
}

/// Lay out `recipe` as its `attempt` makes it.
fn build(recipe: &Recipe, gates: &[Hue], attempt: u64) -> Draft {
    let rngs = &mut Streams::new(recipe.seed, attempt);
    let mut draft = match recipe.shape {
        Shape::Wide => wide(recipe, gates, rngs),
        Shape::Tall => tall(recipe, gates, rngs),
    };
    draft.decorate(&mut rngs.decoration);
    draft
}

/// Whether the draft is a good level: comfortably finishable with every
/// checkpoint in reach, and with no gate that even the full-strength hero
/// can bypass.
fn sound(draft: &Draft, name: &str, recipe: &Recipe, gates: &[Hue]) -> bool {
    let finishable = |draft: &Draft, limits| {
        parse(&draft.text(name, recipe))
            .ok()
            .map(|level| solve_within(&level, limits))
    };
    let Some(Ok(solution)) = finishable(draft, Limits::COMFORT) else {
        return false;
    };
    solution.unreached_checkpoints == 0
        && gates
            .iter()
            .all(|&gate| matches!(finishable(&draft.without(gate), Limits::FULL), Some(Err(_))))
}

/// The gates' hues, checking the recipe asks for something possible.
fn check_recipe(recipe: &Recipe) -> Result<Vec<Hue>, String> {
    if recipe.lenses > 6 {
        return Err("lenses must be at most 6 per gate".to_owned());
    }
    gate_hues(recipe)
}

/// The level file for `recipe`, named `name`: the first sound layout in
/// the order the seed gives, with the attempt that made it recorded.
///
/// # Errors
///
/// If the recipe asks for something impossible, or no sound layout turns
/// up.
pub fn generate(name: &str, recipe: &Recipe) -> Result<String, String> {
    let gates = check_recipe(recipe)?;
    for attempt in 0..ATTEMPTS {
        let made = Recipe {
            attempt: Some(attempt),
            ..recipe.clone()
        };
        let draft = build(&made, &gates, attempt);
        if sound(&draft, name, &made, &gates) {
            return Ok(draft.text(name, &made));
        }
    }
    Err(format!(
        "no sound layout in {ATTEMPTS} attempts; try another seed"
    ))
}

/// The level file for `recipe` laid out again from the attempt it records,
/// without searching or checking.
///
/// It's quick enough to test that committed files are up to date. Layout
/// is deterministic, so a file that matches is the
/// level that was checked when it was generated.
///
/// # Errors
///
/// If the recipe is impossible or records no attempt.
pub fn rebuild(name: &str, recipe: &Recipe) -> Result<String, String> {
    let gates = check_recipe(recipe)?;
    let attempt = recipe
        .attempt
        .ok_or("no attempt recorded; run `just levelgen`")?;
    Ok(build(recipe, &gates, attempt).text(name, recipe))
}

/// The name and settings at the head of a generated level file, which may
/// be nothing more than those.
///
/// # Errors
///
/// If the text isn't TOML, or has no `[generated]` settings.
pub fn read_recipe(text: &str) -> Result<(String, Recipe), String> {
    #[derive(Deserialize)]
    struct Head {
        name: String,
        generated: Recipe,
    }
    let head: Head = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    Ok((head.name, head.generated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::level_files;
    use crate::level::solve::{Unsolvable, solve};
    use std::path::Path;

    fn recipe(seed: u64, shape: Shape, gates: &[Tint]) -> Recipe {
        Recipe {
            seed,
            shape,
            gates: gates.to_vec(),
            lenses: 2,
            wave_interval: Some(9.0),
            attempt: None,
        }
    }

    #[test]
    fn generated_level_files_match_their_settings() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/levels");
        let mut generated = 0;
        for path in level_files(&dir).unwrap() {
            let text = std::fs::read_to_string(&path).unwrap();
            let Ok((name, recipe)) = read_recipe(&text) else {
                continue;
            };
            assert_eq!(
                rebuild(&name, &recipe).as_deref(),
                Ok(text.as_str()),
                "{} differs from what its settings make: run `just levelgen`",
                path.display()
            );
            generated += 1;
        }
        assert!(generated >= 1);
    }

    #[test]
    fn the_same_settings_give_the_same_level() {
        let wide = recipe(7, Shape::Wide, &[Tint::Red, Tint::Yellow]);
        let first = generate("A", &wide).unwrap();
        assert_eq!(generate("A", &wide), Ok(first.clone()));
        let (_, recorded) = read_recipe(&first).unwrap();
        assert_eq!(rebuild("A", &recorded), Ok(first.clone()));
        let other = Recipe { seed: 8, ..wide };
        assert_ne!(generate("A", &other).unwrap(), first);
    }

    #[test]
    fn rebuilding_needs_a_recorded_attempt() {
        let made = recipe(2, Shape::Tall, &[Tint::Red]);
        assert!(rebuild("R", &made).unwrap_err().starts_with("no attempt"));
    }

    #[test]
    fn generated_levels_read_back_with_their_settings() {
        for shape in [Shape::Wide, Shape::Tall] {
            let made = recipe(3, shape, &[Tint::Green, Tint::Magenta]);
            let text = generate("Made", &made).unwrap();
            let level = parse(&text).unwrap();
            assert_eq!(level.name, "Made");
            let recorded = level.generated.clone().unwrap();
            assert!(recorded.attempt.is_some());
            assert_eq!(
                Recipe {
                    attempt: None,
                    ..recorded.clone()
                },
                made
            );
            assert_eq!(read_recipe(&text), Ok(("Made".to_owned(), recorded)));
            assert_eq!(level.waves.interval, Some(9.0));
            assert_eq!(level.checkpoints.len(), 2);
            // Two lenses per gate, of the gate's primaries.
            assert_eq!(level.lenses.len(), 4);
            assert!(level.lenses.iter().any(|l| l.1 == Primary::Green));
            assert!(level.lenses.iter().any(|l| l.1 == Primary::Blue));
            assert_eq!(level.pickups.len(), 2);
            assert!(solve_within(&level, Limits::COMFORT).is_ok());
        }
    }

    #[test]
    fn every_gate_is_needed() {
        for shape in [Shape::Wide, Shape::Tall] {
            let made = recipe(11, shape, &[Tint::Blue, Tint::Cyan]);
            let text = generate("G", &made).unwrap();
            for phantom in ['b', 'c'] {
                let map_end = text.find("background =").unwrap();
                let (map, rest) = text.split_at(map_end);
                let open = map.replace(phantom, " ") + rest;
                assert_eq!(
                    solve(&parse(&open).unwrap()),
                    Err(Unsolvable::ExitOutOfReach),
                    "{shape:?} without {phantom}"
                );
            }
        }
    }

    #[test]
    fn a_level_without_gates_is_a_run_to_the_exit() {
        let text = generate("Run", &recipe(1, Shape::Wide, &[])).unwrap();
        let level = parse(&text).unwrap();
        assert_eq!(level.phantoms.len(), 0);
        assert_eq!(level.checkpoints.len(), 0);
        assert!(solve(&level).is_ok());
    }

    #[test]
    fn terrain_steps_gently_within_bounds() {
        for seed in 0..50 {
            let rng = &mut Rng::new(seed, 0, Stage::Terrain);
            let start = rng.range(0, MAX_GROUND);
            let heights = terrain(40, start, rng);
            assert_eq!(heights.len(), 40);
            let mut previous = start;
            for &height in &heights {
                assert!(height <= MAX_GROUND);
                assert!(height.abs_diff(previous) <= 2, "seed {seed}: {heights:?}");
                previous = height;
            }
        }
    }

    #[test]
    fn each_stage_draws_its_own_numbers() {
        let mut streams = Streams::new(5, 0);
        let firsts = [
            streams.layout.next(),
            streams.terrain.next(),
            streams.ledges.next(),
            streams.lenses.next(),
            streams.pickups.next(),
            streams.decoration.next(),
        ];
        let mut unique = firsts.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), firsts.len());
        // Another attempt draws differently.
        assert_ne!(Streams::new(5, 1).layout.next(), firsts[0]);
    }

    #[test]
    fn impossible_settings_are_reported() {
        let grey = recipe(1, Shape::Wide, &[Tint::Grey]);
        assert!(
            generate("X", &grey)
                .unwrap_err()
                .starts_with("gates must be")
        );
        let white = recipe(1, Shape::Tall, &[Tint::White]);
        assert!(generate("X", &white).is_err());
        let crowded = Recipe {
            lenses: 7,
            ..recipe(1, Shape::Wide, &[Tint::Red])
        };
        assert!(generate("X", &crowded).unwrap_err().contains("lenses"));
        assert!(rebuild("X", &crowded).unwrap_err().contains("lenses"));
        assert!(read_recipe("name = \"No settings\"").is_err());
        assert!(read_recipe("not toml").is_err());
    }

    #[test]
    fn random_numbers_stay_in_range() {
        let mut rng = Rng::new(0, 0, Stage::Layout);
        for _ in 0..1000 {
            let n = rng.range(3, 6);
            assert!((3..=6).contains(&n));
        }
        assert_eq!(rng.range(5, 5), 5);
        let hits = (0..1000).filter(|_| rng.chance(30)).count();
        assert!((200..400).contains(&hits), "{hits}");
    }
}
