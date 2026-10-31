//! Light and the spectrum.
//!
//! Defeated enemies release light of their colour, filling a gauge per
//! primary. A full gauge restores that colour; a secondary is restored with both of its primaries, and white with all
//! three. Restored colours make phantom platforms solid, reveal hidden
//! things, and (white) open the level's exit. Simulation only; visuals live
//! in `spectrum_view`.

use bevy::prelude::*;

use crate::enemy::ResetRoom;
use crate::events::GameEvent;
use crate::hero::{HERO_RADIUS, Hero, Position, move_hero};
use crate::level::Solid;
use crate::physics::Aabb;

/// A primary colour of light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum Primary {
    Red,
    Green,
    Blue,
}

impl Primary {
    pub const ALL: [Self; 3] = [Self::Red, Self::Green, Self::Blue];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Any colour a world element can be: a primary, a secondary mix, or white.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum Hue {
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    White,
}

impl Hue {
    pub const ALL: [Self; 7] = [
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Yellow,
        Self::Cyan,
        Self::Magenta,
        Self::White,
    ];

    /// The primaries that mix to make this hue.
    #[must_use]
    pub const fn primaries(self) -> &'static [Primary] {
        match self {
            Self::Red => &[Primary::Red],
            Self::Green => &[Primary::Green],
            Self::Blue => &[Primary::Blue],
            Self::Yellow => &[Primary::Red, Primary::Green],
            Self::Cyan => &[Primary::Green, Primary::Blue],
            Self::Magenta => &[Primary::Red, Primary::Blue],
            Self::White => &[Primary::Red, Primary::Green, Primary::Blue],
        }
    }
}

impl From<Primary> for Hue {
    fn from(primary: Primary) -> Self {
        match primary {
            Primary::Red => Self::Red,
            Primary::Green => Self::Green,
            Primary::Blue => Self::Blue,
        }
    }
}

/// Tunable light parameters. Adjusted live from the dev tuning panel.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct LightTuning {
    /// Light needed to restore a colour.
    pub gauge_capacity: f32,
    pub mote_light: f32,
    pub lens_light: f32,
}

impl Default for LightTuning {
    fn default() -> Self {
        Self {
            gauge_capacity: 10.0,
            mote_light: 1.0,
            lens_light: 4.0,
        }
    }
}

/// Light gathered towards each primary, and which are restored. Per level:
/// it resets with the room.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct Spectrum {
    pub gauges: [f32; 3],
    pub restored: [bool; 3],
}

impl Spectrum {
    #[must_use]
    pub fn is_restored(&self, hue: Hue) -> bool {
        hue.primaries().iter().all(|p| self.restored[p.index()])
    }

    /// How full `primary`'s gauge is, from 0 to 1.
    #[must_use]
    pub fn fill(&self, primary: Primary, capacity: f32) -> f32 {
        (self.gauges[primary.index()] / capacity.max(f32::EPSILON)).clamp(0.0, 1.0)
    }

    /// Add light to `primary`'s gauge, returning every hue this restores
    /// (the primary, and any secondary or white it completes).
    pub fn absorb(&mut self, primary: Primary, amount: f32, capacity: f32) -> Vec<Hue> {
        let i = primary.index();
        if self.restored[i] {
            return Vec::new();
        }
        let before = *self;
        self.gauges[i] = (self.gauges[i] + amount).min(capacity);
        if self.gauges[i] >= capacity {
            self.restored[i] = true;
        }
        Hue::ALL
            .into_iter()
            .filter(|&hue| self.is_restored(hue) && !before.is_restored(hue))
            .collect()
    }

    /// Losing a life halves every gauge that isn't restored yet.
    pub fn halve_partial(&mut self) {
        for (gauge, restored) in self.gauges.iter_mut().zip(self.restored) {
            if !restored {
                *gauge /= 2.0;
            }
        }
    }

    /// Restore `primary` outright (dev tools).
    pub fn restore(&mut self, primary: Primary, capacity: f32) -> Vec<Hue> {
        self.absorb(primary, capacity, capacity)
    }
}

/// The colour of light an enemy releases when defeated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Colour(pub Primary);

/// A platform that is only an outline until its hue is restored, then
/// solid. Solidity is the `Solid` component, added and removed to match.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Phantom {
    pub hue: Hue,
    pub aabb: Aabb,
}

/// The hue a solid or piece of scenery takes on as colour returns.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Shade(pub Hue);

/// Invisible, and ignored by the game, until its hue is restored.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Hidden(pub Hue);

/// The level's exit: open once white is restored; reaching it completes the
/// level.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct ExitGate(pub Aabb);

/// Levels completed this session.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LevelsCompleted(pub u32);

#[derive(Debug, Default)]
pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightTuning>()
            .register_type::<LightTuning>()
            .init_resource::<Spectrum>()
            .init_resource::<LevelsCompleted>()
            .add_systems(
                FixedUpdate,
                (
                    (solidify_phantoms, reveal_hidden).before(move_hero),
                    reach_exit.after(move_hero).before(crate::enemy::reset_room),
                ),
            );
    }
}

fn overlaps(a: &Aabb, b: &Aabb) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

fn hero_box(at: Vec2) -> Aabb {
    Aabb::from_center(at, Vec2::splat(HERO_RADIUS))
}

/// Phantoms are solid exactly while their hue is restored. A platform waits
/// to turn solid until the hero is clear of it, so it can't trap the hero.
fn solidify_phantoms(
    mut commands: Commands,
    spectrum: Res<Spectrum>,
    heroes: Query<&Position, With<Hero>>,
    phantoms: Query<(Entity, &Phantom, Has<Solid>)>,
) {
    let hero = heroes.single().ok().map(|p| hero_box(p.0));
    for (entity, phantom, solid) in &phantoms {
        let restored = spectrum.is_restored(phantom.hue);
        if restored && !solid && !hero.is_some_and(|h| overlaps(&h, &phantom.aabb)) {
            commands.entity(entity).insert(Solid(phantom.aabb));
        } else if !restored && solid {
            commands.entity(entity).remove::<Solid>();
        }
    }
}

fn reveal_hidden(
    mut commands: Commands,
    spectrum: Res<Spectrum>,
    hidden: Query<(Entity, &Hidden)>,
) {
    for (entity, hidden) in &hidden {
        if spectrum.is_restored(hidden.0) {
            commands.entity(entity).remove::<Hidden>();
        }
    }
}

/// With white restored, touching the exit completes the level; `level` then
/// moves on to the next one.
pub fn reach_exit(
    spectrum: Res<Spectrum>,
    mut reset: ResMut<ResetRoom>,
    mut completed: ResMut<LevelsCompleted>,
    mut events: MessageWriter<GameEvent>,
    heroes: Query<&Position, With<Hero>>,
    gates: Query<&ExitGate>,
) {
    if !spectrum.is_restored(Hue::White) || reset.0 {
        return;
    }
    let Ok(hero) = heroes.single() else {
        return;
    };
    if gates
        .iter()
        .any(|gate| overlaps(&hero_box(hero.0), &gate.0))
    {
        completed.0 += 1;
        reset.0 = true;
        events.write(GameEvent::ExitReached { at: hero.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::enemy::{Enemy, EnemyKind, Lives, STARTING_LIVES, WaveTimer};
    use crate::powerup::{Abilities, Pickup};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const CAP: f32 = 10.0;

    #[test]
    fn secondaries_and_white_need_their_primaries() {
        let mut spectrum = Spectrum::default();
        assert!(Hue::ALL.iter().all(|&h| !spectrum.is_restored(h)));
        assert_eq!(spectrum.absorb(Primary::Red, 4.0, CAP), vec![]);
        assert!((spectrum.fill(Primary::Red, CAP) - 0.4).abs() < 1e-6);
        assert_eq!(spectrum.absorb(Primary::Red, 7.0, CAP), vec![Hue::Red]);
        assert!((spectrum.gauges[0] - CAP).abs() < f32::EPSILON);
        // Already restored: more light changes nothing.
        assert_eq!(spectrum.absorb(Primary::Red, 5.0, CAP), vec![]);

        assert_eq!(
            spectrum.restore(Primary::Green, CAP),
            vec![Hue::Green, Hue::Yellow]
        );
        assert!(!spectrum.is_restored(Hue::Cyan));
        assert_eq!(
            spectrum.restore(Primary::Blue, CAP),
            vec![Hue::Blue, Hue::Cyan, Hue::Magenta, Hue::White]
        );
    }

    #[test]
    fn halving_spares_restored_colours() {
        let mut spectrum = Spectrum::default();
        spectrum.restore(Primary::Red, CAP);
        spectrum.absorb(Primary::Blue, 6.0, CAP);
        spectrum.halve_partial();
        assert!(spectrum.is_restored(Hue::Red));
        assert!((spectrum.gauges[0] - CAP).abs() < f32::EPSILON);
        assert!((spectrum.gauges[2] - 3.0).abs() < f32::EPSILON);
    }

    fn sim_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins(SimulationPlugin);
        app.update();
        app
    }

    fn hero_position(app: &mut App) -> Vec2 {
        let world = app.world_mut();
        world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0
    }

    fn defeat(app: &mut App, kind: EnemyKind, colour: Primary) {
        app.world_mut().spawn((
            Enemy {
                kind,
                health: 0,
                flash: 0.0,
            },
            Colour(colour),
            Position(Vec2::new(4000.0, 900.0)),
        ));
        app.update();
    }

    #[test]
    fn defeated_enemies_feed_their_colour() {
        let mut app = sim_app();
        defeat(&mut app, EnemyKind::Mote, Primary::Green);
        defeat(&mut app, EnemyKind::Lens, Primary::Green);
        let tuning = LightTuning::default();
        let spectrum = *app.world().resource::<Spectrum>();
        assert!((spectrum.gauges[1] - (tuning.mote_light + tuning.lens_light)).abs() < 1e-6);
        assert!(spectrum.gauges[0].abs() < f32::EPSILON);
    }

    #[test]
    fn losing_a_life_halves_partial_gauges() {
        let mut app = sim_app();
        app.world_mut().resource_mut::<Spectrum>().gauges = [8.0, 0.0, 0.0];
        let hero = hero_position(&mut app);
        app.world_mut().spawn((
            Enemy {
                kind: EnemyKind::Mote,
                health: 5,
                flash: 0.0,
            },
            Position(hero),
        ));
        app.update();
        assert_eq!(app.world().resource::<Lives>().0, STARTING_LIVES - 1);
        assert!((app.world().resource::<Spectrum>().gauges[0] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn waves_cycle_through_the_primaries() {
        let mut app = sim_app();
        let mut seen = Vec::new();
        for _ in 0..3 {
            app.world_mut()
                .resource_mut::<WaveTimer>()
                .timer
                .set_elapsed(Duration::from_secs(60));
            app.update();
            let world = app.world_mut();
            let mut colours: Vec<Primary> = world
                .query_filtered::<&Colour, With<crate::enemy::Mote>>()
                .iter(world)
                .map(|c| c.0)
                .collect();
            colours.dedup();
            seen.push(colours);
            let motes: Vec<Entity> = world
                .query_filtered::<Entity, With<crate::enemy::Mote>>()
                .iter(world)
                .collect();
            for mote in motes {
                world.despawn(mote);
            }
        }
        assert_eq!(
            seen,
            vec![
                vec![Primary::Red],
                vec![Primary::Green],
                vec![Primary::Blue]
            ]
        );
    }

    fn phantom_solid(app: &mut App, hue: Hue) -> bool {
        let world = app.world_mut();
        world
            .query::<(&Phantom, Has<Solid>)>()
            .iter(world)
            .find(|(p, _)| p.hue == hue)
            .unwrap()
            .1
    }

    #[test]
    fn phantoms_turn_solid_when_restored_and_back_on_reset() {
        let mut app = sim_app();
        assert!(!phantom_solid(&mut app, Hue::Red));
        app.world_mut()
            .resource_mut::<Spectrum>()
            .restore(Primary::Red, CAP);
        app.update();
        assert!(phantom_solid(&mut app, Hue::Red));
        assert!(!phantom_solid(&mut app, Hue::Yellow));

        app.insert_resource(ResetRoom(true));
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<Spectrum>(), Spectrum::default());
        assert!(!phantom_solid(&mut app, Hue::Red));
    }

    #[test]
    fn phantoms_wait_for_the_hero_to_move_clear() {
        let mut app = sim_app();
        let hero = hero_position(&mut app);
        let aabb = Aabb::from_center(hero, Vec2::splat(60.0));
        let phantom = app
            .world_mut()
            .spawn(Phantom {
                hue: Hue::Blue,
                aabb,
            })
            .id();
        app.world_mut()
            .resource_mut::<Spectrum>()
            .restore(Primary::Blue, CAP);
        app.update();
        assert!(app.world().get::<Solid>(phantom).is_none());
    }

    #[test]
    fn hidden_pickups_appear_and_can_be_collected_once_restored() {
        let mut app = sim_app();
        let hero = hero_position(&mut app);
        let pickup = app
            .world_mut()
            .spawn((
                crate::powerup::pickup_bundle(crate::powerup::PowerUp::Spread, hero),
                Hidden(Hue::Cyan),
            ))
            .id();
        app.update();
        assert!(
            app.world().get::<Pickup>(pickup).is_some(),
            "not collectable"
        );
        let mut spectrum = app.world_mut().resource_mut::<Spectrum>();
        spectrum.restore(Primary::Green, CAP);
        spectrum.restore(Primary::Blue, CAP);
        app.update();
        app.update();
        assert!(
            app.world()
                .resource::<Abilities>()
                .has(crate::powerup::PowerUp::Spread)
        );
    }

    fn gate(app: &mut App) -> Aabb {
        let world = app.world_mut();
        world.query::<&ExitGate>().single(world).unwrap().0
    }

    fn put_hero_at(app: &mut App, at: Vec2) {
        let world = app.world_mut();
        let mut hero = world
            .query_filtered::<&mut Position, With<Hero>>()
            .single_mut(world)
            .unwrap();
        hero.0 = at;
    }

    #[test]
    fn the_exit_opens_with_white_and_completes_the_level() {
        let mut app = sim_app();
        let exit = gate(&mut app).center();
        put_hero_at(&mut app, exit);
        app.update();
        assert_eq!(app.world().resource::<LevelsCompleted>().0, 0, "closed");

        let mut spectrum = app.world_mut().resource_mut::<Spectrum>();
        for primary in Primary::ALL {
            spectrum.restore(primary, CAP);
        }
        put_hero_at(&mut app, exit);
        app.update();
        assert_eq!(app.world().resource::<LevelsCompleted>().0, 1);
        // The room restarts with a fresh spectrum.
        app.update();
        assert_eq!(*app.world().resource::<Spectrum>(), Spectrum::default());
    }
}
