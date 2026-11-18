// Renders song files with the in-house renderer and prints their peak, loudness and render speed
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `cargo run --release -p fizz-audio-undertone --example render_songs -- <song.json>… [--rate 44100]
//! [--no-master] [--tracks] [--wav <folder>]`: each song's whole pass as the export renders it (a 2 s
//! tail, stereo, the master bus), with its length, peak, RMS and how long the render took a second of
//! sound, then the same as a stream in blocks of 128 and 512 frames with the worst block's time
//! against its real-time budget. A `pass:` line gives the figures over the pass alone (peak over the
//! pass and the tail): RMS, integrated loudness (ITU-R BS.1770, K-weighted and gated) and the side
//! signal's RMS against the mid's, a measure of width. `--no-master` leaves the master bus out, to
//! compare with a render that has none. `--tracks` adds each track's solo and in-mix levels; `--wav`
//! writes 16-bit files to look at (nothing here listens to them).

use std::path::PathBuf;
use std::time::Instant;

use fizz_audio_song::compile::{compile_song, CompileOptions};
use fizz_audio_undertone::render::{RenderRequest, Rendered};
use fizz_audio_undertone::render_graph::RenderGraph;
use fizz_audio_undertone::studio::{book, render_performance, render_tracks};

fn db(value: f64) -> String {
    if value <= 1e-9 {
        "-inf".into()
    } else {
        format!("{:.1}", 20.0 * value.log10())
    }
}

/// Integrated loudness in LUFS of stereo `left` and `right` (ITU-R BS.1770-4: the K-weighting
/// filters at any rate, 400 ms blocks every 100 ms, the absolute gate at -70 LUFS and the relative
/// gate 10 LU under the mean of the blocks that pass it).
fn lufs(left: &[f32], right: &[f32], rate: f64) -> f64 {
    let shelf = {
        let (f0, gain, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let vh = 10f64.powf(gain / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
            2.0 * (k * k - 1.0) / a0,
            (1.0 - k / q + k * k) / a0,
        ]
    };
    let highpass = {
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        [
            1.0,
            -2.0,
            1.0,
            2.0 * (k * k - 1.0) / a0,
            (1.0 - k / q + k * k) / a0,
        ]
    };
    let weigh = |input: &[f32]| -> Vec<f64> {
        let mut out: Vec<f64> = input.iter().map(|s| f64::from(*s)).collect();
        for [b0, b1, b2, a1, a2] in [shelf, highpass] {
            let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
            for sample in &mut out {
                let x = *sample;
                let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                (x2, x1, y2, y1) = (x1, x, y1, y);
                *sample = y;
            }
        }
        out
    };
    let (l, r) = (weigh(left), weigh(right));
    let block = (0.4 * rate).round() as usize;
    let hop = (0.1 * rate).round() as usize;
    let mut powers = Vec::new();
    let mut start = 0;
    while start + block <= l.len() {
        let sum: f64 = (start..start + block)
            .map(|n| l[n] * l[n] + r[n] * r[n])
            .sum();
        powers.push(sum / block as f64);
        start += hop;
    }
    let loudness = |power: f64| -0.691 + 10.0 * power.log10();
    let mean = |set: &[f64]| set.iter().sum::<f64>() / set.len().max(1) as f64;
    let loud: Vec<f64> = powers
        .into_iter()
        .filter(|p| loudness(*p) > -70.0)
        .collect();
    if loud.is_empty() {
        return f64::NEG_INFINITY;
    }
    let gate = loudness(mean(&loud)) - 10.0;
    let gated: Vec<f64> = loud.into_iter().filter(|p| loudness(*p) > gate).collect();
    loudness(mean(&gated))
}

/// The side signal's RMS against the mid's, in dB (more negative is narrower).
fn side_to_mid(left: &[f32], right: &[f32]) -> f64 {
    let (mut mid, mut side) = (0f64, 0f64);
    for (l, r) in left.iter().zip(right) {
        let (l, r) = (f64::from(*l), f64::from(*r));
        mid += ((l + r) / 2.0).powi(2);
        side += ((l - r) / 2.0).powi(2);
    }
    10.0 * (side / mid.max(1e-30)).log10()
}

fn wav(path: &PathBuf, sound: &Rendered) {
    let frames = sound.frames();
    let channels = sound.channels.len() as u16;
    let rate = sound.sample_rate as u32;
    let data = frames as u32 * u32::from(channels) * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in sound.interleaved() {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

fn main() {
    let mut paths = Vec::new();
    let mut rate = 48_000.0;
    let mut tracks = false;
    let mut master = true;
    let mut wav_folder: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rate" => rate = args.next().unwrap().parse().unwrap(),
            "--tracks" => tracks = true,
            "--no-master" => master = false,
            "--wav" => wav_folder = Some(args.next().unwrap().into()),
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    for path in paths {
        let text = std::fs::read_to_string(&path).unwrap();
        let song = fizz_audio_song::file::parse_song_file(&text)
            .song
            .expect("the song validates");
        let performance = compile_song(&song, &CompileOptions::default()).unwrap();
        let request = RenderRequest {
            sample_rate: rate,
            master,
            ..RenderRequest::EXPORT
        };
        let started = Instant::now();
        let sound = render_performance(&song, &performance, &request).unwrap();
        let took = started.elapsed().as_secs_f64();
        let seconds = sound.frames() as f64 / rate;
        println!(
            "{}: {} events, {:.1} s at {} Hz, peak {:.3} ({} dBFS), RMS {} dBFS, rendered in {:.2} s ({:.3} s a second of sound)",
            path.display(),
            performance.events.len(),
            seconds,
            rate,
            sound.peak(),
            db(sound.peak()),
            db(sound.rms()),
            took,
            took / seconds
        );
        let pass = sound
            .frames()
            .saturating_sub((request.tail * rate).round() as usize);
        let (left, right) = (&sound.channels[0][..pass], &sound.channels[1][..pass]);
        let pass_rms = Rendered {
            sample_rate: rate,
            channels: vec![left.to_vec(), right.to_vec()],
        }
        .rms();
        println!(
            "  pass: {:.3} s, peak {} dBFS, RMS {} dBFS, {:.1} LUFS, side-to-mid {:.1} dB{}",
            pass as f64 / rate,
            db(sound.peak()),
            db(pass_rms),
            lufs(left, right, rate),
            side_to_mid(left, right),
            if master { "" } else { ", no master bus" }
        );
        // Streaming: prepare an engine, then time each block as an audio callback would see it.
        for block in [128usize, 512] {
            let prepared = Instant::now();
            let graph = book(&song, &performance, &request, |init| {
                RenderGraph::new(init.channels, init.length, init.sample_rate)
            })
            .unwrap();
            let mut engine = graph.engine().unwrap();
            let prepare = prepared.elapsed().as_secs_f64();
            let mut left = vec![0f32; block];
            let mut right = vec![0f32; block];
            let mut worst = 0f64;
            let mut total = 0f64;
            let blocks = engine.length().div_ceil(block);
            for _ in 0..blocks {
                let started = Instant::now();
                engine.process(&mut left, &mut right);
                let took = started.elapsed().as_secs_f64();
                total += took;
                worst = worst.max(took);
            }
            let budget = block as f64 / rate;
            println!(
                "  blocks of {block}: prepared in {:.2} s ({} nodes at most at once), {:.3} s a second of sound, worst block {:.3} ms of its {:.3} ms ({:.0}%)",
                prepare,
                engine.peak_active(),
                total / seconds,
                worst * 1e3,
                budget * 1e3,
                100.0 * worst / budget
            );
        }
        if let Some(folder) = &wav_folder {
            let name = path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .replace(".song", "");
            wav(&folder.join(format!("{name}-{rate}.wav")), &sound);
        }
        if tracks {
            let ids: Vec<&str> = song.tracks.iter().map(|t| t.id.as_str()).collect();
            let started = Instant::now();
            let renders = render_tracks(&song, &performance, &request, &ids).unwrap();
            println!(
                "  mix and {} tracks measured in {:.2} s",
                ids.len(),
                started.elapsed().as_secs_f64()
            );
            for track in &renders.tracks {
                println!(
                    "  {}: solo peak {} RMS {}, in mix peak {} RMS {}",
                    track.track,
                    db(track.solo.peak()),
                    db(track.solo.rms()),
                    db(track.in_mix.peak()),
                    db(track.in_mix.rms())
                );
            }
        }
    }
}
