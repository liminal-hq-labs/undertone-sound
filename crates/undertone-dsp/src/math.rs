// JavaScript's Math functions where Undertone's numbers depend on them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Undertone computes in doubles with `Math.*`. A port that must write the same graph, down to the
//! last bit of a shaped wave's coefficients and an impulse response's samples, uses the same
//! functions. The fixtures are written by Bun, whose JavaScriptCore calls the platform's C library
//! for these, as Rust's `f64` methods do, so on Linux (where the fixtures are written and the gate
//! runs) the two agree to the bit. fdlibm (the `libm` crate) does not: it differs in the last bit
//! of a few sines, which shows in a shaped wave's near-zero coefficients. Elsewhere the coefficients
//! can differ in those low bits, which no renderer can hear. `Math.max` and `Math.min` let NaN win,
//! where `f64::max` ignores it.

/// `Math.sin`.
pub fn sin(x: f64) -> f64 {
	x.sin()
}

/// `Math.cos`.
pub fn cos(x: f64) -> f64 {
	x.cos()
}

/// `Math.exp`.
pub fn exp(x: f64) -> f64 {
	x.exp()
}

/// `Math.log`.
pub fn ln(x: f64) -> f64 {
	x.ln()
}

/// `x ** y` and `Math.pow`.
pub fn pow(x: f64, y: f64) -> f64 {
	x.powf(y)
}

/// `Math.max` of two numbers: NaN wins.
pub fn max(a: f64, b: f64) -> f64 {
	if a.is_nan() || b.is_nan() {
		f64::NAN
	} else if a > b || (a == b && a.is_sign_positive()) {
		a
	} else {
		b
	}
}

/// `Math.min` of two numbers: NaN wins.
pub fn min(a: f64, b: f64) -> f64 {
	if a.is_nan() || b.is_nan() {
		f64::NAN
	} else if a < b || (a == b && a.is_sign_negative()) {
		a
	} else {
		b
	}
}

/// `Math.round`: a tie goes toward positive infinity.
pub fn round(x: f64) -> f64 {
	undertone_mini::js::round(x)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn max_and_min_let_nan_win_and_order_zeros() {
		assert!(max(1.0, f64::NAN).is_nan());
		assert!(min(f64::NAN, 1.0).is_nan());
		assert_eq!(max(-0.0, 0.0).to_bits(), 0.0f64.to_bits());
		assert_eq!(min(0.0, -0.0).to_bits(), (-0.0f64).to_bits());
		assert_eq!(max(2.0, 3.0), 3.0);
		assert_eq!(min(2.0, 3.0), 2.0);
	}

	#[test]
	fn round_sends_ties_up() {
		assert_eq!(round(2.5), 3.0);
		assert_eq!(round(-2.5), -2.0);
	}
}
