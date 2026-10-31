//! Draw a level in a terminal with ANSI colours, for `just level-show`.

use std::fmt::Write;

use bevy::color::{Color, Srgba};

use super::format::{Level, Tile};
use crate::light::Hue;
use crate::spectrum_view::hue_colour;

const GREY_WALL: [u8; 3] = [90, 95, 110];
const MARKER: [u8; 3] = [245, 245, 250];
const RESET: &str = "\x1b[0m";

fn rgb(color: Color) -> [u8; 3] {
    let Srgba {
        red, green, blue, ..
    } = color.to_srgba();
    // Colour channels are in 0..=1, so this stays in u8 range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    [channel(red), channel(green), channel(blue)]
}

fn shade(hue: Option<Hue>) -> [u8; 3] {
    hue.map_or(GREY_WALL, |hue| rgb(hue_colour(hue)))
}

const fn dim([r, g, b]: [u8; 3]) -> [u8; 3] {
    [r / 3, g / 3, b / 3]
}

/// Foreground and background for one cell, if it's coloured at all.
fn cell_colours(
    tile: Option<Tile>,
    scenery: Option<[u8; 3]>,
) -> (Option<[u8; 3]>, Option<[u8; 3]>) {
    match tile {
        Some(Tile::Wall { hue, .. }) => (Some([20, 20, 25]), Some(shade(hue))),
        Some(Tile::Phantom { hue, .. }) => (Some(shade(Some(hue))), scenery),
        Some(Tile::Scenery(hue)) => (None, Some(dim(shade(hue)))),
        Some(Tile::Lens(colour)) => (Some(shade(Some(colour.into()))), scenery),
        Some(Tile::Pickup { .. } | Tile::Start | Tile::Exit | Tile::Checkpoint) => {
            (Some(MARKER), scenery)
        }
        Some(Tile::Empty) | None => (None, scenery),
    }
}

/// The level's map with each tile coloured by its hue, background scenery
/// dimmed behind it.
#[must_use]
pub fn ansi(level: &Level) -> String {
    let mut out = String::new();
    let plain = (None, None);
    for row in 0..level.map.height() {
        // Colours are only written when they change along the row.
        let mut current = plain;
        for col in 0..level.map.width() {
            let ch = level.map.get(row, col);
            let scenery = match level.tile(level.background.get(row, col)) {
                Some(Tile::Wall { hue, .. } | Tile::Scenery(hue)) => Some(dim(shade(hue))),
                _ => None,
            };
            let colours = cell_colours(level.tile(ch), scenery);
            if colours != current {
                if current != plain {
                    out.push_str(RESET);
                }
                if let Some([r, g, b]) = colours.0 {
                    let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
                }
                if let Some([r, g, b]) = colours.1 {
                    let _ = write!(out, "\x1b[48;2;{r};{g};{b}m");
                }
                current = colours;
            }
            out.push(ch);
        }
        if current != plain {
            out.push_str(RESET);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::parse;

    #[test]
    fn preview_colours_walls_and_scenery_and_keeps_the_layout() {
        let level = parse(
            "name = \"T\"\nmap = '''\nRRRR\nR@XR\n#gl#\n'''\nbackground = '''\n\n C\n'''\n\
             [legend]\n\"l\" = { lens = \"blue\" }\n",
        )
        .unwrap();
        let text = ansi(&level);
        assert_eq!(text.lines().count(), 3);
        let plain: String = text
            .split('\x1b')
            .map(|part| part.split_once('m').map_or(part, |(_, rest)| rest))
            .collect();
        assert_eq!(plain, "RRRR\nR@XR\n#gl#\n");
        let red = rgb(hue_colour(Hue::Red));
        assert!(text.starts_with(&format!(
            "\x1b[38;2;20;20;25m\x1b[48;2;{};{};{}mR",
            red[0], red[1], red[2]
        )));
        let cyan = dim(shade(Some(Hue::Cyan)));
        assert!(text.contains(&format!("48;2;{};{};{}m@", cyan[0], cyan[1], cyan[2])));
        let grey = GREY_WALL;
        assert!(text.contains(&format!("48;2;{};{};{}m#", grey[0], grey[1], grey[2])));
    }
}
