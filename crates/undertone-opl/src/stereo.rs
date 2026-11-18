// OPL3 stereo placement: a pan as one channel to the left, right or both, or two channels at equal power
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The continuous pan law (`mix.panLaw`): how a pan becomes one OPL3 channel or a pair.

use crate::numbers::{round, to_fixed};
use crate::registers::{OUTPUT_BOTH, OUTPUT_LEFT, OUTPUT_RIGHT, attenuation};
use crate::sequence::OplChip;

/// Within this distance of the centre, a note plays on one channel to both sides.
pub const CENTRE_WIDTH: f64 = 0.1;

/// From this distance of the centre out, a note plays on one channel to its side only.
pub const HARD_FROM: f64 = 0.9;

/// The song's centre level when it sets none: each side 3 dB down, the classic equal-power law.
pub const DEFAULT_PAN_LAW: f64 = -3.0;

/// One channel of a placement: its OPL3 output bits and the gain on that channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedChannel {
	/// The OPL3 output bits (`0x10` left, `0x20` right, `0x30` both).
	pub output: i32,
	/// Gain relative to the note's own level, 1 for a single channel.
	pub gain: f64,
}

#[derive(Debug, Clone, PartialEq)]
/// How a note is placed on the OPL3.
pub struct StereoPlan {
	/// One channel, or two (the louder side first) for a pan between the centre and hard.
	pub channels: Vec<PlacedChannel>,
	/// The single channel a paired note falls back to when no second channel is free.
	pub fallback: PlacedChannel,
}

/// OPL3 output bits for a pan on one channel: the chip only has left, right and both.
pub fn pan_output(pan: f64) -> i32 {
	if pan <= -0.34 {
		OUTPUT_LEFT
	} else if pan >= 0.34 {
		OUTPUT_RIGHT
	} else {
		OUTPUT_BOTH
	}
}

/// The gain of each side at the centre under a law (dB, clamped to 0 to -6).
pub fn centre_gain(law: f64) -> f64 {
	10f64.powf(crate::numbers::min(0.0, crate::numbers::max(-6.0, law)) / 20.0)
}

/// The gain of a hard-panned side under a law: full, except below -3 dB where the whole curve sits lower.
pub fn hard_gain(law: f64) -> f64 {
	(std::f64::consts::SQRT_2 * centre_gain(law)).min(1.0)
}

/// Equal-power gains: the angle (pan + 1) * pi/4 runs from 0 (left) to pi/2 (right).
pub fn equal_power_gains(pan: f64) -> (f64, f64) {
	let angle = ((pan.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::PI) / 4.0;
	(angle.cos(), angle.sin())
}

/// Gains for each side under a pan law, given as the level of each side at the centre in dB (0 to
/// -6). The equal-power curve is scaled so the centre lands on that level and each side is capped
/// at full, which keeps the curve continuous from hard left through the centre to hard right.
pub fn pan_gains(pan: f64, law: f64) -> (f64, f64) {
	let (left, right) = equal_power_gains(pan);
	let scale = std::f64::consts::SQRT_2 * centre_gain(law);
	((left * scale).min(1.0), (right * scale).min(1.0))
}

/// Where a note with this pan sounds on the OPL3, under the song's pan law. A channel sent to both
/// sides plays at the law's centre level, so the centre is no louder than the pairs beside it.
pub fn stereo_plan(pan: f64, law: f64) -> StereoPlan {
	let output = pan_output(pan);
	let fallback = PlacedChannel {
		output,
		gain: if output == OUTPUT_BOTH {
			centre_gain(law)
		} else {
			hard_gain(law)
		},
	};
	let distance = pan.abs();
	if distance <= CENTRE_WIDTH {
		return StereoPlan {
			channels: vec![PlacedChannel {
				output: OUTPUT_BOTH,
				gain: centre_gain(law),
			}],
			fallback,
		};
	}
	if distance >= HARD_FROM {
		return StereoPlan {
			channels: vec![PlacedChannel {
				output: if pan < 0.0 { OUTPUT_LEFT } else { OUTPUT_RIGHT },
				gain: hard_gain(law),
			}],
			fallback,
		};
	}
	let (left, right) = pan_gains(pan, law);
	let left = PlacedChannel {
		output: OUTPUT_LEFT,
		gain: left,
	};
	let right = PlacedChannel {
		output: OUTPUT_RIGHT,
		gain: right,
	};
	StereoPlan {
		channels: if pan < 0.0 {
			vec![left, right]
		} else {
			vec![right, left]
		},
		fallback,
	}
}

/// How a pan is placed, for a tool to show beside a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OplPlacement {
	/// Short: `mono`, `C`, `L`, `R`, or the power on each side such as `L85·R15`.
	pub label: String,
	/// A sentence on how the chip places such a pan.
	pub detail: String,
	/// Channels each note takes when they are free.
	pub channels: u8,
}

fn decibels(gain: f64) -> String {
	let steps = attenuation(gain);
	if steps == 0 {
		"0 dB".to_string()
	} else {
		format!("\u{2212}{} dB", to_fixed(f64::from(steps) * 0.75, 1))
	}
}

/// The placement of a pan on a chip, as the sequencer will play it when channels are free.
pub fn opl_placement(chip: OplChip, pan: f64, law: f64) -> OplPlacement {
	if chip == OplChip::Opl2 {
		return OplPlacement {
			label: "mono".to_string(),
			detail: "The OPL2 has one output.".to_string(),
			channels: 1,
		};
	}
	let plan = stereo_plan(pan, law);
	if plan.channels.len() == 1 {
		let output = plan.channels[0].output;
		return if output == OUTPUT_BOTH {
			OplPlacement {
				label: "C".to_string(),
				detail: format!(
					"One channel, to both sides at {} (the pan law).",
					decibels(centre_gain(law))
				),
				channels: 1,
			}
		} else if output == OUTPUT_LEFT {
			OplPlacement {
				label: "L".to_string(),
				detail: "One channel, left only.".to_string(),
				channels: 1,
			}
		} else {
			OplPlacement {
				label: "R".to_string(),
				detail: "One channel, right only.".to_string(),
				channels: 1,
			}
		};
	}
	let (left, right) = pan_gains(pan, law);
	let left_share = round(((left * left) / (left * left + right * right)) * 100.0) as i32;
	let fallback = pan_output(pan);
	let fallback_name = if fallback == OUTPUT_BOTH {
		"both sides"
	} else if fallback == OUTPUT_LEFT {
		"the left"
	} else {
		"the right"
	};
	OplPlacement {
		label: format!("L{left_share}\u{b7}R{}", 100 - left_share),
		detail: format!(
			"Two channels: left {}, right {}. When no second channel is free, one channel to {fallback_name}.",
			decibels(left),
			decibels(right)
		),
		channels: 2,
	}
}
