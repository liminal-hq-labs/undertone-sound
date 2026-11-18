// A small radix-2 complex FFT, for the convolver and the oscillator's band-limited tables
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An iterative, in-place, decimation-in-time transform of a power-of-two length, with its
//! twiddle factors computed once in `f64`. Neither direction scales: a forward then an inverse
//! transform multiplies by the length. It is in-house so the renderer needs no new dependency;
//! the sizes Undertone needs (256 to 8192 points) are small enough that a plain radix-2 is fast.

/// A complex number in `f32`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Complex {
	/// The real part.
	pub re: f32,
	/// The imaginary part.
	pub im: f32,
}

impl Complex {
	/// Zero.
	pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

	/// A complex number from its parts.
	pub const fn new(re: f32, im: f32) -> Self {
		Complex { re, im }
	}

	#[inline]
	/// The product of two complex numbers.
	pub fn times(self, other: Complex) -> Complex {
		Complex {
			re: self.re * other.re - self.im * other.im,
			im: self.re * other.im + self.im * other.re,
		}
	}

	#[inline]
	/// The complex conjugate.
	pub fn conj(self) -> Complex {
		Complex {
			re: self.re,
			im: -self.im,
		}
	}
}

/// A transform of one length.
#[derive(Debug, Clone)]
pub struct Fft {
	len: usize,
	/// `e^{-2πik/len}` for `k` below `len / 2`.
	twiddles: Vec<Complex>,
	/// Where each index goes in the bit-reversed order.
	reversed: Vec<u32>,
}

impl Fft {
	/// A transform of `len` points, a power of two of at least 2.
	pub fn new(len: usize) -> Self {
		assert!(
			len >= 2 && len.is_power_of_two(),
			"an FFT needs a power of two"
		);
		let twiddles = (0..len / 2)
			.map(|k| {
				let angle = -2.0 * std::f64::consts::PI * k as f64 / len as f64;
				Complex::new(angle.cos() as f32, angle.sin() as f32)
			})
			.collect();
		let bits = len.trailing_zeros();
		let reversed = (0..len as u32)
			.map(|i| i.reverse_bits() >> (32 - bits))
			.collect();
		Fft {
			len,
			twiddles,
			reversed,
		}
	}

	/// The transform length in points.
	pub fn len(&self) -> usize {
		self.len
	}

	/// Always false: a transform has at least one point.
	pub fn is_empty(&self) -> bool {
		false
	}

	/// `X[k] = Σ x[n] e^{-2πikn/N}` in place.
	pub fn forward(&self, data: &mut [Complex]) {
		self.transform(data, false);
	}

	/// `x[n] = Σ X[k] e^{+2πikn/N}` in place, unscaled.
	pub fn inverse(&self, data: &mut [Complex]) {
		self.transform(data, true);
	}

	fn transform(&self, data: &mut [Complex], inverse: bool) {
		debug_assert_eq!(data.len(), self.len);
		for (i, &j) in self.reversed.iter().enumerate() {
			let j = j as usize;
			if i < j {
				data.swap(i, j);
			}
		}
		let mut size = 2;
		while size <= self.len {
			let half = size / 2;
			let step = self.len / size;
			for start in (0..self.len).step_by(size) {
				for k in 0..half {
					let w = self.twiddles[k * step];
					let w = if inverse { w.conj() } else { w };
					let a = data[start + k];
					let b = data[start + k + half].times(w);
					data[start + k] = Complex::new(a.re + b.re, a.im + b.im);
					data[start + k + half] = Complex::new(a.re - b.re, a.im - b.im);
				}
			}
			size *= 2;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_transform_matches_the_direct_sum_and_inverts() {
		let n = 16;
		let fft = Fft::new(n);
		let signal: Vec<Complex> = (0..n)
			.map(|i| Complex::new((i as f32 * 0.7).sin(), (i as f32 * 0.3).cos()))
			.collect();
		let mut data = signal.clone();
		fft.forward(&mut data);
		for (k, got) in data.iter().enumerate() {
			let (mut re, mut im) = (0f64, 0f64);
			for (i, x) in signal.iter().enumerate() {
				let a = -2.0 * std::f64::consts::PI * (k * i) as f64 / n as f64;
				re += f64::from(x.re) * a.cos() - f64::from(x.im) * a.sin();
				im += f64::from(x.re) * a.sin() + f64::from(x.im) * a.cos();
			}
			assert!((f64::from(got.re) - re).abs() < 1e-5, "bin {k}");
			assert!((f64::from(got.im) - im).abs() < 1e-5, "bin {k}");
		}
		fft.inverse(&mut data);
		for (x, y) in signal.iter().zip(&data) {
			assert!((x.re - y.re / n as f32).abs() < 1e-6);
			assert!((x.im - y.im / n as f32).abs() < 1e-6);
		}
	}
}
