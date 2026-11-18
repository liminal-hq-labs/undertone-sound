// A song's pass booked on a Graph exactly as Undertone's offline render books it
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! The entry point: `render_graph` (and `book`, for any `Graph`) does what the studio's export does
//! with `toScore(song, performance).render({ tail, channels })`, up to the moment the context would
//! render: it sizes the context, seeds it, makes the master bus, collects the pass's voices, sorts
//! them by start time and books each with `play_voice`. Rendering the booked graph to samples is the
//! next step (`docs/research/UNDERTONE-HEADLESS.md`, task 4.5); a renderer implements `Graph` and
//! is made by the `make` function `book` takes.

use fizz_audio_song::compile::Performance;
use fizz_audio_song::model::Song;

use crate::error::Error;
use crate::graph::Graph;
use crate::log::LogGraph;
use crate::master::{master_bus, LIMITER};
use crate::math;
use crate::score::{collect_onsets, to_score, Score};
use crate::voice::{play_voice, Booking, Output};

/// Undertone's offline sample rate.
pub const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;

/// How to render (`RenderOptions`, as far as the studio uses them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRequest {
    /// Seconds after the pass for releases and reverb to ring out.
    pub tail: f64,
    /// Output channels: 1 or 2.
    pub channels: u32,
    pub sample_rate: f64,
    /// The seed of the noise and impulse responses.
    pub seed: f64,
    /// Play through the master bus and its limiter.
    pub master: bool,
}

impl RenderRequest {
    /// What the studio's export asks for: a 2 s tail, stereo, 48 kHz, seed 0, the master bus on.
    pub const EXPORT: RenderRequest = RenderRequest {
        tail: 2.0,
        channels: 2,
        sample_rate: DEFAULT_SAMPLE_RATE,
        seed: 0.0,
        master: true,
    };
}

impl Default for RenderRequest {
    fn default() -> Self {
        RenderRequest::EXPORT
    }
}

/// The context a render needs, as `new OfflineAudioContext(channels, length, sampleRate)` takes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextInit {
    pub channels: u32,
    /// Frames: the pass and the tail.
    pub length: usize,
    pub sample_rate: f64,
}

fn require_positive(name: &str, value: f64) -> Result<(), Error> {
    if !(value > 0.0 && value.is_finite()) {
        return Err(Error::new(format!(
            "{name} must be positive, got {}",
            fizz_audio_song::js::number_to_string(value)
        )));
    }
    Ok(())
}

/// Books one pass of `score` on the graph `make` returns, as `renderToBuffer(pattern, { cps })`
/// with one cycle does. More than two channels (surround) is not ported.
pub fn book_score<G: Graph>(
    score: &Score,
    request: &RenderRequest,
    make: impl FnOnce(ContextInit) -> G,
) -> Result<G, Error> {
    if !matches!(request.channels, 1 | 2) {
        return Err(Error::new(format!(
            "channels must be 1 or 2 in the Rust port, got {}",
            request.channels
        )));
    }
    require_positive("sampleRate", request.sample_rate)?;
    if !(request.tail >= 0.0 && request.tail.is_finite()) {
        return Err(Error::new(format!(
            "tail must be zero or more seconds, got {}",
            fizz_audio_song::js::number_to_string(request.tail)
        )));
    }
    let cycle_seconds = 1.0 / score.cps;
    let pass_seconds = cycle_seconds;
    let length = math::max(
        1.0,
        math::round((pass_seconds + request.tail) * request.sample_rate),
    ) as usize;
    let mut graph = make(ContextInit {
        channels: request.channels,
        length,
        sample_rate: request.sample_rate,
    });
    let mut booking = Booking::new(request.seed);
    let destination = graph.destination();
    let output = if request.master {
        let bus = master_bus(
            &mut graph,
            destination,
            request.channels,
            1.0,
            Some(LIMITER),
            booking.now,
        );
        // The bus's input is a gain node, two channels by default whatever the destination has.
        Output {
            node: bus.input,
            channels: 2,
        }
    } else {
        Output {
            node: destination,
            channels: request.channels,
        }
    };
    let mut voices = collect_onsets(score);
    // In time order, so a cut group chokes the voice that really played before.
    voices.sort_by(|a, b| {
        (a.start_time + a.params.nudge_time)
            .partial_cmp(&(b.start_time + b.params.nudge_time))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for voice in &voices {
        play_voice(
            &mut graph,
            &mut booking,
            &voice.params,
            voice.start_time,
            output,
        )?;
    }
    Ok(graph)
}

/// Books one pass of a performance on the graph `make` returns.
pub fn book<G: Graph>(
    song: &Song,
    performance: &Performance,
    request: &RenderRequest,
    make: impl FnOnce(ContextInit) -> G,
) -> Result<G, Error> {
    book_score(&to_score(song, performance)?, request, make)
}

/// One pass of a performance as the graph Undertone would render, recorded.
pub fn render_graph(
    song: &Song,
    performance: &Performance,
    request: &RenderRequest,
) -> Result<LogGraph, Error> {
    book(song, performance, request, |init| {
        LogGraph::new(init.channels, init.length, init.sample_rate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fizz_audio_song::compile::{compile_song, CompileOptions};

    fn voices() -> (Song, Performance) {
        let text = include_str!("../../../fixtures/undertone/inputs/undertone-voices.song.json");
        let song: Song = serde_json::from_str(text).unwrap();
        let performance = compile_song(&song, &CompileOptions::default()).unwrap();
        (song, performance)
    }

    #[test]
    fn a_pass_and_its_tail_size_the_context() {
        let (song, performance) = voices();
        let graph = render_graph(&song, &performance, &RenderRequest::EXPORT).unwrap();
        // One bar at 120 bpm is 2 s, and the tail another 2 s, at 48 kHz.
        assert_eq!(graph.length(), 192_000);
        assert_eq!(graph.channels(), 2);
        assert_eq!(graph.node_count(), 112);
    }

    #[test]
    fn without_the_master_bus_voices_play_straight_out() {
        let (song, performance) = voices();
        let request = RenderRequest {
            master: false,
            ..RenderRequest::EXPORT
        };
        let graph = render_graph(&song, &performance, &request).unwrap();
        assert_eq!(graph.node_count(), 112 - 3);
    }

    #[test]
    fn surround_and_bad_rates_are_refused() {
        let (song, performance) = voices();
        let surround = RenderRequest {
            channels: 6,
            ..RenderRequest::EXPORT
        };
        assert!(render_graph(&song, &performance, &surround).is_err());
        let silent = RenderRequest {
            sample_rate: 0.0,
            ..RenderRequest::EXPORT
        };
        assert_eq!(
            render_graph(&song, &performance, &silent)
                .unwrap_err()
                .message,
            "sampleRate must be positive, got 0"
        );
    }
}
