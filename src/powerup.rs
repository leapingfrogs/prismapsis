//! Power-ups: drops left by defeated enemies, and the abilities they grant.
//!
//! A defeated enemy sometimes leaves a pickup where it died. The pickup shows
//! which power-up it holds, stays put for a limited time, and grants its
//! ability when the hero touches it. Abilities last until the hero loses a
//! life. Simulation only; visuals live in `render`.

use bevy::prelude::*;

use crate::events::GameEvent;
use crate::hero::{HERO_RADIUS, Hero, Position};
use crate::light::Hidden;

/// Collision radius of a pickup.
pub const PICKUP_RADIUS: f32 = 26.0;

/// A pickup blinks for this long before it vanishes (seconds).
pub const PICKUP_WARNING: f32 = 2.0;

/// An ability the hero can collect. Level files name them in kebab case,
/// such as `rapid-fire`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PowerUp {
    /// No gravity: up/down move the hero directly.
    AntiGravity,
    /// A second beam fires backwards.
    RearShot,
    /// The beam fans out into three.
    Spread,
    /// Faster damage ticks and recharge.
    RapidFire,
    /// Beams pass through enemies.
    Piercing,
}

impl PowerUp {
    pub const ALL: [Self; 5] = [
        Self::AntiGravity,
        Self::RearShot,
        Self::Spread,
        Self::RapidFire,
        Self::Piercing,
    ];

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The abilities the hero currently has.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Abilities(u8);

impl Abilities {
    #[must_use]
    pub const fn has(self, power: PowerUp) -> bool {
        self.0 & power.bit() != 0
    }

    pub const fn grant(&mut self, power: PowerUp) {
        self.0 |= power.bit();
    }

    pub const fn revoke(&mut self, power: PowerUp) {
        self.0 &= !power.bit();
    }

    /// Abilities not collected yet, in `PowerUp::ALL` order.
    pub fn missing(self) -> impl Iterator<Item = PowerUp> {
        PowerUp::ALL.into_iter().filter(move |&p| !self.has(p))
    }
}

/// Tunable power-up parameters. Adjusted live from the dev tuning panel.
#[derive(Resource, Debug, Clone, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct PowerUpTuning {
    /// Chance (0 to 1) that a defeated enemy leaves a pickup.
    pub drop_chance: f32,
    /// How long a pickup stays before vanishing (seconds).
    pub drop_lifetime: f32,
    /// Rapid fire multiplies damage tick rate and recharge rate by this.
    pub rapid_fire_boost: f32,
    /// Angle between the centre beam and each outer beam of spread fire
    /// (degrees).
    pub spread_angle: f32,
}

impl Default for PowerUpTuning {
    fn default() -> Self {
        Self {
            drop_chance: 0.2,
            drop_lifetime: 8.0,
            rapid_fire_boost: 1.6,
            spread_angle: 12.0,
        }
    }
}

/// A power-up waiting to be collected.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Pickup {
    pub power: PowerUp,
    /// Seconds since it appeared.
    pub age: f32,
}

/// A pickup placed by the level rather than dropped: it never expires.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Placed;

/// Small, fast random number generator for drops (xorshift64*). Seeded with
/// a constant so tests are repeatable; the game reseeds it at start-up.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct DropRng(u64);

impl Default for DropRng {
    fn default() -> Self {
        Self::seeded(0x9E37_79B9_7F4A_7C15)
    }
}

impl DropRng {
    /// A generator from `seed`; zero (which would get stuck) is replaced.
    #[must_use]
    pub const fn seeded(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    /// Next value in `0.0..1.0`.
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40;
        // 24 random bits fit exactly in an f32 mantissa.
        #[allow(clippy::cast_precision_loss)]
        let value = bits as f32 / (1u32 << 24) as f32;
        value
    }
}

/// Which power-up a drop holds, from a `roll` in `0.0..1.0`: one the hero
/// doesn't have yet if possible, otherwise any.
#[must_use]
pub fn choose_power_up(abilities: Abilities, roll: f32) -> PowerUp {
    let missing: Vec<_> = abilities.missing().collect();
    let pool: &[PowerUp] = if missing.is_empty() {
        &PowerUp::ALL
    } else {
        &missing
    };
    // `roll` is below 1, so the index stays in range; `min` guards rounding.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let index = ((roll * pool.len() as f32) as usize).min(pool.len() - 1);
    pool[index]
}

/// Roll for a drop: `Some` power-up with probability `chance`.
pub fn roll_drop(rng: &mut DropRng, abilities: Abilities, chance: f32) -> Option<PowerUp> {
    (rng.next_f32() < chance).then(|| choose_power_up(abilities, rng.next_f32()))
}

/// Whether a pickup of this `age` is drawn this frame: steady, then blinking
/// faster and faster during its last `PICKUP_WARNING` seconds.
#[must_use]
pub fn pickup_shown(age: f32, lifetime: f32) -> bool {
    let left = lifetime - age;
    if left > PICKUP_WARNING {
        return true;
    }
    // Truncation is the point: alternate whole blink intervals, 4 to 12
    // blinks per second as time runs out.
    let rate = 12.0 - 8.0 * (left / PICKUP_WARNING).clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation)]
    let interval = (age * rate) as i64;
    interval % 2 == 0
}

#[derive(Debug, Default)]
pub struct PowerUpPlugin;

impl Plugin for PowerUpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PowerUpTuning>()
            .register_type::<PowerUpTuning>()
            .init_resource::<Abilities>()
            .init_resource::<DropRng>()
            .add_systems(
                FixedUpdate,
                (age_pickups, collect_pickups)
                    .chain()
                    .after(crate::enemy::reset_room),
            );
    }
}

/// The bundle for a pickup holding `power` at `at`.
#[must_use]
pub fn pickup_bundle(power: PowerUp, at: Vec2) -> impl Bundle {
    (
        Name::new(format!("Pickup {power:?}")),
        Pickup { power, age: 0.0 },
        Position(at),
    )
}

fn age_pickups(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<PowerUpTuning>,
    mut events: MessageWriter<GameEvent>,
    mut pickups: Query<(Entity, &mut Pickup, &Position), Without<Placed>>,
) {
    let warn_at = tuning.drop_lifetime - PICKUP_WARNING;
    for (entity, mut pickup, pos) in &mut pickups {
        let before = pickup.age;
        pickup.age += time.delta_secs();
        if before < warn_at && pickup.age >= warn_at {
            events.write(GameEvent::PickupExpiring { at: pos.0 });
        }
        if pickup.age >= tuning.drop_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

/// The hero (not the prism) collects pickups it touches.
fn collect_pickups(
    mut commands: Commands,
    mut abilities: ResMut<Abilities>,
    mut events: MessageWriter<GameEvent>,
    heroes: Query<&Position, With<Hero>>,
    pickups: Query<(Entity, &Pickup, &Position), (Without<Hero>, Without<Hidden>)>,
) {
    let Ok(hero) = heroes.single() else {
        return;
    };
    for (entity, pickup, pos) in &pickups {
        if hero.0.distance_squared(pos.0) < (HERO_RADIUS + PICKUP_RADIUS).powi(2) {
            abilities.grant(pickup.power);
            events.write(GameEvent::PickupCollected {
                power: pickup.power,
                at: pos.0,
            });
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::enemy::{Enemy, EnemyKind, Lives, ResetRoom, STARTING_LIVES};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn abilities_grant_revoke_and_list_missing() {
        let mut abilities = Abilities::default();
        assert_eq!(abilities.missing().count(), PowerUp::ALL.len());
        abilities.grant(PowerUp::Spread);
        abilities.grant(PowerUp::Spread);
        assert!(abilities.has(PowerUp::Spread));
        assert!(!abilities.has(PowerUp::Piercing));
        assert!(abilities.missing().all(|p| p != PowerUp::Spread));
        abilities.revoke(PowerUp::Spread);
        assert_eq!(abilities, Abilities::default());
    }

    #[test]
    fn rng_stays_in_range_and_varies() {
        let mut rng = DropRng::default();
        let values: Vec<f32> = (0..1000).map(|_| rng.next_f32()).collect();
        assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
        assert!(values.iter().any(|&v| v < 0.1));
        assert!(values.iter().any(|&v| v > 0.9));
        assert_eq!(DropRng::seeded(0), DropRng::seeded(1));
    }

    #[test]
    fn drops_favour_missing_abilities() {
        let mut abilities = Abilities::default();
        for power in PowerUp::ALL {
            if power != PowerUp::Piercing {
                abilities.grant(power);
            }
        }
        for roll in [0.0, 0.5, 0.999] {
            assert_eq!(choose_power_up(abilities, roll), PowerUp::Piercing);
        }
        abilities.grant(PowerUp::Piercing);
        // With everything collected, any power-up can drop.
        assert_eq!(choose_power_up(abilities, 0.0), PowerUp::ALL[0]);
        assert_eq!(choose_power_up(abilities, 0.999), PowerUp::ALL[4]);
    }

    #[test]
    fn drop_chance_controls_how_often_drops_happen() {
        let mut rng = DropRng::default();
        let abilities = Abilities::default();
        assert!((0..100).all(|_| roll_drop(&mut rng, abilities, 0.0).is_none()));
        assert!((0..100).all(|_| roll_drop(&mut rng, abilities, 1.0).is_some()));
        let drops = (0..1000)
            .filter(|_| roll_drop(&mut rng, abilities, 0.2).is_some())
            .count();
        assert!((120..280).contains(&drops), "got {drops} drops");
    }

    #[test]
    fn pickups_blink_only_near_the_end() {
        assert!(pickup_shown(0.0, 8.0));
        assert!(pickup_shown(5.9, 8.0));
        let late: Vec<bool> = (0..40u8)
            .map(|i| pickup_shown(6.0 + f32::from(i) * 0.05, 8.0))
            .collect();
        assert!(late.contains(&true));
        assert!(late.contains(&false));
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

    /// Pickups dropped by enemies, leaving out the level's placed ones.
    fn dropped(app: &mut App) -> usize {
        let world = app.world_mut();
        world
            .query_filtered::<(), (With<Pickup>, Without<Placed>)>()
            .iter(world)
            .count()
    }

    fn hero_position(app: &mut App) -> Vec2 {
        let world = app.world_mut();
        world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0
    }

    #[test]
    fn defeated_enemies_can_leave_a_pickup_where_they_died() {
        let mut app = sim_app();
        app.world_mut().resource_mut::<PowerUpTuning>().drop_chance = 1.0;
        let at = Vec2::new(3000.0, 900.0);
        app.world_mut().spawn((
            Enemy {
                kind: EnemyKind::Mote,
                health: 0,
                flash: 0.0,
            },
            Position(at),
        ));
        app.update();
        let world = app.world_mut();
        let (pickup, pos) = world
            .query_filtered::<(&Pickup, &Position), Without<Placed>>()
            .single(world)
            .unwrap();
        let (pickup, pos) = (*pickup, pos.0);
        assert_eq!(pos, at);
        // Fresh hero: the drop is something it doesn't have.
        assert!(!app.world().resource::<Abilities>().has(pickup.power));
    }

    #[test]
    fn no_pickup_when_the_drop_chance_is_zero() {
        let mut app = sim_app();
        app.world_mut().resource_mut::<PowerUpTuning>().drop_chance = 0.0;
        app.world_mut().spawn((
            Enemy {
                kind: EnemyKind::Mote,
                health: 0,
                flash: 0.0,
            },
            Position(Vec2::new(3000.0, 900.0)),
        ));
        app.update();
        assert_eq!(dropped(&mut app), 0);
    }

    #[test]
    fn touching_a_pickup_grants_its_ability() {
        let mut app = sim_app();
        let hero = hero_position(&mut app);
        app.world_mut()
            .spawn(pickup_bundle(PowerUp::RearShot, hero));
        app.update();
        assert!(app.world().resource::<Abilities>().has(PowerUp::RearShot));
        assert_eq!(dropped(&mut app), 0);
    }

    #[test]
    fn uncollected_pickups_vanish_after_their_lifetime() {
        let mut app = sim_app();
        app.world_mut()
            .spawn(pickup_bundle(PowerUp::Spread, Vec2::new(4000.0, 900.0)));
        app.update();
        assert_eq!(dropped(&mut app), 1);
        let frames = (PowerUpTuning::default().drop_lifetime / 0.016).ceil();
        // Small positive whole number.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = frames as usize;
        for _ in 0..frames + 5 {
            app.update();
        }
        assert_eq!(dropped(&mut app), 0);
    }

    #[test]
    fn losing_a_life_loses_every_ability() {
        let mut app = sim_app();
        app.world_mut()
            .resource_mut::<Abilities>()
            .grant(PowerUp::Piercing);
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
        assert_eq!(*app.world().resource::<Abilities>(), Abilities::default());
    }

    #[test]
    fn resetting_the_room_clears_pickups_and_abilities() {
        let mut app = sim_app();
        app.world_mut()
            .resource_mut::<Abilities>()
            .grant(PowerUp::AntiGravity);
        app.world_mut()
            .spawn(pickup_bundle(PowerUp::Spread, Vec2::new(4000.0, 900.0)));
        app.insert_resource(ResetRoom(true));
        app.update();
        assert_eq!(dropped(&mut app), 0);
        assert_eq!(*app.world().resource::<Abilities>(), Abilities::default());
    }
}
