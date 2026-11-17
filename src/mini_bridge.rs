// The one place the song crate meets mini-notation
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use fizz_audio_mini::{parse, Fraction};

use crate::validate::check_pitch_word;

/// One onset of a pattern over a section: where it starts and how long it lasts, in bars (one
/// cycle is one bar), as the doubles the TypeScript computes with `Fraction.toNumber()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Onset {
    pub begin: f64,
    pub end: f64,
    pub value: String,
}

fn leaf(drums: bool) -> impl Fn(&str) -> Result<String, String> {
    move |word| check_pitch_word(word, drums).map(|()| word.to_string())
}

/// Checks a mini-notation take's pattern, with the pitch (or drum) rule on each word. The error
/// is the full message the TypeScript reports.
pub fn check(source: &str, drums: bool) -> Result<(), String> {
    parse(source, leaf(drums))
        .map(|_| ())
        .map_err(|error| error.message)
}

/// Expands a pattern over `bars` cycles into its onsets, sorted by begin.
pub fn onsets(source: &str, drums: bool, bars: u32) -> Result<Vec<Onset>, String> {
    let pattern = parse(source, leaf(drums)).map_err(|error| error.message)?;
    let events = pattern.events(Fraction::from_int(0), Fraction::from_int(i128::from(bars)));
    Ok(events
        .into_iter()
        .map(|event| Onset {
            begin: event.begin.to_f64(),
            end: event.end.to_f64(),
            value: event.value,
        })
        .collect())
}
