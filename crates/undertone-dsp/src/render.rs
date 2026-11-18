// A song's pass booked on a Graph exactly as Undertone's offline render books it
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! The core entry points, on a `Score` (no studio types): `book_score` books one pass on any
//! `Graph` as Undertone's `renderToBuffer` does (it sizes the context, seeds it, makes the master
//! bus, collects the pass's voices, sorts them by start time and books each with `play_voice`);
//! `render_score` books it on the in-house `RenderGraph` and renders it as the studio's export
//! does (one pass, the tail after it, stereo at 48 kHz, through the master bus, no wrap unless
//! asked); `render_score_parts` measures groups of voices alone and inside the mix from one
//! booking. `crate::studio` turns a song and its performance into the score.

use std::sync::Arc;

use crate::dsp::compressor::{Compressor, Settings};
use crate::dsp::QUANTUM;
use crate::error::Error;
use crate::graph::Graph;
use crate::master::{makeup_compensation, master_bus, Limiter, LIMITER};
use crate::math;
use crate::render_graph::{Engine, Limits, RenderGraph};
use crate::score::{collect_onsets, Score};
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
    /// Fold the tail onto the start of a pass-length render, so a looped file has no gap at the
    /// seam (`renderToBuffer`'s `wrap`). The export leaves it off.
    pub wrap: bool,
}

impl RenderRequest {
    /// What the studio's export asks for: a 2 s tail, stereo, 48 kHz, seed 0, the master bus on.
    pub const EXPORT: RenderRequest = RenderRequest {
        tail: 2.0,
        channels: 2,
        sample_rate: DEFAULT_SAMPLE_RATE,
        seed: 0.0,
        master: true,
        wrap: false,
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
        graph.voice(voice.source);
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

/// Frames in one pass at the request's rate, as `renderToBuffer` rounds them.
fn pass_frames(score: &Score, request: &RenderRequest) -> usize {
    math::max(1.0, math::round(request.sample_rate / score.cps)) as usize
}

/// Sound rendered by the in-house renderer: planar `f32`, one `Vec` a channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub sample_rate: f64,
    pub channels: Vec<Vec<f32>>,
}

impl Rendered {
    /// Frames in each channel.
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    /// The channels interleaved, frame by frame.
    pub fn interleaved(&self) -> Vec<f32> {
        let frames = self.frames();
        let mut out = Vec::with_capacity(frames * self.channels.len());
        for n in 0..frames {
            for channel in &self.channels {
                out.push(channel[n]);
            }
        }
        out
    }

    /// The largest absolute sample, linear.
    pub fn peak(&self) -> f64 {
        self.channels
            .iter()
            .flatten()
            .fold(0f64, |m, s| m.max(f64::from(*s).abs()))
    }

    /// The RMS over every channel and frame, linear.
    pub fn rms(&self) -> f64 {
        let count = self.channels.iter().map(Vec::len).sum::<usize>();
        if count == 0 {
            return 0.0;
        }
        let sum: f64 = self
            .channels
            .iter()
            .flatten()
            .map(|s| f64::from(*s).powi(2))
            .sum();
        (sum / count as f64).sqrt()
    }
}

/// `foldTail`: the frames after the pass added onto its start, pass-length.
fn fold_tail(channels: Vec<Vec<f32>>, pass: usize) -> Vec<Vec<f32>> {
    channels
        .into_iter()
        .map(|source| {
            let mut target = source[..pass.min(source.len())].to_vec();
            target.resize(pass, 0.0);
            for (i, sample) in source.iter().enumerate().skip(pass) {
                target[(i - pass) % pass] += sample;
            }
            target
        })
        .collect()
}

/// Books one pass of a score on the in-house renderer: the graph and the pass's frames.
pub fn book_score_render(
    score: &Score,
    request: &RenderRequest,
) -> Result<(RenderGraph, usize), Error> {
    let pass = pass_frames(score, request);
    let graph = book_score(score, request, |init| {
        RenderGraph::new(init.channels, init.length, init.sample_rate)
    })?;
    Ok((graph, pass))
}

/// One pass of a score rendered to samples as the studio's export renders it: the pass and
/// `request.tail` after it, `request.channels` at `request.sample_rate`, through the master bus
/// when `request.master`, folded onto the pass when `request.wrap`.
pub fn render_score(score: &Score, request: &RenderRequest) -> Result<Rendered, Error> {
    let (graph, pass) = book_score_render(score, request)?;
    let mut channels = graph.engine()?.render_all();
    if request.wrap && request.tail > 0.0 {
        channels = fold_tail(channels, pass);
    }
    Ok(Rendered {
        sample_rate: request.sample_rate,
        channels,
    })
}

/// The master bus applied to a mix rendered without it, frame for frame as the bus in the graph
/// computes it (a gain of 1, the limiter, the make-up compensation, then the output's channels).
/// Returns the output and, per frame, the gain the limiter applied to its pre-delayed input.
fn master_offline(
    mix: &[Vec<f32>],
    sample_rate: f64,
    limiter: Limiter,
    out_channels: usize,
) -> (Vec<Vec<f32>>, Vec<f32>, usize) {
    let frames = mix.first().map_or(0, Vec::len);
    let padded = frames.div_ceil(QUANTUM) * QUANTUM;
    let mut left = vec![0f32; padded];
    let mut right = vec![0f32; padded];
    left[..frames].copy_from_slice(&mix[0][..frames]);
    right[..frames].copy_from_slice(&mix[mix.len().min(2) - 1][..frames]);
    let mut gains = vec![0f32; padded];
    let mut compressor = Compressor::new(sample_rate);
    let settings = Settings {
        threshold: limiter.threshold as f32,
        knee: limiter.knee as f32,
        ratio: limiter.ratio as f32,
        attack: limiter.attack as f32,
        release: limiter.release as f32,
    };
    for start in (0..padded).step_by(QUANTUM) {
        let range = start..start + QUANTUM;
        compressor.process(
            settings,
            [&mut left[range.clone()], &mut right[range.clone()]],
            Some(&mut gains[range]),
        );
    }
    let compensation = if limiter.compensate {
        makeup_compensation(limiter.threshold, limiter.ratio) as f32
    } else {
        1.0
    };
    if limiter.compensate {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l *= compensation;
            *r *= compensation;
        }
    }
    for gain in &mut gains {
        *gain *= compensation;
    }
    left.truncate(frames);
    right.truncate(frames);
    gains.truncate(frames);
    let out = if out_channels == 1 {
        vec![left
            .iter()
            .zip(&right)
            .map(|(l, r)| 0.5 * (l + r))
            .collect()]
    } else {
        vec![left, right]
    };
    (out, gains, compressor.latency())
}

/// One group of voices measured from a render: alone, and its share of the mix.
#[derive(Debug, Clone, PartialEq)]
pub struct PartRender {
    /// Its voices alone through the master bus.
    pub solo: Rendered,
    /// Its own sound inside the mix: its voices with the limiter's gain the whole mix set.
    pub in_mix: Rendered,
}

/// The mix and some groups of its voices, from one booking.
#[derive(Debug, Clone, PartialEq)]
pub struct PartRenders {
    pub mix: Rendered,
    pub parts: Vec<PartRender>,
}

/// Renders the mix and, for each of `parts` groups of voices, the group alone and its share of
/// the mix, from one booking of the whole pass. `part_of` says which group the score event at a
/// voice's `source` belongs to, if any.
///
/// Everything before the master bus is linear in the voices (FM, filters and envelopes are inside
/// a voice; the reverb and the echo are linear, and booked once), so with the booking kept, the
/// mix before the limiter is the sum of each group's voices rendered with the others left out.
/// The limiter is the only part that mixes them: its gain at each frame follows the whole mix. So
/// `solo` is the group's voices through a limiter of their own, and `in_mix` is the same voices
/// times the gain the mix's limiter applied, which is the group's part of what the mix plays.
/// That costs one render of the mix and one render per group of only that group's voices (the
/// shared reverb and echo run in each), instead of rendering the mix again without each group and
/// subtracting. Room sizes, noise seeds and cut groups are the mix's in every render, so a group
/// alone keeps the room its orbit has in the mix.
pub fn render_score_parts(
    score: &Score,
    request: &RenderRequest,
    part_of: &dyn Fn(usize) -> Option<usize>,
    parts: usize,
) -> Result<PartRenders, Error> {
    let dry = RenderRequest {
        master: false,
        channels: 2,
        ..*request
    };
    let (graph, pass) = book_score_render(score, &dry)?;
    let graph = Arc::new(graph);
    let out_channels = request.channels as usize;
    let finish = |channels: Vec<Vec<f32>>, mixed: bool| {
        let channels = if !mixed && out_channels == 1 && channels.len() == 2 {
            vec![channels[0]
                .iter()
                .zip(&channels[1])
                .map(|(l, r)| 0.5 * (l + r))
                .collect()]
        } else {
            channels
        };
        Rendered {
            sample_rate: request.sample_rate,
            channels: if request.wrap && request.tail > 0.0 {
                fold_tail(channels, pass)
            } else {
                channels
            },
        }
    };
    let whole = Engine::new(graph.clone())?.render_all();
    let limiter = request.master.then_some(LIMITER);
    let (mix, gains, latency) = match limiter {
        Some(limiter) => master_offline(&whole, request.sample_rate, limiter, out_channels),
        None => (whole, Vec::new(), 0),
    };
    let mut out = Vec::with_capacity(parts);
    for part in 0..parts {
        let alone = Engine::with(
            graph.clone(),
            |tag| tag.and_then(part_of) == Some(part),
            Limits::DEFAULT,
        )?
        .render_all();
        let (solo, in_mix) = match limiter {
            Some(limiter) => {
                let (solo, _, _) = master_offline(&alone, request.sample_rate, limiter, 2);
                let share = alone
                    .iter()
                    .map(|channel| {
                        (0..channel.len())
                            .map(|n| {
                                if n < latency {
                                    0.0
                                } else {
                                    channel[n - latency] * gains[n]
                                }
                            })
                            .collect()
                    })
                    .collect();
                (solo, share)
            }
            None => (alone.clone(), alone),
        };
        out.push(PartRender {
            solo: finish(solo, false),
            in_mix: finish(in_mix, false),
        });
    }
    Ok(PartRenders {
        mix: finish(mix, limiter.is_some()),
        parts: out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Patch;
    use crate::score::{score, ScoreEvent, ScorePitch};

    fn note(tick: f64, source: usize) -> ScoreEvent {
        ScoreEvent {
            tick,
            pitch: ScorePitch::Midi(69.0),
            gate: 6.0,
            level: Some(0.8),
            pan: Some(0.0),
            voice: Patch::new(),
            source,
        }
    }

    #[test]
    fn a_score_renders_its_pass_and_tail_and_its_parts_add_up() {
        let score = score(24.0, 48.0, &[note(0.0, 0), note(12.0, 1), note(24.0, 0)]).unwrap();
        let rendered = render_score(&score, &RenderRequest::EXPORT).unwrap();
        // Two seconds of pass and two of tail.
        assert_eq!(rendered.frames(), 192_000);
        assert!(rendered.peak() > 0.1);
        let parts = render_score_parts(&score, &RenderRequest::EXPORT, &|s| Some(s), 2).unwrap();
        assert_eq!(parts.mix, rendered, "the offline bus is the graph's bus");
        for n in (0..rendered.frames()).step_by(101) {
            let sum = parts.parts[0].in_mix.channels[0][n] + parts.parts[1].in_mix.channels[0][n];
            assert!((sum - rendered.channels[0][n]).abs() < 1e-5);
        }
        let wrapped = render_score(
            &score,
            &RenderRequest {
                wrap: true,
                ..RenderRequest::EXPORT
            },
        )
        .unwrap();
        assert_eq!(wrapped.frames(), 96_000);
    }

    #[test]
    fn surround_and_bad_rates_are_refused() {
        let score = score(24.0, 48.0, &[note(0.0, 0)]).unwrap();
        let surround = RenderRequest {
            channels: 6,
            ..RenderRequest::EXPORT
        };
        assert!(render_score(&score, &surround).is_err());
        let silent = RenderRequest {
            sample_rate: 0.0,
            ..RenderRequest::EXPORT
        };
        assert_eq!(
            render_score(&score, &silent).unwrap_err().message,
            "sampleRate must be positive, got 0"
        );
    }
}
