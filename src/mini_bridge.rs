// The one place the song crate meets mini-notation
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Validation and `take_notes` need mini-notation, which `fizz-audio-mini` (phase D1) owns. Every
//! use goes through the two functions here, so switching it on is a change to this file and the
//! `mini` feature in `Cargo.toml`, and nothing else.
//!
//! With the feature off, `check` accepts every pattern and `onsets` fails with a message saying
//! the notation is not available. TODO(D1): turn the feature on once `fizz-audio-mini` merges
//! (see `Cargo.toml`), then drop the gate.

/// One onset of a pattern over a section: where it starts and how long it lasts, in bars (one
/// cycle is one bar), as the doubles the TypeScript computes with `Fraction.toNumber()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Onset {
    pub begin: f64,
    pub end: f64,
    pub value: String,
}

#[cfg(not(feature = "mini"))]
mod imp {
    use super::Onset;

    pub fn check(_source: &str, _drums: bool) -> Result<(), String> {
        Ok(())
    }

    pub fn onsets(_source: &str, _drums: bool, _bars: u32) -> Result<Vec<Onset>, String> {
        Err("mini-notation is not available in this build".to_string())
    }
}

#[cfg(feature = "mini")]
mod imp {
    use super::Onset;
    use crate::validate::check_pitch_word;
    use fizz_audio_mini::{parse, Fraction};

    fn leaf(drums: bool) -> impl Fn(&str) -> Result<String, String> {
        move |word| check_pitch_word(word, drums).map(|()| word.to_string())
    }

    pub fn check(source: &str, drums: bool) -> Result<(), String> {
        parse(source, leaf(drums))
            .map(|_| ())
            .map_err(|error| error.message)
    }

    pub fn onsets(source: &str, drums: bool, bars: u32) -> Result<Vec<Onset>, String> {
        let pattern = parse(source, leaf(drums)).map_err(|error| error.message)?;
        let events = pattern.events(Fraction::new(0, 1), Fraction::new(i64::from(bars), 1));
        // TODO(D1): the field and accessor names below follow the agreed shape (an event is an
        // onset with a begin, an end and a value; a fraction has a numerator and a denominator).
        // Adjust them to the merged API.
        Ok(events
            .into_iter()
            .map(|event| Onset {
                begin: event.begin.numer() as f64 / event.begin.denom() as f64,
                end: event.end.numer() as f64 / event.end.denom() as f64,
                value: event.value,
            })
            .collect())
    }
}

/// Checks a mini-notation take's pattern, with the pitch (or drum) rule on each word. The error
/// is the full message the TypeScript reports.
pub fn check(source: &str, drums: bool) -> Result<(), String> {
    imp::check(source, drums)
}

/// Expands a pattern over `bars` cycles into its onsets, in the order the pattern reports them.
pub fn onsets(source: &str, drums: bool, bars: u32) -> Result<Vec<Onset>, String> {
    imp::onsets(source, drums, bars)
}
