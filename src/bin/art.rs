//! Generated art tools.
//!
//! - `art` paints every theme's tilesets and the sprite sheet into
//!   `assets/art`.
//! - `art --check` changes nothing and fails if a committed image is out of
//!   date.
//! - `art preview DIR` writes a sample of each theme to DIR, drawn
//!   at several stages of restoring colour, and the sprites in their game
//!   colours, for reviewing the art.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bevy::color::{Color, ColorToComponents};
use prismapsis::art::paint::{self, Canvas, Kind, Theme};
use prismapsis::art::sprites::{self, CELL_PX, Figure};
use prismapsis::enemy::EnemyKind;
use prismapsis::light::Primary;
use prismapsis::powerup::PowerUp;
use prismapsis::render::{coloured_enemy, power_up_colour};

/// The sample room drawn by `preview`: ledges, a pillar, an overhang and
/// a pit, so every kind of edge and corner shows, and scenery panels
/// of several shapes and hues, one against a wall.
const SAMPLE: [&str; 8] = [
    "##############",
    "# rr      yyy#",
    "# rr###   ooo#",
    "# rr###   ## #",
    "#     #  b   #",
    "##       b#  #",
    "###  ##   ####",
    "##############",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => generate(&assets, false),
        ["--check"] => generate(&assets, true),
        ["preview", dir] => preview(Path::new(dir)),
        _ => {
            eprintln!("usage: art [--check] | art preview DIR");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(problem) => {
            eprintln!("{problem}");
            ExitCode::FAILURE
        }
    }
}

fn png(canvas: &Canvas) -> Result<Vec<u8>, String> {
    canvas.png().map_err(|e| e.to_string())
}

/// Paint each theme's tilesets and the sprite sheet, or with `check`
/// compare them with what's committed. Returns whether everything was up to
/// date (or written).
fn generate(assets: &Path, check: bool) -> Result<bool, String> {
    let mut ok = true;
    let tilesets = paint::themes()
        .into_iter()
        .flat_map(|theme| Kind::ALL.map(|kind| (theme, kind)))
        .map(|(theme, kind)| {
            (
                paint::tileset_path(&theme, kind),
                paint::tileset(&theme, kind),
            )
        });
    let images = tilesets.chain(std::iter::once((
        sprites::SHEET_PATH.to_owned(),
        sprites::sheet(),
    )));
    for (relative, canvas) in images {
        let path = assets.join(&relative);
        let bytes = png(&canvas)?;
        let shown = Path::new("assets").join(&relative);
        if std::fs::read(&path).is_ok_and(|old| old == bytes) {
            println!("{}: up to date", shown.display());
        } else if check {
            println!("{}: out of date; run `just art`", shown.display());
            ok = false;
        } else {
            write(&path, &bytes)?;
            println!("{}: written", shown.display());
        }
    }
    Ok(ok)
}

fn preview(dir: &Path) -> Result<bool, String> {
    for theme in paint::themes() {
        let path = preview_path(dir, &theme);
        write(&path, &png(&paint::preview(&theme, &SAMPLE))?)?;
        println!("{}", path.display());
    }
    let path = dir.join("sprites.png");
    write(&path, &png(&sprite_preview())?)?;
    println!("{}", path.display());
    Ok(true)
}

fn preview_path(dir: &Path, theme: &Theme) -> PathBuf {
    dir.join(format!("{}.png", theme.name))
}

/// Behind the sprite preview: a mid-tone, like a lit level.
const PREVIEW_BACKGROUND: [f32; 3] = [0.24, 0.25, 0.32];

/// Each sprite in a game colour, one per cell: the hero, the prism, every
/// pickup, motes and lenses of each primary (and none), and an orb. Each
/// entry lists its layers, bottom first.
fn sprite_preview() -> Canvas {
    let white = Color::WHITE;
    let mut cells: Vec<Vec<(Figure, u32, Color)>> = vec![
        vec![
            (Figure::HeroSurface, 0, Color::srgb(0.95, 0.96, 1.0)),
            (Figure::HeroShade, 0, white),
        ],
        vec![(Figure::Prism, 0, Color::srgba(0.8, 0.92, 1.0, 0.85))],
    ];
    cells.extend(PowerUp::ALL.map(|power| {
        vec![
            (Figure::Bubble, 0, power_up_colour(power)),
            (Figure::Glyph(power), 0, white),
        ]
    }));
    let colours = [
        None,
        Some(Primary::Red),
        Some(Primary::Green),
        Some(Primary::Blue),
    ];
    for (kind, figure) in [
        (EnemyKind::Mote, Figure::Mote),
        (EnemyKind::Lens, Figure::Lens),
    ] {
        for (i, colour) in (0u32..).zip(colours) {
            cells.push(vec![(figure, i, coloured_enemy(kind, colour))]);
        }
    }
    cells.extend((0..4).map(|frame| vec![(Figure::Orb, frame, Color::srgb(0.8, 0.5, 1.0))]));

    let sheet = sprites::sheet();
    let columns = 8;
    let rows = u32::try_from(cells.len()).unwrap_or(0).div_ceil(columns);
    let mut canvas = Canvas::new(columns * CELL_PX, rows * CELL_PX);
    for pixel in &mut canvas.pixels {
        *pixel = [
            PREVIEW_BACKGROUND[0],
            PREVIEW_BACKGROUND[1],
            PREVIEW_BACKGROUND[2],
            1.0,
        ];
    }
    for (index, layers) in (0u32..).zip(&cells) {
        let (left, top) = ((index % columns) * CELL_PX, (index / columns) * CELL_PX);
        for (figure, frame, tint) in layers {
            let tint = tint.to_srgba().to_f32_array();
            let corner = figure.rect(*frame).min;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (sx, sy) = (corner.x as u32, corner.y as u32);
            for y in 0..CELL_PX {
                for x in 0..CELL_PX {
                    let [grey, _, _, alpha] = sheet.get(sx + x, sy + y);
                    let alpha = alpha * tint[3];
                    let under = canvas.get(left + x, top + y);
                    let mut out = under;
                    for c in 0..3 {
                        out[c] = grey * tint[c] * alpha + under[c] * (1.0 - alpha);
                    }
                    canvas.set(left + x, top + y, out);
                }
            }
        }
    }
    canvas
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
