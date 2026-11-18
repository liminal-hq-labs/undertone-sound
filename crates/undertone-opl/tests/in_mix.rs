// Checks that a track's level inside the mix is its share of the mix, and not its solo render
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A track's share of the mix: a track the chip drops reads silent in the mix and loud alone.

use serde_json::{Value, json};
use undertone_opl::engine::{ChipEngine, RenderRequest, render_performance, render_track_in_mix};
use undertone_opl::profile::OplProfile;
use undertone_sound::compile::{CompileOptions, compile_song};
use undertone_sound::model::Song;

/// The host's reserved channels, which leave the OPL2 its five melodic channels.
fn profile() -> OplProfile {
	OplProfile::new().reserve([5, 14])
}

const REQUEST: RenderRequest = RenderRequest {
	sample_rate: 22_050,
	tail: 0.5,
	wrap: false,
};

fn peak(left: &[f32], right: &[f32]) -> f64 {
	left.iter()
		.chain(right)
		.fold(0.0_f64, |peak, sample| peak.max(f64::from(sample.abs())))
}

fn track(id: &str, role: &str, priority: Option<u8>) -> Value {
	let mut track = json!({
		"id": id, "name": id, "role": role, "instrument": id, "volume": 0.8, "pan": 0
	});
	if let Some(priority) = priority {
		track["priority"] = json!(priority);
	}
	track
}

fn take(notes: Vec<Value>) -> Value {
	json!({ "active": "a", "takes": [{ "id": "a", "name": "A", "by": "you", "notes": notes }] })
}

/// A pad of five held notes (every OPL2 channel) and a lead note from step 4; `pad` and `lead`
/// are the tracks' priorities. With the pad at 9 the lead's note is lost; with the lead above it,
/// the pad's note is cut and the lead sounds.
fn contest(pad: Option<u8>, lead: Option<u8>, engine: &str) -> Song {
	let held: Vec<Value> = ["c3", "e3", "g3", "b3", "d4"]
		.iter()
		.map(|pitch| json!({ "at": 0, "pitch": pitch, "len": 12 }))
		.collect();
	serde_json::from_value(json!({
		"format": "fizz-audio-studio/song", "version": 1, "title": "Contest", "tempo": 120,
		"beatsPerBar": 4, "stepsPerBeat": 4, "key": { "root": "c", "scale": "major" },
		"engine": engine,
		"instruments": {
			"pad": { "name": "Pad", "preset": "pad" },
			"lead": { "name": "Lead", "preset": "lead" }
		},
		"tracks": [track("pad", "harmony", pad), track("lead", "melody", lead)],
		"sections": [{ "id": "a", "name": "A", "bars": 1 }],
		"arrangement": ["a"],
		"clips": {
			"pad": { "a": take(held) },
			"lead": { "a": take(vec![json!({ "at": 4, "pitch": "a4", "len": 4 })]) }
		},
		"messages": []
	}))
	.expect("a song")
}

struct Readings {
	solo: f64,
	in_mix: f64,
}

fn readings(song: &Song, engine: ChipEngine, track_id: &str) -> Readings {
	let performance = compile_song(song, &CompileOptions::default()).unwrap();
	let mix = render_performance(song, &performance, engine, &profile(), REQUEST);
	let mut alone = performance.clone();
	alone.events.retain(|event| event.track_id == track_id);
	let solo = render_performance(song, &alone, engine, &profile(), REQUEST);
	let share = render_track_in_mix(
		song,
		&performance,
		engine,
		&profile(),
		REQUEST,
		track_id,
		&mix,
	);
	Readings {
		solo: peak(&solo.left, &solo.right),
		in_mix: peak(&share.left, &share.right),
	}
}

#[test]
fn a_track_the_chip_drops_is_loud_alone_and_silent_in_the_mix() {
	// The pad outranks the lead, so the lead's one note never starts on five channels.
	let song = contest(Some(9), Some(1), "opl2");
	let lead = readings(&song, ChipEngine::Opl2, "lead");
	assert!(lead.solo > 0.01, "alone the lead sounds ({})", lead.solo);
	assert!(
		lead.in_mix < lead.solo * 0.02,
		"in the mix the lead is lost, but reads {} against {} alone",
		lead.in_mix,
		lead.solo
	);
	let pad = readings(&song, ChipEngine::Opl2, "pad");
	assert!(
		pad.in_mix > 0.01,
		"the pad carries the mix ({})",
		pad.in_mix
	);
}

#[test]
fn a_track_that_steals_is_as_loud_in_the_mix_as_alone() {
	let song = contest(None, None, "opl2");
	let lead = readings(&song, ChipEngine::Opl2, "lead");
	assert!(
		lead.in_mix > lead.solo * 0.8,
		"{} vs {}",
		lead.in_mix,
		lead.solo
	);
	// The pad lost a note to the steal, so it is a little quieter in the mix than alone.
	let pad = readings(&song, ChipEngine::Opl2, "pad");
	assert!(pad.in_mix > 0.01);
}

#[test]
fn the_tracks_shares_add_up_to_the_mix_when_nothing_is_lost() {
	// On the OPL3 thirteen channels hold everything: no steals, so the parts sum to the whole.
	let song = contest(None, None, "opl3");
	let performance = compile_song(&song, &CompileOptions::default()).unwrap();
	let mix = render_performance(&song, &performance, ChipEngine::Opl3, &profile(), REQUEST);
	let shares: Vec<_> = ["pad", "lead"]
		.iter()
		.map(|id| {
			render_track_in_mix(
				&song,
				&performance,
				ChipEngine::Opl3,
				&profile(),
				REQUEST,
				id,
				&mix,
			)
		})
		.collect();
	let whole = peak(&mix.left, &mix.right);
	let worst = (0..mix.left.len())
		.map(|i| {
			let sum: f32 = shares.iter().map(|share| share.left[i]).sum();
			f64::from((sum - mix.left[i]).abs())
		})
		.fold(0.0, f64::max);
	assert!(
		worst < whole * 0.02,
		"the shares differ from the mix by {worst} of a peak of {whole}"
	);
}

#[test]
fn on_the_speaker_a_track_is_the_frames_it_wins() {
	// One voice: the lead outranks the pad whenever it sounds, so the pad is silent then.
	let song = contest(None, None, "pc-speaker");
	let performance = compile_song(&song, &CompileOptions::default()).unwrap();
	let mix = render_performance(
		&song,
		&performance,
		ChipEngine::PcSpeaker,
		&profile(),
		REQUEST,
	);
	let pad = render_track_in_mix(
		&song,
		&performance,
		ChipEngine::PcSpeaker,
		&profile(),
		REQUEST,
		"pad",
		&mix,
	);
	let lead = render_track_in_mix(
		&song,
		&performance,
		ChipEngine::PcSpeaker,
		&profile(),
		REQUEST,
		"lead",
		&mix,
	);
	assert!(peak(&pad.left, &pad.right) > 0.01);
	assert!(peak(&lead.left, &lead.right) > 0.01);
	assert_eq!(pad.left.len(), mix.left.len());
	// No frame is shared: where the lead sounds the pad does not.
	// At 120 bpm with four steps a beat the lead holds from 0.5 s to 1.0 s.
	let rate = f64::from(REQUEST.sample_rate);
	let (lead_from, lead_to) = ((rate * 0.55) as usize, (rate * 0.95) as usize);
	let both = (lead_from..lead_to)
		.filter(|&i| pad.left[i].abs() > 1e-4 && lead.left[i].abs() > 1e-4)
		.count();
	assert_eq!(both, 0, "the two shares overlap in {both} samples");
}

#[test]
fn the_plain_sequencer_is_unchanged_by_the_hushing_one() {
	use undertone_opl::sequence::{OplChip, sequence_opl, sequence_opl_hushing};
	let song = contest(None, None, "opl2");
	let performance = compile_song(&song, &CompileOptions::default()).unwrap();
	let plain = sequence_opl(&song, &performance, OplChip::Opl2, &profile());
	let nobody = sequence_opl_hushing(
		&song,
		&performance,
		OplChip::Opl2,
		&profile(),
		Some("nobody"),
	);
	assert_eq!(plain, nobody);
	// Hushing a track changes only its level writes, never which channel plays what.
	let hushed = sequence_opl_hushing(&song, &performance, OplChip::Opl2, &profile(), Some("lead"));
	assert_eq!(plain.dropped_events, hushed.dropped_events);
	assert_eq!(plain.cut_events, hushed.cut_events);
	assert_eq!(plain.writes.len(), hushed.writes.len());
}
