//! Embed the campaign: the levels listed in `assets/levels/campaign.toml`,
//! in order, so the game needs no files beside it to play them.

use std::fmt::Write;
use std::path::Path;

fn main() {
    embed_windows_icon();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/levels");
    let campaign = dir.join("campaign.toml");
    println!("cargo::rerun-if-changed={}", campaign.display());
    let source = std::fs::read_to_string(&campaign)
        .unwrap_or_else(|e| panic!("{}: {e}", campaign.display()));
    let table: toml::Table =
        toml::from_str(&source).unwrap_or_else(|e| panic!("{}: {e}", campaign.display()));
    let Some(toml::Value::Array(levels)) = table.get("levels") else {
        panic!(
            "{}: needs a `levels` list of file names",
            campaign.display()
        );
    };
    let mut code = String::from("&[\n");
    for level in levels {
        let Some(file) = level.as_str() else {
            panic!("{}: level {level} is not a file name", campaign.display());
        };
        let path = dir.join(file);
        assert!(path.is_file(), "{}: {file} not found", campaign.display());
        println!("cargo::rerun-if-changed={}", path.display());
        writeln!(
            code,
            "    ({file:?}, include_str!({:?})),",
            path.display().to_string()
        )
        .unwrap_or_default();
    }
    code.push_str("]\n");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap_or_default()).join("campaign.rs");
    std::fs::write(&out, code).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
}

/// Give the Windows program its icon, as Explorer and the taskbar show it.
#[cfg(windows)]
fn embed_windows_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        let icon = "assets/icon/prismapsis.ico";
        println!("cargo::rerun-if-changed={icon}");
        winresource::WindowsResource::new()
            .set_icon(icon)
            .compile()
            .unwrap_or_else(|e| panic!("{icon}: {e}"));
    }
}

#[cfg(not(windows))]
const fn embed_windows_icon() {}
