// Checks the OPL and speaker code against the recorded files in `fixtures/chips/`
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Each file holds cases the TypeScript implementation recorded: register writes, speaker
//! frames, lost notes, the pan law, register tables and rendered samples. A test reads one file
//! and requires every case to match; a failure names the case.
//!
//! The recorded songs were made for a game that keeps channels 5 and 14 free for sound effects
//! and has three patches of its own. This file plays the host: it reserves those channels and
//! supplies those patches through an [`OplProfile`], exactly as a host would.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Value, json};
use undertone_opl::engine::{ChipEngine, RenderRequest, render_song};
use undertone_opl::limits::analyse_limits_on;
use undertone_opl::profile::OplProfile;
use undertone_opl::registers::{
	Operator, Patch, attenuation, attenuation_from, fnumber, midi_to_hz,
};
use undertone_opl::sequence::{OplChip, melodic_channels, sequence_opl};
use undertone_opl::speaker::sequence::{pit_hz, sequence_speaker};
use undertone_opl::stereo::{
	CENTRE_WIDTH, DEFAULT_PAN_LAW, HARD_FROM, centre_gain, equal_power_gains, hard_gain,
	opl_placement, pan_gains, pan_output, stereo_plan,
};
use undertone_sound::compile::{CompileOptions, compile_song};
use undertone_sound::model::{PresetId, Song};
use undertone_sound::validate::validate_song;

/// The channels the recorded songs keep free.
const RESERVED: [usize; 2] = [5, 14];

/// An operator with the frequency multiple and total level set and every other field its base.
fn operator(mult: f64, tl: f64) -> Operator {
	Operator {
		mult,
		tl,
		..Operator::BASE
	}
}

fn fm(modulator: Operator, carrier: Operator, feedback: f64) -> Patch {
	Patch {
		modulator,
		carrier,
		feedback,
		additive: false,
	}
}

/// The song that plays three sounds of the host's own: the host replaces the patches of the
/// presets `bass`, `pad` and `organ`, for this song only, and every other song hears the standard
/// ones.
const HOST_PATCHES_SONG: &str = "chips-host-patches";

/// The host's profile: the reserved channels, and for the host-patches song the three patches the
/// host plays in place of the standard ones.
fn profile_for(input: &str) -> OplProfile {
	let profile = OplProfile::new().reserve(RESERVED);
	if input == HOST_PATCHES_SONG {
		host_patches(profile)
	} else {
		profile
	}
}

/// The reserved channels alone, for the checks that name no song.
fn profile() -> OplProfile {
	profile_for("")
}

fn host_patches(profile: OplProfile) -> OplProfile {
	profile
		.with_patch(
			PresetId::Bass,
			fm(
				Operator {
					dr: 6.0,
					sl: 5.0,
					..operator(1.0, 20.0)
				},
				Operator {
					dr: 5.0,
					sl: 3.0,
					rr: 8.0,
					..operator(1.0, 0.0)
				},
				6.0,
			),
		)
		.with_patch(
			PresetId::Pad,
			fm(
				Operator {
					ar: 7.0,
					dr: 3.0,
					..operator(2.0, 30.0)
				},
				Operator {
					ar: 7.0,
					dr: 3.0,
					sl: 2.0,
					rr: 5.0,
					..operator(1.0, 4.0)
				},
				4.0,
			),
		)
		.with_patch(
			PresetId::Organ,
			fm(
				Operator {
					ar: 8.0,
					dr: 3.0,
					..operator(1.0, 22.0)
				},
				Operator {
					ar: 8.0,
					dr: 3.0,
					sl: 2.0,
					rr: 5.0,
					..operator(1.0, 2.0)
				},
				7.0,
			),
		)
}

fn fixtures() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn load(name: &str) -> Vec<Value> {
	let text = fs::read_to_string(fixtures().join("chips").join(name)).unwrap();
	match serde_json::from_str::<Value>(&text).unwrap() {
		Value::Array(items) => items,
		other => vec![other],
	}
}

fn song(name: &str) -> Song {
	let chips = fixtures()
		.join("chips/inputs")
		.join(format!("{name}.song.json"));
	let path = if chips.exists() {
		chips
	} else {
		fixtures()
			.join("song/inputs")
			.join(format!("{name}.song.json"))
	};
	let raw: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
	let validation = validate_song(&raw);
	validation
		.song
		.unwrap_or_else(|| panic!("{name} does not validate: {:?}", validation.problems))
}

fn options(value: &Value) -> CompileOptions {
	CompileOptions {
		only: value["only"].as_i64().map(|n| n as i32),
		section: value["section"].as_str().map(str::to_string),
		takes: Default::default(),
	}
}

fn chip_of(name: &str) -> OplChip {
	if name == "opl2" {
		OplChip::Opl2
	} else {
		OplChip::Opl3
	}
}
fn engine_of(name: &str) -> ChipEngine {
	match name {
		"opl2" => ChipEngine::Opl2,
		"opl3" => ChipEngine::Opl3,
		_ => ChipEngine::PcSpeaker,
	}
}

/// The largest difference seen between a number here and its golden twin, over the whole run.
static WORST: Mutex<BTreeMap<&'static str, f64>> = Mutex::new(BTreeMap::new());

fn note(kind: &'static str, diff: f64) {
	let mut worst = WORST.lock().unwrap();
	let entry = worst.entry(kind).or_insert(0.0);
	if diff > *entry {
		*entry = diff;
	}
}

/// Equality of JSON values where numbers may differ by `tol` (relative to the larger of 1 and the number).
fn close(a: &Value, b: &Value, tol: f64) -> bool {
	match (a, b) {
		(Value::Number(x), Value::Number(y)) => {
			let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
			(x - y).abs() <= tol * x.abs().max(y.abs()).max(1.0)
		}
		(Value::Array(x), Value::Array(y)) => {
			x.len() == y.len() && x.iter().zip(y).all(|(p, q)| close(p, q, tol))
		}
		(Value::Object(x), Value::Object(y)) => {
			x.len() == y.len()
				&& x.iter()
					.all(|(k, v)| y.get(k).is_some_and(|w| close(v, w, tol)))
		}
		_ => a == b,
	}
}

fn expect(failures: &mut Vec<String>, id: &str, ok: bool, what: &str) {
	if !ok {
		failures.push(format!("{id}: {what}"));
	}
}

fn finish(kind: &str, count: usize, failures: Vec<String>) {
	assert!(
		failures.is_empty(),
		"{} of {count} {kind} cases failed:\n{}",
		failures.len(),
		failures
			.iter()
			.take(20)
			.cloned()
			.collect::<Vec<_>>()
			.join("\n")
	);
	println!("{kind}: {count} cases match");
}

fn opl_sequences(file: &str, chip: OplChip) {
	let cases = load(file);
	let mut failures = Vec::new();
	for case in &cases {
		let id = case["id"].as_str().unwrap();
		let song = song(case["input"].as_str().unwrap());
		let performance = compile_song(&song, &options(&case["options"])).unwrap();
		let sequence = sequence_opl(
			&song,
			&performance,
			chip,
			&profile_for(case["input"].as_str().unwrap()),
		);
		expect(
			&mut failures,
			id,
			sequence.seconds == case["seconds"].as_f64().unwrap(),
			"seconds",
		);
		expect(
			&mut failures,
			id,
			sequence.dropped as u64 == case["dropped"].as_u64().unwrap(),
			"dropped count",
		);
		let dropped: Vec<usize> = sequence
			.dropped_events
			.iter()
			.map(|event| performance.events.iter().position(|e| e == event).unwrap())
			.collect();
		let wanted: Vec<usize> = case["droppedEvents"]
			.as_array()
			.unwrap()
			.iter()
			.map(|n| n.as_u64().unwrap() as usize)
			.collect();
		// Equal events share a position, so compare the events the indexes name.
		let same_events = dropped.len() == wanted.len()
			&& dropped
				.iter()
				.zip(&wanted)
				.all(|(a, b)| performance.events[*a] == performance.events[*b]);
		expect(&mut failures, id, same_events, "dropped events");
		let writes = case["writes"].as_array().unwrap();
		if writes.len() != sequence.writes.len() {
			failures.push(format!(
				"{id}: {} writes, wanted {}",
				sequence.writes.len(),
				writes.len()
			));
			continue;
		}
		for (i, (got, want)) in sequence.writes.iter().zip(writes).enumerate() {
			let ok = got.time == want[0].as_f64().unwrap()
				&& i64::from(got.register) == want[1].as_i64().unwrap()
				&& i64::from(got.value) == want[2].as_i64().unwrap();
			if !ok {
				failures.push(format!("{id}: write {i} is {got:?}, wanted {want}"));
				break;
			}
		}
	}
	finish(file, cases.len(), failures);
}

#[test]
fn speaker_frames_match() {
	let cases = load("speaker.json");
	let mut failures = Vec::new();
	for case in &cases {
		let id = case["id"].as_str().unwrap();
		let song = song(case["input"].as_str().unwrap());
		let performance = compile_song(&song, &options(&case["options"])).unwrap();
		let frames = sequence_speaker(&song, &performance);
		let mut want: Vec<f32> = Vec::new();
		for run in case["runs"].as_array().unwrap() {
			let hz = run[0].as_f64().unwrap() as f32;
			want.extend(std::iter::repeat_n(hz, run[1].as_u64().unwrap() as usize));
		}
		expect(
			&mut failures,
			id,
			frames.len() as u64 == case["frames"].as_u64().unwrap(),
			"length",
		);
		if frames != want {
			let at = frames.iter().zip(&want).position(|(a, b)| a != b);
			failures.push(format!("{id}: frames differ at {at:?}"));
		}
	}
	finish("speaker", cases.len(), failures);
}

#[test]
fn lost_notes_match() {
	let cases = load("lost.json");
	let mut failures = Vec::new();
	for case in &cases {
		let id = case["id"].as_str().unwrap();
		let song = song(case["input"].as_str().unwrap());
		let found = analyse_limits_on(
			&song,
			chip_of(case["chip"].as_str().unwrap()),
			&profile_for(case["input"].as_str().unwrap()),
		)
		.unwrap();
		let want = &case["lost"];
		let Some(found) = found else {
			expect(
				&mut failures,
				id,
				want.is_null(),
				"expected lost notes, found none",
			);
			continue;
		};
		let join = |items: String| items;
		let by_slot: serde_json::Map<String, Value> = found
			.by_slot
			.iter()
			.map(|(k, v)| (k.clone(), json!(v.iter().collect::<Vec<_>>())))
			.collect();
		let cut_by_slot: serde_json::Map<String, Value> = found
			.cut_by_slot
			.iter()
			.map(|(k, v)| (k.clone(), json!(v.iter().collect::<Vec<_>>())))
			.collect();
		let tracks = |losses: &[undertone_opl::limits::TrackLoss]| {
			losses
				.iter()
				.map(|t| json!({"trackId": t.track_id, "name": t.name, "count": t.count}))
				.collect::<Vec<_>>()
		};
		let got = json!({
			"engineName": found.engine_name,
			"capacity": found.capacity,
			"total": found.total,
			"dropped": found.dropped,
			"byTrack": tracks(&found.by_track),
			"bySlot": by_slot,
			"cut": found.cut,
			"cutByTrack": tracks(&found.cut_by_track),
			"cutBySlot": cut_by_slot,
			"demand": join(found.demand.iter().map(u8::to_string).collect::<Vec<_>>().join(",")),
			"lost": found.lost.iter().map(u16::to_string).collect::<Vec<_>>().join(","),
			"cutAt": found.cut_at.iter().map(u16::to_string).collect::<Vec<_>>().join(","),
		});
		expect(
			&mut failures,
			id,
			close(&got, want, 0.0),
			"lost-notes analysis",
		);
	}
	finish("lost notes", cases.len(), failures);
}

#[test]
fn opl2_register_writes_match() {
	opl_sequences("opl2.json", OplChip::Opl2);
}

#[test]
fn opl3_register_writes_match() {
	opl_sequences("opl3.json", OplChip::Opl3);
}

#[test]
fn registers_and_tables_match() {
	let table = load("registers.json").remove(0);
	let mut failures = Vec::new();
	let mut fail = |what: String| failures.push(what);
	let rel = |a: f64, b: f64| (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0);
	if table["centreWidth"].as_f64() != Some(CENTRE_WIDTH)
		|| table["hardFrom"].as_f64() != Some(HARD_FROM)
	{
		fail("pan thresholds".into());
	}
	let channels = |v: &Value| {
		v.as_array()
			.unwrap()
			.iter()
			.map(|n| n.as_u64().unwrap() as usize)
			.collect::<Vec<_>>()
	};
	if channels(&table["sfxChannels"]) != profile().reserved_channels() {
		fail("reserved channels".into());
	}
	if channels(&table["melodicChannels"]["opl2"]) != melodic_channels(OplChip::Opl2, &profile())
		|| channels(&table["melodicChannels"]["opl3"])
			!= melodic_channels(OplChip::Opl3, &profile())
	{
		fail("melodic channels".into());
	}
	for pair in table["midiToHz"].as_array().unwrap() {
		let midi = pair[0].as_f64().unwrap();
		let (got, want) = (midi_to_hz(midi), pair[1].as_f64().unwrap());
		note("midi_to_hz", (got - want).abs() / want);
		if !rel(got, want) {
			fail(format!("midiToHz({midi}): {got} != {want}"));
		}
	}
	for row in table["fnumber"].as_array().unwrap() {
		let hz = row["hz"].as_f64().unwrap();
		let (fnum, block) = fnumber(hz);
		if i64::from(fnum) != row["fnum"].as_i64().unwrap()
			|| i64::from(block) != row["block"].as_i64().unwrap()
		{
			fail(format!("fnumber({hz}): {fnum} {block} != {row}"));
		}
	}
	for pair in table["pitHz"].as_array().unwrap() {
		let hz = pair[0].as_f64().unwrap();
		let (got, want) = (pit_hz(hz), pair[1].as_f64().unwrap());
		if !rel(got, want) {
			fail(format!("pitHz({hz}): {got} != {want}"));
		}
	}
	for pair in table["attenuation"].as_array().unwrap() {
		let gain = pair[0].as_f64().unwrap();
		if i64::from(attenuation(gain)) != pair[1].as_i64().unwrap() {
			fail(format!("attenuation({gain})"));
		}
	}
	for pair in table["attenuationRelative"].as_array().unwrap() {
		let gain = pair[0].as_f64().unwrap();
		if i64::from(attenuation_from(gain, 0.5)) != pair[1].as_i64().unwrap() {
			fail(format!("attenuation({gain}, 0.5)"));
		}
	}
	finish("registers", 1, failures);
}

#[test]
fn pan_law_matches() {
	let cases = load("pan.json");
	let mut failures = Vec::new();
	let channel =
		|c: &undertone_opl::stereo::PlacedChannel| json!({"output": c.output, "gain": c.gain});
	for case in &cases {
		let id = case["id"].as_str().unwrap();
		let pan = case["pan"].as_f64().unwrap();
		let law = case["law"].as_f64();
		let effective = law.unwrap_or(DEFAULT_PAN_LAW);
		let (left, right) = pan_gains(pan, effective);
		let (el, er) = equal_power_gains(pan);
		let plan = stereo_plan(pan, effective);
		let placed = opl_placement(OplChip::Opl3, pan, effective);
		let got = json!({
			"id": id, "pan": pan, "law": law,
			"panOutput": pan_output(pan),
			"panGains": {"left": left, "right": right},
			"equalPower": {"left": el, "right": er},
			"centreGain": centre_gain(effective),
			"hardGain": hard_gain(effective),
			"plan": {"channels": plan.channels.iter().map(channel).collect::<Vec<_>>(), "fallback": channel(&plan.fallback)},
			"placement": {"label": placed.label, "detail": placed.detail, "channels": placed.channels},
		});
		// The recorded `placements` are the studio's readouts for its UI; nothing here makes them.
		let mut want = case.clone();
		want.as_object_mut().unwrap().remove("placements");
		expect(&mut failures, id, close(&got, &want, 1e-12), "pan law");
	}
	finish("pan law", cases.len(), failures);
}

fn parse_samples(text: &str) -> Vec<f32> {
	text.split_whitespace()
		.map(|n| n.parse().unwrap())
		.collect()
}

fn stats(channel: &[f32], start: usize, end: usize) -> (f64, f64) {
	let mut peak = 0f64;
	let mut sum = 0f64;
	for &s in &channel[start..end] {
		let s = f64::from(s);
		peak = peak.max(s.abs());
		sum += s * s;
	}
	(peak, (sum / (end - start) as f64).sqrt())
}

#[test]
fn rendered_samples_match_the_recorded_chip() {
	let cases = load("render.json");
	let mut failures = Vec::new();
	let mut worst = 0f64;
	for case in &cases {
		let id = case["id"].as_str().unwrap();
		let song = song(case["input"].as_str().unwrap());
		let engine = engine_of(case["engine"].as_str().unwrap());
		let sound = render_song(
			&song,
			&options(&case["options"]),
			engine,
			&profile_for(case["input"].as_str().unwrap()),
			RenderRequest {
				sample_rate: case["sampleRate"].as_u64().unwrap() as u32,
				tail: case["tail"].as_f64().unwrap(),
				wrap: case["wrap"].as_bool().unwrap(),
			},
		)
		.unwrap();
		if sound.left.len() as u64 != case["frames"].as_u64().unwrap() {
			failures.push(format!(
				"{id}: {} frames, wanted {}",
				sound.left.len(),
				case["frames"]
			));
			continue;
		}
		let block = case["blockFrames"].as_u64().unwrap() as usize;
		for (n, want) in case["blocks"].as_array().unwrap().iter().enumerate() {
			let start = n * block;
			let end = sound.left.len().min(start + block);
			let (lp, lr) = stats(&sound.left, start, end);
			let (rp, rr) = stats(&sound.right, start, end);
			for (got, want) in [lp, lr, rp, rr].iter().zip(want.as_array().unwrap()) {
				worst = worst.max((got - want.as_f64().unwrap()).abs());
			}
		}
		let excerpt = &case["excerpt"];
		let start = excerpt["start"].as_u64().unwrap() as usize;
		let left = parse_samples(excerpt["left"].as_str().unwrap());
		let right = excerpt["right"].as_str().map(parse_samples);
		if left.len() as u64 != excerpt["frames"].as_u64().unwrap() {
			failures.push(format!("{id}: excerpt length"));
		}
		for (i, want) in left.iter().enumerate() {
			worst = worst.max(f64::from((sound.left[start + i] - want).abs()));
		}
		match right {
			Some(right) => {
				for (i, want) in right.iter().enumerate() {
					worst = worst.max(f64::from((sound.right[start + i] - want).abs()));
				}
			}
			None => expect(
				&mut failures,
				id,
				sound.left == sound.right,
				"mono channels equal",
			),
		}
		println!("{id}: worst so far {worst:e}");
	}
	note("samples", worst);
	println!("largest sample difference: {worst:e}");
	assert!(worst < 1e-6, "samples differ by up to {worst}");
	finish("render", cases.len(), failures);
}
