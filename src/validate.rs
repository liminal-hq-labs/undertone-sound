// Checks a parsed song file and names every problem by its path in the document
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::js::{number_to_string, trim};
use crate::mini_bridge;
use crate::model::{
    Author, DrumPiece, EngineId, PresetId, ScaleName, Song, TrackRole, SONG_FORMAT, SONG_VERSION,
};
use crate::music::{parse_pitch, pitch_class};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub enum Severity {
    Error,
    Warning,
}

/// A stable name for what is wrong, so a caller can react to a kind of problem without reading
/// its message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub enum ProblemCode {
    NotAnObject,
    WrongFormat,
    WrongVersion,
    NeedsTitle,
    BadTempo,
    BadBeatsPerBar,
    BadStepsPerBeat,
    BadKey,
    BadKeyRoot,
    BadKeyScale,
    BadEngine,
    BadInstruments,
    BadId,
    DuplicateId,
    NeedsName,
    BadPreset,
    BadEngines,
    UnknownEngine,
    BadTracks,
    BadRole,
    UnknownInstrument,
    BadVolume,
    BadPan,
    BadSections,
    BadBars,
    BadArrangement,
    EmptyArrangement,
    UnknownSection,
    BadClips,
    UnknownTrack,
    BadSlot,
    BadTakes,
    BadAuthor,
    NeedsOneOfNotesOrMini,
    BadMini,
    BadNotes,
    BadNoteStep,
    BadNoteLength,
    NoteRunsPast,
    BadPitch,
    BadVelocity,
    BadActive,
    BadMessages,
    BadMessageText,
    BadMessageTime,
    BadMix,
    BadPanLaw,
    BadDescription,
    BadSongs,
    BadSongId,
    DuplicateSong,
    NotValidJson,
}

/// One thing wrong, or worth knowing, in a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Problem {
    pub severity: Severity,
    /// Where in the document, such as `clips.melody.verse.takes[0].notes[3].pitch`.
    pub path: String,
    pub message: String,
    pub code: ProblemCode,
}

/// What validating a song gives back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Validation {
    /// The song when there are no errors (warnings allowed), otherwise `null`.
    pub song: Option<Song>,
    pub problems: Vec<Problem>,
    /// The document as read, with its keys in file order, when there are no errors. A `Song` has
    /// its keys in the order of its fields, so formatting this value (`format_document`) is what
    /// keeps a file's own key order; `format_song` writes the canonical order.
    #[serde(skip)]
    #[ts(skip)]
    pub document: Option<Value>,
}

impl Validation {
    pub fn has_errors(&self) -> bool {
        self.problems
            .iter()
            .any(|problem| problem.severity == Severity::Error)
    }
}

/// Collects problems in the order the validator finds them.
#[derive(Default)]
pub(crate) struct Problems(pub Vec<Problem>);

impl Problems {
    pub fn error(
        &mut self,
        code: ProblemCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.push(Severity::Error, code, path.into(), message.into());
    }

    pub fn warning(
        &mut self,
        code: ProblemCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.push(Severity::Warning, code, path.into(), message.into());
    }

    fn push(&mut self, severity: Severity, code: ProblemCode, path: String, message: String) {
        self.0.push(Problem {
            severity,
            path,
            message,
            code,
        });
    }

    pub fn failed(&self) -> bool {
        self.0.iter().any(|p| p.severity == Severity::Error)
    }
}

/// Ids are lower-case letters, digits and hyphens, and start with a letter or digit.
pub fn is_valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_lowercase() || b.is_ascii_digit() => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub(crate) fn as_object(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

/// A JSON number, which is always finite here.
pub(crate) fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        _ => None,
    }
}

pub(crate) fn as_integer(value: &Value) -> Option<f64> {
    as_number(value).filter(|n| n.fract() == 0.0)
}

fn member<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object.get(key)
}

const ID_MESSAGE: &str = "ids are lower-case letters, digits and hyphens";

fn word_list<T: Copy>(all: &[T], word: impl Fn(T) -> &'static str) -> String {
    all.iter()
        .map(|item| word(*item))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Validates an already-parsed JSON value as a song document. Never panics.
pub fn validate_song(raw: &Value) -> Validation {
    let mut problems = Problems::default();
    let Some(raw) = as_object(raw) else {
        problems.error(ProblemCode::NotAnObject, "", "a song is a JSON object");
        return Validation {
            song: None,
            problems: problems.0,
            document: None,
        };
    };

    if member(raw, "format").and_then(Value::as_str) != Some(SONG_FORMAT) {
        problems.error(
            ProblemCode::WrongFormat,
            "format",
            format!("must be \"{SONG_FORMAT}\""),
        );
    }
    if member(raw, "version").and_then(as_number) != Some(f64::from(SONG_VERSION)) {
        problems.error(
            ProblemCode::WrongVersion,
            "version",
            format!("must be {SONG_VERSION}"),
        );
    }
    match member(raw, "title").and_then(Value::as_str) {
        Some(title) if !trim(title).is_empty() => {}
        _ => problems.error(ProblemCode::NeedsTitle, "title", "needs a title"),
    }
    match member(raw, "tempo").and_then(as_number) {
        Some(tempo) if (20.0..=400.0).contains(&tempo) => {}
        _ => problems.error(
            ProblemCode::BadTempo,
            "tempo",
            "must be a number of beats per minute from 20 to 400",
        ),
    }
    let beats_per_bar = member(raw, "beatsPerBar").and_then(as_integer);
    if !beats_per_bar.is_some_and(|n| (1.0..=16.0).contains(&n)) {
        problems.error(
            ProblemCode::BadBeatsPerBar,
            "beatsPerBar",
            "must be a whole number from 1 to 16",
        );
    }
    let steps_per_beat = member(raw, "stepsPerBeat").and_then(as_integer);
    if !steps_per_beat.is_some_and(|n| (1.0..=12.0).contains(&n)) {
        problems.error(
            ProblemCode::BadStepsPerBeat,
            "stepsPerBeat",
            "must be a whole number from 1 to 12",
        );
    }

    match member(raw, "key").and_then(as_object) {
        None => problems.error(
            ProblemCode::BadKey,
            "key",
            "must be { \"root\": \"d\", \"scale\": \"minor\" }",
        ),
        Some(key) => {
            if !member(key, "root")
                .and_then(Value::as_str)
                .is_some_and(|root| pitch_class(root).is_some())
            {
                problems.error(
                    ProblemCode::BadKeyRoot,
                    "key.root",
                    "must be a pitch class such as \"d\", \"f#\" or \"bb\"",
                );
            }
            if !member(key, "scale")
                .and_then(Value::as_str)
                .is_some_and(|scale| ScaleName::from_word(scale).is_some())
            {
                problems.error(
                    ProblemCode::BadKeyScale,
                    "key.scale",
                    format!(
                        "must be one of {}",
                        word_list(ScaleName::ALL, ScaleName::as_str)
                    ),
                );
            }
        }
    }

    if !member(raw, "engine")
        .and_then(Value::as_str)
        .is_some_and(|engine| EngineId::from_word(engine).is_some())
    {
        problems.error(
            ProblemCode::BadEngine,
            "engine",
            format!(
                "must be one of {}",
                word_list(EngineId::ALL, EngineId::as_str)
            ),
        );
    }

    let mut instrument_ids: HashSet<&str> = HashSet::new();
    let mut drum_instruments: HashSet<&str> = HashSet::new();
    match member(raw, "instruments").and_then(as_object) {
        None => problems.error(
            ProblemCode::BadInstruments,
            "instruments",
            "must be an object of instruments by id",
        ),
        Some(instruments) => {
            for (id, instrument) in instruments {
                let path = format!("instruments.{id}");
                if !is_valid_id(id) {
                    problems.error(ProblemCode::BadId, &path, ID_MESSAGE);
                }
                let Some(instrument) = as_object(instrument) else {
                    problems.error(ProblemCode::NotAnObject, &path, "must be an object");
                    continue;
                };
                instrument_ids.insert(id);
                if member(instrument, "name").and_then(Value::as_str).is_none() {
                    problems.error(
                        ProblemCode::NeedsName,
                        format!("{path}.name"),
                        "needs a name",
                    );
                }
                let preset = member(instrument, "preset").and_then(Value::as_str);
                if !preset.is_some_and(|p| PresetId::from_word(p).is_some()) {
                    problems.error(
                        ProblemCode::BadPreset,
                        format!("{path}.preset"),
                        format!(
                            "must be one of {}",
                            word_list(PresetId::ALL, PresetId::as_str)
                        ),
                    );
                }
                if preset == Some("kit") {
                    drum_instruments.insert(id);
                }
                if let Some(engines) = member(instrument, "engines") {
                    match as_object(engines) {
                        None => problems.error(
                            ProblemCode::BadEngines,
                            format!("{path}.engines"),
                            "must be an object",
                        ),
                        Some(engines) => {
                            for (engine, patch) in engines {
                                if EngineId::from_word(engine).is_none() {
                                    problems.error(
                                        ProblemCode::UnknownEngine,
                                        format!("{path}.engines.{engine}"),
                                        "is not a known engine",
                                    );
                                }
                                if as_object(patch).is_none() {
                                    problems.error(
                                        ProblemCode::BadEngines,
                                        format!("{path}.engines.{engine}"),
                                        "must be an object",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut track_ids: HashSet<&str> = HashSet::new();
    let mut drum_tracks: HashSet<&str> = HashSet::new();
    match member(raw, "tracks").and_then(Value::as_array) {
        None => problems.error(ProblemCode::BadTracks, "tracks", "must be a list"),
        Some(tracks) => {
            for (index, track) in tracks.iter().enumerate() {
                let path = format!("tracks[{index}]");
                let Some(track) = as_object(track) else {
                    problems.error(ProblemCode::NotAnObject, path, "must be an object");
                    continue;
                };
                let Some(id) = member(track, "id")
                    .and_then(Value::as_str)
                    .filter(|id| is_valid_id(id))
                else {
                    problems.error(ProblemCode::BadId, format!("{path}.id"), ID_MESSAGE);
                    continue;
                };
                if track_ids.contains(id) {
                    problems.error(
                        ProblemCode::DuplicateId,
                        format!("{path}.id"),
                        format!("\"{id}\" is used twice"),
                    );
                }
                track_ids.insert(id);
                if member(track, "name").and_then(Value::as_str).is_none() {
                    problems.error(
                        ProblemCode::NeedsName,
                        format!("{path}.name"),
                        "needs a name",
                    );
                }
                if !member(track, "role")
                    .and_then(Value::as_str)
                    .is_some_and(|role| TrackRole::from_word(role).is_some())
                {
                    problems.error(
                        ProblemCode::BadRole,
                        format!("{path}.role"),
                        format!(
                            "must be one of {}",
                            word_list(TrackRole::ALL, TrackRole::as_str)
                        ),
                    );
                }
                match member(track, "instrument").and_then(Value::as_str) {
                    Some(instrument) if instrument_ids.contains(instrument) => {
                        if drum_instruments.contains(instrument) {
                            drum_tracks.insert(id);
                        }
                    }
                    _ => problems.error(
                        ProblemCode::UnknownInstrument,
                        format!("{path}.instrument"),
                        "must name an entry in instruments",
                    ),
                }
                if !member(track, "volume")
                    .and_then(as_number)
                    .is_some_and(|v| (0.0..=1.0).contains(&v))
                {
                    problems.error(
                        ProblemCode::BadVolume,
                        format!("{path}.volume"),
                        "must be from 0 to 1",
                    );
                }
                if !member(track, "pan")
                    .and_then(as_number)
                    .is_some_and(|v| (-1.0..=1.0).contains(&v))
                {
                    problems.error(
                        ProblemCode::BadPan,
                        format!("{path}.pan"),
                        "must be from -1 to 1",
                    );
                }
            }
        }
    }

    let mut sections: HashMap<&str, f64> = HashMap::new();
    match member(raw, "sections").and_then(Value::as_array) {
        Some(list) if !list.is_empty() => {
            for (index, section) in list.iter().enumerate() {
                let path = format!("sections[{index}]");
                let Some(section) = as_object(section) else {
                    problems.error(ProblemCode::NotAnObject, path, "must be an object");
                    continue;
                };
                let Some(id) = member(section, "id")
                    .and_then(Value::as_str)
                    .filter(|id| is_valid_id(id))
                else {
                    problems.error(ProblemCode::BadId, format!("{path}.id"), ID_MESSAGE);
                    continue;
                };
                if sections.contains_key(id) {
                    problems.error(
                        ProblemCode::DuplicateId,
                        format!("{path}.id"),
                        format!("\"{id}\" is used twice"),
                    );
                }
                if member(section, "name").and_then(Value::as_str).is_none() {
                    problems.error(
                        ProblemCode::NeedsName,
                        format!("{path}.name"),
                        "needs a name",
                    );
                }
                let bars = member(section, "bars").and_then(as_integer);
                if !bars.is_some_and(|n| (1.0..=64.0).contains(&n)) {
                    problems.error(
                        ProblemCode::BadBars,
                        format!("{path}.bars"),
                        "must be a whole number of bars from 1 to 64",
                    );
                }
                sections.insert(id, bars.unwrap_or(1.0));
            }
        }
        _ => problems.error(
            ProblemCode::BadSections,
            "sections",
            "must be a list with at least one section",
        ),
    }

    match member(raw, "arrangement").and_then(Value::as_array) {
        None => problems.error(
            ProblemCode::BadArrangement,
            "arrangement",
            "must be a list of section ids",
        ),
        Some(list) => {
            if list.is_empty() {
                problems.warning(
                    ProblemCode::EmptyArrangement,
                    "arrangement",
                    "is empty, so nothing plays",
                );
            }
            for (index, id) in list.iter().enumerate() {
                if !id.as_str().is_some_and(|id| sections.contains_key(id)) {
                    problems.error(
                        ProblemCode::UnknownSection,
                        format!("arrangement[{index}]"),
                        "must name a section",
                    );
                }
            }
        }
    }

    let beats = beats_per_bar.unwrap_or(4.0);
    let per_beat = steps_per_beat.unwrap_or(4.0);

    match member(raw, "clips").and_then(as_object) {
        None => problems.error(
            ProblemCode::BadClips,
            "clips",
            "must be an object: clips[trackId][sectionId]",
        ),
        Some(clips) => {
            for (track_id, by_section) in clips {
                if !track_ids.contains(track_id.as_str()) {
                    problems.error(
                        ProblemCode::UnknownTrack,
                        format!("clips.{track_id}"),
                        "names no track",
                    );
                }
                let Some(by_section) = as_object(by_section) else {
                    problems.error(
                        ProblemCode::BadClips,
                        format!("clips.{track_id}"),
                        "must be an object of clip slots by section id",
                    );
                    continue;
                };
                for (section_id, slot) in by_section {
                    let path = format!("clips.{track_id}.{section_id}");
                    let bars = sections.get(section_id.as_str()).copied();
                    if bars.is_none() {
                        problems.error(ProblemCode::UnknownSection, &path, "names no section");
                    }
                    let steps = bars.unwrap_or(64.0) * (beats * per_beat);
                    check_slot(
                        slot,
                        &path,
                        steps,
                        drum_tracks.contains(track_id.as_str()),
                        &mut problems,
                    );
                }
            }
        }
    }

    match member(raw, "messages").and_then(Value::as_array) {
        None => problems.error(
            ProblemCode::BadMessages,
            "messages",
            "must be a list (it may be empty)",
        ),
        Some(messages) => {
            for (index, message) in messages.iter().enumerate() {
                let path = format!("messages[{index}]");
                let Some(message) = as_object(message) else {
                    problems.error(ProblemCode::NotAnObject, path, "must be an object");
                    continue;
                };
                if !member(message, "by")
                    .and_then(Value::as_str)
                    .is_some_and(|by| Author::from_word(by).is_some())
                {
                    problems.error(
                        ProblemCode::BadAuthor,
                        format!("{path}.by"),
                        "must be \"you\" or \"claude\"",
                    );
                }
                if member(message, "text").and_then(Value::as_str).is_none() {
                    problems.error(
                        ProblemCode::BadMessageText,
                        format!("{path}.text"),
                        "needs text",
                    );
                }
                if member(message, "at").and_then(Value::as_str).is_none() {
                    problems.error(
                        ProblemCode::BadMessageTime,
                        format!("{path}.at"),
                        "must be an ISO 8601 timestamp",
                    );
                }
            }
        }
    }

    if let Some(mix) = member(raw, "mix") {
        match as_object(mix) {
            None => problems.error(
                ProblemCode::BadMix,
                "mix",
                "must be an object such as { \"panLaw\": -3 }",
            ),
            Some(mix) => {
                if let Some(pan_law) = member(mix, "panLaw") {
                    if !as_number(pan_law).is_some_and(|n| (-6.0..=0.0).contains(&n)) {
                        problems.error(
                            ProblemCode::BadPanLaw,
                            "mix.panLaw",
                            "must be a number of dB from 0 to -6",
                        );
                    }
                }
            }
        }
    }

    let song = if problems.failed() {
        None
    } else {
        // Validation lets some things through that the typed song cannot hold (a `mute` that is
        // not a boolean, say). Those documents are reported, not read.
        match serde_json::from_value::<Song>(Value::Object(raw.clone())) {
            Ok(song) => Some(song),
            Err(cause) => {
                problems.error(
                    ProblemCode::NotAnObject,
                    "",
                    format!("cannot be read as a song: {cause}"),
                );
                None
            }
        }
    };
    let document = song.as_ref().map(|_| Value::Object(raw.clone()));
    Validation {
        song,
        problems: problems.0,
        document,
    }
}

fn check_slot(slot: &Value, path: &str, steps: f64, drums: bool, problems: &mut Problems) {
    let Some(slot) = as_object(slot) else {
        problems.error(
            ProblemCode::BadSlot,
            path,
            "must be { \"active\": \"a\", \"takes\": [...] }",
        );
        return;
    };
    let Some(takes) = member(slot, "takes").and_then(Value::as_array) else {
        problems.error(
            ProblemCode::BadTakes,
            format!("{path}.takes"),
            "must be a list",
        );
        return;
    };
    let mut take_ids: HashSet<&str> = HashSet::new();
    for (index, take) in takes.iter().enumerate() {
        let take_path = format!("{path}.takes[{index}]");
        let Some(take) = as_object(take) else {
            problems.error(ProblemCode::NotAnObject, take_path, "must be an object");
            continue;
        };
        let Some(id) = member(take, "id")
            .and_then(Value::as_str)
            .filter(|id| is_valid_id(id))
        else {
            problems.error(ProblemCode::BadId, format!("{take_path}.id"), ID_MESSAGE);
            continue;
        };
        if take_ids.contains(id) {
            problems.error(
                ProblemCode::DuplicateId,
                format!("{take_path}.id"),
                format!("\"{id}\" is used twice"),
            );
        }
        take_ids.insert(id);
        if member(take, "name").and_then(Value::as_str).is_none() {
            problems.error(
                ProblemCode::NeedsName,
                format!("{take_path}.name"),
                "needs a name",
            );
        }
        if !member(take, "by")
            .and_then(Value::as_str)
            .is_some_and(|by| Author::from_word(by).is_some())
        {
            problems.error(
                ProblemCode::BadAuthor,
                format!("{take_path}.by"),
                "must be \"you\" or \"claude\"",
            );
        }
        if let Some(pan) = member(take, "pan") {
            if !as_number(pan).is_some_and(|n| (-1.0..=1.0).contains(&n)) {
                problems.error(
                    ProblemCode::BadPan,
                    format!("{take_path}.pan"),
                    "must be from -1 to 1 (leave it out to use the track pan)",
                );
            }
        }
        let notes = member(take, "notes");
        let mini = member(take, "mini");
        if notes.is_some() == mini.is_some() {
            problems.error(
                ProblemCode::NeedsOneOfNotesOrMini,
                &take_path,
                "needs exactly one of \"notes\" or \"mini\"",
            );
        }
        if let Some(mini) = mini {
            match mini.as_str() {
                None => problems.error(
                    ProblemCode::BadMini,
                    format!("{take_path}.mini"),
                    "must be a string",
                ),
                Some(source) => {
                    if let Err(message) = mini_bridge::check(source, drums) {
                        problems.error(ProblemCode::BadMini, format!("{take_path}.mini"), message);
                    }
                }
            }
        }
        if let Some(notes) = notes {
            let Some(notes) = notes.as_array() else {
                problems.error(
                    ProblemCode::BadNotes,
                    format!("{take_path}.notes"),
                    "must be a list",
                );
                continue;
            };
            for (note_index, note) in notes.iter().enumerate() {
                check_note(
                    note,
                    &format!("{take_path}.notes[{note_index}]"),
                    steps,
                    drums,
                    problems,
                );
            }
        }
    }
    match member(slot, "active") {
        Some(Value::Null) => {}
        Some(Value::String(active)) if take_ids.contains(active.as_str()) => {}
        _ => problems.error(
            ProblemCode::BadActive,
            format!("{path}.active"),
            "must name one of the takes, or be null for silence",
        ),
    }
}

fn check_note(note: &Value, path: &str, steps: f64, drums: bool, problems: &mut Problems) {
    let Some(note) = as_object(note) else {
        problems.error(
            ProblemCode::NotAnObject,
            path,
            "must be { \"at\", \"pitch\", \"len\" }",
        );
        return;
    };
    let at = member(note, "at").and_then(as_integer);
    if !at.is_some_and(|at| at >= 0.0 && at < steps) {
        problems.error(
            ProblemCode::BadNoteStep,
            format!("{path}.at"),
            format!(
                "must be a whole step from 0 to {}",
                number_to_string(steps - 1.0)
            ),
        );
    }
    match member(note, "len").and_then(as_integer) {
        Some(len) if len >= 1.0 => {
            if let Some(at) = at {
                if at + len > steps {
                    problems.warning(
                        ProblemCode::NoteRunsPast,
                        format!("{path}.len"),
                        "runs past the end of the section and is cut there",
                    );
                }
            }
        }
        _ => problems.error(
            ProblemCode::BadNoteLength,
            format!("{path}.len"),
            "must be a whole number of steps, at least 1",
        ),
    }
    match member(note, "pitch").and_then(Value::as_str) {
        None => problems.error(
            ProblemCode::BadPitch,
            format!("{path}.pitch"),
            "must be a string",
        ),
        Some(pitch) => {
            if let Err(message) = check_pitch_word(pitch, drums) {
                problems.error(ProblemCode::BadPitch, format!("{path}.pitch"), message);
            }
        }
    }
    if let Some(vel) = member(note, "vel") {
        if !as_number(vel).is_some_and(|n| (0.0..=1.0).contains(&n)) {
            problems.error(
                ProblemCode::BadVelocity,
                format!("{path}.vel"),
                "must be from 0 to 1",
            );
        }
    }
}

/// Fails with the message when a word is not a playable pitch for the track (a drum piece on a
/// drum track). The mini-notation leaf rule is this function too.
pub fn check_pitch_word(word: &str, drums: bool) -> Result<(), String> {
    if drums {
        if DrumPiece::from_word(word).is_none() {
            return Err(format!(
                "\"{word}\" is not a drum piece ({})",
                word_list(DrumPiece::ALL, DrumPiece::as_str)
            ));
        }
        return Ok(());
    }
    let Some(midi) = parse_pitch(word) else {
        return Err(format!(
            "\"{word}\" is not a note name such as \"d4\", \"f#3\" or \"bb2\""
        ));
    };
    if !(12..=120).contains(&midi) {
        return Err(format!("\"{word}\" is out of range (c0 to c9)"));
    }
    Ok(())
}
