// Mini-notation for Fizz Audio Studio: a faithful port of Undertone's `mini`, `Pattern` and `Fraction`
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Rhythm strings such as `c3 [e3 g3]*2 <a3 b3> x(3,8)` become a [`Pattern`] that is queried over
//! spans of cycles. Time is an exact [`Fraction`]. The port matches Undertone, the library the
//! studio's TypeScript uses, for every pattern a song can hold, error messages and positions
//! included; `fixtures/mini/` holds the cases it is tested against.
//!
//! ```
//! use fizz_audio_mini::{parse, Fraction};
//!
//! let pattern = parse("c4 [e4 g4]", |word| Ok::<_, String>(word.to_string())).unwrap();
//! let events = pattern.events(Fraction::ZERO, Fraction::ONE);
//! assert_eq!(events.len(), 3);
//! assert_eq!(events[1].begin, Fraction::new(1, 2));
//! ```
//!
//! The leaf rule is the caller's: it decides which words are valid (a pitch, a drum piece) and what
//! they become. The studio's rule, `checkPitchWord`, belongs to the song crate.

mod fraction;
mod parse;
mod pattern;

pub use fraction::Fraction;
pub use parse::{parse, MiniError};
pub use pattern::{bjorklund, Event, Hap, Pattern, PatternError, Span};
