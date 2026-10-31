//! Moments in the simulation that presentation (sound now; particles and
//! score later) reacts to. The simulation writes them and never reads them.

use bevy::prelude::*;

use crate::enemy::EnemyKind;
use crate::light::{Hue, Primary};
use crate::powerup::PowerUp;

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
    /// A defeated enemy released light into its colour's gauge.
    LightAbsorbed {
        colour: Primary,
        at: Vec2,
    },
    /// A colour came back to the level (white opens the exit).
    ColourRestored {
        hue: Hue,
    },
    /// The hero went through the open exit.
    ExitReached {
        at: Vec2,
    },
}
