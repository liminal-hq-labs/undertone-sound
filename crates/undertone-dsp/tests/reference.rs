// Compares the in-house renderer with web-audio-api's renders of the same graphs, when they are there
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `FIZZ_UNDERTONE_REFERENCE=<folder>` names the folder `tools/undertone-reference` wrote (outside
//! the workspace, over the `web-audio-api` crate). Without it the test says so and passes, so the
//! gate never needs the reference. Each case of `fizz_audio_undertone::reference`, and each song,
//! is rendered in-house at 44.1 and 48 kHz and measured against the reference:
//!
//! - peak and RMS over the whole render, in dB;
//! - the spectral shape: the power in each octave band (Welch, 4096-point Hann frames), in dB
//!   relative to the render's own total, in every band no more than `BAND_FLOOR_DB` below the
//!   loudest;
//! - the loudness over time: the RMS of each 50 ms window louder than −40 dBFS.
//!
//! Most cases must agree within `EXACT` (0.5 dB in level, 1.5 dB a band, 1 dB a window). Two kinds
//! differ by design and get their own tolerances (`Tolerance::for_case`): the square, sawtooth and
//! triangle oscillators, which here are band-limited tables normalised as the specification and
//! Chromium normalise them (peak 1 with the Gibbs overshoot) where web-audio-api uses PolyBLEP
//! (plateau at the normalised level, aliasing above Nyquist folded back), so only their RMS is held;
//! and the compressor (Chromium's kernel here, web-audio-api's simpler detector there) and the
//! songs through the master bus, whose RMS and band shape are held but whose peak and short-term
//! loudness are only reported. Each song is also compared without the master bus (`-dry`), under
//! `EXACT`. The measurements are in `docs/research/UNDERTONE-HEADLESS.md`.

use std::path::PathBuf;

use fizz_audio_song::compile::{compile_song, CompileOptions};
use fizz_audio_undertone::dsp::fft::{Complex, Fft};
use fizz_audio_undertone::reference::{cases, song_requests, RATES, SONGS};
use fizz_audio_undertone::render_graph::RenderGraph;
use fizz_audio_undertone::studio::render_performance;

const BAND_FLOOR_DB: f64 = 40.0;

/// What a case must agree within; `None` is reported only.
#[derive(Debug, Clone, Copy)]
struct Tolerance {
    peak: Option<f64>,
    rms: f64,
    band: Option<f64>,
    window: Option<f64>,
}

const EXACT: Tolerance = Tolerance {
    peak: Some(0.5),
    rms: 0.5,
    band: Some(1.5),
    window: Some(1.0),
};

impl Tolerance {
    fn for_case(name: &str) -> Tolerance {
        let basic = ["osc-square", "osc-sawtooth", "osc-triangle", "osc-sweep"];
        if basic.iter().any(|prefix| name.starts_with(prefix)) {
            // Aliasing adds power to the reference at high pitches (0.7 dB at 5 kHz).
            return Tolerance {
                peak: None,
                rms: 0.75,
                band: None,
                window: Some(1.25),
            };
        }
        let limited =
            name.starts_with("compressor") || (name.starts_with("song") && !name.ends_with("-dry"));
        if limited {
            return Tolerance {
                peak: None,
                rms: 1.1,
                band: Some(1.7),
                window: None,
            };
        }
        EXACT
    }
}

fn read(path: &PathBuf) -> Vec<Vec<f32>> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let word = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    let (channels, frames) = (word(0), word(4));
    (0..channels)
        .map(|c| {
            (0..frames)
                .map(|n| {
                    let at = 8 + (c * frames + n) * 4;
                    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
                })
                .collect()
        })
        .collect()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

fn peak(sound: &[Vec<f32>]) -> f64 {
    sound
        .iter()
        .flatten()
        .fold(0f64, |m, s| m.max(f64::from(*s).abs()))
}

fn rms(sound: &[Vec<f32>]) -> f64 {
    let count: usize = sound.iter().map(Vec::len).sum();
    let sum: f64 = sound.iter().flatten().map(|s| f64::from(*s).powi(2)).sum();
    (sum / count.max(1) as f64).sqrt()
}

/// Power in octave bands from 22 Hz up, relative to the total, in dB.
fn bands(sound: &[Vec<f32>], rate: f64) -> Vec<f64> {
    let size = 4096;
    let fft = Fft::new(size);
    let window: Vec<f32> = (0..size)
        .map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / size as f64).cos()) as f32)
        .collect();
    let mut power = vec![0f64; size / 2];
    for channel in sound {
        let mut start = 0;
        while start + size <= channel.len() {
            let mut data: Vec<Complex> = (0..size)
                .map(|i| Complex::new(channel[start + i] * window[i], 0.0))
                .collect();
            fft.forward(&mut data);
            for (k, p) in power.iter_mut().enumerate() {
                *p += f64::from(data[k].re).powi(2) + f64::from(data[k].im).powi(2);
            }
            start += size / 2;
        }
    }
    let total: f64 = power.iter().sum::<f64>().max(1e-30);
    let mut out = Vec::new();
    let mut low = 22.0;
    while low < rate / 2.0 {
        let high = (low * 2.0f64).min(rate / 2.0);
        let band: f64 = power
            .iter()
            .enumerate()
            .filter(|(k, _)| {
                let f = *k as f64 * rate / size as f64;
                f >= low && f < high
            })
            .map(|(_, p)| p)
            .sum();
        out.push(10.0 * (band.max(1e-30) / total).log10());
        low = high;
    }
    out
}

/// The largest band difference among the bands within `BAND_FLOOR_DB` of the loudest.
fn band_difference(ours: &[f64], theirs: &[f64]) -> f64 {
    let loudest = theirs.iter().cloned().fold(f64::MIN, f64::max);
    ours.iter()
        .zip(theirs)
        .filter(|(_, t)| **t > loudest - BAND_FLOOR_DB)
        .map(|(o, t)| (o - t).abs())
        .fold(0.0, f64::max)
}

/// The largest difference of 50 ms RMS windows louder than −40 dBFS in the reference, and the
/// second it starts at.
fn window_difference(ours: &[Vec<f32>], theirs: &[Vec<f32>], rate: f64) -> (f64, f64) {
    let size = (0.05 * rate) as usize;
    let frames = theirs[0].len().min(ours[0].len());
    let mut worst = (0f64, 0f64);
    let mut start = 0;
    while start + size <= frames {
        let level = |sound: &[Vec<f32>]| {
            let sum: f64 = sound
                .iter()
                .map(|c| {
                    c[start..start + size]
                        .iter()
                        .map(|s| f64::from(*s).powi(2))
                        .sum::<f64>()
                })
                .sum();
            (sum / (size * sound.len()) as f64).sqrt()
        };
        let (o, t) = (level(ours), level(theirs));
        if db(t) > -40.0 && (db(o) - db(t)).abs() > worst.0 {
            worst = ((db(o) - db(t)).abs(), start as f64 / rate);
        }
        start += size;
    }
    worst
}

#[test]
fn the_in_house_renderer_agrees_with_web_audio_api() {
    let Some(folder) = std::env::var_os("FIZZ_UNDERTONE_REFERENCE") else {
        eprintln!("FIZZ_UNDERTONE_REFERENCE is not set, so the comparison with web-audio-api is skipped (see tools/undertone-reference)");
        return;
    };
    let folder = PathBuf::from(folder);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut failures = Vec::new();
    eprintln!("case | peak ours/ref dB | RMS ours/ref dB | worst band dB | worst 50 ms dB");
    for rate in RATES {
        let mut renders: Vec<(String, Vec<Vec<f32>>)> = Vec::new();
        for case in cases() {
            let frames = (case.seconds * rate).round() as usize;
            let mut graph = RenderGraph::new(case.channels, frames, rate);
            (case.build)(&mut graph);
            renders.push((case.name.clone(), graph.render().unwrap()));
        }
        for (name, path) in SONGS {
            let text = std::fs::read_to_string(root.join(path)).unwrap();
            let song = fizz_audio_song::file::parse_song_file(&text).song.unwrap();
            let performance = compile_song(&song, &CompileOptions::default()).unwrap();
            for (suffix, request) in song_requests(rate) {
                let sound = render_performance(&song, &performance, &request).unwrap();
                renders.push((format!("{name}{suffix}"), sound.channels));
            }
        }
        for (name, ours) in renders {
            let path = folder.join(format!("{name}@{rate}.f32"));
            if !path.exists() {
                failures.push(format!("{name}@{rate}: no reference at {}", path.display()));
                continue;
            }
            let theirs = read(&path);
            let (po, pt) = (db(peak(&ours)), db(peak(&theirs)));
            let (ro, rt) = (db(rms(&ours)), db(rms(&theirs)));
            let band = band_difference(&bands(&ours, rate), &bands(&theirs, rate));
            let (window, at) = window_difference(&ours, &theirs, rate);
            eprintln!(
                "{name}@{rate} | {po:.2} / {pt:.2} | {ro:.2} / {rt:.2} | {band:.2} | {window:.2} at {at:.2} s"
            );
            let tolerance = Tolerance::for_case(&name);
            if tolerance.peak.is_some_and(|t| (po - pt).abs() > t) {
                failures.push(format!("{name}@{rate}: peak {po:.2} against {pt:.2} dB"));
            }
            if (ro - rt).abs() > tolerance.rms {
                failures.push(format!("{name}@{rate}: RMS {ro:.2} against {rt:.2} dB"));
            }
            if tolerance.band.is_some_and(|t| band > t) {
                failures.push(format!("{name}@{rate}: a band differs by {band:.2} dB"));
            }
            if tolerance.window.is_some_and(|t| window > t) {
                failures.push(format!(
                    "{name}@{rate}: a 50 ms window differs by {window:.2} dB"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
