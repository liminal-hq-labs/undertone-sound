// Exact rational arithmetic for cycle time
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// An exact rational number, always in lowest terms with a positive denominator.
///
/// Port of Undertone's `Fraction`, with `i128` parts in place of JavaScript doubles so that no
/// product of song-sized numbers can lose precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fraction {
	n: i128,
	d: i128,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
	while b != 0 {
		(a, b) = (b, a % b);
	}
	a
}

impl Fraction {
	/// Zero, as `0/1`.
	pub const ZERO: Fraction = Fraction { n: 0, d: 1 };
	/// One, as `1/1`.
	pub const ONE: Fraction = Fraction { n: 1, d: 1 };

	/// `numerator / denominator`, normalised. Panics when the denominator is 0.
	pub fn new(numerator: i128, denominator: i128) -> Fraction {
		assert!(denominator != 0, "Fraction denominator cannot be 0");
		let (mut n, mut d) = (numerator, denominator);
		if d < 0 {
			n = -n;
			d = -d;
		}
		let g = gcd(n.abs(), d);
		let g = if g == 0 { 1 } else { g };
		Fraction { n: n / g, d: d / g }
	}

	/// A whole number as a fraction.
	pub fn from_int(value: i128) -> Fraction {
		Fraction { n: value, d: 1 }
	}

	/// Undertone's `Fraction.from(number)`: a continued-fraction approximation, exact for anything
	/// a person would type (`0.25`, `1.5`). `None` for a value that is not finite.
	pub fn from_f64(value: f64) -> Option<Fraction> {
		if !value.is_finite() {
			return None;
		}
		if value.fract() == 0.0 {
			return Some(Fraction::from_int(value as i128));
		}
		let sign = if value < 0.0 { -1.0 } else { 1.0 };
		let target = value.abs();
		let (mut h1, mut h2, mut k1, mut k2) = (1.0_f64, 0.0_f64, 0.0_f64, 1.0_f64);
		let mut rest = target;
		loop {
			let whole = rest.floor();
			(h1, h2) = (whole * h1 + h2, h1);
			(k1, k2) = (whole * k1 + k2, k1);
			rest = 1.0 / (rest - whole);
			if !((target - h1 / k1).abs() > target * f64::EPSILON && k1 < 1e9) {
				break;
			}
		}
		Some(Fraction::new((sign * h1) as i128, k1 as i128))
	}

	/// The numerator, in lowest terms.
	pub fn numer(&self) -> i128 {
		self.n
	}

	/// The denominator, in lowest terms and positive.
	pub fn denom(&self) -> i128 {
		self.d
	}

	/// The largest integer not above this value.
	pub fn floor(self) -> i128 {
		self.n.div_euclid(self.d)
	}

	/// The start of the cycle this time falls in.
	pub fn sam(self) -> Fraction {
		Fraction::from_int(self.floor())
	}

	/// The start of the next cycle.
	pub fn next_sam(self) -> Fraction {
		Fraction::from_int(self.floor() + 1)
	}

	/// `n / d` as a double, the way `Fraction.toNumber()` computes it.
	pub fn to_f64(self) -> f64 {
		self.n as f64 / self.d as f64
	}
}

impl Add for Fraction {
	type Output = Fraction;

	fn add(self, other: Fraction) -> Fraction {
		Fraction::new(self.n * other.d + other.n * self.d, self.d * other.d)
	}
}

impl Sub for Fraction {
	type Output = Fraction;

	fn sub(self, other: Fraction) -> Fraction {
		Fraction::new(self.n * other.d - other.n * self.d, self.d * other.d)
	}
}

impl Mul for Fraction {
	type Output = Fraction;

	fn mul(self, other: Fraction) -> Fraction {
		Fraction::new(self.n * other.n, self.d * other.d)
	}
}

impl Div for Fraction {
	type Output = Fraction;

	/// Panics on division by zero (Undertone throws).
	fn div(self, other: Fraction) -> Fraction {
		assert!(other.n != 0, "Division by zero fraction");
		Fraction::new(self.n * other.d, self.d * other.n)
	}
}

impl PartialOrd for Fraction {
	fn partial_cmp(&self, other: &Fraction) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for Fraction {
	fn cmp(&self, other: &Fraction) -> Ordering {
		(self.n * other.d).cmp(&(other.n * self.d))
	}
}

impl fmt::Display for Fraction {
	/// `3` or `3/2`, as Undertone's `toString`.
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		if self.d == 1 {
			write!(f, "{}", self.n)
		} else {
			write!(f, "{}/{}", self.n, self.d)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn normalises() {
		let f = Fraction::new(6, -8);
		assert_eq!((f.numer(), f.denom()), (-3, 4));
		assert_eq!(Fraction::new(0, -5), Fraction::ZERO);
	}

	#[test]
	fn decimals_are_exact_for_typed_numbers() {
		assert_eq!(Fraction::from_f64(1.5), Some(Fraction::new(3, 2)));
		assert_eq!(Fraction::from_f64(0.25), Some(Fraction::new(1, 4)));
		assert_eq!(Fraction::from_f64(-0.125), Some(Fraction::new(-1, 8)));
		assert_eq!(Fraction::from_f64(0.1), Some(Fraction::new(1, 10)));
		assert_eq!(Fraction::from_f64(f64::NAN), None);
	}

	#[test]
	fn floors_toward_negative_infinity() {
		assert_eq!(Fraction::new(-1, 3).floor(), -1);
		assert_eq!(Fraction::new(7, 4).sam(), Fraction::ONE);
		assert_eq!(Fraction::new(7, 4).next_sam(), Fraction::from_int(2));
	}

	#[test]
	fn displays_like_undertone() {
		assert_eq!(Fraction::new(4, 2).to_string(), "2");
		assert_eq!(Fraction::new(-1, 3).to_string(), "-1/3");
	}
}
