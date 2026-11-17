// Checks the song crate against the golden fixtures in `fixtures/song/`
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use std::fs;
use std::path::PathBuf;

use fizz_audio_song::album::*;
use fizz_audio_song::describe::describe_song;
use fizz_audio_song::file::*;
use fizz_audio_song::format::{format_document, format_song};
use fizz_audio_song::model::*;
use fizz_audio_song::music;
use fizz_audio_song::summarise::*;
use fizz_audio_song::validate::{validate_song, Problem, Severity};
use serde_json::{json, Value};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/song")
}

fn load(name: &str) -> Vec<Value> {
    let text = fs::read_to_string(dir().join(format!("{name}.json"))).unwrap();
    match serde_json::from_str::<Value>(&text).unwrap() {
        Value::Array(items) => items,
        other => vec![other],
    }
}

fn input_text(name: &str) -> String {
    fs::read_to_string(dir().join("inputs").join(format!("{name}.song.json"))).unwrap()
}

fn input_song(name: &str) -> Song {
    let validation = fizz_audio_song::file::parse_song_file(&input_text(name));
    validation
        .song
        .unwrap_or_else(|| panic!("{name} does not validate: {:?}", validation.problems))
}

fn lines(value: &Value) -> String {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn plain(problems: &[Problem]) -> Value {
    Value::Array(
        problems
            .iter()
            .map(|p| {
                json!({
                    "severity": match p.severity { Severity::Error => "error", Severity::Warning => "warning" },
                    "path": p.path,
                    "message": p.message,
                })
            })
            .collect(),
    )
}

/// Collects failures and prints the count for a fixture file.
struct Tally {
    file: &'static str,
    passed: usize,
    failed: Vec<String>,
}

impl Tally {
    fn new(file: &'static str) -> Tally {
        Tally {
            file,
            passed: 0,
            failed: Vec::new(),
        }
    }

    fn check(&mut self, id: &str, ok: bool, detail: impl FnOnce() -> String) {
        if ok {
            self.passed += 1;
        } else {
            self.failed.push(format!("{id}: {}", detail()));
        }
    }

    fn finish(self) {
        println!(
            "{}: {} passed, {} failed",
            self.file,
            self.passed,
            self.failed.len()
        );
        assert!(
            self.failed.is_empty(),
            "{} failures:\n{}",
            self.file,
            self.failed.join("\n")
        );
    }
}

/// Equality that treats `96` and `96.0` as the same number, as JSON does.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn id(case: &Value) -> &str {
    case["id"].as_str().unwrap()
}

/// A parse case's message may come from the JavaScript engine's JSON parser, so only its start is
/// compared.
fn same_problems(case: &Value, got: &[Problem]) -> bool {
    let want = &case["problems"];
    if case["messageIsEngineSpecific"] == json!(true) {
        let first = &want[0];
        return got.len() == 1
            && got[0].path.is_empty()
            && got[0].severity == Severity::Error
            && got[0].message.starts_with("not valid JSON:")
            && first["message"]
                .as_str()
                .unwrap()
                .starts_with("not valid JSON:");
    }
    plain(got) == *want
}

#[test]
fn validation() {
    let mut tally = Tally::new("validation.json");
    for case in load("validation") {
        let result = validate_song(&case["input"]);
        let got = plain(&result.problems);
        let ok = result.song.is_some();
        tally.check(
            id(&case),
            got == case["problems"] && ok == case["ok"].as_bool().unwrap(),
            || format!("got {got}\nwant {}", case["problems"]),
        );
    }
    tally.finish();
}

#[test]
fn formatted() {
    let mut tally = Tally::new("formatted.json");
    for case in load("formatted") {
        if case["kind"] == "formatSong" {
            let name = case["input"].as_str().unwrap();
            let want = input_text(name);
            let validation = parse_song_file(&input_text(name));
            let got = format_document(validation.document.as_ref().unwrap());
            // The typed song writes the canonical key order and reads back the same song.
            let canonical = parse_song_file(&format_song(validation.song.as_ref().unwrap()));
            tally.check(
                &format!("{}/canonical", id(&case)),
                canonical.song == validation.song,
                || "typed song does not survive format_song".to_string(),
            );
            tally.check(
                id(&case),
                got == want && got.len() as u64 == case["bytes"].as_u64().unwrap(),
                || format!("formatted text differs for {name}"),
            );
        } else {
            let got = format_document(&case["value"]);
            let want = lines(&case["text"]);
            tally.check(id(&case), got == want, || format!("{got:?} vs {want:?}"));
        }
    }
    tally.finish();
}

#[test]
fn files() {
    let mut tally = Tally::new("files.json");
    for case in load("files") {
        match case["kind"].as_str().unwrap() {
            "round-trip" => {
                let first = parse_song_file(&input_text(case["input"].as_str().unwrap()));
                let song = first.song.clone().unwrap();
                let text = serialise_document(first.document.as_ref().unwrap());
                let back = parse_song_file(&text);
                let same_song = back.song.as_ref() == Some(&song);
                let same_text = back
                    .document
                    .as_ref()
                    .is_some_and(|d| serialise_document(d) == text);
                tally.check(
                    id(&case),
                    !back.has_errors()
                        && same_song == case["sameSong"].as_bool().unwrap()
                        && same_text == case["sameText"].as_bool().unwrap(),
                    || format!("{:?}", back.problems),
                );
            }
            "file-name" => {
                let got = song_file_name(case["title"].as_str().unwrap());
                tally.check(id(&case), got == case["name"], || got.clone());
            }
            _ => {
                let result = parse_song_file(case["text"].as_str().unwrap());
                let mut ok = same_problems(&case, &result.problems)
                    && result.song.is_some() == case["ok"].as_bool().unwrap();
                if let (Some(doc), Some(reformatted)) = (&result.document, case.get("reformatted"))
                {
                    ok &= serialise_document(doc) == lines(reformatted);
                }
                tally.check(id(&case), ok, || format!("{:?}", result.problems));
            }
        }
    }
    tally.finish();
}

#[test]
fn describe() {
    let mut tally = Tally::new("describe.json");
    for case in load("describe") {
        let song = input_song(case["input"].as_str().unwrap());
        let section = case["section"].as_str();
        let got = describe_song(&song, section).unwrap();
        let want = lines(&case["text"]);
        tally.check(id(&case), got == want, || format!("{got}\n----\n{want}"));
    }
    tally.finish();
}

#[test]
fn changes() {
    let mut tally = Tally::new("changes.json");
    for case in load("changes") {
        let before = input_song(case["before"].as_str().unwrap());
        let after = input_song(case["after"].as_str().unwrap());
        let got = json!(summarise_change(&before, &after));
        tally.check(id(&case), got == case["changes"], || {
            format!("{got} vs {}", case["changes"])
        });
    }
    tally.finish();
}

fn album(value: &Value) -> Album {
    serde_json::from_value(value.clone()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn albums() {
    let mut tally = Tally::new("albums.json");
    for case in load("albums") {
        let got: Value = match case["kind"].as_str().unwrap() {
            "validate" => {
                let result = validate_album(&case["input"]);
                json!({ "album": result.album, "problems": plain(&result.problems) })
            }
            "parse" => {
                let result = parse_album(case["text"].as_str().unwrap());
                if case["messageIsEngineSpecific"] == json!(true) {
                    let ok = result.album.is_none()
                        && result.problems.len() == 1
                        && result.problems[0].message.starts_with("not valid JSON:");
                    tally.check(id(&case), ok, || format!("{:?}", result.problems));
                    continue;
                }
                json!({ "album": result.album, "problems": plain(&result.problems) })
            }
            "format" => {
                let result = validate_album(&case["input"]);
                let text = format_album(&result.album.expect("valid album"));
                tally.check(id(&case), text == lines(&case["text"]), || text.clone());
                continue;
            }
            "change" => {
                json!({ "changes": summarise_album_change(&album(&case["before"]), &album(&case["after"])) })
            }
            "constants" => {
                json!({
                    "ALBUM_FORMAT": ALBUM_FORMAT,
                    "ALBUM_VERSION": ALBUM_VERSION,
                    "RESERVED_ALBUM_IDS": RESERVED_ALBUM_IDS,
                })
            }
            _ => {
                let args = case["args"].as_array().unwrap();
                let input = case.get("input").map(album);
                let result = match case["fn"].as_str().unwrap() {
                    "emptyAlbum" => {
                        let songs = args.get(1).map(strings).unwrap_or_default();
                        json!(empty_album(args[0].as_str().unwrap(), &songs))
                    }
                    "renameAlbum" => json!(rename_album(
                        &input.unwrap(),
                        args[0].as_str().unwrap(),
                        args.get(1).and_then(Value::as_str)
                    )),
                    "addSongs" => json!(add_songs(
                        &input.unwrap(),
                        &strings(&args[0]),
                        args.get(1).and_then(Value::as_f64)
                    )),
                    "removeSong" => json!(remove_song(&input.unwrap(), args[0].as_str().unwrap())),
                    "moveSong" => json!(move_song(
                        &input.unwrap(),
                        args[0].as_str().unwrap(),
                        args[1].as_f64().unwrap()
                    )),
                    "placeSong" => json!(place_song(
                        &input.unwrap(),
                        args[0].as_str().unwrap(),
                        args[1].as_f64().unwrap()
                    )),
                    "resolveAlbum" => {
                        let known = strings(&args[0]);
                        json!(resolve_album(
                            &input.unwrap(),
                            known.iter().map(String::as_str)
                        ))
                    }
                    "isAlbumId" => json!(is_album_id(args[0].as_str().unwrap())),
                    other => panic!("unknown album function {other}"),
                };
                tally.check(id(&case), same(&result, &case["result"]), || {
                    format!("{result} vs {}", case["result"])
                });
                continue;
            }
        };
        let want: Value = match case["kind"].as_str().unwrap() {
            "validate" | "parse" => json!({ "album": case["album"], "problems": case["problems"] }),
            "change" => json!({ "changes": case["changes"] }),
            _ => {
                let mut want = case.clone();
                for key in ["id", "kind"] {
                    want.as_object_mut().unwrap().remove(key);
                }
                want
            }
        };
        tally.check(id(&case), same(&got, &want), || format!("{got} vs {want}"));
    }
    tally.finish();
}

fn listings(case: &Value) -> Vec<SongListing> {
    case["listings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let mut entry = entry.clone();
            let overview = entry["song"]
                .as_str()
                .map(|name| summarise_song(&input_song(name)).unwrap());
            let object = entry.as_object_mut().unwrap();
            object.remove("song");
            object.insert("overview".into(), json!(overview));
            serde_json::from_value(entry).unwrap()
        })
        .collect()
}

#[test]
fn summaries() {
    let mut tally = Tally::new("summaries.json");
    let all = load("summaries");
    let table = listings(all.last().unwrap());
    for case in &all {
        match case["kind"].as_str().unwrap() {
            "summariseSong" => {
                let got =
                    json!(summarise_song(&input_song(case["input"].as_str().unwrap())).unwrap());
                tally.check(id(case), same(&got, &case["overview"]), || {
                    format!("{got}\nvs\n{}", case["overview"])
                });
            }
            "formatKey" => {
                let key: Key = serde_json::from_value(case["key"].clone()).unwrap();
                tally.check(
                    id(case),
                    format_key(&key) == case["text"].as_str().unwrap(),
                    || format_key(&key),
                );
            }
            "formatClock" => {
                let got = format_clock(case["seconds"].as_f64().unwrap());
                tally.check(id(case), got == case["text"].as_str().unwrap(), || {
                    got.clone()
                });
            }
            "formatAgo" => {
                let now = parse_date(case["now"].as_str().unwrap()).unwrap();
                let got = format_ago(case["at"].as_str().unwrap(), now);
                tally.check(id(case), got == case["text"].as_str().unwrap(), || {
                    got.clone()
                });
            }
            "matchesSearch" => {
                let search = case["search"].as_str().unwrap();
                let got: Vec<bool> = case["listings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|name| {
                        let listing = table
                            .iter()
                            .find(|l| l.id == name.as_str().unwrap())
                            .unwrap();
                        matches_search(listing, search)
                    })
                    .collect();
                tally.check(id(case), json!(got) == case["matches"], || {
                    format!("{got:?}")
                });
            }
            "sortListings" => {
                let key: SortKey = serde_json::from_value(case["sort"].clone()).unwrap();
                let ordered: Vec<SongListing> = case["listings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|name| {
                        table
                            .iter()
                            .find(|l| l.id == name.as_str().unwrap())
                            .unwrap()
                            .clone()
                    })
                    .collect();
                let got: Vec<String> =
                    sort_listings(&ordered, key, case["descending"].as_bool().unwrap())
                        .into_iter()
                        .map(|l| l.id)
                        .collect();
                tally.check(id(case), json!(got) == case["order"], || format!("{got:?}"));
            }
            _ => {}
        }
    }
    tally.finish();
}

#[test]
fn music_fixtures() {
    let mut tally = Tally::new("music.json");
    let all = load("music");
    let data = &all[0];
    let key = |v: &Value| -> Key { serde_json::from_value(v.clone()).unwrap() };
    for (n, case) in data["parsePitch"].as_array().unwrap().iter().enumerate() {
        let got = music::parse_pitch(case["name"].as_str().unwrap());
        tally.check(
            &format!("parsePitch/{n}"),
            json!(got) == case["midi"],
            || format!("{got:?}"),
        );
    }
    for (n, case) in data["pitchClass"].as_array().unwrap().iter().enumerate() {
        let got = music::pitch_class(case["name"].as_str().unwrap());
        tally.check(
            &format!("pitchClass/{n}"),
            json!(got) == case["semitone"],
            || format!("{got:?}"),
        );
    }
    for (n, case) in data["isInKey"].as_array().unwrap().iter().enumerate() {
        let got = music::is_in_key(case["midi"].as_i64().unwrap() as i32, &key(&case["key"]));
        tally.check(
            &format!("isInKey/{n}"),
            got == case["inKey"].as_bool().unwrap(),
            String::new,
        );
    }
    for (n, case) in data["keyPitchClasses"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let mut got = music::key_pitch_classes(&key(&case["key"]));
        let mut want: Vec<i64> = case["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        got.sort();
        want.sort();
        tally.check(
            &format!("keyPitchClasses/{n}"),
            got.iter().map(|v| i64::from(*v)).collect::<Vec<_>>() == want,
            || format!("{got:?}"),
        );
    }
    for (n, case) in data["prefersFlats"]
        .as_array()
        .unwrap()
        .iter()
        .chain(data["prefersFlatsOddities"].as_array().unwrap())
        .enumerate()
    {
        let got = music::prefers_flats(&key(&case["key"]));
        tally.check(
            &format!("prefersFlats/{n}"),
            got == case["flats"].as_bool().unwrap(),
            String::new,
        );
    }
    for (n, case) in data["midiToName"].as_array().unwrap().iter().enumerate() {
        let k = if case["key"].is_null() {
            None
        } else {
            Some(key(&case["key"]))
        };
        let names: Vec<String> = case["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let got: Vec<String> = (0..names.len() as i32)
            .map(|m| music::midi_to_name(m, k.as_ref()))
            .collect();
        tally.check(&format!("midiToName/{n}"), got == names, String::new);
    }
    for (n, case) in data["midiToNameOutsideRange"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let got = music::midi_to_name(case["midi"].as_i64().unwrap() as i32, None);
        tally.check(
            &format!("midiToNameOutsideRange/{n}"),
            got == case["name"].as_str().unwrap(),
            || got.clone(),
        );
    }
    for (n, case) in data["stepInKey"].as_array().unwrap().iter().enumerate() {
        let got = music::step_in_key(
            case["midi"].as_i64().unwrap() as i32,
            case["degrees"].as_i64().unwrap() as i32,
            &key(&case["key"]),
        );
        tally.check(
            &format!("stepInKey/{n}"),
            i64::from(got) == case["result"].as_i64().unwrap(),
            || format!("{got}"),
        );
    }
    for (name, steps) in data["scales"].as_object().unwrap() {
        let scale = ScaleName::from_word(name).unwrap();
        let got: Vec<i64> = music::scale_intervals(scale)
            .iter()
            .map(|v| i64::from(*v))
            .collect();
        let want: Vec<i64> = steps
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        tally.check(&format!("scales/{name}"), got == want, String::new);
    }
    let c = &data["constants"];
    let words = |all: Vec<&str>| json!(all);
    tally.check(
        "constants/engines",
        words(EngineId::ALL.iter().map(|e| e.as_str()).collect()) == c["ENGINE_IDS"],
        String::new,
    );
    tally.check(
        "constants/presets",
        words(PresetId::ALL.iter().map(|e| e.as_str()).collect()) == c["PRESET_IDS"],
        String::new,
    );
    tally.check(
        "constants/roles",
        words(TrackRole::ALL.iter().map(|e| e.as_str()).collect()) == c["TRACK_ROLES"],
        String::new,
    );
    tally.check(
        "constants/drums",
        words(DrumPiece::ALL.iter().map(|e| e.as_str()).collect()) == c["DRUM_PIECES"],
        String::new,
    );
    tally.check(
        "constants/authors",
        words(Author::ALL.iter().map(|e| e.as_str()).collect()) == c["AUTHORS"],
        String::new,
    );
    tally.check(
        "constants/format",
        json!(SONG_FORMAT) == c["SONG_FORMAT"] && json!(SONG_VERSION) == c["SONG_VERSION"],
        String::new,
    );
    tally.finish();
}

#[test]
fn model_fixtures() {
    let mut tally = Tally::new("model.json");
    for case in load("model") {
        let song = input_song(case["input"].as_str().unwrap());
        let mut ok = song.steps_per_bar() as u64 == case["stepsPerBar"].as_u64().unwrap();
        for entry in case["sections"].as_array().unwrap() {
            let section = song.find_section(entry["id"].as_str().unwrap()).unwrap();
            ok &= u64::from(song.section_steps(section)) == entry["steps"].as_u64().unwrap();
        }
        for entry in case["findSection"].as_array().unwrap() {
            let found = song
                .find_section(entry["id"].as_str().unwrap())
                .map(|s| s.id.clone());
            ok &= json!(found) == entry["found"];
        }
        for entry in case["findTrack"].as_array().unwrap() {
            let found = song
                .find_track(entry["id"].as_str().unwrap())
                .map(|t| t.id.clone());
            ok &= json!(found) == entry["found"];
        }
        for entry in case["isDrumTrack"].as_array().unwrap() {
            let track = song.find_track(entry["id"].as_str().unwrap()).unwrap();
            ok &= song.is_drum_track(track) == entry["drums"].as_bool().unwrap();
        }
        for entry in case["activeTake"].as_array().unwrap() {
            let found = song
                .active_take(
                    entry["track"].as_str().unwrap(),
                    entry["section"].as_str().unwrap(),
                )
                .map(|t| t.id.clone());
            ok &= json!(found) == entry["take"];
        }
        tally.check(id(&case), ok, String::new);
    }
    tally.finish();
}
