// The pattern core: patterns as queries from cycle spans to events
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

use std::ops::{Add, Div, Mul, Sub};
use std::sync::Arc;

use crate::fraction::Fraction;

/// A half-open span of cycle time, `[begin, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub begin: Fraction,
    pub end: Fraction,
}

impl Span {
    pub fn new(begin: Fraction, end: Fraction) -> Span {
        Span { begin, end }
    }

    fn map(self, f: impl Fn(Fraction) -> Fraction) -> Span {
        Span::new(f(self.begin), f(self.end))
    }
}

/// One result of a query, as Undertone's `Hap`. `part` is the fragment inside the queried span,
/// `whole` the event's full extent (it may reach outside the query). The event is an onset, and
/// plays, only when `part.begin == whole.begin`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hap<L> {
    pub part: Span,
    pub whole: Option<Span>,
    pub value: L,
}

impl<L> Hap<L> {
    /// True when the part contains the event's onset.
    pub fn has_onset(&self) -> bool {
        self.whole
            .is_some_and(|whole| whole.begin == self.part.begin)
    }

    fn map_time(self, f: impl Fn(Fraction) -> Fraction) -> Hap<L> {
        Hap {
            part: self.part.map(&f),
            whole: self.whole.map(|whole| whole.map(&f)),
            value: self.value,
        }
    }
}

/// A note that plays: where it starts and ends in cycles, and the leaf's value.
#[derive(Debug, Clone, PartialEq)]
pub struct Event<L> {
    pub begin: Fraction,
    pub end: Fraction,
    pub value: L,
}

/// Why a combinator refused its arguments. The text is Undertone's message, word for word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError(pub String);

#[derive(Debug)]
enum Node<L> {
    Silence,
    Pure(L),
    Stack(Vec<Pattern<L>>),
    Cat(Vec<Pattern<L>>),
    /// A child squeezed into the window `[begin, end)` of every cycle.
    Compress {
        begin: Fraction,
        end: Fraction,
        inner: Pattern<L>,
    },
    Fast(Fraction, Pattern<L>),
    Slow(Fraction, Pattern<L>),
}

/// A pattern is a function from a span of cycles to the events overlapping it. Patterns are
/// immutable and cheap to clone.
#[derive(Debug)]
pub struct Pattern<L>(Arc<Node<L>>);

impl<L> Clone for Pattern<L> {
    fn clone(&self) -> Self {
        Pattern(Arc::clone(&self.0))
    }
}

/// Splits a span at cycle boundaries.
fn split_into_cycles(span: Span) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut begin = span.begin;
    while begin < span.end {
        let end = begin.next_sam().min(span.end);
        spans.push(Span::new(begin, end));
        begin = end;
    }
    spans
}

fn intersect(a: Span, b: Span) -> Option<Span> {
    let begin = a.begin.max(b.begin);
    let end = a.end.min(b.end);
    (begin < end).then(|| Span::new(begin, end))
}

/// A number the way JavaScript prints it in a message: `-0` is `0`, and exponents appear outside
/// 1e-6 to 1e21.
pub(crate) fn js_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs();
    if (1e-6..1e21).contains(&magnitude) {
        return format!("{value}");
    }
    let text = format!("{value:e}");
    match text.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => text,
    }
}

impl<L> Pattern<L> {
    fn node(node: Node<L>) -> Pattern<L> {
        Pattern(Arc::new(node))
    }

    /// The empty pattern.
    pub fn silence() -> Pattern<L> {
        Pattern::node(Node::Silence)
    }

    /// A value repeated once per cycle.
    pub fn pure(value: L) -> Pattern<L> {
        Pattern::node(Node::Pure(value))
    }

    /// Plays every pattern at once.
    pub fn stack(patterns: Vec<Pattern<L>>) -> Pattern<L> {
        Pattern::node(Node::Stack(patterns))
    }

    /// One pattern per cycle, in rotation.
    pub fn cat(patterns: Vec<Pattern<L>>) -> Pattern<L> {
        if patterns.is_empty() {
            return Pattern::silence();
        }
        Pattern::node(Node::Cat(patterns))
    }

    /// Patterns one after another within each cycle, each taking its weight's share.
    pub fn timecat(pairs: Vec<(f64, Pattern<L>)>) -> Result<Pattern<L>, PatternError> {
        let mut weights = Vec::with_capacity(pairs.len());
        for (weight, _) in &pairs {
            if !weight.is_finite() || *weight <= 0.0 {
                return Err(PatternError(format!(
                    "timecat() weights must be positive, got {}",
                    js_number(*weight)
                )));
            }
            weights.push(Fraction::from_f64(*weight).expect("finite weight"));
        }
        if pairs.is_empty() {
            return Ok(Pattern::silence());
        }
        let total = weights
            .iter()
            .fold(Fraction::ZERO, |sum, weight| sum.add(*weight));
        let mut pos = Fraction::ZERO;
        let mut parts = Vec::with_capacity(pairs.len());
        for (weight, (_, pattern)) in weights.into_iter().zip(pairs) {
            let begin = pos.div(total);
            pos = pos.add(weight);
            parts.push(Pattern::compress(begin, pos.div(total), pattern));
        }
        Ok(Pattern::stack(parts))
    }

    /// Equal-weight `timecat`.
    pub fn seq(patterns: Vec<Pattern<L>>) -> Pattern<L> {
        let pairs = patterns.into_iter().map(|p| (1.0, p)).collect();
        Pattern::timecat(pairs).expect("weight 1 is positive")
    }

    fn compress(begin: Fraction, end: Fraction, inner: Pattern<L>) -> Pattern<L> {
        if end.sub(begin).numer() <= 0 {
            return Pattern::silence();
        }
        Pattern::node(Node::Compress { begin, end, inner })
    }

    /// Speeds the pattern up. The factor must be positive.
    pub fn fast(self, factor: f64) -> Result<Pattern<L>, PatternError> {
        match Fraction::from_f64(factor) {
            Some(f) if f.numer() > 0 => Ok(Pattern::node(Node::Fast(f, self))),
            _ => Err(PatternError(format!(
                "fast() factor must be positive, got {}",
                js_number(factor)
            ))),
        }
    }

    /// Slows the pattern down. The factor must be positive.
    pub fn slow(self, factor: f64) -> Result<Pattern<L>, PatternError> {
        match Fraction::from_f64(factor) {
            Some(f) if f.numer() > 0 => Ok(Pattern::node(Node::Slow(f, self))),
            _ => Err(PatternError(format!(
                "slow() factor must be positive, got {}",
                js_number(factor)
            ))),
        }
    }

    /// `pulses` onsets spread over `steps` slots (Bjorklund), rotated left by `rotation`, the whole
    /// pattern squeezed into each onset slot.
    pub fn euclid(
        self,
        pulses: i64,
        steps: i64,
        rotation: i64,
    ) -> Result<Pattern<L>, PatternError> {
        let slots = rotate(&bjorklund(pulses, steps)?, rotation);
        Ok(Pattern::seq(
            slots
                .into_iter()
                .map(|on| if on { self.clone() } else { Pattern::silence() })
                .collect(),
        ))
    }
}

impl<L: Clone> Pattern<L> {
    /// Every hap overlapping `[begin, end)`, in Undertone's order.
    pub fn query(&self, begin: Fraction, end: Fraction) -> Vec<Hap<L>> {
        self.query_span(Span::new(begin, end))
    }

    /// The events that play in `[begin, end)`: haps whose part holds their onset, as a start, an end
    /// and a value, sorted by start (a stable sort, so ties keep Undertone's order). This is
    /// `takeNotes`' loop over `query` in `compile.ts`.
    pub fn events(&self, begin: Fraction, end: Fraction) -> Vec<Event<L>> {
        let mut events: Vec<Event<L>> = self
            .query(begin, end)
            .into_iter()
            .filter(Hap::has_onset)
            .map(|hap| {
                let whole = hap.whole.expect("an onset has a whole");
                Event {
                    begin: whole.begin,
                    end: whole.end,
                    value: hap.value,
                }
            })
            .collect();
        events.sort_by_key(|event| event.begin);
        events
    }

    fn query_span(&self, span: Span) -> Vec<Hap<L>> {
        match &*self.0 {
            Node::Silence => Vec::new(),
            Node::Pure(value) => split_into_cycles(span)
                .into_iter()
                .map(|part| Hap {
                    whole: Some(Span::new(part.begin.sam(), part.begin.next_sam())),
                    part,
                    value: value.clone(),
                })
                .collect(),
            Node::Stack(patterns) => patterns
                .iter()
                .flat_map(|pattern| pattern.query_span(span))
                .collect(),
            Node::Cat(patterns) => {
                let len = patterns.len() as i128;
                split_into_cycles(span)
                    .into_iter()
                    .flat_map(|cycle_span| {
                        let cycle = cycle_span.begin.floor();
                        let index = cycle.rem_euclid(len) as usize;
                        // The chosen pattern experiences its own consecutive cycles.
                        let offset = Fraction::from_int(cycle - cycle.div_euclid(len));
                        let shifted = cycle_span.map(|t| t.sub(offset));
                        patterns[index]
                            .query_span(shifted)
                            .into_iter()
                            .map(move |hap| hap.map_time(|t| t.add(offset)))
                    })
                    .collect()
            }
            Node::Compress { begin, end, inner } => {
                let width = end.sub(*begin);
                split_into_cycles(span)
                    .into_iter()
                    .flat_map(|cycle_span| {
                        let cycle = cycle_span.begin.sam();
                        let window = Span::new(cycle.add(*begin), cycle.add(*end));
                        let Some(clipped) = intersect(cycle_span, window) else {
                            return Vec::new();
                        };
                        let to_inner = |t: Fraction| cycle.add(t.sub(window.begin).div(width));
                        let to_outer = |t: Fraction| window.begin.add(t.sub(cycle).mul(width));
                        inner
                            .query_span(clipped.map(to_inner))
                            .into_iter()
                            .map(|hap| hap.map_time(to_outer))
                            .collect()
                    })
                    .collect()
            }
            Node::Fast(f, inner) => inner
                .query_span(span.map(|t| t.mul(*f)))
                .into_iter()
                .map(|hap| hap.map_time(|t| t.div(*f)))
                .collect(),
            Node::Slow(f, inner) => inner
                .query_span(span.map(|t| t.div(*f)))
                .into_iter()
                .map(|hap| hap.map_time(|t| t.mul(*f)))
                .collect(),
        }
    }
}

/// Bjorklund's algorithm, as Undertone's `bjorklund`: E(3,8) is `x..x..x.`.
pub fn bjorklund(pulses: i64, steps: i64) -> Result<Vec<bool>, PatternError> {
    if steps <= 0 {
        return Err(PatternError(format!(
            "Invalid euclidean rhythm: ({pulses},{steps})"
        )));
    }
    if pulses < 0 || pulses > steps {
        return Err(PatternError(format!(
            "Euclidean pulses must be between 0 and steps, got ({pulses},{steps})"
        )));
    }
    if pulses == 0 {
        return Ok(vec![false; steps as usize]);
    }
    let mut a: Vec<Vec<bool>> = (0..pulses).map(|_| vec![true]).collect();
    let mut b: Vec<Vec<bool>> = (0..steps - pulses).map(|_| vec![false]).collect();
    while b.len() > 1 {
        let take = a.len().min(b.len());
        let combined: Vec<Vec<bool>> = a[..take]
            .iter()
            .zip(&b[..take])
            .map(|(group, other)| group.iter().chain(other).copied().collect())
            .collect();
        let leftover_a = a[take..].to_vec();
        let leftover_b = b[take..].to_vec();
        a = combined;
        b = if leftover_a.is_empty() {
            leftover_b
        } else {
            leftover_a
        };
    }
    Ok(a.into_iter().chain(b).flatten().collect())
}

fn rotate(slots: &[bool], rotation: i64) -> Vec<bool> {
    let len = slots.len();
    let shift = (rotation as i128).rem_euclid(len as i128) as usize;
    (0..len).map(|i| slots[(i + shift) % len]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tresillo() {
        let slots = bjorklund(3, 8).unwrap();
        let text: String = slots.iter().map(|&on| if on { 'x' } else { '.' }).collect();
        assert_eq!(text, "x..x..x.");
    }

    #[test]
    fn js_numbers() {
        assert_eq!(js_number(-0.0), "0");
        assert_eq!(js_number(-2.0), "-2");
        assert_eq!(js_number(0.5), "0.5");
        assert_eq!(js_number(1e21), "1e+21");
        assert_eq!(js_number(1e-7), "1e-7");
    }
}
