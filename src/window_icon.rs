//! The desktop window's icon, shown in its title bar and taskbar.
//!
//! winit has no effect on Wayland or macOS, where an icon comes from a
//! desktop file or an app bundle instead.

use std::io::Cursor;

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;
use winit::window::Icon;

const ICON: &[u8] = include_bytes!("../assets/icon/rounded_256.png");

/// Sets the primary window's icon once the window exists.
#[derive(Debug, Default)]
pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_icon);
    }
}

/// The icon's RGBA pixels and its size.
fn pixels() -> Option<(Vec<u8>, u32, u32)> {
    let mut reader = png::Decoder::new(Cursor::new(ICON)).read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(frame.buffer_size());
    Some((buffer, frame.width, frame.height))
}

/// winit's window is only made after the first update, so this tries until
/// it's there.
fn set_icon(
    mut done: Local<bool>,
    windows: Query<Entity, With<PrimaryWindow>>,
    _main_thread: NonSendMarker,
) {
    if *done {
        return;
    }
    let Ok(entity) = windows.single() else {
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(window) = winit_windows.get_window(entity) else {
            return;
        };
        let icon =
            pixels().and_then(|(rgba, width, height)| Icon::from_rgba(rgba, width, height).ok());
        window.set_window_icon(icon);
        *done = true;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_decodes_to_rgba_a_window_can_use() {
        let (rgba, width, height) = pixels().unwrap_or_default();
        assert_eq!((width, height), (256, 256));
        assert_eq!(rgba.len(), 256 * 256 * 4);
        assert!(Icon::from_rgba(rgba, width, height).is_ok());
    }
}
