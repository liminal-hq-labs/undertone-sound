// Works out which notes a song loses on an engine with few voices, ready to show in the studio
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Port of `apps/studio/src/engines/limits.ts` for the engines that have a voice limit: the two
//! OPL chips. The PC speaker and Undertone report no limit.

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
    /// Lost notes over the whole arrangement (a section that repeats counts each time).
    pub total: usize,
    /// The lost notes themselves, placed in the whole song.
    pub dropped: Vec<PerformanceEvent>,
    /// Lost notes by track, most first.
    pub by_track: Vec<TrackLoss>,
    /// `trackId/sectionId` to the notes lost there, as `note_key(at, pitch)`.
    pub by_slot: BTreeMap<String, BTreeSet<String>>,
    /// The whole song, whose `sections` place each arrangement entry.
    pub performance: Performance,
    /// Pitched voices asked for at each step of the song.
    pub demand: Vec<u8>,
    /// Lost note onsets at each step of the song.
    pub lost: Vec<u16>,
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

/// What a chip loses of a performance already compiled (the song, a section or an arrangement entry).
pub fn analyse_performance(
    song: &Song,
    performance: Performance,
    chip: OplChip,
) -> Option<LostNotes> {
    let capacity = melodic_channels(chip).len();
    let dropped = sequence_opl(song, &performance, chip).dropped_events;
    if dropped.is_empty() {
        return None;
    }

    let length = performance.length_steps.max(0.0) as usize;
    let mut demand = vec![0u8; length];
    for event in &performance.events {
        if event.midi.is_none() {
            continue;
        }
        let end = (length as f64).min((event.step + event.len).ceil()) as usize;
        for step in (event.step.floor().max(0.0) as usize)..end {
            demand[step] = demand[step].saturating_add(1);
        }
    }

    let mut lost = vec![0u16; length];
    let mut by_slot: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut counts: Vec<(String, usize)> = Vec::new();
    for event in &dropped {
        if length > 0 {
            let at = (event.step.floor().max(0.0) as usize).min(length - 1);
            lost[at] = lost[at].wrapping_add(1);
        }
        match counts.iter_mut().find(|(id, _)| *id == event.track_id) {
            Some((_, count)) => *count += 1,
            None => counts.push((event.track_id.clone(), 1)),
        }
        let Some(placed) = section_at_step(&performance, event.step) else {
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
    Some(LostNotes {
        engine_name: chip_name(chip).to_string(),
        capacity,
        total: dropped.len(),
        dropped,
        by_track,
        by_slot,
        performance,
        demand,
        lost,
    })
}
