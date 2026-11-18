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
    Author, DrumPiece, EngineId, PresetId, ScaleName, Song, StepKind, TrackRole,
    MAX_REVISION_STEPS, SONG_FORMAT, SONG_VERSION,
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
    BadPriority,
    /// A field the format does not define, which is ignored.
    UnknownField,
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
    /// A take's `revision` that does not read, or names a take or step that is not there.
    BadRevision,
    BadDescription,
    BadSongs,
    BadSongId,
    DuplicateSong,
    NotValidJson,
    /// A comments file (`<id>.comments.json`) whose frame is wrong: its format, version, size or
    /// `seq`.
    BadCommentsFile,
    /// A thread in a comments file that does not read.
    BadThread,
    /// A comment in a comments file that does not read.
    BadComment,
    /// A comment thread's anchor that does not read.
    BadAnchor,
    /// A comments file over one of its limits: comments in a thread, threads in a file, or the
    /// length of a comment.
    CommentsLimit,
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

/// The fields the format defines for each kind of object, so a field it does not define (a note's
/// `bend`, a section's `tempo`) is warned about rather than dropped without a word. The same lists
/// are in `apps/studio/src/song/validate.ts`, and `docs/SONG-FORMAT.md` documents them.
const SONG_FIELDS: &[&str] = &[
    "format",
    "version",
    "title",
    "tempo",
    "beatsPerBar",
    "stepsPerBeat",
    "key",
    "engine",
    "instruments",
    "tracks",
    "sections",
    "arrangement",
    "clips",
    "messages",
    "mix",
];
const KEY_FIELDS: &[&str] = &["root", "scale"];
const INSTRUMENT_FIELDS: &[&str] = &["name", "preset", "engines"];
const TRACK_FIELDS: &[&str] = &[
    "id",
    "name",
    "role",
    "instrument",
    "volume",
    "pan",
    "mute",
    "solo",
    "priority",
];
const SECTION_FIELDS: &[&str] = &["id", "name", "bars"];
const SLOT_FIELDS: &[&str] = &["active", "takes"];
const TAKE_FIELDS: &[&str] = &[
    "id", "name", "by", "notes", "mini", "pan", "comment", "revision",
];
const NOTE_FIELDS: &[&str] = &["at", "pitch", "len", "vel"];
const MESSAGE_FIELDS: &[&str] = &["by", "text", "at", "done"];
const MIX_FIELDS: &[&str] = &["panLaw"];
const REVISION_FIELDS: &[&str] = &[
    "of", "n", "request", "model", "settings", "steps", "current", "folded",
];
const REVISION_SETTINGS_FIELDS: &[&str] = &["think", "temperature", "bars"];
const STEP_FIELDS: &[&str] = &["id", "kind", "at", "comment", "notes", "mini", "bars"];
const BAR_FIELDS: &[&str] = &["bar", "notes"];

/// Warns about each key of `object` that is not in `known`: it does nothing, and a typo or a
/// field from another format would otherwise pass without a word. `what` names the kind of
/// object in the message (`note`), and `path` is where it is (`""` for the song itself).
fn check_fields(
    object: &Map<String, Value>,
    known: &[&str],
    what: &str,
    path: &str,
    problems: &mut Problems,
) {
    for key in object.keys() {
        if known.contains(&key.as_str()) {
            continue;
        }
        let at = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        let article = if what.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        problems.warning(
            ProblemCode::UnknownField,
            at,
            format!(
                "\"{key}\" is not {article} {what} field and is ignored (the {what} fields are {})",
                known.join(", ")
            ),
        );
    }
}

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

    check_fields(raw, SONG_FIELDS, "song", "", &mut problems);
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
            check_fields(key, KEY_FIELDS, "key", "key", &mut problems);
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
                check_fields(
                    instrument,
                    INSTRUMENT_FIELDS,
                    "instrument",
                    &path,
                    &mut problems,
                );
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
                check_fields(track, TRACK_FIELDS, "track", &path, &mut problems);
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
                if member(track, "priority").is_some_and(|value| {
                    !as_integer(value).is_some_and(|n| (0.0..=9.0).contains(&n))
                }) {
                    problems.error(
                        ProblemCode::BadPriority,
                        format!("{path}.priority"),
                        "must be a whole number from 0 to 9 (leave it out to use the role's)",
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
                check_fields(section, SECTION_FIELDS, "section", &path, &mut problems);
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
                        bars.unwrap_or(64.0),
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
                check_fields(message, MESSAGE_FIELDS, "message", &path, &mut problems);
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
                check_fields(mix, MIX_FIELDS, "mix", "mix", &mut problems);
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

fn check_slot(
    slot: &Value,
    path: &str,
    steps: f64,
    bars: f64,
    drums: bool,
    problems: &mut Problems,
) {
    let Some(slot) = as_object(slot) else {
        problems.error(
            ProblemCode::BadSlot,
            path,
            "must be { \"active\": \"a\", \"takes\": [...] }",
        );
        return;
    };
    check_fields(slot, SLOT_FIELDS, "clip slot", path, problems);
    let Some(takes) = member(slot, "takes").and_then(Value::as_array) else {
        problems.error(
            ProblemCode::BadTakes,
            format!("{path}.takes"),
            "must be a list",
        );
        return;
    };
    let mut take_ids: HashSet<&str> = HashSet::new();
    // Each revised take's path, id and `of`, checked once every id in the slot is known.
    let mut revised: Vec<(String, &str, Option<&str>)> = Vec::new();
    for (index, take) in takes.iter().enumerate() {
        let take_path = format!("{path}.takes[{index}]");
        let Some(take) = as_object(take) else {
            problems.error(ProblemCode::NotAnObject, take_path, "must be an object");
            continue;
        };
        check_fields(take, TAKE_FIELDS, "take", &take_path, problems);
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
        if let Some(revision) = member(take, "revision") {
            let of = check_revision(
                revision,
                &format!("{take_path}.revision"),
                steps,
                bars,
                drums,
                problems,
            );
            revised.push((format!("{take_path}.revision"), id, of));
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
    check_revision_links(&revised, &take_ids, problems);
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

/// Checks a take's `revision` and returns its `of`, when it has one, for the links check.
fn check_revision<'a>(
    revision: &'a Value,
    path: &str,
    steps: f64,
    bars: f64,
    drums: bool,
    problems: &mut Problems,
) -> Option<&'a str> {
    let Some(revision) = as_object(revision) else {
        problems.error(ProblemCode::BadRevision, path, "must be an object");
        return None;
    };
    check_fields(revision, REVISION_FIELDS, "revision", path, problems);
    let of = member(revision, "of").and_then(Value::as_str);
    if of.is_none() {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.of"),
            "must name another take in the slot",
        );
    }
    if !member(revision, "n")
        .and_then(as_integer)
        .is_some_and(|n| n >= 1.0)
    {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.n"),
            "must be a whole number from 1",
        );
    }
    if member(revision, "request")
        .and_then(Value::as_str)
        .is_none()
    {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.request"),
            "needs the request text",
        );
    }
    if member(revision, "model").and_then(Value::as_str).is_none() {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.model"),
            "needs a model name",
        );
    }
    if let Some(settings) = member(revision, "settings") {
        check_revision_settings(settings, &format!("{path}.settings"), bars, problems);
    }
    let mut step_ids: HashSet<&str> = HashSet::new();
    match member(revision, "steps").and_then(Value::as_array) {
        Some(list) if (1..=MAX_REVISION_STEPS).contains(&list.len()) => {
            for (index, step) in list.iter().enumerate() {
                if let Some(id) = check_step(
                    step,
                    &format!("{path}.steps[{index}]"),
                    index == 0,
                    steps,
                    bars,
                    drums,
                    problems,
                ) {
                    if !step_ids.insert(id) {
                        problems.error(
                            ProblemCode::DuplicateId,
                            format!("{path}.steps[{index}].id"),
                            format!("\"{id}\" is used twice"),
                        );
                    }
                }
            }
        }
        _ => problems.error(
            ProblemCode::BadRevision,
            format!("{path}.steps"),
            format!("must be a list of 1 to {MAX_REVISION_STEPS} steps"),
        ),
    }
    if !member(revision, "current")
        .and_then(Value::as_str)
        .is_some_and(|current| step_ids.contains(current))
    {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.current"),
            "must name one of the steps",
        );
    }
    if member(revision, "folded")
        .is_some_and(|folded| !as_integer(folded).is_some_and(|folded| folded >= 1.0))
    {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.folded"),
            "must be a whole number from 1 (leave it out when nothing was folded)",
        );
    }
    of
}

fn check_revision_settings(settings: &Value, path: &str, bars: f64, problems: &mut Problems) {
    let Some(settings) = as_object(settings) else {
        problems.error(ProblemCode::BadRevision, path, "must be an object");
        return;
    };
    check_fields(
        settings,
        REVISION_SETTINGS_FIELDS,
        "revision settings",
        path,
        problems,
    );
    if member(settings, "think").is_some_and(|think| !think.is_boolean()) {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.think"),
            "must be true or false",
        );
    }
    if member(settings, "temperature").is_some_and(|value| as_number(value).is_none()) {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.temperature"),
            "must be a number",
        );
    }
    if let Some(list) = member(settings, "bars") {
        let ok = list.as_array().is_some_and(|list| {
            list.iter()
                .all(|bar| as_integer(bar).is_some_and(|bar| bar >= 1.0 && bar <= bars))
        });
        if !ok {
            problems.error(
                ProblemCode::BadRevision,
                format!("{path}.bars"),
                format!(
                    "must be a list of bars, each a whole number from 1 to {}",
                    number_to_string(bars)
                ),
            );
        }
    }
}

/// Checks one step and returns its id, when it has a good one.
fn check_step<'a>(
    step: &'a Value,
    path: &str,
    first: bool,
    steps: f64,
    bars: f64,
    drums: bool,
    problems: &mut Problems,
) -> Option<&'a str> {
    let Some(step) = as_object(step) else {
        problems.error(ProblemCode::NotAnObject, path, "must be an object");
        return None;
    };
    check_fields(step, STEP_FIELDS, "revision step", path, problems);
    let id = member(step, "id")
        .and_then(Value::as_str)
        .filter(|id| is_valid_id(id));
    if id.is_none() {
        problems.error(ProblemCode::BadId, format!("{path}.id"), ID_MESSAGE);
    }
    if !member(step, "kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| StepKind::from_word(kind).is_some())
    {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.kind"),
            format!(
                "must be one of {}",
                word_list(StepKind::ALL, StepKind::as_str)
            ),
        );
    }
    if member(step, "at").and_then(Value::as_str).is_none() {
        problems.error(
            ProblemCode::BadMessageTime,
            format!("{path}.at"),
            "must be an ISO 8601 timestamp",
        );
    }
    if member(step, "comment").is_some_and(|comment| !comment.is_string()) {
        problems.error(
            ProblemCode::BadRevision,
            format!("{path}.comment"),
            "must be a string",
        );
    }
    let notes = member(step, "notes");
    let mini = member(step, "mini");
    let changed = member(step, "bars");
    let held = [notes.is_some(), mini.is_some(), changed.is_some()];
    if held.iter().filter(|held| **held).count() != 1 {
        problems.error(
            ProblemCode::NeedsOneOfNotesOrMini,
            path,
            "needs exactly one of \"notes\", \"mini\" or \"bars\"",
        );
    } else if first && changed.is_some() {
        problems.error(
            ProblemCode::BadRevision,
            path,
            "the first step must hold \"notes\" or \"mini\", as nothing comes before it",
        );
    }
    if let Some(mini) = mini {
        match mini.as_str() {
            None => problems.error(
                ProblemCode::BadMini,
                format!("{path}.mini"),
                "must be a string",
            ),
            Some(source) => {
                if let Err(message) = mini_bridge::check(source, drums) {
                    problems.error(ProblemCode::BadMini, format!("{path}.mini"), message);
                }
            }
        }
    }
    if let Some(notes) = notes {
        match notes.as_array() {
            None => problems.error(
                ProblemCode::BadNotes,
                format!("{path}.notes"),
                "must be a list",
            ),
            Some(notes) => {
                for (index, note) in notes.iter().enumerate() {
                    check_note(
                        note,
                        &format!("{path}.notes[{index}]"),
                        steps,
                        drums,
                        problems,
                    );
                }
            }
        }
    }
    if let Some(changed) = changed {
        match changed.as_array() {
            None => problems.error(
                ProblemCode::BadRevision,
                format!("{path}.bars"),
                "must be a list",
            ),
            Some(list) => {
                for (index, entry) in list.iter().enumerate() {
                    let entry_path = format!("{path}.bars[{index}]");
                    let Some(entry) = as_object(entry) else {
                        problems.error(
                            ProblemCode::NotAnObject,
                            entry_path,
                            "must be { \"bar\", \"notes\" }",
                        );
                        continue;
                    };
                    check_fields(entry, BAR_FIELDS, "revision bar", &entry_path, problems);
                    if !member(entry, "bar")
                        .and_then(as_integer)
                        .is_some_and(|bar| bar >= 1.0 && bar <= bars)
                    {
                        problems.error(
                            ProblemCode::BadRevision,
                            format!("{entry_path}.bar"),
                            format!("must be a whole bar from 1 to {}", number_to_string(bars)),
                        );
                    }
                    match member(entry, "notes").and_then(Value::as_array) {
                        None => problems.error(
                            ProblemCode::BadNotes,
                            format!("{entry_path}.notes"),
                            "must be a list",
                        ),
                        Some(notes) => {
                            for (note_index, note) in notes.iter().enumerate() {
                                check_note(
                                    note,
                                    &format!("{entry_path}.notes[{note_index}]"),
                                    steps,
                                    drums,
                                    problems,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    id
}

/// Every revised take's `of` must be a take of the slot, and a chain must not loop. A take may
/// name itself, which is a trial of its own (the old version kept as a step); that is no loop.
fn check_revision_links(
    revised: &[(String, &str, Option<&str>)],
    take_ids: &HashSet<&str>,
    problems: &mut Problems,
) {
    let parents: HashMap<&str, &str> = revised
        .iter()
        .filter_map(|(_, id, of)| of.filter(|of| of != id).map(|of| (*id, of)))
        .collect();
    for (path, id, of) in revised {
        let Some(of) = of else { continue };
        if of == id {
            continue;
        }
        if !take_ids.contains(of) {
            problems.error(
                ProblemCode::BadRevision,
                format!("{path}.of"),
                "must name another take in the slot",
            );
            continue;
        }
        let mut here = *of;
        for _ in 0..parents.len() {
            match parents.get(here) {
                Some(next) => here = next,
                None => break,
            }
            if here == *id {
                problems.error(
                    ProblemCode::BadRevision,
                    format!("{path}.of"),
                    "must not lead back to this take",
                );
                break;
            }
        }
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
    check_fields(note, NOTE_FIELDS, "note", path, problems);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn song(change: impl FnOnce(&mut Value)) -> Value {
        let mut song = json!({
            "format": "fizz-audio-studio/song", "version": 1, "title": "T", "tempo": 120,
            "beatsPerBar": 4, "stepsPerBeat": 4, "key": { "root": "c", "scale": "major" },
            "engine": "opl2",
            "instruments": { "lead": { "name": "Lead", "preset": "lead" } },
            "tracks": [{
                "id": "melody", "name": "Melody", "role": "melody", "instrument": "lead",
                "volume": 0.8, "pan": 0, "priority": 5
            }],
            "sections": [{ "id": "a", "name": "A", "bars": 1 }],
            "arrangement": ["a"],
            "clips": { "melody": { "a": { "active": "a", "takes": [{
                "id": "a", "name": "A", "by": "you",
                "notes": [{ "at": 0, "pitch": "c4", "len": 2 }]
            }] } } },
            "messages": []
        });
        change(&mut song);
        song
    }

    #[test]
    fn a_field_the_format_does_not_define_is_a_warning_and_the_song_still_reads() {
        let raw = song(|song| {
            song["clips"]["melody"]["a"]["takes"][0]["notes"][0]["bend"] = json!([[0, 2]]);
            song["sections"][0]["tempo"] = json!(90);
            song["extra"] = json!(true);
        });
        let found = validate_song(&raw);
        assert!(found.song.is_some() && !found.has_errors());
        let seen: Vec<(&str, &str)> = found
            .problems
            .iter()
            .map(|p| (p.path.as_str(), p.message.as_str()))
            .collect();
        assert_eq!(
            seen,
            [
                ("extra", "\"extra\" is not a song field and is ignored (the song fields are format, version, title, tempo, beatsPerBar, stepsPerBeat, key, engine, instruments, tracks, sections, arrangement, clips, messages, mix)"),
                ("sections[0].tempo", "\"tempo\" is not a section field and is ignored (the section fields are id, name, bars)"),
                ("clips.melody.a.takes[0].notes[0].bend", "\"bend\" is not a note field and is ignored (the note fields are at, pitch, len, vel)"),
            ]
        );
        assert!(found
            .problems
            .iter()
            .all(|p| p.severity == Severity::Warning && p.code == ProblemCode::UnknownField));
        // The unknown fields stay in the document, so a file keeps what it carried.
        assert_eq!(found.document.unwrap()["extra"], json!(true));
    }

    #[test]
    fn an_instrument_takes_an_article_and_defined_fields_draw_nothing() {
        let raw = song(|song| {
            song["instruments"]["lead"]["gain"] = json!(1);
            song["mix"] = json!({ "panLaw": -3 });
        });
        let found = validate_song(&raw);
        assert_eq!(found.problems.len(), 1);
        assert!(found.problems[0]
            .message
            .starts_with("\"gain\" is not an instrument field"));
    }

    #[test]
    fn a_track_priority_is_a_whole_number_from_0_to_9() {
        for good in [json!(0), json!(9), json!(9.0)] {
            let found = validate_song(&song(|s| s["tracks"][0]["priority"] = good.clone()));
            assert!(found.problems.is_empty(), "{good}");
            assert!(found.song.is_some(), "{good}");
        }
        for bad in [json!(10), json!(-1), json!(4.5), json!("5"), json!(null)] {
            let found = validate_song(&song(|s| s["tracks"][0]["priority"] = bad.clone()));
            assert_eq!(found.problems.len(), 1, "{bad}");
            assert_eq!(found.problems[0].path, "tracks[0].priority");
            assert_eq!(found.problems[0].code, ProblemCode::BadPriority);
        }
    }
}
