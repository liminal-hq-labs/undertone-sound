// The shared reverb and delay bus of each orbit, made the first time a voice sends to it
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `getOrbitBus`, `createOrbitBus` and `buildImpulseResponse` from Undertone's `effects.ts`. Every
//! voice sending on one orbit shares one convolver and one delay line. The room size and the delay
//! settings are last-writer-wins across the voices of an orbit, and the impulse response is
//! rebuilt only when the size changes, so in a render the size an orbit keeps is the last booked.

use fizz_audio_song::js::number_to_string;

use crate::graph::{Automation, Buffer, Graph, Input, NodeId, NodeKind, Param, ParamId};
use crate::math;
use crate::random::{SeededRandom, Seeds};

const ROOM_SIZE_MIN_SECONDS: f64 = 0.3;
const ROOM_SIZE_MAX_SECONDS: f64 = 4.0;
/// Room sizes 1 to 10 span 9 units.
const ROOM_SIZE_REFERENCE_RANGE: f64 = 9.0;
/// A channel each decorrelates the tail.
const IR_CHANNELS: usize = 2;
/// The envelope reaches this fraction of full scale by the buffer's end.
const IR_DECAY_FLOOR: f64 = 0.001;
/// How long an echo can be.
pub const DELAY_MAX_SECONDS: f64 = 5.0;

/// `roomSizeToSeconds`: size 1 to 10 as 0.3 s to 4 s, never shorter than 0.3 s.
pub fn room_size_to_seconds(size: f64) -> f64 {
    let seconds = ROOM_SIZE_MIN_SECONDS
        + ((size - 1.0) * (ROOM_SIZE_MAX_SECONDS - ROOM_SIZE_MIN_SECONDS))
            / ROOM_SIZE_REFERENCE_RANGE;
    math::max(seconds, ROOM_SIZE_MIN_SECONDS)
}

/// `buildImpulseResponse`: exponentially decaying noise, two channels drawn one after the other
/// from `random`.
pub fn impulse_response(sample_rate: f64, seconds: f64, random: &mut SeededRandom) -> Buffer {
    let length = math::max(1.0, math::round(sample_rate * seconds)) as usize;
    let decay_rate = -math::ln(IR_DECAY_FLOOR);
    let channels = (0..IR_CHANNELS)
        .map(|_| {
            (0..length)
                .map(|i| {
                    let envelope = math::exp(-decay_rate * (i as f64 / length as f64));
                    ((random.next_f64() * 2.0 - 1.0) * envelope) as f32
                })
                .collect()
        })
        .collect();
    Buffer {
        sample_rate,
        channels,
    }
}

/// One orbit's buses.
#[derive(Debug, Clone)]
pub struct OrbitBus {
    orbit: f64,
    /// Where a voice's reverb send goes.
    pub reverb_input: NodeId,
    /// Where a voice's delay send goes.
    pub delay_input: NodeId,
    convolver: NodeId,
    delay: NodeId,
    feedback: NodeId,
    last_room_size: Option<f64>,
    last_delay_time: Option<f64>,
    last_delay_feedback: Option<f64>,
}

impl OrbitBus {
    /// `createOrbitBus`: a gain into a convolver, and a gain into a delay with a feedback gain
    /// around it and an output gain, both into `output`.
    pub fn new<G: Graph>(graph: &mut G, output: NodeId, orbit: f64) -> Self {
        let reverb_input = graph.node(NodeKind::Gain);
        let convolver = graph.node(NodeKind::Convolver { normalize: true });
        graph.connect(reverb_input, 0, Input::node(convolver));
        graph.connect(convolver, 0, Input::node(output));
        let delay_input = graph.node(NodeKind::Gain);
        let delay = graph.node(NodeKind::Delay {
            max_seconds: DELAY_MAX_SECONDS,
        });
        let delay_output = graph.node(NodeKind::Gain);
        let feedback = graph.node(NodeKind::Gain);
        graph.connect(delay_input, 0, Input::node(delay));
        graph.connect(delay, 0, Input::node(delay_output));
        graph.connect(delay_output, 0, Input::node(output));
        graph.connect(delay, 0, Input::node(feedback));
        graph.connect(feedback, 0, Input::node(delay));
        OrbitBus {
            orbit,
            reverb_input,
            delay_input,
            convolver,
            delay,
            feedback,
            last_room_size: None,
            last_delay_time: None,
            last_delay_feedback: None,
        }
    }

    /// `setRoomSize`: a new impulse response, keyed `ir|<orbit>|<size>`, unless the size is the one
    /// the orbit already has.
    pub fn set_room_size<G: Graph>(&mut self, graph: &mut G, seeds: &Seeds, size: f64) {
        // `===`: NaN never equals itself, so a NaN size rebuilds every time, as in the TypeScript.
        if self.last_room_size.is_some_and(|last| last == size) {
            return;
        }
        self.last_room_size = Some(size);
        let key = format!(
            "ir|{}|{}",
            number_to_string(self.orbit),
            number_to_string(size)
        );
        let mut random = seeds.keyed_random(&key);
        let buffer = impulse_response(graph.sample_rate(), room_size_to_seconds(size), &mut random);
        let buffer = graph.buffer(buffer);
        graph.set_buffer(self.convolver, buffer);
    }

    /// `setDelay`: the echo time and feedback, each set only when it changes.
    pub fn set_delay<G: Graph>(&mut self, graph: &mut G, time: f64, feedback: f64, when: f64) {
        if self.last_delay_time.is_none_or(|last| last != time) {
            self.last_delay_time = Some(time);
            graph.automate(
                ParamId::new(self.delay, Param::DelayTime),
                Automation::Set {
                    value: time,
                    at: when,
                },
            );
        }
        if self.last_delay_feedback.is_none_or(|last| last != feedback) {
            self.last_delay_feedback = Some(feedback);
            graph.automate(
                ParamId::new(self.feedback, Param::Gain),
                Automation::Set {
                    value: feedback,
                    at: when,
                },
            );
        }
    }
}

/// Every orbit's bus on one output, made on first use (`getOrbitBus`).
#[derive(Debug, Clone, Default)]
pub struct OrbitBuses {
    buses: Vec<(NodeId, f64, OrbitBus)>,
}

impl OrbitBuses {
    /// The bus for `orbit` into `output`, made now if no voice has used it yet. Orbits are told
    /// apart as a JavaScript `Map` tells numbers apart, so `0` and `-0` are one orbit.
    pub fn get<G: Graph>(&mut self, graph: &mut G, orbit: f64, output: NodeId) -> &mut OrbitBus {
        let same = |a: f64, b: f64| a == b || (a.is_nan() && b.is_nan());
        let index = match self
            .buses
            .iter()
            .position(|(out, o, _)| *out == output && same(*o, orbit))
        {
            Some(index) => index,
            None => {
                let bus = OrbitBus::new(graph, output, orbit);
                self.buses.push((output, orbit, bus));
                self.buses.len() - 1
            }
        };
        &mut self.buses[index].2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::LogGraph;
    use crate::random::seeded_random;

    #[test]
    fn room_sizes_map_to_seconds_with_a_floor() {
        assert_eq!(room_size_to_seconds(1.0), 0.3);
        // 0.3 + 9 × 3.7 / 9 in doubles, as the TypeScript computes it.
        assert_eq!(room_size_to_seconds(10.0), 4.000000000000001);
        assert_eq!(room_size_to_seconds(-5.0), 0.3);
    }

    #[test]
    fn an_impulse_response_decays_to_a_thousandth() {
        let mut random = seeded_random("0|ir|0|3");
        let ir = impulse_response(48000.0, 1.0, &mut random);
        assert_eq!(ir.channels.len(), 2);
        assert_eq!(ir.len(), 48000);
        assert!(ir.channels[0][47999].abs() <= 0.001 * 1.01);
    }

    #[test]
    fn a_bus_is_made_once_and_its_room_rebuilt_only_on_a_new_size() {
        let mut graph = LogGraph::new(2, 10, 48000.0);
        let seeds = Seeds::new(0.0);
        let mut buses = OrbitBuses::default();
        let out = graph.destination();
        buses
            .get(&mut graph, 0.0, out)
            .set_room_size(&mut graph, &seeds, 3.0);
        buses
            .get(&mut graph, -0.0, out)
            .set_room_size(&mut graph, &seeds, 3.0);
        assert_eq!(graph.node_count(), 1 + 6);
        assert_eq!(graph.buffers().len(), 1);
        buses
            .get(&mut graph, 0.0, out)
            .set_room_size(&mut graph, &seeds, 4.0);
        assert_eq!(graph.buffers().len(), 2);
    }
}
