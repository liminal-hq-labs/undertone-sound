// Mini-notation parser: rhythm strings like "c3 [e3 g3]*2 <a3 b3> x(3,8)" into patterns
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fmt;

use crate::pattern::{Pattern, PatternError};

/// A mini-notation failure: Undertone's message word for word, and where it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniError {
	/// Where in the source, in UTF-16 code units (the way JavaScript counts), or `None` for the one
	/// error that has no place: an empty pattern.
	pub position: Option<usize>,
	/// The whole message, such as `Mini-notation error at position 3 in "d4 h4": ...`.
	pub message: String,
}

impl fmt::Display for MiniError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for MiniError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
	Word,
	Symbol,
}

#[derive(Debug)]
struct Token {
	kind: Kind,
	text: String,
	/// UTF-16 offset.
	pos: usize,
}

struct Term<L> {
	pattern: Pattern<L>,
	weight: f64,
	replicate: usize,
}

const SYMBOLS: &[char] = &['[', ']', '<', '>', '(', ')', ',', '*', '/', '!', '@', '~'];

/// JavaScript's `\s`, which is not Rust's `char::is_whitespace` (no U+0085, and U+FEFF counts).
fn is_js_space(c: char) -> bool {
	matches!(
		c,
		'\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
			..='\u{200A}'
				| '\u{2028}' | '\u{2029}'
				| '\u{202F}' | '\u{205F}'
				| '\u{3000}' | '\u{FEFF}'
	)
}

/// `[a-zA-Z0-9#._-]`; the underscore is for sample names.
fn is_word_char(c: char) -> bool {
	c.is_ascii_alphanumeric() || matches!(c, '#' | '.' | '_' | '-')
}

fn error_at(source: &str, pos: usize, message: &str) -> MiniError {
	MiniError {
		position: Some(pos),
		message: format!("Mini-notation error at position {pos} in \"{source}\": {message}"),
	}
}

fn tokenize(source: &str) -> Result<Vec<Token>, MiniError> {
	let mut tokens = Vec::new();
	let mut pos = 0;
	let mut chars = source.chars().peekable();
	while let Some(c) = chars.next() {
		if is_js_space(c) {
			pos += 1;
		} else if SYMBOLS.contains(&c) {
			tokens.push(Token {
				kind: Kind::Symbol,
				text: c.to_string(),
				pos,
			});
			pos += 1;
		} else if is_word_char(c) {
			let start = pos;
			let mut text = String::from(c);
			pos += 1;
			while let Some(&next) = chars.peek() {
				if !is_word_char(next) {
					break;
				}
				text.push(next);
				chars.next();
				pos += 1;
			}
			tokens.push(Token {
				kind: Kind::Word,
				text,
				pos: start,
			});
		} else {
			// JavaScript reports the first UTF-16 unit, a lone surrogate for a character outside
			// the Basic Multilingual Plane; a Rust string cannot hold one, so it shows as U+FFFD.
			let shown = if c.len_utf16() == 2 { '\u{FFFD}' } else { c };
			return Err(error_at(source, pos, &format!("unexpected \"{shown}\"")));
		}
	}
	Ok(tokens)
}

/// `-?\d+(\.\d+)?` and nothing else.
fn is_strict_decimal(text: &str) -> bool {
	let body = text.strip_prefix('-').unwrap_or(text);
	let (int, frac) = match body.split_once('.') {
		Some((int, frac)) => (int, Some(frac)),
		None => (body, None),
	};
	let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
	digits(int) && frac.is_none_or(digits)
}

struct Parser<'a, L> {
	tokens: Vec<Token>,
	index: usize,
	source: &'a str,
	/// The source's length in UTF-16 code units.
	source_len: usize,
	leaf: &'a dyn Fn(&str) -> Result<L, String>,
}

type Parsed<T> = Result<T, MiniError>;

impl<L> Parser<'_, L> {
	fn fail<T>(&self, message: &str, pos: Option<usize>) -> Parsed<T> {
		let at = pos
			.or_else(|| self.tokens.get(self.index).map(|t| t.pos))
			.unwrap_or(self.source_len);
		Err(error_at(self.source, at, message))
	}

	fn peek(&self) -> Option<&Token> {
		self.tokens.get(self.index)
	}

	fn next(&mut self) -> Parsed<usize> {
		if self.index >= self.tokens.len() {
			return self.fail("unexpected end of pattern", None);
		}
		self.index += 1;
		Ok(self.index - 1)
	}

	fn peek_symbol(&self, text: &str) -> bool {
		self.peek()
			.is_some_and(|t| t.kind == Kind::Symbol && t.text == text)
	}

	fn expect_symbol(&mut self, text: &str) -> Parsed<()> {
		let i = self.next()?;
		let token = &self.tokens[i];
		if token.kind != Kind::Symbol || token.text != text {
			return self.fail(
				&format!("expected \"{text}\" but found \"{}\"", token.text),
				Some(token.pos),
			);
		}
		Ok(())
	}

	fn expect_end(&self) -> Parsed<()> {
		match self.peek() {
			Some(token) => self.fail(&format!("unexpected \"{}\"", token.text), Some(token.pos)),
			None => Ok(()),
		}
	}

	/// A comma-separated list of sequences played at once.
	fn parse_stack(&mut self, closer: Option<&str>) -> Parsed<Pattern<L>> {
		let mut sequences = vec![self.parse_sequence(closer)?];
		while self.peek_symbol(",") {
			self.next()?;
			sequences.push(self.parse_sequence(closer)?);
		}
		if sequences.len() == 1 {
			Ok(sequences.remove(0))
		} else {
			Ok(Pattern::stack(sequences))
		}
	}

	fn at_sequence_end(&self, closer: Option<&str>) -> bool {
		match self.peek() {
			None => true,
			Some(token) => {
				token.kind == Kind::Symbol
					&& (token.text == "," || closer.is_some_and(|c| token.text == c))
			}
		}
	}

	fn parse_sequence(&mut self, closer: Option<&str>) -> Parsed<Pattern<L>> {
		let mut terms = Vec::new();
		while !self.at_sequence_end(closer) {
			terms.push(self.parse_term()?);
		}
		if terms.is_empty() {
			return self.fail("expected at least one element", None);
		}
		let mut expanded: Vec<(f64, Pattern<L>)> = Vec::new();
		for term in terms {
			for _ in 0..term.replicate {
				expanded.push((term.weight, term.pattern.clone()));
			}
		}
		if expanded.len() == 1 {
			return Ok(expanded.remove(0).1);
		}
		// The parser has already ruled out a weight that timecat would refuse.
		Pattern::timecat(expanded).or_else(|e| self.fail(&e.0, None))
	}

	/// Runs a combinator, rewrapping its error with the modifier's source position.
	fn guarded(&self, pos: usize, result: Result<Pattern<L>, PatternError>) -> Parsed<Pattern<L>> {
		result.or_else(|e| self.fail(&e.0, Some(pos)))
	}

	fn parse_term(&mut self) -> Parsed<Term<L>> {
		let mut pattern = self.parse_atom()?;
		let mut weight = 1.0;
		let mut replicate = 1;
		loop {
			if self.peek_symbol("*") {
				let symbol = self.next()?;
				let factor = self.parse_number("*", false)?;
				pattern = self.guarded(self.tokens[symbol].pos, pattern.fast(factor))?;
			} else if self.peek_symbol("/") {
				let symbol = self.next()?;
				let factor = self.parse_number("/", false)?;
				pattern = self.guarded(self.tokens[symbol].pos, pattern.slow(factor))?;
			} else if self.peek_symbol("!") {
				self.next()?;
				let count = self.parse_number("!", true)?;
				if count < 1.0 {
					return self.fail("\"!\" count must be at least 1", None);
				}
				replicate = count as usize;
			} else if self.peek_symbol("@") {
				self.next()?;
				weight = self.parse_number("@", false)?;
				if weight <= 0.0 {
					return self.fail("\"@\" weight must be positive", None);
				}
			} else if self.peek_symbol("(") {
				let symbol = self.next()?;
				let pulses = self.parse_number("euclid pulses", true)?;
				self.expect_symbol(",")?;
				let steps = self.parse_number("euclid steps", true)?;
				let mut rotation = 0.0;
				if self.peek_symbol(",") {
					self.next()?;
					rotation = self.parse_number("euclid rotation", true)?;
				}
				self.expect_symbol(")")?;
				pattern = self.guarded(
					self.tokens[symbol].pos,
					pattern.euclid(pulses as i64, steps as i64, rotation as i64),
				)?;
			} else {
				return Ok(Term {
					pattern,
					weight,
					replicate,
				});
			}
		}
	}

	fn parse_atom(&mut self) -> Parsed<Pattern<L>> {
		let i = self.next()?;
		let (kind, pos) = (self.tokens[i].kind, self.tokens[i].pos);
		let text = self.tokens[i].text.clone();
		if kind == Kind::Word {
			return match (self.leaf)(&text) {
				Ok(value) => Ok(Pattern::pure(value)),
				Err(message) => self.fail(&message, Some(pos)),
			};
		}
		match text.as_str() {
			"~" => Ok(Pattern::silence()),
			"[" => {
				let inner = self.parse_stack(Some("]"))?;
				self.expect_symbol("]")?;
				Ok(inner)
			}
			"<" => {
				let inner = self.parse_alternation()?;
				self.expect_symbol(">")?;
				Ok(inner)
			}
			_ => self.fail(&format!("unexpected \"{text}\""), Some(pos)),
		}
	}

	/// The children of `<...>`: one per cycle, in rotation.
	fn parse_alternation(&mut self) -> Parsed<Pattern<L>> {
		let mut terms = Vec::new();
		while !self.peek_symbol(">") && self.peek().is_some() {
			let term = self.parse_term()?;
			if term.weight != 1.0 {
				return self.fail("\"@\" weights are not supported inside <>", None);
			}
			terms.push(term);
		}
		if terms.is_empty() {
			return self.fail("expected at least one element inside <>", None);
		}
		let mut expanded = Vec::new();
		for term in terms {
			for _ in 0..term.replicate {
				expanded.push(term.pattern.clone());
			}
		}
		Ok(Pattern::cat(expanded))
	}

	fn parse_number(&mut self, context: &str, integer: bool) -> Parsed<f64> {
		let i = self.next()?;
		let token = &self.tokens[i];
		// Strict decimal syntax only: a JavaScript `Number()` would accept "0x10" or "1e3".
		let value = if token.kind == Kind::Word && is_strict_decimal(&token.text) {
			token.text.parse::<f64>().unwrap_or(f64::NAN)
		} else {
			f64::NAN
		};
		if !value.is_finite() {
			return self.fail(
				&format!(
					"expected a number after \"{context}\" but found \"{}\"",
					token.text
				),
				Some(token.pos),
			);
		}
		if integer && value.fract() != 0.0 {
			return self.fail(
				&format!(
					"expected an integer for \"{context}\" but found \"{}\"",
					token.text
				),
				Some(token.pos),
			);
		}
		Ok(value)
	}
}

/// Parses a mini-notation string into a [`Pattern`], turning each word into a value with `leaf`.
///
/// The leaf returns `Err(message)` for a word it rejects; the error is reported at the word's
/// position, inside the usual message. Positions count UTF-16 code units, as Undertone's do.
///
/// Notation: whitespace sequences, `~` rests, `[a b]` subdivision, `<a b>` per-cycle alternation,
/// `a*2` and `a/2` speed, `a!3` replication, `a@3` elongation, `[a,b]` stacking and `a(3,8,rot)`
/// euclidean rhythms.
pub fn parse<L>(
	source: &str,
	leaf: impl Fn(&str) -> Result<L, String>,
) -> Result<Pattern<L>, MiniError> {
	let tokens = tokenize(source)?;
	if tokens.is_empty() {
		return Err(MiniError {
			position: None,
			message: format!("Empty mini-notation pattern: \"{source}\""),
		});
	}
	let mut parser = Parser {
		tokens,
		index: 0,
		source,
		source_len: source.encode_utf16().count(),
		leaf: &leaf,
	};
	let pattern = parser.parse_stack(None)?;
	parser.expect_end()?;
	Ok(pattern)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Fraction;

	fn word(w: &str) -> Result<String, String> {
		Ok(w.to_string())
	}

	#[test]
	fn js_whitespace_separates_words() {
		assert!(parse("a\u{3000}b\u{FEFF}c", word).is_ok());
		// U+0085 is white space to Rust but not to JavaScript.
		let err = parse("a\u{85}b", word).unwrap_err();
		assert_eq!(err.position, Some(1));
	}

	#[test]
	fn astral_characters_are_reported_at_their_utf16_position() {
		let err = parse("a\u{3000}\u{1F600}", word).unwrap_err();
		assert_eq!(err.position, Some(2));
	}

	#[test]
	fn empty_pattern_has_no_position() {
		let err = parse("  ", word).unwrap_err();
		assert_eq!(err.position, None);
		assert_eq!(err.message, "Empty mini-notation pattern: \"  \"");
	}

	#[test]
	fn a_single_term_ignores_its_weight() {
		let pattern = parse("a@3", word).unwrap();
		let events = pattern.events(Fraction::ZERO, Fraction::ONE);
		assert_eq!(events.len(), 1);
		assert_eq!(events[0].end, Fraction::ONE);
	}
}
