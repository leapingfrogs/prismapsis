//! Writes every sound effect and music stem to `assets/audio` (or the
//! directory given as the first argument). Run with `just sfx`.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("assets/audio"), PathBuf::from);
    std::fs::create_dir_all(&dir)?;
    let effects = sfx::sounds()
        .into_iter()
        .map(|sound| (sound.name, sfx::render(&sound), sfx::SAMPLE_RATE));
    let music = sfx::music::stems().into_iter().map(|stem| {
        (
            stem.name,
            sfx::music::render_stem(&stem),
            sfx::music::MUSIC_RATE,
        )
    });
    for (name, samples, rate) in effects.chain(music) {
        let path = dir.join(format!("{name}.wav"));
        std::fs::write(&path, sfx::wav_bytes_at(&samples, rate))?;
        #[allow(clippy::cast_precision_loss)]
        let seconds = samples.len() as f32 / rate as f32;
        println!("{} ({seconds:.2} s)", path.display());
    }
    Ok(())
}
