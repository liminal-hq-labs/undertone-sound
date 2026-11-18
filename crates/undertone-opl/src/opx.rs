// Writes a sequenced song as an OPX stream.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An [`OplSequence`] holds register writes in seconds; OPX holds them on a grid of ticks. The
//! export rounds each write to a tick, and the options say how much more of the stream's shape is
//! left to the sequencer and how much a host fixes:
//!
//! - `headroom` adds steps of 0.75 dB to every melodic carrier, for a host that wants its music
//!   quieter than the sequencer's levels.
//! - [`KeyOffs::BeforeNextKeyOn`] ends each note on or before the tick the next note on its channel
//!   begins, and no sooner than one tick after it began, so a grid coarser than the sequencer's
//!   times cannot swallow a note.
//! - [`DrumHolds::Ticks`] replaces the sequencer's rhythm key writes with a hold of a fixed number
//!   of ticks for each drum, or until the drum is struck again.
//!
//! Registers 0x104 and 0x105 belong to the host's audio service (`docs/OPX.md`),
//! so the export never writes them: a host plays an OPL3 stream with the OPL3's mode already on.
//! The header's `reserved_channels` are the profile's, limited to the channels the chip has.

use oplstream::{Chip, WriteError, Writer};

use crate::patches::RHYTHM_DRUMS;
use crate::profile::OplProfile;
use crate::registers::OPERATORS;
use crate::sequence::{OplChip, OplSequence};

/// How the key-off of a melodic note is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOffs {
	/// Where the sequencer put it, rounded to a tick.
	AsSequenced,
	/// At the note's end, but no later than the tick before the next note on the channel starts,
	/// and no sooner than one tick after the note started.
	BeforeNextKeyOn,
}

/// How long a rhythm drum's key bit stays up after a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumHolds {
	/// The sequencer's own rhythm key writes, rounded to ticks.
	AsSequenced,
	/// Each drum held for this many ticks, in the order of [`RHYTHM_DRUMS`] (bass, snare, tom,
	/// cymbal, hat), or until its next hit.
	Ticks([i64; 5]),
}

/// How an export turns a sequence into a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpxOptions {
	/// Ticks a second.
	pub grid_hz: u32,
	/// Steps of 0.75 dB added to every melodic carrier's total level.
	pub headroom: i32,
	/// Where melodic key-offs go.
	pub key_offs: KeyOffs,
	/// How long the rhythm drums are held.
	pub drum_holds: DrumHolds,
}

impl OpxOptions {
	/// A grid with every write left where the sequencer put it, rounded to a tick.
	pub const fn plain(grid_hz: u32) -> OpxOptions {
		OpxOptions {
			grid_hz,
			headroom: 0,
			key_offs: KeyOffs::AsSequenced,
			drum_holds: DrumHolds::AsSequenced,
		}
	}
}

/// What an export made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpxExport {
	/// The OPX file.
	pub bytes: Vec<u8>,
	/// The stream's length in ticks.
	pub ticks: u32,
	/// Register writes in the stream.
	pub writes: usize,
	/// Melodic key-ons.
	pub notes: usize,
	/// Drum hits, with hits of one drum at one tick counted once.
	pub drum_hits: usize,
	/// Notes the chip's channels could not carry.
	pub dropped: usize,
}

/// Why a sequence could not be exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpxError {
	/// The sequence has no length on the grid.
	Empty,
	/// The grid is zero ticks a second.
	ZeroGrid,
	/// The writer refused the stream.
	Write(WriteError),
}

impl std::fmt::Display for OpxError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			OpxError::Empty => write!(f, "the sequence has no length on the grid"),
			OpxError::ZeroGrid => write!(f, "the grid is zero ticks a second"),
			OpxError::Write(error) => write!(f, "the stream could not be written: {error:?}"),
		}
	}
}

impl std::error::Error for OpxError {}

impl From<WriteError> for OpxError {
	fn from(error: WriteError) -> OpxError {
		OpxError::Write(error)
	}
}

#[derive(Debug, Clone, Copy)]
struct Write {
	tick: i64,
	/// The sequencer's time in seconds, kept to work out where a key-off belongs.
	time: f64,
	register: u16,
	value: u8,
	/// The order the write was made in, which breaks ties between writes at one tick.
	seq: usize,
}

/// The sequencer ends a note `KEY_GAP` before its time so the next note re-triggers; adding the gap
/// back gives the moment the note really ends. The constant lives in `sequence`.
const KEY_GAP: f64 = crate::sequence::KEY_GAP;

/// The level registers of the rhythm drums' operators (slots 0x10 to 0x15).
fn is_drum_level(register: u16) -> bool {
	(0x50..=0x55).contains(&register)
}

/// The channel (0 to 17) a write to a key register (B0 to B8 of either bank) keys, leaving out
/// channels 6 to 8 of the first bank, which the rhythm section holds.
fn melodic_key(w: &Write) -> Option<usize> {
	let (bank, low) = (usize::from(w.register >> 8), w.register & 0xff);
	if !(0xb0..=0xb8).contains(&low) {
		return None;
	}
	let channel = usize::from(low - 0xb0);
	(bank == 1 || channel < 6).then_some(bank * 9 + channel)
}

fn add_steps(value: u8, steps: i32) -> u8 {
	let tl = (i32::from(value & 0x3f) + steps).clamp(0, 63) as u8;
	(value & 0xc0) | tl
}

/// Adds `steps` to the carrier level of every melodic channel, the way a converter adds its
/// trim to the carrier alone. An additive patch's modulator is heard too, and is left as it is.
fn add_headroom(writes: &mut [Write], steps: i32) {
	if steps == 0 {
		return;
	}
	for w in writes.iter_mut() {
		let (bank, low) = (w.register >> 8, w.register & 0xff);
		let Some(slot) = low.checked_sub(0x40) else {
			continue;
		};
		let carries = OPERATORS
			.iter()
			.enumerate()
			.any(|(channel, &(_, carrier))| carrier == slot && (bank == 1 || channel < 6));
		if carries {
			w.value = add_steps(w.value, steps);
		}
	}
}

/// Puts each melodic key-off at `off = max(on + 1, min(intended, next_on - 1, total - 1))`,
/// where `intended` is the note's real end; two notes that start on one tick leave the first a
/// zero-length note rather than a key-on the second swallows.
fn retime_key_offs(writes: &mut [Write], rate: f64, total: i64) {
	let n = writes.len();
	let mut next_on: Vec<Option<i64>> = vec![None; n];
	let mut upcoming: [Option<i64>; 18] = [None; 18];
	for i in (0..n).rev() {
		if let Some(channel) = melodic_key(&writes[i]) {
			next_on[i] = upcoming[channel];
			if writes[i].value & 0x20 != 0 {
				upcoming[channel] = Some(writes[i].tick);
			}
		}
	}
	let mut last_on: [Option<i64>; 18] = [None; 18];
	for i in 0..n {
		let Some(channel) = melodic_key(&writes[i]) else {
			continue;
		};
		if writes[i].value & 0x20 != 0 {
			last_on[channel] = Some(writes[i].tick);
			continue;
		}
		let Some(on) = last_on[channel] else {
			continue;
		};
		let intended = ((writes[i].time + KEY_GAP) * rate).round() as i64;
		let mut off = intended.min(total - 1);
		if let Some(next) = next_on[i] {
			off = off.min(next - 1);
		}
		off = off.max(on + 1);
		if let Some(next) = next_on[i] {
			off = off.min(next);
		}
		writes[i].tick = off;
	}
	writes.sort_by_key(|w| w.tick);
}

/// Hits per drum, as the index into [`RHYTHM_DRUMS`] and the unique sorted ticks.
type Hits = Vec<(usize, Vec<i64>)>;

/// The hits in a stream: the rising edges of each drum's bit in register BD, a lowering and a
/// raising at one instant counting as one hit.
fn find_hits(writes: &[Write]) -> Hits {
	let mut found: Vec<(usize, i64)> = Vec::new();
	let mut mask: u8 = 0;
	for w in writes.iter().filter(|w| w.register == 0xbd) {
		let now = w.value & 0x1f;
		let rising = now & !mask;
		for (index, drum) in RHYTHM_DRUMS.iter().enumerate() {
			if i32::from(rising) & drum.rhythm().bit != 0 {
				found.push((index, w.tick));
			}
		}
		mask = now;
	}
	let mut hits: Hits = Vec::new();
	for index in 0..RHYTHM_DRUMS.len() {
		let mut ticks: Vec<i64> = found
			.iter()
			.filter(|(i, _)| *i == index)
			.map(|(_, t)| *t)
			.collect();
		ticks.sort_unstable();
		ticks.dedup();
		if !ticks.is_empty() {
			hits.push((index, ticks));
		}
	}
	hits
}

/// Replaces the sequencer's rhythm key writes (all but the first, which turns the rhythm section
/// on) with a hold of `holds` ticks for each drum, or until its next hit, and one BD write after
/// all the changes of a tick. The per-hit level writes stay.
fn rebuild_drums(writes: &mut Vec<Write>, hits: &Hits, holds: &[i64; 5], total: i64) {
	let first = writes.iter().position(|w| w.register == 0xbd);
	let mut index = 0;
	writes.retain(|w| {
		let keep = w.register != 0xbd || first == Some(index);
		index += 1;
		keep
	});
	let mut changes: Vec<(i64, i32, bool)> = Vec::new();
	for (index, ticks) in hits {
		let bit = RHYTHM_DRUMS[*index].rhythm().bit;
		for (i, &t) in ticks.iter().enumerate() {
			let mut off = (t + holds[*index]).min(total - 1);
			if let Some(&next) = ticks.get(i + 1) {
				off = off.min(next - 1);
			}
			changes.push((t, bit, true));
			changes.push((off.max(t + 1), bit, false));
		}
	}
	// Offs before ons at one tick, then every change of a tick takes effect before one write.
	changes.sort_by_key(|&(tick, _, on)| (tick, on));
	let mut bits: i32 = 0;
	let mut i = 0;
	let mut order = usize::MAX / 2;
	while i < changes.len() {
		let tick = changes[i].0;
		while i < changes.len() && changes[i].0 == tick {
			let (_, bit, on) = changes[i];
			bits = if on { bits | bit } else { bits & !bit };
			i += 1;
		}
		writes.push(Write {
			tick,
			time: 0.0,
			register: 0xbd,
			value: (0x20 | bits) as u8,
			seq: order,
		});
		order += 1;
	}
	writes.sort_by_key(|w| (w.tick, w.seq));
}

/// Leaves out a write to a drum operator's level that sets the level it already has: the
/// sequencer sets a drum's level at every hit, and on the chip a repeat is no change.
fn drop_unchanged_drum_levels(writes: &mut Vec<Write>) {
	let mut levels: [Option<u8>; 6] = [None; 6];
	writes.retain(|w| {
		if !is_drum_level(w.register) {
			return true;
		}
		let slot = usize::from(w.register - 0x50);
		if levels[slot] == Some(w.value) {
			return false;
		}
		levels[slot] = Some(w.value);
		true
	});
}

/// Exports a sequence as an OPX file for `chip`, under the host's profile.
///
/// The stream's header names the chip and the grid, and its `reserved_channels` are the
/// profile's. The writer checks the finished file against the specification.
pub fn export_opx(
	sequence: &OplSequence,
	chip: OplChip,
	profile: &OplProfile,
	options: &OpxOptions,
) -> Result<OpxExport, OpxError> {
	if options.grid_hz == 0 {
		return Err(OpxError::ZeroGrid);
	}
	let rate = f64::from(options.grid_hz);
	let total = (sequence.seconds * rate).round() as i64;
	if total < 1 {
		return Err(OpxError::Empty);
	}

	let mut writes: Vec<Write> = sequence
		.writes
		.iter()
		.enumerate()
		.filter(|(_, w)| !matches!(w.register, 0x104 | 0x105))
		.map(|(seq, w)| Write {
			tick: (w.time * rate).round() as i64,
			time: w.time,
			register: w.register,
			value: (w.value & 0xff) as u8,
			seq,
		})
		.collect();

	add_headroom(&mut writes, options.headroom);
	if options.key_offs == KeyOffs::BeforeNextKeyOn {
		retime_key_offs(&mut writes, rate, total);
	}
	let hits = find_hits(&writes);
	if let DrumHolds::Ticks(holds) = &options.drum_holds {
		rebuild_drums(&mut writes, &hits, holds, total);
	}
	for w in &mut writes {
		w.tick = w.tick.clamp(0, total - 1);
	}
	writes.sort_by_key(|w| (w.tick, w.seq));
	drop_unchanged_drum_levels(&mut writes);

	let notes = writes
		.iter()
		.filter(|w| melodic_key(w).is_some() && w.value & 0x20 != 0)
		.count();
	let drum_hits = hits.iter().map(|(_, ticks)| ticks.len()).sum();

	let mut writer = Writer::new(
		match chip {
			OplChip::Opl2 => Chip::Opl2,
			OplChip::Opl3 => Chip::Opl3,
		},
		options.grid_hz,
	);
	// A channel the chip does not have cannot be written, so the mask holds only the chip's own.
	let channels: u32 = match chip {
		OplChip::Opl2 => 9,
		OplChip::Opl3 => 18,
	};
	writer.reserved_channels(profile.reserved_mask() & ((1 << channels) - 1));
	let mut now = 0;
	for w in &writes {
		if w.tick > now {
			writer.wait((w.tick - now) as u32);
			now = w.tick;
		}
		writer.write((w.register >> 8) as u8, (w.register & 0xff) as u8, w.value);
	}
	writer.wait((total - now) as u32);
	let bytes = writer.finish()?;
	Ok(OpxExport {
		bytes,
		ticks: total as u32,
		writes: writes.len(),
		notes,
		drum_hits,
		dropped: sequence.dropped,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::analyse::decode_opx;
	use crate::sequence::TimedWrite;
	use oplstream::{Command, Decoder, HEADER_LEN};

	fn at(time: f64, register: u16, value: i32) -> TimedWrite {
		TimedWrite {
			time,
			register,
			value,
		}
	}

	fn sequence(writes: Vec<TimedWrite>, seconds: f64) -> OplSequence {
		OplSequence {
			writes,
			seconds,
			dropped: 0,
			dropped_events: Vec::new(),
			cut_events: Vec::new(),
		}
	}

	/// Two notes on channel 0: the first is keyed off 4 ms before the second begins.
	fn two_notes() -> OplSequence {
		sequence(
			vec![
				at(0.0, 0x01, 0x20),
				at(0.0, 0x43, 0x10),
				at(0.0, 0xa0, 0x41),
				at(0.0, 0xb0, 0x31),
				at(0.5 - 0.004, 0xb0, 0x11),
				at(0.5, 0xa0, 0x57),
				at(0.5, 0xb0, 0x31),
				at(1.0 - 0.004, 0xb0, 0x11),
			],
			1.0,
		)
	}

	fn notes(bytes: &[u8]) -> Vec<(i64, i64)> {
		decode_opx(bytes)
			.unwrap()
			.notes
			.iter()
			.map(|n| (n.on, n.off))
			.collect()
	}

	#[test]
	fn a_plain_export_rounds_each_write_to_a_tick_and_holds_the_last_back_from_the_end() {
		let profile = OplProfile::new();
		let out = export_opx(
			&two_notes(),
			OplChip::Opl2,
			&profile,
			&OpxOptions::plain(100),
		)
		.unwrap();
		assert_eq!(out.ticks, 100);
		assert_eq!(out.notes, 2);
		assert_eq!(notes(&out.bytes), [(0, 50), (50, 99)]);
		oplstream::validate(&out.bytes).unwrap();
	}

	#[test]
	fn a_coarse_grid_cannot_swallow_a_note_when_key_offs_go_before_the_next_key_on() {
		let profile = OplProfile::new();
		// At 100 Hz the first key-off (0.496 s) and the second key-on (0.5 s) round to one tick,
		// so the key-off moves one tick earlier.
		let mut options = OpxOptions::plain(100);
		options.key_offs = KeyOffs::BeforeNextKeyOn;
		let out = export_opx(&two_notes(), OplChip::Opl2, &profile, &options).unwrap();
		assert_eq!(notes(&out.bytes), [(0, 49), (50, 99)]);
	}

	#[test]
	fn headroom_is_added_to_a_melodic_carrier_and_not_to_a_modulator() {
		let profile = OplProfile::new();
		let mut options = OpxOptions::plain(100);
		options.headroom = 6;
		let out = export_opx(
			&sequence(
				vec![
					at(0.0, 0x40, 0x10),
					at(0.0, 0x43, 0x10),
					at(0.0, 0x143, 0x3e),
				],
				1.0,
			),
			OplChip::Opl3,
			&profile,
			&options,
		)
		.unwrap();
		let mut values = Vec::new();
		for step in Decoder::new(&out.bytes[HEADER_LEN..], Chip::Opl3) {
			if let Command::Write { bank, reg, val } = step.unwrap().command {
				values.push((bank, reg, val));
			}
		}
		assert_eq!(values, [(0, 0x40, 0x10), (0, 0x43, 0x16), (1, 0x43, 0x3f)]);
	}

	#[test]
	fn the_opl3_mode_registers_are_the_hosts() {
		let profile = OplProfile::new();
		let sequence = sequence(
			vec![at(0.0, 0x105, 1), at(0.0, 0x104, 0), at(0.0, 0x01, 0x20)],
			1.0,
		);
		let out = export_opx(&sequence, OplChip::Opl3, &profile, &OpxOptions::plain(100)).unwrap();
		assert_eq!(out.writes, 1);
	}

	#[test]
	fn the_headers_reserved_channels_are_the_profiles_on_the_chips_own_channels() {
		let profile = OplProfile::new().reserve([5, 14]);
		let reserved = |chip| {
			let out = export_opx(&two_notes(), chip, &profile, &OpxOptions::plain(100)).unwrap();
			oplstream::validate(&out.bytes).unwrap().reserved_channels
		};
		assert_eq!(reserved(OplChip::Opl3), 1 << 5 | 1 << 14);
		assert_eq!(reserved(OplChip::Opl2), 1 << 5);
	}

	#[test]
	fn a_drum_is_held_for_its_ticks_or_until_it_is_struck_again() {
		let profile = OplProfile::new();
		let mut options = OpxOptions::plain(100);
		options.drum_holds = DrumHolds::Ticks([20, 10, 10, 10, 10]);
		let strike = |time: f64, mask: i32| at(time, 0xbd, 0x20 | mask);
		let out = export_opx(
			&sequence(
				vec![
					strike(0.0, 0),
					strike(0.10, 0x10),
					strike(0.17, 0),
					strike(0.30, 0x10),
					strike(0.37, 0),
				],
				1.0,
			),
			OplChip::Opl2,
			&profile,
			&options,
		)
		.unwrap();
		assert_eq!(out.drum_hits, 2);
		let decoded = decode_opx(&out.bytes).unwrap();
		assert_eq!(
			decoded.hits.iter().map(|h| h.tick).collect::<Vec<_>>(),
			[10, 30]
		);
		// Held 20 ticks, or until the next hit: the first drops a tick before the second strikes.
		let mut writes = Vec::new();
		for step in Decoder::new(&out.bytes[HEADER_LEN..], Chip::Opl2) {
			let step = step.unwrap();
			if let Command::Write { reg: 0xbd, val, .. } = step.command {
				writes.push((step.tick, val));
			}
		}
		assert_eq!(
			writes,
			[(0, 0x20), (10, 0x30), (29, 0x20), (30, 0x30), (50, 0x20)]
		);
	}

	#[test]
	fn a_grid_of_zero_and_a_sequence_of_no_length_are_refused() {
		let profile = OplProfile::new();
		assert_eq!(
			export_opx(&two_notes(), OplChip::Opl2, &profile, &OpxOptions::plain(0)),
			Err(OpxError::ZeroGrid)
		);
		assert_eq!(
			export_opx(
				&sequence(vec![], 0.0),
				OplChip::Opl2,
				&profile,
				&OpxOptions::plain(100)
			),
			Err(OpxError::Empty)
		);
	}
}
