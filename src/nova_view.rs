//! How the light nova looks: a bright ring spreading from the hero, and
//! its HUD bubble dimmed while it recharges.

use bevy::prelude::*;

use crate::nova::{NOVA_SPREAD, Nova};
use crate::powerup::PowerUp;
use crate::render::{AbilityPip, glowing};

/// The ring's thickness, as a share of its radius.
const RING_WIDTH: f32 = 0.08;

/// How far above white the ring glows, for bloom.
const RING_GLOW: f32 = 2.5;

/// How bright the HUD bubble is while the nova recharges.
const RECHARGING_ALPHA: f32 = 0.3;

/// The nova's ring.
#[derive(Component, Debug)]
struct NovaRing;

#[derive(Debug, Default)]
pub struct NovaViewPlugin;

impl Plugin for NovaViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ring)
            .add_systems(Update, (draw_ring, dim_pip));
    }
}

fn spawn_ring(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Name::new("Nova ring"),
        NovaRing,
        Mesh2d(meshes.add(Annulus::new(1.0 - RING_WIDTH, 1.0))),
        MeshMaterial2d(materials.add(glowing(Color::WHITE, RING_GLOW))),
        Transform::from_xyz(0.0, 0.0, 3.0),
        Visibility::Hidden,
    ));
}

/// Spread the ring with the burst, fading as it goes.
fn draw_ring(
    nova: Res<Nova>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut rings: Query<
        (
            &mut Transform,
            &mut Visibility,
            &MeshMaterial2d<ColorMaterial>,
        ),
        With<NovaRing>,
    >,
) {
    for (mut transform, mut visibility, material) in &mut rings {
        let Some(burst) = &nova.burst else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = burst.centre.extend(transform.translation.z);
        transform.scale = Vec3::splat(burst.radius().max(1.0));
        if let Some(mut material) = materials.get_mut(&material.0) {
            let fade = (1.0 - burst.age / NOVA_SPREAD).clamp(0.0, 1.0);
            material.color = glowing(Color::WHITE, RING_GLOW).with_alpha(fade);
        }
    }
}

/// The nova's HUD bubble shows faintly while it recharges.
fn dim_pip(
    nova: Res<Nova>,
    pips: Query<(Entity, &AbilityPip, &Children)>,
    mut images: Query<&mut ImageNode>,
) {
    let alpha = if nova.recharge > 0.0 {
        RECHARGING_ALPHA
    } else {
        1.0
    };
    for (entity, pip, children) in &pips {
        if pip.0 != PowerUp::LightNova {
            continue;
        }
        // The bubble and the glyph over it.
        for part in std::iter::once(entity).chain(children.iter()) {
            if let Ok(mut image) = images.get_mut(part)
                && (image.color.alpha() - alpha).abs() > f32::EPSILON
            {
                image.color.set_alpha(alpha);
            }
        }
    }
}
