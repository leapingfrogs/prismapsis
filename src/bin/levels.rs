//! Level file tools.
//!
//! - `levels check [file]...` reports every problem with each file, and
//!   whether the level can be finished.
//! - `levels show [file]...` prints each level's map in colour.
//! - `levels generate [--check] [file]...` remakes each generated level from
//!   the `[generated]` settings in its file, which may hold nothing else
//!   yet. With `--check` it changes nothing and fails if a file is out of
//!   date.
//!
//! With no files, each works on every level in `assets/levels` (`generate`
//! on those with settings). A campaign file stands for the levels it lists,
//! in play order.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use prismapsis::level::{self, CAMPAIGN_FILE, generate, preview, solve};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Check,
    Show,
    Generate { check: bool },
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.first().map(String::as_str) {
        Some("check") => (Command::Check, &args[1..]),
        Some("show") => (Command::Show, &args[1..]),
        Some("generate") if args.get(1).is_some_and(|a| a == "--check") => {
            (Command::Generate { check: true }, &args[2..])
        }
        Some("generate") => (Command::Generate { check: false }, &args[1..]),
        _ => return usage(),
    };
    let mut ok = true;
    let named = !rest.is_empty();
    for file in files(rest, &mut ok) {
        let name = file
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .map_or(file.as_path(), |relative| {
                relative.strip_prefix("/").unwrap_or(relative)
            })
            .display()
            .to_string();
        let result = match command {
            Command::Check => check(&file, &name),
            Command::Show => level::load(&file).map(|level| print!("{}", preview::ansi(&level))),
            Command::Generate { check } => regenerate(&file, &name, check, named),
        };
        if let Err(problem) = result {
            eprintln!("{problem}");
            ok = false;
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn check(file: &Path, name: &str) -> Result<(), String> {
    let level = level::load(file)?;
    let size = format!("{} x {} tiles", level.map.width(), level.map.height());
    match solve::solve(&level) {
        Ok(solution) => {
            let comfort = if solve::solve_within(&level, solve::Limits::COMFORT).is_ok() {
                "comfortable"
            } else {
                "some jumps need nearly the full bounce or top spin"
            };
            println!(
                "{name}: ok, \"{}\" ({size}): {solution}; {comfort}",
                level.name
            );
            Ok(())
        }
        Err(problem) => Err(format!(
            "{name}: \"{}\" can't be finished: {problem}",
            level.name
        )),
    }
}

/// Remake a generated level, or with `check` only see whether it's up to
/// date. Files without settings are skipped unless they were `named`.
fn regenerate(file: &Path, name: &str, check: bool, named: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{name}: {e}"))?;
    let (title, recipe) = match generate::read_recipe(&text) {
        Ok(found) => found,
        Err(problem) if named => return Err(format!("{name}: {problem}")),
        Err(_) => return Ok(()),
    };
    // Checking rebuilds the recorded attempt rather than searching again.
    let made = if check {
        generate::rebuild(&title, &recipe)
    } else {
        generate::generate(&title, &recipe)
    }
    .map_err(|e| format!("{name}: {e}"))?;
    if made == text {
        println!("{name}: up to date");
    } else if check {
        return Err(format!("{name}: out of date; run `just levelgen`"));
    } else {
        std::fs::write(file, made).map_err(|e| format!("{name}: {e}"))?;
        println!("{name}: generated");
    }
    Ok(())
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
    eprintln!("usage: levels check|show [file]... | levels generate [--check] [file]...");
    ExitCode::from(2)
}
