// JavaScript's number and string behaviours that output matching the TypeScript depends on
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The TypeScript prints numbers with `String(n)`, `toFixed(2)` and `JSON.stringify`, rounds with
//! `Math.round` and trims with `String.prototype.trim`. Each differs from Rust's default in a way
//! the golden fixtures catch, so this module copies the JavaScript rule.

/// `String(n)`: the shortest digits that read back as `n`, laid out by the rules of
/// `Number::toString` (no exponent from `1e-6` up to below `1e21`, `-0` as `0`).
pub fn number_to_string(value: f64) -> String {
	if value.is_nan() {
		return "NaN".to_string();
	}
	if value == 0.0 {
		return "0".to_string();
	}
	if value.is_infinite() {
		return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
	}
	if value < 0.0 {
		return format!("-{}", number_to_string(-value));
	}
	// `{:e}` gives the shortest round-tripping digits as `d.ddde<exp>`.
	let scientific = format!("{value:e}");
	let (mantissa, exponent) = scientific
		.split_once('e')
		.expect("`{:e}` always has an exponent");
	let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
	let exponent: i32 = exponent.parse().expect("`{:e}` exponent is an integer");
	let k = digits.len() as i32;
	// The decimal point sits `n` digits from the left of `digits`.
	let n = exponent + 1;
	if k <= n && n <= 21 {
		format!("{digits}{}", "0".repeat((n - k) as usize))
	} else if 0 < n && n <= 21 {
		format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
	} else if -6 < n && n <= 0 {
		format!("0.{}{digits}", "0".repeat((-n) as usize))
	} else {
		let e = n - 1;
		let sign = if e < 0 { '-' } else { '+' };
		if k == 1 {
			format!("{digits}e{sign}{}", e.abs())
		} else {
			format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs())
		}
	}
}

/// `n.toFixed(2)`. JavaScript rounds an exact tie up in magnitude (`0.125` gives `0.13`), where
/// Rust's formatter rounds a tie to even.
pub fn to_fixed_2(value: f64) -> String {
	if !value.is_finite() || value.abs() >= 1e21 {
		return number_to_string(value);
	}
	let negative = value < 0.0;
	let magnitude = value.abs();
	// A tie at the third decimal is a multiple of 1/8, so it prints exactly in three decimals.
	let wide = format!("{magnitude:.40}");
	let (whole, fraction) = wide.split_once('.').expect("a fixed format has a point");
	let tie = fraction.as_bytes()[2] == b'5' && fraction[3..].bytes().all(|b| b == b'0');
	let text = if tie {
		let hundredths = format!("{whole}{}", &fraction[..2]);
		let bumped: u128 = hundredths.parse::<u128>().expect("digits") + 1;
		let padded = format!("{bumped:03}");
		let (integer, cents) = padded.split_at(padded.len() - 2);
		format!("{integer}.{cents}")
	} else {
		format!("{magnitude:.2}")
	};
	if negative { format!("-{text}") } else { text }
}

/// `Math.round`: ties go toward positive infinity (`-0.125 * 100` style cases round up).
pub fn round(value: f64) -> f64 {
	let floor = value.floor();
	if value - floor >= 0.5 {
		floor + 1.0
	} else {
		floor
	}
}

/// Whether a character is whitespace or a line terminator to JavaScript's `trim()`.
pub fn is_js_space(c: char) -> bool {
	matches!(
		c,
		'\u{0009}'
			| '\u{000A}'
			| '\u{000B}'
			| '\u{000C}'
			| '\u{000D}'
			| '\u{0020}'
			| '\u{00A0}'
			| '\u{1680}'
			| '\u{2000}'
			..='\u{200A}'
				| '\u{2028}' | '\u{2029}'
				| '\u{202F}' | '\u{205F}'
				| '\u{3000}' | '\u{FEFF}'
	)
}

/// `String.prototype.trim`: Rust's `trim` differs on U+0085 (trimmed there, not in JavaScript)
/// and U+FEFF (the reverse).
pub fn trim(text: &str) -> &str {
	text.trim_matches(is_js_space)
}

/// The length of a string in UTF-16 code units, which is what JavaScript's `.length` counts.
pub fn utf16_len(text: &str) -> usize {
	text.encode_utf16().count()
}

/// `JSON.stringify` for a string.
pub fn quote(text: &str) -> String {
	let mut out = String::with_capacity(text.len() + 2);
	out.push('"');
	for c in text.chars() {
		match c {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\u{0008}' => out.push_str("\\b"),
			'\u{000C}' => out.push_str("\\f"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
			c => out.push(c),
		}
	}
	out.push('"');
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn numbers_print_like_javascript() {
		let cases: &[(f64, &str)] = &[
			(0.0, "0"),
			(-0.0, "0"),
			(1.0, "1"),
			(-1.5, "-1.5"),
			(0.1, "0.1"),
			(100.0, "100"),
			(1e21, "1e+21"),
			(1e20, "100000000000000000000"),
			(1e-7, "1e-7"),
			(1e-6, "0.000001"),
			(123456789012.0, "123456789012"),
			(0.30000000000000004, "0.30000000000000004"),
			(5e-324, "5e-324"),
			(1.7976931348623157e308, "1.7976931348623157e+308"),
			(1.5e300, "1.5e+300"),
			(123.456, "123.456"),
		];
		for (value, text) in cases {
			assert_eq!(number_to_string(*value), *text, "{value:e}");
		}
	}

	#[test]
	fn to_fixed_rounds_ties_up_in_magnitude() {
		assert_eq!(to_fixed_2(0.125), "0.13");
		assert_eq!(to_fixed_2(0.375), "0.38");
		assert_eq!(to_fixed_2(0.625), "0.63");
		assert_eq!(to_fixed_2(-0.125), "-0.13");
		assert_eq!(to_fixed_2(0.5), "0.50");
		assert_eq!(to_fixed_2(1.005), "1.00");
		assert_eq!(to_fixed_2(2.675), "2.67");
		assert_eq!(to_fixed_2(-0.0), "0.00");
		assert_eq!(to_fixed_2(-0.001), "-0.00");
		assert_eq!(to_fixed_2(0.001), "0.00");
		assert_eq!(to_fixed_2(9.995), "9.99");
		assert_eq!(to_fixed_2(99.875), "99.88");
		assert_eq!(to_fixed_2(1e21), "1e+21");
		assert_eq!(to_fixed_2(0.33333), "0.33");
	}

	#[test]
	fn round_ties_go_up() {
		assert_eq!(round(-0.125 * 100.0 + 0.0), -12.0);
		assert_eq!(round(2.5), 3.0);
		assert_eq!(round(-2.5), -2.0);
		assert_eq!(round(-2.25), -2.0);
		assert_eq!(round(0.49999999999999994), 0.0);
		assert_eq!(round(59.5), 60.0);
	}

	#[test]
	fn trim_follows_javascript() {
		assert_eq!(trim("\u{FEFF} a \u{00A0}"), "a");
		assert_eq!(trim("\u{0085}a"), "\u{0085}a");
	}

	#[test]
	fn strings_quote_like_json_stringify() {
		assert_eq!(quote("a\"b\\c\n\u{7}/é"), "\"a\\\"b\\\\c\\n\\u0007/é\"");
	}
}
