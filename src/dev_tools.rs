//! Developer tools, compiled only with the `dev` feature: a live tuning panel
//! for bounce parameters. Toggle with F1 or backtick; R resets the hero.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use crate::beam::BeamTuning;
use crate::enemy::{EnemyTuning, Kills, Lives, ResetRoom};
use crate::hero::{Hero, Position, PreviousPosition, Spin, Velocity};
use crate::level::{CurrentLevel, RespawnPoint};
use crate::light::{LightTuning, Primary, Spectrum};
use crate::physics::{BounceTuning, MAX_SPIN};
use crate::powerup::{Abilities, PowerUp, PowerUpTuning};
use crate::prism::{PrismFrozen, PrismTuning};
use crate::sound::{Auditions, SoundId, SoundSettings};

#[derive(Debug, Default)]
pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_plugins(EguiPlugin::default())
            .insert_resource(PanelVisible(true))
            .init_resource::<AuditionChoice>()
            .add_systems(Update, (toggle_panel, reset_hero))
            .add_systems(EguiPrimaryContextPass, tuning_panel);
    }
}

#[derive(Resource, Debug)]
struct PanelVisible(bool);

/// The sound picked for auditioning in the panel.
#[derive(Resource, Debug)]
struct AuditionChoice(SoundId);

impl Default for AuditionChoice {
    fn default() -> Self {
        Self(SoundId::MoteDefeat)
    }
}

/// The resources the light section of the panel edits.
#[derive(SystemParam)]
struct LightPanel<'w> {
    tuning: ResMut<'w, LightTuning>,
    spectrum: ResMut<'w, Spectrum>,
}

/// The resources the sound section of the panel edits.
#[derive(SystemParam)]
struct SoundPanel<'w> {
    settings: ResMut<'w, SoundSettings>,
    auditions: ResMut<'w, Auditions>,
    choice: ResMut<'w, AuditionChoice>,
}

const PRESENT_MODES: [PresentMode; 6] = [
    PresentMode::AutoVsync,
    PresentMode::AutoNoVsync,
    PresentMode::Fifo,
    PresentMode::FifoRelaxed,
    PresentMode::Mailbox,
    PresentMode::Immediate,
];

fn toggle_panel(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<PanelVisible>) {
    if keys.any_just_pressed([KeyCode::F1, KeyCode::Backquote]) {
        visible.0 = !visible.0;
    }
}

fn reset_hero(
    keys: Res<ButtonInput<KeyCode>>,
    respawn: Res<RespawnPoint>,
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
        *pos = Position(respawn.0);
        *prev = PreviousPosition(respawn.0);
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
    mut enemy_tuning: ResMut<EnemyTuning>,
    mut powerup_tuning: ResMut<PowerUpTuning>,
    mut abilities: ResMut<Abilities>,
    (mut sound, mut light): (SoundPanel, LightPanel),
    (kills, lives, level): (Res<Kills>, Res<Lives>, Res<CurrentLevel>),
    mut reset_room: ResMut<ResetRoom>,
    heroes: Query<(&Position, &Velocity, &Spin), With<Hero>>,
    diagnostics: Res<DiagnosticsStore>,
    mut fixed_time: ResMut<Time<Fixed>>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) -> Result {
    if !visible.0 {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Tuning (F1 or `)")
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .show(ctx, |ui| {
            movement_section(ui, &mut tuning);
            ui.separator();
            prism_section(ui, &mut prism_tuning, prism_frozen.0);
            ui.separator();
            beam_section(ui, &mut beam_tuning);
            ui.separator();
            enemy_section(ui, &mut enemy_tuning, *kills, *lives);
            ui.separator();
            powerup_section(ui, &mut powerup_tuning, &mut abilities);
            ui.separator();
            sound_section(ui, &mut sound);
            ui.separator();
            light_section(ui, &mut light);
            ui.horizontal(|ui| {
                if ui.button("Reset tuning").clicked() {
                    *tuning = BounceTuning::default();
                    *prism_tuning = PrismTuning::default();
                    *beam_tuning = BeamTuning::default();
                    *enemy_tuning = EnemyTuning::default();
                    *powerup_tuning = PowerUpTuning::default();
                    *sound.settings = SoundSettings::default();
                    *light.tuning = LightTuning::default();
                    level.0.apply_tuning(&mut enemy_tuning, &mut light.tuning);
                }
                if ui.button("Reset room").clicked() {
                    reset_room.0 = true;
                }
            });
            ui.separator();
            ui.label(format!("level: {}", level.0.name));
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
            ui.label("Space: fire   Shift: freeze prism   Q/E while frozen: nudge");
            ui.label("↑/↓ or W/S: hover (with anti-gravity)");
        });
    Ok(())
}

fn movement_section(ui: &mut egui::Ui, tuning: &mut BounceTuning) {
    ui.add(egui::Slider::new(&mut tuning.gravity, 500.0..=8000.0).text("gravity"));
    ui.add(egui::Slider::new(&mut tuning.apex_height, 50.0..=900.0).text("apex height"));
    ui.add(egui::Slider::new(&mut tuning.speed_per_step, 10.0..=300.0).text("speed per step"));
    ui.checkbox(&mut tuning.wall_flips_spin, "walls flip spin");
    ui.add(egui::Slider::new(&mut tuning.hover_speed, 50.0..=900.0).text("hover speed"));
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

fn beam_section(ui: &mut egui::Ui, tuning: &mut BeamTuning) {
    ui.add(egui::Slider::new(&mut tuning.max_energy, 20.0..=300.0).text("max energy"));
    ui.add(egui::Slider::new(&mut tuning.drain_rate, 5.0..=150.0).text("drain / s"));
    ui.add(egui::Slider::new(&mut tuning.recharge_rate, 5.0..=150.0).text("recharge / s"));
    ui.add(egui::Slider::new(&mut tuning.recharge_delay, 0.0..=3.0).text("recharge delay (s)"));
    ui.add(egui::Slider::new(&mut tuning.min_energy_to_fire, 0.0..=50.0).text("min to fire"));
    ui.add(egui::Slider::new(&mut tuning.damage_rate, 1.0..=30.0).text("damage ticks / s"));
    ui.add(egui::Slider::new(&mut tuning.split_angle, 0.0..=60.0).text("split angle (°)"));
}

fn enemy_section(ui: &mut egui::Ui, tuning: &mut EnemyTuning, kills: Kills, lives: Lives) {
    ui.add(egui::Slider::new(&mut tuning.mote_health, 1..=20).text("mote health (ticks)"));
    ui.add(egui::Slider::new(&mut tuning.lens_health, 1..=60).text("lens health (ticks)"));
    ui.add(egui::Slider::new(&mut tuning.mote_speed, 50.0..=600.0).text("mote speed"));
    ui.add(egui::Slider::new(&mut tuning.wave_size, 1..=12).text("motes per wave"));
    ui.add(egui::Slider::new(&mut tuning.wave_interval, 1.0..=20.0).text("wave interval (s)"));
    ui.add(
        egui::Slider::new(&mut tuning.lens_fire_interval, 0.5..=6.0).text("lens fire interval (s)"),
    );
    ui.add(egui::Slider::new(&mut tuning.orb_speed, 50.0..=600.0).text("orb speed"));
    ui.label(format!(
        "lives: {}   kills: {} motes, {} lenses",
        lives.0, kills.motes, kills.lenses
    ));
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

fn powerup_section(ui: &mut egui::Ui, tuning: &mut PowerUpTuning, abilities: &mut Abilities) {
    ui.add(egui::Slider::new(&mut tuning.drop_chance, 0.0..=1.0).text("drop chance"));
    ui.add(egui::Slider::new(&mut tuning.drop_lifetime, 2.0..=20.0).text("drop lifetime (s)"));
    ui.add(egui::Slider::new(&mut tuning.rapid_fire_boost, 1.0..=3.0).text("rapid fire boost"));
    ui.add(egui::Slider::new(&mut tuning.spread_angle, 2.0..=45.0).text("spread angle (°)"));
    ui.horizontal_wrapped(|ui| {
        for power in PowerUp::ALL {
            let mut has = abilities.has(power);
            if ui.checkbox(&mut has, format!("{power:?}")).changed() {
                if has {
                    abilities.grant(power);
                } else {
                    abilities.revoke(power);
                }
            }
        }
    });
}

fn sound_section(ui: &mut egui::Ui, panel: &mut SoundPanel) {
    let SoundPanel {
        settings,
        auditions,
        choice,
    } = panel;
    ui.add(egui::Slider::new(&mut settings.master, 0.0..=1.0).text("master volume"));
    ui.add(egui::Slider::new(&mut settings.effects, 0.0..=1.0).text("effects volume"));
    ui.add(egui::Slider::new(&mut settings.beam, 0.0..=1.0).text("beam volume"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("audition")
            .selected_text(choice.0.name())
            .show_ui(ui, |ui| {
                for &id in SoundId::ALL {
                    ui.selectable_value(&mut choice.0, id, id.name());
                }
            });
        if ui.button("Play").clicked() {
            auditions.0.push(choice.0);
        }
    });
}

fn light_section(ui: &mut egui::Ui, panel: &mut LightPanel) {
    let LightPanel { tuning, spectrum } = panel;
    ui.add(egui::Slider::new(&mut tuning.gauge_capacity, 1.0..=40.0).text("gauge capacity"));
    ui.add(egui::Slider::new(&mut tuning.mote_light, 0.5..=10.0).text("light per mote"));
    ui.add(egui::Slider::new(&mut tuning.lens_light, 0.5..=20.0).text("light per lens"));
    ui.horizontal(|ui| {
        for primary in Primary::ALL {
            let i = primary.index();
            let state = if spectrum.restored[i] {
                "restored".to_owned()
            } else {
                format!("{:.1}", spectrum.gauges[i])
            };
            if ui.button(format!("{primary:?}: {state}")).clicked() {
                spectrum.restore(primary, tuning.gauge_capacity);
            }
        }
        if ui.button("Grey").clicked() {
            **spectrum = Spectrum::default();
        }
    });
}
