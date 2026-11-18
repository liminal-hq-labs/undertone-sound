// Note names and raw frequencies to Hz, as Undertone's `pitch.ts` reads them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use fizz_audio_song::js::{number_to_string, trim};
use serde_json::Value;

use crate::error::Error;
use crate::math;

/// A voice's pitch: a note name (`"c2"`, `"f#3"`, `"bb3"`) or a frequency in Hz.
#[derive(Debug, Clone, PartialEq)]
pub enum Pitch {
    Name(String),
    Hz(f64),
}

impl Pitch {
    /// The pitch a JSON value holds: a string is a name and a number is Hz; anything else is none.
    pub fn from_value(value: &Value) -> Option<Pitch> {
        match value {
            Value::String(name) => Some(Pitch::Name(name.clone())),
            Value::Number(number) => number.as_f64().map(Pitch::Hz),
            _ => None,
        }
    }

    /// The pitch as a JSON value, the way a patch carries it.
    pub fn to_value(&self) -> Value {
        match self {
            Pitch::Name(name) => Value::String(name.clone()),
            Pitch::Hz(hz) => json_number(*hz),
        }
    }

    /// `String(pitch)`, as the fixture's score lines print it.
    pub fn text(&self) -> String {
        match self {
            Pitch::Name(name) => name.clone(),
            Pitch::Hz(hz) => number_to_string(*hz),
        }
    }
}

/// A number as a JSON value; a value JSON cannot hold becomes `null`.
pub fn json_number(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// `noteToMidi`: scientific pitch notation, C4 = 60, the name trimmed and its letter in either case.
pub fn note_to_midi(name: &str) -> Result<f64, Error> {
    let invalid = || {
        Error::new(format!(
            "Invalid note name: \"{name}\". Expected e.g. \"c2\", \"a4\", \"f#3\", \"bb3\"."
        ))
    };
    let text = trim(name);
    let mut chars = text.chars();
    let letter = chars.next().ok_or_else(invalid)?;
    let base = match letter.to_ascii_lowercase() {
        'c' => 0.0,
        'd' => 2.0,
        'e' => 4.0,
        'f' => 5.0,
        'g' => 7.0,
        'a' => 9.0,
        'b' => 11.0,
        _ => return Err(invalid()),
    };
    let rest = chars.as_str();
    let (accidental, octave) = match rest.chars().next() {
        Some('#') => (1.0, &rest[1..]),
        Some('b') => (-1.0, &rest[1..]),
        _ => (0.0, rest),
    };
    let digits = octave.strip_prefix('-').unwrap_or(octave);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    // `Number.parseInt` of the digits: a double, however long.
    let magnitude: f64 = digits.parse().map_err(|_| invalid())?;
    let octave = if octave.starts_with('-') {
        -magnitude
    } else {
        magnitude
    };
    Ok((octave + 1.0) * 12.0 + base + accidental)
}

/// `midiToFrequency`: A4 = MIDI 69 = 440 Hz.
pub fn midi_to_frequency(midi: f64) -> f64 {
    440.0 * math::pow(2.0, (midi - 69.0) / 12.0)
}

/// `noteToFrequency`: a name through its MIDI number, a number as Hz (positive and finite).
pub fn note_to_frequency(pitch: &Pitch) -> Result<f64, Error> {
    match pitch {
        Pitch::Hz(hz) => {
            if !hz.is_finite() || *hz <= 0.0 {
                Err(Error::new(format!(
                    "Invalid frequency: {}",
                    number_to_string(*hz)
                )))
            } else {
                Ok(*hz)
            }
        }
        Pitch::Name(name) => Ok(midi_to_frequency(note_to_midi(name)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_names_as_undertone_does() {
        assert_eq!(note_to_midi("c4").unwrap(), 60.0);
        assert_eq!(note_to_midi(" A4 ").unwrap(), 69.0);
        assert_eq!(note_to_midi("f#3").unwrap(), 54.0);
        assert_eq!(note_to_midi("bb3").unwrap(), 58.0);
        assert_eq!(note_to_midi("c-1").unwrap(), 0.0);
        assert!(note_to_midi("h2").is_err());
        assert!(note_to_midi("c").is_err());
        assert_eq!(
            note_to_midi("x").unwrap_err().message,
            "Invalid note name: \"x\". Expected e.g. \"c2\", \"a4\", \"f#3\", \"bb3\"."
        );
    }

    #[test]
    fn frequencies_follow_a440() {
        assert_eq!(midi_to_frequency(69.0), 440.0);
        assert_eq!(midi_to_frequency(33.0), 55.0);
        assert_eq!(note_to_frequency(&Pitch::Hz(123.5)).unwrap(), 123.5);
        assert_eq!(
            note_to_frequency(&Pitch::Hz(0.0)).unwrap_err().message,
            "Invalid frequency: 0"
        );
    }
}
