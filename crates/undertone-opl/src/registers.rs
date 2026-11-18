// OPL2/OPL3 register arithmetic: operator slots, F-numbers, patch writes and levels
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! OPL register arithmetic: pitch, levels and operator settings. Register values are `i32`, not
//! bytes: the TypeScript reference masks a value to a byte in `patch_writes` but not in `level_writes` or the drum
//! levels, so an out-of-range override can leave a value above 255 or below 0. The chip masks
//! when it is written, which is what the reference's emulator boundary did.

use crate::numbers::{max, min, round, to_int32};

/// The chip's sample rate: the rate F-numbers are worked out against.
pub const OPL_CLOCK: f64 = 49_716.0;

/// Register offsets of the modulator and carrier operators of each channel in a bank.
pub const OPERATORS: [(u16, u16); 9] = [
	(0x00, 0x03),
	(0x01, 0x04),
	(0x02, 0x05),
	(0x08, 0x0b),
	(0x09, 0x0c),
	(0x0a, 0x0d),
	(0x10, 0x13),
	(0x11, 0x14),
	(0x12, 0x15),
];

/// OPL3 output bits in register C0: left, right, or both (on the OPL3 a channel with neither is silent).
pub const OUTPUT_LEFT: i32 = 0x10;
/// Output to the right speaker (OPL3).
pub const OUTPUT_RIGHT: i32 = 0x20;
/// Output to both speakers (OPL3).
pub const OUTPUT_BOTH: i32 = 0x30;

/// One operator. Numbers are doubles because a song's override can be any JSON number, and every
/// register use truncates them as JavaScript's bitwise operators do.
#[derive(Debug, Clone, PartialEq)]
pub struct Operator {
	/// Frequency multiple as the 4-bit register value (0 is x0.5).
	pub mult: f64,
	/// Total level, 0 (loudest) to 63 in 0.75 dB steps.
	pub tl: f64,
	/// Key scale level, 0 to 3.
	pub ksl: f64,
	/// Attack, decay and release rates 0 to 15 (15 is fastest).
	pub ar: f64,
	/// Decay rate.
	pub dr: f64,
	/// Sustain level 0 (loudest) to 15 (silent).
	pub sl: f64,
	/// Release rate.
	pub rr: f64,
	/// Hold at the sustain level while the key is down.
	pub sustain: bool,
	/// Key scale rate: envelopes run faster on higher notes.
	pub ksr: bool,
	/// Waveform 0 to 3 (OPL2) or 0 to 7 (OPL3).
	pub wave: f64,
	/// Vibrato on.
	pub vibrato: bool,
	/// Tremolo on.
	pub tremolo: bool,
}

impl Operator {
	/// The starting point every preset operator is built from.
	pub const BASE: Operator = Operator {
		mult: 1.0,
		tl: 0.0,
		ksl: 0.0,
		ar: 15.0,
		dr: 4.0,
		sl: 3.0,
		rr: 7.0,
		sustain: true,
		ksr: false,
		wave: 0.0,
		vibrato: false,
		tremolo: false,
	};
}

#[derive(Debug, Clone, PartialEq)]
/// Two operators and how they join.
pub struct Patch {
	/// The modulator, which drives the carrier (or adds to it).
	pub modulator: Operator,
	/// The carrier, which makes the sound.
	pub carrier: Operator,
	/// Modulator feedback, 0 to 7.
	pub feedback: f64,
	/// Adds both operators instead of the modulator driving the carrier.
	pub additive: bool,
}

/// One register write. Registers 0x100 and up are the OPL3's second bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterWrite {
	/// The register: 0x100 and up are the OPL3's second bank.
	pub register: u16,
	/// The value written; the chip takes it modulo 256.
	pub value: i32,
}

/// The bank offset (0 or 0x100) and in-bank slot of a channel numbered 0 to 17.
pub fn bank_of(channel: usize) -> (u16, usize) {
	(if channel >= 9 { 0x100 } else { 0 }, channel % 9)
}

/// The F-number and block that sound `hz`, with the F-number as high as it goes for resolution.
pub fn fnumber(hz: f64) -> (i32, i32) {
	let safe = min(max(hz, 16.0), 6000.0);
	for block in 0..8 {
		let scaled = safe * f64::from(1u32 << (20 - block));
		let fnum = round(scaled / OPL_CLOCK);
		if fnum <= 1023.0 {
			return (to_int32(fnum), block);
		}
	}
	(1023, 7)
}

/// The five operator registers of one operator.
#[derive(Debug, Clone, Copy)]
pub struct OperatorRegisters {
	/// Register 20: tremolo, vibrato, sustain, key scale rate and multiple.
	pub r20: i32,
	/// Register 40: key scale level and total level.
	pub r40: i32,
	/// Register 60: attack and decay rates.
	pub r60: i32,
	/// Register 80: sustain level and release rate.
	pub r80: i32,
	/// Register E0: waveform.
	pub re0: i32,
}

/// The five registers an operator writes.
pub fn operator_registers(op: &Operator, max_wave: i32) -> OperatorRegisters {
	let flag = |on: bool, bit: i32| if on { bit } else { 0 };
	let wave = min(op.wave, f64::from(max_wave));
	OperatorRegisters {
		r20: flag(op.tremolo, 0x80)
			| flag(op.vibrato, 0x40)
			| flag(op.sustain, 0x20)
			| flag(op.ksr, 0x10)
			| (to_int32(op.mult) & 0x0f),
		r40: to_int32(op.ksl).wrapping_shl(6) | (to_int32(op.tl) & 0x3f),
		r60: ((to_int32(op.ar) & 0x0f) << 4) | (to_int32(op.dr) & 0x0f),
		r80: ((to_int32(op.sl) & 0x0f) << 4) | (to_int32(op.rr) & 0x0f),
		re0: to_int32(wave) & 7,
	}
}

/// Writes a whole patch to a channel, with `extra_tl` (0.75 dB steps) added to the carrier,
/// `mod_extra_tl` to the modulator, and the OPL3 `output` bits (0 on an OPL2) in C0.
pub fn patch_writes(
	channel: usize,
	patch: &Patch,
	extra_tl: i32,
	output: i32,
	max_wave: i32,
	mod_extra_tl: i32,
) -> Vec<RegisterWrite> {
	let (bank, slot) = bank_of(channel);
	let (mod_slot, car_slot) = OPERATORS[slot];
	let m = operator_registers(&patch.modulator, max_wave);
	let c = operator_registers(&patch.carrier, max_wave);
	let at = |register: u16, value: i32| RegisterWrite {
		register: bank + register,
		value: value & 0xff,
	};
	vec![
		at(0x20 + mod_slot, m.r20),
		at(0x20 + car_slot, c.r20),
		at(
			0x40 + mod_slot,
			(m.r40 & 0xc0) | (63).min((m.r40 & 0x3f) + mod_extra_tl),
		),
		at(
			0x40 + car_slot,
			(c.r40 & 0xc0) | (63).min((c.r40 & 0x3f) + extra_tl),
		),
		at(0x60 + mod_slot, m.r60),
		at(0x60 + car_slot, c.r60),
		at(0x80 + mod_slot, m.r80),
		at(0x80 + car_slot, c.r80),
		at(0xe0 + mod_slot, m.re0),
		at(0xe0 + car_slot, c.re0),
		output_write(channel, patch, output),
	]
}

/// Sets a channel's C0 register: its OPL3 output bits with the patch's feedback and connection.
pub fn output_write(channel: usize, patch: &Patch, output: i32) -> RegisterWrite {
	let (bank, slot) = bank_of(channel);
	RegisterWrite {
		register: bank + 0xc0 + slot as u16,
		value: (output | ((to_int32(patch.feedback) & 7) << 1) | i32::from(patch.additive)) & 0xff,
	}
}

/// Sets only the levels of a channel: `extra_tl` on the carrier (the note's loudness) and, for an
/// additive patch, whose modulator is heard too, `mod_extra_tl` on the modulator.
pub fn level_writes(
	channel: usize,
	patch: &Patch,
	extra_tl: i32,
	mod_extra_tl: i32,
) -> Vec<RegisterWrite> {
	let (bank, slot) = bank_of(channel);
	let (mod_slot, car_slot) = OPERATORS[slot];
	let level = |op: &Operator, extra: i32| {
		to_int32(op.ksl).wrapping_shl(6) | (63).min((to_int32(op.tl) & 0x3f) + extra)
	};
	let mut writes = vec![RegisterWrite {
		register: bank + 0x40 + car_slot,
		value: level(&patch.carrier, extra_tl),
	}];
	if patch.additive {
		writes.push(RegisterWrite {
			register: bank + 0x40 + mod_slot,
			value: level(&patch.modulator, mod_extra_tl),
		});
	}
	writes
}

/// Sets a channel's pitch and its key bit.
pub fn pitch_writes(channel: usize, hz: f64, key_on: bool) -> [RegisterWrite; 2] {
	let (bank, slot) = bank_of(channel);
	let (fnum, block) = fnumber(hz);
	[
		RegisterWrite {
			register: bank + 0xa0 + slot as u16,
			value: fnum & 0xff,
		},
		RegisterWrite {
			register: bank + 0xb0 + slot as u16,
			value: (if key_on { 0x20 } else { 0 }) | (block << 2) | (fnum >> 8),
		},
	]
}

/// Attenuation in 0.75 dB steps for a gain relative to a reference of 1.
pub fn attenuation(gain: f64) -> i32 {
	attenuation_from(gain, 1.0)
}

/// Attenuation in 0.75 dB steps for a gain relative to `reference`.
pub fn attenuation_from(gain: f64, reference: f64) -> i32 {
	if gain <= 0.0 {
		return 63;
	}
	let steps = round((20.0 * (reference / gain).log10()) / 0.75);
	to_int32(max(0.0, min(63.0, steps)))
}

/// The frequency of a MIDI note in Hz.
pub fn midi_to_hz(midi: f64) -> f64 {
	440.0 * 2f64.powf((midi - 69.0) / 12.0)
}
