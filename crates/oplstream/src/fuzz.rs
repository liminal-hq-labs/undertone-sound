// Fuzz-style tests: random, mutated and truncated bytes must never panic any reader.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::examples;
use crate::{Chip, Decoder, Stepper};
use std::vec::Vec;

/// A tiny deterministic xorshift generator.
pub struct Rng(u64);

impl Rng {
	pub fn new(seed: u64) -> Self {
		Self(seed | 1)
	}

	pub fn next(&mut self) -> u64 {
		let mut x = self.0;
		x ^= x << 13;
		x ^= x >> 7;
		x ^= x << 17;
		self.0 = x;
		x
	}

	pub fn below(&mut self, n: usize) -> usize {
		(self.next() % n as u64) as usize
	}

	pub fn bytes(&mut self, len: usize) -> Vec<u8> {
		(0..len).map(|_| self.next() as u8).collect()
	}
}

/// Runs `check` on random buffers, on truncations and single-byte mutations of every example,
/// and on random buffers that start with a plausible header.
pub fn for_each_input(mut check: impl FnMut(&[u8])) {
	let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
	for _ in 0..3000 {
		let len = rng.below(96);
		check(&rng.bytes(len));
	}
	for (_, file) in examples::ALL {
		for cut in 0..=file.len() {
			check(&file[..cut]);
		}
		for _ in 0..1500 {
			let mut copy = file.to_vec();
			for _ in 0..1 + rng.below(3) {
				let at = rng.below(copy.len());
				copy[at] = rng.next() as u8;
			}
			check(&copy);
			if copy.len() > 4 {
				copy.truncate(rng.below(copy.len()));
				check(&copy);
			}
		}
		for _ in 0..500 {
			let mut copy = file[..32].to_vec();
			let len = rng.below(48);
			copy.extend(rng.bytes(len));
			check(&copy);
		}
	}
}

#[test]
fn the_decoder_and_stepper_never_panic() {
	for_each_input(|bytes| {
		for chip in [Chip::Opl2, Chip::Opl3, Chip::Speaker] {
			let decoded = Decoder::new(bytes, chip).count();
			assert!(decoded <= bytes.len() + 1);
			let mut stepper = Stepper::new(bytes, chip).with_loop(rngless_offset(bytes));
			for _ in 0..200 {
				let _ = stepper.tick(|_| {});
			}
		}
		if let Ok(mut stepper) = Stepper::from_file(bytes) {
			for _ in 0..200 {
				let _ = stepper.tick(|_| {});
			}
		}
		if let Ok((_, decoder)) = Decoder::from_file(bytes) {
			let _ = decoder.count();
		}
	});
}

fn rngless_offset(bytes: &[u8]) -> usize {
	bytes.first().map_or(0, |&b| usize::from(b) % 8)
}
