//! The hero's laser beam, its energy, and how it splits through the prism.
//!
//! Holding fire keeps a continuous beam on while energy lasts. The beam is a
//! ray from the hero to the first thing it hits; crossing the prism splits it
//! into red, green and blue beams fanning out away from the hero. Simulation
//! only; visuals live in `render`.

use bevy::prelude::*;

use crate::enemy::{Enemy, HIT_FLASH};
use crate::events::GameEvent;
use crate::hero::{Facing, HERO_RADIUS, Hero, Position, update_facing};
use crate::level::Solid;
use crate::light::Primary;
use crate::physics::{Aabb, Traits, ray_aabb, ray_aabb_face, ray_circle};
use crate::platforms::Breakable;
use crate::powerup::{Abilities, PowerUp, PowerUpTuning};
use crate::prism::{PRISM_RADIUS, Prism, orbit_prism};

/// Longest a beam can be if it hits nothing (world units).
pub const MAX_BEAM_LENGTH: f32 = 6000.0;

/// Tunable beam parameters. Adjusted live from the dev tuning panel.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct BeamTuning {
    /// Full energy.
    pub max_energy: f32,
    /// Energy used per second while firing.
    pub drain_rate: f32,
    /// Energy regained per second while not firing.
    pub recharge_rate: f32,
    /// Pause after firing stops before recharging starts (seconds).
    pub recharge_delay: f32,
    /// Energy needed to start firing.
    pub min_energy_to_fire: f32,
    /// Damage ticks per second applied to whatever the beam touches.
    pub damage_rate: f32,
    /// Angle between the centre beam and each outer beam of a split
    /// (degrees).
    pub split_angle: f32,
}

impl Default for BeamTuning {
    fn default() -> Self {
        Self {
            max_energy: 100.0,
            drain_rate: 35.0,
            recharge_rate: 25.0,
            recharge_delay: 0.6,
            min_energy_to_fire: 15.0,
            damage_rate: 10.0,
            split_angle: 20.0,
        }
    }
}

/// The beam's energy and firing state.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct BeamEnergy {
    pub current: f32,
    pub firing: bool,
    /// Seconds since the beam last fired, for the recharge delay.
    pub since_fired: f32,
    /// Set when energy runs out mid-burst; fire must be released before the
    /// beam can start again.
    pub needs_release: bool,
}

impl Default for BeamEnergy {
    fn default() -> Self {
        Self {
            current: BeamTuning::default().max_energy,
            firing: false,
            since_fired: f32::MAX,
            needs_release: false,
        }
    }
}

/// Advance the energy state by `dt` with fire `held` or not.
///
/// Starting needs `min_energy_to_fire`; once firing, the beam continues until
/// fire is released or energy runs out. Running out requires a release
/// before firing again. Recharging begins `recharge_delay` after firing
/// stops.
#[must_use]
pub fn step_energy(energy: BeamEnergy, held: bool, tuning: &BeamTuning, dt: f32) -> BeamEnergy {
    let mut next = energy;
    if !held {
        next.needs_release = false;
    }
    let can_fire = if energy.firing {
        energy.current > 0.0
    } else {
        !next.needs_release && energy.current >= tuning.min_energy_to_fire
    };
    next.firing = held && can_fire;

    if next.firing {
        next.current = (energy.current - tuning.drain_rate * dt).max(0.0);
        next.since_fired = 0.0;
        if next.current <= 0.0 {
            next.firing = false;
            next.needs_release = true;
        }
    } else {
        next.since_fired = energy.since_fired + dt;
        if next.since_fired >= tuning.recharge_delay {
            next.current = (energy.current + tuning.recharge_rate * dt).min(tuning.max_energy);
        }
    }
    next
}

/// Whether the player is holding fire this frame.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FireHeld(pub bool);

/// Colour of a beam: white from the hero, red/green/blue after the prism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum BeamColour {
    White,
    Red,
    Green,
    Blue,
}

impl BeamColour {
    pub const ALL: [Self; 4] = [Self::White, Self::Red, Self::Green, Self::Blue];
}

/// Times a beam can reflect off mirrors before it stops.
pub const MAX_REFLECTIONS: usize = 4;

/// Things a beam can hit.
#[derive(Debug, Default)]
pub struct RayScene {
    pub solids: Vec<Aabb>,
    /// Solids whose faces reflect the beam.
    pub mirrors: Vec<Aabb>,
    /// Target centres and radii.
    pub targets: Vec<(Vec2, f32)>,
}

impl RayScene {
    /// The scene made by the level's solids, sorting mirrors from the rest,
    /// with `targets` to hit.
    pub fn new<'a>(
        solids: impl IntoIterator<Item = (&'a Solid, Option<&'a Traits>)>,
        targets: Vec<(Vec2, f32)>,
    ) -> Self {
        let (mirrors, solids): (Vec<_>, Vec<_>) = solids
            .into_iter()
            .partition(|(_, traits)| traits.is_some_and(|t| t.mirror));
        Self {
            solids: solids.into_iter().map(|(s, _)| s.0).collect(),
            mirrors: mirrors.into_iter().map(|(s, _)| s.0).collect(),
            targets,
        }
    }
}

/// One straight piece of a traced beam.
pub type BeamPiece = (Vec2, Vec2, BeamColour);

/// What a traced beam produced: its pieces (white first, then red, green,
/// blue if split) and the indices (into `RayScene::targets`) of targets hit.
#[derive(Debug, Default, PartialEq)]
pub struct BeamTrace {
    pub segments: Vec<BeamPiece>,
    pub hit_targets: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Nothing,
    Solid,
    /// A mirror face, facing this way.
    Mirror(Vec2),
    Target(usize),
    Prism,
}

/// Cast a ray, returning where it stops and what stopped it, and adding the
/// targets it hit to `hits`. Targets stop the ray unless `piercing`, in which
/// case it hits every target before the solid or prism that stops it.
fn cast(
    origin: Vec2,
    dir: Vec2,
    scene: &RayScene,
    prism: Option<Vec2>,
    piercing: bool,
    hits: &mut Vec<usize>,
) -> (f32, Hit) {
    let mut best = (MAX_BEAM_LENGTH, Hit::Nothing);
    let mut consider = |t: Option<f32>, hit| {
        if let Some(t) = t.filter(|&t| t < best.0) {
            best = (t, hit);
        }
    };
    for solid in &scene.solids {
        consider(ray_aabb(origin, dir, solid), Hit::Solid);
    }
    for mirror in &scene.mirrors {
        if let Some((t, normal)) = ray_aabb_face(origin, dir, mirror) {
            consider(Some(t), Hit::Mirror(normal));
        }
    }
    if let Some(center) = prism {
        consider(ray_circle(origin, dir, center, PRISM_RADIUS), Hit::Prism);
    }
    let target_hits = scene
        .targets
        .iter()
        .enumerate()
        .filter_map(|(i, &(center, radius))| {
            ray_circle(origin, dir, center, radius).map(|t| (t, i))
        });
    if piercing {
        hits.extend(target_hits.filter(|&(t, _)| t < best.0).map(|(_, i)| i));
        return best;
    }
    for (t, i) in target_hits {
        consider(Some(t), Hit::Target(i));
    }
    if let Hit::Target(i) = best.1 {
        hits.push(i);
    }
    best
}

/// How far off a mirror a reflected beam starts, so it doesn't hit the
/// face it left.
const MIRROR_GAP: f32 = 0.01;

/// Follow one beam of `colour` from `origin` along `dir`, reflecting off
/// mirrors up to `MAX_REFLECTIONS` times and adding each piece to `trace`.
/// Returns what stopped it, and where its last piece started.
fn follow(
    trace: &mut BeamTrace,
    mut origin: Vec2,
    mut dir: Vec2,
    colour: BeamColour,
    prism: Option<Vec2>,
    scene: &RayScene,
    piercing: bool,
) -> (Hit, Vec2) {
    for reflections in 0..=MAX_REFLECTIONS {
        let (t, hit) = cast(origin, dir, scene, prism, piercing, &mut trace.hit_targets);
        let end = origin + dir * t;
        trace.segments.push((origin, end, colour));
        match hit {
            Hit::Mirror(normal) if reflections < MAX_REFLECTIONS => {
                dir -= 2.0 * dir.dot(normal) * normal;
                origin = end + normal * MIRROR_GAP;
            }
            _ => return (hit, origin),
        }
    }
    (Hit::Nothing, origin)
}

/// Trace the beam fired from `origin` along `dir` (normalised).
///
/// Mirrors reflect it. If it crosses the prism at `prism`, the white beam
/// ends there and three coloured beams fan out from the prism's far side,
/// centred on the direction from `hero` (or the last mirror) to the prism.
/// Split beams don't re-enter the prism. Targets stop each beam unless
/// `piercing`.
#[must_use]
pub fn trace_beam(
    origin: Vec2,
    dir: Vec2,
    hero: Vec2,
    prism: Option<Vec2>,
    scene: &RayScene,
    split_angle: f32,
    piercing: bool,
) -> BeamTrace {
    let mut trace = BeamTrace::default();
    let (hit, last) = follow(
        &mut trace,
        origin,
        dir,
        BeamColour::White,
        prism,
        scene,
        piercing,
    );
    let (Hit::Prism, Some(center)) = (hit, prism) else {
        return trace;
    };
    let source = if last == origin { hero } else { last };
    let outward = (center - source).normalize_or(dir);
    let spread = split_angle.to_radians();
    for (angle, colour) in [
        (spread, BeamColour::Red),
        (0.0, BeamColour::Green),
        (-spread, BeamColour::Blue),
    ] {
        let split_dir = Vec2::from_angle(angle).rotate(outward);
        let start = center + split_dir * PRISM_RADIUS;
        follow(&mut trace, start, split_dir, colour, None, scene, piercing);
    }
    trace
}

/// How the hero's beam fires, as shaped by weapon power-ups.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct BeamLoadout {
    /// Also fire backwards.
    pub rear: bool,
    /// Fan each beam into three, this many degrees apart.
    pub spread: Option<f32>,
    /// Beams pass through enemies.
    pub piercing: bool,
}

impl BeamLoadout {
    /// The loadout given the hero's `abilities`.
    #[must_use]
    pub fn from_abilities(abilities: Abilities, tuning: &PowerUpTuning) -> Self {
        Self {
            rear: abilities.has(PowerUp::RearShot),
            spread: abilities
                .has(PowerUp::Spread)
                .then_some(tuning.spread_angle),
            piercing: abilities.has(PowerUp::Piercing),
        }
    }

    /// Directions the hero's beams leave in when facing `facing`.
    #[must_use]
    pub fn directions(&self, facing: Facing) -> Vec<Vec2> {
        let forward = facing.direction();
        let mains: &[Vec2] = if self.rear {
            &[forward, -forward]
        } else {
            &[forward]
        };
        mains
            .iter()
            .flat_map(|&dir| {
                self.spread.map_or_else(
                    || vec![dir],
                    |angle| {
                        let angle = angle.to_radians();
                        vec![
                            dir,
                            Vec2::from_angle(angle).rotate(dir),
                            Vec2::from_angle(-angle).rotate(dir),
                        ]
                    },
                )
            })
            .collect()
    }
}

/// Beam tuning with power-ups applied: rapid fire speeds up damage ticks and
/// recharging.
#[must_use]
pub fn boosted_tuning(
    base: &BeamTuning,
    abilities: Abilities,
    powerups: &PowerUpTuning,
) -> BeamTuning {
    if !abilities.has(PowerUp::RapidFire) {
        return base.clone();
    }
    BeamTuning {
        damage_rate: base.damage_rate * powerups.rapid_fire_boost,
        recharge_rate: base.recharge_rate * powerups.rapid_fire_boost,
        ..base.clone()
    }
}

/// Trace every beam from a hero at `hero` facing `facing`. Each target hit
/// is listed once, however many beams touch it.
#[must_use]
pub fn trace_from_hero(
    hero: Vec2,
    facing: Facing,
    prism: Option<Vec2>,
    scene: &RayScene,
    split_angle: f32,
    loadout: &BeamLoadout,
) -> BeamTrace {
    let mut trace = BeamTrace::default();
    for dir in loadout.directions(facing) {
        let beam = trace_beam(
            hero + dir * HERO_RADIUS,
            dir,
            hero,
            prism,
            scene,
            split_angle,
            loadout.piercing,
        );
        trace.segments.extend(beam.segments);
        trace.hit_targets.extend(beam.hit_targets);
    }
    trace.hit_targets.sort_unstable();
    trace.hit_targets.dedup();
    trace
}

/// Repeating timer for damage ticks while the beam is on.
#[derive(Resource, Debug)]
pub struct DamageTimer(pub Timer);

impl Default for DamageTimer {
    fn default() -> Self {
        let rate = BeamTuning::default().damage_rate;
        Self(Timer::from_seconds(1.0 / rate, TimerMode::Repeating))
    }
}

#[derive(Debug, Default)]
pub struct BeamPlugin;

impl Plugin for BeamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BeamTuning>()
            .register_type::<BeamTuning>()
            .init_resource::<BeamEnergy>()
            .init_resource::<FireHeld>()
            .init_resource::<DamageTimer>()
            .add_systems(Update, fire_input.run_if(crate::pause::running))
            .add_systems(
                FixedUpdate,
                (update_energy, damage_enemies)
                    .chain()
                    .after(orbit_prism)
                    .after(update_facing),
            );
    }
}

/// Hold Space (gamepad South or right trigger) to fire.
fn fire_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut held: ResMut<FireHeld>,
) {
    let pressed = keys.pressed(KeyCode::Space)
        || gamepads.iter().any(|pad| {
            pad.pressed(GamepadButton::South) || pad.pressed(GamepadButton::RightTrigger2)
        });
    if held.0 != pressed {
        held.0 = pressed;
    }
}

fn update_energy(
    time: Res<Time>,
    tuning: Res<BeamTuning>,
    abilities: Res<Abilities>,
    powerups: Res<PowerUpTuning>,
    held: Res<FireHeld>,
    mut energy: ResMut<BeamEnergy>,
) {
    let tuning = boosted_tuning(&tuning, *abilities, &powerups);
    let next = step_energy(*energy, held.0, &tuning, time.delta_secs());
    if *energy != next {
        *energy = next;
    }
}

/// While firing, apply damage ticks to every enemy the beams touch.
/// Defeating them (at zero health) is handled in `enemy`.
/// Whether a beam of `colour` breaks `block`: only its own split colour.
fn breaks(colour: BeamColour, block: &Breakable) -> bool {
    let primary = match colour {
        BeamColour::White => return false,
        BeamColour::Red => Primary::Red,
        BeamColour::Green => Primary::Green,
        BeamColour::Blue => Primary::Blue,
    };
    primary == block.colour
}

/// How near a beam must end to a block's face to have hit it.
const HIT_TOLERANCE: f32 = 1.0;

/// Whether a beam ending at `end` stopped on `aabb`.
fn touches(aabb: &Aabb, end: Vec2) -> bool {
    end.cmpge(aabb.min - HIT_TOLERANCE).all() && end.cmple(aabb.max + HIT_TOLERANCE).all()
}

pub fn damage_enemies(
    time: Res<Time>,
    mut events: MessageWriter<GameEvent>,
    tuning: Res<BeamTuning>,
    abilities: Res<Abilities>,
    powerups: Res<PowerUpTuning>,
    energy: Res<BeamEnergy>,
    mut timer: ResMut<DamageTimer>,
    heroes: Query<(&Position, &Facing), With<Hero>>,
    prisms: Query<&Position, With<Prism>>,
    solids: Query<(&Solid, Option<&Traits>)>,
    mut enemies: Query<(&Position, &mut Enemy)>,
    mut blocks: Query<&mut Breakable, With<Solid>>,
) {
    if !energy.firing {
        timer.0.reset();
        return;
    }
    let tuning = boosted_tuning(&tuning, *abilities, &powerups);
    let period = std::time::Duration::from_secs_f32(1.0 / tuning.damage_rate.max(0.1));
    if timer.0.duration() != period {
        timer.0.set_duration(period);
    }
    timer.0.tick(time.delta());
    let ticks = timer.0.times_finished_this_tick();
    let Ok((hero, facing)) = heroes.single() else {
        return;
    };
    if ticks == 0 {
        return;
    }
    let scene = RayScene::new(
        solids,
        enemies.iter().map(|(p, e)| (p.0, e.radius())).collect(),
    );
    let prism = prisms.single().ok().map(|p| p.0);
    let loadout = BeamLoadout::from_abilities(*abilities, &powerups);
    let trace = trace_from_hero(hero.0, *facing, prism, &scene, tuning.split_angle, &loadout);

    for mut block in &mut blocks {
        if trace
            .segments
            .iter()
            .any(|&(_, end, colour)| breaks(colour, &block) && touches(&block.aabb, end))
        {
            block.health = block.health.saturating_sub(ticks);
            events.write(GameEvent::BlockHit {
                at: block.aabb.center(),
            });
        }
    }

    // Same query, same order as the scene built above.
    let mut enemies: Vec<_> = enemies.iter_mut().collect();
    for i in trace.hit_targets {
        if let Some((pos, enemy)) = enemies.get_mut(i) {
            enemy.health = enemy.health.saturating_sub(ticks);
            enemy.flash = HIT_FLASH;
            events.write(GameEvent::EnemyHit { at: pos.0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::enemy::EnemyKind;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const SPLIT: f32 = 20.0;

    fn wall_at(x: f32) -> Aabb {
        Aabb::new(Vec2::new(x, -1000.0), Vec2::new(x + 50.0, 1000.0))
    }

    #[test]
    fn beam_stops_at_the_first_wall() {
        let scene = RayScene {
            mirrors: Vec::new(),
            solids: vec![wall_at(500.0), wall_at(300.0)],
            targets: vec![],
        };
        let trace = trace_beam(Vec2::ZERO, Vec2::X, Vec2::ZERO, None, &scene, SPLIT, false);
        assert_eq!(trace.segments.len(), 1);
        let (start, end, colour) = trace.segments[0];
        assert_eq!(start, Vec2::ZERO);
        assert!((end.x - 300.0).abs() < 1e-3);
        assert_eq!(colour, BeamColour::White);
        assert_eq!(trace.hit_targets, Vec::<usize>::new());
    }

    #[test]
    fn beam_hits_target_in_front_of_wall() {
        let scene = RayScene {
            mirrors: Vec::new(),
            solids: vec![wall_at(500.0)],
            targets: vec![
                (Vec2::new(200.0, 0.0), 20.0),
                (Vec2::new(200.0, 300.0), 20.0),
            ],
        };
        let trace = trace_beam(Vec2::ZERO, Vec2::X, Vec2::ZERO, None, &scene, SPLIT, false);
        assert_eq!(trace.hit_targets, vec![0]);
        assert!((trace.segments[0].1.x - 180.0).abs() < 1e-3);
    }

    #[test]
    fn mirrors_reflect_the_beam_by_its_angle() {
        // A mirror floor below a beam heading down and right: it comes back
        // up at the same angle and hits the target above.
        let floor = Aabb::new(Vec2::new(-1000.0, -200.0), Vec2::new(1000.0, -100.0));
        let scene = RayScene {
            mirrors: vec![floor],
            solids: vec![wall_at(600.0)],
            targets: vec![(Vec2::new(200.0, 0.0), 10.0)],
        };
        let dir = Vec2::new(1.0, -1.0).normalize();
        let trace = trace_beam(Vec2::ZERO, dir, Vec2::ZERO, None, &scene, SPLIT, false);
        assert_eq!(trace.segments.len(), 2);
        let bounce = trace.segments[0].1;
        assert!(
            (bounce - Vec2::new(100.0, -100.0)).length() < 0.1,
            "{bounce}"
        );
        let out = (trace.segments[1].1 - trace.segments[1].0).normalize();
        assert!((out - Vec2::new(1.0, 1.0).normalize()).length() < 1e-3);
        assert_eq!(trace.hit_targets, vec![0]);
    }

    #[test]
    fn beams_reflect_a_limited_number_of_times() {
        // Two facing mirrors catch the beam between them.
        let left = Aabb::new(Vec2::new(-200.0, -500.0), Vec2::new(-100.0, 500.0));
        let right = Aabb::new(Vec2::new(100.0, -500.0), Vec2::new(200.0, 500.0));
        let scene = RayScene {
            mirrors: vec![left, right],
            solids: vec![],
            targets: vec![],
        };
        let trace = trace_beam(Vec2::ZERO, Vec2::X, Vec2::ZERO, None, &scene, SPLIT, false);
        assert_eq!(trace.segments.len(), 1 + MAX_REFLECTIONS);
    }

    #[test]
    fn split_beams_reflect_too() {
        let prism = Vec2::new(90.0, 0.0);
        let scene = RayScene {
            mirrors: vec![wall_at(400.0)],
            solids: vec![],
            targets: vec![],
        };
        let trace = trace_beam(
            Vec2::new(24.0, 0.0),
            Vec2::X,
            Vec2::ZERO,
            Some(prism),
            &scene,
            SPLIT,
            false,
        );
        for colour in [BeamColour::Red, BeamColour::Green, BeamColour::Blue] {
            let pieces = trace.segments.iter().filter(|s| s.2 == colour).count();
            assert!(pieces >= 2, "{colour:?} has {pieces} pieces");
        }
    }

    #[test]
    fn scenes_sort_mirrors_from_other_solids() {
        let (plain, mirror) = (Solid(wall_at(0.0)), Solid(wall_at(100.0)));
        let traits = Traits {
            mirror: true,
            ..Traits::default()
        };
        let scene = RayScene::new([(&plain, None), (&mirror, Some(&traits))], vec![]);
        assert_eq!(scene.solids, vec![wall_at(0.0)]);
        assert_eq!(scene.mirrors, vec![wall_at(100.0)]);
    }

    #[test]
    fn beam_without_obstacles_is_capped() {
        let trace = trace_beam(
            Vec2::ZERO,
            Vec2::X,
            Vec2::ZERO,
            None,
            &RayScene::default(),
            SPLIT,
            false,
        );
        assert!((trace.segments[0].1.x - MAX_BEAM_LENGTH).abs() < 1e-3);
    }

    #[test]
    fn prism_in_the_line_of_fire_splits_into_three_colours() {
        let prism = Vec2::new(90.0, 0.0);
        let scene = RayScene {
            mirrors: Vec::new(),
            solids: vec![wall_at(1000.0)],
            targets: vec![],
        };
        let trace = trace_beam(
            Vec2::new(24.0, 0.0),
            Vec2::X,
            Vec2::ZERO,
            Some(prism),
            &scene,
            SPLIT,
            false,
        );

        let colours: Vec<_> = trace.segments.iter().map(|s| s.2).collect();
        assert_eq!(
            colours,
            vec![
                BeamColour::White,
                BeamColour::Red,
                BeamColour::Green,
                BeamColour::Blue
            ]
        );
        assert!((trace.segments[0].1.x - (90.0 - PRISM_RADIUS)).abs() < 1e-3);
        let dir = |i: usize| (trace.segments[i].1 - trace.segments[i].0).normalize();
        assert!(dir(2).abs_diff_eq(Vec2::X, 1e-4));
        assert!((dir(1).to_angle() - SPLIT.to_radians()).abs() < 1e-4);
        assert!((dir(3).to_angle() + SPLIT.to_radians()).abs() < 1e-4);
        for i in 1..=3 {
            assert!((trace.segments[i].1.x - 1000.0).abs() < 1e-2);
        }
    }

    #[test]
    fn split_fan_points_away_from_the_hero() {
        let hero = Vec2::ZERO;
        let prism = Vec2::new(88.0, 15.0);
        let trace = trace_beam(
            Vec2::new(24.0, 0.0),
            Vec2::X,
            hero,
            Some(prism),
            &RayScene::default(),
            SPLIT,
            false,
        );
        let green = trace.segments[2];
        let dir = (green.1 - green.0).normalize();
        assert!(dir.abs_diff_eq((prism - hero).normalize(), 1e-4));
    }

    #[test]
    fn split_beams_can_hit_targets() {
        let prism = Vec2::new(90.0, 0.0);
        let up = Vec2::from_angle(SPLIT.to_radians());
        let scene = RayScene {
            mirrors: Vec::new(),
            solids: vec![],
            targets: vec![(prism + up * 300.0, 20.0)],
        };
        let trace = trace_beam(
            Vec2::new(24.0, 0.0),
            Vec2::X,
            Vec2::ZERO,
            Some(prism),
            &scene,
            SPLIT,
            false,
        );
        assert_eq!(trace.hit_targets, vec![0]);
    }

    #[test]
    fn prism_out_of_line_is_ignored() {
        let trace = trace_beam(
            Vec2::new(24.0, 0.0),
            Vec2::X,
            Vec2::ZERO,
            Some(Vec2::new(0.0, 90.0)),
            &RayScene::default(),
            SPLIT,
            false,
        );
        assert_eq!(trace.segments.len(), 1);
    }

    #[test]
    fn trace_from_hero_starts_at_the_hero_edge_in_the_facing_direction() {
        let trace = trace_from_hero(
            Vec2::ZERO,
            Facing(-1),
            None,
            &RayScene::default(),
            SPLIT,
            &BeamLoadout::default(),
        );
        assert_eq!(trace.segments[0].0, Vec2::new(-HERO_RADIUS, 0.0));
        assert!(trace.segments[0].1.x < -1000.0);
    }

    #[test]
    fn piercing_beams_hit_every_target_up_to_the_wall() {
        let scene = RayScene {
            mirrors: Vec::new(),
            solids: vec![wall_at(500.0)],
            targets: vec![
                (Vec2::new(200.0, 0.0), 20.0),
                (Vec2::new(350.0, 0.0), 20.0),
                (Vec2::new(700.0, 0.0), 20.0),
            ],
        };
        let trace = trace_beam(Vec2::ZERO, Vec2::X, Vec2::ZERO, None, &scene, SPLIT, true);
        assert_eq!(trace.hit_targets, vec![0, 1]);
        assert!((trace.segments[0].1.x - 500.0).abs() < 1e-3);
    }

    #[test]
    fn loadout_follows_abilities() {
        let tuning = PowerUpTuning::default();
        assert_eq!(
            BeamLoadout::from_abilities(Abilities::default(), &tuning),
            BeamLoadout::default()
        );
        let mut abilities = Abilities::default();
        for power in [PowerUp::RearShot, PowerUp::Spread, PowerUp::Piercing] {
            abilities.grant(power);
        }
        let loadout = BeamLoadout::from_abilities(abilities, &tuning);
        assert!(loadout.rear && loadout.piercing);
        assert_eq!(loadout.spread, Some(tuning.spread_angle));
    }

    #[test]
    fn rear_shot_and_spread_add_beam_directions() {
        let facing = Facing(1);
        assert_eq!(BeamLoadout::default().directions(facing), vec![Vec2::X]);

        let rear = BeamLoadout {
            rear: true,
            ..default()
        };
        assert_eq!(rear.directions(facing), vec![Vec2::X, Vec2::NEG_X]);

        let both = BeamLoadout {
            rear: true,
            spread: Some(10.0),
            piercing: false,
        };
        let dirs = both.directions(facing);
        assert_eq!(dirs.len(), 6);
        assert_eq!(dirs.iter().filter(|d| d.x > 0.0).count(), 3);
        assert!((dirs[1].to_angle() - 10f32.to_radians()).abs() < 1e-4);
    }

    #[test]
    fn rear_shot_hits_targets_behind_and_each_target_counts_once() {
        let scene = RayScene {
            mirrors: Vec::new(),
            solids: vec![],
            targets: vec![
                (Vec2::new(-300.0, 0.0), 20.0),
                (Vec2::new(300.0, 0.0), 40.0),
            ],
        };
        let forward_only = trace_from_hero(
            Vec2::ZERO,
            Facing(1),
            None,
            &scene,
            SPLIT,
            &BeamLoadout::default(),
        );
        assert_eq!(forward_only.hit_targets, vec![1]);

        // Spread's three forward beams all reach the big target ahead.
        let loadout = BeamLoadout {
            rear: true,
            spread: Some(2.0),
            piercing: false,
        };
        let trace = trace_from_hero(Vec2::ZERO, Facing(1), None, &scene, SPLIT, &loadout);
        assert_eq!(trace.hit_targets, vec![0, 1]);
        assert_eq!(trace.segments.len(), 6);
    }

    #[test]
    fn rapid_fire_boosts_damage_and_recharge() {
        let base = BeamTuning::default();
        let powerups = PowerUpTuning::default();
        assert_eq!(boosted_tuning(&base, Abilities::default(), &powerups), base);
        let mut abilities = Abilities::default();
        abilities.grant(PowerUp::RapidFire);
        let boosted = boosted_tuning(&base, abilities, &powerups);
        assert!(boosted.damage_rate > base.damage_rate);
        assert!(boosted.recharge_rate > base.recharge_rate);
        assert!((boosted.drain_rate - base.drain_rate).abs() < f32::EPSILON);
    }

    const DT: f32 = 0.1;

    fn run(energy: BeamEnergy, held: bool, steps: usize, tuning: &BeamTuning) -> BeamEnergy {
        (0..steps).fold(energy, |e, _| step_energy(e, held, tuning, DT))
    }

    #[test]
    fn holding_fire_drains_energy_until_empty() {
        let tuning = BeamTuning::default();
        let start = BeamEnergy::default();
        let after = step_energy(start, true, &tuning, DT);
        assert!(after.firing);
        assert!((after.current - (tuning.max_energy - tuning.drain_rate * DT)).abs() < 1e-3);

        // Hold until the beam cuts out (about 2.9 s at the default drain).
        let mut empty = after;
        for _ in 0..100 {
            if !empty.firing {
                break;
            }
            empty = step_energy(empty, true, &tuning, DT);
        }
        assert!(!empty.firing);
        assert!(empty.current.abs() < f32::EPSILON);
        assert!(empty.needs_release);
    }

    #[test]
    fn running_dry_needs_a_release_before_firing_again() {
        let tuning = BeamTuning::default();
        let empty = run(BeamEnergy::default(), true, 40, &tuning);
        // Keep holding long enough to recharge past the minimum: still off.
        let held = run(empty, true, 30, &tuning);
        assert!(!held.firing);
        assert!(held.current >= tuning.min_energy_to_fire);
        // Release, then press again: fires.
        let released = step_energy(held, false, &tuning, DT);
        assert!(step_energy(released, true, &tuning, DT).firing);
    }

    #[test]
    fn starting_needs_the_minimum_energy() {
        let tuning = BeamTuning::default();
        let low = BeamEnergy {
            current: tuning.min_energy_to_fire - 1.0,
            ..default()
        };
        assert!(!step_energy(low, true, &tuning, DT).firing);
        let enough = BeamEnergy {
            current: tuning.min_energy_to_fire,
            ..default()
        };
        assert!(step_energy(enough, true, &tuning, DT).firing);
    }

    #[test]
    fn recharge_waits_for_the_delay_then_caps_at_max() {
        let tuning = BeamTuning::default();
        let fired = run(BeamEnergy::default(), true, 10, &tuning);
        let level = fired.current;
        // Just inside the delay: no recharge yet.
        let waiting = run(fired, false, 5, &tuning);
        assert!((waiting.current - level).abs() < f32::EPSILON);
        let recharged = run(fired, false, 200, &tuning);
        assert!((recharged.current - tuning.max_energy).abs() < f32::EPSILON);
    }

    fn sim_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins(SimulationPlugin);
        app
    }

    const COLUMN_HEALTH: u32 = 1000;

    /// A column of very tough enemies ahead of the hero, covering every height
    /// it bounces through, so the beam always has something to hit.
    fn spawn_enemy_column(app: &mut App) {
        for i in 0..7u8 {
            app.world_mut().spawn((
                Position(Vec2::new(900.0, 110.0 + f32::from(i) * 55.0)),
                Enemy {
                    kind: EnemyKind::Lens,
                    health: COLUMN_HEALTH,
                    flash: 0.0,
                },
            ));
        }
    }

    /// Total beam damage taken by the column so far.
    fn column_damage(app: &mut App) -> u32 {
        let world = app.world_mut();
        world
            .query::<&Enemy>()
            .iter(world)
            .filter(|e| e.health > COLUMN_HEALTH / 2)
            .map(|e| COLUMN_HEALTH - e.health)
            .sum()
    }

    #[test]
    fn nothing_is_hit_without_firing() {
        let mut app = sim_app();
        app.update();
        spawn_enemy_column(&mut app);
        for _ in 0..60 {
            app.update();
        }
        assert_eq!(column_damage(&mut app), 0);
        assert!(!app.world().resource::<BeamEnergy>().firing);
    }

    #[test]
    fn holding_space_damages_enemies_and_uses_energy() {
        let mut app = sim_app();
        app.update();
        spawn_enemy_column(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        for _ in 0..60 {
            app.update();
        }
        let damage = column_damage(&mut app);
        assert!(damage >= 5, "expected several damage ticks, got {damage}");
        let energy = *app.world().resource::<BeamEnergy>();
        assert!(energy.current < BeamTuning::default().max_energy);
    }

    #[test]
    fn only_a_block_s_own_split_colour_breaks_it_where_the_beam_stops() {
        let aabb = Aabb::new(Vec2::new(300.0, -50.0), Vec2::new(340.0, 50.0));
        let block = Breakable {
            aabb,
            colour: Primary::Green,
            health: 1,
        };
        assert!(breaks(BeamColour::Green, &block));
        assert!(!breaks(BeamColour::Red, &block));
        assert!(!breaks(BeamColour::White, &block));
        assert!(touches(&aabb, Vec2::new(300.0, 10.0)));
        assert!(!touches(&aabb, Vec2::new(250.0, 10.0)));
    }
}
