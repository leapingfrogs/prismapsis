//! Generated art tools.
//!
//! - `art` paints every theme's tilesets into `assets/art`.
//! - `art --check` changes nothing and fails if a committed image is out of
//!   date.
//! - `art preview DIR` writes a sample of each theme to DIR, drawn
//!   at several stages of restoring colour, for reviewing the art.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use prismapsis::art::paint::{self, Kind, Theme};

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

fn png(canvas: &paint::Canvas) -> Result<Vec<u8>, String> {
    canvas.png().map_err(|e| e.to_string())
}

/// Paint each theme's tilesets, or with `check` compare them with what's
/// committed. Returns whether everything was up to date (or written).
fn generate(assets: &Path, check: bool) -> Result<bool, String> {
    let mut ok = true;
    let tilesets = paint::themes()
        .into_iter()
        .flat_map(|theme| Kind::ALL.map(|kind| (theme, kind)));
    for (theme, kind) in tilesets {
        let path = assets.join(paint::tileset_path(&theme, kind));
        let bytes = png(&paint::tileset(&theme, kind))?;
        let shown = Path::new("assets").join(paint::tileset_path(&theme, kind));
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
    Ok(true)
}

fn preview_path(dir: &Path, theme: &Theme) -> PathBuf {
    dir.join(format!("{}.png", theme.name))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
