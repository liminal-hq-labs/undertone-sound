// Pitch names, MIDI numbers and scales, as the song format spells them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use crate::js;
use crate::model::{Key, ScaleName};

/// Semitone steps of each scale from its tonic, matching the names Undertone's `.scale()` knows.
pub fn scale_intervals(scale: ScaleName) -> &'static [i32] {
    match scale {
        ScaleName::Major => &[0, 2, 4, 5, 7, 9, 11],
        ScaleName::Minor => &[0, 2, 3, 5, 7, 8, 10],
        ScaleName::Dorian => &[0, 2, 3, 5, 7, 9, 10],
        ScaleName::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
        ScaleName::Lydian => &[0, 2, 4, 6, 7, 9, 11],
        ScaleName::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
        ScaleName::Locrian => &[0, 1, 3, 5, 6, 8, 10],
        ScaleName::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
        ScaleName::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
        ScaleName::MajorPentatonic => &[0, 2, 4, 7, 9],
        ScaleName::MinorPentatonic => &[0, 3, 5, 7, 10],
        ScaleName::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    }
}

const SHARP_NAMES: [&str; 12] = [
    "c", "c#", "d", "d#", "e", "f", "f#", "g", "g#", "a", "a#", "b",
];
const FLAT_NAMES: [&str; 12] = [
    "c", "db", "d", "eb", "e", "f", "gb", "g", "ab", "a", "bb", "b",
];

fn natural_semitone(letter: u8) -> i32 {
    match letter.to_ascii_lowercase() {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        _ => 11,
    }
}

/// `#` raises a note and a lower-case `b` lowers it. The TypeScript's patterns are case
/// insensitive, so an upper-case `B` matches as a mark but moves nothing: `bB2` is `b2`.
fn accidental_offset(mark: Option<u8>) -> i32 {
    match mark {
        Some(b'#') => 1,
        Some(b'b') => -1,
        _ => 0,
    }
}

/// Splits `^([a-g])(#|b)?` (case-insensitive) off the front of the bytes.
fn letter_and_mark(bytes: &[u8]) -> Option<(u8, Option<u8>, &[u8])> {
    let (&letter, rest) = bytes.split_first()?;
    if !(b'a'..=b'g').contains(&letter.to_ascii_lowercase()) {
        return None;
    }
    match rest.split_first() {
        Some((&mark, tail)) if mark == b'#' || mark.eq_ignore_ascii_case(&b'b') => {
            Some((letter, Some(mark), tail))
        }
        _ => Some((letter, None, rest)),
    }
}

/// The semitone (0 to 11) of a pitch class such as `"f#"`, or `None` when it is not one.
pub fn pitch_class(name: &str) -> Option<i32> {
    let (letter, mark, rest) = letter_and_mark(js::trim(name).as_bytes())?;
    if !rest.is_empty() {
        return None;
    }
    Some((natural_semitone(letter) + accidental_offset(mark) + 12) % 12)
}

/// The MIDI number of a note name such as `"d4"` (middle C is `c4`, 60), or `None`.
pub fn parse_pitch(name: &str) -> Option<i32> {
    let (letter, mark, rest) = letter_and_mark(js::trim(name).as_bytes())?;
    let digits = match rest {
        [b'-', digit] => {
            if !digit.is_ascii_digit() {
                return None;
            }
            -i32::from(digit - b'0')
        }
        [digit] => {
            if !digit.is_ascii_digit() {
                return None;
            }
            i32::from(digit - b'0')
        }
        _ => return None,
    };
    Some((digits + 1) * 12 + natural_semitone(letter) + accidental_offset(mark))
}

/// Spells a MIDI number as a note name, with flats when the key prefers them.
pub fn midi_to_name(midi: i32, key: Option<&Key>) -> String {
    let names = if key.is_some_and(prefers_flats) {
        &FLAT_NAMES
    } else {
        &SHARP_NAMES
    };
    let octave = midi.div_euclid(12) - 1;
    format!("{}{}", names[midi.rem_euclid(12) as usize], octave)
}

/// Keys written with flats: an `f` or flat tonic, and the minor keys of those majors.
pub fn prefers_flats(key: &Key) -> bool {
    let root = js::trim(&key.root).to_lowercase();
    if root.ends_with('b') && root.encode_utf16().count() == 2 {
        return true;
    }
    let minorish = matches!(
        key.scale,
        ScaleName::Minor | ScaleName::Dorian | ScaleName::Phrygian
    );
    if root == "f" {
        return true;
    }
    minorish && ["d", "g", "c"].contains(&root.as_str())
}

/// The pitch classes (0 to 11) that belong to a key, as a set in the order the scale lists them.
pub fn key_pitch_classes(key: &Key) -> Vec<i32> {
    let root = pitch_class(&key.root).unwrap_or(0);
    scale_intervals(key.scale)
        .iter()
        .map(|step| (root + step) % 12)
        .collect()
}

pub fn is_in_key(midi: i32, key: &Key) -> bool {
    key_pitch_classes(key).contains(&midi.rem_euclid(12))
}

/// Moves a pitch by a number of scale degrees within a key, snapping an outside note first.
pub fn step_in_key(midi: i32, degrees: i32, key: &Key) -> i32 {
    let classes = key_pitch_classes(key);
    let mut pitch = midi;
    while !classes.contains(&pitch.rem_euclid(12)) {
        pitch -= 1;
    }
    let direction = degrees.signum();
    let mut moved = 0;
    while moved < degrees.abs() {
        pitch += direction;
        if classes.contains(&pitch.rem_euclid(12)) {
            moved += 1;
        }
    }
    pitch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitches_parse_like_the_typescript() {
        assert_eq!(parse_pitch("c4"), Some(60));
        assert_eq!(parse_pitch(" C4 "), Some(60));
        assert_eq!(parse_pitch("bB2"), Some(47));
        assert_eq!(parse_pitch("Bb2"), Some(46));
        assert_eq!(parse_pitch("ab-1"), Some(8));
        assert_eq!(parse_pitch("c##4"), None);
        assert_eq!(parse_pitch("d44"), None);
        assert_eq!(parse_pitch("EB"), None);
        assert_eq!(pitch_class("EB"), Some(4));
        assert_eq!(pitch_class("Eb"), Some(3));
    }
}
