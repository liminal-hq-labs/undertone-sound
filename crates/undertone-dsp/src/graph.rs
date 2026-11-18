// The Web Audio graph Undertone books a voice on, as a small trait any renderer can implement
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The voice port (`voice`, `effects`, `master`) writes to a `Graph`: it makes nodes, connects
//! them, automates their AudioParams, and starts and stops the sources, in exactly the order the
//! TypeScript engine calls an `OfflineAudioContext`. `LogGraph` (`crate::log`) records the calls
//! so they can be compared with `fixtures/undertone/graph.json`; a renderer (the in-house DSP, or an
//! adapter over another Web Audio implementation) implements the same trait and plays them.
//!
//! Ids are indices in creation order, one sequence each for nodes, buffers and periodic waves, so a
//! recording numbers them as the TypeScript recorder does (`n0` is the destination). Times are
//! seconds on the context's clock, as `f64`. These are the only node kinds and AudioParam methods
//! Undertone uses.

/// A node, numbered in the order it was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

/// An audio buffer (a noise source's samples or a reverb's impulse response), in the order made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BufferId(pub usize);

/// A periodic wave (`createPeriodicWave`), in the order made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WaveId(pub usize);

/// An AudioParam by its Web Audio name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Param {
	/// A gain node's `gain`.
	Gain,
	/// An oscillator's or a biquad's `frequency`.
	Frequency,
	/// A biquad's `Q` (Undertone leaves it at 1).
	Q,
	/// An oscillator's, a biquad's or a buffer source's `detune`, in cents (Undertone leaves it
	/// at 0).
	Detune,
	/// A stereo panner's `pan`.
	Pan,
	/// A delay's `delayTime`.
	DelayTime,
	/// A buffer source's `playbackRate`.
	PlaybackRate,
	/// A compressor's `threshold`, `knee`, `ratio`, `attack` and `release`.
	Threshold,
	/// See [`Param::Threshold`].
	Knee,
	/// See [`Param::Threshold`].
	Ratio,
	/// See [`Param::Threshold`].
	Attack,
	/// See [`Param::Threshold`].
	Release,
}

impl Param {
	/// The name Web Audio gives the param.
	pub fn name(self) -> &'static str {
		match self {
			Param::Gain => "gain",
			Param::Frequency => "frequency",
			Param::Q => "Q",
			Param::Detune => "detune",
			Param::Pan => "pan",
			Param::DelayTime => "delayTime",
			Param::PlaybackRate => "playbackRate",
			Param::Threshold => "threshold",
			Param::Knee => "knee",
			Param::Ratio => "ratio",
			Param::Attack => "attack",
			Param::Release => "release",
		}
	}
}

/// One AudioParam of one node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamId {
	/// The node.
	pub node: NodeId,
	/// Which of its params.
	pub param: Param,
}

impl ParamId {
	/// The `param` of `node`.
	pub fn new(node: NodeId, param: Param) -> Self {
		ParamId { node, param }
	}
}

/// Where a connection ends: a node's input, or an AudioParam (whose value the signal is added to).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
	/// One input of a node.
	Node {
		/// The node.
		node: NodeId,
		/// Which of its inputs.
		input: u32,
	},
	/// An AudioParam.
	Param(ParamId),
}

impl Input {
	/// A node's first input.
	pub fn node(node: NodeId) -> Self {
		Input::Node { node, input: 0 }
	}
}

/// The four waveforms an oscillator has built in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OscillatorType {
	/// A sine wave.
	Sine,
	/// A square wave.
	Square,
	/// A sawtooth wave.
	Sawtooth,
	/// A triangle wave.
	Triangle,
}

impl OscillatorType {
	/// The name Web Audio gives the type.
	pub fn name(self) -> &'static str {
		match self {
			OscillatorType::Sine => "sine",
			OscillatorType::Square => "square",
			OscillatorType::Sawtooth => "sawtooth",
			OscillatorType::Triangle => "triangle",
		}
	}
}

/// An oscillator's waveform: a built-in type, or a periodic wave (Web Audio's `custom`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave {
	/// One of the built-in waveforms.
	Basic(OscillatorType),
	/// A periodic wave made with `create_periodic_wave`.
	Periodic(WaveId),
}

/// The biquad filter types Undertone uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
	/// Passes frequencies below the cutoff.
	Lowpass,
	/// Passes frequencies above the cutoff.
	Highpass,
	/// Passes every frequency and shifts phase around the cutoff.
	Allpass,
}

impl FilterType {
	/// The name Web Audio gives the type.
	pub fn name(self) -> &'static str {
		match self {
			FilterType::Lowpass => "lowpass",
			FilterType::Highpass => "highpass",
			FilterType::Allpass => "allpass",
		}
	}
}

/// A node to make, with what is fixed when it is made. Every param starts at Web Audio's default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
	/// `frequency` (440), `detune`.
	Oscillator(Wave),
	/// `playbackRate` (1), `detune`; `looped` repeats the buffer while the source plays.
	BufferSource {
		/// The samples it plays.
		buffer: BufferId,
		/// Whether it repeats the buffer.
		looped: bool,
	},
	/// `gain` (1).
	Gain,
	/// `frequency` (350), `Q` (1), `gain`, `detune`.
	Biquad(FilterType),
	/// `pan` (0); channel count mode `clamped-max`.
	StereoPanner,
	/// `delayTime` (0), up to `max_seconds`.
	Delay {
		/// The longest delay it can hold, in seconds.
		max_seconds: f64,
	},
	/// No buffer until `set_buffer`; channel count mode `clamped-max`.
	Convolver {
		/// Whether the impulse response is normalised when it is set.
		normalize: bool,
	},
	/// `threshold` (−24), `knee` (30), `ratio` (12), `attack` (0.003), `release` (0.25); `clamped-max`.
	Compressor,
	/// One mono input a channel, merged into one output.
	Merger {
		/// How many mono inputs it has.
		inputs: u32,
	},
}

/// One AudioParam call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Automation {
	/// `setValueAtTime(value, at)`.
	Set {
		/// The value to set.
		value: f64,
		/// When, in seconds.
		at: f64,
	},
	/// `linearRampToValueAtTime(value, until)`.
	Linear {
		/// The value to reach.
		value: f64,
		/// When it is reached, in seconds.
		until: f64,
	},
	/// `exponentialRampToValueAtTime(value, until)`.
	Exponential {
		/// The value to reach.
		value: f64,
		/// When it is reached, in seconds.
		until: f64,
	},
	/// `setTargetAtTime(value, from, time_constant)`.
	Target {
		/// The value to approach.
		value: f64,
		/// When the approach starts, in seconds.
		from: f64,
		/// How quickly it approaches, in seconds.
		time_constant: f64,
	},
	/// `cancelScheduledValues(from)`.
	Cancel {
		/// Calls scheduled at or after this time, in seconds, are dropped.
		from: f64,
	},
}

/// Samples to make a buffer from: one `f32` vector a channel, all the same length.
#[derive(Debug, Clone, PartialEq)]
pub struct Buffer {
	/// Frames a second.
	pub sample_rate: f64,
	/// The samples, one vector a channel.
	pub channels: Vec<Vec<f32>>,
}

impl Buffer {
	/// Frames in the buffer.
	pub fn len(&self) -> usize {
		self.channels.first().map_or(0, Vec::len)
	}

	/// Whether the buffer has no frames.
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}
}

/// The calls Undertone makes on an audio context, in the order it makes them.
pub trait Graph {
	/// The context's sample rate, which noise and impulse responses are generated at.
	fn sample_rate(&self) -> f64;
	/// The destination node, `n0`.
	fn destination(&self) -> NodeId;
	/// Makes a node.
	fn node(&mut self, kind: NodeKind) -> NodeId;
	/// Calls one AudioParam method.
	fn automate(&mut self, param: ParamId, step: Automation);
	/// Connects a node's output to a node's input or an AudioParam.
	fn connect(&mut self, from: NodeId, output: u32, to: Input);
	/// Gives a convolver its impulse response, replacing any it had.
	fn set_buffer(&mut self, convolver: NodeId, buffer: BufferId);
	/// Starts a source.
	fn start(&mut self, node: NodeId, at: f64);
	/// Stops a source; a later call replaces the time an earlier one set.
	fn stop(&mut self, node: NodeId, at: f64);
	/// Makes a buffer.
	fn buffer(&mut self, buffer: Buffer) -> BufferId;
	/// Makes a periodic wave from its Fourier coefficients.
	fn periodic_wave(
		&mut self,
		real: Vec<f32>,
		imag: Vec<f32>,
		disable_normalization: bool,
	) -> WaveId;
	/// Says that the nodes made from now on play the performance event `source`. Not a Web Audio
	/// call: a renderer may use it to render some tracks alone; a recording ignores it.
	fn voice(&mut self, _source: usize) {}
}
