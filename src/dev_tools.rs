//! Developer tools, compiled only with the `dev` feature: a live tuning panel
//! for bounce parameters. Toggle with F1; R resets the hero.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use crate::beam::BeamTuning;
use crate::hero::{HERO_SPAWN, Hero, Position, PreviousPosition, Spin, Velocity};
use crate::level::Target;
use crate::physics::{BounceTuning, MAX_SPIN};
use crate::prism::{PrismFrozen, PrismTuning};

#[derive(Debug, Default)]
pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_plugins(EguiPlugin::default())
            .insert_resource(PanelVisible(true))
            .add_systems(Update, (toggle_panel, reset_hero))
            .add_systems(EguiPrimaryContextPass, tuning_panel);
    }
}

#[derive(Resource, Debug)]
struct PanelVisible(bool);

const PRESENT_MODES: [PresentMode; 6] = [
    PresentMode::AutoVsync,
    PresentMode::AutoNoVsync,
    PresentMode::Fifo,
    PresentMode::FifoRelaxed,
    PresentMode::Mailbox,
    PresentMode::Immediate,
];

fn toggle_panel(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<PanelVisible>) {
    if keys.just_pressed(KeyCode::F1) {
        visible.0 = !visible.0;
    }
}

fn reset_hero(
    keys: Res<ButtonInput<KeyCode>>,
    mut heroes: Query<
        (
            &mut Position,
            &mut PreviousPosition,
            &mut Velocity,
            &mut Spin,
        ),
        With<Hero>,
    >,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    for (mut pos, mut prev, mut vel, mut spin) in &mut heroes {
        // Move both so the reset snaps rather than sweeping across the room.
        *pos = Position(HERO_SPAWN);
        *prev = PreviousPosition(HERO_SPAWN);
        *vel = Velocity::default();
        *spin = Spin::default();
    }
}

fn tuning_panel(
    mut contexts: EguiContexts,
    visible: Res<PanelVisible>,
    mut tuning: ResMut<BounceTuning>,
    mut prism_tuning: ResMut<PrismTuning>,
    prism_frozen: Res<PrismFrozen>,
    mut beam_tuning: ResMut<BeamTuning>,
    targets: Query<(&Name, &Target)>,
    heroes: Query<(&Position, &Velocity, &Spin), With<Hero>>,
    diagnostics: Res<DiagnosticsStore>,
    mut fixed_time: ResMut<Time<Fixed>>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) -> Result {
    if !visible.0 {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Bounce tuning (F1)").show(ctx, |ui| {
        movement_section(ui, &mut tuning);
        ui.separator();
        prism_section(ui, &mut prism_tuning, prism_frozen.0);
        ui.separator();
        beam_section(ui, &mut beam_tuning, &targets);
        if ui.button("Reset tuning").clicked() {
            *tuning = BounceTuning::default();
            *prism_tuning = PrismTuning::default();
            *beam_tuning = BeamTuning::default();
        }
        ui.separator();
        for (pos, vel, spin) in &heroes {
            ui.label(format!("spin step: {:+} / ±{MAX_SPIN}", spin.0));
            ui.label(format!("position: {:.0}, {:.0}", pos.0.x, pos.0.y));
            ui.label(format!("velocity: {:.0}, {:.0}", vel.0.x, vel.0.y));
        }
        ui.separator();
        performance_section(ui, &diagnostics, &mut fixed_time);
        if let Ok(window) = window.single_mut() {
            window_section(ui, window);
        }
        ui.label("←/→ or A/D: spin   R: reset hero");
        ui.label("Space: fire   Shift: freeze prism   ↑/↓ or W/S while frozen: nudge");
    });
    Ok(())
}

fn movement_section(ui: &mut egui::Ui, tuning: &mut BounceTuning) {
    ui.add(egui::Slider::new(&mut tuning.gravity, 500.0..=8000.0).text("gravity"));
    ui.add(egui::Slider::new(&mut tuning.apex_height, 50.0..=900.0).text("apex height"));
    ui.add(egui::Slider::new(&mut tuning.speed_per_step, 10.0..=300.0).text("speed per step"));
    ui.checkbox(&mut tuning.wall_flips_spin, "walls flip spin");
}

fn prism_section(ui: &mut egui::Ui, tuning: &mut PrismTuning, frozen: bool) {
    ui.add(egui::Slider::new(&mut tuning.orbit_radius, 40.0..=250.0).text("prism orbit radius"));
    ui.add(egui::Slider::new(&mut tuning.orbit_speed, -2.0..=2.0).text("prism orbit (rev/s)"));
    ui.add(egui::Slider::new(&mut tuning.nudge_speed, 0.05..=1.0).text("prism nudge (rev/s)"));
    ui.label(if frozen {
        "prism: frozen"
    } else {
        "prism: orbiting"
    });
}

fn beam_section(ui: &mut egui::Ui, tuning: &mut BeamTuning, targets: &Query<(&Name, &Target)>) {
    ui.add(egui::Slider::new(&mut tuning.max_energy, 20.0..=300.0).text("max energy"));
    ui.add(egui::Slider::new(&mut tuning.drain_rate, 5.0..=150.0).text("drain / s"));
    ui.add(egui::Slider::new(&mut tuning.recharge_rate, 5.0..=150.0).text("recharge / s"));
    ui.add(egui::Slider::new(&mut tuning.recharge_delay, 0.0..=3.0).text("recharge delay (s)"));
    ui.add(egui::Slider::new(&mut tuning.min_energy_to_fire, 0.0..=50.0).text("min to fire"));
    ui.add(egui::Slider::new(&mut tuning.damage_rate, 1.0..=30.0).text("damage ticks / s"));
    ui.add(egui::Slider::new(&mut tuning.split_angle, 0.0..=60.0).text("split angle (°)"));
    let mut hits: Vec<_> = targets
        .iter()
        .map(|(name, target)| format!("{}: {}", name.as_str(), target.hits))
        .collect();
    hits.sort();
    ui.label(format!("hits: {}", hits.join(", ")));
}

fn performance_section(
    ui: &mut egui::Ui,
    diagnostics: &DiagnosticsStore,
    fixed_time: &mut Time<Fixed>,
) {
    let smoothed = |path| {
        diagnostics
            .get(path)
            .and_then(bevy::diagnostic::Diagnostic::smoothed)
    };
    if let (Some(fps), Some(ms)) = (
        smoothed(&FrameTimeDiagnosticsPlugin::FPS),
        smoothed(&FrameTimeDiagnosticsPlugin::FRAME_TIME),
    ) {
        ui.label(format!("{fps:.0} fps ({ms:.2} ms)"));
    }
    // Uneven frame times cause judder even at a steady average fps.
    if let Some((min, max)) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .map(|d| {
            d.values()
                .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)))
        })
        .filter(|(min, max)| min <= max)
    {
        ui.label(format!("frame time range: {min:.2}–{max:.2} ms"));
    }

    let mut hz = 1.0 / fixed_time.timestep().as_secs_f64();
    if ui
        .add(egui::Slider::new(&mut hz, 30.0..=240.0).text("physics Hz"))
        .changed()
    {
        fixed_time.set_timestep_hz(hz);
    }
}

fn window_section(ui: &mut egui::Ui, mut window: Mut<Window>) {
    ui.label(format!(
        "window: {}×{} px (scale {:.2}), F11 fullscreen",
        window.resolution.physical_width(),
        window.resolution.physical_height(),
        window.resolution.scale_factor()
    ));
    // Only write on change: any write to Window re-applies it.
    let mut mode = window.present_mode;
    egui::ComboBox::from_label("present mode")
        .selected_text(format!("{mode:?}"))
        .show_ui(ui, |ui| {
            for option in PRESENT_MODES {
                ui.selectable_value(&mut mode, option, format!("{option:?}"));
            }
        });
    if mode != window.present_mode {
        window.present_mode = mode;
    }
}
