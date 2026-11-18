// Undertone's mini-notation in Rust: a faithful port of `mini`, `Pattern` and `Fraction`
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rhythm strings such as `c3 [e3 g3]*2 <a3 b3> x(3,8)` become a [`Pattern`] that is queried over
//! spans of cycles. Time is an exact [`Fraction`]. The port matches Undertone's TypeScript library
//! for every pattern a song can hold, error messages and positions included; `fixtures/mini/`
//! holds the cases it is tested against.
//!
//! ```
//! use undertone_mini::{parse, Fraction};
//!
//! let pattern = parse("c4 [e4 g4]", |word| Ok::<_, String>(word.to_string())).unwrap();
//! let events = pattern.events(Fraction::ZERO, Fraction::ONE);
//! assert_eq!(events.len(), 3);
//! assert_eq!(events[1].begin, Fraction::new(1, 2));
//! ```
//!
//! The leaf rule is the caller's: it decides which words are valid (a pitch, a drum piece) and what
//! they become. A host that validates pitches or drum names supplies its own rule.
//!
//! [`js`] holds JavaScript's number and string behaviours (`String(n)`, `toFixed(2)`, `Math.round`,
//! `trim`, UTF-16 lengths and `JSON.stringify` quoting), so Rust output matches the TypeScript
//! fixtures byte for byte.

mod fraction;
pub mod js;
mod parse;
mod pattern;

pub use fraction::Fraction;
pub use parse::{MiniError, parse};
pub use pattern::{Event, Hap, Pattern, PatternError, Span, bjorklund};
