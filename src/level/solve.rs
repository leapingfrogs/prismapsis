//! Proving a level can be finished.
//!
//! Every bounce is simulated with the game's own physics: from each place
//! the hero can land, with each spin, to where it lands next. The search
//! covers those landings together with which primaries are restored, since
//! restoring a colour makes its phantoms solid, and whether the hero has
//! picked up anti-gravity, which lets it hover anywhere open. A level can be
//! finished if the hero can touch an exit with white restored.
//!
//! Light is counted generously: motes come in endless waves, so a primary
//! the waves bring can always be restored, and otherwise the lenses the beam
//! can reach must hold enough light.

use std::collections::{HashSet, VecDeque};
use std::fmt;

use bevy::math::Vec2;

use super::format::Level;
use crate::PHYSICS_HZ;
use crate::beam::{BeamTuning, MAX_BEAM_LENGTH};
use crate::enemy::{EnemyTuning, LENS_RADIUS, line_of_sight};
use crate::hero::HERO_RADIUS;
use crate::light::{Hue, LightTuning, Primary};
use crate::physics::{self, Aabb, Body, BounceTuning, MAX_SPIN};
use crate::powerup::{PICKUP_RADIUS, PowerUp};
use crate::prism::PrismTuning;

/// Landings closer than this horizontally count as the same place (world
/// units).
const LANDING_GRID: f32 = 8.0;

/// Spacing of the places a hovering hero is tried at (world units).
const HOVER_GRID: f32 = 10.0;

/// Longest a single bounce is followed before it counts as never landing
/// (seconds).
const MAX_ARC: f32 = 8.0;

/// Places along a bounce, or hover cells along each axis, checked for a shot
/// at a lens: one in this many.
const SIGHT_EVERY: usize = 4;

/// Solids further than this horizontally from a bounce are left out of its
/// simulation, and the selection is renewed once the hero has moved
/// `WINDOW / 2` from where it was made (world units).
const WINDOW: f32 = 800.0;

/// Which primaries are restored, one bit per `Primary::index`.
type Restored = u8;

const ALL_RESTORED: Restored = 0b111;

const fn bit(primary: Primary) -> Restored {
    1 << primary.index()
}

fn is_restored(hue: Hue, restored: Restored) -> bool {
    hue.primaries().iter().all(|&p| restored & bit(p) != 0)
}

fn primaries(restored: Restored) -> Vec<Primary> {
    Primary::ALL
        .into_iter()
        .filter(|&p| restored & bit(p) != 0)
        .collect()
}

fn names(primaries: &[Primary]) -> String {
    let names: Vec<_> = primaries
        .iter()
        .map(|p| format!("{p:?}").to_lowercase())
        .collect();
    names.join(", ")
}

/// How a level can be finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Solution {
    /// The order the primaries can be restored in.
    pub restores: Vec<Primary>,
    /// Whether finishing needs anti-gravity.
    pub anti_gravity: bool,
    /// Checkpoints the hero can never touch.
    pub unreached_checkpoints: usize,
}

impl fmt::Display for Solution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "restore {}", names(&self.restores))?;
        if self.anti_gravity {
            write!(f, ", using anti-gravity")?;
        }
        match self.unreached_checkpoints {
            0 => Ok(()),
            1 => write!(f, "; a checkpoint is out of reach"),
            n => write!(f, "; {n} checkpoints are out of reach"),
        }
    }
}

/// Why a level can't be finished.
#[derive(Debug, Clone, PartialEq)]
pub enum Unsolvable {
    /// Some primaries can never be restored: the waves don't bring them and
    /// the lenses in reach hold too little light.
    NoLight {
        restored: Vec<Primary>,
        /// Each missing primary, with the lens light in reach.
        missing: Vec<(Primary, f32)>,
        needed: f32,
    },
    /// Every colour can be restored, but the exit is still out of reach.
    ExitOutOfReach,
}

impl fmt::Display for Unsolvable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLight {
                restored,
                missing,
                needed,
            } => {
                if restored.is_empty() {
                    write!(f, "no colour can be restored")?;
                } else {
                    write!(f, "only {} can be restored", names(restored))?;
                }
                for (primary, light) in missing {
                    let name = names(&[*primary]);
                    write!(
                        f,
                        "; no {name} waves, and {name} lenses in reach give {light} of {needed} light"
                    )?;
                }
                Ok(())
            }
            Self::ExitOutOfReach => {
                write!(f, "the exit is out of reach with every colour restored")
            }
        }
    }
}

/// A point in the search: which primaries are restored, and whether the hero
/// is hovering with anti-gravity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct State {
    restored: Restored,
    hovering: bool,
}

impl State {
    const COUNT: usize = 16;

    const fn index(self) -> usize {
        self.restored as usize * 2 + self.hovering as usize
    }

    /// Every state, each after all those that can lead to it: restoring only
    /// adds colours, and anti-gravity is never lost.
    fn in_order() -> Vec<Self> {
        let mut states: Vec<Self> = (0..=ALL_RESTORED)
            .flat_map(|restored| [false, true].map(|hovering| Self { restored, hovering }))
            .collect();
        states.sort_by_key(|s| (s.restored.count_ones(), s.hovering));
        states
    }
}

/// How a state was first reached.
#[derive(Debug, Clone, Copy)]
enum Step {
    Restore(Primary),
    AntiGravity,
}

/// What the hero can reach in one state.
#[derive(Debug, Default)]
struct Explored {
    /// Where the hero can be: landings while bouncing, open places while
    /// hovering.
    places: Vec<Vec2>,
    exit: bool,
    /// Where the hero can pick up anti-gravity.
    anti_gravity: Option<Vec2>,
    checkpoints: Vec<bool>,
    lenses: Vec<bool>,
}

/// A level and the tuning it's played with.
struct World<'a> {
    level: &'a Level,
    bounce: BounceTuning,
    light: LightTuning,
    /// Whether waves bring each primary.
    waves: [bool; 3],
    dt: f32,
    max_steps: usize,
    /// How far above or below the beam can reach for each unit across,
    /// through the prism.
    sight_slope: f32,
    /// How far the prism can carry the beam off the hero's line.
    sight_offset: f32,
}

fn hero_box(at: Vec2) -> Aabb {
    Aabb::from_center(at, Vec2::splat(HERO_RADIUS))
}

/// The solids within `WINDOW` of `x`.
fn nearby(solids: &[Aabb], x: f32) -> Vec<Aabb> {
    solids
        .iter()
        .filter(|s| s.max.x > x - WINDOW && s.min.x < x + WINDOW)
        .copied()
        .collect()
}

impl<'a> World<'a> {
    fn new(level: &'a Level) -> Self {
        let mut enemy = EnemyTuning::default();
        let mut light = LightTuning::default();
        level.apply_tuning(&mut enemy, &mut light);
        let waves = Primary::ALL.map(|p| enemy.wave_size > 0 && level.waves.colours.contains(&p));
        #[allow(clippy::cast_possible_truncation)]
        let dt = (1.0 / PHYSICS_HZ) as f32;
        // Bounded by the constants, so the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let max_steps = (MAX_ARC / dt).ceil() as usize;
        Self {
            level,
            bounce: BounceTuning::default(),
            light,
            waves,
            dt,
            max_steps,
            sight_slope: BeamTuning::default().split_angle.to_radians().tan(),
            sight_offset: PrismTuning::default().orbit_radius,
        }
    }

    /// Walls, and the phantoms whose hue is restored.
    fn solids(&self, restored: Restored) -> Vec<Aabb> {
        self.level
            .walls
            .iter()
            .map(|&(aabb, _)| aabb)
            .chain(
                self.level
                    .phantoms
                    .iter()
                    .filter(|&&(_, hue)| is_restored(hue, restored))
                    .map(|&(aabb, _)| aabb),
            )
            .collect()
    }

    /// Note everything the hero touches at `at`, and the lenses it can shoot
    /// from there if `sight`.
    fn touch(
        &self,
        at: Vec2,
        restored: Restored,
        solids: &[Aabb],
        sight: bool,
        out: &mut Explored,
    ) {
        let hero = hero_box(at);
        let level = self.level;
        out.exit |= level.exits.iter().any(|e| e.overlaps(&hero));
        for (reached, checkpoint) in out.checkpoints.iter_mut().zip(&level.checkpoints) {
            *reached |= checkpoint.overlaps(&hero);
        }
        if out.anti_gravity.is_none()
            && level.pickups.iter().any(|p| {
                p.power == PowerUp::AntiGravity
                    && p.hidden.is_none_or(|hue| is_restored(hue, restored))
                    && p.at.distance(at) < HERO_RADIUS + PICKUP_RADIUS
            })
        {
            out.anti_gravity = Some(at);
        }
        if !sight {
            return;
        }
        for (shot, &(lens, _)) in out.lenses.iter_mut().zip(&level.lenses) {
            if *shot {
                continue;
            }
            let span = lens - at;
            let across = span.x.abs();
            *shot = span.length() <= MAX_BEAM_LENGTH
                && span.y.abs() <= across * self.sight_slope + self.sight_offset + LENS_RADIUS
                && line_of_sight(at, lens, solids);
        }
    }

    /// Follow one bounce from `from` with `vel` until the hero lands,
    /// noting what it touches on the way. `None` if it never lands.
    fn arc(
        &self,
        from: Vec2,
        vel: Vec2,
        spin: i8,
        restored: Restored,
        solids: &[Aabb],
        out: &mut Explored,
    ) -> Option<Vec2> {
        let mut body = Body {
            pos: from,
            vel,
            half: HERO_RADIUS,
        };
        let mut spin = spin;
        let mut centre = from.x;
        let mut near = nearby(solids, centre);
        for step in 0..self.max_steps {
            if (body.pos.x - centre).abs() > WINDOW / 2.0 {
                centre = body.pos.x;
                near = nearby(solids, centre);
            }
            let contacts = physics::step(&mut body, &mut spin, &self.bounce, &near, self.dt);
            self.touch(body.pos, restored, solids, step % SIGHT_EVERY == 0, out);
            if contacts.floor {
                return Some(body.pos);
            }
        }
        None
    }

    /// Everywhere a bouncing hero can land from `seeds`, where it starts
    /// falling from rest.
    fn bounce(&self, restored: Restored, seeds: &[Vec2], out: &mut Explored) {
        let solids = self.solids(restored);
        let key = |at: Vec2| {
            // Places are inside the level, so these casts cannot truncate.
            #[allow(clippy::cast_possible_truncation)]
            let key = ((at.x / LANDING_GRID).round() as i32, at.y.round() as i32);
            key
        };
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        for &seed in seeds {
            // A phantom that has just turned solid around the hero waits for
            // it to leave; the hero's other places cover where it goes.
            if solids.iter().any(|s| s.overlaps(&hero_box(seed))) {
                continue;
            }
            if let Some(landing) = self.arc(seed, Vec2::ZERO, 0, restored, &solids, out)
                && seen.insert(key(landing))
            {
                queue.push_back(landing);
            }
        }
        let launch = self.bounce.bounce_speed();
        while let Some(from) = queue.pop_front() {
            out.places.push(from);
            for spin in -MAX_SPIN..=MAX_SPIN {
                let vel = Vec2::new(self.bounce.horizontal_speed(spin), launch);
                if let Some(landing) = self.arc(from, vel, spin, restored, &solids, out)
                    && seen.insert(key(landing))
                {
                    queue.push_back(landing);
                }
            }
        }
    }

    /// Everywhere open a hovering hero can move to from `seeds`.
    fn hover(&self, restored: Restored, seeds: &[Vec2], out: &mut Explored) {
        let solids = self.solids(restored);
        let size = self.level.size;
        // Bounded by the level's size, so the casts cannot truncate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (cols, rows) = (
            (size.x / HOVER_GRID) as usize + 1,
            (size.y / HOVER_GRID) as usize + 1,
        );
        #[allow(clippy::cast_precision_loss)]
        let place = |col: usize, row: usize| Vec2::new(col as f32, row as f32) * HOVER_GRID;
        let open = |at: Vec2| {
            at.cmpge(Vec2::splat(HERO_RADIUS)).all()
                && at.cmple(size - HERO_RADIUS).all()
                && !solids.iter().any(|s| s.overlaps(&hero_box(at)))
        };
        let mut seen = vec![false; cols * rows];
        let mut queue = VecDeque::new();
        for &seed in seeds {
            // The grid places around the seed that are open: the nearest one
            // alone may clip a floor the seed itself clears.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (col, row) = (
                (seed.x / HOVER_GRID).round().max(1.0) as usize,
                (seed.y / HOVER_GRID).round().max(1.0) as usize,
            );
            for (c, r) in (col - 1..=col + 1).flat_map(|c| (row - 1..=row + 1).map(move |r| (c, r)))
            {
                if c < cols && r < rows && !seen[r * cols + c] && open(place(c, r)) {
                    seen[r * cols + c] = true;
                    queue.push_back((c, r));
                }
            }
        }
        while let Some((col, row)) = queue.pop_front() {
            let at = place(col, row);
            out.places.push(at);
            let sight = col % SIGHT_EVERY == 0 && row % SIGHT_EVERY == 0;
            self.touch(at, restored, &solids, sight, out);
            let neighbours = [
                (col.wrapping_sub(1), row),
                (col + 1, row),
                (col, row.wrapping_sub(1)),
                (col, row + 1),
            ];
            for (c, r) in neighbours {
                if c < cols && r < rows && !seen[r * cols + c] && open(place(c, r)) {
                    seen[r * cols + c] = true;
                    queue.push_back((c, r));
                }
            }
        }
    }

    fn explore(&self, state: State, seeds: &[Vec2]) -> Explored {
        let mut out = Explored {
            checkpoints: vec![false; self.level.checkpoints.len()],
            lenses: vec![false; self.level.lenses.len()],
            ..Explored::default()
        };
        if state.hovering {
            self.hover(state.restored, seeds, &mut out);
        } else {
            self.bounce(state.restored, seeds, &mut out);
        }
        out
    }

    /// Light for `primary` from the lenses the hero can shoot.
    fn lens_light(&self, primary: Primary, explored: &Explored) -> f32 {
        let lenses = self
            .level
            .lenses
            .iter()
            .zip(&explored.lenses)
            .filter(|&(&(_, colour), &shot)| shot && colour == primary)
            .count();
        // A handful of lenses, so the cast is exact.
        #[allow(clippy::cast_precision_loss)]
        let light = lenses as f32 * self.light.lens_light;
        light
    }

    fn restorable(&self, primary: Primary, explored: &Explored) -> bool {
        self.waves[primary.index()]
            || self.lens_light(primary, explored) >= self.light.gauge_capacity
    }
}

/// Whether `level` can be finished, and how.
///
/// # Errors
///
/// Says why the level can't be finished.
pub fn solve(level: &Level) -> Result<Solution, Unsolvable> {
    let world = World::new(level);
    let mut seeds: Vec<Vec<Vec2>> = vec![Vec::new(); State::COUNT];
    let mut came_from: Vec<Option<(State, Step)>> = vec![None; State::COUNT];
    let mut explored: Vec<Option<Explored>> = (0..State::COUNT).map(|_| None).collect();
    let start = State {
        restored: 0,
        hovering: false,
    };
    seeds[start.index()].push(level.start);

    for state in State::in_order() {
        if seeds[state.index()].is_empty() {
            continue;
        }
        let here = world.explore(state, &seeds[state.index()]);
        let mut lead = |next: State, step: Step, places: &[Vec2]| {
            seeds[next.index()].extend_from_slice(places);
            came_from[next.index()].get_or_insert((state, step));
        };
        if let (false, Some(at)) = (state.hovering, here.anti_gravity) {
            let next = State {
                hovering: true,
                ..state
            };
            lead(next, Step::AntiGravity, &[at]);
        }
        for primary in Primary::ALL {
            if state.restored & bit(primary) == 0 && world.restorable(primary, &here) {
                let next = State {
                    restored: state.restored | bit(primary),
                    ..state
                };
                lead(next, Step::Restore(primary), &here.places);
            }
        }
        explored[state.index()] = Some(here);
    }

    let finished = [false, true]
        .map(|hovering| State {
            restored: ALL_RESTORED,
            hovering,
        })
        .into_iter()
        .find(|s| explored[s.index()].as_ref().is_some_and(|e| e.exit));
    let Some(end) = finished else {
        return Err(unsolvable(&world, &explored));
    };

    let mut restores = Vec::new();
    let mut anti_gravity = false;
    let mut at = end;
    while let Some((before, step)) = came_from[at.index()] {
        match step {
            Step::Restore(primary) => restores.push(primary),
            Step::AntiGravity => anti_gravity = true,
        }
        at = before;
    }
    restores.reverse();
    let unreached_checkpoints = (0..level.checkpoints.len())
        .filter(|&i| !explored.iter().flatten().any(|e| e.checkpoints[i]))
        .count();
    Ok(Solution {
        restores,
        anti_gravity,
        unreached_checkpoints,
    })
}

/// Why no state reached the exit: the colours that can't be restored from
/// the furthest state, or the exit itself.
fn unsolvable(world: &World, explored: &[Option<Explored>]) -> Unsolvable {
    let furthest = State::in_order()
        .into_iter()
        .rev()
        .find(|s| explored[s.index()].is_some());
    let Some(state) = furthest else {
        return Unsolvable::ExitOutOfReach;
    };
    if state.restored == ALL_RESTORED {
        return Unsolvable::ExitOutOfReach;
    }
    let here = explored[state.index()].as_ref();
    let missing = Primary::ALL
        .into_iter()
        .filter(|&p| state.restored & bit(p) == 0)
        .map(|p| (p, here.map_or(0.0, |e| world.lens_light(p, e))))
        .collect();
    Unsolvable::NoLight {
        restored: primaries(state.restored),
        missing,
        needed: world.light.gauge_capacity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{level_files, load, parse};
    use std::path::Path;

    fn level(map: &str, extra: &str) -> Level {
        parse(&format!("name = \"T\"\nmap = '''\n{map}'''\n{extra}")).unwrap()
    }

    #[test]
    fn every_level_file_can_be_finished() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/levels");
        for path in level_files(&dir).unwrap() {
            let level = load(&path).unwrap();
            let solution = solve(&level);
            assert!(
                solution.is_ok(),
                "{}: {}",
                path.display(),
                solution.unwrap_err()
            );
        }
    }

    /// A room 12 rows tall with its floor top at 40. Its exit sits on a
    /// ledge whose top is 320, the bounce's height, above `body`'s rows.
    const LEDGE: &str = "\
##############
#            #
#            #
#           X#
#         ####
#            #
#            #
#            #
#            #
#            #
#  @         #
##############
";

    /// A room 13 rows tall with its floor top at 40 and its exit on a ledge
    /// whose top is 400: out of a bounce's reach from the floor.
    const HIGH: &str = "\
##############
#            #
#           X#
#         ####
#            #
#            #
#            #
#            #
#            #
#            #
#            #
#  @         #
##############
";

    /// `HIGH` with a red phantom step whose top is 200.
    fn stepped() -> String {
        HIGH.replacen(
            "#            #\n#            #\n#            #\n#  @",
            "#     rrr    #\n#            #\n#            #\n#  @",
            1,
        )
    }

    #[test]
    fn a_ledge_within_bounce_height_can_be_reached() {
        let solution = solve(&level(LEDGE, "")).unwrap();
        assert_eq!(solution.restores.len(), 3);
        assert!(!solution.anti_gravity);
        assert_eq!(solve(&level(HIGH, "")), Err(Unsolvable::ExitOutOfReach));
    }

    #[test]
    fn a_phantom_step_helps_only_once_its_colour_can_return() {
        assert!(solve(&level(&stepped(), "")).is_ok());

        let no_red = "[waves]\ncolours = [\"green\", \"blue\"]\n";
        assert_eq!(
            solve(&level(&stepped(), no_red)),
            Err(Unsolvable::NoLight {
                restored: vec![Primary::Green, Primary::Blue],
                missing: vec![(Primary::Red, 0.0)],
                needed: 10.0,
            })
        );
    }

    #[test]
    fn lenses_in_sight_supply_light_the_waves_do_not() {
        let extra = "[waves]\ncolours = [\"green\", \"blue\"]\n\
                     [legend]\n\"1\" = { lens = \"red\" }\n";
        let lenses = stepped().replacen("#            #\n#  @", "# 1  1  1    #\n#  @", 1);
        assert!(solve(&level(&lenses, extra)).is_ok());

        // Walled in, the lenses can't be shot.
        let walled = stepped().replacen(
            "#            #\n#            #\n#  @",
            "#   #######  #\n#   #1 1 1#  #\n#  @#######  #",
            1,
        );
        let unsolvable = solve(&level(&walled, extra)).unwrap_err();
        assert!(
            matches!(unsolvable, Unsolvable::NoLight { .. }),
            "{unsolvable}"
        );
    }

    #[test]
    fn anti_gravity_reaches_what_bouncing_cannot() {
        let extra = "[legend]\n\"a\" = { pickup = \"anti-gravity\" }\n";
        let pickup = HIGH.replace("#  @      ", "#  @  a   ");
        let solution = solve(&level(&pickup, extra)).unwrap();
        assert!(solution.anti_gravity);

        // Hidden until cyan, it's still there once green and blue return.
        let hidden = "[legend]\n\"a\" = { pickup = \"anti-gravity\", hidden = \"cyan\" }\n";
        assert!(solve(&level(&pickup, hidden)).unwrap().anti_gravity);
    }

    #[test]
    fn unreached_checkpoints_are_counted() {
        let open = LEDGE.replace("#  @      ", "#  @  K   ");
        assert_eq!(solve(&level(&open, "")).unwrap().unreached_checkpoints, 0);

        let sealed = LEDGE.replace(
            "#            #\n#  @         #",
            "###          #\n#K#   @      #",
        );
        let solution = solve(&level(&sealed, "")).unwrap();
        assert_eq!(solution.unreached_checkpoints, 1);
        assert!(
            solution
                .to_string()
                .ends_with("; a checkpoint is out of reach")
        );
    }

    #[test]
    fn problems_read_clearly() {
        let none = Unsolvable::NoLight {
            restored: Vec::new(),
            missing: vec![(Primary::Red, 4.0)],
            needed: 10.0,
        };
        assert_eq!(
            none.to_string(),
            "no colour can be restored; no red waves, and red lenses in reach give 4 of 10 light"
        );
        let some = Unsolvable::NoLight {
            restored: vec![Primary::Green],
            missing: vec![(Primary::Red, 0.0), (Primary::Blue, 8.0)],
            needed: 10.0,
        };
        assert!(
            some.to_string()
                .starts_with("only green can be restored; no red")
        );
        let solution = Solution {
            restores: vec![Primary::Blue, Primary::Red, Primary::Green],
            anti_gravity: true,
            unreached_checkpoints: 2,
        };
        assert_eq!(
            solution.to_string(),
            "restore blue, red, green, using anti-gravity; 2 checkpoints are out of reach"
        );
    }
}
