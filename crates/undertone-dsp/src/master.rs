// The master bus between the voices and the speakers: a gain stage, a limiter and a make-up gain
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `createMasterBus` from Undertone's `masterBus.ts`, with its default limiter: threshold −2 dB,
//! a hard knee, ratio 20, 3 ms attack and 100 ms release, then a gain that cancels the automatic
//! make-up gain browsers add to a compressor. The limiter is skipped on an output of more than two
//! channels, which a compressor would fold to stereo.

use crate::graph::{Automation, Graph, Input, NodeId, NodeKind, Param, ParamId};
use crate::math;

/// The limiter's settings (`MasterLimiterOptions`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limiter {
	/// Level in decibels above which the limiter acts.
	pub threshold: f64,
	/// Width in decibels of the soft transition around the threshold.
	pub knee: f64,
	/// The compression ratio.
	pub ratio: f64,
	/// Seconds to reduce the gain by 10 dB.
	pub attack: f64,
	/// Seconds to raise the gain by 10 dB.
	pub release: f64,
	/// Cancel the compressor's automatic make-up gain.
	pub compensate: bool,
}

/// Undertone's default limiter.
pub const LIMITER: Limiter = Limiter {
	threshold: -2.0,
	knee: 0.0,
	ratio: 20.0,
	attack: 0.003,
	release: 0.1,
	compensate: true,
};

/// The browsers' make-up exponent: the reciprocal of a full-scale input's gain, raised to 0.6.
const MAKEUP_EXPONENT: f64 = 0.6;

/// `makeupCompensation`: the gain that cancels the automatic make-up for a hard knee.
pub fn makeup_compensation(threshold_db: f64, ratio: f64) -> f64 {
	// `!(threshold < 0) || !(ratio > 1)`: NaN in either also means no make-up.
	if threshold_db.is_nan() || threshold_db >= 0.0 || ratio.is_nan() || ratio <= 1.0 {
		return 1.0;
	}
	let full_scale_out_db = threshold_db + (0.0 - threshold_db) / ratio;
	let full_scale_gain = math::pow(10.0, full_scale_out_db / 20.0);
	math::pow(1.0 / full_scale_gain, -MAKEUP_EXPONENT)
}

/// The master bus's nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MasterBus {
	/// Where the voices play into.
	pub input: NodeId,
	/// The limiter, when there is one.
	pub compressor: Option<NodeId>,
}

/// `createMasterBus(ctx, { gain, limiter })` into `output`, whose channel count is
/// `output_channels`, with every param set at `now`.
pub fn master_bus<G: Graph>(
	graph: &mut G,
	output: NodeId,
	output_channels: u32,
	gain: f64,
	limiter: Option<Limiter>,
	now: f64,
) -> MasterBus {
	let input = graph.node(NodeKind::Gain);
	graph.automate(
		ParamId::new(input, Param::Gain),
		Automation::Set {
			value: gain,
			at: now,
		},
	);
	let Some(settings) = limiter.filter(|_| output_channels <= 2) else {
		graph.connect(input, 0, Input::node(output));
		return MasterBus {
			input,
			compressor: None,
		};
	};
	let compressor = graph.node(NodeKind::Compressor);
	for (param, value) in [
		(Param::Threshold, settings.threshold),
		(Param::Knee, settings.knee),
		(Param::Ratio, settings.ratio),
		(Param::Attack, settings.attack),
		(Param::Release, settings.release),
	] {
		graph.automate(
			ParamId::new(compressor, param),
			Automation::Set { value, at: now },
		);
	}
	graph.connect(input, 0, Input::node(compressor));
	if settings.compensate {
		let makeup = graph.node(NodeKind::Gain);
		graph.automate(
			ParamId::new(makeup, Param::Gain),
			Automation::Set {
				value: makeup_compensation(settings.threshold, settings.ratio),
				at: now,
			},
		);
		graph.connect(compressor, 0, Input::node(makeup));
		graph.connect(makeup, 0, Input::node(output));
	} else {
		graph.connect(compressor, 0, Input::node(output));
	}
	MasterBus {
		input,
		compressor: Some(compressor),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::log::{LogGraph, num};

	#[test]
	fn the_make_up_cancels_the_browsers_gain() {
		assert_eq!(num(makeup_compensation(-2.0, 20.0)), "0.877000821");
		assert_eq!(makeup_compensation(0.0, 20.0), 1.0);
		assert_eq!(makeup_compensation(-2.0, 1.0), 1.0);
	}

	#[test]
	fn the_limiter_is_skipped_on_more_than_two_channels() {
		let mut graph = LogGraph::new(4, 10, 48000.0);
		let out = graph.destination();
		let bus = master_bus(&mut graph, out, 4, 1.0, Some(LIMITER), 0.0);
		assert_eq!(bus.compressor, None);
		assert_eq!(graph.node_count(), 2);
	}
}
