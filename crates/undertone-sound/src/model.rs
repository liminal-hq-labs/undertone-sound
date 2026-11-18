// The song document: types and the wire format
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! These types are the wire format of a song document (`<id>.song.json`). With the `ts` feature
//! they also export TypeScript bindings. Maps that hold document order (`instruments`, `clips`,
//! an instrument's `engines`) are `IndexMap`s, so reading and writing a song keeps its keys where
//! the file had them.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The `format` every song file carries, so a stray JSON file is never mistaken for one.
pub const SONG_FORMAT: &str = "undertone-sound/song";
/// The song document version this crate reads and writes.
pub const SONG_VERSION: u32 = 1;

macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident, $rename:literal, [$($variant:ident => $text:literal),+ $(,)?]) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
        #[serde(rename_all = $rename)]
        #[cfg_attr(feature = "ts", ts(export))]
        pub enum $name {
            $(
                #[serde(rename = $text)]
                #[cfg_attr(feature = "ts", ts(rename = $text))]
                #[doc = concat!("The word `", $text, "`.")]
                $variant,
            )+
        }

        impl $name {
            /// Every value, in declaration order.
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
	/// What kind of author wrote a take or a message.
	AuthorKind, "lowercase", [Human => "human", Agent => "agent"]
);

/// Who wrote a take or a message: a person, or an agent such as a model, which may be named. A
/// host maps its own words onto the two kinds.
///
/// In a song file an author is an object: `{ "kind": "human" }`, `{ "kind": "agent" }` or
/// `{ "kind": "agent", "name": "Claude" }`. Only an agent has a name, which is free text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Author {
	/// Whether a person or an agent wrote it.
	pub kind: AuthorKind,
	/// The agent's name, such as the model that wrote it. Left out when there is none.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub name: Option<String>,
}

impl Author {
	/// A person.
	pub fn human() -> Author {
		Author {
			kind: AuthorKind::Human,
			name: None,
		}
	}

	/// An agent with no name given.
	pub fn agent() -> Author {
		Author {
			kind: AuthorKind::Agent,
			name: None,
		}
	}

	/// This author with an agent's name.
	pub fn named(mut self, name: impl Into<String>) -> Author {
		self.name = Some(name.into());
		self
	}
}

text_enum!(
	/// The sound engines a song can be played on.
	EngineId, "lowercase",
	[Undertone => "undertone", Opl2 => "opl2", Opl3 => "opl3", PcSpeaker => "pc-speaker"]
);

text_enum!(
	/// What a track does in the song. It sets the default instrument and how a tool draws the track.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
/// A key: a tonic and a scale.
pub struct Key {
	/// Pitch class of the tonic, such as `"d"`, `"f#"` or `"bb"`.
	pub root: String,
	/// The scale.
	pub scale: ScaleName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
/// A named instrument: a preset and its overrides.
pub struct Instrument {
	/// A name to show.
	pub name: String,
	/// The preset the instrument starts from.
	pub preset: PresetId,
	/// Per-engine voice overrides layered over the preset, keyed by engine.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(
		feature = "ts",
		ts(
			type = "Partial<Record<\"undertone\" | \"opl2\" | \"opl3\" | \"pc-speaker\", Record<string, unknown>>>",
			optional
		)
	)]
	pub engines: Option<IndexMap<EngineId, Map<String, Value>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
/// One voice of the song: what it plays, how loud and where.
pub struct Track {
	/// The track's id, unique in the song.
	pub id: String,
	/// A name to show.
	pub name: String,
	/// What the track does.
	pub role: TrackRole,
	/// Id of an entry in `Song.instruments`.
	pub instrument: String,
	/// Track level, 0 to 1, multiplied into every note's velocity.
	pub volume: f64,
	/// Stereo position, -1 (left) to 1 (right).
	pub pan: f64,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// Whether the track is muted.
	pub mute: Option<bool>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// Whether the track is soloed.
	pub solo: Option<bool>,
	/// How much the track's notes matter when an OPL chip runs out of channels, 0 to 9. Left out,
	/// the role decides (`TrackRole::default_priority`).
	#[serde(
		default,
		skip_serializing_if = "Option::is_none",
		with = "whole_priority"
	)]
	#[cfg_attr(feature = "ts", ts(optional, type = "number"))]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
/// A stretch of the song, measured in bars.
pub struct Section {
	/// The section's id, unique in the song.
	pub id: String,
	/// A name to show.
	pub name: String,
	/// Length in bars.
	#[serde(with = "whole_number")]
	#[cfg_attr(feature = "ts", ts(type = "number"))]
	pub bars: u32,
}

/// One note in a take. Times are in grid steps from the start of the section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Note {
	/// Onset step, 0 up to but not including the section's length in steps.
	pub at: f64,
	/// A note name (`"d4"`, `"f#3"`, `"bb2"`), or a drum piece on a drum track.
	pub pitch: String,
	/// Length in steps, at least 1.
	pub len: f64,
	/// Strength, 0 to 1 (default 0.8).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub vel: Option<f64>,
	/// Stereo position for this note, -1 to 1, in place of the take's and the track's pan.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub pan: Option<f64>,
	/// Stereo motion over this note's length, in place of its own `pan` and the take's.
	#[serde(default, rename = "panMotion", skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional, rename = "panMotion"))]
	pub pan_motion: Option<PanMotion>,
}

text_enum!(
	/// How a `slide` moves between its two positions.
	PanCurve, "lowercase",
	[Linear => "linear", Smooth => "smooth"]
);

text_enum!(
	/// The wave a `wobble` follows.
	WobbleShape, "lowercase",
	[Sine => "sine", Triangle => "triangle"]
);

/// How often a `jump` moves to its next position: with every note of the take (a chord moves
/// together), or every so many steps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JumpEvery {
	/// Moves with every note of the take.
	Note,
	/// Moves every so many steps.
	Steps(f64),
}

impl Serialize for JumpEvery {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			JumpEvery::Note => serializer.serialize_str("note"),
			JumpEvery::Steps(steps) => serializer.serialize_f64(*steps),
		}
	}
}

impl<'de> Deserialize<'de> for JumpEvery {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let message = "expected \"note\" or a number of steps";
		match Value::deserialize(deserializer)? {
			Value::String(word) if word == "note" => Ok(JumpEvery::Note),
			Value::Number(number) => number
				.as_f64()
				.map(JumpEvery::Steps)
				.ok_or_else(|| serde::de::Error::custom(message)),
			_ => Err(serde::de::Error::custom(message)),
		}
	}
}

/// Stereo motion: where a note or a take's notes sit over time, in place of a fixed pan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum PanMotion {
	/// Moves from one position to another: over the note's length on a note, over the section on
	/// a take.
	Slide {
		/// Where the slide starts, -1 to 1.
		from: f64,
		/// Where the slide ends, -1 to 1.
		to: f64,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		#[cfg_attr(feature = "ts", ts(optional))]
		/// How the slide moves; `linear` when left out.
		curve: Option<PanCurve>,
	},
	/// Swings about a centre; `period` is in steps (default one bar) and `phase` counts from the
	/// section's start.
	Wobble {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		#[cfg_attr(feature = "ts", ts(optional))]
		/// The position the wobble swings about, -1 to 1; 0 when left out.
		centre: Option<f64>,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		#[cfg_attr(feature = "ts", ts(optional))]
		/// How far it swings from the centre, 0 to 1.
		depth: Option<f64>,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		#[cfg_attr(feature = "ts", ts(optional))]
		/// The length of one swing in steps.
		period: Option<f64>,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		#[cfg_attr(feature = "ts", ts(optional))]
		/// The wave; `sine` when left out.
		shape: Option<WobbleShape>,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		#[cfg_attr(feature = "ts", ts(optional))]
		/// Steps into the cycle where the swing starts.
		phase: Option<f64>,
	},
	/// Steps through 2 to 8 positions, one at a time.
	Jump {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		#[cfg_attr(feature = "ts", ts(optional))]
		/// The positions, -1 to 1; `DEFAULT_JUMP_POSITIONS` when left out.
		positions: Option<Vec<f64>>,
		#[cfg_attr(feature = "ts", ts(type = "\"note\" | number"))]
		/// How often the jump moves on.
		every: JumpEvery,
	},
}

/// The positions of a `jump` that does not say: left, then right.
pub const DEFAULT_JUMP_POSITIONS: [f64; 2] = [-0.6, 0.6];
/// The fewest and the most positions a `jump` may list.
pub const JUMP_POSITIONS_MIN: usize = 2;
/// The most positions a `jump` may list.
pub const JUMP_POSITIONS_MAX: usize = 8;
/// How far a `wobble` that does not say swings from its centre.
pub const DEFAULT_WOBBLE_DEPTH: f64 = 0.5;

text_enum!(
	/// How a revision step came about: the model's first answer, another go, a hand edit, or the
	/// kept bars joined back with the new ones.
	StepKind, "lowercase",
	[Model => "model", Retry => "retry", Edit => "edit", Splice => "splice"]
);

/// The most steps one revision keeps, so a take's history cannot grow the song file without end.
pub const MAX_REVISION_STEPS: usize = 20;

/// What a revision was asked with, all of it optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct RevisionSettings {
	/// Whether the model thought before it answered.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub think: Option<bool>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// The sampling temperature.
	pub temperature: Option<f64>,
	/// The bars (counted from 1) the request was limited to.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub bars: Option<Vec<u32>>,
}

/// The notes of one bar in a step that holds only what changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct BarNotes {
	/// The bar, counted from 1.
	pub bar: u32,
	/// Every note whose onset is in the bar; none means the bar is empty.
	pub notes: Vec<Note>,
}

/// One version of a revised take. Exactly one of `notes`, `mini` and `bars` holds its content:
/// `notes` or `mini` is a whole snapshot (the first and the current step are always one), `bars`
/// lists the bars that differ from the step before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct RevisionStep {
	/// Unique in the take (`s1`, `s2`, ...).
	pub id: String,
	/// How the step came about.
	pub kind: StepKind,
	/// ISO 8601 timestamp.
	pub at: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// A word about the step.
	pub comment: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// A whole snapshot of the take's notes.
	pub notes: Option<Vec<Note>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// A whole snapshot of the take's pattern.
	pub mini: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// The bars that differ from the step before.
	pub bars: Option<Vec<BarNotes>>,
}

/// A take's link to the take it revises,, and the steps it went through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
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
	#[cfg_attr(feature = "ts", ts(optional))]
	/// What the revision was asked with.
	pub settings: Option<RevisionSettings>,
	/// One to `MAX_REVISION_STEPS` versions, oldest first.
	pub steps: Vec<RevisionStep>,
	/// Id of the step the take is on.
	pub current: String,
	/// How many steps were dropped by folding the history (all but the first and the current).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub folded: Option<u32>,
}

/// One idea for what a track plays in a section. A take holds either explicit `notes` or a `mini`
/// pattern in Undertone mini-notation, where one cycle is one bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Take {
	/// The take's id, unique in its slot.
	pub id: String,
	/// A name to show.
	pub name: String,
	/// Who wrote the take.
	pub by: Author,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// The notes the take plays.
	pub notes: Option<Vec<Note>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	/// A mini-notation pattern the take plays, one cycle a bar.
	pub mini: Option<String>,
	/// Stereo position for this take's notes, -1 to 1, in place of the track's pan.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub pan: Option<f64>,
	/// Stereo motion for this take's notes (or its pattern's), over the whole section, in place
	/// of the take's `pan`. A note's own `pan` or `panMotion` wins over it.
	#[serde(default, rename = "panMotion", skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional, rename = "panMotion"))]
	pub pan_motion: Option<PanMotion>,
	/// A word about the idea, from whoever wrote it.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub comment: Option<String>,
	/// Set when this take revises another and keeps the steps it went through.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub revision: Option<Revision>,
}

/// Every take a track has for one section, and which of them plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ClipSlot {
	/// Id of the take that plays, or `null` for silence.
	pub active: Option<String>,
	/// Every take the slot holds.
	pub takes: Vec<Take>,
}

/// A message left in the song for the other collaborator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Message {
	/// Who wrote the message.
	pub by: Author,
	/// The message.
	pub text: String,
	/// ISO 8601 timestamp.
	pub at: String,
	/// Set once the other collaborator has dealt with it.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub done: Option<bool>,
}

/// How the song is mixed, where it differs from the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Mix {
	/// The pan law: each side's level at the centre in dB, from 0 to -6. Default -3.
	#[serde(default, rename = "panLaw", skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional, rename = "panLaw"))]
	pub pan_law: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
/// A whole song.
pub struct Song {
	#[cfg_attr(feature = "ts", ts(type = "\"undertone-sound/song\""))]
	/// The format id, `SONG_FORMAT`.
	pub format: String,
	#[serde(with = "whole_number")]
	#[cfg_attr(feature = "ts", ts(type = "1"))]
	/// The document version, `SONG_VERSION`.
	pub version: u32,
	/// The song's title.
	pub title: String,
	/// Tempo in beats per minute.
	pub tempo: f64,
	#[serde(rename = "beatsPerBar", with = "whole_number")]
	#[cfg_attr(feature = "ts", ts(rename = "beatsPerBar", type = "number"))]
	/// Beats in one bar.
	pub beats_per_bar: u32,
	/// Grid steps per beat: 4 gives a sixteenth-note grid in 4/4.
	#[serde(rename = "stepsPerBeat", with = "whole_number")]
	#[cfg_attr(feature = "ts", ts(rename = "stepsPerBeat", type = "number"))]
	pub steps_per_beat: u32,
	/// The song's key.
	pub key: Key,
	/// The engine the song is meant for.
	pub engine: EngineId,
	/// Instruments by id, in document order.
	pub instruments: IndexMap<String, Instrument>,
	/// The song's tracks.
	pub tracks: Vec<Track>,
	/// The song's sections.
	pub sections: Vec<Section>,
	/// Section ids in play order; a section may appear more than once.
	pub arrangement: Vec<String>,
	/// `clips[trackId][sectionId]`. A missing slot is silence.
	pub clips: IndexMap<String, IndexMap<String, ClipSlot>>,
	/// Messages left in the song.
	pub messages: Vec<Message>,
	/// How the song is mixed, where it differs from the defaults.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "ts", ts(optional))]
	pub mix: Option<Mix>,
}

/// Grid steps in one bar.
pub fn steps_per_bar(beats_per_bar: u32, steps_per_beat: u32) -> u32 {
	beats_per_bar * steps_per_beat
}

impl Song {
	/// Grid steps in one bar of this song.
	pub fn steps_per_bar(&self) -> u32 {
		steps_per_bar(self.beats_per_bar, self.steps_per_beat)
	}

	/// Grid steps in a section.
	pub fn section_steps(&self, section: &Section) -> u32 {
		section.bars * self.steps_per_bar()
	}

	/// The section with this id.
	pub fn find_section(&self, id: &str) -> Option<&Section> {
		self.sections.iter().find(|section| section.id == id)
	}

	/// The track with this id.
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
	fn an_author_is_a_kind_and_an_agent_may_carry_a_name() {
		let written = |author: &Author| serde_json::to_value(author).unwrap();
		assert_eq!(written(&Author::human()), json!({ "kind": "human" }));
		assert_eq!(written(&Author::agent()), json!({ "kind": "agent" }));
		assert_eq!(
			written(&Author::agent().named("Claude")),
			json!({ "kind": "agent", "name": "Claude" })
		);
		let read: Author =
			serde_json::from_value(json!({ "kind": "agent", "name": "Claude" })).unwrap();
		assert_eq!(read, Author::agent().named("Claude"));
		assert!(serde_json::from_value::<Author>(json!("human")).is_err());
		assert!(serde_json::from_value::<Author>(json!({ "kind": "robot" })).is_err());
	}

	#[test]
	fn the_retired_presets_are_not_words_of_the_format() {
		for word in ["bass-saw", "pad-bright", "drone"] {
			assert_eq!(PresetId::from_word(word), None);
		}
		assert_eq!(PresetId::ALL.len(), 10);
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
		assert!(
			serde_json::to_value(track(json!({})))
				.unwrap()
				.get("priority")
				.is_none()
		);
	}
}
