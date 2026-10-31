//! A tiny synthesiser for Prismapsis's sound effects.
//!
//! Each sound is a list of tones (an oscillator with a pitch slide, an
//! envelope and optional vibrato), mixed, optionally low-passed and echoed,
//! then normalised. Pitched sounds use the Lydian scale on C so everything
//! the game plays is in one key; see `docs/design.md`.

use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 44_100;

/// Semitone offsets from the root for each degree of the Lydian scale.
pub const LYDIAN: [i32; 7] = [0, 2, 4, 6, 7, 9, 11];

/// Frequency of the note `semitones` above middle C (C4).
#[must_use]
pub fn note(semitones: i32) -> f32 {
    // Small whole numbers, exactly representable.
    #[allow(clippy::cast_precision_loss)]
    let semitones = semitones as f32;
    261.625_57 * (semitones / 12.0).exp2()
}

/// Oscillator shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wave {
    Sine,
    Triangle,
    Saw,
    /// Pulse wave with this duty cycle (0.5 is square).
    Square(f32),
    /// Random values held for one cycle of the tone's frequency, so higher
    /// frequencies give brighter noise.
    Noise,
}

/// One voice of a sound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    pub wave: Wave,
    /// When it starts, in seconds from the start of the sound.
    pub start: f32,
    pub length: f32,
    pub freq: f32,
    /// Frequency reached at the end, sliding exponentially from `freq`.
    pub freq_end: f32,
    /// Linear fade-in time (seconds).
    pub attack: f32,
    /// Exponential decay time constant (seconds); infinite holds steady.
    pub decay: f32,
    pub volume: f32,
    /// Vibrato depth (fraction of the frequency) and rate (Hz).
    pub vibrato: (f32, f32),
}

impl Tone {
    #[must_use]
    pub const fn new(wave: Wave, freq: f32, length: f32) -> Self {
        Self {
            wave,
            start: 0.0,
            length,
            freq,
            freq_end: freq,
            attack: 0.004,
            decay: f32::INFINITY,
            volume: 1.0,
            vibrato: (0.0, 0.0),
        }
    }

    #[must_use]
    pub const fn at(mut self, start: f32) -> Self {
        self.start = start;
        self
    }

    #[must_use]
    pub const fn slide(mut self, freq_end: f32) -> Self {
        self.freq_end = freq_end;
        self
    }

    #[must_use]
    pub const fn attack(mut self, attack: f32) -> Self {
        self.attack = attack;
        self
    }

    #[must_use]
    pub const fn decay(mut self, decay: f32) -> Self {
        self.decay = decay;
        self
    }

    #[must_use]
    pub const fn volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    #[must_use]
    pub const fn vibrato(mut self, depth: f32, rate: f32) -> Self {
        self.vibrato = (depth, rate);
        self
    }

    fn end(&self) -> f32 {
        self.start + self.length
    }
}

/// A feedback echo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Echo {
    /// Seconds between repeats.
    pub delay: f32,
    /// Level of each repeat relative to the last.
    pub feedback: f32,
}

/// A complete sound effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    /// File name without extension; must match the game's `SoundId`.
    pub name: &'static str,
    pub tones: Vec<Tone>,
    /// One-pole low-pass cutoff (Hz).
    pub lowpass: Option<f32>,
    pub echo: Option<Echo>,
    /// Peak level after normalising (0 to 1). Constant, frequent sounds are
    /// quieter so they don't tire the ear.
    pub peak: f32,
    /// For a seamless loop: its exact length. Tones should then complete
    /// whole cycles in that time; no fades or echo tails are added.
    pub looping: Option<f32>,
}

impl Sound {
    #[must_use]
    pub const fn new(name: &'static str, tones: Vec<Tone>) -> Self {
        Self {
            name,
            tones,
            lowpass: None,
            echo: None,
            peak: 0.8,
            looping: None,
        }
    }

    #[must_use]
    pub const fn lowpass(mut self, cutoff: f32) -> Self {
        self.lowpass = Some(cutoff);
        self
    }

    #[must_use]
    pub const fn echo(mut self, delay: f32, feedback: f32) -> Self {
        self.echo = Some(Echo { delay, feedback });
        self
    }

    #[must_use]
    pub const fn peak(mut self, peak: f32) -> Self {
        self.peak = peak;
        self
    }

    #[must_use]
    pub const fn looping(mut self, length: f32) -> Self {
        self.looping = Some(length);
        self
    }
}

/// Small deterministic noise source (xorshift32), so output is repeatable.
#[derive(Debug)]
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        // 24 random bits map exactly onto -1..1.
        #[allow(clippy::cast_precision_loss)]
        let unit = (self.0 >> 8) as f32 / (1u32 << 24) as f32;
        unit * 2.0 - 1.0
    }
}

fn seconds_to_samples(seconds: f32) -> usize {
    // Durations are short and non-negative.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let samples = (seconds * SAMPLE_RATE as f32).round().max(0.0) as usize;
    samples
}

/// Add `tone` into `out`. Without `looping`, each tone ends with a short
/// fade so it never clicks.
fn add_tone(out: &mut [f32], tone: &Tone, looping: bool, noise: &mut Noise) {
    #[allow(clippy::cast_precision_loss)]
    let rate = SAMPLE_RATE as f32;
    let start = seconds_to_samples(tone.start);
    let count = seconds_to_samples(tone.length);
    let fade = if looping {
        0
    } else {
        seconds_to_samples(0.004)
    };
    let mut phase = 0.0f64;
    let mut held = noise.next();
    for i in 0..count {
        let Some(sample) = out.get_mut(start + i) else {
            break;
        };
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 / rate;
        let progress = t / tone.length.max(f32::EPSILON);
        let (depth, vib_rate) = tone.vibrato;
        let freq = tone.freq
            * (tone.freq_end / tone.freq).powf(progress)
            * (1.0 + depth * (TAU * vib_rate * t).sin());

        // Phase is kept in f64 so long loops stay exactly periodic.
        #[allow(clippy::cast_possible_truncation)]
        let phase_f32 = phase as f32;
        let value = match tone.wave {
            Wave::Sine => (TAU * phase_f32).sin(),
            Wave::Triangle => 4.0 * (phase_f32 - 0.5).abs() - 1.0,
            Wave::Saw => 2.0 * phase_f32 - 1.0,
            Wave::Square(duty) => {
                if phase_f32 < duty {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Noise => held,
        };
        phase += f64::from(freq / rate);
        if phase >= 1.0 {
            phase -= phase.floor();
            held = noise.next();
        }

        let mut envelope = if looping {
            1.0
        } else {
            (t / tone.attack.max(f32::EPSILON)).min(1.0) * (-t / tone.decay).exp()
        };
        let left = count - i;
        if left < fade {
            #[allow(clippy::cast_precision_loss)]
            let tail = left as f32 / fade as f32;
            envelope *= tail;
        }
        *sample += value * envelope * tone.volume;
    }
}

/// One-pole low-pass. Looping sounds run it twice, so the filter state at
/// the start matches the end and the loop stays seamless.
fn lowpass(samples: &mut [f32], cutoff: f32, looping: bool) {
    #[allow(clippy::cast_precision_loss)]
    let rate = SAMPLE_RATE as f32;
    let alpha = 1.0 - (-TAU * cutoff / rate).exp();
    let source = samples.to_vec();
    let mut state = 0.0;
    if looping {
        for &x in &source {
            state += alpha * (x - state);
        }
    }
    for (out, &x) in samples.iter_mut().zip(&source) {
        state += alpha * (x - state);
        *out = state;
    }
}

/// Render `sound` to samples in -1..1.
#[must_use]
pub fn render(sound: &Sound) -> Vec<f32> {
    let looping = sound.looping.is_some();
    let tail = match (sound.echo, looping) {
        // Enough repeats to fade below 1%.
        (Some(echo), false) => echo.delay * (0.01f32.ln() / echo.feedback.ln()).ceil(),
        _ => 0.0,
    };
    let length = sound
        .looping
        .unwrap_or_else(|| sound.tones.iter().map(Tone::end).fold(0.0, f32::max) + tail);
    let mut samples = vec![0.0; seconds_to_samples(length)];
    let mut noise = Noise(0x1234_5678);
    for tone in &sound.tones {
        add_tone(&mut samples, tone, looping, &mut noise);
    }
    if let Some(cutoff) = sound.lowpass {
        lowpass(&mut samples, cutoff, looping);
    }
    if let (Some(echo), false) = (sound.echo, looping) {
        let delay = seconds_to_samples(echo.delay).max(1);
        for i in delay..samples.len() {
            samples[i] += samples[i - delay] * echo.feedback;
        }
    }
    let loudest = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if loudest > 0.0 {
        let scale = sound.peak / loudest;
        for sample in &mut samples {
            *sample *= scale;
        }
    }
    samples
}

/// Encode samples as a 16-bit mono PCM WAV file.
#[must_use]
pub fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &sample in samples {
        // Clamped to -1..1 first, so it fits in an i16.
        #[allow(clippy::cast_possible_truncation)]
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// The note `degree` steps up the Lydian scale from C4 (degree 7 is C5).
#[must_use]
pub fn lydian(degree: i32) -> f32 {
    let octave = degree.div_euclid(7);
    let step = LYDIAN[usize::try_from(degree.rem_euclid(7)).unwrap_or(0)];
    note(octave * 12 + step)
}

/// Every sound the game uses. Names match `SoundId` in the game.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn sounds() -> Vec<Sound> {
    use Wave::{Noise, Saw, Sine, Square, Triangle};
    let c5 = lydian(7);
    let e5 = lydian(9);
    let fs5 = lydian(10);
    let g5 = lydian(11);
    let c6 = lydian(14);
    let e6 = lydian(16);
    let fs6 = lydian(17);
    let g6 = lydian(18);
    vec![
        // --- Combat ---
        // Loops use whole-number frequencies so they repeat seamlessly
        // every second; 131 Hz is C3 to within a few cents.
        Sound::new(
            "beam_hum",
            vec![
                Tone::new(Saw, 131.0, 1.0).volume(0.5),
                Tone::new(Saw, 132.0, 1.0).volume(0.4),
                Tone::new(Sine, 262.0, 1.0).volume(0.35),
                Tone::new(Square(0.3), 393.0, 1.0)
                    .volume(0.08)
                    .vibrato(0.004, 6.0),
            ],
        )
        .lowpass(1400.0)
        .peak(0.5)
        .looping(1.0),
        // Root, third and fifth (red, green, blue): a glassy major triad.
        Sound::new(
            "beam_chord",
            vec![
                Tone::new(Sine, 523.0, 1.0).volume(0.5),
                Tone::new(Sine, 525.0, 1.0).volume(0.25),
                Tone::new(Sine, 659.0, 1.0).volume(0.45),
                Tone::new(Sine, 784.0, 1.0).volume(0.4).vibrato(0.002, 5.0),
                Tone::new(Sine, 1568.0, 1.0).volume(0.08),
            ],
        )
        .peak(0.45)
        .looping(1.0),
        Sound::new(
            "beam_start",
            vec![
                Tone::new(Square(0.5), 220.0, 0.09).slide(660.0).decay(0.08),
                Tone::new(Sine, 523.0, 0.12)
                    .at(0.04)
                    .decay(0.06)
                    .volume(0.5),
            ],
        )
        .lowpass(4000.0)
        .peak(0.5),
        Sound::new(
            "beam_stop",
            vec![Tone::new(Square(0.5), 520.0, 0.1).slide(160.0).decay(0.05)],
        )
        .lowpass(2500.0)
        .peak(0.4),
        Sound::new(
            "enemy_hit",
            vec![
                Tone::new(Noise, 6000.0, 0.03).decay(0.01),
                Tone::new(Square(0.25), 1760.0, 0.03)
                    .decay(0.012)
                    .volume(0.5),
            ],
        )
        .peak(0.3),
        // Pitched up the Lydian scale at runtime as kills chain.
        Sound::new(
            "mote_defeat",
            vec![
                Tone::new(Triangle, c5, 0.3).decay(0.09),
                Tone::new(Square(0.5), c6, 0.15).decay(0.05).volume(0.25),
                Tone::new(Noise, 8000.0, 0.05).decay(0.015).volume(0.4),
            ],
        )
        .echo(0.09, 0.25)
        .peak(0.6),
        Sound::new(
            "lens_defeat",
            vec![
                Tone::new(Noise, 3000.0, 0.45).slide(400.0).decay(0.15),
                Tone::new(Square(0.5), 400.0, 0.35)
                    .slide(80.0)
                    .decay(0.12)
                    .volume(0.6),
                Tone::new(Sine, c6, 0.4).at(0.08).decay(0.15).volume(0.5),
                Tone::new(Sine, e6, 0.4).at(0.14).decay(0.15).volume(0.45),
                Tone::new(Sine, g6, 0.5).at(0.2).decay(0.2).volume(0.4),
            ],
        )
        .lowpass(6000.0)
        .echo(0.12, 0.35)
        .peak(0.75),
        Sound::new(
            "orb_fire",
            vec![
                Tone::new(Sine, 300.0, 0.16)
                    .slide(520.0)
                    .decay(0.08)
                    .vibrato(0.05, 30.0),
            ],
        )
        .peak(0.45),
        Sound::new(
            "orb_blocked",
            vec![
                Tone::new(Sine, g6, 0.35).decay(0.08),
                Tone::new(Sine, c6 * 2.0, 0.3).decay(0.06).volume(0.5),
            ],
        )
        .echo(0.07, 0.3)
        .peak(0.5),
        // --- Hero ---
        // Pitched up with the spin step at runtime.
        Sound::new(
            "bounce",
            vec![
                Tone::new(Sine, 110.0, 0.12)
                    .slide(55.0)
                    .attack(0.003)
                    .decay(0.05),
            ],
        )
        .lowpass(500.0)
        .peak(0.3),
        Sound::new(
            "wall_bump",
            vec![
                Tone::new(Triangle, 180.0, 0.08).slide(100.0).decay(0.03),
                Tone::new(Noise, 2000.0, 0.03).decay(0.01).volume(0.4),
            ],
        )
        .lowpass(2000.0)
        .peak(0.4),
        Sound::new(
            "life_lost",
            vec![
                Tone::new(Square(0.5), g5, 0.16)
                    .decay(0.2)
                    .vibrato(0.01, 8.0),
                Tone::new(Square(0.5), e5, 0.16).at(0.15).decay(0.2),
                Tone::new(Square(0.5), c5, 0.45)
                    .at(0.3)
                    .slide(c5 / 2.0)
                    .decay(0.3)
                    .vibrato(0.02, 7.0),
            ],
        )
        .lowpass(3500.0)
        .peak(0.6),
        Sound::new(
            "respawn",
            vec![
                Tone::new(Sine, c5, 0.3).decay(0.15),
                Tone::new(Sine, e5, 0.3).at(0.07).decay(0.15),
                Tone::new(Sine, g5, 0.3).at(0.14).decay(0.15),
                Tone::new(Sine, c6, 0.4).at(0.21).decay(0.2),
            ],
        )
        .echo(0.1, 0.35)
        .peak(0.5),
        Sound::new(
            "invulnerability_end",
            vec![
                Tone::new(Triangle, g5, 0.07).decay(0.04),
                Tone::new(Triangle, c6, 0.1).at(0.07).decay(0.05),
            ],
        )
        .peak(0.35),
        Sound::new(
            "room_reset",
            vec![
                Tone::new(Saw, 600.0, 1.0).slide(60.0).decay(0.5),
                Tone::new(Noise, 4000.0, 0.8)
                    .slide(300.0)
                    .decay(0.4)
                    .volume(0.3),
            ],
        )
        .lowpass(2500.0)
        .peak(0.6),
        // --- Power-ups ---
        // The raised fourth, Lydian's colour note, marks power-up moments.
        Sound::new(
            "pickup_appear",
            vec![
                Tone::new(Sine, fs6, 0.15).decay(0.06),
                Tone::new(Sine, fs6 * 2.0, 0.15)
                    .at(0.05)
                    .decay(0.06)
                    .volume(0.5),
            ],
        )
        .echo(0.08, 0.35)
        .peak(0.4),
        // Pitched per power-up at runtime.
        Sound::new(
            "pickup_collect",
            vec![
                Tone::new(Square(0.5), c5, 0.08).decay(0.06).volume(0.6),
                Tone::new(Square(0.5), e5, 0.08)
                    .at(0.06)
                    .decay(0.06)
                    .volume(0.6),
                Tone::new(Square(0.5), g5, 0.08)
                    .at(0.12)
                    .decay(0.06)
                    .volume(0.6),
                Tone::new(Sine, c6, 0.4).at(0.18).decay(0.15),
                Tone::new(Sine, fs6, 0.4).at(0.18).decay(0.12).volume(0.25),
            ],
        )
        .lowpass(6000.0)
        .echo(0.09, 0.3)
        .peak(0.6),
        Sound::new(
            "pickup_expiring",
            vec![
                Tone::new(Sine, c6 * 2.0, 0.04).decay(0.015),
                Tone::new(Sine, fs6, 0.04).at(0.08).decay(0.015),
            ],
        )
        .peak(0.3),
        Sound::new(
            "abilities_lost",
            vec![
                Tone::new(Sine, fs5, 0.5)
                    .slide(c5 / 2.0)
                    .decay(0.25)
                    .vibrato(0.03, 9.0),
            ],
        )
        .echo(0.1, 0.3)
        .peak(0.45),
        // --- Light and colour ---
        // Pitched to the light's colour at runtime: root, third or fifth.
        Sound::new(
            "light_absorb",
            vec![
                Tone::new(Sine, c5, 0.25).decay(0.07),
                Tone::new(Sine, c6, 0.2).decay(0.05).volume(0.35),
            ],
        )
        .echo(0.07, 0.3)
        .peak(0.4),
        // A rising flourish, pitched to the restored primary at runtime.
        Sound::new(
            "colour_restored",
            vec![
                Tone::new(Sine, c5, 0.3).decay(0.15),
                Tone::new(Sine, e5, 0.3).at(0.06).decay(0.15),
                Tone::new(Sine, g5, 0.3).at(0.12).decay(0.15),
                Tone::new(Sine, c6, 0.6).at(0.18).decay(0.3),
                Tone::new(Sine, fs6, 0.5).at(0.24).decay(0.2).volume(0.3),
                Tone::new(Triangle, c5 / 2.0, 0.6)
                    .attack(0.05)
                    .decay(0.3)
                    .volume(0.4),
            ],
        )
        .echo(0.11, 0.35)
        .peak(0.6),
        // White: the full triad swells in with a shimmer as the exit opens.
        Sound::new(
            "spectrum_complete",
            vec![
                Tone::new(Sine, c5, 1.6)
                    .attack(0.15)
                    .decay(0.8)
                    .vibrato(0.003, 5.0),
                Tone::new(Sine, e5, 1.6)
                    .attack(0.2)
                    .decay(0.8)
                    .vibrato(0.003, 5.5),
                Tone::new(Sine, g5, 1.6)
                    .attack(0.25)
                    .decay(0.8)
                    .vibrato(0.003, 6.0),
                Tone::new(Sine, c6, 1.4)
                    .at(0.2)
                    .attack(0.2)
                    .decay(0.7)
                    .volume(0.6),
                Tone::new(Sine, fs6, 0.3).at(0.4).decay(0.1).volume(0.3),
                Tone::new(Sine, g6, 0.3).at(0.55).decay(0.1).volume(0.3),
                Tone::new(Triangle, c5 / 4.0, 1.6)
                    .attack(0.3)
                    .decay(0.8)
                    .volume(0.5),
            ],
        )
        .echo(0.15, 0.3)
        .peak(0.7),
        Sound::new(
            "level_complete",
            vec![
                Tone::new(Square(0.5), c5, 0.12).decay(0.1).volume(0.6),
                Tone::new(Square(0.5), e5, 0.12)
                    .at(0.1)
                    .decay(0.1)
                    .volume(0.6),
                Tone::new(Square(0.5), g5, 0.12)
                    .at(0.2)
                    .decay(0.1)
                    .volume(0.6),
                Tone::new(Square(0.5), c6, 0.9)
                    .at(0.3)
                    .decay(0.5)
                    .volume(0.6),
                Tone::new(Sine, e6, 0.9).at(0.3).decay(0.5).volume(0.4),
                Tone::new(Sine, g6, 0.9).at(0.3).decay(0.5).volume(0.35),
                Tone::new(Triangle, c5 / 2.0, 0.9)
                    .at(0.3)
                    .decay(0.5)
                    .volume(0.5),
            ],
        )
        .lowpass(5000.0)
        .echo(0.12, 0.3)
        .peak(0.7),
        // --- Prism and energy ---
        Sound::new(
            "prism_freeze",
            vec![
                Tone::new(Sine, c6 * 2.0, 0.12).slide(c6).decay(0.05),
                Tone::new(Sine, e6, 0.12).decay(0.04).volume(0.4),
            ],
        )
        .peak(0.3),
        Sound::new(
            "prism_unfreeze",
            vec![
                Tone::new(Sine, c6, 0.12).slide(c6 * 2.0).decay(0.05),
                Tone::new(Sine, g6, 0.12).decay(0.04).volume(0.4),
            ],
        )
        .peak(0.3),
        Sound::new(
            "energy_empty",
            vec![Tone::new(Square(0.25), 98.0, 0.25).slide(80.0).decay(0.12)],
        )
        .lowpass(1200.0)
        .peak(0.45),
        Sound::new(
            "energy_full",
            vec![
                Tone::new(Sine, g5, 0.3).decay(0.12),
                Tone::new(Sine, c6, 0.35).at(0.06).decay(0.15),
            ],
        )
        .echo(0.08, 0.3)
        .peak(0.35),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_follow_equal_temperament() {
        assert!((note(0) - 261.626).abs() < 0.01);
        assert!((note(12) - 2.0 * note(0)).abs() < 0.01);
        assert!((note(-12) - note(0) / 2.0).abs() < 0.01);
        assert!((lydian(7) - note(12)).abs() < 0.01);
        assert!((lydian(3) - note(6)).abs() < 0.01, "raised fourth");
        assert!((lydian(-1) - note(-1)).abs() < 0.01);
    }

    #[test]
    fn every_sound_renders_at_its_peak_with_unique_names() {
        let all = sounds();
        let mut names: Vec<_> = all.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len());
        for sound in &all {
            let samples = render(sound);
            assert!(!samples.is_empty(), "{} is empty", sound.name);
            let loudest = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(
                (loudest - sound.peak).abs() < 1e-3,
                "{} peaks at {loudest}",
                sound.name
            );
        }
    }

    #[test]
    fn loops_have_exact_length_and_repeat_seamlessly() {
        for sound in sounds().iter().filter(|s| s.looping.is_some()) {
            let once = render(sound);
            let n = SAMPLE_RATE as usize;
            assert_eq!(once.len(), n);
            // Rendered for two seconds, the second must repeat the first.
            let mut longer = sound.clone().looping(2.0);
            for tone in &mut longer.tones {
                tone.length = 2.0;
            }
            let twice = render(&longer);
            // Mean difference, as pulse edges may shift by one sample.
            #[allow(clippy::cast_precision_loss)]
            let mismatch = (0..n).map(|i| (twice[i] - twice[i + n]).abs()).sum::<f32>() / n as f32;
            assert!(mismatch < 0.002, "{} drifts by {mismatch}", sound.name);
        }
    }

    #[test]
    fn one_shots_fade_out_and_echo_adds_a_tail() {
        let dry = Sound::new("dry", vec![Tone::new(Wave::Square(0.5), 440.0, 0.1)]);
        let samples = render(&dry);
        assert!(samples.last().unwrap().abs() < 0.01);
        let wet = dry.echo(0.05, 0.5);
        assert!(render(&wet).len() > samples.len());
    }

    #[test]
    fn noise_and_lowpass_render() {
        let bright = Sound::new("n", vec![Tone::new(Wave::Noise, 8000.0, 0.1)]);
        let dull = bright.clone().lowpass(200.0);
        let roughness = |s: &[f32]| s.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>();
        assert!(roughness(&render(&dull)) < roughness(&render(&bright)) / 2.0);
    }

    #[test]
    fn wav_has_a_valid_header() {
        let bytes = wav_bytes(&[0.0, 1.0, -1.0]);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(bytes.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), i16::MAX);
    }
}
