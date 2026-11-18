// One voice booked as Undertone's `playVoice` builds it: source, filters, envelope, placement, sends
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `playVoice`, `scheduleEnvelope`, `connectFm`, `connectPhaser`, `connectOutput`,
//! `connectEffectSends` and the cut groups from Undertone's `engine.ts`, making the same nodes in
//! the same order with the same numbers. A `Booking` is what Undertone keeps per context: the
//! seeds, the orbit buses, the shaped waves made so far and the voice holding each cut group.
//!
//! A voice is: a source (an oscillator, with an FM modulator when `fmIndex` is set, or a noise
//! buffer), a lowpass with its own envelope (`filterCutoff`), a highpass (`hpfCutoff`), a four-stage
//! phaser (`phaserRate`), the gain envelope, then a stereo panner (`pan`) or per-speaker gains into
//! a channel merger (`channelGains`), and sends to its orbit's reverb and delay. A sample voice
//! (`sampleName`) is skipped, as it is on a context that has decoded no samples, which is every
//! render's.

use std::collections::HashMap;

use undertone_mini::js::number_to_string;

use crate::effects::OrbitBuses;
use crate::error::Error;
use crate::graph::{
	Automation, FilterType, Graph, Input, NodeId, NodeKind, OscillatorType, Param, ParamId, Wave,
	WaveId,
};
use crate::math;
use crate::noise::noise_buffer;
use crate::params::{Curve, Params, SoundType};
use crate::pitch::note_to_frequency;
use crate::pitch_curve::{PitchPoint, apply_pitch_curve, build_pitch_curve};
use crate::random::Seeds;
use crate::waves::shaped_wave_table;

const DEFAULT_OSCILLATOR_FREQUENCY: f64 = 440.0;
/// Longer gates loop the noise buffer instead of growing it.
const MAX_NOISE_BUFFER_SECONDS: f64 = 2.0;
/// Headroom past the last ramp, so the ramp completes before the source stops.
const STOP_TAIL_SECONDS: f64 = 0.02;
const DEFAULT_ORBIT: f64 = 0.0;
const PHASER_STAGE_COUNT: usize = 4;
const PHASER_CENTER_HZ: f64 = 1000.0;
const PHASER_DEPTH_HZ: f64 = 600.0;
/// An early stop's fade: short enough to feel immediate, long enough not to click.
pub const DEFAULT_STOP_FADE_SECONDS: f64 = 0.01;
/// The lowest level an exponential envelope ramps to.
const EXP_RAMP_FLOOR: f64 = 1e-4;
/// A cut group's choke.
const CUT_FADE_SECONDS: f64 = 0.005;
/// `setTargetAtTime` gets about 99% of the way in five time constants.
const STOP_FADE_TIME_CONSTANTS: f64 = 5.0;
/// Most speakers a placement addresses (7.1).
const MAX_CHANNELS: usize = 8;

/// An envelope as scheduled, so a stopped voice can fade from wherever it really is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
	start: f64,
	base: f64,
	peak: f64,
	sustain: f64,
	attack_end: f64,
	decay_end: f64,
	release_start: f64,
	release: f64,
	/// When it has fully released.
	pub end: f64,
}

impl Envelope {
	/// The envelope's value at `t`, interpolated linearly whatever its curve, as the TypeScript does.
	pub fn value_at(&self, t: f64) -> f64 {
		let lerp = |from: f64, to: f64, t: f64, t0: f64, t1: f64| {
			from + ((to - from) * (t - t0)) / (t1 - t0)
		};
		if t <= self.start {
			return self.base;
		}
		if t < self.attack_end {
			return lerp(self.base, self.peak, t, self.start, self.attack_end);
		}
		if t < self.decay_end {
			return lerp(self.peak, self.sustain, t, self.attack_end, self.decay_end);
		}
		if t < self.release_start {
			return self.sustain;
		}
		if self.release > 0.0 && t < self.end {
			return lerp(self.sustain, self.base, t, self.release_start, self.end);
		}
		if self.release > 0.0 {
			self.base
		} else {
			self.sustain
		}
	}
}

/// The shape of an envelope to schedule (`scheduleEnvelope`'s arguments after the param).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeShape {
	/// When the envelope begins, in seconds.
	pub start: f64,
	/// The level before the attack and after the release.
	pub base: f64,
	/// The level at the top of the attack.
	pub peak: f64,
	/// Seconds from `base` to `peak`.
	pub attack: f64,
	/// Seconds from `peak` to the sustain level.
	pub decay: f64,
	/// The sustain level as a fraction of the way from `base` to `peak`.
	pub sustain: f64,
	/// Seconds from the end of the gate to `base`.
	pub release: f64,
	/// When the gate closes; without it the envelope is percussive.
	pub gate_end: Option<f64>,
	/// How the ramps are shaped.
	pub curve: Curve,
}

/// `scheduleEnvelope`: attack, decay, an optional hold until the gate closes, and release, ramping
/// between `base` and `peak`. An exponential curve floors its levels at 1e-4.
pub fn schedule_envelope<G: Graph>(
	graph: &mut G,
	param: ParamId,
	shape: EnvelopeShape,
) -> Envelope {
	let EnvelopeShape {
		start,
		base,
		peak,
		attack,
		decay,
		sustain,
		release,
		gate_end,
		curve,
	} = shape;
	let sustain_value = base + (peak - base) * sustain;
	let exp = curve == Curve::Exponential;
	let level = |v: f64| if exp { math::max(v, EXP_RAMP_FLOOR) } else { v };
	let ramp = |graph: &mut G, v: f64, t: f64| {
		let step = if exp {
			Automation::Exponential {
				value: level(v),
				until: t,
			}
		} else {
			Automation::Linear { value: v, until: t }
		};
		graph.automate(param, step);
	};
	let set =
		|graph: &mut G, v: f64, t: f64| graph.automate(param, Automation::Set { value: v, at: t });
	set(graph, level(base), start);
	let attack_end = start + attack;
	if attack > 0.0 {
		ramp(graph, peak, attack_end);
	} else {
		set(graph, level(peak), start);
	}
	let decay_end = attack_end + decay;
	if decay > 0.0 {
		ramp(graph, sustain_value, decay_end);
	} else {
		set(graph, level(sustain_value), decay_end);
	}
	let release_start = match gate_end {
		Some(gate_end) => math::max(decay_end, gate_end),
		None => decay_end,
	};
	if release_start > decay_end {
		set(graph, level(sustain_value), release_start);
	}
	let release_end = release_start + release;
	if release > 0.0 {
		ramp(graph, base, release_end);
	}
	Envelope {
		start,
		base,
		peak,
		sustain: sustain_value,
		attack_end,
		decay_end,
		release_start,
		release,
		end: release_end,
	}
}

/// A booked voice: when it plays, and what to fade and stop if it is cut short.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceHandle {
	/// When the voice starts, in seconds.
	pub start_time: f64,
	/// When the voice ends, in seconds.
	pub end_time: f64,
	gain: Option<(NodeId, Envelope)>,
	stoppables: Vec<NodeId>,
}

impl VoiceHandle {
	/// The handle of a voice that made no nodes.
	fn silent(at: f64) -> Self {
		VoiceHandle {
			start_time: at,
			end_time: at,
			gain: None,
			stoppables: Vec::new(),
		}
	}

	/// `handle.stop(when, { fade })`: cancels the envelope from `when`, holds the value it had,
	/// fades to silence over `fade` seconds, and moves every source's stop to then. Nothing happens
	/// once the voice has ended.
	pub fn stop<G: Graph>(&self, graph: &mut G, when: f64, fade: Option<f64>) {
		let Some((gain_node, envelope)) = self.gain else {
			return;
		};
		let stop_at = math::max(when, 0.0);
		if stop_at >= self.end_time {
			return;
		}
		let fade = math::max(fade.unwrap_or(DEFAULT_STOP_FADE_SECONDS), 0.0);
		let gain = ParamId::new(gain_node, Param::Gain);
		graph.automate(gain, Automation::Cancel { from: stop_at });
		graph.automate(
			gain,
			Automation::Set {
				value: envelope.value_at(stop_at),
				at: stop_at,
			},
		);
		let mut silent_at = stop_at;
		if fade > 0.0 {
			graph.automate(
				gain,
				Automation::Target {
					value: 0.0,
					from: stop_at,
					time_constant: fade / STOP_FADE_TIME_CONSTANTS,
				},
			);
			silent_at = stop_at + fade;
		} else {
			graph.automate(
				gain,
				Automation::Set {
					value: 0.0,
					at: stop_at,
				},
			);
		}
		for node in &self.stoppables {
			graph.stop(*node, math::min(silent_at, self.end_time));
		}
	}
}

/// What Undertone keeps per context while it books voices.
#[derive(Debug)]
pub struct Booking {
	/// The context's seed and per-key counts.
	pub seeds: Seeds,
	/// `ctx.currentTime`, which an offline render books at: 0.
	pub now: f64,
	buses: OrbitBuses,
	waves: HashMap<SoundType, WaveId>,
	/// The voice holding each cut group, per output.
	cut_groups: Vec<(NodeId, f64, VoiceHandle)>,
}

impl Booking {
	/// A context seeded with `seed` (`seedContext`), at time 0.
	pub fn new(seed: f64) -> Self {
		Booking {
			seeds: Seeds::new(seed),
			now: 0.0,
			buses: OrbitBuses::default(),
			waves: HashMap::new(),
			cut_groups: Vec::new(),
		}
	}

	/// `getShapedWave`: a shape's periodic wave, made once per context.
	fn shaped_wave<G: Graph>(&mut self, graph: &mut G, kind: SoundType) -> WaveId {
		if let Some(wave) = self.waves.get(&kind) {
			return *wave;
		}
		let (real, imag) = shaped_wave_table(kind);
		let wave = graph.periodic_wave(real, imag, false);
		self.waves.insert(kind, wave);
		wave
	}

	/// `setOscillatorWave`: a built-in type, or a shaped wave's periodic wave. Noise words, which an
	/// oscillator does not know, play as a sine.
	fn wave<G: Graph>(&mut self, graph: &mut G, kind: SoundType) -> Wave {
		let basic = match kind {
			SoundType::Square => OscillatorType::Square,
			SoundType::Sawtooth => OscillatorType::Sawtooth,
			SoundType::Triangle => OscillatorType::Triangle,
			SoundType::HalfSine | SoundType::AbsSine | SoundType::PulseSine => {
				return Wave::Periodic(self.shaped_wave(graph, kind));
			}
			_ => OscillatorType::Sine,
		};
		Wave::Basic(basic)
	}

	/// `claimCutGroup`: chokes the group's earlier voice at this one's onset, then holds the group.
	/// A voice booked before the holder's onset is itself choked there, and the holder stays.
	fn claim_cut_group<G: Graph>(
		&mut self,
		graph: &mut G,
		output: NodeId,
		group: f64,
		handle: VoiceHandle,
	) {
		let same = |a: f64, b: f64| a == b || (a.is_nan() && b.is_nan());
		let index = self
			.cut_groups
			.iter()
			.position(|(out, g, _)| *out == output && same(*g, group));
		if let Some(index) = index {
			let previous = &self.cut_groups[index].2;
			if previous.start_time < handle.start_time {
				previous.stop(graph, handle.start_time, Some(CUT_FADE_SECONDS));
			} else if handle.start_time < previous.start_time {
				handle.stop(graph, previous.start_time, Some(CUT_FADE_SECONDS));
				return;
			}
			self.cut_groups[index].2 = handle;
		} else {
			self.cut_groups.push((output, group, handle));
		}
	}
}

/// Where a voice plays into, and that node's channel count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output {
	/// The node.
	pub node: NodeId,
	/// Its channel count.
	pub channels: u32,
}

/// `playVoice(ctx, params, startTime, { output })`.
pub fn play_voice<G: Graph>(
	graph: &mut G,
	booking: &mut Booking,
	params: &Params,
	start_time: f64,
	output: Output,
) -> Result<VoiceHandle, Error> {
	// A sample voice's buffer is never decoded on a fresh context, so it is skipped.
	if params.sample_name.is_some() {
		return Ok(VoiceHandle::silent(math::max(
			start_time + params.nudge_time,
			0.0,
		)));
	}
	let voice_start = math::max(start_time + params.nudge_time, 0.0);
	// A gate means something only when there is a sustain level to hold.
	let gate_end = match params.duration {
		Some(duration) if params.sustain > 0.0 => Some(voice_start + duration),
		_ => None,
	};
	let gain_node = graph.node(NodeKind::Gain);
	let gain_envelope = schedule_envelope(
		graph,
		ParamId::new(gain_node, Param::Gain),
		EnvelopeShape {
			start: voice_start,
			base: 0.0,
			peak: params.gain_level * params.velocity.unwrap_or(1.0),
			attack: params.attack,
			decay: params.decay,
			sustain: params.sustain,
			release: params.release,
			gate_end,
			curve: Curve::Linear,
		},
	);
	let gain_release_end = gain_envelope.end;
	let mut filter_release_end = gain_release_end;
	let mut fm_modulator = None;

	let source = if params.sound_type.is_noise() {
		// The buffer is capped and looped for longer gates.
		let audible = math::max(gain_release_end - voice_start, 0.05);
		let duration = math::min(audible, MAX_NOISE_BUFFER_SECONDS);
		let key = format!(
			"noise|{}|{}",
			params.sound_type.as_str(),
			number_to_string(start_time)
		);
		let mut random = booking.seeds.context_random(&key);
		let buffer = noise_buffer(
			graph.sample_rate(),
			params.sound_type,
			duration,
			&mut random,
		);
		let buffer = graph.buffer(buffer);
		graph.node(NodeKind::BufferSource {
			buffer,
			looped: audible > duration,
		})
	} else {
		let wave = booking.wave(graph, params.sound_type);
		let oscillator = graph.node(NodeKind::Oscillator(wave));
		let target = match &params.pitch {
			Some(pitch) => note_to_frequency(pitch)?,
			None => DEFAULT_OSCILLATOR_FREQUENCY,
		};
		let curve = build_pitch_curve(params, voice_start);
		apply_pitch_curve(
			graph,
			ParamId::new(oscillator, Param::Frequency),
			target,
			&curve,
		);
		if params.fm_index.is_some_and(|index| index > 0.0) {
			fm_modulator = Some(connect_fm(
				graph,
				booking,
				oscillator,
				target,
				&curve,
				params,
				voice_start,
			));
		}
		oscillator
	};

	let mut tail = source;
	if let Some(cutoff) = params.filter_cutoff {
		let filter = graph.node(NodeKind::Biquad(FilterType::Lowpass));
		filter_release_end = schedule_envelope(
			graph,
			ParamId::new(filter, Param::Frequency),
			EnvelopeShape {
				start: voice_start,
				base: cutoff,
				peak: cutoff + params.filter_env_amount,
				attack: params.filter_attack,
				decay: params.filter_decay,
				sustain: params.filter_sustain,
				release: params.filter_release,
				gate_end,
				curve: Curve::Linear,
			},
		)
		.end;
		graph.connect(tail, 0, Input::node(filter));
		tail = filter;
	}
	let voice_end = math::max(gain_release_end, filter_release_end) + STOP_TAIL_SECONDS;
	if let Some(cutoff) = params.hpf_cutoff {
		let hpf = graph.node(NodeKind::Biquad(FilterType::Highpass));
		graph.automate(
			ParamId::new(hpf, Param::Frequency),
			Automation::Set {
				value: cutoff,
				at: voice_start,
			},
		);
		graph.connect(tail, 0, Input::node(hpf));
		tail = hpf;
	}
	let mut stoppables = vec![source];
	if let Some(modulator) = fm_modulator {
		stoppables.push(modulator);
	}
	if let Some(rate) = params.phaser_rate {
		tail = connect_phaser(graph, tail, rate, voice_start, voice_end, &mut stoppables);
	}
	graph.connect(tail, 0, Input::node(gain_node));
	let final_node = connect_output(graph, output, gain_node, params, voice_start);
	connect_effect_sends(graph, booking, output.node, final_node, params, voice_start);
	graph.start(source, voice_start);
	graph.stop(source, voice_end);
	if let Some(modulator) = fm_modulator {
		graph.start(modulator, voice_start);
		graph.stop(modulator, voice_end);
	}
	let handle = VoiceHandle {
		start_time: voice_start,
		end_time: voice_end,
		gain: Some((gain_node, gain_envelope)),
		stoppables,
	};
	if let Some(group) = params.cut_group {
		booking.claim_cut_group(graph, output.node, group, handle.clone());
	}
	Ok(handle)
}

/// `connectFm`: a modulator at `fmHarmonicity` × the carrier into a depth gain (index × the
/// modulator's frequency, so brightness follows pitch), through an optional envelope gain, into the
/// carrier's frequency. Both follow the carrier's pitch curve. The caller starts and stops it.
fn connect_fm<G: Graph>(
	graph: &mut G,
	booking: &mut Booking,
	carrier: NodeId,
	carrier_hz: f64,
	curve: &[PitchPoint],
	params: &Params,
	voice_start: f64,
) -> NodeId {
	let harmonicity = params.fm_harmonicity.unwrap_or(1.0);
	let index = params.fm_index.unwrap_or(0.0);
	let wave = booking.wave(graph, params.fm_wave.unwrap_or(SoundType::Sine));
	let modulator = graph.node(NodeKind::Oscillator(wave));
	apply_pitch_curve(
		graph,
		ParamId::new(modulator, Param::Frequency),
		carrier_hz * harmonicity,
		curve,
	);
	let depth = graph.node(NodeKind::Gain);
	apply_pitch_curve(
		graph,
		ParamId::new(depth, Param::Gain),
		carrier_hz * harmonicity * index,
		curve,
	);
	graph.connect(modulator, 0, Input::node(depth));
	let has_envelope = params.fm_attack.is_some()
		|| params.fm_decay.is_some()
		|| params.fm_sustain.is_some()
		|| params.fm_release.is_some();
	let frequency = Input::Param(ParamId::new(carrier, Param::Frequency));
	if has_envelope {
		// Strudel's rule: a decay with no sustain falls toward silence; otherwise the depth holds.
		let sustain =
			params
				.fm_sustain
				.unwrap_or(if params.fm_decay.is_some() { 0.0 } else { 1.0 });
		let envelope = graph.node(NodeKind::Gain);
		schedule_envelope(
			graph,
			ParamId::new(envelope, Param::Gain),
			EnvelopeShape {
				start: voice_start,
				base: 0.0,
				peak: 1.0,
				attack: params.fm_attack.unwrap_or(0.0),
				decay: params.fm_decay.unwrap_or(0.0),
				sustain,
				release: params.fm_release.unwrap_or(0.0),
				// The modulator holds for the gate on its own sustain.
				gate_end: match params.duration {
					Some(duration) if sustain > 0.0 => Some(voice_start + duration),
					_ => None,
				},
				curve: params.fm_env_curve.unwrap_or(Curve::Exponential),
			},
		);
		graph.connect(depth, 0, Input::node(envelope));
		graph.connect(envelope, 0, frequency);
	} else {
		graph.connect(depth, 0, frequency);
	}
	modulator
}

/// `connectPhaser`: four allpass stages in series, their frequencies swept together by one LFO
/// through a shared depth gain. Returns the new tail.
fn connect_phaser<G: Graph>(
	graph: &mut G,
	input: NodeId,
	rate: f64,
	voice_start: f64,
	voice_end: f64,
	stoppables: &mut Vec<NodeId>,
) -> NodeId {
	let lfo = graph.node(NodeKind::Oscillator(Wave::Basic(OscillatorType::Sine)));
	graph.automate(
		ParamId::new(lfo, Param::Frequency),
		Automation::Set {
			value: rate,
			at: voice_start,
		},
	);
	let depth = graph.node(NodeKind::Gain);
	graph.automate(
		ParamId::new(depth, Param::Gain),
		Automation::Set {
			value: PHASER_DEPTH_HZ,
			at: voice_start,
		},
	);
	graph.connect(lfo, 0, Input::node(depth));
	let mut tail = input;
	for _ in 0..PHASER_STAGE_COUNT {
		let stage = graph.node(NodeKind::Biquad(FilterType::Allpass));
		let frequency = ParamId::new(stage, Param::Frequency);
		graph.automate(
			frequency,
			Automation::Set {
				value: PHASER_CENTER_HZ,
				at: voice_start,
			},
		);
		graph.connect(depth, 0, Input::Param(frequency));
		graph.connect(tail, 0, Input::node(stage));
		tail = stage;
	}
	graph.start(lfo, voice_start);
	graph.stop(lfo, voice_end);
	stoppables.push(lfo);
	tail
}

/// `foldToStereo`: approximately ITU down-mix coefficients for centre, sides, rears and the LFE.
fn fold_to_stereo(gains: &[f64]) -> Vec<f64> {
	let g = |index: usize| gains.get(index).copied().unwrap_or(0.0);
	let centre = std::f64::consts::FRAC_1_SQRT_2;
	let surround = std::f64::consts::FRAC_1_SQRT_2;
	let lfe = 0.5;
	vec![
		g(0) + centre * g(2) + lfe * g(3) + surround * (g(4) + g(6)),
		g(1) + centre * g(2) + lfe * g(3) + surround * (g(5) + g(7)),
	]
}

/// `foldChannelGains`: as they are when they fit, 7.1 rears into 5.1 sides on six channels, else
/// stereo.
fn fold_channel_gains(gains: Vec<f64>, available: u32) -> Vec<f64> {
	if gains.len() <= available as usize {
		return gains;
	}
	if available >= 6 {
		let g = |index: usize| gains.get(index).copied().unwrap_or(0.0);
		let mut five1 = vec![g(0), g(1), g(2), g(3), g(4), g(5)];
		five1[4] += std::f64::consts::FRAC_1_SQRT_2 * g(6);
		five1[5] += std::f64::consts::FRAC_1_SQRT_2 * g(7);
		return five1;
	}
	fold_to_stereo(&gains)
}

/// `connectOutput`: per-speaker gains into a merger when `channelGains` is set, else a stereo
/// panner when `pan` is, else straight to the output. Returns the voice's last node.
fn connect_output<G: Graph>(
	graph: &mut G,
	output: Output,
	gain_node: NodeId,
	params: &Params,
	voice_start: f64,
) -> NodeId {
	let mut placement = params
		.channel_gains
		.as_ref()
		.map(|gains| {
			gains
				.iter()
				.copied()
				.take(MAX_CHANNELS)
				.collect::<Vec<f64>>()
		})
		.filter(|gains| !gains.is_empty());
	if placement.is_none() {
		if let Some(pan) = params.pan {
			let panner = graph.node(NodeKind::StereoPanner);
			graph.automate(
				ParamId::new(panner, Param::Pan),
				Automation::Set {
					value: pan,
					at: voice_start,
				},
			);
			graph.connect(gain_node, 0, Input::node(panner));
			graph.connect(panner, 0, Input::node(output.node));
			return panner;
		}
		if output.channels <= 2 {
			graph.connect(gain_node, 0, Input::node(output.node));
			return gain_node;
		}
		// A centred stereo image, as the specification's mono-to-stereo up-mix gives.
		placement = Some(vec![1.0, 1.0]);
	}
	let mut placement = placement.unwrap_or_default();
	while placement.len() < 2 {
		placement.push(0.0);
	}
	let gains = fold_channel_gains(placement, output.channels);
	let merger = graph.node(NodeKind::Merger {
		inputs: gains.len() as u32,
	});
	for (channel, level) in gains.iter().enumerate() {
		if *level > 0.0 {
			let channel_gain = graph.node(NodeKind::Gain);
			graph.automate(
				ParamId::new(channel_gain, Param::Gain),
				Automation::Set {
					value: *level,
					at: voice_start,
				},
			);
			graph.connect(gain_node, 0, Input::node(channel_gain));
			graph.connect(
				channel_gain,
				0,
				Input::Node {
					node: merger,
					input: channel as u32,
				},
			);
		}
	}
	graph.connect(merger, 0, Input::node(output.node));
	merger
}

/// `connectSend`: a fresh send gain at `level` from the voice into `target`.
fn connect_send<G: Graph>(
	graph: &mut G,
	from: NodeId,
	target: NodeId,
	level: f64,
	voice_start: f64,
) {
	let send = graph.node(NodeKind::Gain);
	graph.automate(
		ParamId::new(send, Param::Gain),
		Automation::Set {
			value: level,
			at: voice_start,
		},
	);
	graph.connect(from, 0, Input::node(send));
	graph.connect(send, 0, Input::node(target));
}

/// `connectEffectSends`: the voice, after placement, into its orbit's reverb and delay when
/// `roomLevel` and `delayLevel` are above 0. No send, no bus.
fn connect_effect_sends<G: Graph>(
	graph: &mut G,
	booking: &mut Booking,
	output: NodeId,
	final_node: NodeId,
	params: &Params,
	voice_start: f64,
) {
	let room = params.room_level.filter(|level| *level > 0.0);
	let delay = params.delay_level.filter(|level| *level > 0.0);
	if room.is_none() && delay.is_none() {
		return;
	}
	let now = booking.now;
	let orbit = params.orbit.unwrap_or(DEFAULT_ORBIT);
	let Booking { seeds, buses, .. } = booking;
	let bus = buses.get(graph, orbit, output);
	if let Some(level) = room {
		bus.set_room_size(graph, seeds, params.room_size);
		let target = bus.reverb_input;
		connect_send(graph, final_node, target, level, voice_start);
	}
	if let Some(level) = delay {
		bus.set_delay(
			graph,
			params.delay_time,
			params.delay_feedback,
			math::max(voice_start, now),
		);
		let target = bus.delay_input;
		connect_send(graph, final_node, target, level, voice_start);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::log::LogGraph;
	use crate::params::{DEFAULT_PARAMS, Patch, resolve_params};
	use serde_json::json;

	fn params(value: serde_json::Value) -> Params {
		resolve_params(&serde_json::from_value::<Patch>(value).unwrap())
	}

	fn book(voices: &[(Params, f64)]) -> String {
		let mut graph = LogGraph::new(2, 48000, 48000.0);
		let mut booking = Booking::new(0.0);
		let output = Output {
			node: graph.destination(),
			channels: 2,
		};
		for (params, at) in voices {
			play_voice(&mut graph, &mut booking, params, *at, output).unwrap();
		}
		graph.describe().stable()
	}

	#[test]
	fn a_plain_voice_is_an_oscillator_into_its_envelope() {
		let text = book(&[(DEFAULT_PARAMS, 0.0)]);
		assert!(
			text.contains(
				"\"gain\": [\"set 0 @0\", \"lin 0.8 @0.01\", \"lin 0 @0.11\", \"lin 0 @0.16\"]"
			),
			"{text}"
		);
		assert!(text.contains("\"frequency\": [\"set 440 @0\"]"), "{text}");
		assert!(text.contains("\"stop\": [0.18]"), "{text}");
	}

	#[test]
	fn a_gate_holds_only_with_a_sustain() {
		let held = book(&[(params(json!({ "sustain": 0.5, "duration": 1 })), 0.0)]);
		assert!(held.contains("\"set 0.4 @1\""), "{held}");
		let percussive = book(&[(params(json!({ "duration": 1 })), 0.0)]);
		assert!(!percussive.contains("@1\""), "{percussive}");
	}

	#[test]
	fn a_closed_hat_chokes_the_open_one() {
		let open = params(json!({ "soundType": "white", "decay": 0.3, "cutGroup": 1 }));
		let closed = params(json!({ "soundType": "white", "decay": 0.04, "cutGroup": 1 }));
		let text = book(&[(open, 0.0), (closed, 0.1)]);
		assert!(text.contains("\"cancel @0.1\""), "{text}");
		assert!(text.contains("\"target 0 @0.1 tau 0.001\""), "{text}");
		assert!(text.contains("\"stop\": [0.38, 0.105]"), "{text}");
	}

	#[test]
	fn a_stop_after_the_end_does_nothing_and_samples_are_skipped() {
		let mut graph = LogGraph::new(2, 10, 48000.0);
		let mut booking = Booking::new(0.0);
		let output = Output {
			node: graph.destination(),
			channels: 2,
		};
		let handle = play_voice(&mut graph, &mut booking, &DEFAULT_PARAMS, 0.0, output).unwrap();
		let before = graph.describe();
		handle.stop(&mut graph, 5.0, None);
		assert_eq!(graph.describe(), before);
		let sample = params(json!({ "sampleName": "kick" }));
		play_voice(&mut graph, &mut booking, &sample, 0.0, output).unwrap();
		assert_eq!(graph.node_count(), before_nodes(&before));
	}

	fn before_nodes(graph: &crate::json::Json) -> usize {
		match graph {
			crate::json::Json::Object(entries) => {
				match &entries.iter().find(|(k, _)| k == "nodes").unwrap().1 {
					crate::json::Json::Array(nodes) => nodes.len(),
					_ => 0,
				}
			}
			_ => 0,
		}
	}

	#[test]
	fn an_envelope_reports_its_value_between_points() {
		let mut graph = LogGraph::new(2, 10, 48000.0);
		let envelope = schedule_envelope(
			&mut graph,
			ParamId::new(NodeId(0), Param::Gain),
			EnvelopeShape {
				start: 0.0,
				base: 0.0,
				peak: 1.0,
				attack: 0.1,
				decay: 0.1,
				sustain: 0.5,
				release: 0.2,
				gate_end: Some(0.5),
				curve: Curve::Linear,
			},
		);
		assert_eq!(envelope.value_at(0.05), 0.5);
		assert_eq!(envelope.value_at(0.3), 0.5);
		assert!((envelope.value_at(0.6) - 0.25).abs() < 1e-12);
		assert_eq!(envelope.value_at(1.0), 0.0);
		assert!((envelope.end - 0.7).abs() < 1e-12);
	}
}
