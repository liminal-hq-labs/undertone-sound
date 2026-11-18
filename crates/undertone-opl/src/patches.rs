// OPL patches for presets, rhythm-mode drums, and an instrument's own patch edits
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The patches a preset plays on the OPL chips, the rhythm-mode drums, and an instrument's own
//! patch edits. A host supplies the patches of presets this crate does not hold through
//! [`PatchBank`].

use std::collections::HashMap;
use undertone_sound::model::{DrumPiece, EngineId, PresetId, Song};

use serde_json::{Map, Value};

use crate::numbers::{js_number, js_truthy};
use crate::profile::OplProfile;
use crate::registers::{Operator, Patch};
use crate::sequence::OplChip;

const fn op(mult: f64, tl: f64) -> Operator {
	Operator {
		mult,
		tl,
		..Operator::BASE
	}
}

/// The patch this crate holds for a preset, or `None` for a preset a host supplies the sound of
/// (`bass-saw`, `pad-bright` and `drone`). A kit has no pitched patch and plays as `lead`. The
/// operator settings are first guesses meant to be tuned by ear.
pub fn standard_patch(preset: PresetId) -> Option<Patch> {
	let fm = |modulator: Operator, carrier: Operator, feedback: f64| Patch {
		modulator,
		carrier,
		feedback,
		additive: false,
	};
	Some(match preset {
		PresetId::Lead | PresetId::Kit => fm(op(1.0, 14.0), op(1.0, 0.0), 7.0),
		PresetId::SquareLead => fm(
			Operator {
				dr: 4.0,
				sl: 4.0,
				..op(1.0, 24.0)
			},
			op(1.0, 0.0),
			4.0,
		),
		PresetId::SoftLead => fm(op(1.0, 40.0), op(1.0, 0.0), 2.0),
		PresetId::Pluck => fm(
			Operator {
				ar: 15.0,
				dr: 10.0,
				sl: 12.0,
				sustain: false,
				..op(2.0, 26.0)
			},
			Operator {
				ar: 15.0,
				dr: 9.0,
				sl: 8.0,
				rr: 8.0,
				sustain: false,
				..op(1.0, 0.0)
			},
			3.0,
		),
		PresetId::Bell => fm(
			Operator {
				ar: 15.0,
				dr: 7.0,
				sl: 15.0,
				sustain: false,
				..op(7.0, 28.0)
			},
			Operator {
				ar: 15.0,
				dr: 6.0,
				sl: 10.0,
				rr: 6.0,
				sustain: false,
				..op(1.0, 0.0)
			},
			0.0,
		),
		PresetId::Pad => fm(
			Operator {
				ar: 6.0,
				dr: 3.0,
				..op(1.0, 38.0)
			},
			Operator {
				ar: 6.0,
				dr: 3.0,
				sl: 2.0,
				rr: 5.0,
				..op(1.0, 4.0)
			},
			2.0,
		),
		PresetId::Organ => Patch {
			modulator: Operator {
				ar: 14.0,
				dr: 0.0,
				sl: 0.0,
				rr: 9.0,
				..op(2.0, 18.0)
			},
			carrier: Operator {
				ar: 14.0,
				dr: 0.0,
				sl: 0.0,
				rr: 9.0,
				..op(1.0, 2.0)
			},
			feedback: 1.0,
			additive: true,
		},
		PresetId::Bass => fm(
			Operator {
				dr: 6.0,
				sl: 6.0,
				..op(1.0, 34.0)
			},
			Operator {
				dr: 5.0,
				sl: 3.0,
				rr: 8.0,
				..op(1.0, 0.0)
			},
			2.0,
		),
		PresetId::SubBass => fm(
			op(1.0, 63.0),
			Operator {
				dr: 3.0,
				sl: 2.0,
				rr: 8.0,
				..op(1.0, 0.0)
			},
			0.0,
		),
		// The names a host supplies patches for: no patch is held here.
		PresetId::BassSaw | PresetId::PadBright | PresetId::Drone => return None,
	})
}

/// The patch for each preset a host has settled, over the ones this crate holds.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchBank {
	patches: HashMap<PresetId, Patch>,
}

impl PatchBank {
	/// The patches this crate holds: every preset but the three a host supplies.
	pub fn standard() -> PatchBank {
		PatchBank {
			patches: PresetId::ALL
				.iter()
				.filter_map(|&preset| standard_patch(preset).map(|patch| (preset, patch)))
				.collect(),
		}
	}

	/// Sets the patch a preset plays, replacing any it had.
	pub fn insert(&mut self, preset: PresetId, patch: Patch) -> &mut PatchBank {
		self.patches.insert(preset, patch);
		self
	}

	/// The patch a preset plays: the bank's, else the `lead` patch, so a song that names a preset
	/// the host has not supplied still sounds.
	pub fn get(&self, preset: PresetId) -> Patch {
		self.patches
			.get(&preset)
			.or_else(|| self.patches.get(&PresetId::Lead))
			.cloned()
			.or_else(|| standard_patch(PresetId::Lead))
			.expect("the lead patch is always held")
	}

	/// Whether the bank holds a patch for the preset.
	pub fn contains(&self, preset: PresetId) -> bool {
		self.patches.contains_key(&preset)
	}
}

impl Default for PatchBank {
	fn default() -> Self {
		PatchBank::standard()
	}
}

/// The chip's five rhythm-mode drums, on channels 6 to 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RhythmDrum {
	/// The bass drum.
	Bass,
	/// The snare drum.
	Snare,
	/// The tom-tom.
	Tom,
	/// The cymbal.
	Cymbal,
	/// The hi-hat.
	Hat,
}

/// Every rhythm drum, in register order.
pub const RHYTHM_DRUMS: [RhythmDrum; 5] = [
	RhythmDrum::Bass,
	RhythmDrum::Snare,
	RhythmDrum::Tom,
	RhythmDrum::Cymbal,
	RhythmDrum::Hat,
];

impl RhythmDrum {
	/// The word the song file and the voice parameters spell this drum with.
	pub fn as_str(self) -> &'static str {
		match self {
			RhythmDrum::Bass => "bass",
			RhythmDrum::Snare => "snare",
			RhythmDrum::Tom => "tom",
			RhythmDrum::Cymbal => "cymbal",
			RhythmDrum::Hat => "hat",
		}
	}

	/// The drum's key bit in register BD, its operator slots, and how long a hit holds in seconds.
	pub fn rhythm(self) -> Rhythm {
		match self {
			RhythmDrum::Bass => Rhythm {
				bit: 0x10,
				ops: &[0x10, 0x13],
				hold: 0.07,
			},
			RhythmDrum::Snare => Rhythm {
				bit: 0x08,
				ops: &[0x14],
				hold: 0.05,
			},
			RhythmDrum::Tom => Rhythm {
				bit: 0x04,
				ops: &[0x12],
				hold: 0.07,
			},
			RhythmDrum::Cymbal => Rhythm {
				bit: 0x02,
				ops: &[0x15],
				hold: 0.1,
			},
			RhythmDrum::Hat => Rhythm {
				bit: 0x01,
				ops: &[0x11],
				hold: 0.025,
			},
		}
	}

	/// The drum's operators, one per slot in `rhythm().ops`.
	pub fn patch(self) -> Vec<Operator> {
		let drum = |mult: f64, tl: f64, dr: f64, rr: f64| Operator {
			ar: 15.0,
			dr,
			sl: 15.0,
			rr,
			sustain: false,
			..op(mult, tl)
		};
		match self {
			RhythmDrum::Bass => vec![drum(0.0, 26.0, 9.0, 7.0), drum(0.0, 6.0, 8.0, 7.0)],
			RhythmDrum::Snare => vec![drum(1.0, 12.0, 10.0, 8.0)],
			RhythmDrum::Tom => vec![drum(1.0, 12.0, 9.0, 8.0)],
			RhythmDrum::Cymbal => vec![drum(1.0, 20.0, 9.0, 6.0)],
			RhythmDrum::Hat => vec![drum(1.0, 22.0, 12.0, 9.0)],
		}
	}
}

#[derive(Debug, Clone, Copy)]
/// How a rhythm drum is keyed and held.
pub struct Rhythm {
	/// The drum's key bit in register BD.
	pub bit: i32,
	/// The drum's operator slots.
	pub ops: &'static [u16],
	/// How long a hit holds, in seconds.
	pub hold: f64,
}

/// Pitches of channels 6, 7 and 8, which tune the bass drum, the hat and snare, and the tom and cymbal.
pub const RHYTHM_CHANNEL_HZ: [f64; 3] = [150.0, 800.0, 300.0];

/// Which rhythm drum plays each kit piece, and how much quieter (in 0.75 dB steps).
pub fn piece_drum(piece: DrumPiece) -> (RhythmDrum, i32) {
	match piece {
		DrumPiece::Kick => (RhythmDrum::Bass, 0),
		DrumPiece::Snare => (RhythmDrum::Snare, 0),
		DrumPiece::Clap => (RhythmDrum::Snare, 3),
		DrumPiece::Rim => (RhythmDrum::Hat, 2),
		DrumPiece::Hat => (RhythmDrum::Hat, 0),
		DrumPiece::Openhat => (RhythmDrum::Cymbal, 4),
		DrumPiece::Tom => (RhythmDrum::Tom, 0),
		DrumPiece::Crash => (RhythmDrum::Cymbal, 0),
	}
}

const OPERATOR_FIELDS: [&str; 12] = [
	"mult", "tl", "ksl", "ar", "dr", "sl", "rr", "sustain", "ksr", "wave", "vibrato", "tremolo",
];

/// An instrument's overrides on an OPL engine. The OPL3 starts from the instrument's OPL2 edits,
/// so a patch made for one chip carries to the other.
pub fn opl_overrides(song: &Song, instrument_id: &str, chip: OplChip) -> Map<String, Value> {
	let empty = Map::new();
	let engines = song
		.instruments
		.get(instrument_id)
		.and_then(|instrument| instrument.engines.as_ref());
	let of = |id: EngineId| engines.and_then(|e| e.get(&id)).unwrap_or(&empty);
	let mut merged = of(EngineId::Opl2).clone();
	if chip == OplChip::Opl3 {
		for (key, value) in of(EngineId::Opl3) {
			merged.insert(key.clone(), value.clone());
		}
	}
	merged
}

fn set_field(operator: &mut Operator, field: &str, value: &Value) {
	let number = js_number(value);
	let flag = js_truthy(value);
	match field {
		"mult" => operator.mult = number,
		"tl" => operator.tl = number,
		"ksl" => operator.ksl = number,
		"ar" => operator.ar = number,
		"dr" => operator.dr = number,
		"sl" => operator.sl = number,
		"rr" => operator.rr = number,
		"sustain" => operator.sustain = flag,
		"ksr" => operator.ksr = flag,
		"wave" => operator.wave = number,
		"vibrato" => operator.vibrato = flag,
		"tremolo" => operator.tremolo = flag,
		_ => {}
	}
}

/// The patch an instrument plays: its preset's from the profile, with the song's `mod.*`, `car.*`, `feedback` and
/// `additive` edits.
pub fn instrument_patch(
	song: &Song,
	instrument_id: &str,
	chip: OplChip,
	profile: &OplProfile,
) -> Patch {
	let preset = song
		.instruments
		.get(instrument_id)
		.map_or(PresetId::Lead, |instrument| instrument.preset);
	let mut patch = profile.patches().get(preset);
	let overrides = opl_overrides(song, instrument_id, chip);
	for (prefix, operator) in [("mod", &mut patch.modulator), ("car", &mut patch.carrier)] {
		for field in OPERATOR_FIELDS {
			match overrides.get(&format!("{prefix}.{field}")) {
				None | Some(Value::Null) => {}
				Some(value) => set_field(operator, field, value),
			}
		}
	}
	if let Some(Value::Number(feedback)) = overrides.get("feedback") {
		patch.feedback = feedback.as_f64().unwrap_or(f64::NAN);
	}
	if let Some(Value::Bool(additive)) = overrides.get("additive") {
		patch.additive = *additive;
	}
	patch
}

/// A kit's level trim for one rhythm drum, from `<drum>.tl` (extra attenuation in 0.75 dB steps).
pub fn drum_trim(song: &Song, instrument_id: &str, chip: OplChip, drum: RhythmDrum) -> f64 {
	match opl_overrides(song, instrument_id, chip).get(&format!("{}.tl", drum.as_str())) {
		Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
		_ => 0.0,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::registers::operator_registers;

	/// The registers a patch writes for its two operators: 20, 40, 60, 80 and E0 for each.
	fn registers(patch: &Patch) -> [i32; 10] {
		let m = operator_registers(&patch.modulator, 3);
		let c = operator_registers(&patch.carrier, 3);
		[
			m.r20, m.r40, m.r60, m.r80, m.re0, c.r20, c.r40, c.r60, c.r80, c.re0,
		]
	}

	#[test]
	fn the_presets_a_host_supplies_hold_no_patch_here() {
		for preset in [PresetId::BassSaw, PresetId::PadBright, PresetId::Drone] {
			assert!(standard_patch(preset).is_none());
			assert!(!PatchBank::standard().contains(preset));
		}
		assert_eq!(PatchBank::standard().patches.len(), 10);
	}

	#[test]
	fn a_preset_without_a_patch_plays_as_lead_until_the_host_supplies_one() {
		let lead = standard_patch(PresetId::Lead).unwrap();
		let mut bank = PatchBank::standard();
		assert_eq!(bank.get(PresetId::Drone), lead);
		let other = standard_patch(PresetId::Bell).unwrap();
		bank.insert(PresetId::Drone, other.clone());
		assert_eq!(bank.get(PresetId::Drone), other);
		assert_eq!(registers(&bank.get(PresetId::Drone)).len(), 10);
	}
}
