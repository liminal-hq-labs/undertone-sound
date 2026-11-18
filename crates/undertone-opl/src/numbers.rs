// JavaScript's number rules where the chips' arithmetic depends on them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The TypeScript works in doubles and bitwise operators, so a port that must write the same
//! registers has to truncate, wrap and round the same way. Song overrides can hold any JSON
//! value, so these helpers also convert one to a number or a truth value as JavaScript would.

use serde_json::Value;

/// `ToInt32`: truncate toward zero, wrap modulo 2^32, and send NaN and infinities to 0.
pub fn to_int32(value: f64) -> i32 {
	if !value.is_finite() {
		return 0;
	}
	let wrapped = value.trunc().rem_euclid(4_294_967_296.0);
	(wrapped as u64 as u32) as i32
}

/// `Math.round`: a tie goes toward positive infinity.
pub fn round(value: f64) -> f64 {
	undertone_mini::js::round(value)
}

/// `Math.min` for two numbers: NaN wins, where `f64::min` ignores it.
pub fn min(a: f64, b: f64) -> f64 {
	if a.is_nan() || b.is_nan() {
		f64::NAN
	} else {
		a.min(b)
	}
}

/// `Math.max` for two numbers: NaN wins.
pub fn max(a: f64, b: f64) -> f64 {
	if a.is_nan() || b.is_nan() {
		f64::NAN
	} else {
		a.max(b)
	}
}

/// `Number(value)` for a JSON value, as far as a song override can reach it.
pub fn js_number(value: &Value) -> f64 {
	match value {
		Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
		Value::Bool(true) => 1.0,
		Value::Bool(false) | Value::Null => 0.0,
		Value::String(text) => {
			let text = undertone_mini::js::trim(text);
			if text.is_empty() {
				0.0
			} else {
				text.parse::<f64>().unwrap_or(f64::NAN)
			}
		}
		Value::Array(_) | Value::Object(_) => f64::NAN,
	}
}

/// Whether JavaScript treats a JSON value as true in a condition.
pub fn js_truthy(value: &Value) -> bool {
	match value {
		Value::Null => false,
		Value::Bool(b) => *b,
		Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
		Value::String(text) => !text.is_empty(),
		Value::Array(_) | Value::Object(_) => true,
	}
}

/// `value.toFixed(digits)`: an exact tie rounds up in magnitude, where Rust's formatter rounds it to even.
pub fn to_fixed(value: f64, digits: usize) -> String {
	let negative = value < 0.0;
	let wide = format!("{:.60}", value.abs());
	let (whole, fraction) = wide.split_once('.').expect("a fixed format has a point");
	let mut kept: Vec<u8> = whole.bytes().chain(fraction.bytes().take(digits)).collect();
	if fraction.as_bytes()[digits] >= b'5' {
		let mut i = kept.len();
		loop {
			if i == 0 {
				kept.insert(0, b'1');
				break;
			}
			i -= 1;
			if kept[i] == b'9' {
				kept[i] = b'0';
			} else {
				kept[i] += 1;
				break;
			}
		}
	}
	let point = kept.len() - digits;
	let mut text = String::from_utf8(kept[..point].to_vec()).expect("digits");
	if digits > 0 {
		text.push('.');
		text.push_str(std::str::from_utf8(&kept[point..]).expect("digits"));
	}
	let zero = text.bytes().all(|b| b == b'0' || b == b'.');
	if negative && !zero {
		format!("-{text}")
	} else {
		text
	}
}

/// A whole-valued double as a JSON integer, anything else as the double, so values print like JavaScript's.
pub fn json_number(value: f64) -> Value {
	if value.is_finite() && value.fract() == 0.0 && value.abs() < 9.0e15 {
		Value::from(value as i64)
	} else {
		serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
	}
}

/// `to_fixed` as JavaScript's number-to-text conversion (`Number(text)` then `String`) for a step position.
pub fn fixed_number_text(value: f64, digits: usize) -> String {
	let parsed: f64 = to_fixed(value, digits).parse().unwrap_or(f64::NAN);
	undertone_mini::js::number_to_string(parsed)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn int32_wraps_and_truncates() {
		assert_eq!(to_int32(2.9), 2);
		assert_eq!(to_int32(-2.9), -2);
		assert_eq!(to_int32(4_294_967_297.0), 1);
		assert_eq!(to_int32(2_147_483_648.0), i32::MIN);
		assert_eq!(to_int32(f64::NAN), 0);
	}

	#[test]
	fn to_fixed_rounds_ties_up() {
		assert_eq!(to_fixed(0.75, 1), "0.8");
		assert_eq!(to_fixed(2.25, 1), "2.3");
		assert_eq!(to_fixed(0.0625, 3), "0.063");
		assert_eq!(to_fixed(9.96, 1), "10.0");
		assert_eq!(to_fixed(12.0, 1), "12.0");
		assert_eq!(to_fixed(3.0, 0), "3");
	}
}
