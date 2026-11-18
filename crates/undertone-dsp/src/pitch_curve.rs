// A voice's frequency automation (steps, a pitch envelope or a slide) as ratios of its pitch
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `buildPitchCurve` and `applyPitchCurve` from Undertone's `pitchCurve.ts`. The curve is relative
//! to the base pitch, so the carrier, an FM modulator at a multiple of it and the modulator's depth
//! all follow it by scaling the ratios. When several are set, steps win, then the envelope, then
//! the slide.

use crate::graph::{Automation, Graph, ParamId};
use crate::math;
use crate::params::Params;

/// A slide starts an octave above its target.
const SLIDE_START_RATIO: f64 = 2.0;
const DEFAULT_PITCH_DECAY_SECONDS: f64 = 0.1;
/// The frame rate of classic AdLib sound effects.
const DEFAULT_PITCH_STEP_RATE_HZ: f64 = 140.0;

/// One point of a pitch curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PitchPoint {
	/// `setValueAtTime(base · ratio, time)`.
	Set {
		/// The multiple of the base frequency.
		ratio: f64,
		/// When, in seconds.
		time: f64,
	},
	/// `exponentialRampToValueAtTime(base · ratio, time)`.
	Ramp {
		/// The multiple of the base frequency.
		ratio: f64,
		/// When it is reached, in seconds.
		time: f64,
	},
}

fn semitones_to_ratio(semitones: f64) -> f64 {
	math::pow(2.0, semitones / 12.0)
}

/// `buildPitchCurve`: the frequency automation for a voice that starts at `voice_start`.
pub fn build_pitch_curve(params: &Params, voice_start: f64) -> Vec<PitchPoint> {
	if let Some(steps) = params
		.pitch_steps
		.as_ref()
		.filter(|steps| !steps.is_empty())
	{
		let rate = params.pitch_step_rate.unwrap_or(DEFAULT_PITCH_STEP_RATE_HZ);
		return steps
			.iter()
			.enumerate()
			.map(|(frame, semitones)| PitchPoint::Set {
				ratio: semitones_to_ratio(*semitones),
				time: voice_start + frame as f64 / rate,
			})
			.collect();
	}
	if let Some(amount) = params.pitch_env_amount.filter(|amount| *amount != 0.0) {
		let peak = semitones_to_ratio(amount);
		let attack = params.pitch_attack.unwrap_or(0.0);
		let decay = params.pitch_decay.unwrap_or(DEFAULT_PITCH_DECAY_SECONDS);
		let mut points = Vec::new();
		if attack > 0.0 {
			points.push(PitchPoint::Set {
				ratio: 1.0,
				time: voice_start,
			});
			points.push(PitchPoint::Ramp {
				ratio: peak,
				time: voice_start + attack,
			});
		} else {
			points.push(PitchPoint::Set {
				ratio: peak,
				time: voice_start,
			});
		}
		points.push(if decay > 0.0 {
			PitchPoint::Ramp {
				ratio: 1.0,
				time: voice_start + attack + decay,
			}
		} else {
			PitchPoint::Set {
				ratio: 1.0,
				time: voice_start + attack,
			}
		});
		return points;
	}
	if params.slide_time > 0.0 {
		return vec![
			PitchPoint::Set {
				ratio: SLIDE_START_RATIO,
				time: voice_start,
			},
			PitchPoint::Ramp {
				ratio: 1.0,
				time: voice_start + params.slide_time,
			},
		];
	}
	vec![PitchPoint::Set {
		ratio: 1.0,
		time: voice_start,
	}]
}

/// `applyPitchCurve`: writes the curve onto `param`, every ratio multiplied by `base`.
pub fn apply_pitch_curve<G: Graph>(
	graph: &mut G,
	param: ParamId,
	base: f64,
	points: &[PitchPoint],
) {
	for point in points {
		let step = match *point {
			PitchPoint::Set { ratio, time } => Automation::Set {
				value: base * ratio,
				at: time,
			},
			PitchPoint::Ramp { ratio, time } => Automation::Exponential {
				value: base * ratio,
				until: time,
			},
		};
		graph.automate(param, step);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::params::DEFAULT_PARAMS;

	#[test]
	fn a_plain_voice_sets_its_pitch_once() {
		assert_eq!(
			build_pitch_curve(&DEFAULT_PARAMS, 1.0),
			vec![PitchPoint::Set {
				ratio: 1.0,
				time: 1.0
			}]
		);
	}

	#[test]
	fn a_slide_falls_from_an_octave_above() {
		let params = Params {
			slide_time: 0.05,
			..DEFAULT_PARAMS
		};
		assert_eq!(
			build_pitch_curve(&params, 0.0),
			vec![
				PitchPoint::Set {
					ratio: 2.0,
					time: 0.0
				},
				PitchPoint::Ramp {
					ratio: 1.0,
					time: 0.05
				}
			]
		);
	}

	#[test]
	fn steps_win_over_an_envelope_and_an_envelope_over_a_slide() {
		let params = Params {
			pitch_steps: Some(vec![12.0, 0.0]),
			pitch_step_rate: Some(10.0),
			pitch_env_amount: Some(24.0),
			slide_time: 0.1,
			..DEFAULT_PARAMS
		};
		assert_eq!(
			build_pitch_curve(&params, 0.0),
			vec![
				PitchPoint::Set {
					ratio: 2.0,
					time: 0.0
				},
				PitchPoint::Set {
					ratio: 1.0,
					time: 0.1
				}
			]
		);
		let params = Params {
			pitch_env_amount: Some(12.0),
			pitch_attack: Some(0.01),
			pitch_decay: Some(0.0),
			slide_time: 0.1,
			..DEFAULT_PARAMS
		};
		assert_eq!(
			build_pitch_curve(&params, 0.0),
			vec![
				PitchPoint::Set {
					ratio: 1.0,
					time: 0.0
				},
				PitchPoint::Ramp {
					ratio: 2.0,
					time: 0.01
				},
				PitchPoint::Set {
					ratio: 1.0,
					time: 0.01
				}
			]
		);
	}
}
