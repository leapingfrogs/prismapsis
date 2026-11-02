//! The score: points for what the hero does, counted from the game's
//! events, and the best score this session.

use bevy::prelude::*;

use crate::enemy::EnemyKind;
use crate::events::GameEvent;

/// Points for defeating a mote.
pub const MOTE_POINTS: u32 = 100;
/// Points for defeating a lens.
pub const LENS_POINTS: u32 = 500;
/// Points for collecting a pickup.
pub const PICKUP_POINTS: u32 = 250;
/// Points for each colour restored, white included.
pub const COLOUR_POINTS: u32 = 1000;
/// Points for finishing a level.
pub const EXIT_POINTS: u32 = 5000;

/// The points scored in this game, and the most scored in any game since
/// the game started.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    pub points: u32,
    pub best: u32,
}

impl Score {
    /// Start again from nothing, keeping the best.
    pub const fn restart(&mut self) {
        self.points = 0;
    }

    fn add(&mut self, points: u32) {
        self.points = self.points.saturating_add(points);
        self.best = self.best.max(self.points);
    }
}

/// The points an event is worth.
#[must_use]
pub const fn points_for(event: &GameEvent) -> u32 {
    match event {
        GameEvent::EnemyDefeated {
            kind: EnemyKind::Mote,
            ..
        } => MOTE_POINTS,
        GameEvent::EnemyDefeated {
            kind: EnemyKind::Lens,
            ..
        } => LENS_POINTS,
        GameEvent::PickupCollected { .. } => PICKUP_POINTS,
        GameEvent::ColourRestored { .. } => COLOUR_POINTS,
        GameEvent::ExitReached { .. } => EXIT_POINTS,
        _ => 0,
    }
}

/// `points` with its thousands separated by commas.
#[must_use]
pub fn grouped(points: u32) -> String {
    let digits = points.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[derive(Debug, Default)]
pub struct ScorePlugin;

impl Plugin for ScorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .add_systems(Startup, spawn_score_text)
            .add_systems(Update, (add_points, show_score.after(add_points)));
    }
}

fn add_points(mut events: MessageReader<GameEvent>, mut score: ResMut<Score>) {
    for event in events.read() {
        let points = points_for(event);
        if points > 0 {
            score.add(points);
        }
    }
}

/// The score in the top right corner.
#[derive(Component, Debug)]
pub struct ScoreText;

fn spawn_score_text(mut commands: Commands) {
    commands.spawn((
        Name::new("Score"),
        ScoreText,
        Text::new("0"),
        TextFont {
            font_size: FontSize::Vh(3.4),
            weight: FontWeight::SEMIBOLD,
            ..default()
        },
        TextColor(Color::WHITE),
        // A soft shadow keeps it readable over bright light.
        TextShadow {
            offset: Vec2::splat(2.0),
            color: Color::srgba(0.0, 0.0, 0.05, 0.7),
        },
        Node {
            position_type: PositionType::Absolute,
            right: Val::Vw(1.5),
            top: Val::Vh(2.0),
            ..default()
        },
    ));
}

fn show_score(score: Res<Score>, mut texts: Query<&mut Text, With<ScoreText>>) {
    if !score.is_changed() {
        return;
    }
    for mut text in &mut texts {
        text.0 = grouped(score.points);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::light::Hue;
    use crate::powerup::PowerUp;

    #[test]
    fn events_score_their_points() {
        let at = Vec2::ZERO;
        let mote = GameEvent::EnemyDefeated {
            kind: EnemyKind::Mote,
            at,
        };
        let lens = GameEvent::EnemyDefeated {
            kind: EnemyKind::Lens,
            at,
        };
        assert_eq!(points_for(&mote), MOTE_POINTS);
        assert_eq!(points_for(&lens), LENS_POINTS);
        let pickup = GameEvent::PickupCollected {
            power: PowerUp::Spread,
            at,
        };
        assert_eq!(points_for(&pickup), PICKUP_POINTS);
        let white = GameEvent::ColourRestored { hue: Hue::White };
        assert_eq!(points_for(&white), COLOUR_POINTS);
        assert_eq!(points_for(&GameEvent::ExitReached { at }), EXIT_POINTS);
        assert_eq!(points_for(&GameEvent::EnemyHit { at }), 0);
        assert_eq!(points_for(&GameEvent::OutOfLives), 0);
    }

    #[test]
    fn the_best_score_outlasts_a_restart() {
        let mut score = Score::default();
        score.add(700);
        score.restart();
        score.add(200);
        assert_eq!(
            score,
            Score {
                points: 200,
                best: 700
            }
        );
        score.add(u32::MAX);
        assert_eq!(score.points, u32::MAX);
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(12_300), "12,300");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn the_score_shows_what_events_earned() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GameEvent>()
            .add_plugins(ScorePlugin);
        app.update();
        let at = Vec2::ZERO;
        app.world_mut().write_message(GameEvent::EnemyDefeated {
            kind: EnemyKind::Lens,
            at,
        });
        app.world_mut().write_message(GameEvent::ExitReached { at });
        app.update();
        assert_eq!(app.world().resource::<Score>().points, 5500);
        let world = app.world_mut();
        let text = world
            .query_filtered::<&Text, With<ScoreText>>()
            .single(world)
            .unwrap();
        assert_eq!(text.0, "5,500");
    }
}
