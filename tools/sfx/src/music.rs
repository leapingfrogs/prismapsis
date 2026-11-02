//! The music: one piece in Lydian on C, written as four stems that loop
//! together.
//!
//! The grey base plays alone at first; red, green and blue each
//! add a layer as that colour returns to the level, and the title plays
//! them all. Red, green and blue are the root, third and fifth in the sound
//! effects, so here red is the rhythm, green the melody and blue the high
//! shimmer.
//!
//! The chords move every two bars: C major 7, D over C (Lydian's raised
//! fourth in the bass's company), E minor 7, then D over C again, which
//! leads back to C as the loop repeats.

use crate::{SAMPLE_RATE, Sound, Tone, Wave, halve_rate, note, render_wrapped};

/// Seconds per beat: 100 beats a minute.
pub const BEAT: f32 = 0.6;

/// Bars in the loop, four beats each.
pub const BARS: usize = 8;

/// Seconds in the loop.
#[allow(clippy::cast_precision_loss)]
pub const LOOP: f32 = (BARS * 4) as f32 * BEAT;

/// The music's sample rate. Half the effects' rate keeps the files small;
/// the music is soft and has little above it.
pub const MUSIC_RATE: u32 = SAMPLE_RATE / 2;

/// Each two-bar chord's notes in semitones from C4, lowest first.
const CHORDS: [[i32; 4]; 4] = [
    // C major 7.
    [0, 4, 7, 11],
    // D over C.
    [0, 2, 6, 9],
    // E minor 7.
    [4, 7, 11, 14],
    // D over C.
    [0, 2, 6, 9],
];

/// The chord sounding in `bar`.
const fn chord(bar: usize) -> [i32; 4] {
    CHORDS[bar / 2 % CHORDS.len()]
}

/// Seconds from the loop's start to `beat`.
#[allow(clippy::cast_precision_loss)]
fn at(beat: f32) -> f32 {
    beat * BEAT
}

#[allow(clippy::cast_precision_loss)]
const fn bar_start(bar: usize) -> f32 {
    (bar * 4) as f32
}

/// The melody: (beat, beats long, semitones from C4).
const MELODY: [(f32, f32, i32); 22] = [
    // Over C major 7.
    (0.0, 1.5, 16),
    (1.5, 0.5, 14),
    (2.0, 2.0, 11),
    (4.0, 1.0, 7),
    (5.0, 1.0, 9),
    (6.0, 2.0, 11),
    // Over D over C.
    (8.0, 1.0, 14),
    (9.0, 1.0, 16),
    (10.0, 2.0, 18),
    (12.0, 1.0, 16),
    (13.0, 1.0, 14),
    (14.0, 2.0, 9),
    // Over E minor 7.
    (16.0, 1.5, 7),
    (17.5, 0.5, 11),
    (18.0, 2.0, 16),
    (20.0, 1.0, 14),
    (21.0, 1.0, 11),
    (22.0, 2.0, 7),
    // Over D over C, rising back to the start.
    (24.0, 1.0, 9),
    (25.0, 1.0, 11),
    (26.0, 2.0, 14),
    (28.0, 4.0, 18),
];

/// The grey base: a soft pad holding each chord and a low root on the first
/// and third beats.
fn base() -> Sound {
    let mut tones = Vec::new();
    for bar in (0..BARS).step_by(2) {
        let start = at(bar_start(bar));
        for &pitch in &chord(bar) {
            tones.push(
                Tone::new(Wave::Triangle, note(pitch - 12), at(8.0) + 0.6)
                    .at(start)
                    .attack(0.9)
                    .decay(6.0)
                    .volume(0.3),
            );
            // A quieter, slightly wavering double an octave up, for warmth.
            tones.push(
                Tone::new(Wave::Triangle, note(pitch), at(8.0) + 0.6)
                    .at(start)
                    .attack(1.4)
                    .decay(5.0)
                    .volume(0.1)
                    .vibrato(0.003, 0.7),
            );
        }
    }
    for bar in 0..BARS {
        let root = chord(bar)[0];
        // The root on the beat and pushed ahead of the third, then the
        // fifth leading into the next bar.
        for (beat, pitch, volume) in [
            (0.0, root, 1.0),
            (1.5, root, 0.6),
            (2.0, root, 0.9),
            (3.5, root + 7, 0.6),
        ] {
            tones.push(
                Tone::new(Wave::Sine, note(pitch - 24), 0.9)
                    .at(at(bar_start(bar) + beat))
                    .attack(0.01)
                    .decay(0.4)
                    .volume(volume),
            );
        }
        // A soft brush on the off-beats.
        for beat in [0.5, 1.5, 2.5, 3.5] {
            tones.push(
                Tone::new(Wave::Noise, 6000.0, 0.12)
                    .at(at(bar_start(bar) + beat))
                    .attack(0.002)
                    .decay(0.04)
                    .volume(0.12),
            );
        }
    }
    Sound::new("music_base", tones).lowpass(1400.0).peak(0.4)
}

/// Red, the rhythm: a plucked arpeggio of the chord in eighth notes.
fn red() -> Sound {
    // Indexes into the chord, an octave up where 4 or more.
    const PATTERN: [usize; 8] = [0, 2, 4, 2, 1, 2, 4, 3];
    let mut tones = Vec::new();
    for bar in 0..BARS {
        let notes = chord(bar);
        for (eighth, &index) in PATTERN.iter().enumerate() {
            let pitch = notes[index % 4] + 12 * i32::from(index >= 4);
            #[allow(clippy::cast_precision_loss)]
            let beat = bar_start(bar) + eighth as f32 / 2.0;
            tones.push(
                Tone::new(Wave::Square(0.25), note(pitch - 12), 0.28)
                    .at(at(beat))
                    .decay(0.1)
                    .volume(0.5),
            );
        }
    }
    Sound::new("music_red", tones)
        .lowpass(2200.0)
        .echo(at(0.75), 0.25)
        .peak(0.22)
}

/// Green, the melody: a warm lead with a little vibrato.
fn green() -> Sound {
    let tones = MELODY
        .iter()
        .map(|&(beat, beats, pitch)| {
            Tone::new(Wave::Triangle, note(pitch), at(beats) * 0.92)
                .at(at(beat))
                .attack(0.03)
                .decay(1.6)
                .vibrato(0.005, 5.0)
        })
        .collect();
    Sound::new("music_green", tones)
        .lowpass(3000.0)
        .echo(at(0.75), 0.3)
        .peak(0.26)
}

/// Blue, the shimmer: high bells on the chord's upper notes, echoing.
fn blue() -> Sound {
    // Bells for each chord, in semitones from C4.
    const BELLS: [[i32; 3]; 4] = [[26, 31, 35], [30, 33, 26], [28, 35, 31], [33, 30, 38]];
    let mut tones = Vec::new();
    for bar in 0..BARS {
        let bells = BELLS[bar / 2 % BELLS.len()];
        for (&beat, &pitch) in [0.0, 1.5, 3.0].iter().zip(&bells) {
            let start = at(bar_start(bar) + beat);
            tones.push(
                Tone::new(Wave::Sine, note(pitch), 1.4)
                    .at(start)
                    .attack(0.002)
                    .decay(0.5),
            );
            tones.push(
                Tone::new(Wave::Sine, note(pitch + 12), 0.6)
                    .at(start)
                    .attack(0.002)
                    .decay(0.15)
                    .volume(0.3),
            );
        }
    }
    Sound::new("music_blue", tones)
        .echo(at(0.75), 0.4)
        .peak(0.18)
}

/// The four stems, base first. Names match the game's music stems.
#[must_use]
pub fn stems() -> Vec<Sound> {
    vec![base(), red(), green(), blue()]
}

/// A stem rendered as its loop at `MUSIC_RATE`.
#[must_use]
pub fn render_stem(stem: &Sound) -> Vec<f32> {
    halve_rate(&render_wrapped(stem, LOOP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_are_loops_of_the_same_length_at_their_peaks() {
        let expected = (f64::from(LOOP) * f64::from(MUSIC_RATE)).round();
        for stem in stems() {
            let samples = render_stem(&stem);
            #[allow(clippy::cast_precision_loss)]
            let len = samples.len() as f64;
            assert!((len - expected).abs() < 1.0, "{} is {len}", stem.name);
            let loudest = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            // Averaging pairs can only lower the peak.
            assert!(loudest <= stem.peak + 1e-6, "{}", stem.name);
            assert!(loudest > stem.peak * 0.8, "{}", stem.name);
        }
    }

    #[test]
    fn all_the_stems_together_never_clip() {
        let rendered: Vec<Vec<f32>> = stems().iter().map(render_stem).collect();
        let loudest = (0..rendered[0].len())
            .map(|i| rendered.iter().map(|s| s[i]).sum::<f32>().abs())
            .fold(0.0f32, f32::max);
        assert!(loudest < 0.9, "{loudest}");
    }

    #[test]
    fn stems_join_seamlessly() {
        for stem in stems() {
            let samples = render_stem(&stem);
            let end = samples[samples.len() - 1];
            let start = samples[0];
            // The jump across the join is no bigger than a typical step.
            let steps: Vec<f32> = samples.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            let biggest = steps.iter().fold(0.0f32, |m, &s| m.max(s));
            assert!((start - end).abs() <= biggest, "{}", stem.name);
        }
    }

    #[test]
    fn notes_ringing_past_the_end_come_back_at_the_start() {
        let late = Sound::new(
            "late",
            vec![Tone::new(Wave::Sine, 440.0, 1.0).at(LOOP - 0.5)],
        );
        let samples = render_wrapped(&late, LOOP);
        assert!(samples[..1000].iter().any(|s| s.abs() > 0.1));
    }

    #[test]
    fn chords_and_melody_stay_in_lydian_on_c() {
        let in_key = |pitch: i32| crate::LYDIAN.contains(&pitch.rem_euclid(12));
        assert!(CHORDS.iter().flatten().all(|&p| in_key(p)));
        assert!(MELODY.iter().all(|&(_, _, p)| in_key(p)));
        // The melody fills the loop.
        let (beat, beats, _) = MELODY[MELODY.len() - 1];
        #[allow(clippy::cast_precision_loss)]
        let total = (BARS * 4) as f32;
        assert!((beat + beats - total).abs() < f32::EPSILON);
    }

    #[test]
    fn halving_the_rate_averages_pairs() {
        assert_eq!(halve_rate(&[1.0, 0.0, -1.0, -0.5, 0.2]), [0.5, -0.75, 0.1]);
    }
}
