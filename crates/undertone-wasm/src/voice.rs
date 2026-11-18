// Renders one Undertone voice to stereo samples.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A note through `undertone-dsp`'s renderer: one event on a one-pass score, through the master
//! bus. It is the smallest useful entry into the crate from a web page.

use serde_json::json;
use undertone_dsp::params::Patch;
use undertone_dsp::render::{RenderRequest, render_score};
use undertone_dsp::score::{ScoreEvent, ScorePitch, score};

/// The ticks a second of the one-event score.
const TICKS_PER_SECOND: f64 = 96.0;

/// Renders `midi` (a MIDI note number) on the `sound` waveform, held for `seconds`, with half a
/// second of tail, as interleaved stereo `f32`. `sound` is one of `sine`, `triangle`, `square` and
/// `sawtooth`.
pub fn render_note(
	sound: &str,
	midi: f64,
	seconds: f64,
	sample_rate: f64,
) -> Result<Vec<f32>, String> {
	if !matches!(sound, "sine" | "triangle" | "square" | "sawtooth") {
		return Err(format!(
			"Unknown waveform \"{sound}\": use sine, triangle, square or sawtooth"
		));
	}
	if !(seconds > 0.0 && seconds <= 30.0) {
		return Err("The note must last more than 0 and at most 30 seconds".to_string());
	}
	let mut voice = Patch::new();
	voice.insert("soundType".into(), json!(sound));
	voice.insert("attack".into(), json!(0.005));
	voice.insert("decay".into(), json!(0.3));
	voice.insert("sustain".into(), json!(0.5));
	voice.insert("release".into(), json!(0.25));

	let ticks = (seconds * TICKS_PER_SECOND).round().max(1.0);
	let event = ScoreEvent {
		tick: 0.0,
		pitch: ScorePitch::Midi(midi),
		gate: ticks,
		level: Some(0.8),
		pan: Some(0.0),
		voice,
		source: 0,
	};
	let pass = score(TICKS_PER_SECOND, ticks, &[event]).map_err(|error| error.to_string())?;
	let request = RenderRequest {
		tail: 0.5,
		sample_rate,
		..RenderRequest::EXPORT
	};
	let sound = render_score(&pass, &request).map_err(|error| error.to_string())?;
	Ok(sound.interleaved())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_note_makes_stereo_sound_and_a_tail() {
		let samples = render_note("triangle", 69.0, 0.5, 48_000.0).unwrap();
		assert_eq!(samples.len() % 2, 0);
		assert!(
			samples.len() >= 2 * 48_000,
			"a half-second note and a half-second tail"
		);
		assert!(samples.iter().any(|s| s.abs() > 0.01));
	}

	#[test]
	fn bad_input_is_refused_with_a_reason() {
		assert!(
			render_note("kazoo", 69.0, 1.0, 48_000.0)
				.unwrap_err()
				.contains("kazoo")
		);
		assert!(render_note("sine", 69.0, 0.0, 48_000.0).is_err());
		assert!(render_note("sine", 69.0, 1.0, 0.0).is_err());
	}
}
