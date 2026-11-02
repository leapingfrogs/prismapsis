//! The game's screens: the title, its settings, play, game over and the
//! campaign's end.
//!
//! Menu screens show over the level with the game clock stopped, as pausing
//! does, so the level waits greyed out behind them. Choosing to play from
//! the title starts a game from the campaign's first level; going back to
//! the title from a game starts a new one. Continuing after a game over
//! plays the same level again with the score back at nothing.

use bevy::prelude::*;

use crate::enemy::{KeepAbilities, ResetRoom, reset_room};
use crate::events::GameEvent;
use crate::level::{Campaign, CurrentLevel};
use crate::score::{Score, grouped};
use crate::sound::{Auditions, SoundId, SoundSettings};

/// Which screen is showing.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Screen {
    #[default]
    Title,
    Playing,
    /// The hero has lost its last life.
    GameOver,
    /// The campaign's last level is finished.
    Finished,
    /// The volumes, from the title.
    Settings,
}

/// Run condition: true while playing, and always when the game has no
/// screens, as in headless tests of the simulation.
#[must_use]
pub fn in_play(screen: Option<Res<State<Screen>>>) -> bool {
    screen.is_none_or(|screen| *screen.get() == Screen::Playing)
}

/// Something a menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Start a new game.
    Play,
    /// Play the level again after a game over, scoring from nothing.
    Continue,
    /// Go back to the title, starting a new game.
    Title,
    Settings,
    /// A volume, changed with left and right.
    Volume(Channel),
    /// Back to the title from the settings.
    Back,
    Quit,
}

/// A volume the player sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Master,
    Music,
    Effects,
}

impl Channel {
    const fn name(self) -> &'static str {
        match self {
            Self::Master => "Volume",
            Self::Music => "Music",
            Self::Effects => "Effects",
        }
    }

    const fn level(self, settings: &mut SoundSettings) -> &mut f32 {
        match self {
            Self::Master => &mut settings.master,
            Self::Music => &mut settings.music,
            Self::Effects => &mut settings.effects,
        }
    }
}

/// How much left or right changes a volume.
pub const VOLUME_STEP: f32 = 0.1;

impl Choice {
    fn label(self, settings: Option<&SoundSettings>) -> String {
        match self {
            Self::Play => "Play".to_owned(),
            Self::Continue => "Continue".to_owned(),
            Self::Title => "Back to the title".to_owned(),
            Self::Settings => "Settings".to_owned(),
            Self::Volume(channel) => {
                let mut settings = settings.cloned().unwrap_or_default();
                let percent = (*channel.level(&mut settings) * 100.0).round();
                format!("{} {percent}%", channel.name())
            }
            Self::Back => "Back".to_owned(),
            Self::Quit => "Quit".to_owned(),
        }
    }
}

/// Set on going back to the title from a game, so it starts a new one.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NewGame(pub bool);

/// A screen's menu: its choices and which is selected.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Menu {
    pub choices: Vec<Choice>,
    pub selected: usize,
    /// When the menu appeared, in real seconds.
    shown_at: f64,
}

impl Menu {
    /// A menu of `choices`, the first selected, shown at `now` in real seconds.
    #[must_use]
    pub const fn new(choices: Vec<Choice>, now: f64) -> Self {
        Self {
            choices,
            selected: 0,
            shown_at: now,
        }
    }
}

/// One of a menu's lines, by its index among the choices.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuLine(pub usize);

/// Seconds a menu ignores input after it appears, so a press meant for play
/// doesn't choose something.
pub const MENU_DELAY: f64 = 0.6;

const SELECTED_COLOUR: Color = Color::WHITE;
const UNSELECTED_COLOUR: Color = Color::srgb(0.55, 0.56, 0.62);
const SHADE_COLOUR: Color = Color::srgba(0.0, 0.0, 0.03, 0.6);

/// Starts on `start`: the title, or straight into play when a level file
/// was given.
#[derive(Debug, Default)]
pub struct ScreensPlugin {
    pub start: Screen,
}

impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        app.insert_state(self.start)
            .init_resource::<NewGame>()
            .add_systems(
                OnEnter(Screen::Title),
                (new_game.run_if(|new: Res<NewGame>| new.0), spawn_title).chain(),
            )
            .add_systems(OnEnter(Screen::Settings), spawn_settings)
            .add_systems(OnEnter(Screen::Playing), resume)
            .add_systems(OnEnter(Screen::GameOver), (freeze, spawn_game_over))
            .add_systems(OnEnter(Screen::Finished), (freeze, spawn_finished))
            // A new game runs one step to reset the room, then stops.
            .add_systems(FixedUpdate, freeze.after(reset_room).run_if(not(in_play)))
            .add_systems(
                Update,
                (
                    watch_play.run_if(in_state(Screen::Playing)),
                    (choose, highlight).chain().run_if(not(in_play)),
                ),
            );
    }
}

fn freeze(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn resume(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

/// Back to the campaign's first level, with the room reset, no abilities
/// and no points. The clock runs until the reset is done.
fn new_game(
    mut new: ResMut<NewGame>,
    mut campaign: ResMut<Campaign>,
    mut level: ResMut<CurrentLevel>,
    mut reset: ResMut<ResetRoom>,
    mut keep: ResMut<KeepAbilities>,
    mut score: ResMut<Score>,
    mut time: ResMut<Time<Virtual>>,
) {
    new.0 = false;
    campaign.restart();
    level.set_if_neq(CurrentLevel(campaign.level().level.clone()));
    reset.0 = true;
    keep.0 = false;
    score.restart();
    time.unpause();
}

/// Leave play when the lives run out or the campaign is finished.
fn watch_play(mut events: MessageReader<GameEvent>, mut next: ResMut<NextState<Screen>>) {
    for event in events.read() {
        match event {
            GameEvent::OutOfLives => next.set(Screen::GameOver),
            GameEvent::CampaignFinished => next.set(Screen::Finished),
            _ => {}
        }
    }
}

/// Text this size or larger is a heading.
const HEADING_SIZE: f32 = 8.0;

/// Centred text, `size` a percentage of the window's smaller side.
fn text(content: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (
        Text::new(content),
        TextLayout::justify(Justify::Center),
        TextFont {
            font_size: FontSize::VMin(size),
            // Headings stand out in a heavier weight.
            weight: if size >= HEADING_SIZE {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            },
            ..default()
        },
        TextColor(colour),
    )
}

/// A screen over the shaded level: a heading, lines of text, a menu and
/// a footer in small print.
fn spawn_screen(
    commands: &mut Commands,
    screen: Screen,
    heading: &str,
    lines: &[String],
    choices: Vec<Choice>,
    footer: &str,
    now: f64,
) {
    let count = choices.len();
    commands
        .spawn((
            Name::new(format!("{screen:?} screen")),
            DespawnOnExit(screen),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::VMin(1.5),
                ..default()
            },
            BackgroundColor(SHADE_COLOUR),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn(text(heading, 10.0, Color::WHITE));
            for line in lines {
                root.spawn(text(line.clone(), 3.5, Color::WHITE));
            }
            root.spawn((
                Menu::new(choices, now),
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::VMin(1.0),
                    margin: UiRect::vertical(Val::VMin(4.0)),
                    ..default()
                },
            ))
            .with_children(|menu| {
                for index in 0..count {
                    menu.spawn((MenuLine(index), text("", 4.5, UNSELECTED_COLOUR)));
                }
            });
            if !footer.is_empty() {
                root.spawn(text(footer, 2.5, UNSELECTED_COLOUR));
            }
        });
}

fn spawn_title(mut commands: Commands, score: Res<Score>, time: Res<Time<Real>>) {
    let mut lines = Vec::new();
    if score.best > 0 {
        lines.push(format!("Best {}", grouped(score.best)));
    }
    spawn_screen(
        &mut commands,
        Screen::Title,
        "Prismapsis",
        &lines,
        title_choices(),
        "Left and right spin, space fires\nShift holds the prism, P pauses",
        time.elapsed_secs_f64(),
    );
}

fn spawn_settings(mut commands: Commands, time: Res<Time<Real>>) {
    spawn_screen(
        &mut commands,
        Screen::Settings,
        "Settings",
        &[],
        vec![
            Choice::Volume(Channel::Master),
            Choice::Volume(Channel::Music),
            Choice::Volume(Channel::Effects),
            Choice::Back,
        ],
        "Left and right change a volume, as do taps on either side",
        time.elapsed_secs_f64(),
    );
}

/// The title's choices. A browser tab can't be quit from the game, so the
/// web build leaves that out.
fn title_choices() -> Vec<Choice> {
    let mut choices = vec![Choice::Play, Choice::Settings];
    if cfg!(not(target_arch = "wasm32")) {
        choices.push(Choice::Quit);
    }
    choices
}

fn score_lines(score: Score) -> Vec<String> {
    vec![
        format!("Score {}", grouped(score.points)),
        format!("Best {}", grouped(score.best)),
    ]
}

fn spawn_game_over(mut commands: Commands, score: Res<Score>, time: Res<Time<Real>>) {
    spawn_screen(
        &mut commands,
        Screen::GameOver,
        "Game over",
        &score_lines(*score),
        vec![Choice::Continue, Choice::Title],
        "Continuing plays this level again, scoring from nothing",
        time.elapsed_secs_f64(),
    );
}

fn spawn_finished(mut commands: Commands, score: Res<Score>, time: Res<Time<Real>>) {
    spawn_screen(
        &mut commands,
        Screen::Finished,
        "Every colour restored",
        &score_lines(*score),
        vec![Choice::Title],
        "",
        time.elapsed_secs_f64(),
    );
}

/// Up and down move through the menu; left and right change a volume,
/// playing a sound to hear it by; enter, space or the gamepad's south or
/// start button chooses.
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    mut menus: Query<&mut Menu>,
    (mut score, mut new): (ResMut<Score>, ResMut<NewGame>),
    (mut settings, mut auditions): (Option<ResMut<SoundSettings>>, Option<ResMut<Auditions>>),
    mut next: ResMut<NextState<Screen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(mut menu) = menus.single_mut() else {
        return;
    };
    if time.elapsed_secs_f64() - menu.shown_at < MENU_DELAY {
        return;
    }
    let pad = |buttons: &[GamepadButton]| {
        gamepads
            .iter()
            .any(|pad| buttons.iter().any(|&b| pad.just_pressed(b)))
    };
    let count = menu.choices.len();
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || pad(&[GamepadButton::DPadUp]) {
        menu.selected = (menu.selected + count - 1) % count;
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || pad(&[GamepadButton::DPadDown])
    {
        menu.selected = (menu.selected + 1) % count;
    }
    let left = keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA])
        || pad(&[GamepadButton::DPadLeft]);
    let right = keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD])
        || pad(&[GamepadButton::DPadRight]);
    if let (Choice::Volume(channel), Some(settings)) = (menu.choices[menu.selected], &mut settings)
        && left != right
    {
        let step = if right { VOLUME_STEP } else { -VOLUME_STEP };
        let level = channel.level(settings);
        // Rounded to whole steps, so repeated changes don't drift.
        *level = ((*level + step).clamp(0.0, 1.0) * 10.0).round() / 10.0;
        if channel != Channel::Music
            && let Some(auditions) = &mut auditions
        {
            auditions.0.push(SoundId::Checkpoint);
        }
    }
    let confirm = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
        || pad(&[GamepadButton::South, GamepadButton::Start]);
    if !confirm {
        return;
    }
    match menu.choices[menu.selected] {
        Choice::Play => next.set(Screen::Playing),
        Choice::Continue => {
            score.restart();
            next.set(Screen::Playing);
        }
        Choice::Title => {
            new.0 = true;
            next.set(Screen::Title);
        }
        Choice::Settings => next.set(Screen::Settings),
        Choice::Back => next.set(Screen::Title),
        Choice::Volume(_) => {}
        Choice::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

/// Show each choice, the selected one brighter and marked.
fn highlight(
    menus: Query<&Menu>,
    settings: Option<Res<SoundSettings>>,
    mut lines: Query<(&MenuLine, &mut Text, &mut TextColor)>,
) {
    let Ok(menu) = menus.single() else {
        return;
    };
    for (line, mut text, mut colour) in &mut lines {
        let Some(choice) = menu.choices.get(line.0) else {
            continue;
        };
        let selected = line.0 == menu.selected;
        let label = choice.label(settings.as_deref());
        let wanted = if selected {
            format!("- {label} -")
        } else {
            label
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
        colour.0 = if selected {
            SELECTED_COLOUR
        } else {
            UNSELECTED_COLOUR
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationPlugin;
    use crate::hero::{Hero, Position};
    use crate::score::ScorePlugin;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app(start: Screen) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins((SimulationPlugin, ScorePlugin, ScreensPlugin { start }));
        app
    }

    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
        }
    }

    /// Long enough for a menu to take input.
    fn wait_for_menu(app: &mut App) {
        run(app, 45);
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        // The state changes on the next frame.
        app.update();
    }

    fn screen(app: &App) -> Screen {
        *app.world().resource::<State<Screen>>().get()
    }

    fn paused(app: &App) -> bool {
        app.world().resource::<Time<Virtual>>().is_paused()
    }

    fn hero(app: &mut App) -> Vec2 {
        let world = app.world_mut();
        world
            .query_filtered::<&Position, With<Hero>>()
            .single(world)
            .unwrap()
            .0
    }

    fn menu(app: &mut App) -> Option<Menu> {
        let world = app.world_mut();
        world.query::<&Menu>().iter(world).next().cloned()
    }

    fn lines(app: &mut App) -> Vec<String> {
        let world = app.world_mut();
        let mut lines: Vec<(usize, String)> = world
            .query::<(&MenuLine, &Text)>()
            .iter(world)
            .map(|(line, text)| (line.0, text.0.clone()))
            .collect();
        lines.sort();
        lines.into_iter().map(|(_, text)| text).collect()
    }

    fn score(app: &mut App, points: u32) {
        *app.world_mut().resource_mut::<Score>() = Score {
            points,
            best: points,
        };
    }

    fn send(app: &mut App, event: GameEvent) {
        app.world_mut().write_message(event);
        run(app, 2);
    }

    #[test]
    fn the_title_waits_over_a_still_level_until_play_is_chosen() {
        let mut app = app(Screen::Title);
        wait_for_menu(&mut app);
        assert_eq!(screen(&app), Screen::Title);
        assert!(paused(&app));
        let still = hero(&mut app);
        run(&mut app, 20);
        assert_eq!(hero(&mut app), still);
        assert_eq!(lines(&mut app), ["- Play -", "Settings", "Quit"]);

        // P doesn't pause from a menu.
        tap(&mut app, KeyCode::KeyP);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Playing);
        assert!(!paused(&app));
        assert_eq!(menu(&mut app), None);
        run(&mut app, 20);
        assert_ne!(hero(&mut app), still);
    }

    #[test]
    fn a_menu_ignores_presses_until_it_has_been_seen() {
        let mut app = app(Screen::Title);
        run(&mut app, 5);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Title);
    }

    #[test]
    fn menus_move_up_and_down_and_wrap() {
        let mut app = app(Screen::Title);
        wait_for_menu(&mut app);
        tap(&mut app, KeyCode::ArrowDown);
        assert_eq!(lines(&mut app), ["Play", "- Settings -", "Quit"]);
        tap(&mut app, KeyCode::ArrowDown);
        tap(&mut app, KeyCode::ArrowDown);
        assert_eq!(menu(&mut app).unwrap().selected, 0);
        tap(&mut app, KeyCode::KeyW);
        assert_eq!(menu(&mut app).unwrap().selected, 2);
        tap(&mut app, KeyCode::Space);
        let exits = app.world().resource::<Messages<AppExit>>();
        assert!(!exits.is_empty(), "quit");
    }

    #[test]
    fn the_settings_change_volumes_in_steps_and_go_back_to_the_title() {
        let mut app = app(Screen::Title);
        app.init_resource::<SoundSettings>()
            .init_resource::<Auditions>();
        wait_for_menu(&mut app);
        tap(&mut app, KeyCode::ArrowDown);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Settings);
        wait_for_menu(&mut app);
        assert_eq!(
            lines(&mut app),
            ["- Volume 80% -", "Music 60%", "Effects 100%", "Back"]
        );

        // Music down twice, then effects up past the top.
        tap(&mut app, KeyCode::ArrowDown);
        tap(&mut app, KeyCode::ArrowLeft);
        tap(&mut app, KeyCode::KeyA);
        tap(&mut app, KeyCode::ArrowDown);
        tap(&mut app, KeyCode::ArrowRight);
        let settings = app.world().resource::<SoundSettings>().clone();
        assert!((settings.music - 0.4).abs() < 1e-6, "{}", settings.music);
        assert!((settings.effects - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            lines(&mut app),
            ["Volume 80%", "Music 40%", "- Effects 100% -", "Back"]
        );
        // Changing the effects plays one to hear it by; the music can be
        // heard already.
        assert_eq!(app.world().resource::<Auditions>().0, [SoundId::Checkpoint]);

        tap(&mut app, KeyCode::ArrowDown);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Title);
        assert!(!app.world().resource::<NewGame>().0, "no new game");
    }

    #[test]
    fn volumes_need_settings_to_change() {
        let mut app = app(Screen::Settings);
        wait_for_menu(&mut app);
        tap(&mut app, KeyCode::ArrowRight);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Settings);
        assert_eq!(lines(&mut app)[0], "- Volume 80% -");
    }

    #[test]
    fn running_out_of_lives_offers_to_continue_from_nothing() {
        let mut app = app(Screen::Playing);
        run(&mut app, 5);
        assert!(!paused(&app));
        score(&mut app, 4000);
        send(&mut app, GameEvent::OutOfLives);
        assert_eq!(screen(&app), Screen::GameOver);
        assert!(paused(&app));
        assert_eq!(lines(&mut app), ["- Continue -", "Back to the title"]);

        wait_for_menu(&mut app);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Playing);
        assert!(!paused(&app));
        let score = *app.world().resource::<Score>();
        assert_eq!((score.points, score.best), (0, 4000));
    }

    #[test]
    fn the_title_starts_a_new_game_from_the_first_level() {
        let mut app = app(Screen::Playing);
        run(&mut app, 5);
        // Move on to the second level, with some points.
        let second = {
            let mut campaign = app.world_mut().resource_mut::<Campaign>();
            campaign.advance();
            campaign.level().level.clone()
        };
        app.insert_resource(CurrentLevel(second.clone()));
        score(&mut app, 900);
        run(&mut app, 5);
        assert_eq!(hero(&mut app).x, second.start.x);

        send(&mut app, GameEvent::OutOfLives);
        wait_for_menu(&mut app);
        tap(&mut app, KeyCode::ArrowDown);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Title);
        run(&mut app, 5);
        assert!(paused(&app), "stopped again once the room is reset");
        let first = app.world().resource::<Campaign>().levels[0].level.clone();
        assert_eq!(app.world().resource::<Campaign>().current, 0);
        assert_eq!(app.world().resource::<CurrentLevel>().0, first);
        // A step may move the hero a little before the clock stops.
        assert!(hero(&mut app).distance(first.start) < 2.0);
        assert_eq!(app.world().resource::<Score>().points, 0);
    }

    #[test]
    fn finishing_the_campaign_shows_the_score_then_the_title() {
        let mut app = app(Screen::Playing);
        run(&mut app, 5);
        score(&mut app, 12_300);
        send(&mut app, GameEvent::CampaignFinished);
        assert_eq!(screen(&app), Screen::Finished);
        assert!(paused(&app));
        let world = app.world_mut();
        let texts: Vec<String> = world
            .query::<&Text>()
            .iter(world)
            .map(|t| t.0.clone())
            .collect();
        assert!(texts.contains(&"Score 12,300".to_owned()), "{texts:?}");
        wait_for_menu(&mut app);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(screen(&app), Screen::Title);
        // The title shows the best score.
        let world = app.world_mut();
        let texts: Vec<String> = world
            .query::<&Text>()
            .iter(world)
            .map(|t| t.0.clone())
            .collect();
        assert!(texts.contains(&"Best 12,300".to_owned()), "{texts:?}");
    }

    #[test]
    fn without_screens_the_game_is_always_in_play() {
        let mut app = App::new();
        app.add_systems(Update, |screen: Option<Res<State<Screen>>>| {
            assert!(in_play(screen));
        });
        app.update();
    }
}
