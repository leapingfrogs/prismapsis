//! Level file tools.
//!
//! - `levels check <file>...` reports every problem with each file.
//! - `levels show <file>...` prints each level's map in colour.

use std::path::Path;
use std::process::ExitCode;

use prismapsis::level::{self, preview};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(command), files) = (args.first(), args.get(1..).unwrap_or_default()) else {
        return usage();
    };
    if files.is_empty() {
        return usage();
    }
    let show = match command.as_str() {
        "check" => false,
        "show" => true,
        _ => return usage(),
    };
    let mut ok = true;
    for file in files {
        match level::load(Path::new(file)) {
            Ok(level) if show => print!("{}", preview::ansi(&level)),
            Ok(level) => println!(
                "{file}: ok, \"{}\" ({} x {} tiles)",
                level.name,
                level.map.width(),
                level.map.height()
            ),
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

fn usage() -> ExitCode {
    eprintln!("usage: levels check|show <file>...");
    ExitCode::from(2)
}
