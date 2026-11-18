// The snippets of the `undertone-mini` guide page, compiled and run so they stay true.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Run with `cargo run -p undertone-mini --example mini_guide`. The regions are what
//! `site/guide/undertone-mini.md` shows.

use undertone_mini::{Fraction, bjorklund, parse};

fn main() {
	// #region parse
	// The caller's rule decides which words are valid; here every word is accepted as text.
	let pattern = parse("c4 [e4 g4]*2 <a4 b4>", |word| {
		Ok::<_, String>(word.to_string())
	})
	.expect("a valid pattern");

	// One cycle of events, each with an exact start and end.
	let events = pattern.events(Fraction::ZERO, Fraction::ONE);
	for event in &events {
		println!("{} at {}..{}", event.value, event.begin, event.end);
	}
	assert_eq!(events.len(), 6);
	assert_eq!(events[1].begin, Fraction::new(1, 3));
	// #endregion parse

	// #region euclid
	// Euclidean rhythms are the same pattern language: `x(3,8)` is three onsets over eight steps.
	assert_eq!(
		bjorklund(3, 8).expect("a valid rhythm"),
		[true, false, false, true, false, false, true, false]
	);
	// #endregion euclid

	// #region errors
	// A refused pattern says what went wrong and where.
	let error = parse("c4 [e4", |word| Ok::<_, String>(word.to_string())).expect_err("unclosed");
	println!("{error:?}");
	// #endregion errors
}
