// The studio's presets and drum pieces as Undertone voices, and a song's overrides over them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Undertone's `fmVoice` and `fmPresets` (`instruments.ts`), and the studio's `PRESET_VOICES`,
//! `DRUM_VOICES`, `instrumentVoice`, `drumAdjustment` and `drumLayers`
//! (`apps/studio/src/engines/undertone/voices.ts`), field for field and in the same key order. A
//! kit reads its flat `<piece>.<key>` overrides as `docs/ENGINES.md` ("Undertone's kit") says.

use fizz_audio_song::model::{DrumPiece, PresetId, Song};
use serde_json::Value;

use crate::error::Error;
use crate::math;
use crate::params::Patch;
use crate::pitch::{json_number, note_to_frequency, Pitch};

/// A patch from `(field, number)` pairs and `(field, word)` pairs, in order.
fn patch(fields: &[(&str, Field)]) -> Patch {
    fields
        .iter()
        .map(|(name, value)| {
            let value = match value {
                Field::Num(n) => json_number(*n),
                Field::Word(w) => Value::String((*w).to_string()),
            };
            (name.to_string(), value)
        })
        .collect()
}

/// One field of a written-out patch.
#[derive(Debug, Clone, Copy)]
enum Field {
    Num(f64),
    Word(&'static str),
}

use Field::{Num, Word};

/// Tone.js's shape of an envelope, as `fmVoice` takes it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Envelope {
    pub attack: Option<f64>,
    pub decay: Option<f64>,
    pub sustain: Option<f64>,
    pub release: Option<f64>,
}

/// A Tone.js-shaped FM definition (`FmVoiceDefinition`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FmDefinition {
    pub harmonicity: Option<f64>,
    pub modulation_index: Option<f64>,
    pub oscillator: Option<&'static str>,
    pub modulation: Option<&'static str>,
    pub envelope: Envelope,
    pub modulation_envelope: Envelope,
    pub level: Option<f64>,
    pub pan: Option<f64>,
}

/// `fmVoice`: Tone's `modulationIndex` is relative to the carrier, Undertone's `fmIndex` to the
/// modulator, so the index is divided by the harmonicity. Fields left out stay unset.
pub fn fm_voice(definition: &FmDefinition) -> Result<Patch, Error> {
    if let Some(h) = definition.harmonicity {
        if h.is_nan() || h <= 0.0 {
            return Err(Error::new(format!(
                "fmVoice() harmonicity must be positive, got {}",
                fizz_audio_song::js::number_to_string(h)
            )));
        }
    }
    if let Some(index) = definition.modulation_index {
        if index.is_nan() || index < 0.0 {
            return Err(Error::new(format!(
                "fmVoice() modulationIndex must not be negative, got {}",
                fizz_audio_song::js::number_to_string(index)
            )));
        }
    }
    let mut out = Patch::new();
    let mut put = |field: &str, value: Value| {
        out.insert(field.to_string(), value);
    };
    let number = |n: f64| json_number(n);
    if let Some(wave) = definition.oscillator {
        put("soundType", Value::String(wave.to_string()));
    }
    let envelope = definition.envelope;
    for (field, value) in [
        ("attack", envelope.attack),
        ("decay", envelope.decay),
        ("sustain", envelope.sustain),
        ("release", envelope.release),
        ("gainLevel", definition.level),
        ("pan", definition.pan),
    ] {
        if let Some(value) = value {
            put(field, number(value));
        }
    }
    if let Some(index) = definition.modulation_index.filter(|index| *index > 0.0) {
        put(
            "fmIndex",
            number(index / definition.harmonicity.unwrap_or(1.0)),
        );
        if let Some(h) = definition.harmonicity {
            put("fmHarmonicity", number(h));
        }
        if let Some(wave) = definition.modulation {
            put("fmWave", Value::String(wave.to_string()));
        }
        let modulation = definition.modulation_envelope;
        for (field, value) in [
            ("fmAttack", modulation.attack),
            ("fmDecay", modulation.decay),
            ("fmSustain", modulation.sustain),
            ("fmRelease", modulation.release),
        ] {
            if let Some(value) = value {
                put(field, number(value));
            }
        }
    }
    Ok(out)
}

fn envelope(attack: f64, decay: f64, sustain: Option<f64>, release: f64) -> Envelope {
    Envelope {
        attack: Some(attack),
        decay: Some(decay),
        sustain,
        release: Some(release),
    }
}

/// Undertone's `fmPresets`, by the name the package gives each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmPreset {
    GlassBell,
    RoundBass,
    SoftPad,
    Pluck,
    Lead,
}

/// The definition behind each of Undertone's FM presets.
pub fn fm_preset_definition(preset: FmPreset) -> FmDefinition {
    match preset {
        FmPreset::GlassBell => FmDefinition {
            harmonicity: Some(3.5),
            modulation_index: Some(5.0),
            envelope: envelope(0.002, 0.7, Some(0.0), 0.9),
            modulation_envelope: envelope(0.0, 0.35, None, 0.2),
            level: Some(0.3),
            ..FmDefinition::default()
        },
        FmPreset::RoundBass => FmDefinition {
            harmonicity: Some(1.0),
            modulation_index: Some(3.5),
            modulation: Some("triangle"),
            envelope: envelope(0.005, 0.2, Some(0.6), 0.12),
            modulation_envelope: envelope(0.0, 0.18, Some(0.3), 0.1),
            level: Some(0.5),
            ..FmDefinition::default()
        },
        FmPreset::SoftPad => FmDefinition {
            harmonicity: Some(2.0),
            modulation_index: Some(1.2),
            envelope: envelope(0.4, 0.3, Some(0.8), 0.9),
            modulation_envelope: envelope(0.5, 0.6, Some(0.5), 0.8),
            level: Some(0.22),
            ..FmDefinition::default()
        },
        FmPreset::Pluck => FmDefinition {
            harmonicity: Some(2.0),
            modulation_index: Some(6.0),
            envelope: envelope(0.003, 0.2, Some(0.25), 0.15),
            modulation_envelope: envelope(0.0, 0.12, Some(0.1), 0.1),
            level: Some(0.35),
            ..FmDefinition::default()
        },
        FmPreset::Lead => FmDefinition {
            harmonicity: Some(2.0),
            modulation_index: Some(3.4),
            envelope: envelope(0.008, 0.12, Some(0.52), 0.16),
            modulation_envelope: envelope(0.004, 0.16, Some(0.14), 0.1),
            level: Some(0.32),
            ..FmDefinition::default()
        },
    }
}

/// `fmPresets.<name>` as a patch.
pub fn fm_preset(preset: FmPreset) -> Patch {
    fm_voice(&fm_preset_definition(preset)).expect("Undertone's presets are valid")
}

/// `{ ...base, ...extra }`.
fn with(mut base: Patch, extra: &[(&str, Field)]) -> Patch {
    for (field, value) in patch(extra) {
        base.insert(field, value);
    }
    base
}

/// The studio's `PRESET_VOICES`: each pitched preset as an Undertone voice (sends on orbit 0). The
/// kit has none.
pub fn preset_voice(preset: PresetId) -> Patch {
    match preset {
        PresetId::Lead => with(
            fm_preset(FmPreset::Lead),
            &[
                ("delayLevel", Num(0.12)),
                ("delayTime", Num(0.375)),
                ("delayFeedback", Num(0.3)),
            ],
        ),
        PresetId::SquareLead => patch(&[
            ("soundType", Word("square")),
            ("attack", Num(0.004)),
            ("decay", Num(0.12)),
            ("sustain", Num(0.6)),
            ("release", Num(0.1)),
            ("gainLevel", Num(0.16)),
            ("filterCutoff", Num(2400.0)),
        ]),
        PresetId::SoftLead => patch(&[
            ("soundType", Word("triangle")),
            ("attack", Num(0.02)),
            ("decay", Num(0.2)),
            ("sustain", Num(0.7)),
            ("release", Num(0.3)),
            ("gainLevel", Num(0.36)),
            ("roomLevel", Num(0.2)),
        ]),
        PresetId::Pluck => with(fm_preset(FmPreset::Pluck), &[("roomLevel", Num(0.15))]),
        PresetId::Bell => with(fm_preset(FmPreset::GlassBell), &[("roomLevel", Num(0.3))]),
        PresetId::Pad => with(
            fm_preset(FmPreset::SoftPad),
            &[("roomLevel", Num(0.35)), ("roomSize", Num(4.0))],
        ),
        PresetId::Organ => patch(&[
            ("soundType", Word("sine")),
            ("fmIndex", Num(0.6)),
            ("fmHarmonicity", Num(2.0)),
            ("attack", Num(0.01)),
            ("decay", Num(0.05)),
            ("sustain", Num(0.9)),
            ("release", Num(0.08)),
            ("gainLevel", Num(0.22)),
        ]),
        PresetId::Bass => fm_preset(FmPreset::RoundBass),
        PresetId::SubBass => patch(&[
            ("soundType", Word("sine")),
            ("attack", Num(0.005)),
            ("decay", Num(0.3)),
            ("sustain", Num(0.8)),
            ("release", Num(0.1)),
            ("gainLevel", Num(0.6)),
        ]),
        PresetId::BassSaw => patch(&[
            ("soundType", Word("sawtooth")),
            ("attack", Num(0.005)),
            ("decay", Num(0.12)),
            ("sustain", Num(0.45)),
            ("release", Num(0.08)),
            ("gainLevel", Num(0.3)),
            ("filterCutoff", Num(900.0)),
        ]),
        PresetId::PadBright => patch(&[
            ("soundType", Word("square")),
            ("attack", Num(0.3)),
            ("decay", Num(0.4)),
            ("sustain", Num(0.7)),
            ("release", Num(0.9)),
            ("gainLevel", Num(0.12)),
            ("filterCutoff", Num(1800.0)),
            ("roomLevel", Num(0.3)),
            ("roomSize", Num(3.0)),
        ]),
        PresetId::Drone => patch(&[
            ("soundType", Word("sawtooth")),
            ("attack", Num(0.8)),
            ("decay", Num(0.5)),
            ("sustain", Num(0.8)),
            ("release", Num(1.2)),
            ("gainLevel", Num(0.14)),
            ("filterCutoff", Num(700.0)),
            ("roomLevel", Num(0.4)),
            ("roomSize", Num(4.0)),
        ]),
        PresetId::Kit => Patch::new(),
    }
}

/// One layer of a drum piece: the pitch it sounds at and its voice.
#[derive(Debug, Clone, PartialEq)]
pub struct DrumLayer {
    pub pitch: Pitch,
    pub voice: Patch,
}

fn layer(pitch: &str, fields: &[(&str, Field)]) -> DrumLayer {
    DrumLayer {
        pitch: Pitch::Name(pitch.to_string()),
        voice: patch(fields),
    }
}

/// The studio's `DRUM_VOICES`: each piece as one or more layered voices, all percussive (no
/// sustain). The hats share cut group 1, so a closed hat chokes an open one.
pub fn drum_voices(piece: DrumPiece) -> Vec<DrumLayer> {
    match piece {
        DrumPiece::Kick => vec![layer(
            "a1",
            &[
                ("soundType", Word("sine")),
                ("pitchEnvAmount", Num(24.0)),
                ("pitchDecay", Num(0.05)),
                ("attack", Num(0.001)),
                ("decay", Num(0.28)),
                ("sustain", Num(0.0)),
                ("release", Num(0.05)),
                ("gainLevel", Num(0.95)),
            ],
        )],
        DrumPiece::Snare => vec![
            layer(
                "c4",
                &[
                    ("soundType", Word("white")),
                    ("attack", Num(0.001)),
                    ("decay", Num(0.16)),
                    ("sustain", Num(0.0)),
                    ("release", Num(0.05)),
                    ("gainLevel", Num(0.36)),
                    ("hpfCutoff", Num(1200.0)),
                    ("roomLevel", Num(0.12)),
                ],
            ),
            layer(
                "e3",
                &[
                    ("soundType", Word("triangle")),
                    ("pitchEnvAmount", Num(7.0)),
                    ("pitchDecay", Num(0.03)),
                    ("attack", Num(0.001)),
                    ("decay", Num(0.08)),
                    ("sustain", Num(0.0)),
                    ("release", Num(0.03)),
                    ("gainLevel", Num(0.4)),
                ],
            ),
        ],
        DrumPiece::Clap => vec![layer(
            "c4",
            &[
                ("soundType", Word("white")),
                ("attack", Num(0.002)),
                ("decay", Num(0.12)),
                ("sustain", Num(0.0)),
                ("release", Num(0.06)),
                ("gainLevel", Num(0.34)),
                ("hpfCutoff", Num(900.0)),
                ("filterCutoff", Num(3200.0)),
                ("roomLevel", Num(0.25)),
            ],
        )],
        DrumPiece::Rim => vec![layer(
            "e6",
            &[
                ("soundType", Word("square")),
                ("attack", Num(0.001)),
                ("decay", Num(0.025)),
                ("sustain", Num(0.0)),
                ("release", Num(0.01)),
                ("gainLevel", Num(0.14)),
                ("filterCutoff", Num(3000.0)),
            ],
        )],
        DrumPiece::Hat => vec![layer(
            "c4",
            &[
                ("soundType", Word("white")),
                ("attack", Num(0.001)),
                ("decay", Num(0.04)),
                ("sustain", Num(0.0)),
                ("release", Num(0.02)),
                ("gainLevel", Num(0.2)),
                ("hpfCutoff", Num(7000.0)),
                ("cutGroup", Num(1.0)),
            ],
        )],
        DrumPiece::Openhat => vec![layer(
            "c4",
            &[
                ("soundType", Word("white")),
                ("attack", Num(0.001)),
                ("decay", Num(0.32)),
                ("sustain", Num(0.0)),
                ("release", Num(0.08)),
                ("gainLevel", Num(0.18)),
                ("hpfCutoff", Num(6000.0)),
                ("cutGroup", Num(1.0)),
            ],
        )],
        DrumPiece::Tom => vec![layer(
            "d2",
            &[
                ("soundType", Word("sine")),
                ("pitchEnvAmount", Num(7.0)),
                ("pitchDecay", Num(0.08)),
                ("attack", Num(0.001)),
                ("decay", Num(0.32)),
                ("sustain", Num(0.0)),
                ("release", Num(0.06)),
                ("gainLevel", Num(0.7)),
            ],
        )],
        DrumPiece::Crash => vec![layer(
            "c4",
            &[
                ("soundType", Word("white")),
                ("attack", Num(0.001)),
                ("decay", Num(1.3)),
                ("sustain", Num(0.0)),
                ("release", Num(0.3)),
                ("gainLevel", Num(0.2)),
                ("hpfCutoff", Num(4200.0)),
                ("roomLevel", Num(0.3)),
            ],
        )],
    }
}

/// An instrument's Undertone overrides, if it has any.
fn overrides<'a>(
    song: &'a Song,
    instrument_id: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    song.instruments
        .get(instrument_id)?
        .engines
        .as_ref()?
        .get(&fizz_audio_song::model::EngineId::Undertone)
}

/// Drops the fields set to `null`, keeping the others in order.
fn without_nulls(mut voice: Patch) -> Patch {
    voice.retain(|_, value| !value.is_null());
    voice
}

/// `instrumentVoice`: the preset's voice, then the song's own overrides over it. `null` turns an
/// optional part of the voice off, so it is left out. A kit, or an unknown instrument, has none.
pub fn instrument_voice(song: &Song, instrument_id: &str) -> Patch {
    let Some(instrument) = song.instruments.get(instrument_id) else {
        return Patch::new();
    };
    if instrument.preset == PresetId::Kit {
        return Patch::new();
    }
    let mut merged = preset_voice(instrument.preset);
    if let Some(overrides) = overrides(song, instrument_id) {
        for (field, value) in overrides {
            merged.insert(field.clone(), value.clone());
        }
    }
    without_nulls(merged)
}

/// How one drum piece is adjusted on a kit (`DrumAdjustment`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrumAdjustment {
    /// Loudness multiplier, 1 as built.
    pub level: f64,
    /// Decay length multiplier, 1 as built; 1 for a piece with a voice of its own.
    pub decay: f64,
    /// Pitch offset in semitones.
    pub tune: f64,
}

/// The keys of a kit's flat `<piece>.<key>` overrides that are not voice fields.
const KIT_CONTROLS: [&str; 5] = ["level", "decay", "tune", "layers", "pitch"];

/// A kit's overrides for one piece, without the `<piece>.` prefix, in song order.
fn piece_overrides(song: &Song, instrument_id: &str, piece: DrumPiece) -> Vec<(String, Value)> {
    let prefix = format!("{}.", piece.as_str());
    overrides(song, instrument_id)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    key.strip_prefix(&prefix)
                        .map(|field| (field.to_string(), value.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn get<'a>(overrides: &'a [(String, Value)], field: &str) -> Option<&'a Value> {
    // A JSON object's keys are unique, so the first match is the only one.
    overrides
        .iter()
        .find(|(key, _)| key == field)
        .map(|(_, value)| value)
}

/// `<piece>.layers`, floored and at least 0, when the piece has a voice of its own.
fn own_layers(overrides: &[(String, Value)]) -> Option<usize> {
    let value = get(overrides, "layers")?.as_f64()?;
    Some(math::max(0.0, value.floor()) as usize)
}

/// `drumAdjustment`: a piece's `level`, `decay` and `tune` on a kit.
pub fn drum_adjustment(song: &Song, instrument_id: &str, piece: DrumPiece) -> DrumAdjustment {
    let overrides = piece_overrides(song, instrument_id, piece);
    let read = |field: &str, fallback: f64| {
        get(&overrides, field)
            .and_then(Value::as_f64)
            .unwrap_or(fallback)
    };
    let own = own_layers(&overrides).is_some();
    DrumAdjustment {
        level: read("level", 1.0),
        decay: if own { 1.0 } else { read("decay", 1.0) },
        tune: read("tune", 0.0),
    }
}

/// Sets voice fields over a voice, in order, and drops the ones that end up `null`.
fn with_fields(mut voice: Patch, fields: &[(String, Value)]) -> Patch {
    for (field, value) in fields {
        voice.insert(field.clone(), value.clone());
    }
    without_nulls(voice)
}

/// A number field of a voice, or `fallback` when it has none (`voice.field ?? fallback`).
fn field_or(voice: &Patch, field: &str, fallback: f64) -> f64 {
    match voice.get(field) {
        None | Some(Value::Null) => fallback,
        Some(value) => value.as_f64().unwrap_or(f64::NAN),
    }
}

/// `drumLayers`: a drum piece's layers on a song's kit, with its flat keys applied
/// (`docs/ENGINES.md`, "Undertone's kit").
pub fn drum_layers(
    song: &Song,
    instrument_id: &str,
    piece: DrumPiece,
) -> Result<Vec<DrumLayer>, Error> {
    let overrides = piece_overrides(song, instrument_id, piece);
    let DrumAdjustment { level, decay, tune } = drum_adjustment(song, instrument_id, piece);
    let layers = own_layers(&overrides);
    let fields: Vec<(String, Value)> = overrides
        .iter()
        .filter(|(field, _)| {
            !KIT_CONTROLS.contains(&field.as_str()) || (layers.is_some() && field == "decay")
        })
        .cloned()
        .collect();
    let first_pitch = get(&overrides, "pitch").and_then(Pitch::from_value);
    let tuned = |pitch: Pitch| -> Result<Pitch, Error> {
        if tune == 0.0 {
            Ok(pitch)
        } else {
            Ok(Pitch::Hz(
                note_to_frequency(&pitch)? * math::pow(2.0, tune / 12.0),
            ))
        }
    };
    let scaled = |voice: &Patch| -> Patch {
        let mut out = voice.clone();
        out.insert(
            "gainLevel".into(),
            json_number(field_or(voice, "gainLevel", 0.5) * level),
        );
        out.insert(
            "decay".into(),
            json_number(field_or(voice, "decay", 0.1) * decay),
        );
        out.insert(
            "release".into(),
            json_number(field_or(voice, "release", 0.05) * decay),
        );
        out
    };
    let built = drum_voices(piece);
    let Some(layers) = layers else {
        return built
            .into_iter()
            .enumerate()
            .map(|(index, layer)| {
                let first = index == 0;
                let pitch = match (&first_pitch, first) {
                    (Some(pitch), true) => pitch.clone(),
                    _ => layer.pitch,
                };
                let voice = if first {
                    with_fields(layer.voice, &fields)
                } else {
                    layer.voice
                };
                Ok(DrumLayer {
                    pitch: tuned(pitch)?,
                    voice: scaled(&voice),
                })
            })
            .collect();
    };
    if layers == 0 {
        return Ok(Vec::new());
    }
    let own = with_fields(Patch::new(), &fields);
    let voice = if level == 1.0 {
        own
    } else {
        let gain = field_or(&own, "gainLevel", 0.8) * level;
        let mut voice = own;
        voice.insert("gainLevel".into(), json_number(gain));
        voice
    };
    let first_pitch = first_pitch.unwrap_or_else(|| built[0].pitch.clone());
    let mut out = vec![DrumLayer {
        pitch: tuned(first_pitch)?,
        voice,
    }];
    for layer in built.into_iter().skip(1).take(layers.saturating_sub(1)) {
        out.push(DrumLayer {
            pitch: tuned(layer.pitch)?,
            voice: scaled(&layer.voice),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fizz_audio_song::model::EngineId;
    use serde_json::json;

    fn song_with_kit(keys: Value) -> Song {
        let text = include_str!("../../../fixtures/undertone/inputs/undertone-voices.song.json");
        let mut song: Song = serde_json::from_str(text).unwrap();
        let kit = song.instruments.get_mut("kit").unwrap();
        let map = keys.as_object().unwrap().clone();
        kit.engines = Some([(EngineId::Undertone, map)].into_iter().collect());
        song
    }

    fn number(voice: &Patch, field: &str) -> f64 {
        voice[field].as_f64().unwrap()
    }

    #[test]
    fn fm_voice_divides_the_index_by_the_harmonicity() {
        let bell = fm_preset(FmPreset::GlassBell);
        assert_eq!(number(&bell, "fmIndex"), 5.0 / 3.5);
        let keys: Vec<&str> = bell.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "attack",
                "decay",
                "sustain",
                "release",
                "gainLevel",
                "fmIndex",
                "fmHarmonicity",
                "fmAttack",
                "fmDecay",
                "fmRelease"
            ]
        );
        assert!(fm_voice(&FmDefinition {
            harmonicity: Some(0.0),
            ..FmDefinition::default()
        })
        .is_err());
    }

    #[test]
    fn the_lead_adds_its_echo_after_the_fm_fields() {
        let lead = preset_voice(PresetId::Lead);
        assert_eq!(
            lead.keys().last().map(String::as_str),
            Some("delayFeedback")
        );
        assert_eq!(number(&lead, "fmIndex"), 1.7);
    }

    #[test]
    fn instrument_overrides_replace_and_null_removes() {
        let text = include_str!("../../../fixtures/undertone/inputs/title-theme.song.json");
        let song: Song = serde_json::from_str(text).unwrap();
        let pad = instrument_voice(&song, "pad");
        assert!(!pad.contains_key("fmIndex"));
        assert_eq!(number(&pad, "roomSize"), 6.0);
        assert_eq!(pad.get("soundType"), Some(&json!("sine")));
        assert!(instrument_voice(&song, "kit").is_empty());
    }

    #[test]
    fn built_in_layers_scale_by_level_decay_and_tune() {
        let song = song_with_kit(json!({ "snare.level": 0.5, "snare.decay": 2, "snare.tune": 12 }));
        let layers = drum_layers(&song, "kit", DrumPiece::Snare).unwrap();
        assert_eq!(number(&layers[0].voice, "gainLevel"), 0.18);
        assert_eq!(number(&layers[1].voice, "decay"), 0.16);
        let e3 = note_to_frequency(&Pitch::Name("e3".into())).unwrap();
        assert_eq!(layers[1].pitch, Pitch::Hz(e3 * 2.0));
    }

    #[test]
    fn a_field_and_a_pitch_set_the_first_layer_only() {
        let song = song_with_kit(
            json!({ "snare.hpfCutoff": 2000, "snare.roomLevel": null, "snare.pitch": "d4" }),
        );
        let layers = drum_layers(&song, "kit", DrumPiece::Snare).unwrap();
        assert_eq!(number(&layers[0].voice, "hpfCutoff"), 2000.0);
        assert!(!layers[0].voice.contains_key("roomLevel"));
        assert_eq!(layers[0].pitch, Pitch::Name("d4".into()));
        assert!(!layers[1].voice.contains_key("hpfCutoff"));
        assert_eq!(layers[1].pitch, Pitch::Name("e3".into()));
    }

    #[test]
    fn layers_give_a_piece_a_voice_of_its_own() {
        let song = song_with_kit(json!({
            "kick.layers": 1, "kick.soundType": "sine", "kick.decay": 0.12, "kick.pitch": "c1", "kick.slideTime": 0.09,
            "snare.layers": 2, "snare.soundType": "pink", "snare.level": 0.5,
            "rim.layers": 0,
        }));
        let kick = drum_layers(&song, "kit", DrumPiece::Kick).unwrap();
        assert_eq!(kick.len(), 1);
        assert_eq!(kick[0].pitch, Pitch::Name("c1".into()));
        assert_eq!(
            serde_json::to_value(&kick[0].voice).unwrap(),
            json!({ "soundType": "sine", "decay": 0.12, "slideTime": 0.09 })
        );
        assert_eq!(drum_adjustment(&song, "kit", DrumPiece::Kick).decay, 1.0);
        let snare = drum_layers(&song, "kit", DrumPiece::Snare).unwrap();
        assert_eq!(
            serde_json::to_value(&snare[0].voice).unwrap(),
            json!({ "soundType": "pink", "gainLevel": 0.4 })
        );
        assert_eq!(number(&snare[1].voice, "gainLevel"), 0.2);
        assert!(drum_layers(&song, "kit", DrumPiece::Rim)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_layer_count_that_is_not_a_number_is_ignored() {
        let song = song_with_kit(json!({ "snare.layers": "two", "snare.decay": 2 }));
        let layers = drum_layers(&song, "kit", DrumPiece::Snare).unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(number(&layers[0].voice, "decay"), 0.32);
    }
}
