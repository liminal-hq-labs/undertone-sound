// The PC speaker's one voice: what sounds in each 1/140 s frame, chosen by priority, with chords arpeggiated
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Port of `apps/studio/src/engines/speaker/sequence.ts`. The output is one frequency per frame as
//! an `f32`, as the TypeScript stores it in a `Float32Array`.

use fizz_audio_song::compile::Performance;
use fizz_audio_song::model::{DrumPiece, EngineId, PresetId, Song, TrackRole};
use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::numbers::{js_number, round, to_int32};

/// Frames a second: how often the voice can change pitch.
pub const SPEAKER_FRAME_RATE: f64 = 140.0;
/// The PIT's input clock; the speaker plays `PIT_CLOCK / divisor` Hz.
pub const PIT_CLOCK: f64 = 1_193_182.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordMode {
    Arpeggio,
    Top,
    Bottom,
}

/// An instrument's speaker voice. Numbers are doubles because an override can be any JSON number.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerVoice {
    /// Which instrument wins the voice when several play at once (higher wins).
    pub priority: f64,
    /// The chord word as written; anything but `top` or `bottom` arpeggiates, as in the TypeScript.
    pub chord: String,
    /// Frames each chord note holds in an arpeggio.
    pub arpeggio_frames: f64,
    /// Octaves to move the instrument by; the speaker is thin below about 100 Hz.
    pub octave: f64,
    /// Silent frames before a note ends, so repeated notes are heard apart.
    pub gap_frames: f64,
}

impl SpeakerVoice {
    pub fn chord_mode(&self) -> ChordMode {
        match self.chord.as_str() {
            "top" => ChordMode::Top,
            "bottom" => ChordMode::Bottom,
            _ => ChordMode::Arpeggio,
        }
    }
}

fn role_priority(role: TrackRole) -> f64 {
    match role {
        TrackRole::Melody => 9.0,
        TrackRole::Drums => 8.0,
        TrackRole::Bass => 6.0,
        TrackRole::Harmony => 4.0,
        TrackRole::Texture => 3.0,
    }
}

pub fn default_speaker_voice(preset: PresetId, role: TrackRole) -> SpeakerVoice {
    SpeakerVoice {
        priority: role_priority(role),
        chord: "arpeggio".to_string(),
        arpeggio_frames: 2.0,
        octave: if matches!(preset, PresetId::Bass | PresetId::SubBass | PresetId::BassSaw) {
            1.0
        } else {
            0.0
        },
        gap_frames: 1.0,
    }
}

/// The song's `pc-speaker` overrides for an instrument, with nulls left out.
fn speaker_overrides(song: &Song, instrument_id: &str) -> Map<String, Value> {
    song.instruments
        .get(instrument_id)
        .and_then(|instrument| instrument.engines.as_ref())
        .and_then(|engines| engines.get(&EngineId::PcSpeaker))
        .cloned()
        .unwrap_or_default()
}

fn base_voice(song: &Song, instrument_id: &str) -> SpeakerVoice {
    let preset = song
        .instruments
        .get(instrument_id)
        .map_or(PresetId::Lead, |instrument| instrument.preset);
    let role = song
        .tracks
        .iter()
        .find(|track| track.instrument == instrument_id)
        .map_or(TrackRole::Melody, |track| track.role);
    default_speaker_voice(preset, role)
}

/// An instrument's speaker voice: the role's defaults, then the song's `pc-speaker` overrides.
pub fn speaker_voice(song: &Song, instrument_id: &str) -> SpeakerVoice {
    let mut voice = base_voice(song, instrument_id);
    for (key, value) in speaker_overrides(song, instrument_id) {
        if value.is_null() {
            continue;
        }
        match key.as_str() {
            "priority" => voice.priority = js_number(&value),
            "chord" => {
                voice.chord = match &value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                }
            }
            "arpeggioFrames" => voice.arpeggio_frames = js_number(&value),
            "octave" => voice.octave = js_number(&value),
            "gapFrames" => voice.gap_frames = js_number(&value),
            _ => {}
        }
    }
    voice
}

/// The voice as the instrument editor shows it: the defaults in order, then every override
/// (including keys the speaker does not use), as the TypeScript's object spread leaves it.
pub fn speaker_voice_values(song: &Song, instrument_id: &str) -> Map<String, Value> {
    let base = base_voice(song, instrument_id);
    let mut values = Map::new();
    values.insert(
        "priority".into(),
        crate::numbers::json_number(base.priority),
    );
    values.insert("chord".into(), Value::String(base.chord));
    values.insert(
        "arpeggioFrames".into(),
        crate::numbers::json_number(base.arpeggio_frames),
    );
    values.insert("octave".into(), crate::numbers::json_number(base.octave));
    values.insert(
        "gapFrames".into(),
        crate::numbers::json_number(base.gap_frames),
    );
    for (key, value) in speaker_overrides(song, instrument_id) {
        if !value.is_null() {
            values.insert(key, value);
        }
    }
    values
}

/// Frames each drum lasts when the kit does not say.
pub fn drum_frames(piece: DrumPiece) -> f64 {
    match piece {
        DrumPiece::Kick | DrumPiece::Snare | DrumPiece::Clap | DrumPiece::Openhat => 4.0,
        DrumPiece::Rim | DrumPiece::Hat => 1.0,
        DrumPiece::Tom => 5.0,
        DrumPiece::Crash => 10.0,
    }
}

/// Whether a kit plays a piece and for how many frames, from `<piece>.on` and `<piece>.frames`.
pub fn drum_setting(song: &Song, instrument_id: &str, piece: DrumPiece) -> (bool, f64) {
    let overrides = speaker_overrides(song, instrument_id);
    let on = overrides.get(&format!("{}.on", piece.as_str()));
    let frames = overrides.get(&format!("{}.frames", piece.as_str()));
    (
        on != Some(&Value::Bool(false)),
        match frames {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
            _ => drum_frames(piece),
        },
    )
}

/// A repeatable pseudo-random number for a frame, so the same song always sounds the same.
fn noise(frame: f64, salt: f64) -> f64 {
    let mut x = to_int32(frame * 374_761_393.0 + salt * 668_265_263.0);
    x = (x ^ ((x as u32) >> 13) as i32).wrapping_mul(1_274_126_177);
    f64::from((x ^ ((x as u32) >> 16) as i32) as u32) / 4_294_967_296.0
}

/// The pitch of frame `i` of a drum hit: falling sweeps for the drums with a body, pitched noise for the rest.
pub fn drum_hz(piece: DrumPiece, i: f64, frames: f64, frame: f64) -> f64 {
    let t = if frames <= 1.0 {
        0.0
    } else {
        i / (frames - 1.0)
    };
    match piece {
        DrumPiece::Kick => 180.0 * (55.0f64 / 180.0).powf(t),
        DrumPiece::Tom => 260.0 * (110.0f64 / 260.0).powf(t),
        DrumPiece::Rim => 2000.0,
        DrumPiece::Snare => 900.0 + noise(frame, 1.0) * 2600.0,
        DrumPiece::Clap => {
            if i == 1.0 {
                0.0
            } else {
                1200.0 + noise(frame, 2.0) * 1400.0
            }
        }
        DrumPiece::Hat | DrumPiece::Openhat => 5000.0 + noise(frame, 3.0) * 4000.0,
        DrumPiece::Crash => 3000.0 + noise(frame, 4.0) * 6000.0,
    }
}

/// The frequency the PIT really makes for a requested one: whole divisors only.
pub fn pit_hz(hz: f64) -> f64 {
    if hz <= 0.0 {
        return 0.0;
    }
    let divisor = crate::numbers::min(65_535.0, crate::numbers::max(1.0, round(PIT_CLOCK / hz)));
    PIT_CLOCK / divisor
}

struct Span<'a> {
    start: f64,
    end: f64,
    priority: f64,
    instrument_id: &'a str,
    midi: f64,
}

struct DrumSpan {
    start: f64,
    frames: f64,
    piece: DrumPiece,
    priority: f64,
}

/// One frequency per frame for one pass (0 is silence).
pub fn sequence_speaker(song: &Song, performance: &Performance) -> Vec<f32> {
    let seconds = performance.length_steps / performance.steps_per_second;
    let frame_count = round(seconds * SPEAKER_FRAME_RATE).max(1.0) as usize;
    let mut hz = vec![0f32; frame_count];
    let mut voices: HashMap<&str, SpeakerVoice> = HashMap::new();
    let to_frame = |step: f64| round((step / performance.steps_per_second) * SPEAKER_FRAME_RATE);

    let mut notes: Vec<Span> = Vec::new();
    let mut drums: Vec<DrumSpan> = Vec::new();
    for event in &performance.events {
        let voice = voices
            .entry(event.instrument_id.as_str())
            .or_insert_with(|| speaker_voice(song, &event.instrument_id))
            .clone();
        let start = to_frame(event.step);
        if let Some(piece) = event.drum {
            let (on, frames) = drum_setting(song, &event.instrument_id, piece);
            if on {
                drums.push(DrumSpan {
                    start,
                    frames,
                    piece,
                    priority: voice.priority,
                });
            }
            continue;
        }
        let Some(midi) = event.midi else {
            continue;
        };
        let end = crate::numbers::max(
            start + 1.0,
            to_frame(event.step + event.len) - voice.gap_frames,
        );
        notes.push(Span {
            start,
            end,
            priority: voice.priority,
            instrument_id: event.instrument_id.as_str(),
            midi: f64::from(midi) + 12.0 * voice.octave,
        });
    }

    for (frame, slot) in hz.iter_mut().enumerate() {
        let frame = frame as f64;
        let sounding: Vec<&Span> = notes
            .iter()
            .filter(|n| n.start <= frame && frame < n.end)
            .collect();
        let mut best: Option<&Span> = None;
        for &note in &sounding {
            if best.is_none_or(|b| {
                note.priority > b.priority || (note.priority == b.priority && note.start > b.start)
            }) {
                best = Some(note);
            }
        }

        let drum = drums
            .iter()
            .find(|d| d.start <= frame && frame < d.start + d.frames);
        if let Some(drum) = drum {
            if best.is_none_or(|b| drum.priority >= b.priority) {
                *slot = pit_hz(drum_hz(drum.piece, frame - drum.start, drum.frames, frame)) as f32;
                continue;
            }
        }
        let Some(winner) = best else {
            continue;
        };
        let mut chord: Vec<f64> = sounding
            .iter()
            .filter(|n| n.instrument_id == winner.instrument_id && n.priority == winner.priority)
            .map(|n| n.midi)
            .collect();
        chord.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let voice = &voices[winner.instrument_id];
        let midi = match voice.chord_mode() {
            ChordMode::Top => chord[chord.len() - 1],
            ChordMode::Bottom => chord[0],
            ChordMode::Arpeggio => {
                let step = (frame / crate::numbers::max(1.0, voice.arpeggio_frames)).floor();
                chord[(step % chord.len() as f64) as usize]
            }
        };
        *slot = pit_hz(440.0 * 2f64.powf((midi - 69.0) / 12.0)) as f32;
    }
    hz
}
