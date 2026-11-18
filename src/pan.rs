// Where a note sits in the stereo field over its length: its pan, and the path a motion draws
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Port of `apps/studio/src/song/panMotion.ts`, with the same floating-point operations in the
//! same order. Pans in a path are rounded to three decimals with `Math.round`'s rule, which
//! absorbs the last-bit differences between JavaScript's and Rust's `sin` and `cos`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::js;
use crate::model::{
    JumpEvery, Note, PanCurve, PanMotion, Take, WobbleShape, DEFAULT_JUMP_POSITIONS,
    DEFAULT_WOBBLE_DEPTH,
};

/// A point of a pan path: steps from the note's onset, and the pan from there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct PanPoint {
    pub at: f64,
    pub pan: f64,
}

/// What a note's stereo position works out to: the pan at its onset, and the path when it moves.
#[derive(Debug, Clone, PartialEq)]
pub struct NotePan {
    pub pan: f64,
    /// Steps from the onset, interpolated linearly and holding the last value. Two points at the
    /// same `at` make a step. `None` when the pan does not move.
    pub pan_path: Option<Vec<PanPoint>>,
}

/// What a note's pan depends on besides the note.
pub struct PanScope<'a> {
    pub track_pan: f64,
    pub take: &'a Take,
    /// The section's length in steps.
    pub section_steps: f64,
    pub steps_per_bar: f64,
    /// Rank of each distinct onset of the take (by the bits of `at`), from 0.
    pub ranks: HashMap<u64, usize>,
}

/// The rank of every distinct onset of a take's notes, counting from 0 in time order.
pub fn onset_ranks(notes: &[Note]) -> HashMap<u64, usize> {
    let mut onsets: Vec<f64> = notes.iter().map(|note| note.at).collect();
    onsets.sort_by(f64::total_cmp);
    onsets.dedup();
    onsets
        .into_iter()
        .enumerate()
        .map(|(rank, at)| (key(at), rank))
        .collect()
}

/// A hash key for an onset: `0.0` and `-0.0` are the same onset.
fn key(at: f64) -> u64 {
    (at + 0.0).to_bits()
}

/// `Math.round` to three decimals.
pub fn round3(value: f64) -> f64 {
    js::round(value * 1000.0) / 1000.0 + 0.0
}

fn clamp(value: f64) -> f64 {
    value.max(-1.0).min(1.0)
}

fn fraction(value: f64) -> f64 {
    value - value.floor()
}

/// The triangle wave of a cycle count: 0 at 0, 1 at a quarter, 0 at a half, -1 at three quarters.
pub fn triangle(cycles: f64) -> f64 {
    1.0 - 4.0 * (fraction(cycles + 0.25) - 0.5).abs()
}

/// Where a motion starts and how long it runs, in steps from the section's start.
struct Reach {
    start: f64,
    length: f64,
}

fn positions_of(positions: &Option<Vec<f64>>) -> Vec<f64> {
    positions
        .clone()
        .unwrap_or_else(|| DEFAULT_JUMP_POSITIONS.to_vec())
}

fn jump_index(count: usize, every: &JumpEvery, t: f64, rank: usize) -> usize {
    match every {
        JumpEvery::Steps(steps) => ((t / steps).floor() % count as f64) as usize,
        _ => rank % count,
    }
}

/// The pan a motion gives at step `t` of the section, before rounding.
fn motion_at(
    motion: &PanMotion,
    t: f64,
    reach: &Reach,
    centre: f64,
    steps_per_bar: f64,
    rank: usize,
) -> f64 {
    match motion {
        PanMotion::Slide { from, to, curve } => {
            let u = if reach.length > 0.0 {
                (t - reach.start) / reach.length
            } else {
                0.0
            };
            let along = if *curve == Some(PanCurve::Smooth) {
                (1.0 - (std::f64::consts::PI * u).cos()) / 2.0
            } else {
                u
            };
            from + (to - from) * along
        }
        PanMotion::Wobble {
            centre: own_centre,
            depth,
            period,
            shape,
            phase,
        } => {
            let cycles = t / period.unwrap_or(steps_per_bar) + phase.unwrap_or(0.0);
            let wave = if *shape == Some(WobbleShape::Triangle) {
                triangle(cycles)
            } else {
                (2.0 * std::f64::consts::PI * cycles).sin()
            };
            clamp(own_centre.unwrap_or(centre) + depth.unwrap_or(DEFAULT_WOBBLE_DEPTH) * wave)
        }
        PanMotion::Jump { positions, every } => {
            let positions = positions_of(positions);
            positions[jump_index(positions.len(), every, t, rank)]
        }
    }
}

/// The pan at `t` steps from the onset of a path: the last value held, a step taking its first.
pub fn pan_at(path: &[PanPoint], t: f64) -> f64 {
    let Some(index) = path.iter().position(|point| point.at >= t) else {
        return path[path.len() - 1].pan;
    };
    if index == 0 || path[index].at == t {
        return path[index].pan;
    }
    let before = path[index - 1];
    let after = path[index];
    before.pan + ((after.pan - before.pan) * (t - before.at)) / (after.at - before.at)
}

/// The same path for a note cut to `length` steps: its points before the cut, then the end.
/// `None` when it no longer moves.
pub fn cut_path(path: &[PanPoint], length: f64) -> Option<Vec<PanPoint>> {
    let mut kept: Vec<PanPoint> = path
        .iter()
        .copied()
        .filter(|point| point.at < length)
        .collect();
    kept.push(PanPoint {
        at: length,
        pan: pan_at(path, length),
    });
    if kept.iter().all(|point| point.pan == kept[0].pan) {
        None
    } else {
        Some(kept)
    }
}

/// The points of a motion over one note, as steps from its onset.
fn motion_path(
    motion: &PanMotion,
    at: f64,
    len: f64,
    reach: &Reach,
    centre: f64,
    steps_per_bar: f64,
    rank: usize,
) -> Vec<PanPoint> {
    let value_at = |t: f64| round3(motion_at(motion, t, reach, centre, steps_per_bar, rank));
    let end = at + len;
    let mut points = vec![PanPoint {
        at: 0.0,
        pan: value_at(at),
    }];
    match motion {
        PanMotion::Jump { every, .. } => {
            let JumpEvery::Steps(every) = every else {
                return points;
            };
            let mut previous = points[0].pan;
            let mut k = (at / every).floor() + 1.0;
            while k * every < end {
                let boundary = k * every;
                k += 1.0;
                if boundary <= at {
                    continue;
                }
                let next = value_at(boundary);
                points.push(PanPoint {
                    at: boundary - at,
                    pan: previous,
                });
                points.push(PanPoint {
                    at: boundary - at,
                    pan: next,
                });
                previous = next;
            }
            return points;
        }
        PanMotion::Slide { curve, .. } if *curve != Some(PanCurve::Smooth) => {
            points.push(PanPoint {
                at: len,
                pan: value_at(end),
            });
            return points;
        }
        _ => {}
    }
    let mut marks: Vec<f64> = Vec::new();
    match motion {
        PanMotion::Wobble {
            shape: Some(WobbleShape::Triangle),
            period,
            phase,
            ..
        } => {
            // The corners of the wave: a peak a quarter in, a trough three quarters in.
            let period = period.unwrap_or(steps_per_bar);
            let phase = phase.unwrap_or(0.0);
            let mut k = ((at / period + phase - 0.25) / 0.5).floor();
            loop {
                let t = (0.25 + 0.5 * k - phase) * period;
                if t >= end {
                    break;
                }
                if t > at {
                    marks.push(t);
                }
                k += 1.0;
            }
        }
        _ => {
            let period = match motion {
                PanMotion::Wobble { period, .. } => period.unwrap_or(steps_per_bar),
                _ => f64::INFINITY,
            };
            let h = (0.5_f64.min(period / 16.0)).max(len / 256.0);
            let mut k = (at / h).floor() + 1.0;
            while k * h < end {
                if k * h > at {
                    marks.push(k * h);
                }
                k += 1.0;
            }
        }
    }
    for t in marks {
        points.push(PanPoint {
            at: t - at,
            pan: value_at(t),
        });
    }
    points.push(PanPoint {
        at: len,
        pan: value_at(end),
    });
    points
}

/// Works out where a note sits. The first of these that is there decides: the note's `panMotion`,
/// its `pan`, the take's `panMotion`, the take's `pan`, the track's `pan`. `len` is the note's
/// length in the section (cut at the section's end).
pub fn note_pan(note: &Note, len: f64, scope: &PanScope) -> NotePan {
    let motion = match (&note.pan_motion, note.pan) {
        (Some(motion), _) => Some(motion),
        (None, None) => scope.take.pan_motion.as_ref(),
        (None, Some(_)) => None,
    };
    let otherwise = scope.take.pan.unwrap_or(scope.track_pan);
    let Some(motion) = motion else {
        return NotePan {
            pan: note.pan.unwrap_or(otherwise),
            pan_path: None,
        };
    };
    let reach = if note.pan_motion.is_some() {
        Reach {
            start: note.at,
            length: len,
        }
    } else {
        Reach {
            start: 0.0,
            length: scope.section_steps,
        }
    };
    let rank = scope.ranks.get(&key(note.at)).copied().unwrap_or(0);
    let path = motion_path(
        motion,
        note.at,
        len,
        &reach,
        otherwise,
        scope.steps_per_bar,
        rank,
    );
    let first = path[0].pan;
    let moves = path.iter().any(|point| point.pan != first);
    NotePan {
        pan: first,
        pan_path: moves.then_some(path),
    }
}
