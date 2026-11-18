// The song document: the one file a person and Claude both edit
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! These types are the wire format of `songs/<id>.song.json`, exported to
//! `packages/protocol/src/generated/`. Maps that hold document order (`instruments`, `clips`,
//! an instrument's `engines`) are `IndexMap`s, so reading and writing a song keeps its keys where
//! the file had them.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

/// The `format` every song file carries, so a stray JSON file is never mistaken for one.
pub const SONG_FORMAT: &str = "fizz-audio-studio/song";
/// The song document version this studio reads and writes.
pub const SONG_VERSION: u32 = 1;

macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident, $rename:literal, [$($variant:ident => $text:literal),+ $(,)?]) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[serde(rename_all = $rename)]
        #[ts(export, export_to = "../../../packages/protocol/src/generated/")]
        pub enum $name {
            $(
                #[serde(rename = $text)]
                #[ts(rename = $text)]
                $variant,
            )+
        }

        impl $name {
            /// Every value, in the order the TypeScript lists them.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The word the song file spells this value with.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text,)+
                }
            }

            /// The value a word names, or `None`.
            pub fn from_word(word: &str) -> Option<$name> {
                match word {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

text_enum!(
    /// Who wrote a take or a message: the person at the studio, or Claude.
    Author, "lowercase", [You => "you", Claude => "claude"]
);

text_enum!(
    /// The sound engines a song can be played on.
    EngineId, "lowercase",
    [Undertone => "undertone", Opl2 => "opl2", Opl3 => "opl3", PcSpeaker => "pc-speaker"]
);

text_enum!(
    /// What a track does in the song. It sets the default instrument and how the studio draws it.
    TrackRole, "lowercase",
    [Melody => "melody", Harmony => "harmony", Bass => "bass", Drums => "drums", Texture => "texture"]
);

text_enum!(
    /// Engine-neutral instrument presets. Each engine maps a preset to its own timbre.
    PresetId, "kebab-case",
    [
        Lead => "lead",
        SquareLead => "square-lead",
        SoftLead => "soft-lead",
        Pluck => "pluck",
        Bell => "bell",
        Pad => "pad",
        Organ => "organ",
        Bass => "bass",
        SubBass => "sub-bass",
        Kit => "kit",
    ]
);

text_enum!(
    /// The pieces of a drum kit. A drum track's notes use these words as their `pitch`. The order
    /// is the order the text readout draws them in.
    DrumPiece, "lowercase",
    [
        Crash => "crash",
        Openhat => "openhat",
        Hat => "hat",
        Clap => "clap",
        Rim => "rim",
        Snare => "snare",
        Tom => "tom",
        Kick => "kick",
    ]
);

text_enum!(
    /// A scale a key can name; `music::scale_intervals` gives its semitone steps.
    ScaleName, "camelCase",
    [
        Major => "major",
        Minor => "minor",
        Dorian => "dorian",
        Phrygian => "phrygian",
        Lydian => "lydian",
        Mixolydian => "mixolydian",
        Locrian => "locrian",
        HarmonicMinor => "harmonicMinor",
        MelodicMinor => "melodicMinor",
        MajorPentatonic => "majorPentatonic",
        MinorPentatonic => "minorPentatonic",
        Chromatic => "chromatic",
    ]
);

/// A whole number that JSON may spell with a fraction part (`4.0`), as JavaScript does.
mod whole_number {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u32, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(*value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
        let number = f64::deserialize(deserializer)?;
        if number.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&number) {
            Ok(number as u32)
        } else {
            Err(serde::de::Error::custom("expected a whole number"))
        }
    }
}

/// An optional small whole number that JSON may spell with a fraction part (`9.0`).
mod whole_priority {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<u8>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_u8(*value),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u8>, D::Error> {
        let Some(number) = Option::<f64>::deserialize(deserializer)? else {
            return Ok(None);
        };
        if number.fract() == 0.0 && (0.0..=255.0).contains(&number) {
            Ok(Some(number as u8))
        } else {
            Err(serde::de::Error::custom("expected a whole number"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Key {
    /// Pitch class of the tonic, such as `"d"`, `"f#"` or `"bb"`.
    pub root: String,
    pub scale: ScaleName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Instrument {
    pub name: String,
    pub preset: PresetId,
    /// Per-engine voice overrides layered over the preset, keyed by engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(
        type = "Partial<Record<\"undertone\" | \"opl2\" | \"opl3\" | \"pc-speaker\", Record<string, unknown>>>",
        optional
    )]
    pub engines: Option<IndexMap<EngineId, Map<String, Value>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Track {
    pub id: String,
    pub name: String,
    pub role: TrackRole,
    /// Id of an entry in `Song.instruments`.
    pub instrument: String,
    /// Track level, 0 to 1, multiplied into every note's velocity.
    pub volume: f64,
    /// Stereo position, -1 (left) to 1 (right).
    pub pan: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solo: Option<bool>,
    /// How much the track's notes matter when an OPL chip runs out of channels, 0 to 9. Left out,
    /// the role decides (`TrackRole::default_priority`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "whole_priority"
    )]
    #[ts(optional, type = "number")]
    pub priority: Option<u8>,
}

impl TrackRole {
    /// The priority a track of this role has when it sets none. Melody 9, bass 8, drums 7,
    /// harmony 5, texture 4: a lead outlasts a bass, which outlasts a pad, which outlasts a bell.
    pub fn default_priority(self) -> u8 {
        match self {
            TrackRole::Melody => 9,
            TrackRole::Bass => 8,
            TrackRole::Drums => 7,
            TrackRole::Harmony => 5,
            TrackRole::Texture => 4,
        }
    }
}

impl Track {
    /// The track's own priority, else its role's.
    pub fn effective_priority(&self) -> u8 {
        self.priority
            .unwrap_or_else(|| self.role.default_priority())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Section {
    pub id: String,
    pub name: String,
    /// Length in bars.
    #[serde(with = "whole_number")]
    #[ts(type = "number")]
    pub bars: u32,
}

/// One note in a take. Times are in grid steps from the start of the section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Note {
    /// Onset step, 0 up to but not including the section's length in steps.
    pub at: f64,
    /// A note name (`"d4"`, `"f#3"`, `"bb2"`), or a drum piece on a drum track.
    pub pitch: String,
    /// Length in steps, at least 1.
    pub len: f64,
    /// Strength, 0 to 1 (default 0.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub vel: Option<f64>,
}

text_enum!(
    /// How a revision step came about: the model's first answer, another go, a hand edit, or the
    /// kept bars joined back with the new ones.
    StepKind, "lowercase",
    [Model => "model", Retry => "retry", Edit => "edit", Splice => "splice"]
);

/// The most steps one revision keeps, so a take's history cannot grow the song file without end.
pub const MAX_REVISION_STEPS: usize = 20;

/// What a revision was asked with, all of it optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct RevisionSettings {
    /// Whether the model thought before it answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub think: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub temperature: Option<f64>,
    /// The bars (counted from 1) the request was limited to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bars: Option<Vec<u32>>,
}

/// The notes of one bar in a step that holds only what changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct BarNotes {
    /// The bar, counted from 1.
    pub bar: u32,
    /// Every note whose onset is in the bar; none means the bar is empty.
    pub notes: Vec<Note>,
}

/// One version of a revised take. Exactly one of `notes`, `mini` and `bars` holds its content:
/// `notes` or `mini` is a whole snapshot (the first and the current step are always one), `bars`
/// lists the bars that differ from the step before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct RevisionStep {
    /// Unique in the take (`s1`, `s2`, ...).
    pub id: String,
    pub kind: StepKind,
    /// ISO 8601 timestamp.
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notes: Option<Vec<Note>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mini: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bars: Option<Vec<BarNotes>>,
}

/// A take's link to the take it revises, and the steps it went through (`docs/SONG-FORMAT.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Revision {
    /// Id of the take this one revises, in the same slot.
    pub of: String,
    /// Which trial of the parent this is, from 1.
    pub n: u32,
    /// What the person asked for.
    pub request: String,
    /// The model's name.
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub settings: Option<RevisionSettings>,
    /// One to `MAX_REVISION_STEPS` versions, oldest first.
    pub steps: Vec<RevisionStep>,
    /// Id of the step the take is on.
    pub current: String,
}

/// One idea for what a track plays in a section. A take holds either explicit `notes` or a `mini`
/// pattern in Undertone mini-notation, where one cycle is one bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Take {
    pub id: String,
    pub name: String,
    pub by: Author,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notes: Option<Vec<Note>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mini: Option<String>,
    /// Stereo position for this take's notes, -1 to 1, in place of the track's pan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pan: Option<f64>,
    /// A word about the idea, from whoever wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub comment: Option<String>,
    /// Set when this take revises another and keeps the steps it went through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub revision: Option<Revision>,
}

/// Every take a track has for one section, and which of them plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct ClipSlot {
    /// Id of the take that plays, or `null` for silence.
    pub active: Option<String>,
    pub takes: Vec<Take>,
}

/// A message left in the song for the other collaborator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Message {
    pub by: Author,
    pub text: String,
    /// ISO 8601 timestamp.
    pub at: String,
    /// Set once the other collaborator has dealt with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub done: Option<bool>,
}

/// How the song is mixed, where it differs from the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Mix {
    /// The pan law: each side's level at the centre in dB, from 0 to -6. Default -3.
    #[serde(default, rename = "panLaw", skip_serializing_if = "Option::is_none")]
    #[ts(optional, rename = "panLaw")]
    pub pan_law: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/protocol/src/generated/")]
pub struct Song {
    #[ts(type = "\"fizz-audio-studio/song\"")]
    pub format: String,
    #[serde(with = "whole_number")]
    #[ts(type = "1")]
    pub version: u32,
    pub title: String,
    /// Tempo in beats per minute.
    pub tempo: f64,
    #[serde(rename = "beatsPerBar", with = "whole_number")]
    #[ts(rename = "beatsPerBar", type = "number")]
    pub beats_per_bar: u32,
    /// Grid steps per beat: 4 gives a sixteenth-note grid in 4/4.
    #[serde(rename = "stepsPerBeat", with = "whole_number")]
    #[ts(rename = "stepsPerBeat", type = "number")]
    pub steps_per_beat: u32,
    pub key: Key,
    /// The engine the song is meant for.
    pub engine: EngineId,
    pub instruments: IndexMap<String, Instrument>,
    pub tracks: Vec<Track>,
    pub sections: Vec<Section>,
    /// Section ids in play order; a section may appear more than once.
    pub arrangement: Vec<String>,
    /// `clips[trackId][sectionId]`. A missing slot is silence.
    pub clips: IndexMap<String, IndexMap<String, ClipSlot>>,
    pub messages: Vec<Message>,
    /// How the song is mixed, where it differs from the defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mix: Option<Mix>,
}

/// Grid steps in one bar.
pub fn steps_per_bar(beats_per_bar: u32, steps_per_beat: u32) -> u32 {
    beats_per_bar * steps_per_beat
}

impl Song {
    pub fn steps_per_bar(&self) -> u32 {
        steps_per_bar(self.beats_per_bar, self.steps_per_beat)
    }

    /// Grid steps in a section.
    pub fn section_steps(&self, section: &Section) -> u32 {
        section.bars * self.steps_per_bar()
    }

    pub fn find_section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|section| section.id == id)
    }

    pub fn find_track(&self, id: &str) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == id)
    }

    /// The take that plays in a slot, if the slot has one active.
    pub fn active_take(&self, track_id: &str, section_id: &str) -> Option<&Take> {
        let slot = self.clips.get(track_id)?.get(section_id)?;
        let active = slot.active.as_deref()?;
        slot.takes.iter().find(|take| take.id == active)
    }

    /// Whether the track plays a drum kit.
    pub fn is_drum_track(&self, track: &Track) -> bool {
        self.instruments
            .get(&track.instrument)
            .is_some_and(|instrument| instrument.preset == PresetId::Kit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn track(extra: Value) -> Track {
        let mut value = json!({
            "id": "t", "name": "T", "role": "texture", "instrument": "i", "volume": 1, "pan": 0
        });
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_track_without_a_priority_takes_its_roles() {
        assert_eq!(track(json!({})).effective_priority(), 4);
        assert_eq!(track(json!({ "role": "melody" })).effective_priority(), 9);
        assert_eq!(track(json!({ "priority": 0 })).effective_priority(), 0);
    }

    #[test]
    fn a_priority_may_be_spelled_with_a_fraction_part_and_is_written_whole() {
        let parsed = track(json!({ "priority": 6.0 }));
        assert_eq!(parsed.priority, Some(6));
        assert_eq!(serde_json::to_value(&parsed).unwrap()["priority"], json!(6));
        assert!(track(json!({})).priority.is_none());
        assert!(serde_json::to_value(track(json!({})))
            .unwrap()
            .get("priority")
            .is_none());
    }
}
