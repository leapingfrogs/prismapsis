//! Touch controls for phones and tablets: buttons drawn over the game, and
//! menus chosen by tapping them.
//!
//! Touches become key presses, so the game reads one set of inputs
//! whether they come from the keyboard, a gamepad or a screen. Each button
//! holds its key while any finger is on it, so several can be held at once
//! and a finger sliding onto a button presses it. A tap on hold locks the
//! prism held until the next tap. A finger that starts on hold stays with
//! it, and sliding it up or down turns the prism and leaves it locked. The
//! buttons appear after the first touch and hide again on a key or gamepad
//! press. Presentation only.

use bevy::input::InputSystems;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::touch::{Touch, Touches};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::art::touch::Glyph;
use crate::hud::HudArt;
use crate::pause::{KEY_HINT, PauseHint};
use crate::powerup::{Abilities, PowerUp};
use crate::screens::{Choice, Menu, MenuLine, Screen, in_play};

/// Whether the buttons are showing, and the keys touches are holding.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct TouchControls {
    pub shown: bool,
    /// Whether a tap has locked the sliding button's key held, with no
    /// finger on it.
    pub locked: bool,
    /// Keys held by fingers on buttons, or by the lock.
    held: Vec<KeyCode>,
    /// Keys pressed by a tap on a menu, let go the frame after.
    tapped: Vec<KeyCode>,
    /// The finger that started on a sliding button, while it's down.
    grip: Option<Grip>,
}

impl TouchControls {
    /// Let go of the lock and forget any finger on a sliding button.
    const fn unlock(&mut self) {
        self.locked = false;
        self.grip = None;
    }
}

/// A finger that started on a sliding button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grip {
    id: u64,
    /// Whether it has slid far enough to hold an up or down key, which
    /// makes lifting it a slide rather than a tap.
    slid: bool,
}

/// An on-screen button and the key it holds.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchButton {
    pub key: KeyCode,
    /// Only shown while the hero has anti-gravity.
    pub hover: bool,
    /// Keys held by sliding a finger up or down from the button. A finger
    /// that starts on such a button keeps it held wherever it goes.
    pub slide: Option<[KeyCode; 2]>,
}

impl TouchButton {
    #[must_use]
    pub const fn new(key: KeyCode) -> Self {
        Self {
            key,
            hover: false,
            slide: None,
        }
    }

    #[must_use]
    pub const fn hovering(self) -> Self {
        Self {
            hover: true,
            ..self
        }
    }

    #[must_use]
    pub const fn sliding(self, up: KeyCode, down: KeyCode) -> Self {
        Self {
            slide: Some([up, down]),
            ..self
        }
    }
}

/// How far a finger slides from where it started, in button heights,
/// before a sliding button holds its up or down key.
pub const SLIDE_STEP: f32 = 0.3;

/// The key a slide of `dy` (down positive) holds on a button `height`
/// tall, if it has gone far enough.
#[must_use]
pub fn slide_key(slide: [KeyCode; 2], dy: f32, height: f32) -> Option<KeyCode> {
    let step = height * SLIDE_STEP;
    if dy < -step {
        Some(slide[0])
    } else if dy > step {
        Some(slide[1])
    } else {
        None
    }
}

/// The buttons' container, shown only in play.
#[derive(Component, Debug)]
pub struct TouchOverlay;

/// The hold button's symbol, a lock shown open or closed.
#[derive(Component, Debug)]
pub struct HoldGlyph;

// Faint, so the buttons don't hide the play behind them; a pressed button
// lights up clearly.
const BUTTON_COLOUR: Color = Color::srgba(0.8, 0.82, 0.9, 0.06);
const PRESSED_COLOUR: Color = Color::srgba(0.8, 0.82, 0.9, 0.3);
const EDGE_COLOUR: Color = Color::srgba(0.8, 0.82, 0.9, 0.22);
const GLYPH_COLOUR: Color = Color::srgba(1.0, 1.0, 1.0, 0.45);

/// How much of a button's width its symbol takes.
const GLYPH_SCALE: f32 = 0.5;

/// How far from a menu line a tap still chooses it, in line heights: most
/// of the gap above and below, and well out to either side.
const MENU_REACH: Vec2 = Vec2::new(3.0, 0.6);

#[derive(Debug, Default)]
pub struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchControls>()
            .add_systems(Startup, spawn_controls)
            .add_systems(
                PreUpdate,
                (show_controls, press_buttons, tap_menus)
                    .chain()
                    .after(InputSystems),
            )
            .add_systems(Update, (update_controls, hint_pause));
    }
}

/// Show the buttons on a touch; hide them on a key or gamepad press.
fn show_controls(
    touches: Res<Touches>,
    mut keyboard: MessageReader<KeyboardInput>,
    gamepads: Query<&Gamepad>,
    mut controls: ResMut<TouchControls>,
) {
    let keyed = keyboard.read().any(|key| key.state.is_pressed())
        || gamepads
            .iter()
            .any(|pad| pad.get_just_pressed().next().is_some());
    let shown = if touches.any_just_pressed() {
        true
    } else if keyed {
        false
    } else {
        controls.shown
    };
    if controls.shown != shown {
        controls.shown = shown;
    }
    // Keyboard and gamepad players are never left with a locked prism.
    if keyed && (controls.locked || controls.grip.is_some()) {
        controls.unlock();
    }
}

/// Hold each visible button's key while a finger is on it, and a sliding
/// button's keys while the finger that started on it is down.
///
/// Lifting that finger after a tap toggles the lock, and after a slide
/// locks it. The lock holds the sliding button's key until play ends or
/// the buttons hide.
fn press_buttons(
    touches: Res<Touches>,
    screen: Option<Res<State<Screen>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    buttons: Query<(
        &TouchButton,
        &ComputedNode,
        &UiGlobalTransform,
        &InheritedVisibility,
    )>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut controls: ResMut<TouchControls>,
) {
    for key in std::mem::take(&mut controls.tapped) {
        keys.release(key);
    }
    // UI nodes are laid out in physical pixels.
    let scale = windows.single().map_or(1.0, Window::scale_factor);
    let visible: Vec<_> = buttons.iter().filter(|(.., shown)| shown.get()).collect();
    // The key of the sliding button a touch started on, and the up or down
    // key its slide holds, if it has gone far enough.
    let sliding = |touch: &Touch| {
        let start = touch.start_position() * scale;
        let dy = touch.position().y * scale - start.y;
        visible.iter().find_map(|(button, node, transform, _)| {
            let slide = button.slide?;
            node.contains_point(**transform, start)
                .then(|| (button.key, slide_key(slide, dy, node.size().y)))
        })
    };

    if !in_play(screen) && (controls.locked || controls.grip.is_some()) {
        controls.unlock();
    }
    for touch in touches.iter_just_pressed() {
        if sliding(touch).is_some() {
            controls.grip = Some(Grip {
                id: touch.id(),
                slid: false,
            });
        }
    }
    if let Some(mut grip) = controls.grip {
        let id = grip.id;
        let finger = touches
            .get_pressed(id)
            .or_else(|| touches.get_released(id))
            .or_else(|| touches.iter_just_canceled().find(|touch| touch.id() == id));
        grip.slid |= finger
            .and_then(sliding)
            .is_some_and(|(_, slid)| slid.is_some());
        if touches.get_pressed(id).is_some() {
            if controls.grip != Some(grip) {
                controls.grip = Some(grip);
            }
        } else {
            // A slide parks the prism and a tap toggles it; a cancelled
            // tap leaves it be.
            if grip.slid {
                controls.locked = true;
            } else if touches.just_released(id) {
                controls.locked = !controls.locked;
            }
            controls.grip = None;
        }
    }

    let mut wanted = Vec::new();
    let mut want = |key| {
        if !wanted.contains(&key) {
            wanted.push(key);
        }
    };
    if controls.locked {
        for (button, ..) in &visible {
            if button.slide.is_some() {
                want(button.key);
            }
        }
    }
    for touch in touches.iter() {
        if let Some((key, slid)) = sliding(touch) {
            want(key);
            if let Some(key) = slid {
                want(key);
            }
            continue;
        }
        let point = touch.position() * scale;
        for (button, node, transform, _) in &visible {
            if node.contains_point(**transform, point) {
                want(button.key);
            }
        }
    }
    for &key in &controls.held {
        if !wanted.contains(&key) {
            keys.release(key);
        }
    }
    // Pressing again each frame keeps a key held if the keyboard let it
    // go; only a new press counts as just pressed.
    for &key in &wanted {
        keys.press(key);
    }
    if controls.held != wanted {
        controls.held = wanted;
    }
}

/// A tap on a menu line selects and chooses it; on a volume, the left half
/// turns it down and the right half up.
fn tap_menus(
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut menus: Query<&mut Menu>,
    lines: Query<(&MenuLine, &ComputedNode, &UiGlobalTransform)>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut controls: ResMut<TouchControls>,
) {
    let Ok(mut menu) = menus.single_mut() else {
        return;
    };
    let scale = windows.single().map_or(1.0, Window::scale_factor);
    for touch in touches.iter_just_pressed() {
        let point = touch.position() * scale;
        let Some((index, right)) = nearest_line(point, &lines) else {
            continue;
        };
        let Some(&choice) = menu.choices.get(index) else {
            continue;
        };
        menu.selected = index;
        let key = match (choice, right) {
            (Choice::Volume(_), false) => KeyCode::ArrowLeft,
            (Choice::Volume(_), true) => KeyCode::ArrowRight,
            _ => KeyCode::Enter,
        };
        keys.press(key);
        controls.tapped.push(key);
    }
}

/// The menu line a tap at `point` is meant for, and whether it's right of
/// the line's middle.
fn nearest_line(
    point: Vec2,
    lines: &Query<(&MenuLine, &ComputedNode, &UiGlobalTransform)>,
) -> Option<(usize, bool)> {
    lines
        .iter()
        .filter_map(|(line, node, transform)| {
            let size = node.size();
            let offset = transform.try_inverse()?.transform_point2(point);
            let reach = size / 2.0 + MENU_REACH * size.y;
            (offset.abs().cmple(reach).all()).then_some((line.0, offset))
        })
        .min_by(|a, b| a.1.y.abs().total_cmp(&b.1.y.abs()))
        .map(|(index, offset)| (index, offset.x > 0.0))
}

/// A button: a translucent rounded square with a symbol, named `name`.
fn button(button: TouchButton, name: &str, art: &HudArt, glyph: Glyph, size: f32) -> impl Bundle {
    (
        square(button, name, size),
        children![button_glyph(art, glyph, size)],
    )
}

/// A button's rounded square, without its symbol.
fn square(button: TouchButton, name: &str, size: f32) -> impl Bundle {
    (
        Name::new(format!("Touch {name}")),
        button,
        Node {
            width: Val::VMin(size),
            height: Val::VMin(size),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::VMin(0.4)),
            border_radius: BorderRadius::all(Val::VMin(size / 4.0)),
            ..default()
        },
        BackgroundColor(BUTTON_COLOUR),
        BorderColor::all(EDGE_COLOUR),
    )
}

/// The symbol on a button `size` across. Exo 2 has no arrows, stars or
/// locks, so they come from generated art rather than text.
fn button_glyph(art: &HudArt, glyph: Glyph, size: f32) -> impl Bundle {
    (
        Node {
            width: Val::VMin(size * GLYPH_SCALE),
            height: Val::VMin(size * GLYPH_SCALE),
            ..default()
        },
        ImageNode {
            image: art.glyphs.clone(),
            color: GLYPH_COLOUR,
            rect: Some(glyph.rect()),
            ..default()
        },
    )
}

/// A column or row of buttons with gaps between.
fn group(direction: FlexDirection) -> Node {
    Node {
        flex_direction: direction,
        align_items: AlignItems::Center,
        column_gap: Val::VMin(3.0),
        row_gap: Val::VMin(3.0),
        ..default()
    }
}

/// Spin on the left with hovering above it; fire on the right with hold
/// above it in reach of the same thumb, sliding hold up turning the prism
/// anticlockwise and down clockwise, as Q and E do; pause at the top
/// right, under the score.
fn spawn_controls(mut commands: Commands, art: Option<Res<HudArt>>) {
    // Without the HUD's art (headless tests), the symbols are left empty.
    let art = &art.as_deref().cloned().unwrap_or_default();
    commands.spawn((
        Name::new("Touch controls"),
        TouchOverlay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            padding: UiRect::all(Val::VMin(4.0)),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },
        Visibility::Hidden,
        GlobalZIndex(5),
        children![
            (
                Node {
                    justify_content: JustifyContent::FlexEnd,
                    margin: UiRect::top(Val::Vh(6.0)),
                    ..default()
                },
                children![button(
                    TouchButton::new(KeyCode::KeyP),
                    "Pause",
                    art,
                    Glyph::Pause,
                    11.0
                )],
            ),
            (
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::FlexEnd,
                    ..default()
                },
                children![
                    (
                        group(FlexDirection::Column),
                        children![
                            button(
                                TouchButton::new(KeyCode::ArrowUp).hovering(),
                                "Up",
                                art,
                                Glyph::Up,
                                15.0
                            ),
                            button(
                                TouchButton::new(KeyCode::ArrowDown).hovering(),
                                "Down",
                                art,
                                Glyph::Down,
                                15.0
                            ),
                            (
                                group(FlexDirection::Row),
                                children![
                                    button(
                                        TouchButton::new(KeyCode::ArrowLeft),
                                        "Left",
                                        art,
                                        Glyph::Left,
                                        19.0
                                    ),
                                    button(
                                        TouchButton::new(KeyCode::ArrowRight),
                                        "Right",
                                        art,
                                        Glyph::Right,
                                        19.0
                                    ),
                                ],
                            ),
                        ],
                    ),
                    (
                        group(FlexDirection::Column),
                        children![
                            (
                                square(
                                    TouchButton::new(KeyCode::ShiftLeft)
                                        .sliding(KeyCode::KeyQ, KeyCode::KeyE),
                                    "Hold",
                                    15.0,
                                ),
                                children![(button_glyph(art, Glyph::Unlocked, 15.0), HoldGlyph)],
                            ),
                            button(
                                TouchButton::new(KeyCode::Space),
                                "Fire",
                                art,
                                Glyph::Fire,
                                24.0
                            ),
                        ],
                    ),
                ],
            ),
        ],
    ));
}

/// Show the buttons in play once touched, the hover buttons only with
/// anti-gravity, pressed or locked buttons brighter, and hold's lock
/// closed while it's locked.
fn update_controls(
    controls: Res<TouchControls>,
    screen: Option<Res<State<Screen>>>,
    abilities: Option<Res<Abilities>>,
    mut overlays: Query<&mut Visibility, With<TouchOverlay>>,
    mut buttons: Query<
        (&TouchButton, &mut Visibility, &mut BackgroundColor),
        Without<TouchOverlay>,
    >,
    mut glyphs: Query<&mut ImageNode, With<HoldGlyph>>,
) {
    let wanted = Some(if controls.locked {
        Glyph::Locked.rect()
    } else {
        Glyph::Unlocked.rect()
    });
    for mut image in &mut glyphs {
        if image.rect != wanted {
            image.rect = wanted;
        }
    }
    let shown = controls.shown && in_play(screen);
    for mut visibility in &mut overlays {
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let hovering = abilities.is_some_and(|a| a.has(PowerUp::AntiGravity));
    for (button, mut visibility, mut colour) in &mut buttons {
        visibility.set_if_neq(if button.hover && !hovering {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        let wanted = if controls.held.contains(&button.key) {
            PRESSED_COLOUR
        } else {
            BUTTON_COLOUR
        };
        if colour.0 != wanted {
            colour.0 = wanted;
        }
    }
}

/// The hint while the touch buttons show.
pub const TOUCH_HINT: &str = "Tap pause to resume";

/// The pause overlay says how to resume with whatever is in use.
fn hint_pause(controls: Res<TouchControls>, mut hints: Query<&mut Text, With<PauseHint>>) {
    let wanted = if controls.shown { TOUCH_HINT } else { KEY_HINT };
    for mut hint in &mut hints {
        if hint.0 != wanted {
            wanted.clone_into(&mut hint.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::input::touch::{TouchInput, TouchPhase};
    use bevy::state::app::StatesPlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin, StatesPlugin))
            .insert_state(Screen::Playing)
            .add_plugins(TouchPlugin);
        app
    }

    /// A visible node `size` across centred on `centre`, in physical pixels.
    fn node(centre: Vec2, size: Vec2) -> (ComputedNode, UiGlobalTransform, InheritedVisibility) {
        (
            ComputedNode { size, ..default() },
            UiGlobalTransform::from_translation(centre),
            InheritedVisibility::VISIBLE,
        )
    }

    fn touch(app: &mut App, id: u64, phase: TouchPhase, position: Vec2) {
        app.world_mut().write_message(TouchInput {
            phase,
            position,
            window: Entity::PLACEHOLDER,
            force: None,
            id,
        });
        app.update();
    }

    fn keys(app: &App) -> &ButtonInput<KeyCode> {
        app.world().resource::<ButtonInput<KeyCode>>()
    }

    fn spawn_button(app: &mut App, button: TouchButton, centre: Vec2) {
        app.world_mut()
            .spawn((button, node(centre, Vec2::splat(100.0))));
    }

    #[test]
    fn fingers_hold_keys_together_and_sliding_onto_a_button_presses_it() {
        let mut app = app();
        spawn_button(
            &mut app,
            TouchButton::new(KeyCode::Space),
            Vec2::new(900.0, 600.0),
        );
        spawn_button(
            &mut app,
            TouchButton::new(KeyCode::ArrowLeft),
            Vec2::new(100.0, 600.0),
        );
        app.update();
        assert!(!app.world().resource::<TouchControls>().shown);

        touch(&mut app, 1, TouchPhase::Started, Vec2::new(910.0, 590.0));
        assert!(app.world().resource::<TouchControls>().shown);
        assert!(keys(&app).just_pressed(KeyCode::Space));
        // A second finger slides onto spin while the first holds fire.
        touch(&mut app, 2, TouchPhase::Started, Vec2::new(300.0, 600.0));
        assert!(!keys(&app).pressed(KeyCode::ArrowLeft));
        touch(&mut app, 2, TouchPhase::Moved, Vec2::new(120.0, 610.0));
        assert!(keys(&app).just_pressed(KeyCode::ArrowLeft));
        assert!(keys(&app).pressed(KeyCode::Space));
        assert!(!keys(&app).just_pressed(KeyCode::Space));

        touch(&mut app, 1, TouchPhase::Ended, Vec2::new(910.0, 590.0));
        assert!(!keys(&app).pressed(KeyCode::Space));
        assert!(keys(&app).pressed(KeyCode::ArrowLeft));
        touch(&mut app, 2, TouchPhase::Canceled, Vec2::new(120.0, 610.0));
        assert!(!keys(&app).pressed(KeyCode::ArrowLeft));
    }

    #[test]
    fn sliding_from_hold_turns_the_prism_and_keeps_it_held() {
        let mut app = app();
        let hold = TouchButton::new(KeyCode::ShiftLeft).sliding(KeyCode::KeyQ, KeyCode::KeyE);
        spawn_button(&mut app, hold, Vec2::new(700.0, 600.0));
        spawn_button(
            &mut app,
            TouchButton::new(KeyCode::Space),
            Vec2::new(700.0, 450.0),
        );
        touch(&mut app, 1, TouchPhase::Started, Vec2::new(700.0, 600.0));
        assert!(keys(&app).pressed(KeyCode::ShiftLeft));
        // A small wobble doesn't turn it.
        touch(&mut app, 1, TouchPhase::Moved, Vec2::new(700.0, 590.0));
        assert!(!keys(&app).pressed(KeyCode::KeyQ));
        // Up, off the button and over fire: anticlockwise, still held, no fire.
        touch(&mut app, 1, TouchPhase::Moved, Vec2::new(700.0, 450.0));
        assert!(keys(&app).pressed(KeyCode::ShiftLeft));
        assert!(keys(&app).pressed(KeyCode::KeyQ));
        assert!(!keys(&app).pressed(KeyCode::Space));
        touch(&mut app, 1, TouchPhase::Moved, Vec2::new(700.0, 650.0));
        assert!(!keys(&app).pressed(KeyCode::KeyQ));
        assert!(keys(&app).pressed(KeyCode::KeyE));
        // Lifted after a slide, it stays parked.
        touch(&mut app, 1, TouchPhase::Ended, Vec2::new(700.0, 650.0));
        assert!(keys(&app).pressed(KeyCode::ShiftLeft));
        assert!(!keys(&app).pressed(KeyCode::KeyE));
        assert!(app.world().resource::<TouchControls>().locked);

        // Sliding again while locked leaves it locked.
        touch(&mut app, 2, TouchPhase::Started, Vec2::new(700.0, 600.0));
        touch(&mut app, 2, TouchPhase::Moved, Vec2::new(700.0, 500.0));
        assert!(keys(&app).pressed(KeyCode::KeyQ));
        touch(&mut app, 2, TouchPhase::Ended, Vec2::new(700.0, 500.0));
        assert!(app.world().resource::<TouchControls>().locked);
        assert!(keys(&app).pressed(KeyCode::ShiftLeft));
        assert!(!keys(&app).pressed(KeyCode::KeyQ));
    }

    fn hold(app: &mut App) {
        let hold = TouchButton::new(KeyCode::ShiftLeft).sliding(KeyCode::KeyQ, KeyCode::KeyE);
        spawn_button(app, hold, Vec2::new(700.0, 600.0));
    }

    fn tap(app: &mut App, id: u64, position: Vec2) {
        touch(app, id, TouchPhase::Started, position);
        touch(app, id, TouchPhase::Ended, position);
    }

    fn locked(app: &App) -> bool {
        app.world().resource::<TouchControls>().locked
    }

    /// The hold symbol's place in the art and the colour of the buttons
    /// holding Shift.
    fn hold_looks(app: &mut App) -> (Option<Rect>, Vec<Color>) {
        let world = app.world_mut();
        let glyph = world
            .query_filtered::<&ImageNode, With<HoldGlyph>>()
            .single(world)
            .unwrap()
            .rect;
        let colours = world
            .query::<(&TouchButton, &BackgroundColor)>()
            .iter(world)
            .filter(|(button, _)| button.key == KeyCode::ShiftLeft)
            .map(|(_, colour)| colour.0)
            .collect();
        (glyph, colours)
    }

    #[test]
    fn tapping_hold_locks_the_prism_until_tapped_again() {
        let mut app = app();
        hold(&mut app);
        let spot = Vec2::new(705.0, 595.0);
        touch(&mut app, 1, TouchPhase::Started, spot);
        assert!(keys(&app).just_pressed(KeyCode::ShiftLeft));
        assert!(!locked(&app));
        touch(&mut app, 1, TouchPhase::Ended, spot);
        assert!(locked(&app));
        assert!(keys(&app).pressed(KeyCode::ShiftLeft));
        app.update();
        assert!(keys(&app).pressed(KeyCode::ShiftLeft));
        let (glyph, colours) = hold_looks(&mut app);
        assert_eq!(glyph, Some(Glyph::Locked.rect()));
        assert!(colours.iter().all(|&colour| colour == PRESSED_COLOUR));

        // Another finger on fire doesn't touch the lock.
        spawn_button(
            &mut app,
            TouchButton::new(KeyCode::Space),
            Vec2::new(700.0, 400.0),
        );
        tap(&mut app, 2, Vec2::new(700.0, 400.0));
        assert!(locked(&app));

        // A second tap holds while down, then unlocks when lifted.
        touch(&mut app, 3, TouchPhase::Started, spot);
        assert!(keys(&app).pressed(KeyCode::ShiftLeft));
        assert!(locked(&app));
        touch(&mut app, 3, TouchPhase::Ended, spot);
        assert!(!locked(&app));
        assert!(!keys(&app).pressed(KeyCode::ShiftLeft));
        let (glyph, colours) = hold_looks(&mut app);
        assert_eq!(glyph, Some(Glyph::Unlocked.rect()));
        assert!(colours.iter().all(|&colour| colour == BUTTON_COLOUR));

        // A cancelled touch isn't a tap.
        touch(&mut app, 4, TouchPhase::Started, spot);
        touch(&mut app, 4, TouchPhase::Canceled, spot);
        assert!(!locked(&app));
        assert!(!keys(&app).pressed(KeyCode::ShiftLeft));
    }

    #[test]
    fn leaving_play_unlocks_hold() {
        let mut app = app();
        hold(&mut app);
        tap(&mut app, 1, Vec2::new(700.0, 600.0));
        assert!(locked(&app));
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::GameOver);
        app.update();
        app.update();
        assert!(!locked(&app));
        assert!(!keys(&app).pressed(KeyCode::ShiftLeft));
    }

    #[test]
    fn a_key_press_unlocks_hold() {
        let mut app = app();
        hold(&mut app);
        tap(&mut app, 1, Vec2::new(700.0, 600.0));
        assert!(locked(&app));
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::ArrowLeft,
            logical_key: bevy::input::keyboard::Key::ArrowLeft,
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert!(!app.world().resource::<TouchControls>().shown);
        assert!(!locked(&app));
        assert!(!keys(&app).pressed(KeyCode::ShiftLeft));
    }

    #[test]
    fn hidden_buttons_ignore_touches() {
        let mut app = app();
        app.world_mut().spawn((
            TouchButton::new(KeyCode::ArrowUp).hovering(),
            ComputedNode {
                size: Vec2::splat(100.0),
                ..default()
            },
            UiGlobalTransform::from_translation(Vec2::splat(100.0)),
            InheritedVisibility::HIDDEN,
        ));
        touch(&mut app, 1, TouchPhase::Started, Vec2::splat(100.0));
        assert!(!keys(&app).pressed(KeyCode::ArrowUp));
    }

    #[test]
    fn a_key_press_hides_the_buttons() {
        let mut app = app();
        touch(&mut app, 1, TouchPhase::Started, Vec2::ZERO);
        touch(&mut app, 1, TouchPhase::Ended, Vec2::ZERO);
        let overlay = |app: &mut App| {
            let world = app.world_mut();
            *world
                .query_filtered::<&Visibility, With<TouchOverlay>>()
                .single(world)
                .unwrap()
        };
        assert_eq!(overlay(&mut app), Visibility::Inherited);

        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Space,
            logical_key: bevy::input::keyboard::Key::Space,
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert_eq!(overlay(&mut app), Visibility::Hidden);

        // Only in play.
        touch(&mut app, 2, TouchPhase::Started, Vec2::ZERO);
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Title);
        app.update();
        app.update();
        assert!(app.world().resource::<TouchControls>().shown);
        assert_eq!(overlay(&mut app), Visibility::Hidden);
    }

    #[test]
    fn tapping_a_menu_line_chooses_it_and_volumes_go_by_side() {
        let mut app = app();
        let line = Vec2::new(400.0, 40.0);
        let menu = Menu::new(
            vec![Choice::Play, Choice::Volume(crate::screens::Channel::Music)],
            0.0,
        );
        app.world_mut().spawn(menu);
        for (index, y) in [(0, 300.0), (1, 350.0)] {
            app.world_mut()
                .spawn((MenuLine(index), node(Vec2::new(640.0, y), line)));
        }
        let selected = |app: &mut App| {
            let world = app.world_mut();
            world.query::<&Menu>().single(world).unwrap().selected
        };

        // Nearer the second line than the first, and off to its left.
        touch(&mut app, 1, TouchPhase::Started, Vec2::new(380.0, 335.0));
        assert_eq!(selected(&mut app), 1);
        assert!(keys(&app).just_pressed(KeyCode::ArrowLeft));
        touch(&mut app, 1, TouchPhase::Ended, Vec2::new(380.0, 335.0));
        assert!(!keys(&app).pressed(KeyCode::ArrowLeft));

        touch(&mut app, 2, TouchPhase::Started, Vec2::new(700.0, 352.0));
        assert!(keys(&app).just_pressed(KeyCode::ArrowRight));
        touch(&mut app, 2, TouchPhase::Ended, Vec2::new(700.0, 352.0));

        touch(&mut app, 3, TouchPhase::Started, Vec2::new(640.0, 290.0));
        assert_eq!(selected(&mut app), 0);
        assert!(keys(&app).just_pressed(KeyCode::Enter));
        touch(&mut app, 3, TouchPhase::Ended, Vec2::new(640.0, 290.0));

        // Too far from any line.
        touch(&mut app, 4, TouchPhase::Started, Vec2::new(640.0, 600.0));
        assert!(!keys(&app).pressed(KeyCode::Enter));
        assert_eq!(selected(&mut app), 0);
    }

    #[test]
    fn the_pause_button_pauses_and_resumes() {
        let mut app = app();
        app.add_plugins(crate::pause::PausePlugin);
        spawn_button(
            &mut app,
            TouchButton::new(KeyCode::KeyP),
            Vec2::new(900.0, 100.0),
        );
        app.update();
        let paused = |app: &App| app.world().resource::<Time<Virtual>>().is_paused();
        let tap = |app: &mut App, id| {
            touch(app, id, TouchPhase::Started, Vec2::new(900.0, 100.0));
            touch(app, id, TouchPhase::Ended, Vec2::new(900.0, 100.0));
        };
        tap(&mut app, 1);
        assert!(paused(&app));
        tap(&mut app, 2);
        assert!(!paused(&app));
    }

    #[test]
    fn the_pause_hint_says_how_to_resume_with_what_is_in_use() {
        let mut app = app();
        let hint = app.world_mut().spawn((PauseHint, Text::new(KEY_HINT))).id();
        touch(&mut app, 1, TouchPhase::Started, Vec2::ZERO);
        assert_eq!(app.world().get::<Text>(hint).unwrap().0, TOUCH_HINT);
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Space,
            logical_key: bevy::input::keyboard::Key::Space,
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert_eq!(app.world().get::<Text>(hint).unwrap().0, KEY_HINT);
    }
}
