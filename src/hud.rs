//! The heads-up display's look, shared by its parts. Presentation only.
//!
//! The game's font, the glass tubes the energy bar and spectrum gauges fill
//! with light, the touch buttons' symbols, and icons drawn from the sprite
//! sheet.

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::text::Font;

use crate::art::figures::Sheet;
use crate::art::hud::TUBE_BORDER_PX;
use crate::art::sprites::Figure;

/// Exo 2, used for all the game's text (SIL Open Font License; see
/// `assets/fonts/Exo2-OFL.txt`).
const FONT: &[u8] = include_bytes!("../assets/fonts/Exo2.ttf");

/// The HUD's art, painted by `just art`.
const TUBE: &[u8] = include_bytes!("../assets/art/hud_tube.png");
const FILL: &[u8] = include_bytes!("../assets/art/hud_fill.png");
const GLYPHS: &[u8] = include_bytes!("../assets/art/touch_glyphs.png");

/// How far the light sits inside a tube's glass, as a fraction of its
/// height.
const TUBE_INSET: f32 = 0.22;

#[derive(Debug, Default)]
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        // Replacing the default font before any text is laid out makes
        // every piece of text use it.
        if let Some(mut fonts) = app.world_mut().get_resource_mut::<Assets<Font>>()
            && let Err(problem) = fonts.insert(AssetId::default(), Font::from_bytes(FONT.to_vec()))
        {
            error!("font: {problem:?}");
        }
        app.add_systems(PreStartup, load_art);
    }
}

/// The glass tube, the light that fills it and the touch buttons' symbols.
#[derive(Resource, Debug, Clone, Default)]
pub struct HudArt {
    pub tube: Handle<Image>,
    pub fill: Handle<Image>,
    pub glyphs: Handle<Image>,
}

/// Without images (headless tests), the handles are left empty.
fn load_art(mut commands: Commands, mut images: Option<ResMut<Assets<Image>>>) {
    let mut load = |name: &str, png: &[u8]| {
        let image = Image::from_buffer(
            png,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::linear(),
            RenderAssetUsages::RENDER_WORLD,
        );
        match (images.as_mut(), image) {
            (Some(images), Ok(image)) => images.add(image),
            (None, _) => Handle::default(),
            (_, Err(problem)) => {
                error!("{name}: {problem}");
                Handle::default()
            }
        }
    };
    commands.insert_resource(HudArt {
        tube: load("HUD tube", TUBE),
        fill: load("HUD fill", FILL),
        glyphs: load("touch glyphs", GLYPHS),
    });
}

/// A glass tube `width` long and `height` tall, holding `fill` (marked
/// as the caller likes) as light tinted `colour`. Set the fill's width as
/// a percentage to show how full it is.
pub fn tube(
    art: &HudArt,
    width: Val,
    height: f32,
    colour: Color,
    fill: impl Bundle,
) -> impl Bundle {
    let inset = Val::Vh(height * TUBE_INSET);
    (
        Node {
            width,
            height: Val::Vh(height),
            padding: UiRect::axes(Val::Vh(height * 0.3), inset),
            ..default()
        },
        children![
            (
                fill,
                Node {
                    width: Val::Percent(0.0),
                    height: Val::Percent(100.0),
                    border_radius: BorderRadius::all(Val::Vh(height)),
                    ..default()
                },
                ImageNode {
                    image: art.fill.clone(),
                    color: colour,
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                ImageNode {
                    image: art.tube.clone(),
                    image_mode: NodeImageMode::Sliced(TextureSlicer {
                        border: BorderRect::all(TUBE_BORDER_PX),
                        // The middle is the same all along, so tiling it
                        // avoids stretching seams.
                        center_scale_mode: SliceScaleMode::Tile { stretch_value: 1.0 },
                        sides_scale_mode: SliceScaleMode::Tile { stretch_value: 1.0 },
                        ..default()
                    }),
                    ..default()
                },
            ),
        ],
    )
}

/// A figure from the sprite sheet as a HUD icon `size` tall, tinted
/// `colour`.
#[must_use]
pub fn icon(sheet: &Sheet, figure: Figure, colour: Color, size: f32) -> impl Bundle {
    (
        Node {
            width: Val::Vh(size),
            height: Val::Vh(size),
            ..default()
        },
        ImageNode {
            image: sheet.0.clone(),
            color: colour,
            rect: Some(figure.rect(0)),
            ..default()
        },
    )
}

/// A figure laid over its parent icon, filling it.
#[must_use]
pub fn overlay(sheet: &Sheet, figure: Figure, colour: Color) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        ImageNode {
            image: sheet.0.clone(),
            color: colour,
            rect: Some(figure.rect(0)),
            ..default()
        },
    )
}
