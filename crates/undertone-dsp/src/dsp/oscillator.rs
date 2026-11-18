// Band-limited wavetables for the oscillator, built and read as Chromium's PeriodicWave does
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Every waveform, the four built-in types included, is a Fourier series (the specification's
//! "oscillator coefficients" for the basic types). From it come one table per pitch range, three
//! ranges an octave, each keeping only the partials that stay under Nyquist for the fundamentals
//! of its range (the inverse FFT of the culled coefficients). Unless normalisation is disabled,
//! every table is scaled by the reciprocal of the peak of the first, fullest table, as the
//! specification's "waveform normalisation" and Chromium do. A sample is read with linear
//! interpolation inside the two tables nearest the fundamental and a crossfade between them.
//! The table size is 2048, 4096 or 16384 frames by sample rate (4096 at 44.1 and 48 kHz).

use std::f64::consts::PI;

use crate::dsp::fft::{Complex, Fft};
use crate::graph::OscillatorType;

/// Pitch ranges an octave.
const RANGES_PER_OCTAVE: f64 = 3.0;

/// One waveform's tables at one sample rate.
#[derive(Debug, Clone)]
pub struct WaveTables {
    size: usize,
    /// One table a range, fullest first; each `size + 1` long, the last repeating the first.
    tables: Vec<Vec<f32>>,
    lowest_fundamental: f64,
    cents_per_range: f64,
    /// Table frames per second of phase at 1 Hz: `size / sampleRate`.
    rate_scale: f64,
}

/// `PeriodicWaveSize`: the table length for a sample rate.
pub fn table_size(sample_rate: f64) -> usize {
    if sample_rate <= 24_000.0 {
        2048
    } else if sample_rate <= 88_200.0 {
        4096
    } else {
        16384
    }
}

/// The specification's Fourier coefficients of a basic type, `n` from 1 to `count - 1`:
/// `(real, imag)`, index 0 unused.
pub fn basic_coefficients(kind: OscillatorType, count: usize) -> (Vec<f32>, Vec<f32>) {
    let real = vec![0f32; count];
    let mut imag = vec![0f32; count];
    for (n, b) in imag.iter_mut().enumerate().skip(1) {
        let nf = n as f64;
        let value = match kind {
            OscillatorType::Sine => {
                if n == 1 {
                    1.0
                } else {
                    0.0
                }
            }
            OscillatorType::Square => 2.0 / (nf * PI) * (1.0 - (-1f64).powi(n as i32)),
            OscillatorType::Sawtooth => (-1f64).powi(n as i32 + 1) * 2.0 / (nf * PI),
            OscillatorType::Triangle => 8.0 * (nf * PI / 2.0).sin() / (PI * nf).powi(2),
        };
        *b = value as f32;
    }
    (real, imag)
}

impl WaveTables {
    /// A built-in type's tables.
    pub fn basic(kind: OscillatorType, sample_rate: f64) -> Self {
        let half = table_size(sample_rate) / 2;
        let (real, imag) = basic_coefficients(kind, half);
        WaveTables::new(&real, &imag, false, sample_rate)
    }

    /// `createPeriodicWave(real, imag, { disableNormalization })`'s tables.
    pub fn new(real: &[f32], imag: &[f32], disable_normalization: bool, sample_rate: f64) -> Self {
        let size = table_size(sample_rate);
        let half = size / 2;
        let components = real.len().min(imag.len()).min(half);
        let ranges = (0.5 + RANGES_PER_OCTAVE * (size as f64).log2()) as usize;
        let cents_per_range = 1200.0 / RANGES_PER_OCTAVE;
        let fft = Fft::new(size);
        let mut scale = 1.0f32;
        let mut tables = Vec::with_capacity(ranges);
        let mut data = vec![Complex::ZERO; size];
        for range in 0..ranges {
            let culling = 2f64.powf(-(range as f64 * cents_per_range) / 1200.0);
            let partials = (culling * half as f64) as usize;
            data.fill(Complex::ZERO);
            // x[j] = Σ a_k cos(2πkj/N) + b_k sin(2πkj/N) = Re Σ (a_k − i b_k) e^{2πikj/N}.
            for k in 1..components.min(partials + 1) {
                data[k] = Complex::new(real[k], -imag[k]);
            }
            fft.inverse(&mut data);
            let mut table: Vec<f32> = data.iter().map(|c| c.re).collect();
            if range == 0 && !disable_normalization {
                let peak = table.iter().fold(0f32, |m, v| m.max(v.abs()));
                if peak > 0.0 {
                    scale = 1.0 / peak;
                }
            }
            for value in &mut table {
                *value *= scale;
            }
            table.push(table[0]);
            tables.push(table);
        }
        let nyquist = sample_rate / 2.0;
        WaveTables {
            size,
            tables,
            lowest_fundamental: nyquist / half as f64,
            cents_per_range,
            rate_scale: size as f64 / sample_rate,
        }
    }

    /// The two tables for a fundamental and the crossfade between them:
    /// `(fuller, thinner, factor)`, the sample being `fuller · (1 − factor) + thinner · factor`.
    pub fn pick(&self, frequency: f64) -> (usize, usize, f32) {
        let frequency = frequency.abs();
        let ratio = if frequency > 0.0 {
            frequency / self.lowest_fundamental
        } else {
            0.5
        };
        let cents = ratio.log2() * 1200.0;
        // One range up, so partials are culled just before they would alias.
        let last = (self.tables.len() - 1) as f64;
        let range = (1.0 + cents / self.cents_per_range).clamp(0.0, last);
        let fuller = range as usize;
        let thinner = (fuller + 1).min(self.tables.len() - 1);
        (fuller, thinner, (range - fuller as f64) as f32)
    }

    /// The sample at `phase` (cycles, 0 to 1) from the tables `pick` chose.
    #[inline]
    pub fn sample(&self, phase: f64, pick: (usize, usize, f32)) -> f32 {
        let position = phase * self.size as f64;
        let index = position as usize;
        let index = index.min(self.size - 1);
        let frac = (position - index as f64) as f32;
        let read = |table: &[f32]| table[index] + (table[index + 1] - table[index]) * frac;
        let fuller = read(&self.tables[pick.0]);
        if pick.2 == 0.0 {
            return fuller;
        }
        let thinner = read(&self.tables[pick.1]);
        fuller + (thinner - fuller) * pick.2
    }

    /// Phase advance a frame at `frequency`, in cycles.
    pub fn increment(&self, frequency: f64) -> f64 {
        frequency * self.rate_scale / self.size as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_table_is_a_unit_sine() {
        let tables = WaveTables::basic(OscillatorType::Sine, 48_000.0);
        let pick = tables.pick(440.0);
        for i in 0..64 {
            let phase = i as f64 / 64.0;
            let expected = (2.0 * PI * phase).sin() as f32;
            assert!((tables.sample(phase, pick) - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn tables_keep_only_partials_below_nyquist() {
        let rate = 48_000.0;
        let tables = WaveTables::basic(OscillatorType::Sawtooth, rate);
        let pick = tables.pick(5_000.0);
        // The fuller table at 5 kHz has at most four partials: 20 kHz is under 24 kHz.
        let size = tables.size;
        let fft = Fft::new(size);
        let mut data: Vec<Complex> = tables.tables[pick.0][..size]
            .iter()
            .map(|v| Complex::new(*v, 0.0))
            .collect();
        fft.forward(&mut data);
        for (k, bin) in data.iter().enumerate().take(size / 2).skip(1) {
            let magnitude = (bin.re * bin.re + bin.im * bin.im).sqrt();
            if k as f64 * 5_000.0 >= rate / 2.0 {
                assert!(magnitude < 1e-3, "partial {k} is {magnitude}");
            }
        }
    }

    #[test]
    fn a_normalised_square_peaks_at_one() {
        let tables = WaveTables::basic(OscillatorType::Square, 44_100.0);
        let peak = tables.tables[0].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - 1.0).abs() < 1e-6);
        let raw = WaveTables::new(&[0.0, 0.0], &[0.0, 0.5], true, 44_100.0);
        let peak = raw.tables[0].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 1e-6);
    }
}
