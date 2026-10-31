//! Writes every sound effect to `assets/audio` (or the directory given as
//! the first argument). Run with `just sfx`.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("assets/audio"), PathBuf::from);
    std::fs::create_dir_all(&dir)?;
    for sound in sfx::sounds() {
        let samples = sfx::render(&sound);
        let path = dir.join(format!("{}.wav", sound.name));
        std::fs::write(&path, sfx::wav_bytes(&samples))?;
        #[allow(clippy::cast_precision_loss)]
        let seconds = samples.len() as f32 / sfx::SAMPLE_RATE as f32;
        println!("{} ({seconds:.2} s)", path.display());
    }
    Ok(())
}
