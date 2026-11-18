// An AudioParam's automation timeline: the five methods Undertone calls, as values at frame times
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Events are kept as the browsers keep them: sorted by time (a ramp's time is its end), a new
//! event after those already at its time, and one of the same type at the same time replacing the
//! old (Chromium's `InsertEvent`); `cancelScheduledValues(t)` removes every event at or after `t`.
//! The timeline is then read once into segments, each a formula from the specification's
//! "computation of value": a held value, a linear or an exponential ramp from the previous event's
//! time and value, or `setTargetAtTime`'s approach `V1 + (V0 − V1)·e^{−(t − T0)/τ}`. A ramp after a
//! `setTargetAtTime` that has not started starts where the target would have, replacing it. Values
//! are computed in `f64` at each frame's time `n / sampleRate`.

use crate::graph::Automation;

/// An automation event as it sits in the list.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Event {
	Set { value: f64, at: f64 },
	Linear { value: f64, until: f64 },
	Exponential { value: f64, until: f64 },
	Target { value: f64, from: f64, tau: f64 },
}

impl Event {
	fn time(self) -> f64 {
		match self {
			Event::Set { at, .. } => at,
			Event::Linear { until, .. } | Event::Exponential { until, .. } => until,
			Event::Target { from, .. } => from,
		}
	}

	fn same_type(self, other: Event) -> bool {
		std::mem::discriminant(&self) == std::mem::discriminant(&other)
	}
}

/// One piece of the param's value over time, from `start` until the next segment's start.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
	Hold(f64),
	Linear { t0: f64, v0: f64, t1: f64, v1: f64 },
	Exponential { t0: f64, v0: f64, t1: f64, v1: f64 },
	Target { t0: f64, v0: f64, v1: f64, tau: f64 },
}

impl Shape {
	fn at(self, t: f64) -> f64 {
		match self {
			Shape::Hold(v) => v,
			Shape::Linear { t0, v0, t1, v1 } => v0 + (v1 - v0) * ((t - t0) / (t1 - t0)),
			Shape::Exponential { t0, v0, t1, v1 } => v0 * (v1 / v0).powf((t - t0) / (t1 - t0)),
			Shape::Target { t0, v0, v1, tau } => {
				if tau <= 0.0 {
					v1
				} else {
					v1 + (v0 - v1) * (-(t - t0) / tau).exp()
				}
			}
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Segment {
	start: f64,
	shape: Shape,
}

/// The automation of one AudioParam.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timeline {
	events: Vec<Event>,
}

impl Timeline {
	/// Adds one call, as the browsers insert it.
	pub fn push(&mut self, step: Automation) {
		let event = match step {
			Automation::Set { value, at } => Event::Set { value, at },
			Automation::Linear { value, until } => Event::Linear { value, until },
			Automation::Exponential { value, until } => Event::Exponential { value, until },
			Automation::Target {
				value,
				from,
				time_constant,
			} => Event::Target {
				value,
				from,
				tau: time_constant,
			},
			Automation::Cancel { from } => {
				self.events.retain(|event| event.time() < from);
				return;
			}
		};
		let time = event.time();
		let mut index = self.events.len();
		for (i, old) in self.events.iter().enumerate() {
			if old.time() == time && old.same_type(event) {
				self.events[i] = event;
				return;
			}
			if old.time() > time {
				index = i;
				break;
			}
		}
		self.events.insert(index, event);
	}

	/// Whether no event is scheduled.
	pub fn is_empty(&self) -> bool {
		self.events.is_empty()
	}

	/// The segments a render reads, starting from `default`.
	pub fn curve(&self, default: f64) -> Curve {
		let mut segments = vec![Segment {
			start: f64::NEG_INFINITY,
			shape: Shape::Hold(default),
		}];
		// The previous event's time and the value it ends on.
		let (mut time, mut value) = (0.0f64, default);
		// A target that is the last event: where it started and the value before it.
		let mut target: Option<(f64, f64)> = None;
		for event in &self.events {
			match *event {
				Event::Set { value: v, at } => {
					segments.push(Segment {
						start: at,
						shape: Shape::Hold(v),
					});
					(time, value) = (at, v);
					target = None;
				}
				Event::Linear { value: v1, until } | Event::Exponential { value: v1, until } => {
					let (t0, v0) = match target.take() {
						Some(started) => {
							segments.pop();
							started
						}
						None => (time, value),
					};
					let shape = match event {
						Event::Linear { .. } => Shape::Linear {
							t0,
							v0,
							t1: until,
							v1,
						},
						// Opposite signs or a zero start hold the start until the end.
						_ if v0 * v1 <= 0.0 => Shape::Hold(v0),
						_ => Shape::Exponential {
							t0,
							v0,
							t1: until,
							v1,
						},
					};
					if until > t0 {
						segments.push(Segment { start: t0, shape });
					}
					segments.push(Segment {
						start: until,
						shape: Shape::Hold(v1),
					});
					(time, value) = (until, v1);
				}
				Event::Target {
					value: v1,
					from,
					tau,
				} => {
					let before = Curve::value_in(&segments, from);
					segments.push(Segment {
						start: from,
						shape: Shape::Target {
							t0: from,
							v0: before,
							v1,
							tau,
						},
					});
					target = Some((from, before));
					time = from;
					value = before;
				}
			}
		}
		Curve {
			segments,
			cursor: 0,
		}
	}
}

/// A timeline read into segments, with a cursor that moves forward as a render reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
	segments: Vec<Segment>,
	cursor: usize,
}

impl Curve {
	/// A param that never moves from `value`.
	pub fn constant(value: f64) -> Self {
		Curve {
			segments: vec![Segment {
				start: f64::NEG_INFINITY,
				shape: Shape::Hold(value),
			}],
			cursor: 0,
		}
	}

	fn value_in(segments: &[Segment], t: f64) -> f64 {
		// The last segment that has started; among equal starts, the one added last.
		let index = segments.partition_point(|s| s.start <= t);
		segments[index.saturating_sub(1)].shape.at(t)
	}

	/// The value at time `t`, which must not go backwards between calls.
	pub fn at(&mut self, t: f64) -> f64 {
		while self.cursor + 1 < self.segments.len() && self.segments[self.cursor + 1].start <= t {
			self.cursor += 1;
		}
		self.segments[self.cursor].shape.at(t)
	}

	/// When the value is held for the whole span `[from, to)`, that value.
	pub fn held_over(&mut self, from: f64, to: f64) -> Option<f64> {
		let value = self.at(from);
		let next = self.segments.get(self.cursor + 1).map(|s| s.start);
		match self.segments[self.cursor].shape {
			Shape::Hold(_) if next.is_none_or(|start| start >= to) => Some(value),
			_ => None,
		}
	}

	/// Fills `out` with the values at `first_frame`, `first_frame + 1`, … at `sample_rate`.
	/// Returns the value when it is the same for all of them.
	pub fn fill(&mut self, first_frame: usize, sample_rate: f64, out: &mut [f32]) -> Option<f32> {
		let t0 = first_frame as f64 / sample_rate;
		let t1 = (first_frame + out.len()) as f64 / sample_rate;
		if let Some(value) = self.held_over(t0, t1) {
			return Some(value as f32);
		}
		for (i, o) in out.iter_mut().enumerate() {
			*o = self.at((first_frame + i) as f64 / sample_rate) as f32;
		}
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn curve(steps: &[Automation], default: f64) -> Curve {
		let mut timeline = Timeline::default();
		for step in steps {
			timeline.push(*step);
		}
		timeline.curve(default)
	}

	#[test]
	fn set_and_ramps_follow_the_specification() {
		let mut c = curve(
			&[
				Automation::Set {
					value: 0.0,
					at: 1.0,
				},
				Automation::Linear {
					value: 1.0,
					until: 2.0,
				},
				Automation::Exponential {
					value: 4.0,
					until: 4.0,
				},
			],
			0.5,
		);
		assert_eq!(c.at(0.5), 0.5);
		assert_eq!(c.at(1.0), 0.0);
		assert_eq!(c.at(1.25), 0.25);
		assert_eq!(c.at(2.0), 1.0);
		assert!((c.at(3.0) - 2.0).abs() < 1e-12);
		assert_eq!(c.at(5.0), 4.0);
	}

	#[test]
	fn a_target_approaches_and_a_cancel_removes_later_events() {
		let mut c = curve(
			&[
				Automation::Set {
					value: 1.0,
					at: 0.0,
				},
				Automation::Linear {
					value: 0.0,
					until: 10.0,
				},
				Automation::Cancel { from: 1.0 },
				Automation::Set {
					value: 0.8,
					at: 1.0,
				},
				Automation::Target {
					value: 0.0,
					from: 1.0,
					time_constant: 0.5,
				},
			],
			0.0,
		);
		assert_eq!(c.at(0.5), 1.0);
		assert!((c.at(1.5) - 0.8 * (-1.0f64).exp()).abs() < 1e-12);
	}

	#[test]
	fn an_exponential_ramp_from_zero_holds_and_same_events_replace() {
		let mut c = curve(
			&[
				Automation::Set {
					value: 0.0,
					at: 0.0,
				},
				Automation::Exponential {
					value: 1.0,
					until: 1.0,
				},
			],
			0.0,
		);
		assert_eq!(c.at(0.5), 0.0);
		assert_eq!(c.at(1.0), 1.0);
		let mut replaced = curve(
			&[
				Automation::Set {
					value: 0.2,
					at: 0.0,
				},
				Automation::Set {
					value: 0.7,
					at: 0.0,
				},
			],
			0.0,
		);
		assert_eq!(replaced.at(0.0), 0.7);
	}

	#[test]
	fn a_held_block_reads_as_one_value() {
		let mut c = curve(
			&[Automation::Set {
				value: 3.0,
				at: 0.0,
			}],
			1.0,
		);
		let mut out = [0f32; 128];
		assert_eq!(c.fill(0, 48000.0, &mut out), Some(3.0));
		let mut ramp = curve(
			&[Automation::Linear {
				value: 1.0,
				until: 1.0,
			}],
			0.0,
		);
		assert_eq!(ramp.fill(0, 128.0, &mut out), None);
		assert_eq!(out[64], 0.5);
	}
}
