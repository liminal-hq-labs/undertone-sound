// The studio glue: a song and its compiled performance as Undertone's score, rendered and measured
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! The only part of the renderer that knows the studio's types (`Song`, `Performance`), with
//! `presets` (the studio's presets and a song's overrides). The studio's `toScoreEvents`
//! (`apps/studio/src/engines/undertone/engine.ts`) turns a performance into score events at 12
//! ticks a step; `to_score` makes the pass's score; the rest are the studio's entry points over the
//! core in `render`: `book` and `render_graph` (the recording the fixtures compare),
//! `render_performance` (the export) and `render_tracks` (each track alone and in the mix).

use fizz_audio_song::compile::Performance;
use fizz_audio_song::model::Song;

use crate::error::Error;
use crate::graph::Graph;
use crate::log::LogGraph;
use crate::math;
use crate::params::Patch;
use crate::presets::{drum_layers, instrument_voice};
use crate::render::{
    book_score, render_score, render_score_parts, ContextInit, RenderRequest, Rendered,
};
use crate::score::{score, Score, ScoreEvent, ScorePitch, TICKS_PER_STEP};

/// `toScoreEvents`: every event of the performance that starts inside it, a drum hit as one event
/// per layer of its piece, a rest left out.
pub fn to_score_events(song: &Song, performance: &Performance) -> Result<Vec<ScoreEvent>, Error> {
    let length_ticks = performance.length_steps * TICKS_PER_STEP;
    let mut events = Vec::new();
    let mut voices: Vec<(String, Patch)> = Vec::new();
    for (source, event) in performance.events.iter().enumerate() {
        let tick = math::round(event.step * TICKS_PER_STEP);
        if tick >= length_ticks {
            continue;
        }
        let gate = math::max(1.0, math::round(event.len * TICKS_PER_STEP));
        if let Some(piece) = event.drum {
            for layer in drum_layers(song, &event.instrument_id, piece)? {
                events.push(ScoreEvent {
                    tick,
                    pitch: ScorePitch::Note(layer.pitch),
                    gate,
                    level: Some(event.velocity),
                    pan: Some(event.pan),
                    voice: layer.voice,
                    source,
                });
            }
            continue;
        }
        let Some(midi) = event.midi else { continue };
        let voice = match voices.iter().find(|(id, _)| *id == event.instrument_id) {
            Some((_, voice)) => voice.clone(),
            None => {
                let voice = instrument_voice(song, &event.instrument_id);
                voices.push((event.instrument_id.clone(), voice.clone()));
                voice
            }
        };
        events.push(ScoreEvent {
            tick,
            pitch: ScorePitch::Midi(f64::from(midi)),
            gate,
            level: Some(event.velocity),
            pan: Some(event.pan),
            voice,
            source,
        });
    }
    Ok(events)
}

/// `toScore`: the score for one pass of a performance, at its own tick rate.
pub fn to_score(song: &Song, performance: &Performance) -> Result<Score, Error> {
    score(
        performance.steps_per_second * TICKS_PER_STEP,
        math::max(1.0, performance.length_steps * TICKS_PER_STEP),
        &to_score_events(song, performance)?,
    )
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

/// One pass of a performance rendered as the studio's export renders it (`render_score`).
pub fn render_performance(
    song: &Song,
    performance: &Performance,
    request: &RenderRequest,
) -> Result<Rendered, Error> {
    render_score(&to_score(song, performance)?, request)
}

/// One track measured from a render: alone, and its share of the mix.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRender {
    pub track: String,
    /// Its voices alone through the master bus, booked as in the mix.
    pub solo: Rendered,
    /// Its own sound inside the mix.
    pub in_mix: Rendered,
}

/// The mix and some tracks, from one booking.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRenders {
    pub mix: Rendered,
    pub tracks: Vec<TrackRender>,
}

/// The mix and each of `tracks` alone and in the mix, from one booking (`render_score_parts`,
/// a track's voices being the score events of its performance events).
pub fn render_tracks(
    song: &Song,
    performance: &Performance,
    request: &RenderRequest,
    tracks: &[&str],
) -> Result<TrackRenders, Error> {
    let part_of = |source: usize| {
        let event = performance.events.get(source)?;
        tracks.iter().position(|track| *track == event.track_id)
    };
    let renders = render_score_parts(
        &to_score(song, performance)?,
        request,
        &part_of,
        tracks.len(),
    )?;
    Ok(TrackRenders {
        mix: renders.mix,
        tracks: tracks
            .iter()
            .zip(renders.parts)
            .map(|(track, part)| TrackRender {
                track: (*track).to_string(),
                solo: part.solo,
                in_mix: part.in_mix,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::RenderGraph;
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
    fn the_export_is_the_same_bits_every_time() {
        let (song, performance) = voices();
        let rendered = render_performance(&song, &performance, &RenderRequest::EXPORT).unwrap();
        assert_eq!(rendered.channels.len(), 2);
        assert_eq!(rendered.frames(), 192_000);
        assert!(rendered.peak() > 0.05 && rendered.peak() < 1.5);
        assert_eq!(rendered.interleaved().len(), 384_000);
        let again = render_performance(&song, &performance, &RenderRequest::EXPORT).unwrap();
        assert_eq!(rendered, again);
    }

    #[test]
    fn any_block_size_gives_the_same_bits() {
        let (song, performance) = voices();
        let graph = book(&song, &performance, &RenderRequest::EXPORT, |init| {
            RenderGraph::new(init.channels, init.length, init.sample_rate)
        })
        .unwrap();
        let whole = graph.clone().engine().unwrap().render_all();
        let length = graph.length();
        for block in [1usize, 64, 128, 480, 4096] {
            let mut engine = graph.clone().engine().unwrap();
            let mut left = vec![0f32; length];
            let mut right = vec![0f32; length];
            for start in (0..length).step_by(block) {
                let end = (start + block).min(length);
                engine.process(&mut left[start..end], &mut right[start..end]);
            }
            assert!(left == whole[0] && right == whole[1], "blocks of {block}");
        }
        let mut engine = graph.engine().unwrap();
        let mut interleaved = vec![0f32; 2 * 1000];
        engine.process_interleaved(&mut interleaved);
        assert_eq!(interleaved[2 * 999 + 1], whole[1][999]);
    }

    #[test]
    fn the_tracks_add_up_to_the_mix_and_the_bus_matches_the_graphs() {
        let (song, performance) = voices();
        let ids: Vec<&str> = song.tracks.iter().map(|t| t.id.as_str()).collect();
        let renders = render_tracks(&song, &performance, &RenderRequest::EXPORT, &ids).unwrap();
        let export = render_performance(&song, &performance, &RenderRequest::EXPORT).unwrap();
        assert_eq!(renders.mix, export, "the offline bus is the graph's bus");
        for c in 0..2 {
            for n in (0..export.frames()).step_by(97) {
                let sum: f32 = renders.tracks.iter().map(|t| t.in_mix.channels[c][n]).sum();
                assert!((sum - export.channels[c][n]).abs() < 1e-4, "{c} {n}");
            }
        }
        assert!(renders.tracks.iter().all(|t| t.solo.peak() > 0.0));
    }
}
