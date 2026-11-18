// The graphs the in-house renderer is compared on against web-audio-api, built through `Graph`
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Each case builds one small graph through the `Graph` trait, so the in-house `RenderGraph` and
//! the reference renderer outside the workspace (`tools/undertone-reference`, over the
//! `web-audio-api` crate) render exactly the same calls. The song cases are booked with
//! `render::book` on either. `tests/reference.rs` reads what the reference wrote and compares;
//! the comparison and its tolerances are in `docs/research/UNDERTONE-HEADLESS.md`.

use crate::effects::impulse_response;
use crate::graph::{
    Automation, FilterType, Graph, Input, NodeId, NodeKind, OscillatorType, Param, ParamId, Wave,
};
use crate::noise::noise_buffer;
use crate::params::SoundType;
use crate::random::seeded_random;
use crate::render::RenderRequest;
use crate::waves::shaped_wave_table;

/// What builds a case's graph.
pub type Build = Box<dyn Fn(&mut dyn Graph)>;

/// A graph to render on both renderers.
pub struct Case {
    pub name: String,
    pub channels: u32,
    pub seconds: f64,
    pub build: Build,
}

/// The sample rates every case renders at.
pub const RATES: [f64; 2] = [44_100.0, 48_000.0];

/// Songs booked as the export books them: a name and the path from the repository's root.
pub const SONGS: [(&str, &str); 5] = [
    (
        "song-voices",
        "fixtures/undertone/inputs/undertone-voices.song.json",
    ),
    (
        "song-corners",
        "fixtures/undertone/inputs/undertone-corners.song.json",
    ),
    ("song-title-theme", "songs/title-theme.song.json"),
    (
        "song-colossus-showdown",
        "songs/colossus-showdown.song.json",
    ),
    ("song-first-light", "songs/first-light.song.json"),
];

/// Each song renders twice: as the export does, and without the master bus (`-dry`), so the
/// voices and effects are compared apart from the limiter.
pub fn song_requests(rate: f64) -> [(&'static str, RenderRequest); 2] {
    let export = RenderRequest {
        sample_rate: rate,
        ..RenderRequest::EXPORT
    };
    [
        ("", export),
        (
            "-dry",
            RenderRequest {
                master: false,
                ..export
            },
        ),
    ]
}

fn set(graph: &mut dyn Graph, node: NodeId, param: Param, value: f64) {
    graph.automate(
        ParamId::new(node, param),
        Automation::Set { value, at: 0.0 },
    );
}

fn gain(graph: &mut dyn Graph, level: f64) -> NodeId {
    let node = graph.node(NodeKind::Gain);
    set(graph, node, Param::Gain, level);
    node
}

fn oscillator(graph: &mut dyn Graph, kind: OscillatorType, frequency: f64) -> NodeId {
    let node = graph.node(NodeKind::Oscillator(Wave::Basic(kind)));
    set(graph, node, Param::Frequency, frequency);
    node
}

/// Seeded white noise, looped, from 0 to `until`.
fn noise(graph: &mut dyn Graph, key: &str, seconds: f64, until: f64) -> NodeId {
    let mut random = seeded_random(key);
    let buffer = noise_buffer(graph.sample_rate(), SoundType::White, seconds, &mut random);
    let buffer = graph.buffer(buffer);
    let node = graph.node(NodeKind::BufferSource {
        buffer,
        looped: true,
    });
    graph.start(node, 0.0);
    graph.stop(node, until);
    node
}

fn out(graph: &mut dyn Graph, node: NodeId) {
    let destination = graph.destination();
    graph.connect(node, 0, Input::node(destination));
}

fn case(name: &str, channels: u32, seconds: f64, build: impl Fn(&mut dyn Graph) + 'static) -> Case {
    Case {
        name: name.to_string(),
        channels,
        seconds,
        build: Box::new(build),
    }
}

/// The node cases, one or a few nodes each.
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for kind in [
        OscillatorType::Sine,
        OscillatorType::Square,
        OscillatorType::Sawtooth,
        OscillatorType::Triangle,
    ] {
        for frequency in [110.0, 440.0, 1760.0, 5000.0] {
            cases.push(case(
                &format!("osc-{}-{frequency}", kind.name()),
                1,
                1.0,
                move |g| {
                    let osc = oscillator(g, kind, frequency);
                    let level = gain(g, 0.5);
                    g.connect(osc, 0, Input::node(level));
                    out(g, level);
                    g.start(osc, 0.0);
                },
            ));
        }
    }
    cases.push(case("osc-sweep-sawtooth", 1, 2.0, |g| {
        let osc = oscillator(g, OscillatorType::Sawtooth, 50.0);
        g.automate(
            ParamId::new(osc, Param::Frequency),
            Automation::Exponential {
                value: 8000.0,
                until: 2.0,
            },
        );
        let level = gain(g, 0.5);
        g.connect(osc, 0, Input::node(level));
        out(g, level);
        g.start(osc, 0.0);
    }));
    for (name, shape) in [
        ("osc-halfsine", SoundType::HalfSine),
        ("osc-pulsesine", SoundType::PulseSine),
    ] {
        cases.push(case(name, 1, 1.0, move |g| {
            let (real, imag) = shaped_wave_table(shape);
            let wave = g.periodic_wave(real, imag, false);
            let osc = g.node(NodeKind::Oscillator(Wave::Periodic(wave)));
            set(g, osc, Param::Frequency, 220.0);
            out(g, osc);
            g.start(osc, 0.0);
        }));
    }
    cases.push(case("fm", 1, 1.0, |g| {
        let carrier = oscillator(g, OscillatorType::Sine, 440.0);
        let modulator = oscillator(g, OscillatorType::Sine, 880.0);
        let depth = gain(g, 1500.0);
        g.connect(modulator, 0, Input::node(depth));
        g.connect(
            depth,
            0,
            Input::Param(ParamId::new(carrier, Param::Frequency)),
        );
        let level = gain(g, 0.5);
        g.connect(carrier, 0, Input::node(level));
        out(g, level);
        g.start(carrier, 0.0);
        g.start(modulator, 0.0);
    }));
    for (name, kind, cutoff) in [
        ("lowpass-noise", FilterType::Lowpass, 1000.0),
        ("highpass-noise", FilterType::Highpass, 2000.0),
    ] {
        cases.push(case(name, 1, 1.0, move |g| {
            let source = noise(g, "0|reference|noise", 0.5, 1.0);
            let filter = g.node(NodeKind::Biquad(kind));
            set(g, filter, Param::Frequency, cutoff);
            g.connect(source, 0, Input::node(filter));
            let level = gain(g, 0.5);
            g.connect(filter, 0, Input::node(level));
            out(g, level);
        }));
    }
    cases.push(case("lowpass-envelope", 1, 1.0, |g| {
        let osc = oscillator(g, OscillatorType::Sawtooth, 110.0);
        let filter = g.node(NodeKind::Biquad(FilterType::Lowpass));
        let frequency = ParamId::new(filter, Param::Frequency);
        g.automate(
            frequency,
            Automation::Set {
                value: 200.0,
                at: 0.0,
            },
        );
        g.automate(
            frequency,
            Automation::Linear {
                value: 5000.0,
                until: 0.3,
            },
        );
        g.automate(
            frequency,
            Automation::Linear {
                value: 400.0,
                until: 0.9,
            },
        );
        g.connect(osc, 0, Input::node(filter));
        let level = gain(g, 0.5);
        g.connect(filter, 0, Input::node(level));
        out(g, level);
        g.start(osc, 0.0);
    }));
    cases.push(case("phaser", 1, 2.0, |g| {
        let source = noise(g, "0|reference|noise", 0.5, 2.0);
        let lfo = oscillator(g, OscillatorType::Sine, 0.5);
        let depth = gain(g, 600.0);
        g.connect(lfo, 0, Input::node(depth));
        let mut tail = source;
        for _ in 0..4 {
            let stage = g.node(NodeKind::Biquad(FilterType::Allpass));
            let frequency = ParamId::new(stage, Param::Frequency);
            g.automate(
                frequency,
                Automation::Set {
                    value: 1000.0,
                    at: 0.0,
                },
            );
            g.connect(depth, 0, Input::Param(frequency));
            g.connect(tail, 0, Input::node(stage));
            tail = stage;
        }
        // The dry signal plus the four stages is what makes the notches.
        let level = gain(g, 0.3);
        g.connect(source, 0, Input::node(level));
        g.connect(tail, 0, Input::node(level));
        out(g, level);
        g.start(lfo, 0.0);
    }));
    for (name, threshold, knee, ratio, attack, release) in [
        ("compressor-limiter", -2.0, 0.0, 20.0, 0.003, 0.1),
        ("compressor-default", -24.0, 30.0, 12.0, 0.003, 0.25),
    ] {
        cases.push(case(name, 2, 2.0, move |g| {
            let osc = oscillator(g, OscillatorType::Sine, 330.0);
            let envelope = g.node(NodeKind::Gain);
            let level = ParamId::new(envelope, Param::Gain);
            // Bursts at three levels, with gaps, so attack and release both show.
            for (k, peak) in [0.3, 1.6, 0.9, 2.5].iter().enumerate() {
                let at = 0.1 + k as f64 * 0.45;
                g.automate(level, Automation::Set { value: 0.0, at });
                g.automate(
                    level,
                    Automation::Linear {
                        value: *peak,
                        until: at + 0.002,
                    },
                );
                g.automate(
                    level,
                    Automation::Set {
                        value: *peak,
                        at: at + 0.25,
                    },
                );
                g.automate(
                    level,
                    Automation::Linear {
                        value: 0.0,
                        until: at + 0.3,
                    },
                );
            }
            g.connect(osc, 0, Input::node(envelope));
            let compressor = g.node(NodeKind::Compressor);
            for (param, value) in [
                (Param::Threshold, threshold),
                (Param::Knee, knee),
                (Param::Ratio, ratio),
                (Param::Attack, attack),
                (Param::Release, release),
            ] {
                set(g, compressor, param, value);
            }
            g.connect(envelope, 0, Input::node(compressor));
            out(g, compressor);
            g.start(osc, 0.0);
        }));
    }
    cases.push(case("delay-feedback", 1, 2.0, |g| {
        let source = noise(g, "0|reference|burst", 0.05, 0.05);
        let delay = g.node(NodeKind::Delay { max_seconds: 5.0 });
        set(g, delay, Param::DelayTime, 0.25);
        let feedback = gain(g, 0.5);
        g.connect(source, 0, Input::node(delay));
        g.connect(delay, 0, Input::node(feedback));
        g.connect(feedback, 0, Input::node(delay));
        out(g, source);
        out(g, delay);
    }));
    cases.push(case("convolver", 2, 3.0, |g| {
        let source = noise(g, "0|reference|burst", 0.05, 0.05);
        let mut random = seeded_random("0|ir|0|4");
        let ir = impulse_response(g.sample_rate(), 1.5, &mut random);
        let ir = g.buffer(ir);
        let convolver = g.node(NodeKind::Convolver { normalize: true });
        g.set_buffer(convolver, ir);
        g.connect(source, 0, Input::node(convolver));
        out(g, convolver);
    }));
    cases.push(case("panner-sweep", 2, 1.0, |g| {
        let osc = oscillator(g, OscillatorType::Sine, 440.0);
        let panner = g.node(NodeKind::StereoPanner);
        let pan = ParamId::new(panner, Param::Pan);
        g.automate(
            pan,
            Automation::Set {
                value: -1.0,
                at: 0.0,
            },
        );
        g.automate(
            pan,
            Automation::Linear {
                value: 1.0,
                until: 1.0,
            },
        );
        g.connect(osc, 0, Input::node(panner));
        out(g, panner);
        g.start(osc, 0.0);
    }));
    cases.push(case("merger-and-envelopes", 2, 1.0, |g| {
        let low = oscillator(g, OscillatorType::Sine, 220.0);
        let high = oscillator(g, OscillatorType::Sine, 660.0);
        let left = g.node(NodeKind::Gain);
        let l = ParamId::new(left, Param::Gain);
        g.automate(
            l,
            Automation::Set {
                value: 0.0,
                at: 0.0,
            },
        );
        g.automate(
            l,
            Automation::Linear {
                value: 0.6,
                until: 0.05,
            },
        );
        g.automate(
            l,
            Automation::Exponential {
                value: 0.01,
                until: 0.8,
            },
        );
        let right = g.node(NodeKind::Gain);
        let r = ParamId::new(right, Param::Gain);
        g.automate(
            r,
            Automation::Set {
                value: 0.4,
                at: 0.0,
            },
        );
        g.automate(
            r,
            Automation::Linear {
                value: 0.1,
                until: 1.0,
            },
        );
        g.automate(r, Automation::Cancel { from: 0.4 });
        g.automate(
            r,
            Automation::Set {
                value: 0.3,
                at: 0.4,
            },
        );
        g.automate(
            r,
            Automation::Target {
                value: 0.0,
                from: 0.4,
                time_constant: 0.05,
            },
        );
        g.connect(low, 0, Input::node(left));
        g.connect(high, 0, Input::node(right));
        let merger = g.node(NodeKind::Merger { inputs: 2 });
        g.connect(
            left,
            0,
            Input::Node {
                node: merger,
                input: 0,
            },
        );
        g.connect(
            right,
            0,
            Input::Node {
                node: merger,
                input: 1,
            },
        );
        out(g, merger);
        g.start(low, 0.0);
        g.start(high, 0.0);
    }));
    cases
}
