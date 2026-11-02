//! Droplets of colour that fly from a defeated enemy to the hero.
//!
//! Each half unit of light a gauge takes is one droplet, so a lens's eight
//! clearly outnumber a mote's two. They burst out and up, then swoop in on
//! the hero, chasing it if it moves. A colour that's already restored takes
//! no light, so its droplets only spill and fade where they fall.
//! Presentation only: the gauge has already filled.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use crate::art::figures::Sheet;
use crate::art::mix;
use crate::art::sprites::Figure;
use crate::events::GameEvent;
use crate::hero::{HERO_RADIUS, Hero};
use crate::light::Primary;
use crate::render::glowing;
use crate::spectrum_view::hue_colour;

/// Droplets for each unit of light a gauge takes.
pub const DROPS_PER_LIGHT: f32 = 2.0;

/// Most droplets one enemy releases.
const MAX_DROPS: u32 = 16;

/// Droplets spilled when the colour is already restored.
const SPILLED_DROPS: u32 = 2;

/// How fast droplets leave the enemy, slowest to fastest (units per second).
const LAUNCH_SPEED: (f32, f32) = (240.0, 460.0);

/// How wide droplets fan out, either side of straight up (radians).
const LAUNCH_SPREAD: f32 = PI * 0.45;

/// Seconds droplets fly free before the hero starts to pull them in.
const LAUNCH_TIME: f32 = 0.18;

/// How quickly the hero's pull grows once it starts (per second squared).
const PULL: f32 = 14.0;

/// How fast droplets fly in at most (units per second).
const MAX_SPEED: f32 = 1600.0;

/// Gravity on droplets in flight (units per second squared).
const GRAVITY: f32 = 900.0;

/// Seconds a spilled droplet takes to fade, and the longest any droplet
/// flies before it arrives regardless.
const SPILL_TIME: f32 = 0.6;
const MAX_FLIGHT: f32 = 2.0;

/// How brightly droplets glow, past white.
const DROPLET_LIGHT: f32 = 2.2;

/// How far a droplet stretches along its path at full speed, as a trail.
const STRETCH: f32 = 1.2;

/// Seconds between arrivals that still count as one stream, so each tick
/// climbs the scale.
const STREAM_GAP: f32 = 0.4;

/// A droplet in flight.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Droplet {
    pub colour: Primary,
    pub velocity: Vec2,
    pub age: f32,
    /// Flying to the hero; otherwise spilling and fading.
    pub homing: bool,
}

/// A droplet reached the hero. `step` counts arrivals in a stream, so
/// sounds can climb with it.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct DropletLanded {
    pub colour: Primary,
    pub at: Vec2,
    pub step: i32,
}

/// Arrivals in the current stream.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
struct Stream {
    count: i32,
    since: f32,
}

#[derive(Debug, Default)]
pub struct DropletsPlugin;

impl Plugin for DropletsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DropletLanded>()
            .init_resource::<Stream>()
            .add_systems(Update, (spawn_droplets, fly_droplets).chain());
    }
}

/// How many droplets `amount` of light makes; spilled light makes a few.
#[must_use]
pub fn droplet_count(amount: f32) -> u32 {
    if amount <= 0.0 {
        return SPILLED_DROPS;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (amount * DROPS_PER_LIGHT).round() as u32;
    count.clamp(1, MAX_DROPS)
}

/// A number from 0 to 1, from `seed`.
#[allow(clippy::cast_precision_loss)]
fn unit(seed: u64) -> f32 {
    (mix(seed) % 10_000) as f32 / 10_000.0
}

fn spawn_droplets(
    mut commands: Commands,
    sheet: Res<Sheet>,
    mut events: MessageReader<GameEvent>,
    mut seed: Local<u64>,
) {
    for event in events.read() {
        let &GameEvent::LightAbsorbed { colour, at, amount } = event else {
            continue;
        };
        let homing = amount > 0.0;
        let tint = glowing(hue_colour(colour.into()), DROPLET_LIGHT);
        for _ in 0..droplet_count(amount) {
            *seed += 1;
            let angle = FRAC_PI_2 + (unit(*seed) - 0.5) * 2.0 * LAUNCH_SPREAD;
            let launch = LAUNCH_SPEED.0 + (LAUNCH_SPEED.1 - LAUNCH_SPEED.0) * unit(!*seed);
            commands.spawn((
                Name::new("Droplet"),
                Droplet {
                    colour,
                    velocity: Vec2::from_angle(angle) * launch,
                    age: 0.0,
                    homing,
                },
                sheet.sprite(Figure::Droplet, tint),
                Transform::from_translation(at.extend(4.5)),
            ));
        }
    }
}

/// Where a droplet heads next: free flight under gravity at first, then
/// an ever stronger pull towards `target`.
#[must_use]
pub fn steer(droplet: &Droplet, from: Vec2, target: Vec2, dt: f32) -> Vec2 {
    let mut velocity = droplet.velocity;
    velocity.y -= GRAVITY * dt;
    let pulling = (droplet.age - LAUNCH_TIME).max(0.0);
    if pulling > 0.0 {
        let wanted = (target - from).normalize_or_zero() * MAX_SPEED;
        velocity = velocity.lerp(wanted, (PULL * pulling * dt).min(1.0));
    }
    velocity.clamp_length_max(MAX_SPEED)
}

fn fly_droplets(
    mut commands: Commands,
    time: Res<Time>,
    mut stream: ResMut<Stream>,
    mut landed: MessageWriter<DropletLanded>,
    heroes: Query<&Transform, (With<Hero>, Without<Droplet>)>,
    mut droplets: Query<(Entity, &mut Droplet, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    stream.since += dt;
    let hero = heroes.single().ok().map(|t| t.translation.truncate());
    for (entity, mut droplet, mut transform, mut sprite) in &mut droplets {
        droplet.age += dt;
        let at = transform.translation.truncate();
        let target = hero.filter(|_| droplet.homing);
        if let Some(target) = target
            && (at.distance(target) < HERO_RADIUS || droplet.age > MAX_FLIGHT)
        {
            commands.entity(entity).despawn();
            if stream.since > STREAM_GAP {
                stream.count = 0;
            }
            landed.write(DropletLanded {
                colour: droplet.colour,
                at: target,
                step: stream.count,
            });
            stream.count += 1;
            stream.since = 0.0;
            continue;
        }
        if target.is_none() && droplet.age > SPILL_TIME {
            commands.entity(entity).despawn();
            continue;
        }
        droplet.velocity = match target {
            Some(target) => steer(&droplet, at, target, dt),
            None => droplet.velocity - Vec2::Y * GRAVITY * dt,
        };
        transform.translation += (droplet.velocity * dt).extend(0.0);
        // Stretched along its path as a short trail.
        let speed = droplet.velocity.length();
        transform.rotation = Quat::from_rotation_z(droplet.velocity.to_angle());
        transform.scale = Vec3::new(1.0 + STRETCH * speed / MAX_SPEED, 1.0, 1.0);
        if target.is_none() {
            let left = 1.0 - droplet.age / SPILL_TIME;
            sprite.color.set_alpha(left.clamp(0.0, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::figures::FiguresPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn more_light_makes_more_droplets() {
        assert_eq!(droplet_count(1.0), 2);
        assert_eq!(droplet_count(4.0), 8);
        assert_eq!(droplet_count(100.0), MAX_DROPS);
        // A sliver still shows one; spilled light shows a few.
        assert_eq!(droplet_count(0.1), 1);
        assert_eq!(droplet_count(0.0), SPILLED_DROPS);
    }

    #[test]
    fn droplets_fly_free_then_turn_towards_the_hero() {
        let droplet = Droplet {
            colour: Primary::Red,
            velocity: Vec2::new(0.0, 300.0),
            age: 0.0,
            homing: true,
        };
        let target = Vec2::new(500.0, 0.0);
        let free = steer(&droplet, Vec2::ZERO, target, 0.016);
        assert!(free.x.abs() < f32::EPSILON, "no pull at launch");
        let later = Droplet {
            age: 0.5,
            ..droplet
        };
        assert!(steer(&later, Vec2::ZERO, target, 0.016).x > 0.0);
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, FiguresPlugin, DropletsPlugin))
            .add_message::<GameEvent>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )));
        app.world_mut()
            .spawn((Hero, Transform::from_xyz(300.0, 0.0, 0.0)));
        app.update();
        app
    }

    fn droplets(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query::<&Droplet>().iter(world).count()
    }

    fn landed(app: &App) -> Vec<DropletLanded> {
        let messages = app.world().resource::<Messages<DropletLanded>>();
        messages.iter_current_update_messages().copied().collect()
    }

    #[test]
    fn a_lens_sends_eight_droplets_that_land_on_the_hero_climbing() {
        let mut app = app();
        app.world_mut().write_message(GameEvent::LightAbsorbed {
            colour: Primary::Blue,
            at: Vec2::ZERO,
            amount: 4.0,
        });
        app.update();
        assert_eq!(droplets(&mut app), 8);
        let mut steps = Vec::new();
        for _ in 0..200 {
            app.update();
            steps.extend(landed(&app).iter().map(|l| l.step));
        }
        assert_eq!(droplets(&mut app), 0);
        assert_eq!(steps.len(), 8);
        assert!(steps.windows(2).all(|w| w[1] > w[0]), "{steps:?}");
    }

    #[test]
    fn spilled_droplets_fade_without_landing() {
        let mut app = app();
        app.world_mut().write_message(GameEvent::LightAbsorbed {
            colour: Primary::Red,
            at: Vec2::ZERO,
            amount: 0.0,
        });
        app.update();
        assert_eq!(droplets(&mut app), SPILLED_DROPS as usize);
        let mut count = 0;
        for _ in 0..60 {
            app.update();
            count += landed(&app).len();
        }
        assert_eq!(droplets(&mut app), 0);
        assert_eq!(count, 0);
    }
}
