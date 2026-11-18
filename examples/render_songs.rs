// Renders song files with the in-house renderer and prints their peak, loudness and render speed
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `cargo run --release -p fizz-audio-undertone --example render_songs -- <song.json>… [--rate 44100]
//! [--tracks] [--wav <folder>]`: each song's whole pass as the export renders it (a 2 s tail,
//! stereo, the master bus), with its length, peak, RMS and how long the render took a second of
//! sound, then the same as a stream in blocks of 128 and 512 frames with the worst block's time
//! against its real-time budget. `--tracks` adds each track's solo and in-mix levels; `--wav` writes 16-bit files to look
//! at (nothing here listens to them).

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
    let mut wav_folder: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rate" => rate = args.next().unwrap().parse().unwrap(),
            "--tracks" => tracks = true,
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
