// A compiled score as Undertone's tick-timed score, and the score's onsets as voices to book
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Undertone's `score()` (`score.ts`), its events at 12 ticks a step, with the part of `renderToBuffer` and
//! `collectOnsets` that turns one pass into voices. Pattern time is exact: an event begins at
//! `tick / lengthTicks` of the cycle, reduced to lowest terms as Undertone's `Fraction` keeps it,
//! and its start in seconds is that fraction as a double times the cycle's length, the same two
//! operations the TypeScript performs.

use crate::error::Error;
use crate::math;
use crate::params::{Params, Patch, resolve_params};
use crate::pitch::{Pitch, json_number, midi_to_frequency};

/// Score ticks a grid step: twelve keeps triplets exact on any grid.
pub const TICKS_PER_STEP: f64 = 12.0;

/// A score event's pitch: a MIDI number for a pitched note, a pitch for a drum layer.
#[derive(Debug, Clone, PartialEq)]
pub enum ScorePitch {
	/// A MIDI note number.
	Midi(f64),
	/// A pitch, as a drum layer's.
	Note(Pitch),
}

/// One `ScoreEvent`: an onset in ticks, a gate in ticks, a level, a pan and a voice.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreEvent {
	/// Where the event begins, in ticks.
	pub tick: f64,
	/// What it plays.
	pub pitch: ScorePitch,
	/// How long the gate stays open, in ticks.
	pub gate: f64,
	/// Its velocity, if set.
	pub level: Option<f64>,
	/// Its pan, if set.
	pub pan: Option<f64>,
	/// The voice it plays, as a partial set of parameters.
	pub voice: Patch,
	/// The index of the performance event it plays, so a render can tell the tracks apart.
	pub source: usize,
}

/// An event of a compiled score: where it begins in the cycle, as a fraction in lowest terms, and
/// the patch it plays.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreOnset {
	/// Where in the cycle it begins, as numerator and denominator.
	pub begin: (i64, i64),
	/// The voice with its pitch, duration, velocity and pan filled in.
	pub value: Patch,
	/// The performance event it plays.
	pub source: usize,
}

/// A compiled score (`score()`): one cycle is one pass.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
	/// The tick rate.
	pub ticks_per_second: f64,
	/// One pass in ticks.
	pub length_ticks: f64,
	/// The events in onset order (a stable sort, so events at one tick keep their order).
	pub onsets: Vec<ScoreOnset>,
	/// Cycles a second.
	pub cps: f64,
	/// One pass in seconds.
	pub seconds: f64,
}

fn require_integer(value: f64, label: &str, min: f64) -> Result<(), Error> {
	if value.fract() != 0.0 || !value.is_finite() || value < min {
		return Err(Error::new(format!(
			"score() {label} must be an integer of at least {}, got {}",
			undertone_mini::js::number_to_string(min),
			undertone_mini::js::number_to_string(value)
		)));
	}
	Ok(())
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
	while b != 0 {
		(a, b) = (b, a % b);
	}
	a
}

/// `new Fraction(n, d)` in lowest terms, the denominator positive.
fn fraction(n: i64, d: i64) -> (i64, i64) {
	let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
	let g = gcd(n.abs(), d);
	let g = if g == 0 { 1 } else { g };
	(n / g, d / g)
}

/// `score({ ticksPerSecond, lengthTicks, events })` with the default seam, `cut`: a gate that runs
/// past the end of the pass is cut there.
pub fn score(
	ticks_per_second: f64,
	length_ticks: f64,
	events: &[ScoreEvent],
) -> Result<Score, Error> {
	if ticks_per_second.is_nan() || ticks_per_second <= 0.0 {
		return Err(Error::new(format!(
			"score() ticksPerSecond must be positive, got {}",
			undertone_mini::js::number_to_string(ticks_per_second)
		)));
	}
	require_integer(length_ticks, "lengthTicks", 1.0)?;
	let mut onsets = Vec::with_capacity(events.len());
	for event in events {
		require_integer(event.tick, "event tick", 0.0)?;
		if event.tick >= length_ticks {
			return Err(Error::new(format!(
				"score() event tick {} is outside the cue (length {})",
				undertone_mini::js::number_to_string(event.tick),
				undertone_mini::js::number_to_string(length_ticks)
			)));
		}
		require_integer(event.gate, "event gate", 1.0)?;
		let gate_ticks = math::min(event.gate, length_ticks - event.tick);
		let mut value = event.voice.clone();
		let pitch = match &event.pitch {
			ScorePitch::Note(pitch) => pitch.to_value(),
			ScorePitch::Midi(midi) => json_number(midi_to_frequency(*midi)),
		};
		value.insert("pitch".into(), pitch);
		value.insert(
			"duration".into(),
			json_number(gate_ticks / ticks_per_second),
		);
		if let Some(level) = event.level {
			value.insert("velocity".into(), json_number(level));
		}
		if let Some(pan) = event.pan {
			value.insert("pan".into(), json_number(pan));
		}
		onsets.push(ScoreOnset {
			begin: fraction(event.tick as i64, length_ticks as i64),
			value,
			source: event.source,
		});
	}
	// Every begin shares the denominator `lengthTicks` before reduction, so comparing the reduced
	// fractions by cross-multiplying orders them as the ticks do.
	onsets.sort_by(|a, b| (a.begin.0 * b.begin.1).cmp(&(b.begin.0 * a.begin.1)));
	let cps = ticks_per_second / length_ticks;
	Ok(Score {
		ticks_per_second,
		length_ticks,
		onsets,
		cps,
		seconds: length_ticks / ticks_per_second,
	})
}

/// A voice of the pass: when it starts and its full parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingVoice {
	/// When it starts, in seconds.
	pub start_time: f64,
	/// Its parameters, resolved.
	pub params: Params,
	/// The performance event it plays.
	pub source: usize,
}

/// The voices of the score's first cycle, as `renderToBuffer` collects them (`collectOnsets`,
/// gated): each onset at `begin × cycleSeconds`, its gate already in the patch's `duration`.
pub fn collect_onsets(score: &Score) -> Vec<PendingVoice> {
	let cycle_seconds = 1.0 / score.cps;
	score
		.onsets
		.iter()
		.map(|onset| {
			let begin = onset.begin.0 as f64 / onset.begin.1 as f64;
			PendingVoice {
				start_time: 0.0 + begin * cycle_seconds,
				params: resolve_params(&onset.value),
				source: onset.source,
			}
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn event(tick: f64, gate: f64) -> ScoreEvent {
		ScoreEvent {
			tick,
			pitch: ScorePitch::Midi(69.0),
			gate,
			level: Some(0.5),
			pan: Some(0.0),
			voice: Patch::new(),
			source: 0,
		}
	}

	#[test]
	fn the_seam_cuts_a_gate_and_onsets_follow_ticks() {
		let score = score(24.0, 48.0, &[event(36.0, 24.0), event(12.0, 6.0)]).unwrap();
		assert_eq!(score.cps, 0.5);
		assert_eq!(score.seconds, 2.0);
		assert_eq!(score.onsets[0].begin, (1, 4));
		assert_eq!(score.onsets[1].value["duration"].as_f64(), Some(0.5));
		assert_eq!(score.onsets[1].value["pitch"].as_f64(), Some(440.0));
		let voices = collect_onsets(&score);
		assert_eq!(voices[0].start_time, 0.5);
		assert_eq!(voices[1].start_time, 1.5);
		assert_eq!(voices[1].params.velocity, Some(0.5));
	}

	#[test]
	fn a_tick_outside_the_cue_is_refused() {
		assert_eq!(
			score(24.0, 48.0, &[event(48.0, 1.0)]).unwrap_err().message,
			"score() event tick 48 is outside the cue (length 48)"
		);
		assert!(score(24.0, 48.5, &[]).is_err());
		assert!(score(0.0, 48.0, &[]).is_err());
	}

	#[test]
	fn fractions_reduce() {
		assert_eq!(fraction(12, 48), (1, 4));
		assert_eq!(fraction(0, 48), (0, 1));
		assert_eq!(fraction(3, -6), (-1, 2));
	}
}
