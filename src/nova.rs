//! Light nova: a power-up whose burst of light clears enemies near the
//! hero, then recharges.
//!
//! N (gamepad North) sets it off. A ring of light spreads from where the
//! hero was, defeating motes and orbs it reaches and taking half a lens's
//! health, and the nova can't be used again until it has recharged.
//! Simulation only; visuals live in `render`.

use bevy::prelude::*;

use crate::beam::damage_enemies;
use crate::enemy::{Enemy, EnemyKind, EnemyTuning, HIT_FLASH, Orb};
use crate::events::GameEvent;
use crate::hero::{Hero, Position};
use crate::powerup::{Abilities, PowerUp};
use crate::prism::orbit_prism;

/// How far the burst reaches from the hero (world units).
pub const NOVA_REACH: f32 = 350.0;

/// Seconds the burst takes to spread to its full reach.
pub const NOVA_SPREAD: f32 = 0.3;

/// Seconds before the nova can be used again.
pub const NOVA_RECHARGE: f32 = 25.0;

/// The nova's state: how long until it's ready, and the burst spreading
/// now, if any.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Nova {
    /// Seconds until it can be used again; none left when ready.
    pub recharge: f32,
    pub burst: Option<NovaBurst>,
}

impl Nova {
    /// Whether it can be set off now.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.recharge <= 0.0 && self.burst.is_none()
    }
}

/// A burst of light spreading from where the nova was set off.
#[derive(Debug, Clone, PartialEq)]
pub struct NovaBurst {
    pub centre: Vec2,
    /// Seconds since it was set off.
    pub age: f32,
    /// Enemies it has already reached, so each is hit once.
    hit: Vec<Entity>,
}

impl NovaBurst {
    /// How far it has spread.
    #[must_use]
    pub fn radius(&self) -> f32 {
        NOVA_REACH * (self.age / NOVA_SPREAD).min(1.0)
    }
}

/// The player asked for the nova since the last physics step.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NovaRequest(pub bool);

#[derive(Debug, Default)]
pub struct NovaPlugin;

impl Plugin for NovaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Nova>()
            .init_resource::<NovaRequest>()
            .add_systems(Update, nova_input.run_if(crate::pause::running))
            .add_systems(
                FixedUpdate,
                (charge_on_collect, recharge_nova, set_off_nova, spread_nova)
                    .chain()
                    .after(orbit_prism)
                    .before(damage_enemies),
            );
    }
}

/// N (gamepad North) asks for the nova.
fn nova_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut request: ResMut<NovaRequest>,
) {
    if keys.just_pressed(KeyCode::KeyN)
        || gamepads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::North))
    {
        request.0 = true;
    }
}

/// A freshly collected nova is ready at once.
fn charge_on_collect(mut events: MessageReader<GameEvent>, mut nova: ResMut<Nova>) {
    for event in events.read() {
        if let GameEvent::PickupCollected {
            power: PowerUp::LightNova,
            ..
        } = event
        {
            nova.recharge = 0.0;
        }
    }
}

fn recharge_nova(
    time: Res<Time>,
    abilities: Res<Abilities>,
    heroes: Query<&Position, With<Hero>>,
    mut nova: ResMut<Nova>,
    mut events: MessageWriter<GameEvent>,
) {
    if nova.recharge <= 0.0 {
        return;
    }
    nova.recharge -= time.delta_secs();
    if nova.recharge <= 0.0 && abilities.has(PowerUp::LightNova) {
        let at = heroes.single().map_or(Vec2::ZERO, |hero| hero.0);
        events.write(GameEvent::NovaReady { at });
    }
}

/// Set the nova off if asked for, held and ready.
fn set_off_nova(
    abilities: Res<Abilities>,
    heroes: Query<&Position, With<Hero>>,
    mut request: ResMut<NovaRequest>,
    mut nova: ResMut<Nova>,
    mut events: MessageWriter<GameEvent>,
) {
    if !std::mem::take(&mut request.0) || !abilities.has(PowerUp::LightNova) || !nova.ready() {
        return;
    }
    let Ok(hero) = heroes.single() else {
        return;
    };
    nova.recharge = NOVA_RECHARGE;
    nova.burst = Some(NovaBurst {
        centre: hero.0,
        age: 0.0,
        hit: Vec::new(),
    });
    events.write(GameEvent::NovaFired { at: hero.0 });
}

/// Spread the burst: each enemy it reaches is hit once, motes defeated and
/// lenses losing half their health, and orbs it reaches are put out.
fn spread_nova(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<EnemyTuning>,
    mut nova: ResMut<Nova>,
    mut enemies: Query<(Entity, &Position, &mut Enemy)>,
    orbs: Query<(Entity, &Position), With<Orb>>,
) {
    let Some(burst) = nova.burst.as_mut() else {
        return;
    };
    burst.age += time.delta_secs();
    let radius = burst.radius();
    for (entity, pos, mut enemy) in &mut enemies {
        if burst.hit.contains(&entity) || pos.0.distance(burst.centre) > radius + enemy.radius() {
            continue;
        }
        burst.hit.push(entity);
        enemy.health = match enemy.kind {
            EnemyKind::Mote => 0,
            EnemyKind::Lens => enemy.health.saturating_sub(tuning.lens_health.div_ceil(2)),
        };
        enemy.flash = HIT_FLASH;
    }
    for (entity, pos) in &orbs {
        if pos.0.distance(burst.centre) <= radius {
            commands.entity(entity).despawn();
        }
    }
    if burst.age >= NOVA_SPREAD {
        nova.burst = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemy::Lens;
    use crate::hero::PreviousPosition;
    use crate::light::Primary;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                1.0 / crate::PHYSICS_HZ,
            )))
            .add_plugins(crate::SimulationPlugin);
        app.update();
        app
    }

    fn hero(app: &mut App) -> Vec2 {
        let world = app.world_mut();
        world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0
    }

    fn enemy(kind: EnemyKind, health: u32, at: Vec2) -> impl Bundle {
        (
            Enemy {
                kind,
                health,
                flash: 0.0,
            },
            Position(at),
            PreviousPosition(at),
        )
    }

    /// Tap N for one frame.
    fn press_n(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyN);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyN);
        keys.clear();
    }

    #[test]
    fn the_nova_clears_motes_and_orbs_in_reach_and_wounds_lenses() {
        let mut app = app();
        let hero = hero(&mut app);
        let lens_health = EnemyTuning::default().lens_health;
        let near = app
            .world_mut()
            .spawn(enemy(EnemyKind::Mote, 5, hero + Vec2::new(200.0, 0.0)))
            .id();
        let far = app
            .world_mut()
            .spawn(enemy(EnemyKind::Mote, 5, hero + Vec2::new(600.0, 0.0)))
            .id();
        let lens = app
            .world_mut()
            .spawn((
                enemy(EnemyKind::Lens, lens_health, hero + Vec2::new(0.0, 250.0)),
                Lens { cooldown: 99.0 },
                crate::light::Colour(Primary::Red),
            ))
            .id();
        let orb = app
            .world_mut()
            .spawn((
                Orb {
                    vel: Vec2::ZERO,
                    age: 0.0,
                },
                Position(hero + Vec2::new(-150.0, 0.0)),
                PreviousPosition(hero + Vec2::new(-150.0, 0.0)),
            ))
            .id();
        // Without the power-up, N does nothing.
        press_n(&mut app);
        app.update();
        assert!(app.world().resource::<Nova>().ready());

        app.world_mut()
            .resource_mut::<Abilities>()
            .grant(PowerUp::LightNova);
        press_n(&mut app);
        for _ in 0..40 {
            app.update();
        }
        let world = app.world();
        assert!(world.get_entity(near).is_err(), "near mote defeated");
        assert!(world.get_entity(orb).is_err(), "orb put out");
        assert!(world.get::<Enemy>(far).is_some(), "far mote untouched");
        let wounded = world.get::<Enemy>(lens).unwrap().health;
        assert_eq!(wounded, lens_health - lens_health.div_ceil(2));
        // It's recharging, so a second press does nothing more.
        assert!(!world.resource::<Nova>().ready());
        press_n(&mut app);
        for _ in 0..40 {
            app.update();
        }
        assert_eq!(app.world().get::<Enemy>(lens).unwrap().health, wounded);
    }

    #[test]
    fn the_nova_recharges() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<Abilities>()
            .grant(PowerUp::LightNova);
        press_n(&mut app);
        // Keys are read after the physics step, so it goes off next frame.
        app.update();
        let recharge = app.world().resource::<Nova>().recharge;
        assert!(recharge > NOVA_RECHARGE - 0.1);
        // Once the burst has spread and the time is up, it's ready again.
        app.world_mut().resource_mut::<Nova>().recharge = 0.5;
        for _ in 0..200 {
            app.update();
        }
        assert!(app.world().resource::<Nova>().ready());
    }
}
