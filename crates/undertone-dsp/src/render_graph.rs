// The in-house offline renderer: a Graph that records the booking, then computes it in quanta
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `RenderGraph` takes the same calls as `LogGraph` and keeps them, and `render` plays them as an
//! `OfflineAudioContext` would: in render quanta of 128 frames, every node after the nodes it
//! hears, with the kernels in `crate::dsp`.
//!
//! - **Order.** Nodes are processed in a topological order of their connections (AudioParam
//!   inputs included). A `DelayNode` inside a cycle (Undertone's echo feedback) is split as the
//!   specification splits it: it is read early in the quantum and written at the end, so the
//!   cycle has a delay of at least one quantum. A cycle without a delay is refused.
//! - **Only what sounds.** Each node gets a window of frames it can sound in: a source from its
//!   start to its stop, a node from the first to the last frame any of its inputs can sound (a
//!   biquad 0.1 s longer, for its ring), and the shared nodes (delays, convolvers, the
//!   compressor and what follows them) the whole render. A node is made when its window opens and
//!   dropped when it closes, so a long score costs the voices sounding in each quantum, not every
//!   voice booked. A node whose inputs are all silent in a quantum passes silence on cheaply.
//! - **Channels.** Each input mixes its connections as `channelCountMode` and the speaker rules
//!   say (gain, biquad and delay `max`; panner, convolver and compressor `clamped-max` 2; each
//!   merger input `explicit` 1; the destination `explicit` at its channels).
//! - **Tags.** `set_tag` marks the nodes made after it; `render_where` renders with the sources of
//!   some tags left out, which is how one booking renders a track alone with every other decision
//!   (room sizes, seeds, cut groups) kept as the mix made it.
//!
//! - **Streaming.** `Engine` is the booking prepared to play: every allocation happens when it is
//!   made, and `process` fills blocks of any size without allocating, so the offline render is a
//!   loop over it and a live output could call it from an audio thread.
//!
//! The same booking renders to the same bits every time, in any block size.

// Frame and bin loops index several buffers at once; the indices read as the formulas do.
#![allow(clippy::needless_range_loop)]

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::dsp::biquad::{self, Coefficients};
use crate::dsp::compressor::{Compressor, Settings};
use crate::dsp::convolver::Convolver;
use crate::dsp::delay::DelayLine;
use crate::dsp::oscillator::WaveTables;
use crate::dsp::panner::pan_frame;
use crate::dsp::timeline::{Curve, Timeline};
use crate::dsp::{Block, ChannelMode, MAX_CHANNELS, QUANTUM};
use crate::error::Error;
use crate::graph::{
	Automation, Buffer, BufferId, FilterType, Graph, Input, NodeId, NodeKind, OscillatorType,
	Param, ParamId, Wave, WaveId,
};

/// How long a biquad keeps ringing after its input has stopped.
const BIQUAD_TAIL_SECONDS: f64 = 0.1;
/// The detune range in cents, `±1200·log2(FLT_MAX)`.
const DETUNE_LIMIT: f32 = 153_600.0;
const NO_SLOT: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
	Destination,
	Node(NodeKind),
}

#[derive(Debug, Clone)]
struct Record {
	kind: Kind,
	params: Vec<(Param, Timeline)>,
	/// Connections into the node: the node they come from and the input they reach.
	inputs: Vec<(usize, u32)>,
	/// Connections into its AudioParams.
	param_inputs: Vec<(Param, usize)>,
	start: Option<f64>,
	stop: Option<f64>,
	buffer: Option<BufferId>,
	tag: Option<usize>,
}

impl Record {
	fn new(kind: Kind, tag: Option<usize>) -> Self {
		Record {
			kind,
			params: Vec::new(),
			inputs: Vec::new(),
			param_inputs: Vec::new(),
			start: None,
			stop: None,
			buffer: None,
			tag,
		}
	}

	fn timeline(&self, param: Param) -> Option<&Timeline> {
		self.params
			.iter()
			.find(|(p, _)| *p == param)
			.map(|(_, timeline)| timeline)
	}
}

#[derive(Debug, Clone)]
struct PeriodicRecord {
	real: Vec<f32>,
	imag: Vec<f32>,
	disable_normalization: bool,
}

/// A booked graph that renders to samples.
#[derive(Debug, Clone)]
pub struct RenderGraph {
	sample_rate: f64,
	channels: u32,
	length: usize,
	records: Vec<Record>,
	buffers: Vec<Buffer>,
	waves: Vec<PeriodicRecord>,
	tag: Option<usize>,
}

impl RenderGraph {
	/// An empty context of `length` frames and `channels` output channels (1 or 2).
	pub fn new(channels: u32, length: usize, sample_rate: f64) -> Self {
		RenderGraph {
			sample_rate,
			channels,
			length,
			records: vec![Record::new(Kind::Destination, None)],
			buffers: Vec::new(),
			waves: Vec::new(),
			tag: None,
		}
	}

	/// Frames the context renders.
	pub fn length(&self) -> usize {
		self.length
	}

	/// Output channels.
	pub fn channels(&self) -> u32 {
		self.channels
	}

	/// Nodes made so far, the destination included.
	pub fn node_count(&self) -> usize {
		self.records.len()
	}

	/// Marks the nodes made from now on with `tag` (`None` for none).
	pub fn set_tag(&mut self, tag: Option<usize>) {
		self.tag = tag;
	}

	/// Renders every source: one `Vec` a channel, `length` frames each.
	pub fn render(&self) -> Result<Vec<Vec<f32>>, Error> {
		self.render_where(|_| true)
	}

	/// The graph prepared for streaming, with every source.
	pub fn engine(self) -> Result<Engine, Error> {
		Engine::new(Arc::new(self))
	}

	/// Renders with only the sources whose tag `keep` accepts; untagged sources are asked about
	/// as `None`.
	pub fn render_where(
		&self,
		keep: impl Fn(Option<usize>) -> bool,
	) -> Result<Vec<Vec<f32>>, Error> {
		Ok(Engine::with(Arc::new(self.clone()), keep, Limits::DEFAULT)?.render_all())
	}

	fn record_mut(&mut self, node: NodeId) -> &mut Record {
		&mut self.records[node.0]
	}
}

impl Graph for RenderGraph {
	fn sample_rate(&self) -> f64 {
		self.sample_rate
	}

	fn voice(&mut self, source: usize) {
		self.set_tag(Some(source));
	}

	fn destination(&self) -> NodeId {
		NodeId(0)
	}

	fn node(&mut self, kind: NodeKind) -> NodeId {
		let mut record = Record::new(Kind::Node(kind), self.tag);
		if let NodeKind::BufferSource { buffer, .. } = kind {
			record.buffer = Some(buffer);
		}
		self.records.push(record);
		NodeId(self.records.len() - 1)
	}

	fn automate(&mut self, param: ParamId, step: Automation) {
		let record = self.record_mut(param.node);
		match record.params.iter_mut().find(|(p, _)| *p == param.param) {
			Some((_, timeline)) => timeline.push(step),
			None => {
				let mut timeline = Timeline::default();
				timeline.push(step);
				record.params.push((param.param, timeline));
			}
		}
	}

	fn connect(&mut self, from: NodeId, _output: u32, to: Input) {
		match to {
			Input::Node { node, input } => {
				let inputs = &mut self.record_mut(node).inputs;
				// Connecting the same output to the same input again changes nothing.
				if !inputs.contains(&(from.0, input)) {
					inputs.push((from.0, input));
				}
			}
			Input::Param(param) => {
				let inputs = &mut self.record_mut(param.node).param_inputs;
				if !inputs.contains(&(param.param, from.0)) {
					inputs.push((param.param, from.0));
				}
			}
		}
	}

	fn set_buffer(&mut self, convolver: NodeId, buffer: BufferId) {
		self.record_mut(convolver).buffer = Some(buffer);
	}

	fn start(&mut self, node: NodeId, at: f64) {
		self.record_mut(node).start = Some(at);
	}

	fn stop(&mut self, node: NodeId, at: f64) {
		self.record_mut(node).stop = Some(at);
	}

	fn buffer(&mut self, buffer: Buffer) -> BufferId {
		self.buffers.push(buffer);
		BufferId(self.buffers.len() - 1)
	}

	fn periodic_wave(
		&mut self,
		real: Vec<f32>,
		imag: Vec<f32>,
		disable_normalization: bool,
	) -> WaveId {
		self.waves.push(PeriodicRecord {
			real,
			imag,
			disable_normalization,
		});
		WaveId(self.waves.len() - 1)
	}
}

/// An AudioParam as a render reads it: its curve and the nodes connected to it.
#[derive(Debug, Clone)]
struct ParamSlot {
	curve: Curve,
	inputs: Vec<usize>,
	min: f32,
	max: f32,
}

impl ParamSlot {
	fn new(record: &Record, param: Param, default: f64, min: f32, max: f32) -> Self {
		ParamSlot {
			curve: record
				.timeline(param)
				.map_or_else(|| Curve::constant(default), |t| t.curve(default)),
			inputs: record
				.param_inputs
				.iter()
				.filter(|(p, _)| *p == param)
				.map(|(_, from)| *from)
				.collect(),
			min,
			max,
		}
	}

	/// The values for the quantum at `first`, clamped to the nominal range: one value when it holds
	/// for the whole quantum, else `out`.
	fn values(
		&mut self,
		first: usize,
		rate: f64,
		io: &Io,
		out: &mut [f32; QUANTUM],
	) -> Option<f32> {
		let held = self.curve.fill(first, rate, out);
		let mut sum = Block::silent();
		for from in &self.inputs {
			if let Some(block) = io.output(*from) {
				if sum.is_silent() {
					sum.zero(1);
				}
				sum.add(block);
			}
		}
		if sum.is_silent() {
			return held.map(|v| v.clamp(self.min, self.max)).or_else(|| {
				for v in out.iter_mut() {
					*v = v.clamp(self.min, self.max);
				}
				None
			});
		}
		for (i, v) in out.iter_mut().enumerate() {
			let intrinsic = held.unwrap_or(*v);
			*v = (intrinsic + sum.data[0][i]).clamp(self.min, self.max);
		}
		None
	}

	/// The value at the start of the quantum (a k-rate param).
	fn k_rate(&mut self, first: usize, rate: f64, io: &Io) -> f32 {
		let mut out = [0f32; QUANTUM];
		self.values(first, rate, io, &mut out).unwrap_or(out[0])
	}
}

/// The outputs of the nodes processed so far this quantum.
struct Io<'a> {
	slots: &'a [Block],
	slot_of: &'a [usize],
}

impl Io<'_> {
	fn output(&self, node: usize) -> Option<&Block> {
		let slot = self.slot_of[node];
		if slot == NO_SLOT {
			return None;
		}
		let block = &self.slots[slot];
		(!block.is_silent()).then_some(block)
	}

	/// One input's connections mixed as `mode` says.
	fn mix(&self, record: &Record, input: u32, mode: ChannelMode) -> Block {
		let mut widest = 0;
		for (from, to) in &record.inputs {
			if *to == input
				&& let Some(block) = self.output(*from)
			{
				widest = widest.max(block.channels);
			}
		}
		let mut mix = Block::silent();
		let channels = mode.computed(widest);
		if channels == 0 {
			return mix;
		}
		mix.zero(channels);
		for (from, to) in &record.inputs {
			if *to == input
				&& let Some(block) = self.output(*from)
			{
				mix.add(block);
			}
		}
		mix
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WaveKey {
	Basic(OscillatorType),
	Periodic(usize),
}

/// What a node keeps between quanta.
enum State {
	Destination,
	/// A node that never sounds in this render.
	Idle,
	Oscillator {
		tables: Arc<WaveTables>,
		begin: usize,
		end: usize,
		start: f64,
		phase: f64,
		started: bool,
		frequency: ParamSlot,
		detune: ParamSlot,
		last: Option<(f64, (usize, usize, f32))>,
	},
	Source {
		buffer: usize,
		looped: bool,
		begin: usize,
		end: usize,
		start: f64,
		position: f64,
		started: bool,
		rate: ParamSlot,
		detune: ParamSlot,
	},
	Gain {
		gain: ParamSlot,
	},
	Biquad {
		kind: FilterType,
		channels: usize,
		memory: [biquad::State; MAX_CHANNELS],
		frequency: ParamSlot,
		q: ParamSlot,
		detune: ParamSlot,
		last: Option<(f32, f32, Coefficients)>,
	},
	Panner {
		pan: ParamSlot,
	},
	Delay {
		line: DelayLine,
		time: ParamSlot,
		in_cycle: bool,
	},
	Convolver(Option<Box<Convolver>>),
	Compressor {
		compressor: Box<Compressor>,
		threshold: ParamSlot,
		knee: ParamSlot,
		ratio: ParamSlot,
		attack: ParamSlot,
		release: ParamSlot,
	},
	Merger {
		inputs: u32,
	},
}

/// What an `Engine` may hold at once, checked when it is prepared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
	/// The most nodes sounding in one render quantum (a voice is 3 to 15 nodes; the shared buses
	/// are always counted). Every slot is allocated when the engine is prepared.
	pub max_active_nodes: usize,
}

impl Limits {
	/// 16,384 nodes: about a thousand of Undertone's voices sounding at once.
	pub const DEFAULT: Limits = Limits {
		max_active_nodes: 16_384,
	};
}

impl Default for Limits {
	fn default() -> Self {
		Limits::DEFAULT
	}
}

/// A booked graph prepared for streaming: call `process` for blocks of any size.
///
/// Everything is allocated in `new`: every node's state (oscillator tables, delay lines,
/// convolver partitions), the output slots for the most nodes that ever sound at once, and the
/// active list. `process` then neither allocates, frees, locks nor panics, and renders in render
/// quanta of 128 frames whatever the caller's block size, keeping the rest of a quantum for the
/// next call, so the output is the same bits for any block size. Past the context's length it
/// plays silence.
pub struct Engine {
	graph: Arc<RenderGraph>,
	rate: f64,
	position: Vec<usize>,
	in_cycle: Vec<bool>,
	windows: Vec<(usize, usize)>,
	/// Nodes with a window, by when it opens.
	opening: Vec<usize>,
	next: usize,
	active: Vec<usize>,
	slot_of: Vec<usize>,
	slots: Vec<Block>,
	free: Vec<usize>,
	states: Vec<State>,
	peak_active: usize,
	/// The next quantum to render.
	quantum: usize,
	/// The quantum rendered last, and how much of it has been handed out.
	ready: Block,
	handed: usize,
	scratch: Block,
}

fn frame_at(time: f64, rate: f64) -> usize {
	let frame = (time * rate).ceil();
	if frame <= 0.0 { 0 } else { frame as usize }
}

impl Engine {
	/// Prepares `graph` with every source, within `Limits::DEFAULT`.
	pub fn new(graph: Arc<RenderGraph>) -> Result<Self, Error> {
		Engine::with(graph, |_| true, Limits::DEFAULT)
	}

	/// Prepares `graph` with only the sources whose tag `keep` accepts (untagged ones are asked
	/// about as `None`), refusing a booking that needs more than `limits`.
	pub fn with(
		graph: Arc<RenderGraph>,
		keep: impl Fn(Option<usize>) -> bool,
		limits: Limits,
	) -> Result<Self, Error> {
		let count = graph.records.len();
		let rate = graph.sample_rate;
		if !matches!(graph.channels, 1 | 2) {
			return Err(Error::new(format!(
				"the renderer makes 1 or 2 channels, not {}",
				graph.channels
			)));
		}
		for record in &graph.records {
			if let Kind::Node(NodeKind::Merger { inputs }) = record.kind
				&& inputs as usize > MAX_CHANNELS
			{
				return Err(Error::new(format!(
					"a channel merger of {inputs} inputs is more than the renderer's {MAX_CHANNELS} channels"
				)));
			}
		}
		for buffer in &graph.buffers {
			if buffer.channels.is_empty() || buffer.channels.len() > MAX_CHANNELS {
				return Err(Error::new(format!(
					"a buffer of {} channels; the renderer takes 1 or 2",
					buffer.channels.len()
				)));
			}
		}
		let is_delay =
			|n: usize| matches!(graph.records[n].kind, Kind::Node(NodeKind::Delay { .. }));
		// Who hears whom.
		let mut hearers: Vec<Vec<usize>> = vec![Vec::new(); count];
		for (to, record) in graph.records.iter().enumerate() {
			for (from, _) in &record.inputs {
				hearers[*from].push(to);
			}
			for (_, from) in &record.param_inputs {
				hearers[*from].push(to);
			}
		}
		// A delay in a cycle reaches itself.
		let mut in_cycle = vec![false; count];
		for (d, flag) in in_cycle.iter_mut().enumerate() {
			if !is_delay(d) {
				continue;
			}
			let mut seen = vec![false; count];
			let mut stack: Vec<usize> = hearers[d].clone();
			while let Some(n) = stack.pop() {
				if n == d {
					*flag = true;
					break;
				}
				if !seen[n] {
					seen[n] = true;
					stack.extend(&hearers[n]);
				}
			}
		}
		// Kahn's order, with the edges into a cycle's delay left out.
		let mut waiting = vec![0usize; count];
		let mut after: Vec<Vec<usize>> = vec![Vec::new(); count];
		for (to, record) in graph.records.iter().enumerate() {
			let mut from_nodes: Vec<usize> = Vec::new();
			if !in_cycle[to] {
				from_nodes.extend(record.inputs.iter().map(|(from, _)| *from));
			}
			from_nodes.extend(record.param_inputs.iter().map(|(_, from)| *from));
			for from in from_nodes {
				after[from].push(to);
				waiting[to] += 1;
			}
		}
		let mut ready: VecDeque<usize> = (0..count).filter(|n| waiting[*n] == 0).collect();
		let mut order = Vec::with_capacity(count);
		while let Some(n) = ready.pop_front() {
			order.push(n);
			for &to in &after[n] {
				waiting[to] -= 1;
				if waiting[to] == 0 {
					ready.push_back(to);
				}
			}
		}
		if order.len() < count {
			return Err(Error::new(
				"the graph has a cycle with no delay in it, which Web Audio renders silent",
			));
		}
		let mut position = vec![0; count];
		for (i, n) in order.iter().enumerate() {
			position[*n] = i;
		}
		// Windows, in order, so a node's inputs are known before it.
		let length = graph.length;
		let mut windows = vec![(0usize, 0usize); count];
		let tail = (BIQUAD_TAIL_SECONDS * rate).ceil() as usize;
		for &n in &order {
			let record = &graph.records[n];
			let window = match record.kind {
				Kind::Destination | Kind::Node(NodeKind::Compressor) => (0, length),
				Kind::Node(NodeKind::Delay { .. } | NodeKind::Convolver { .. }) => {
					if record.inputs.is_empty() {
						(0, 0)
					} else {
						(0, length)
					}
				}
				Kind::Node(NodeKind::Oscillator(_) | NodeKind::BufferSource { .. }) => {
					match record.start {
						Some(start) if keep(record.tag) => {
							let begin = frame_at(start, rate);
							let end = record.stop.map_or(length, |stop| frame_at(stop, rate));
							(begin.min(length), end.min(length))
						}
						_ => (0, 0),
					}
				}
				Kind::Node(kind) => {
					let mut hull: Option<(usize, usize)> = None;
					for (from, _) in &record.inputs {
						let (b, e) = windows[*from];
						if b < e {
							hull = Some(hull.map_or((b, e), |(hb, he)| (hb.min(b), he.max(e))));
						}
					}
					match hull {
						Some((b, e)) if matches!(kind, NodeKind::Biquad(_)) => {
							(b, (e + tail).min(length))
						}
						Some(window) => window,
						None => (0, 0),
					}
				}
			};
			windows[n] = window;
		}
		let mut opening: Vec<usize> = (0..count)
			.filter(|n| windows[*n].0 < windows[*n].1)
			.collect();
		opening.sort_by_key(|n| (windows[*n].0, position[*n]));
		// The most nodes active in one quantum: a node is active in quanta begin/128 through
		// (end - 1)/128.
		let quanta = length.div_ceil(QUANTUM).max(1);
		let mut change = vec![0i64; quanta + 1];
		for &n in &opening {
			let (b, e) = windows[n];
			change[b / QUANTUM] += 1;
			change[(e - 1) / QUANTUM + 1] -= 1;
		}
		let mut running = 0i64;
		let mut peak_active = 0usize;
		for delta in &change {
			running += delta;
			peak_active = peak_active.max(running as usize);
		}
		if peak_active > limits.max_active_nodes {
			return Err(Error::new(format!(
				"the booking sounds {peak_active} nodes at once, over the engine's limit of {}",
				limits.max_active_nodes
			)));
		}
		let mut tables: HashMap<WaveKey, Arc<WaveTables>> = HashMap::new();
		let mut states = Vec::with_capacity(count);
		for n in 0..count {
			states.push(state_for(&graph, n, windows[n], in_cycle[n], &mut tables));
		}
		let mut ready = Block::silent();
		ready.zero(graph.channels as usize);
		Ok(Engine {
			graph,
			rate,
			position,
			in_cycle,
			windows,
			opening,
			next: 0,
			active: Vec::with_capacity(peak_active),
			slot_of: vec![NO_SLOT; count],
			slots: vec![Block::silent(); peak_active],
			free: (0..peak_active).rev().collect(),
			states,
			peak_active,
			quantum: 0,
			ready,
			handed: QUANTUM,
			scratch: Block::silent(),
		})
	}

	/// Output channels: 1 or 2.
	pub fn channels(&self) -> usize {
		self.graph.channels as usize
	}

	/// Frames of the context; `process` plays silence after them.
	pub fn length(&self) -> usize {
		self.graph.length
	}

	/// The most nodes that sound in one quantum of this booking.
	pub fn peak_active(&self) -> usize {
		self.peak_active
	}

	/// Fills the next `left.len()` frames (and `right`, of the same length; a mono engine copies
	/// its channel to both).
	pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
		let frames = left.len().min(right.len());
		let mut done = 0;
		while done < frames {
			if self.handed == QUANTUM {
				self.render_quantum();
			}
			let take = (QUANTUM - self.handed).min(frames - done);
			let from = self.handed;
			let stereo = self.ready.channels.min(2).saturating_sub(1);
			left[done..done + take].copy_from_slice(&self.ready.data[0][from..from + take]);
			right[done..done + take].copy_from_slice(&self.ready.data[stereo][from..from + take]);
			self.handed += take;
			done += take;
		}
	}

	/// Fills `out` with interleaved frames of `channels()` channels.
	pub fn process_interleaved(&mut self, out: &mut [f32]) {
		let channels = self.channels();
		let frames = out.len() / channels;
		let mut done = 0;
		while done < frames {
			if self.handed == QUANTUM {
				self.render_quantum();
			}
			let take = (QUANTUM - self.handed).min(frames - done);
			for i in 0..take {
				for c in 0..channels {
					out[(done + i) * channels + c] = self.ready.data[c][self.handed + i];
				}
			}
			self.handed += take;
			done += take;
		}
	}

	/// Renders the whole context, one `Vec` a channel.
	pub fn render_all(mut self) -> Vec<Vec<f32>> {
		let length = self.length();
		let mut left = vec![0f32; length];
		let mut right = vec![0f32; length];
		self.process(&mut left, &mut right);
		if self.channels() == 1 {
			vec![left]
		} else {
			vec![left, right]
		}
	}

	fn render_quantum(&mut self) {
		self.handed = 0;
		let channels = self.graph.channels as usize;
		self.ready.zero(channels);
		let first = self.quantum * QUANTUM;
		self.quantum += 1;
		if first >= self.graph.length {
			return;
		}
		let mut changed = false;
		while self.next < self.opening.len()
			&& self.windows[self.opening[self.next]].0 < first + QUANTUM
		{
			let n = self.opening[self.next];
			self.next += 1;
			// The peak was counted when the engine was prepared, so a slot is always free.
			let Some(slot) = self.free.pop() else {
				continue;
			};
			self.slots[slot].channels = 0;
			self.slot_of[n] = slot;
			self.active.push(n);
			changed = true;
		}
		let (windows, free, slot_of) = (&self.windows, &mut self.free, &mut self.slot_of);
		self.active.retain(|&n| {
			if windows[n].1 <= first {
				free.push(slot_of[n]);
				slot_of[n] = NO_SLOT;
				false
			} else {
				true
			}
		});
		if changed {
			let position = &self.position;
			self.active.sort_unstable_by_key(|n| position[*n]);
		}
		for index in 0..self.active.len() {
			let n = self.active[index];
			{
				let io = Io {
					slots: &self.slots,
					slot_of: &self.slot_of,
				};
				process(
					&mut self.states[n],
					&self.graph.records[n],
					&self.graph.buffers,
					&io,
					first,
					self.rate,
					&mut self.scratch,
					&mut self.ready,
				);
			}
			let slot = self.slot_of[n];
			let target = &mut self.slots[slot];
			target.channels = self.scratch.channels;
			for c in 0..self.scratch.channels {
				target.data[c] = self.scratch.data[c];
			}
		}
		// The cycles' delays take their input last.
		for index in 0..self.active.len() {
			let n = self.active[index];
			if !self.in_cycle[n] {
				continue;
			}
			let input = Io {
				slots: &self.slots,
				slot_of: &self.slot_of,
			}
			.mix(&self.graph.records[n], 0, ChannelMode::Max);
			if let State::Delay { line, .. } = &mut self.states[n] {
				line.write(first, &input);
			}
		}
		// The frames past the end of the context are silence.
		let left = self.graph.length - first;
		if left < QUANTUM {
			for c in 0..channels {
				self.ready.data[c][left..].fill(0.0);
			}
		}
	}
}

fn state_for(
	graph: &RenderGraph,
	n: usize,
	(begin, end): (usize, usize),
	in_cycle: bool,
	tables: &mut HashMap<WaveKey, Arc<WaveTables>>,
) -> State {
	let record = &graph.records[n];
	let rate = graph.sample_rate;
	let nyquist = (rate / 2.0) as f32;
	let mut table = |key: WaveKey| {
		tables
			.entry(key)
			.or_insert_with(|| {
				Arc::new(match key {
					WaveKey::Basic(kind) => WaveTables::basic(kind, rate),
					WaveKey::Periodic(index) => {
						let wave = &graph.waves[index];
						WaveTables::new(&wave.real, &wave.imag, wave.disable_normalization, rate)
					}
				})
			})
			.clone()
	};
	let unused = begin >= end;
	match record.kind {
		Kind::Destination => State::Destination,
		// A node that never sounds keeps no memory.
		Kind::Node(_) if unused => State::Idle,
		Kind::Node(kind) => match kind {
			NodeKind::Oscillator(wave) => {
				let key = match wave {
					Wave::Basic(kind) => WaveKey::Basic(kind),
					Wave::Periodic(id) => WaveKey::Periodic(id.0),
				};
				State::Oscillator {
					tables: table(key),
					begin,
					end,
					start: record.start.unwrap_or(0.0),
					phase: 0.0,
					started: false,
					frequency: ParamSlot::new(record, Param::Frequency, 440.0, -nyquist, nyquist),
					detune: ParamSlot::new(record, Param::Detune, 0.0, -DETUNE_LIMIT, DETUNE_LIMIT),
					last: None,
				}
			}
			NodeKind::BufferSource { buffer, looped } => State::Source {
				buffer: buffer.0,
				looped,
				begin,
				end,
				start: record.start.unwrap_or(0.0),
				position: 0.0,
				started: false,
				rate: ParamSlot::new(record, Param::PlaybackRate, 1.0, f32::MIN, f32::MAX),
				detune: ParamSlot::new(record, Param::Detune, 0.0, f32::MIN, f32::MAX),
			},
			NodeKind::Gain => State::Gain {
				gain: ParamSlot::new(record, Param::Gain, 1.0, f32::MIN, f32::MAX),
			},
			NodeKind::Biquad(kind) => State::Biquad {
				kind,
				channels: 1,
				memory: [biquad::State::default(); MAX_CHANNELS],
				frequency: ParamSlot::new(record, Param::Frequency, 350.0, 0.0, nyquist),
				q: ParamSlot::new(record, Param::Q, 1.0, f32::MIN, f32::MAX),
				detune: ParamSlot::new(record, Param::Detune, 0.0, -DETUNE_LIMIT, DETUNE_LIMIT),
				last: None,
			},
			NodeKind::StereoPanner => State::Panner {
				pan: ParamSlot::new(record, Param::Pan, 0.0, -1.0, 1.0),
			},
			NodeKind::Delay { max_seconds } => {
				let min = if in_cycle {
					(QUANTUM as f64 / rate) as f32
				} else {
					0.0
				};
				State::Delay {
					line: DelayLine::new(max_seconds, rate),
					time: ParamSlot::new(
						record,
						Param::DelayTime,
						0.0,
						min,
						(max_seconds as f32).max(min),
					),
					in_cycle,
				}
			}
			NodeKind::Convolver { normalize } => State::Convolver(
				record
					.buffer
					.map(|buffer| Box::new(Convolver::new(&graph.buffers[buffer.0], normalize))),
			),
			NodeKind::Compressor => State::Compressor {
				compressor: Box::new(Compressor::new(rate)),
				threshold: ParamSlot::new(record, Param::Threshold, -24.0, -100.0, 0.0),
				knee: ParamSlot::new(record, Param::Knee, 30.0, 0.0, 40.0),
				ratio: ParamSlot::new(record, Param::Ratio, 12.0, 1.0, 20.0),
				attack: ParamSlot::new(record, Param::Attack, 0.003, 0.0, 1.0),
				release: ParamSlot::new(record, Param::Release, 0.25, 0.0, 1.0),
			},
			NodeKind::Merger { inputs } => State::Merger { inputs },
		},
	}
}

/// Renders one quantum of one node into `out`.
#[allow(clippy::too_many_arguments)]
fn process(
	state: &mut State,
	record: &Record,
	buffers: &[Buffer],
	io: &Io,
	first: usize,
	rate: f64,
	out: &mut Block,
	destination: &mut Block,
) {
	out.channels = 0;
	let mut values = [0f32; QUANTUM];
	match state {
		State::Idle => {}
		State::Destination => {
			let channels = destination.channels;
			let mix = io.mix(record, 0, ChannelMode::Explicit(channels));
			if mix.is_silent() {
				return;
			}
			for c in 0..channels {
				destination.data[c] = mix.data[c];
			}
		}
		State::Oscillator {
			tables,
			begin,
			end,
			start,
			phase,
			started,
			frequency,
			detune,
			last,
		} => {
			let mut detunes = [0f32; QUANTUM];
			let held = frequency.values(first, rate, io, &mut values);
			let held_detune = detune.values(first, rate, io, &mut detunes);
			out.zero(1);
			for i in 0..QUANTUM {
				let n = first + i;
				if n < *begin || n >= *end {
					continue;
				}
				let f = f64::from(held.unwrap_or(values[i]));
				let d = held_detune.unwrap_or(detunes[i]);
				let f = if d == 0.0 {
					f
				} else {
					f * 2f64.powf(f64::from(d) / 1200.0)
				};
				if !*started {
					// A start between frames begins part of a cycle in.
					*phase = (f * (n as f64 / rate - *start)).rem_euclid(1.0);
					*started = true;
				}
				let pick = match *last {
					Some((lf, pick)) if lf == f => pick,
					_ => {
						let pick = tables.pick(f);
						*last = Some((f, pick));
						pick
					}
				};
				out.data[0][i] = tables.sample(*phase, pick);
				*phase += f / rate;
				*phase -= phase.floor();
			}
		}
		State::Source {
			buffer,
			looped,
			begin,
			end,
			start,
			position,
			started,
			rate: playback,
			detune,
		} => {
			let buffer = &buffers[*buffer];
			let len = buffer.len();
			let speed = f64::from(playback.k_rate(first, rate, io))
				* 2f64.powf(f64::from(detune.k_rate(first, rate, io)) / 1200.0)
				* (buffer.sample_rate / rate);
			let channels = buffer.channels.len();
			out.zero(channels);
			for i in 0..QUANTUM {
				let n = first + i;
				if n < *begin || n >= *end {
					continue;
				}
				if !*started {
					*position = (n as f64 - *start * rate) * speed;
					*started = true;
				}
				let mut at = *position;
				if *looped && len > 0 {
					at = at.rem_euclid(len as f64);
				} else if at < 0.0 || at >= len as f64 {
					*position += speed;
					continue;
				}
				let index = (at as usize).min(len - 1);
				let frac = (at - index as f64) as f32;
				let next = if index + 1 < len {
					Some(index + 1)
				} else if *looped {
					Some(0)
				} else {
					None
				};
				for c in 0..channels {
					let data = &buffer.channels[c];
					let a = data[index];
					let b = next.map_or(a, |j| data[j]);
					out.data[c][i] = if frac == 0.0 { a } else { a + (b - a) * frac };
				}
				*position += speed;
			}
		}
		State::Gain { gain } => {
			let input = io.mix(record, 0, ChannelMode::Max);
			let held = gain.values(first, rate, io, &mut values);
			if input.is_silent() {
				return;
			}
			out.zero(input.channels);
			for c in 0..input.channels {
				match held {
					Some(g) => {
						for i in 0..QUANTUM {
							out.data[c][i] = input.data[c][i] * g;
						}
					}
					None => {
						for i in 0..QUANTUM {
							out.data[c][i] = input.data[c][i] * values[i];
						}
					}
				}
			}
		}
		State::Biquad {
			kind,
			channels,
			memory,
			frequency,
			q,
			detune,
			last,
		} => {
			let input = io.mix(record, 0, ChannelMode::Max);
			if input.channels > *channels {
				memory[1] = memory[0];
				*channels = input.channels;
			}
			let mut qs = [0f32; QUANTUM];
			let mut detunes = [0f32; QUANTUM];
			let held_f = frequency.values(first, rate, io, &mut values);
			let held_q = q.values(first, rate, io, &mut qs);
			let held_d = detune.values(first, rate, io, &mut detunes);
			let coefficient = |f: f32, q: f32, d: f32| {
				let f = if d == 0.0 {
					f64::from(f)
				} else {
					f64::from(f) * 2f64.powf(f64::from(d) / 1200.0)
				};
				biquad::coefficients(*kind, f, f64::from(q), rate)
			};
			out.zero(*channels);
			let sample = |c: usize, i: usize| -> f64 {
				match input.channels {
					0 => 0.0,
					1 => f64::from(input.data[0][i]),
					_ => f64::from(input.data[c][i]),
				}
			};
			if let (Some(f), Some(qv), Some(d)) = (held_f, held_q, held_d) {
				let c = match *last {
					Some((lf, lq, c)) if lf == f && lq == qv && d == 0.0 => c,
					_ => {
						let c = coefficient(f, qv, d);
						*last = Some((f, qv, c));
						c
					}
				};
				for ch in 0..*channels {
					for i in 0..QUANTUM {
						out.data[ch][i] = memory[ch].tick(&c, sample(ch, i)) as f32;
					}
				}
			} else {
				for i in 0..QUANTUM {
					let c = coefficient(
						held_f.unwrap_or(values[i]),
						held_q.unwrap_or(qs[i]),
						held_d.unwrap_or(detunes[i]),
					);
					for ch in 0..*channels {
						out.data[ch][i] = memory[ch].tick(&c, sample(ch, i)) as f32;
					}
				}
				*last = None;
			}
		}
		State::Panner { pan } => {
			let input = io.mix(record, 0, ChannelMode::ClampedMax(2));
			let held = pan.values(first, rate, io, &mut values);
			if input.is_silent() {
				return;
			}
			out.zero(2);
			for i in 0..QUANTUM {
				let p = held.unwrap_or(values[i]);
				let right = if input.channels == 2 {
					input.data[1][i]
				} else {
					0.0
				};
				let (l, r) = pan_frame(p, input.channels, input.data[0][i], right);
				out.data[0][i] = l;
				out.data[1][i] = r;
			}
		}
		State::Delay {
			line,
			time,
			in_cycle,
		} => {
			let held = time.values(first, rate, io, &mut values);
			let mut delays = [0f64; QUANTUM];
			for (i, d) in delays.iter_mut().enumerate() {
				*d = f64::from(held.unwrap_or(values[i])) * rate;
			}
			if !*in_cycle {
				let input = io.mix(record, 0, ChannelMode::Max);
				line.write(first, &input);
			}
			line.read(first, &delays, out);
		}
		State::Convolver(convolver) => {
			let input = io.mix(record, 0, ChannelMode::ClampedMax(2));
			if let Some(convolver) = convolver {
				convolver.process(&input, out);
			}
		}
		State::Compressor {
			compressor,
			threshold,
			knee,
			ratio,
			attack,
			release,
		} => {
			let mut input = io.mix(record, 0, ChannelMode::ClampedMax(2));
			if input.is_silent() {
				input.zero(2);
			}
			input.upmix_stereo();
			let settings = Settings {
				threshold: threshold.k_rate(first, rate, io),
				knee: knee.k_rate(first, rate, io),
				ratio: ratio.k_rate(first, rate, io),
				attack: attack.k_rate(first, rate, io),
				release: release.k_rate(first, rate, io),
			};
			let [left, right] = &mut input.data;
			compressor.process(settings, [&mut left[..], &mut right[..]], None);
			*out = input;
		}
		State::Merger { inputs } => {
			let count = *inputs as usize;
			let mut any = false;
			let mut merged = Block::silent();
			merged.zero(count);
			for c in 0..count {
				let mix = io.mix(record, c as u32, ChannelMode::Explicit(1));
				if !mix.is_silent() {
					any = true;
					merged.data[c] = mix.data[0];
				}
			}
			if any {
				*out = merged;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graph::Buffer;

	fn rms(samples: &[f32]) -> f64 {
		(samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
	}

	fn peak(samples: &[f32]) -> f32 {
		samples.iter().fold(0f32, |m, s| m.max(s.abs()))
	}

	#[test]
	fn a_sine_has_its_period_and_amplitude() {
		for rate in [44_100.0, 48_000.0] {
			let mut graph = RenderGraph::new(1, rate as usize, rate);
			let osc = graph.node(NodeKind::Oscillator(Wave::Basic(OscillatorType::Sine)));
			graph.automate(
				ParamId::new(osc, Param::Frequency),
				Automation::Set {
					value: 441.0,
					at: 0.0,
				},
			);
			graph.connect(osc, 0, Input::node(graph.destination()));
			graph.start(osc, 0.0);
			let out = graph.render().unwrap().remove(0);
			for (n, s) in out.iter().enumerate().take(2000) {
				let expected = (2.0 * std::f64::consts::PI * 441.0 * n as f64 / rate).sin();
				assert!((f64::from(*s) - expected).abs() < 1e-4, "{rate} frame {n}");
			}
			assert!((f64::from(peak(&out)) - 1.0).abs() < 1e-4);
			assert!((rms(&out) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3);
			// Rising zero crossings a period apart.
			let crossings: Vec<usize> = (1..out.len())
				.filter(|&n| out[n - 1] < 0.0 && out[n] >= 0.0)
				.collect();
			let period = (crossings[crossings.len() - 1] - crossings[0]) as f64
				/ (crossings.len() - 1) as f64;
			assert!((period - rate / 441.0).abs() < 0.01, "{period}");
		}
	}

	#[test]
	fn sources_play_only_between_start_and_stop() {
		let rate = 48_000.0;
		let mut graph = RenderGraph::new(1, 4_800, rate);
		let osc = graph.node(NodeKind::Oscillator(Wave::Basic(OscillatorType::Square)));
		graph.connect(osc, 0, Input::node(graph.destination()));
		graph.start(osc, 0.01);
		graph.stop(osc, 0.05);
		let out = graph.render().unwrap().remove(0);
		assert!(out[..480].iter().all(|s| *s == 0.0));
		assert!(out[480..2400].iter().any(|s| *s != 0.0));
		assert!(out[2400..].iter().all(|s| *s == 0.0));
	}

	#[test]
	fn a_gain_envelope_follows_its_ramps() {
		let rate = 1_000.0;
		let mut graph = RenderGraph::new(1, 1_000, rate);
		let buffer = graph.buffer(Buffer {
			sample_rate: rate,
			channels: vec![vec![1.0; 10]],
		});
		let source = graph.node(NodeKind::BufferSource {
			buffer,
			looped: true,
		});
		let gain = graph.node(NodeKind::Gain);
		let g = ParamId::new(gain, Param::Gain);
		graph.automate(
			g,
			Automation::Set {
				value: 0.0,
				at: 0.0,
			},
		);
		graph.automate(
			g,
			Automation::Linear {
				value: 1.0,
				until: 0.5,
			},
		);
		graph.connect(source, 0, Input::node(gain));
		graph.connect(gain, 0, Input::node(graph.destination()));
		graph.start(source, 0.0);
		let out = graph.render().unwrap().remove(0);
		assert_eq!(out[250], 0.5);
		assert_eq!(out[900], 1.0);
	}

	#[test]
	fn a_feedback_echo_repeats_at_the_delay_time() {
		let rate = 48_000.0;
		let mut graph = RenderGraph::new(1, 48_000, rate);
		let buffer = graph.buffer(Buffer {
			sample_rate: rate,
			channels: vec![vec![1.0]],
		});
		let click = graph.node(NodeKind::BufferSource {
			buffer,
			looped: false,
		});
		let delay = graph.node(NodeKind::Delay { max_seconds: 1.0 });
		let feedback = graph.node(NodeKind::Gain);
		graph.automate(
			ParamId::new(delay, Param::DelayTime),
			Automation::Set {
				value: 0.125,
				at: 0.0,
			},
		);
		graph.automate(
			ParamId::new(feedback, Param::Gain),
			Automation::Set {
				value: 0.5,
				at: 0.0,
			},
		);
		graph.connect(click, 0, Input::node(delay));
		graph.connect(delay, 0, Input::node(feedback));
		graph.connect(feedback, 0, Input::node(delay));
		graph.connect(delay, 0, Input::node(graph.destination()));
		graph.start(click, 0.0);
		let out = graph.render().unwrap().remove(0);
		let hits: Vec<(usize, f32)> = out
			.iter()
			.enumerate()
			.filter(|(_, s)| s.abs() > 1e-6)
			.map(|(n, s)| (n, *s))
			.collect();
		assert_eq!(&hits[..3], &[(6_000, 1.0), (12_000, 0.5), (18_000, 0.25)]);
	}

	#[test]
	fn a_cycle_without_a_delay_is_refused() {
		let mut graph = RenderGraph::new(1, 128, 48_000.0);
		let a = graph.node(NodeKind::Gain);
		let b = graph.node(NodeKind::Gain);
		graph.connect(a, 0, Input::node(b));
		graph.connect(b, 0, Input::node(a));
		assert!(graph.render().is_err());
	}

	#[test]
	fn the_panner_places_a_mono_voice_by_equal_power() {
		let rate = 48_000.0;
		let mut graph = RenderGraph::new(2, 256, rate);
		let buffer = graph.buffer(Buffer {
			sample_rate: rate,
			channels: vec![vec![1.0; 256]],
		});
		let source = graph.node(NodeKind::BufferSource {
			buffer,
			looped: false,
		});
		let panner = graph.node(NodeKind::StereoPanner);
		graph.automate(
			ParamId::new(panner, Param::Pan),
			Automation::Set {
				value: 0.5,
				at: 0.0,
			},
		);
		graph.connect(source, 0, Input::node(panner));
		graph.connect(panner, 0, Input::node(graph.destination()));
		graph.start(source, 0.0);
		let out = graph.render().unwrap();
		let x = 0.75 * std::f64::consts::FRAC_PI_2;
		assert!((f64::from(out[0][10]) - x.cos()).abs() < 1e-6);
		assert!((f64::from(out[1][10]) - x.sin()).abs() < 1e-6);
	}

	#[test]
	fn fm_drives_the_carriers_frequency() {
		let rate = 48_000.0;
		let mut graph = RenderGraph::new(1, 48_000, rate);
		let carrier = graph.node(NodeKind::Oscillator(Wave::Basic(OscillatorType::Sine)));
		let modulator = graph.node(NodeKind::Oscillator(Wave::Basic(OscillatorType::Sine)));
		let depth = graph.node(NodeKind::Gain);
		graph.automate(
			ParamId::new(carrier, Param::Frequency),
			Automation::Set {
				value: 1_000.0,
				at: 0.0,
			},
		);
		graph.automate(
			ParamId::new(modulator, Param::Frequency),
			Automation::Set {
				value: 1.0,
				at: 0.0,
			},
		);
		graph.automate(
			ParamId::new(depth, Param::Gain),
			Automation::Set {
				value: 500.0,
				at: 0.0,
			},
		);
		graph.connect(modulator, 0, Input::node(depth));
		graph.connect(
			depth,
			0,
			Input::Param(ParamId::new(carrier, Param::Frequency)),
		);
		graph.connect(carrier, 0, Input::node(graph.destination()));
		graph.start(carrier, 0.0);
		graph.start(modulator, 0.0);
		let out = graph.render().unwrap().remove(0);
		// The phase is the integral of 1000 + 500 sin(2πt): crossings bunch near t = 0.25.
		let count = |from: usize, to: usize| {
			(from + 1..to)
				.filter(|&n| out[n - 1] < 0.0 && out[n] >= 0.0)
				.count()
		};
		let fast = count(11_400, 12_600);
		let slow = count(35_400, 36_600);
		assert!(
			(fast as i64 - 37).abs() <= 1 && (slow as i64 - 12).abs() <= 1,
			"{fast} {slow}"
		);
	}

	#[test]
	fn muted_tags_keep_their_booking_but_make_no_sound() {
		let rate = 48_000.0;
		let mut graph = RenderGraph::new(1, 1_000, rate);
		for tag in 0..2 {
			graph.set_tag(Some(tag));
			let osc = graph.node(NodeKind::Oscillator(Wave::Basic(OscillatorType::Sine)));
			graph.connect(osc, 0, Input::node(graph.destination()));
			graph.start(osc, 0.0);
		}
		let both = graph.render().unwrap().remove(0);
		let one = graph.render_where(|tag| tag == Some(0)).unwrap().remove(0);
		for (a, b) in both.iter().zip(&one) {
			assert!((a - 2.0 * b).abs() < 1e-6);
		}
		assert_eq!(graph.render().unwrap()[0], both);
	}
}
