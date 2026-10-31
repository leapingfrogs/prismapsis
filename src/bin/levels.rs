//! Level file tools.
//!
//! - `levels check [file]...` reports every problem with each file, and
//!   whether the level can be finished.
//! - `levels show [file]...` prints each level's map in colour.
//!
//! With no files, each works on every level in `assets/levels`. A campaign
//! file stands for the levels it lists, in play order.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use prismapsis::level::{self, CAMPAIGN_FILE, preview, solve};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show = match args.first().map(String::as_str) {
        Some("check") => false,
        Some("show") => true,
        _ => return usage(),
    };
    let mut ok = true;
    for file in files(&args[1..], &mut ok) {
        let name = file
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .map_or(file.as_path(), |relative| {
                relative.strip_prefix("/").unwrap_or(relative)
            })
            .display();
        match level::load(&file) {
            Ok(level) if show => print!("{}", preview::ansi(&level)),
            Ok(level) => {
                let size = format!("{} x {} tiles", level.map.width(), level.map.height());
                match solve::solve(&level) {
                    Ok(solution) => println!("{name}: ok, \"{}\" ({size}): {solution}", level.name),
                    Err(problem) => {
                        eprintln!("{name}: \"{}\" can't be finished: {problem}", level.name);
                        ok = false;
                    }
                }
            }
            Err(problems) => {
                eprintln!("{problems}");
                ok = false;
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The level files to work on: those named, with each campaign file
/// replaced by its levels, or every level when none are named. Problems
/// finding them are reported and clear `ok`.
fn files(args: &[String], ok: &mut bool) -> Vec<PathBuf> {
    let mut report = |problem: String| {
        eprintln!("{problem}");
        *ok = false;
    };
    if args.is_empty() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/levels");
        return level::level_files(&dir).unwrap_or_else(|e| {
            report(format!("{}: {e}", dir.display()));
            Vec::new()
        });
    }
    let mut files = Vec::new();
    for arg in args {
        let path = PathBuf::from(arg);
        if path.file_name().is_some_and(|n| n == CAMPAIGN_FILE) {
            match level::campaign_levels(&path) {
                Ok(levels) => files.extend(levels),
                Err(problem) => report(problem),
            }
        } else {
            files.push(path);
        }
    }
    files
}

fn usage() -> ExitCode {
    eprintln!("usage: levels check|show [file]...");
    ExitCode::from(2)
}
