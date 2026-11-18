// Undertone's seeded random sources: mulberry32, and a context's seed with its per-key counts
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `seededRandom`, `seedContext`, `contextRandom` and `keyedRandom` from `random.ts` in
//! `@liminal-hq/undertone`. The generator is the same mulberry32 `fizz-audio-generate` ports for
//! the pattern generator; it is repeated here so the engine does not depend on the generator. Every
//! step is the integer operation JavaScript performs: `Math.imul` is a wrapping multiply, `>>> 0`
//! keeps 32 bits, and a number is the 32-bit word over 2^32.

use std::collections::HashMap;

use fizz_audio_song::js::number_to_string;

/// A stream of numbers in `[0, 1)`; the same seed gives the same stream.
#[derive(Debug, Clone)]
pub struct SeededRandom {
    state: u32,
}

/// The generator for a string seed, hashed first with FNV-1a over UTF-16 code units.
pub fn seeded_random(seed: &str) -> SeededRandom {
    let mut hash: u32 = 0x811c_9dc5;
    for unit in seed.encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    SeededRandom { state: hash }
}

impl SeededRandom {
    /// The next number, in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b_79f5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// A context's seed (`seedContext`) and how many sources each key has had, so two voices with the
/// same key still differ, deterministically.
#[derive(Debug, Clone)]
pub struct Seeds {
    seed: f64,
    occurrences: HashMap<String, u32>,
}

impl Seeds {
    /// `seedContext(ctx, seed)`: a fresh seed with every count at zero.
    pub fn new(seed: f64) -> Self {
        Seeds {
            seed,
            occurrences: HashMap::new(),
        }
    }

    /// `contextRandom(ctx, key)`: the `n`th source for `key`, seeded `"<seed>|<key>#<n>"`.
    pub fn context_random(&mut self, key: &str) -> SeededRandom {
        let n = self.occurrences.entry(key.to_string()).or_insert(0);
        let seed = format!("{}|{key}#{n}", number_to_string(self.seed));
        *n += 1;
        seeded_random(&seed)
    }

    /// `keyedRandom(ctx, key)`: the same source for the same key every time, seeded `"<seed>|<key>"`.
    pub fn keyed_random(&self, key: &str) -> SeededRandom {
        seeded_random(&format!("{}|{key}", number_to_string(self.seed)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_undertone_for_the_empty_seed() {
        // The first values of `seededRandom("")`, as `fixtures/generate/random.json` records them.
        let mut random = seeded_random("");
        assert_eq!(random.next_f64(), 0.6112444521859288);
        assert_eq!(random.next_f64(), 0.4935242917854339);
        assert_eq!(random.next_f64(), 0.7740248835179955);
    }

    #[test]
    fn context_sources_count_each_key() {
        let mut seeds = Seeds::new(0.0);
        let first = seeds.context_random("noise|white|0").next_f64();
        let second = seeds.context_random("noise|white|0").next_f64();
        assert_eq!(first, seeded_random("0|noise|white|0#0").next_f64());
        assert_eq!(second, seeded_random("0|noise|white|0#1").next_f64());
        assert_eq!(
            seeds.keyed_random("ir|1|6").next_f64(),
            seeded_random("0|ir|1|6").next_f64()
        );
    }
}
