// Checks the recorded Undertone graph against `fixtures/undertone/graph.json`, which the TypeScript wrote
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Each case in the file names a song in `fixtures/undertone/inputs/` and how much of it to play.
//! The port compiles it, makes the score, books the pass on a `LogGraph` and writes the case as
//! `tools/fixtures/undertone.ts` does; the whole file must come out byte for byte. When it does
//! not, the test names the first case, node or buffer that differs.

use std::fs;
use std::path::PathBuf;

use fizz_audio_song::compile::{compile_song, CompileOptions, Performance};
use fizz_audio_song::model::Song;
use fizz_audio_undertone::json::Json;
use fizz_audio_undertone::log::{num, round};
use fizz_audio_undertone::render::RenderRequest;
use fizz_audio_undertone::score::{ScoreEvent, ScorePitch};
use fizz_audio_undertone::studio::{render_graph, to_score, to_score_events};
use serde_json::Value;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/undertone")
}

fn song(name: &str) -> Song {
    let text =
        fs::read_to_string(fixtures().join("inputs").join(format!("{name}.song.json"))).unwrap();
    let validation = fizz_audio_song::file::parse_song_file(&text);
    validation
        .song
        .unwrap_or_else(|| panic!("{name} does not validate: {:?}", validation.problems))
}

/// `firstBars`: the events that start in the first `bars` bars, and that length.
fn first_bars(song: &Song, performance: Performance, bars: f64) -> Performance {
    let steps = bars * f64::from(song.beats_per_bar) * f64::from(song.steps_per_beat);
    Performance {
        length_steps: steps.min(performance.length_steps),
        events: performance
            .events
            .into_iter()
            .filter(|event| event.step < steps)
            .collect(),
        ..performance
    }
}

/// `describeScore`: one line an event, each distinct voice once, named in order of first use.
fn describe_score(events: &[ScoreEvent]) -> (Json, Json) {
    let mut voices: Vec<(String, Json)> = Vec::new();
    let lines = events
        .iter()
        .map(|event| {
            let voice = Json::Object(
                event
                    .voice
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::from_value(v)))
                    .collect(),
            );
            let key = voice.compact();
            let index = match voices.iter().position(|(k, _)| *k == key) {
                Some(index) => index,
                None => {
                    voices.push((key, voice));
                    voices.len() - 1
                }
            };
            let pitch = match &event.pitch {
                ScorePitch::Midi(midi) => {
                    format!("midi {}", fizz_audio_song::js::number_to_string(*midi))
                }
                ScorePitch::Note(pitch) => format!("note {}", pitch.text()),
            };
            let level = event
                .level
                .map(|l| format!(" level {}", num(l)))
                .unwrap_or_default();
            let pan = event
                .pan
                .map(|p| format!(" pan {}", num(p)))
                .unwrap_or_default();
            Json::text(format!(
                "tick {} {pitch} gate {}{level}{pan} voice v{index}",
                fizz_audio_song::js::number_to_string(event.tick),
                fizz_audio_song::js::number_to_string(event.gate)
            ))
        })
        .collect();
    let voices = voices
        .into_iter()
        .enumerate()
        .map(|(index, (_, voice))| {
            let Json::Object(fields) = voice else {
                unreachable!()
            };
            let mut entries = vec![("id".to_string(), Json::text(format!("v{index}")))];
            entries.extend(fields);
            Json::Object(entries)
        })
        .collect();
    (Json::Array(lines), Json::Array(voices))
}

/// The case as the TypeScript writes it, from the case's own `id`, `from`, `input` and `bars`.
fn build_case(case: &Value) -> Json {
    let input = case["input"].as_str().unwrap();
    let song = song(input);
    let compiled = compile_song(&song, &CompileOptions::default()).unwrap();
    let performance = match case["bars"].as_f64() {
        Some(bars) => first_bars(&song, compiled, bars),
        None => compiled,
    };
    let request = RenderRequest::EXPORT;
    let score = to_score(&song, &performance).unwrap();
    let events = to_score_events(&song, &performance).unwrap();
    let (lines, voices) = describe_score(&events);
    let graph = render_graph(&song, &performance, &request).unwrap();
    Json::object([
        ("id", Json::from_value(&case["id"])),
        ("from", Json::from_value(&case["from"])),
        ("input", Json::text(input)),
        ("bars", Json::from_value(&case["bars"])),
        (
            "render",
            Json::object([
                ("tail", Json::Number(request.tail)),
                ("channels", Json::Number(f64::from(request.channels))),
                ("seed", Json::Number(request.seed)),
                ("master", Json::Bool(request.master)),
            ]),
        ),
        (
            "score",
            Json::object([
                ("seconds", Json::Number(round(score.seconds))),
                ("cps", Json::Number(round(score.cps))),
                ("events", lines),
                ("voices", voices),
            ]),
        ),
        ("graph", graph.describe()),
    ])
}

/// The first line where two texts differ, with a little context, for a failure message.
fn first_difference(expected: &str, actual: &str) -> String {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    for (index, (e, a)) in expected.iter().zip(&actual).enumerate() {
        if e != a {
            let from = index.saturating_sub(8);
            return format!(
                "line {}:\n  expected: {e}\n  actual:   {a}\ncontext:\n{}",
                index + 1,
                expected[from..=index].join("\n")
            );
        }
    }
    format!(
        "lengths differ: {} lines expected, {} written",
        expected.len(),
        actual.len()
    )
}

#[test]
fn the_recorded_graph_matches_the_typescript() {
    let text = fs::read_to_string(fixtures().join("graph.json")).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 3, "the fixture has its cases");
    for case in &cases {
        let expected = Json::Array(vec![Json::from_value(case)]).stable();
        let actual = Json::Array(vec![build_case(case)]).stable();
        assert!(
            expected == actual,
            "case {} differs at {}",
            case["id"],
            first_difference(&expected, &actual)
        );
    }
    let all = Json::Array(cases.iter().map(build_case).collect()).stable();
    assert_eq!(all, text, "the whole file, byte for byte");
}
