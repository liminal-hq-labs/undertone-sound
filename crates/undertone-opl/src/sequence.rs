// Turns a compiled performance into timed OPL register writes: channels, stealing, drums and pan
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lowers a compiled performance to timed register writes. The allocation follows the TypeScript
//! reference step for step so the writes match its recorded ones: reuse the channel a part last played, else a free one,
//! else steal the least important, and drop the note when nothing is less important. The second
//! channel of an OPL3 stereo pair is taken only when one is free, and gives way to any note.

use std::collections::HashMap;

use undertone_sound::compile::{Performance, PerformanceEvent};
use undertone_sound::model::{Song, Track, TrackRole};

use crate::numbers::{max, to_int32};
use crate::patches::{
	RHYTHM_CHANNEL_HZ, RHYTHM_DRUMS, RhythmDrum, drum_trim, instrument_patch, piece_drum,
};
use crate::profile::OplProfile;
use crate::registers::{
	OUTPUT_BOTH, Patch, RegisterWrite, attenuation, level_writes, midi_to_hz, operator_registers,
	output_write, patch_writes, pitch_writes,
};
use crate::stereo::{DEFAULT_PAN_LAW, PlacedChannel, stereo_plan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// An OPL chip.
pub enum OplChip {
	/// The Yamaha YM3812, with nine channels.
	Opl2,
	/// The Yamaha YMF262, with eighteen channels and stereo.
	Opl3,
}

impl OplChip {
	/// The chip's name in a song file: `opl2` or `opl3`.
	pub fn as_str(self) -> &'static str {
		match self {
			OplChip::Opl2 => "opl2",
			OplChip::Opl3 => "opl3",
		}
	}

	/// The highest waveform number the chip has.
	pub fn max_wave(self) -> i32 {
		match self {
			OplChip::Opl2 => 3,
			OplChip::Opl3 => 7,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// One timed register write.
pub struct TimedWrite {
	/// Seconds from the start of the pass.
	pub time: f64,
	/// The register: 0x100 and up are the second bank.
	pub register: u16,
	/// The value written; the chip takes it modulo 256.
	pub value: i32,
}

#[derive(Debug, Clone, PartialEq)]
/// A performance as register writes.
pub struct OplSequence {
	/// The writes in time order, ties in the order made.
	pub writes: Vec<TimedWrite>,
	/// Length of one pass in seconds.
	pub seconds: f64,
	/// Notes that found no channel.
	pub dropped: usize,
	/// The notes that found no channel, in performance order.
	pub dropped_events: Vec<PerformanceEvent>,
	/// The notes that started and were ended early because a more important note took their
	/// channel, in the order they were cut. A note is counted once, though it holds two channels.
	pub cut_events: Vec<PerformanceEvent>,
}

/// The channels the sequencer may give a melodic note, in the order it tries them: every channel
/// of the chip except 6 to 8 of the first bank, which belong to the rhythm section, and the
/// profile's reserved channels. A reserved channel the chip does not have, or one the rhythm
/// section holds, changes nothing.
pub fn melodic_channels(chip: OplChip, profile: &OplProfile) -> Vec<usize> {
	let all: &[usize] = match chip {
		OplChip::Opl2 => &[0, 1, 2, 3, 4, 5],
		OplChip::Opl3 => &[0, 1, 2, 3, 4, 5, 9, 10, 11, 12, 13, 14, 15, 16, 17],
	};
	all.iter()
		.copied()
		.filter(|channel| !profile.reserved_channels().contains(channel))
		.collect()
}

/// How much a track's notes matter when channels run out: its own `priority`, else its role's. A
/// note an event names a track the song does not have for is treated as a melody's.
fn priority_of(track: Option<&Track>) -> i32 {
	track.map_or_else(
		|| i32::from(TrackRole::Melody.default_priority()),
		|track| i32::from(track.effective_priority()),
	)
}

/// The chip's lowest level: total level 63, the attenuation a note at velocity 0 gets.
const QUIETEST: i32 = 63;

/// A short silence before a note ends, so the next note on the channel re-triggers the envelope.
pub(crate) const KEY_GAP: f64 = 0.004;

/// The single-channel placement a note keeps if it loses the second channel of its pair.
#[derive(Clone)]
struct Fallback {
	patch: Patch,
	instrument_id: String,
	output: i32,
	extra_tl: i32,
	/// The modulator's share on an additive patch (0, or the lowest level for a hushed track).
	mod_extra_tl: i32,
}

#[derive(Clone)]
struct Channel {
	busy_until: f64,
	priority: i32,
	patch_key: Option<String>,
	track_id: Option<String>,
	/// The scheduled key-off of the note now on the channel (an index into the writes), cancelled if the note is stolen.
	key_off: Option<usize>,
	note_start: f64,
	/// The note now on the channel, so a steal can say which note it cut. Left empty on the
	/// second channel of a stereo pair: that note is the first channel's.
	event: Option<PerformanceEvent>,
	/// On the first channel of a stereo pair, its second channel.
	partner: Option<usize>,
	/// On the second channel of a stereo pair, the first.
	paired_with: Option<usize>,
	fallback: Option<Fallback>,
}

struct Sequencer {
	writes: Vec<TimedWrite>,
	/// Key-offs replaced by earlier ones when a note is cut short.
	cancelled: Vec<bool>,
	channels: Vec<Channel>,
	numbers: Vec<usize>,
	/// Notes ended early by a steal (`OplSequence::cut_events`).
	cut_events: Vec<PerformanceEvent>,
}

impl Sequencer {
	fn at(&mut self, time: f64, write: RegisterWrite) -> usize {
		self.writes.push(TimedWrite {
			register: write.register,
			value: write.value,
			time: max(0.0, time),
		});
		self.cancelled.push(false);
		self.writes.len() - 1
	}

	/// Ends a channel's note just before `start` if it is still sounding, and returns when it ends.
	fn release(&mut self, index: usize, start: f64) -> f64 {
		let channel = &self.channels[index];
		let Some(key_off) = channel.key_off else {
			return start;
		};
		if channel.busy_until <= start {
			return start;
		}
		// Stealing: cancel the old note's key-off and release it just before the new one.
		self.cancelled[key_off] = true;
		let time = max(channel.note_start, start - KEY_GAP);
		let old = self.writes[key_off];
		let new = self.at(
			time,
			RegisterWrite {
				register: old.register,
				value: old.value,
			},
		);
		let channel = &mut self.channels[index];
		channel.key_off = Some(new);
		channel.busy_until = time;
		time
	}

	/// Clears a channel's pairing on both sides, leaving it ready for a new note.
	fn unpair(&mut self, index: usize) {
		if let Some(partner) = self.channels[index].partner {
			self.channels[partner].paired_with = None;
		}
		if let Some(first) = self.channels[index].paired_with {
			self.channels[first].partner = None;
			self.channels[first].fallback = None;
		}
		let channel = &mut self.channels[index];
		channel.partner = None;
		channel.paired_with = None;
		channel.fallback = None;
	}

	/// Readies a channel for a new note at `start`, cutting short whatever it is playing.
	fn claim(&mut self, index: usize, start: f64) {
		let sounding = self.channels[index].busy_until > start;
		let time = self.release(index, start);
		let paired_with = self.channels[index].paired_with;
		let partner = self.channels[index].partner;
		// A sounding note taken from its channel is cut, unless it is the second half of a pair,
		// whose note carries on alone on the first channel.
		if sounding
			&& paired_with.is_none()
			&& let Some(event) = self.channels[index].event.clone()
		{
			self.cut_events.push(event);
		}
		if let (true, Some(first)) = (sounding, paired_with) {
			// The second channel of a pair gives way: its note carries on alone on its single-channel placement.
			let number = self.numbers[first];
			if let Some(fallback) = self.channels[first].fallback.clone()
				&& self.channels[first].busy_until > time
			{
				let write = output_write(number, &fallback.patch, fallback.output);
				self.at(time, write);
				for write in level_writes(
					number,
					&fallback.patch,
					fallback.extra_tl,
					fallback.mod_extra_tl,
				) {
					self.at(time, write);
				}
				self.channels[first].patch_key =
					Some(format!("{}|{}", fallback.instrument_id, fallback.output));
			}
		} else if let Some(partner) = partner {
			// A pair cut short loses both its channels.
			self.release(partner, start);
		}
		self.unpair(index);
	}
}

/// `a - b` as an ordering, where a negative zero and a zero are the same time.
fn compare(a: f64, b: f64) -> std::cmp::Ordering {
	a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

fn key_of(instrument_id: &str, placed: &PlacedChannel) -> String {
	format!("{instrument_id}|{}", placed.output)
}

/// The best of some free channels for a note: one its part used with this patch, else this patch,
/// else unused, else the longest idle.
fn preferred(
	channels: &[Channel],
	free: &[usize],
	patch_key: &str,
	track_id: &str,
) -> Option<usize> {
	let same_key = |i: usize| channels[i].patch_key.as_deref() == Some(patch_key);
	let found = free
		.iter()
		.copied()
		.find(|&i| same_key(i) && channels[i].track_id.as_deref() == Some(track_id))
		.or_else(|| free.iter().copied().find(|&i| same_key(i)))
		.or_else(|| {
			free.iter()
				.copied()
				.find(|&i| channels[i].patch_key.is_none())
		});
	found.or_else(|| {
		free.iter()
			.copied()
			.min_by(|&a, &b| compare(channels[a].busy_until, channels[b].busy_until))
	})
}

/// The channel for a note's own sound: a free one, else the second channel of a stereo pair
/// (which counts as free, so pairs never cost a note), else the least important note's, else none.
fn choose_channel(
	channels: &[Channel],
	start: f64,
	priority: i32,
	patch_key: &str,
	track_id: &str,
) -> Option<usize> {
	let free: Vec<usize> = (0..channels.len())
		.filter(|&i| channels[i].busy_until <= start)
		.collect();
	if let Some(pick) = preferred(channels, &free, patch_key, track_id) {
		return Some(pick);
	}
	let second = (0..channels.len())
		.filter(|&i| channels[i].paired_with.is_some())
		.min_by(|&a, &b| compare(channels[a].busy_until, channels[b].busy_until));
	if second.is_some() {
		return second;
	}
	let mut victim: Option<usize> = None;
	for (i, channel) in channels.iter().enumerate() {
		if channel.priority >= priority {
			continue;
		}
		match victim {
			None => victim = Some(i),
			Some(best) => {
				let best = &channels[best];
				if channel.priority < best.priority
					|| (channel.priority == best.priority && channel.busy_until < best.busy_until)
				{
					victim = Some(i);
				}
			}
		}
	}
	victim
}

/// Turns a performance into register writes for a chip, under the host's profile.
pub fn sequence_opl(
	song: &Song,
	performance: &Performance,
	chip: OplChip,
	profile: &OplProfile,
) -> OplSequence {
	sequence_opl_hushing(song, performance, chip, profile, None)
}

/// Like [`sequence_opl`], but every note and drum hit of the track `hushed` names is played at the
/// chip's lowest level (carrier and, on an additive patch, modulator at total level 63, about
/// 47 dB down). Everything else is the same: the track still takes and steals the channels it
/// takes and steals, so the mix of the others is exactly the mix they have with the track in. The
/// difference between this render and the plain one is the track's own sound in the mix.
pub fn sequence_opl_hushing(
	song: &Song,
	performance: &Performance,
	chip: OplChip,
	profile: &OplProfile,
	hushed: Option<&str>,
) -> OplSequence {
	let max_wave = chip.max_wave();
	let seconds = performance.length_steps / performance.steps_per_second;
	let numbers = melodic_channels(chip, profile);
	let channels = vec![
		Channel {
			busy_until: -1.0,
			priority: 0,
			patch_key: None,
			track_id: None,
			key_off: None,
			note_start: -1.0,
			event: None,
			partner: None,
			paired_with: None,
			fallback: None,
		};
		numbers.len()
	];
	let mut s = Sequencer {
		writes: Vec::new(),
		cancelled: Vec::new(),
		channels,
		numbers,
		cut_events: Vec::new(),
	};
	let plain = |register: u16, value: i32| RegisterWrite { register, value };

	// Set the chip up: OPL3 mode, waveform select, rhythm mode with its drums tuned and patched.
	if chip == OplChip::Opl3 {
		s.at(0.0, plain(0x105, 0x01));
		s.at(0.0, plain(0x104, 0x00));
	}
	s.at(0.0, plain(0x01, 0x20));
	s.at(0.0, plain(0xbd, 0x20));
	for drum in RHYTHM_DRUMS {
		let operators = drum.patch();
		for (i, &slot) in drum.rhythm().ops.iter().enumerate() {
			let r = operator_registers(&operators[i], max_wave);
			s.at(0.0, plain(0x20 + slot, r.r20));
			s.at(0.0, plain(0x40 + slot, r.r40));
			s.at(0.0, plain(0x60 + slot, r.r60));
			s.at(0.0, plain(0x80 + slot, r.r80));
			s.at(0.0, plain(0xe0 + slot, r.re0));
		}
	}
	for (i, &hz) in RHYTHM_CHANNEL_HZ.iter().enumerate() {
		for write in pitch_writes(6 + i, hz, false) {
			s.at(0.0, write);
		}
		if chip == OplChip::Opl3 {
			s.at(0.0, plain(0xc0 + 6 + i as u16, OUTPUT_BOTH));
		}
	}

	let priorities: HashMap<&str, i32> = song
		.tracks
		.iter()
		.map(|track| (track.id.as_str(), priority_of(Some(track))))
		.collect();
	let mut patches: HashMap<String, Patch> = HashMap::new();
	let pan_law = song
		.mix
		.as_ref()
		.and_then(|mix| mix.pan_law)
		.unwrap_or(DEFAULT_PAN_LAW);
	let mono = PlacedChannel {
		output: 0,
		gain: 1.0,
	};

	let mut dropped_events: Vec<PerformanceEvent> = Vec::new();
	let mut drum_hits: Vec<(f64, &PerformanceEvent)> = Vec::new();

	for event in &performance.events {
		let start = event.step / performance.steps_per_second;
		if event.drum.is_some() {
			drum_hits.push((start, event));
			continue;
		}
		let Some(midi) = event.midi else {
			continue;
		};
		let end = max(
			start + 0.01,
			(event.step + event.len) / performance.steps_per_second - KEY_GAP,
		);
		let hush = hushed == Some(event.track_id.as_str());
		let priority = priorities
			.get(event.track_id.as_str())
			.copied()
			.unwrap_or_else(|| priority_of(None));
		let (plan_channels, plan_fallback) = if chip == OplChip::Opl3 {
			let plan = stereo_plan(event.pan, pan_law);
			(plan.channels, plan.fallback)
		} else {
			(vec![mono], mono)
		};
		let first_key = key_of(&event.instrument_id, &plan_channels[0]);

		// The note's own channel is found exactly as for a single channel, so pairing never loses a note.
		let Some(index) = choose_channel(&s.channels, start, priority, &first_key, &event.track_id)
		else {
			dropped_events.push(event.clone());
			continue;
		};
		s.claim(index, start);
		// A second channel is taken only when one is free; otherwise the note plays on one channel.
		let mut placement = vec![plan_channels[0]];
		let mut partner: Option<usize> = None;
		if plan_channels.len() == 2 {
			let free: Vec<usize> = (0..s.channels.len())
				.filter(|&i| i != index && s.channels[i].busy_until <= start)
				.collect();
			partner = preferred(
				&s.channels,
				&free,
				&key_of(&event.instrument_id, &plan_channels[1]),
				&event.track_id,
			);
			if let Some(second) = partner {
				s.claim(second, start);
				placement = plan_channels.clone();
			} else {
				placement = vec![plan_fallback];
			}
		}

		let patch = patches
			.entry(event.instrument_id.clone())
			.or_insert_with(|| instrument_patch(song, &event.instrument_id, chip, profile))
			.clone();
		let hz = midi_to_hz(f64::from(midi));
		for (i, placed) in placement.iter().enumerate() {
			let which = if i == 0 { index } else { partner.unwrap_or(0) };
			let number = s.numbers[which];
			let extra_tl = if hush {
				QUIETEST
			} else {
				attenuation(event.velocity * placed.gain)
			};
			// An additive patch's modulator is heard too, so it takes the side's share as well.
			let mod_extra_tl = match (patch.additive, hush) {
				(false, _) => 0,
				(true, true) => QUIETEST,
				(true, false) => attenuation(placed.gain),
			};
			let patch_key = key_of(&event.instrument_id, placed);
			if s.channels[which].patch_key.as_deref() != Some(patch_key.as_str()) {
				for write in patch_writes(
					number,
					&patch,
					extra_tl,
					placed.output,
					max_wave,
					mod_extra_tl,
				) {
					s.at(start, write);
				}
			} else {
				for write in level_writes(number, &patch, extra_tl, mod_extra_tl) {
					s.at(start, write);
				}
			}
			for write in pitch_writes(number, hz, true) {
				s.at(start, write);
			}
			let off = s.at(end, pitch_writes(number, hz, false)[1]);
			let channel = &mut s.channels[which];
			channel.key_off = Some(off);
			channel.busy_until = end;
			// The second channel gives way to any note before anything is stolen.
			channel.priority = if i == 0 { priority } else { 0 };
			channel.patch_key = Some(patch_key);
			channel.track_id = Some(event.track_id.clone());
			channel.note_start = start;
			channel.event = (i == 0).then(|| event.clone());
		}
		if let Some(second) = partner {
			s.channels[index].partner = Some(second);
			s.channels[second].paired_with = Some(index);
			s.channels[index].fallback = Some(Fallback {
				patch,
				instrument_id: event.instrument_id.clone(),
				output: plan_fallback.output,
				extra_tl: if hush {
					QUIETEST
				} else {
					attenuation(event.velocity)
				},
				mod_extra_tl: if hush { QUIETEST } else { 0 },
			});
		}
	}

	sequence_drums(song, &drum_hits, chip, hushed, &mut s);

	let Sequencer {
		writes,
		cancelled,
		cut_events,
		..
	} = s;
	let mut kept: Vec<TimedWrite> = writes
		.into_iter()
		.zip(cancelled)
		.filter(|(_, cancelled)| !cancelled)
		.map(|(write, _)| write)
		.collect();
	// A stable sort keeps writes at the same time in the order they were made.
	kept.sort_by(|a, b| compare(a.time, b.time));
	OplSequence {
		writes: kept,
		seconds,
		dropped: dropped_events.len(),
		dropped_events,
		cut_events,
	}
}

struct Edge<'a> {
	time: f64,
	drum: RhythmDrum,
	on: bool,
	hit: usize,
	event: Option<&'a PerformanceEvent>,
}

/// Drum hits on the rhythm section. Each hit sets the drum's level from its velocity, then raises
/// its key bit in register BD (lowering it first if it was still up, so it strikes again), and
/// lowers it after the drum's hold unless a newer hit has come.
fn sequence_drums(
	song: &Song,
	hits: &[(f64, &PerformanceEvent)],
	chip: OplChip,
	hushed: Option<&str>,
	s: &mut Sequencer,
) {
	let mut edges: Vec<Edge> = Vec::new();
	for (hit, &(start, event)) in hits.iter().enumerate() {
		let (drum, _) = piece_drum(event.drum.expect("a drum hit has a piece"));
		edges.push(Edge {
			time: start,
			drum,
			on: true,
			hit,
			event: Some(event),
		});
		edges.push(Edge {
			time: start + drum.rhythm().hold,
			drum,
			on: false,
			hit,
			event: None,
		});
	}
	edges.sort_by(|a, b| compare(a.time, b.time).then(a.on.cmp(&b.on)));
	let mut mask: i32 = 0;
	let mut latest: HashMap<RhythmDrum, usize> = HashMap::new();
	let plain = |register: u16, value: i32| RegisterWrite { register, value };
	for edge in &edges {
		let rhythm = edge.drum.rhythm();
		if edge.on {
			let event = edge.event.expect("an on edge has its event");
			let (_, trim) = piece_drum(event.drum.expect("a drum hit has a piece"));
			let slot = *rhythm.ops.last().expect("a drum has an operator");
			let operator = &edge.drum.patch()[rhythm.ops.len() - 1];
			let tl = operator.tl
				+ f64::from(trim)
				+ drum_trim(song, &event.instrument_id, chip, edge.drum)
				+ if hushed == Some(event.track_id.as_str()) {
					f64::from(QUIETEST)
				} else {
					f64::from(attenuation(event.velocity))
				};
			let level =
				to_int32(operator.ksl).wrapping_shl(6) | to_int32(crate::numbers::min(63.0, tl));
			s.at(edge.time, plain(0x40 + slot, level));
			if mask & rhythm.bit != 0 {
				s.at(edge.time, plain(0xbd, 0x20 | (mask & !rhythm.bit)));
			}
			mask |= rhythm.bit;
			latest.insert(edge.drum, edge.hit);
			s.at(edge.time, plain(0xbd, 0x20 | mask));
		} else if latest.get(&edge.drum) == Some(&edge.hit) {
			mask &= !rhythm.bit;
			s.at(edge.time, plain(0xbd, 0x20 | mask));
		}
	}
}
