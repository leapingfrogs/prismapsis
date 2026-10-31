//! The hero's laser beam, its energy, and how it splits through the prism.
//!
//! Holding fire keeps a continuous beam on while energy lasts. The beam is a
//! ray from the hero to the first thing it hits; crossing the prism splits it
//! into red, green and blue beams fanning out away from the hero. Simulation
//! only; visuals live in `render`.

use bevy::prelude::*;

use crate::enemy::{Enemy, HIT_FLASH};
use crate::hero::{Facing, HERO_RADIUS, Hero, Position, update_facing};
use crate::level::Solid;
use crate::physics::{Aabb, ray_aabb, ray_circle};
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

/// Things a beam can hit.
#[derive(Debug, Default)]
pub struct RayScene {
    pub solids: Vec<Aabb>,
    /// Target centres and radii.
    pub targets: Vec<(Vec2, f32)>,
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
    Target(usize),
    Prism,
}

/// Nearest thing along a ray, and the distance to it.
fn nearest_hit(origin: Vec2, dir: Vec2, scene: &RayScene, prism: Option<Vec2>) -> (f32, Hit) {
    let mut best = (MAX_BEAM_LENGTH, Hit::Nothing);
    let mut consider = |t: Option<f32>, hit| {
        if let Some(t) = t.filter(|&t| t < best.0) {
            best = (t, hit);
        }
    };
    for solid in &scene.solids {
        consider(ray_aabb(origin, dir, solid), Hit::Solid);
    }
    for (i, &(center, radius)) in scene.targets.iter().enumerate() {
        consider(ray_circle(origin, dir, center, radius), Hit::Target(i));
    }
    if let Some(center) = prism {
        consider(ray_circle(origin, dir, center, PRISM_RADIUS), Hit::Prism);
    }
    best
}

/// Trace the beam fired from `origin` along `dir` (normalised).
///
/// If it crosses the prism at `prism`, the white beam ends there and three
/// coloured beams fan out from the prism's far side, centred on the
/// direction from `hero` to the prism. Split beams don't re-enter the prism.
#[must_use]
pub fn trace_beam(
    origin: Vec2,
    dir: Vec2,
    hero: Vec2,
    prism: Option<Vec2>,
    scene: &RayScene,
    split_angle: f32,
) -> BeamTrace {
    let mut trace = BeamTrace::default();
    let (t, hit) = nearest_hit(origin, dir, scene, prism);
    trace
        .segments
        .push((origin, origin + dir * t, BeamColour::White));
    match hit {
        Hit::Target(i) => trace.hit_targets.push(i),
        Hit::Prism => {
            let Some(center) = prism else {
                return trace;
            };
            let outward = (center - hero).normalize_or(dir);
            let spread = split_angle.to_radians();
            for (angle, colour) in [
                (spread, BeamColour::Red),
                (0.0, BeamColour::Green),
                (-spread, BeamColour::Blue),
            ] {
                let split_dir = Vec2::from_angle(angle).rotate(outward);
                let start = center + split_dir * PRISM_RADIUS;
                let (t, hit) = nearest_hit(start, split_dir, scene, None);
                trace.segments.push((start, start + split_dir * t, colour));
                if let Hit::Target(i) = hit {
                    trace.hit_targets.push(i);
                }
            }
        }
        Hit::Nothing | Hit::Solid => {}
    }
    trace
}

/// Trace the hero's beam from a hero at `hero` facing `facing`.
#[must_use]
pub fn trace_from_hero(
    hero: Vec2,
    facing: Facing,
    prism: Option<Vec2>,
    scene: &RayScene,
    split_angle: f32,
) -> BeamTrace {
    let dir = facing.direction();
    trace_beam(
        hero + dir * HERO_RADIUS,
        dir,
        hero,
        prism,
        scene,
        split_angle,
    )
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
            .add_systems(Update, fire_input)
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
    held: Res<FireHeld>,
    mut energy: ResMut<BeamEnergy>,
) {
    let next = step_energy(*energy, held.0, &tuning, time.delta_secs());
    if *energy != next {
        *energy = next;
    }
}

/// While firing, apply damage ticks to every enemy the beam touches.
/// Defeating them (at zero health) is handled in `enemy`.
pub fn damage_enemies(
    time: Res<Time>,
    tuning: Res<BeamTuning>,
    energy: Res<BeamEnergy>,
    mut timer: ResMut<DamageTimer>,
    heroes: Query<(&Position, &Facing), With<Hero>>,
    prisms: Query<&Position, With<Prism>>,
    solids: Query<&Solid>,
    mut enemies: Query<(&Position, &mut Enemy)>,
) {
    if !energy.firing {
        timer.0.reset();
        return;
    }
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
    let scene = RayScene {
        solids: solids.iter().map(|s| s.0).collect(),
        targets: enemies.iter().map(|(p, e)| (p.0, e.radius())).collect(),
    };
    let prism = prisms.single().ok().map(|p| p.0);
    let trace = trace_from_hero(hero.0, *facing, prism, &scene, tuning.split_angle);

    // Same query, same order as the scene built above.
    let mut enemies: Vec<_> = enemies.iter_mut().map(|(_, e)| e).collect();
    for i in trace.hit_targets {
        if let Some(enemy) = enemies.get_mut(i) {
            enemy.health = enemy.health.saturating_sub(ticks);
            enemy.flash = HIT_FLASH;
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
            solids: vec![wall_at(500.0), wall_at(300.0)],
            targets: vec![],
        };
        let trace = trace_beam(Vec2::ZERO, Vec2::X, Vec2::ZERO, None, &scene, SPLIT);
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
            solids: vec![wall_at(500.0)],
            targets: vec![
                (Vec2::new(200.0, 0.0), 20.0),
                (Vec2::new(200.0, 300.0), 20.0),
            ],
        };
        let trace = trace_beam(Vec2::ZERO, Vec2::X, Vec2::ZERO, None, &scene, SPLIT);
        assert_eq!(trace.hit_targets, vec![0]);
        assert!((trace.segments[0].1.x - 180.0).abs() < 1e-3);
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
        );
        assert!((trace.segments[0].1.x - MAX_BEAM_LENGTH).abs() < 1e-3);
    }

    #[test]
    fn prism_in_the_line_of_fire_splits_into_three_colours() {
        let prism = Vec2::new(90.0, 0.0);
        let scene = RayScene {
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
        );
        assert_eq!(trace.segments.len(), 1);
    }

    #[test]
    fn trace_from_hero_starts_at_the_hero_edge_in_the_facing_direction() {
        let trace = trace_from_hero(Vec2::ZERO, Facing(-1), None, &RayScene::default(), SPLIT);
        assert_eq!(trace.segments[0].0, Vec2::new(-HERO_RADIUS, 0.0));
        assert!(trace.segments[0].1.x < -1000.0);
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
}
