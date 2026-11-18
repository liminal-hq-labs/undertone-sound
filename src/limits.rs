// Works out which notes a song loses on an engine with few voices, ready to show in the studio
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Port of `apps/studio/src/engines/limits.ts` for the engines that have a voice limit: the two
//! OPL chips. The PC speaker and Undertone report no limit here.
//!
//! Two kinds of loss are told apart. A note is **lost** when it never starts, because every
//! channel held a note of equal or higher priority. A note is **cut** when it starts and is ended
//! early, because a more important note took its channel.

use std::collections::{BTreeMap, BTreeSet};

use fizz_audio_song::compile::{
    compile_song, section_at_step, CompileOptions, Performance, PerformanceEvent,
};
use fizz_audio_song::model::{EngineId, Song};

use crate::numbers::fixed_number_text;
use crate::sequence::{melodic_channels, sequence_opl, OplChip};

/// Lost notes by track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackLoss {
    pub track_id: String,
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LostNotes {
    pub engine_name: String,
    /// Melodic voices the engine has.
    pub capacity: usize,
    /// Lost notes (never started) over the whole arrangement (a section that repeats counts each time).
    pub total: usize,
    /// The lost notes themselves, placed in the whole song.
    pub dropped: Vec<PerformanceEvent>,
    /// Lost notes by track, most first.
    pub by_track: Vec<TrackLoss>,
    /// `trackId/sectionId` to the notes lost there, as `note_key(at, pitch)`.
    pub by_slot: BTreeMap<String, BTreeSet<String>>,
    /// Notes that started and were ended early by a more important note taking their channel.
    pub cut: Vec<PerformanceEvent>,
    /// Cut notes by track, most first.
    pub cut_by_track: Vec<TrackLoss>,
    /// `trackId/sectionId` to the notes cut there, as `note_key(at, pitch)`.
    pub cut_by_slot: BTreeMap<String, BTreeSet<String>>,
    /// The whole song, whose `sections` place each arrangement entry.
    pub performance: Performance,
    /// Pitched voices asked for at each step of the song.
    pub demand: Vec<u8>,
    /// Lost note onsets at each step of the song.
    pub lost: Vec<u16>,
    /// Cut note onsets at each step of the song.
    pub cut_at: Vec<u16>,
}

/// A note's identity within its section, for matching lost notes to the notes in a take.
pub fn note_key(at: f64, pitch: &str) -> String {
    format!("{}:{pitch}", fixed_number_text(at, 3))
}

pub fn slot_key(track_id: &str, section_id: &str) -> String {
    format!("{track_id}/{section_id}")
}

/// The voice count and display name of an OPL chip.
pub fn chip_name(chip: OplChip) -> &'static str {
    match chip {
        OplChip::Opl2 => "OPL2",
        OplChip::Opl3 => "OPL3",
    }
}

/// What the song's own engine loses, or `None` when it loses nothing or has no voice limit. Fails
/// with the notation's message when a take does not parse.
pub fn analyse_limits(song: &Song) -> Result<Option<LostNotes>, String> {
    let chip = match song.engine {
        EngineId::Opl2 => OplChip::Opl2,
        EngineId::Opl3 => OplChip::Opl3,
        EngineId::Undertone | EngineId::PcSpeaker => return Ok(None),
    };
    analyse_limits_on(song, chip)
}

/// What a chip loses of the whole song, whatever engine the song names.
pub fn analyse_limits_on(song: &Song, chip: OplChip) -> Result<Option<LostNotes>, String> {
    let performance = compile_song(song, &CompileOptions::default())?;
    Ok(analyse_performance(song, performance, chip))
}

/// Where each event falls: its step's count, its track's count, and its slot's notes.
struct Tally {
    at: Vec<u16>,
    by_track: Vec<TrackLoss>,
    by_slot: BTreeMap<String, BTreeSet<String>>,
}

fn tally(
    song: &Song,
    performance: &Performance,
    length: usize,
    events: &[PerformanceEvent],
) -> Tally {
    let mut at = vec![0u16; length];
    let mut by_slot: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut counts: Vec<(String, usize)> = Vec::new();
    for event in events {
        if length > 0 {
            let step = (event.step.floor().max(0.0) as usize).min(length - 1);
            at[step] = at[step].wrapping_add(1);
        }
        match counts.iter_mut().find(|(id, _)| *id == event.track_id) {
            Some((_, count)) => *count += 1,
            None => counts.push((event.track_id.clone(), 1)),
        }
        let Some(placed) = section_at_step(performance, event.step) else {
            continue;
        };
        by_slot
            .entry(slot_key(&event.track_id, &placed.section_id))
            .or_default()
            .insert(note_key(event.step - placed.start_step, &event.pitch));
    }
    let mut by_track: Vec<TrackLoss> = counts
        .into_iter()
        .map(|(track_id, count)| TrackLoss {
            name: song
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .map_or_else(|| track_id.clone(), |track| track.name.clone()),
            track_id,
            count,
        })
        .collect();
    by_track.sort_by_key(|loss| std::cmp::Reverse(loss.count));
    Tally {
        at,
        by_track,
        by_slot,
    }
}

/// What a chip loses of a performance already compiled (the song, a section or an arrangement
/// entry): the notes that never start and the notes that are cut, or `None` when there are neither.
pub fn analyse_performance(
    song: &Song,
    performance: Performance,
    chip: OplChip,
) -> Option<LostNotes> {
    let capacity = melodic_channels(chip).len();
    let sequence = sequence_opl(song, &performance, chip);
    let (dropped, cut) = (sequence.dropped_events, sequence.cut_events);
    if dropped.is_empty() && cut.is_empty() {
        return None;
    }

    let length = performance.length_steps.max(0.0) as usize;
    let mut demand = vec![0u8; length];
    for event in &performance.events {
        if event.midi.is_none() {
            continue;
        }
        let end = (length as f64).min((event.step + event.len).ceil()) as usize;
        let first = event.step.floor().max(0.0) as usize;
        for slot in demand.iter_mut().take(end).skip(first) {
            *slot = slot.saturating_add(1);
        }
    }

    let lost = tally(song, &performance, length, &dropped);
    let cuts = tally(song, &performance, length, &cut);
    Some(LostNotes {
        engine_name: chip_name(chip).to_string(),
        capacity,
        total: dropped.len(),
        dropped,
        by_track: lost.by_track,
        by_slot: lost.by_slot,
        cut,
        cut_by_track: cuts.by_track,
        cut_by_slot: cuts.by_slot,
        performance,
        demand,
        lost: lost.at,
        cut_at: cuts.at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// A pad of five held notes (every channel of an OPL2) and a lead note that arrives mid-way.
    /// `pad` and `lead` are the tracks' own priorities, when they set one.
    fn contest(pad: Option<u8>, lead: Option<u8>) -> Song {
        let held: Vec<Value> = ["c3", "e3", "g3", "b3", "d4"]
            .iter()
            .map(|pitch| json!({ "at": 0, "pitch": pitch, "len": 8 }))
            .collect();
        let track = |id: &str, role: &str, priority: Option<u8>| {
            let mut track = json!({
                "id": id, "name": id, "role": role, "instrument": id, "volume": 0.8, "pan": 0
            });
            if let Some(priority) = priority {
                track["priority"] = json!(priority);
            }
            track
        };
        let take = |notes: Vec<Value>| {
            json!({ "active": "a", "takes": [{
            "id": "a", "name": "A", "by": "you", "notes": notes
        }] })
        };
        serde_json::from_value(json!({
            "format": "fizz-audio-studio/song", "version": 1, "title": "Contest", "tempo": 120,
            "beatsPerBar": 4, "stepsPerBeat": 4, "key": { "root": "c", "scale": "major" },
            "engine": "opl2",
            "instruments": {
                "pad": { "name": "Pad", "preset": "pad" },
                "lead": { "name": "Lead", "preset": "lead" }
            },
            "tracks": [track("pad", "harmony", pad), track("lead", "melody", lead)],
            "sections": [{ "id": "a", "name": "A", "bars": 1 }],
            "arrangement": ["a"],
            "clips": {
                "pad": { "a": take(held) },
                "lead": { "a": take(vec![json!({ "at": 4, "pitch": "a4", "len": 2 })]) }
            },
            "messages": []
        }))
        .expect("a song")
    }

    #[test]
    fn a_higher_part_cuts_a_held_note_of_a_lower_one_and_nothing_is_lost() {
        let found = analyse_limits(&contest(None, None)).unwrap().unwrap();
        assert_eq!(found.total, 0);
        assert_eq!(found.cut.len(), 1);
        assert_eq!(found.cut[0].track_id, "pad");
        assert_eq!(found.cut_by_track[0].count, 1);
        assert_eq!(found.cut_by_slot["pad/a"].len(), 1);
        assert_eq!(found.cut_at[0], 1);
        assert_eq!(found.lost[4], 0);
    }

    #[test]
    fn an_equal_or_higher_priority_is_never_cut_and_the_new_note_is_lost() {
        for (pad, lead) in [(Some(9), None), (None, Some(5)), (None, Some(1))] {
            let found = analyse_limits(&contest(pad, lead)).unwrap().unwrap();
            assert!(found.cut.is_empty(), "{pad:?} {lead:?}");
            assert_eq!(found.total, 1, "{pad:?} {lead:?}");
            assert_eq!(found.by_track[0].track_id, "lead");
        }
    }

    #[test]
    fn nothing_is_reported_when_the_channels_are_enough() {
        let song = contest(None, None);
        assert!(analyse_limits_on(&song, OplChip::Opl3).unwrap().is_none());
    }
}
