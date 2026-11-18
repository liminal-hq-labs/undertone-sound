// White, pink and brown noise buffers for noise voices, as Undertone's `noise.ts` makes them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use crate::graph::Buffer;
use crate::math;
use crate::params::SoundType;
use crate::random::SeededRandom;

/// `buildNoiseBuffer`: one channel of `duration` seconds (at least one frame) of the noise `kind`,
/// drawn from `random`, computed in doubles and stored as `f32` as a `Float32Array` stores it.
/// Anything but white or pink is brown.
pub fn noise_buffer(
    sample_rate: f64,
    kind: SoundType,
    duration: f64,
    random: &mut SeededRandom,
) -> Buffer {
    let length = math::max(1.0, math::round(sample_rate * duration)) as usize;
    let mut data = Vec::with_capacity(length);
    match kind {
        SoundType::White => {
            for _ in 0..length {
                data.push((random.next_f64() * 2.0 - 1.0) as f32);
            }
        }
        SoundType::Pink => {
            // Paul Kellet's refined pink noise filter (economy variant).
            let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5, mut b6) =
                (0.0f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for _ in 0..length {
                let white = random.next_f64() * 2.0 - 1.0;
                b0 = 0.99886 * b0 + white * 0.0555179;
                b1 = 0.99332 * b1 + white * 0.0750759;
                b2 = 0.969 * b2 + white * 0.153852;
                b3 = 0.8665 * b3 + white * 0.3104856;
                b4 = 0.55 * b4 + white * 0.5329522;
                b5 = -0.7616 * b5 - white * 0.016898;
                let pink = b0 + b1 + b2 + b3 + b4 + b5 + b6 + white * 0.5362;
                b6 = white * 0.115926;
                data.push((pink * 0.11) as f32);
            }
        }
        _ => {
            // A leaky random walk: integrated white noise.
            let mut last = 0.0f64;
            for _ in 0..length {
                let white = random.next_f64() * 2.0 - 1.0;
                last = (last + 0.02 * white) / 1.02;
                data.push((last * 3.5) as f32);
            }
        }
    }
    Buffer {
        sample_rate,
        channels: vec![data],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::seeded_random;

    #[test]
    fn lengths_round_and_never_reach_zero() {
        let mut random = seeded_random("x");
        assert_eq!(
            noise_buffer(48000.0, SoundType::White, 0.1, &mut random).len(),
            4800
        );
        assert_eq!(
            noise_buffer(48000.0, SoundType::White, 0.0, &mut random).len(),
            1
        );
    }

    #[test]
    fn white_noise_is_the_seeded_stream() {
        let mut random = seeded_random("0|noise|white|0#0");
        let buffer = noise_buffer(48000.0, SoundType::White, 0.001, &mut random);
        let mut again = seeded_random("0|noise|white|0#0");
        assert_eq!(buffer.channels[0][0], (again.next_f64() * 2.0 - 1.0) as f32);
    }

    #[test]
    fn pink_and_brown_stay_in_range() {
        for kind in [SoundType::Pink, SoundType::Brown] {
            let mut random = seeded_random("y");
            let buffer = noise_buffer(48000.0, kind, 0.5, &mut random);
            assert!(buffer.channels[0].iter().all(|s| s.abs() < 1.5));
        }
    }
}
