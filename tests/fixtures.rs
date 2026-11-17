// Checks the chips against the golden files in `fixtures/chips/`, which today's TypeScript wrote
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use fizz_audio_chips::engine::{
    placement, render_song, voice_params, voice_values, ChipEngine, RenderRequest,
};
use fizz_audio_chips::limits::analyse_limits_on;
use fizz_audio_chips::registers::{attenuation, attenuation_from, fnumber, midi_to_hz};
use fizz_audio_chips::sequence::{melodic_channels, sequence_opl, OplChip, SFX_CHANNELS};
use fizz_audio_chips::speaker::sequence::{pit_hz, sequence_speaker};
use fizz_audio_chips::stereo::{
    centre_gain, equal_power_gains, hard_gain, opl_placement, pan_gains, pan_output, stereo_plan,
    CENTRE_WIDTH, DEFAULT_PAN_LAW, HARD_FROM,
};
use fizz_audio_song::compile::{compile_song, CompileOptions};
use fizz_audio_song::model::{Mix, Song};
use serde_json::{json, Value};

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
    let validation = fizz_audio_song::file::parse_song_file(&fs::read_to_string(path).unwrap());
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
        let sequence = sequence_opl(&song, &performance, chip);
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
fn opl2_register_writes_match() {
    opl_sequences("opl2.json", OplChip::Opl2);
}

#[test]
fn opl3_register_writes_match() {
    opl_sequences("opl3.json", OplChip::Opl3);
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
        let found = analyse_limits_on(&song, chip_of(case["chip"].as_str().unwrap())).unwrap();
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
        let got = json!({
            "engineName": found.engine_name,
            "capacity": found.capacity,
            "total": found.total,
            "dropped": found.dropped,
            "byTrack": found.by_track.iter().map(|t| json!({"trackId": t.track_id, "name": t.name, "count": t.count})).collect::<Vec<_>>(),
            "bySlot": by_slot,
            "demand": join(found.demand.iter().map(u8::to_string).collect::<Vec<_>>().join(",")),
            "lost": found.lost.iter().map(u16::to_string).collect::<Vec<_>>().join(","),
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
fn voice_parameters_and_values_match() {
    let mut failures = Vec::new();
    let params = load("voice-params.json");
    let mut by_kind = BTreeMap::new();
    for case in &params {
        let id = case["id"].as_str().unwrap();
        let song = song(case["input"].as_str().unwrap());
        let engine = engine_of(case["engine"].as_str().unwrap());
        let got = serde_json::to_value(voice_params(
            &song,
            case["instrument"].as_str().unwrap(),
            engine,
        ))
        .unwrap();
        expect(
            &mut failures,
            id,
            close(&got, &case["params"], 0.0),
            "parameters",
        );
        by_kind.insert(id.to_string(), case["params"].clone());
    }
    let values = load("voice-values.json");
    for case in &values {
        let id = case["id"].as_str().unwrap();
        let song = song(case["input"].as_str().unwrap());
        let engine = engine_of(case["engine"].as_str().unwrap());
        let instrument = case["instrument"].as_str().unwrap();
        let got = Value::Object(voice_values(&song, instrument, engine));
        expect(
            &mut failures,
            id,
            close(&got, &case["values"], 0.0),
            "values",
        );
        // Every instrument's parameters are those of its kind.
        let key = format!(
            "{}:{}",
            case["engine"].as_str().unwrap(),
            case["kind"].as_str().unwrap()
        );
        let params = serde_json::to_value(voice_params(&song, instrument, engine)).unwrap();
        expect(
            &mut failures,
            id,
            close(&params, &by_kind[&key], 0.0),
            "parameters of its kind",
        );
    }
    finish("voice", params.len() + values.len(), failures);
}

#[test]
fn pan_law_matches() {
    let cases = load("pan.json");
    let mut failures = Vec::new();
    let channel =
        |c: &fizz_audio_chips::stereo::PlacedChannel| json!({"output": c.output, "gain": c.gain});
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let pan = case["pan"].as_f64().unwrap();
        let law = case["law"].as_f64();
        let effective = law.unwrap_or(DEFAULT_PAN_LAW);
        let (left, right) = pan_gains(pan, effective);
        let (el, er) = equal_power_gains(pan);
        let plan = stereo_plan(pan, effective);
        let placed = opl_placement(OplChip::Opl3, pan, effective);
        let mix = law.map(|l| Mix { pan_law: Some(l) });
        let placements: serde_json::Map<String, Value> = ["opl2", "opl3", "pc-speaker"]
            .iter()
            .map(|name| {
                let p = placement(engine_of(name), pan, mix.as_ref());
                (
                    name.to_string(),
                    json!({"label": p.label, "detail": p.detail, "heard": p.heard}),
                )
            })
            .collect();
        let got = json!({
            "id": id, "pan": pan, "law": law,
            "panOutput": pan_output(pan),
            "panGains": {"left": left, "right": right},
            "equalPower": {"left": el, "right": er},
            "centreGain": centre_gain(effective),
            "hardGain": hard_gain(effective),
            "plan": {"channels": plan.channels.iter().map(channel).collect::<Vec<_>>(), "fallback": channel(&plan.fallback)},
            "placement": {"label": placed.label, "detail": placed.detail, "channels": placed.channels},
            "placements": placements,
        });
        expect(&mut failures, id, close(&got, case, 1e-12), "pan law");
    }
    finish("pan law", cases.len(), failures);
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
    if channels(&table["sfxChannels"]) != SFX_CHANNELS {
        fail("sfx channels".into());
    }
    if channels(&table["melodicChannels"]["opl2"]) != melodic_channels(OplChip::Opl2)
        || channels(&table["melodicChannels"]["opl3"]) != melodic_channels(OplChip::Opl3)
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
fn rendered_samples_match_the_webassembly_chip() {
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
