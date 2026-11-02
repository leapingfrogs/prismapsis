//! Moments in the simulation that presentation (sound, and later
//! particles), the score and the game's screens react to. The simulation
//! writes them and never reads them.

use bevy::prelude::*;

use crate::enemy::EnemyKind;
use crate::light::{Hue, Primary};
use crate::powerup::PowerUp;

/// Count every `event` the app writes from now on, in `Seen`.
#[cfg(test)]
pub fn count_events(app: &mut App, event: GameEvent) {
    app.init_resource::<Seen>().add_systems(
        Update,
        move |mut events: MessageReader<GameEvent>, mut seen: ResMut<Seen>| {
            seen.0 += events.read().filter(|&&e| e == event).count();
        },
    );
}

/// How many events `count_events` has seen.
#[cfg(test)]
#[derive(Resource, Debug, Default)]
pub struct Seen(pub usize);

/// Something that just happened in the game. Positions are in world units.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum GameEvent {
    /// A beam damage tick landed on an enemy.
    EnemyHit {
        at: Vec2,
    },
    EnemyDefeated {
        kind: EnemyKind,
        at: Vec2,
    },
    OrbFired {
        at: Vec2,
    },
    /// The prism stopped an orb.
    OrbBlocked {
        at: Vec2,
    },
    /// The hero bounced off a floor with this spin step.
    Bounced {
        at: Vec2,
        spin: i8,
    },
    WallBump {
        at: Vec2,
    },
    /// The hero was hurt and respawns at `respawn`.
    LifeLost {
        at: Vec2,
        respawn: Vec2,
    },
    /// Losing a life took away at least one ability.
    AbilitiesLost,
    InvulnerabilityEnded {
        at: Vec2,
    },
    RoomReset,
    /// The hero lost its last life; the room resets with lives refilled.
    OutOfLives,
    /// The last level of the campaign was finished; it starts again from the
    /// first.
    CampaignFinished,
    PickupDropped {
        power: PowerUp,
        at: Vec2,
    },
    PickupCollected {
        power: PowerUp,
        at: Vec2,
    },
    /// A pickup started blinking before it vanishes.
    PickupExpiring {
        at: Vec2,
    },
    /// A defeated enemy released light into its colour's gauge. `amount`
    /// is how much the gauge took: none once the colour is restored.
    LightAbsorbed {
        colour: Primary,
        at: Vec2,
        amount: f32,
    },
    /// A colour came back to the level (white opens the exit).
    ColourRestored {
        hue: Hue,
    },
    /// The hero went through the open exit.
    ExitReached {
        at: Vec2,
    },
    /// The hero touched a new checkpoint, which is now where it respawns.
    CheckpointReached {
        at: Vec2,
    },
}
