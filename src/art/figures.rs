//! Draws figures from the sprite sheet, and plays their animations; also
//! loads the beam's and the exit's art.

use super::sprites::{CELL_SIZE, Figure};
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;

/// The sheet painted by `just art`.
const SHEET: &[u8] = include_bytes!("../../assets/art/sprites.png");

/// The beam's and exit's art, painted by `just art`.
const BEAM: &[u8] = include_bytes!("../../assets/art/beam.png");
const EXIT_FRAME: &[u8] = include_bytes!("../../assets/art/exit_frame.png");
const EXIT_LIGHT: &[u8] = include_bytes!("../../assets/art/exit_light.png");

#[derive(Debug, Default)]
pub struct FiguresPlugin;

impl Plugin for FiguresPlugin {
    fn build(&self, app: &mut App) {
        // Loaded now, so screens spawned when the first state is entered
        // have their art.
        let glow = load_glow(
            app.world_mut()
                .get_resource_mut::<Assets<Image>>()
                .as_deref_mut(),
        );
        app.insert_resource(glow)
            .add_systems(PreStartup, load_sheet)
            .add_systems(Update, animate);
    }
}

/// The loaded sprite sheet.
#[derive(Resource, Debug, Clone, Default)]
pub struct Sheet(pub Handle<Image>);

impl Sheet {
    /// A sprite showing a figure's first frame, tinted `colour`, at the
    /// sheet's scale.
    #[must_use]
    pub fn sprite(&self, figure: Figure, colour: Color) -> Sprite {
        Sprite {
            image: self.0.clone(),
            color: colour,
            custom_size: Some(Vec2::splat(CELL_SIZE)),
            rect: Some(figure.rect(0)),
            ..default()
        }
    }
}

/// Decode the sheet.
///
/// # Errors
/// If the image can't be decoded.
pub fn sheet_image(png: &[u8]) -> Result<Image, String> {
    Image::from_buffer(
        png,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        RenderAssetUsages::RENDER_WORLD,
    )
    .map_err(|e| e.to_string())
}

/// Without images (headless tests), figures still get sprites, just no
/// sheet.
fn load_sheet(mut commands: Commands, images: Option<ResMut<Assets<Image>>>) {
    let handle = match (images, sheet_image(SHEET)) {
        (Some(mut images), Ok(image)) => images.add(image),
        (None, _) => Handle::default(),
        (_, Err(problem)) => {
            error!("sprite sheet: {problem}");
            Handle::default()
        }
    };
    commands.insert_resource(Sheet(handle));
}

/// The beam's glow and the exit's frame and light.
#[derive(Resource, Debug, Clone, Default)]
pub struct GlowArt {
    pub beam: Handle<Image>,
    pub exit_frame: Handle<Image>,
    pub exit_light: Handle<Image>,
}

/// Without images (headless tests), the handles are left empty.
fn load_glow(mut images: Option<&mut Assets<Image>>) -> GlowArt {
    let mut load = |name: &str, png: &[u8]| match (images.as_mut(), sheet_image(png)) {
        (Some(images), Ok(image)) => images.add(image),
        (None, _) => Handle::default(),
        (_, Err(problem)) => {
            error!("{name}: {problem}");
            Handle::default()
        }
    };
    GlowArt {
        beam: load("beam", BEAM),
        exit_frame: load("exit frame", EXIT_FRAME),
        exit_light: load("exit light", EXIT_LIGHT),
    }
}

/// Plays a figure's frames over and over.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Animation {
    pub figure: Figure,
    /// Frames per second.
    pub rate: f32,
    /// Seconds from the start of one play to the next; frame 0 shows in
    /// between. Plays back to back if no longer than the animation.
    pub every: f32,
    /// Seconds into the cycle at the start, so neighbours don't move in
    /// step.
    pub offset: f32,
}

impl Animation {
    /// The frame to show `elapsed` seconds in.
    #[must_use]
    pub fn frame(&self, elapsed: f32) -> u32 {
        #[allow(clippy::cast_precision_loss)]
        let length = self.figure.frames() as f32 / self.rate;
        let cycle = self.every.max(length);
        let into = (elapsed + self.offset).rem_euclid(cycle);
        // Truncation picks the frame being shown.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frame = (into * self.rate) as u32;
        if frame < self.figure.frames() {
            frame
        } else {
            0
        }
    }
}

/// An offset into an animation's cycle from an entity, so each starts at a
/// different point.
#[must_use]
pub fn stagger(entity: Entity, every: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let unit = (super::mix(entity.to_bits()) % 1000) as f32 / 1000.0;
    unit * every
}

fn animate(time: Res<Time>, mut sprites: Query<(&Animation, &mut Sprite)>) {
    let elapsed = time.elapsed_secs();
    for (animation, mut sprite) in &mut sprites {
        let rect = Some(animation.figure.rect(animation.frame(elapsed)));
        if sprite.rect != rect {
            sprite.rect = rect;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_sheet_loads() {
        let image = sheet_image(SHEET).unwrap();
        let size = image.size();
        assert_eq!(
            size.x,
            super::super::sprites::COLUMNS * super::super::sprites::CELL_PX
        );
    }

    #[test]
    fn animations_loop_and_can_rest_between_plays() {
        let mote = Animation {
            figure: Figure::Mote,
            rate: 10.0,
            every: 0.0,
            offset: 0.0,
        };
        assert_eq!(mote.frame(0.0), 0);
        assert_eq!(mote.frame(0.25), 2);
        assert_eq!(mote.frame(0.65), 0, "six frames at 10 a second loop");
        let lens = Animation {
            figure: Figure::Lens,
            rate: 10.0,
            every: 2.0,
            offset: 0.5,
        };
        assert_eq!(lens.frame(0.0), 0, "resting");
        assert_eq!(lens.frame(1.75), 2, "playing");
        assert_eq!(lens.frame(1.9), 0, "resting again");
    }

    #[test]
    fn staggered_starts_differ_but_stay_in_the_cycle() {
        let offsets: Vec<f32> = (1..20)
            .map(|i| stagger(Entity::from_raw_u32(i).unwrap(), 3.0))
            .collect();
        assert!(offsets.iter().all(|o| (0.0..3.0).contains(o)));
        assert!(
            offsets
                .windows(2)
                .any(|w| (w[0] - w[1]).abs() > f32::EPSILON)
        );
    }
}
