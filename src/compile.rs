// The performance compiler: a song as engine-neutral timed events
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Port of `apps/studio/src/song/compile.ts`. Every step and length is computed with the same
//! floating-point operations in the same order as the TypeScript, so the golden performances in
//! `fixtures/song/compile.json` match bit for bit.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{DrumPiece, PanMotion, Song, Track};
use crate::music::parse_pitch;
use crate::pan::{cut_path, note_pan, onset_ranks, PanPoint, PanScope};
use crate::takes::take_notes;

/// The velocity of a note that does not say.
pub const DEFAULT_VELOCITY: f64 = 0.8;

/// One note, placed in song time. Steps may be fractional where mini-notation splits a step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../packages/protocol/src/generated/",
    rename_all = "camelCase"
)]
pub struct PerformanceEvent {
    pub step: f64,
    pub len: f64,
    pub track_id: String,
    pub instrument_id: String,
    /// The pitch as written: a note name, or the drum piece.
    pub pitch: String,
    /// MIDI number for a pitched note; `null` on a drum track.
    pub midi: Option<i32>,
    pub drum: Option<DrumPiece>,
    /// Note strength times the track's volume.
    pub velocity: f64,
    /// Stereo position at the note's onset, -1 to 1: the first of the note's motion, its pan, the
    /// take's motion, the take's pan, and the track's pan.
    pub pan: f64,
    /// Where the pan goes from there: points in steps from the onset, interpolated linearly, the
    /// last value held, and two points at the same step making a jump. Left out when the pan stays
    /// put.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pan_path: Option<Vec<PanPoint>>,
}

/// Where one arrangement entry sits in the performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../packages/protocol/src/generated/",
    rename_all = "camelCase"
)]
pub struct PlacedSection {
    /// Index into `Song.arrangement`, or -1 when one section was compiled on its own.
    pub index: i32,
    pub section_id: String,
    pub start_step: f64,
    pub steps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../packages/protocol/src/generated/",
    rename_all = "camelCase"
)]
pub struct Performance {
    pub steps_per_second: f64,
    pub length_steps: f64,
    pub events: Vec<PerformanceEvent>,
    pub sections: Vec<PlacedSection>,
}

/// What to compile, where the default is the whole arrangement.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompileOptions {
    /// Compile just this arrangement entry, to loop one section while working on it.
    pub only: Option<i32>,
    /// Compile just this section, whether or not the arrangement uses it (its `index` is -1).
    pub section: Option<String>,
    /// Plays a different take in some slots without changing the song: `"melody/verse"` to `"b"`.
    pub takes: HashMap<String, String>,
}

/// Which tracks are heard: every unmuted track, or only the soloed ones when any is soloed.
pub fn audible_tracks(song: &Song) -> Vec<&Track> {
    let soloed = song.tracks.iter().any(|track| track.solo == Some(true));
    song.tracks
        .iter()
        .filter(|track| track.mute != Some(true) && (!soloed || track.solo == Some(true)))
        .collect()
}

/// Compiles a song into a performance. Fails with the notation's message when a mini take does
/// not parse (the TypeScript throws it).
pub fn compile_song(song: &Song, options: &CompileOptions) -> Result<Performance, String> {
    let steps_per_second = (song.tempo / 60.0) * f64::from(song.steps_per_beat);
    let tracks = audible_tracks(song);
    let mut events: Vec<PerformanceEvent> = Vec::new();
    let mut sections: Vec<PlacedSection> = Vec::new();

    let mut cursor: f64 = 0.0;
    let order: Vec<&String> = match &options.section {
        Some(section) => vec![section],
        None => song.arrangement.iter().collect(),
    };
    for (position, section_id) in order.into_iter().enumerate() {
        let index: i32 = if options.section.is_some() {
            -1
        } else {
            position as i32
        };
        if options.only.is_some_and(|only| only != index) {
            continue;
        }
        let Some(section) = song.find_section(section_id) else {
            continue;
        };
        let steps = f64::from(song.section_steps(section));
        sections.push(PlacedSection {
            index,
            section_id: section_id.clone(),
            start_step: cursor,
            steps,
        });

        for track in &tracks {
            let Some(slot) = song
                .clips
                .get(&track.id)
                .and_then(|by_section| by_section.get(section_id))
            else {
                continue;
            };
            let override_id = options
                .takes
                .get(&format!("{}/{}", track.id, section_id))
                .map(String::as_str);
            let take_id = override_id.or(slot.active.as_deref());
            let Some(take) = take_id.and_then(|id| slot.takes.iter().find(|t| t.id == id)) else {
                continue;
            };
            let drums = song.is_drum_track(track);
            let notes = take_notes(song, track, section, take)?;
            let is_jump = matches!(take.pan_motion, Some(PanMotion::Jump { .. }));
            let scope = PanScope {
                track_pan: track.pan,
                take,
                section_steps: steps,
                steps_per_bar: f64::from(song.steps_per_bar()),
                ranks: onset_ranks(if is_jump { &notes[..] } else { &[] }),
            };
            for note in &notes {
                if note.at >= steps {
                    continue;
                }
                let len = note.len.min(steps - note.at);
                let placed = note_pan(note, len, &scope);
                events.push(PerformanceEvent {
                    step: cursor + note.at,
                    len,
                    track_id: track.id.clone(),
                    instrument_id: track.instrument.clone(),
                    pitch: note.pitch.clone(),
                    midi: if drums {
                        None
                    } else {
                        parse_pitch(&note.pitch)
                    },
                    drum: if drums {
                        DrumPiece::from_word(&note.pitch)
                    } else {
                        None
                    },
                    velocity: note.vel.unwrap_or(DEFAULT_VELOCITY) * track.volume,
                    pan: placed.pan,
                    pan_path: placed.pan_path,
                });
            }
        }
        cursor += steps;
    }

    events.sort_by(|a, b| a.step.total_cmp(&b.step));
    Ok(Performance {
        steps_per_second,
        length_steps: cursor,
        events,
        sections,
    })
}

/// Turns a looping performance so it starts at `from_step`: what comes after that point plays
/// first, then the start of the loop. A note that spans the point is cut there.
pub fn rotate_performance(performance: &Performance, from_step: f64) -> Performance {
    let length = performance.length_steps;
    if length == 0.0 || from_step <= 0.0 || from_step >= length {
        return performance.clone();
    }
    let mut events: Vec<PerformanceEvent> = performance
        .events
        .iter()
        .map(|event| {
            if event.step >= from_step {
                PerformanceEvent {
                    step: event.step - from_step,
                    ..event.clone()
                }
            } else {
                let len = event.len.min(from_step - event.step);
                let pan_path = match &event.pan_path {
                    Some(path) if len < event.len => cut_path(path, len),
                    other => other.clone(),
                };
                PerformanceEvent {
                    step: event.step + length - from_step,
                    len,
                    pan_path,
                    ..event.clone()
                }
            }
        })
        .collect();
    events.sort_by(|a, b| a.step.total_cmp(&b.step));
    Performance {
        events,
        ..performance.clone()
    }
}

/// The arrangement entry playing at a step of the performance.
pub fn section_at_step(performance: &Performance, step: f64) -> Option<&PlacedSection> {
    performance
        .sections
        .iter()
        .find(|placed| step >= placed.start_step && step < placed.start_step + placed.steps)
}
