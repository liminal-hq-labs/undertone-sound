// A Graph that renders nothing and writes down every call, as the TypeScript recorder does
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `LogGraph` is the Rust side of `tools/fixtures/undertone/recorder.ts`. Its `describe()` is the
//! `graph` of a case in `fixtures/undertone/graph.json`, field for field: nodes in the order made
//! with what differs from Web Audio's defaults, every AudioParam call as one line, connections,
//! start and stop times, and a summary of every buffer and periodic wave. Numbers are rounded to
//! nine significant digits as the recorder's `round` does, so a port whose arithmetic matches the
//! TypeScript's writes the same text.

use fizz_audio_song::js::number_to_string;

use crate::graph::{
    Automation, Buffer, BufferId, Graph, Input, NodeId, NodeKind, Param, ParamId, Wave, WaveId,
};
use crate::json::Json;

/// Significant digits kept for every number written.
const DIGITS: usize = 9;

/// The recorder's `round`: nine significant digits, as `Number(value.toPrecision(9))`, and no
/// negative zero. Rust rounds an exact decimal tie to even where `toPrecision` rounds it up; a tie
/// needs a double whose exact value ends in a 5 at the tenth digit, which the voices never make.
pub fn round(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let rounded: f64 = format!("{value:.prec$e}", prec = DIGITS - 1)
        .parse()
        .expect("a formatted number reads back");
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

/// A number as every call and buffer summary writes it.
pub fn num(value: f64) -> String {
    number_to_string(round(value))
}

#[derive(Debug, Clone)]
struct LogNode {
    kind: &'static str,
    /// Fields fixed at creation that the recorder writes as they are (`maxDelayTime`, `numberOfInputs`).
    extra: Vec<(&'static str, f64)>,
    type_name: Option<&'static str>,
    wave: Option<WaveId>,
    buffer: Option<BufferId>,
    looped: bool,
    clamped_max: bool,
    normalize: bool,
    params: Vec<(Param, Vec<String>)>,
    to: Vec<String>,
    start: Vec<f64>,
    stop: Vec<f64>,
}

impl LogNode {
    fn new(kind: &'static str) -> Self {
        LogNode {
            kind,
            extra: Vec::new(),
            type_name: None,
            wave: None,
            buffer: None,
            looped: false,
            clamped_max: false,
            normalize: true,
            params: Vec::new(),
            to: Vec::new(),
            start: Vec::new(),
            stop: Vec::new(),
        }
    }

    fn calls(&mut self, param: Param) -> &mut Vec<String> {
        let index = match self.params.iter().position(|(p, _)| *p == param) {
            Some(index) => index,
            None => {
                self.params.push((param, Vec::new()));
                self.params.len() - 1
            }
        };
        &mut self.params[index].1
    }

    fn describe(&self, id: usize) -> Json {
        let mut out: Vec<(String, Json)> = vec![
            ("id".into(), Json::text(format!("n{id}"))),
            ("kind".into(), Json::text(self.kind)),
        ];
        for (field, value) in &self.extra {
            out.push(((*field).into(), Json::Number(*value)));
        }
        if let Some(type_name) = self.type_name {
            out.push(("type".into(), Json::text(type_name)));
        }
        if let Some(wave) = self.wave {
            out.push(("wave".into(), Json::text(format!("w{}", wave.0))));
        }
        if let Some(buffer) = self.buffer {
            out.push(("buffer".into(), Json::text(format!("b{}", buffer.0))));
        }
        if self.looped {
            out.push(("loop".into(), Json::Bool(true)));
        }
        if self.clamped_max {
            out.push(("channelCountMode".into(), Json::text("clamped-max")));
        }
        let params: Vec<(String, Json)> = self
            .params
            .iter()
            .filter(|(_, calls)| !calls.is_empty())
            .map(|(param, calls)| {
                (
                    param.name().to_string(),
                    Json::Array(calls.iter().map(Json::text).collect()),
                )
            })
            .collect();
        if !params.is_empty() {
            out.push(("params".into(), Json::Object(params)));
        }
        let times =
            |times: &[f64]| Json::Array(times.iter().map(|t| Json::Number(round(*t))).collect());
        if !self.start.is_empty() {
            out.push(("start".into(), times(&self.start)));
        }
        if !self.stop.is_empty() {
            out.push(("stop".into(), times(&self.stop)));
        }
        if !self.to.is_empty() {
            out.push((
                "to".into(),
                Json::Array(self.to.iter().map(Json::text).collect()),
            ));
        }
        if !self.normalize {
            out.push(("normalize".into(), Json::Bool(false)));
        }
        Json::Object(out)
    }
}

/// A recorded periodic wave.
#[derive(Debug, Clone)]
struct LogWave {
    real: Vec<f32>,
    imag: Vec<f32>,
    disable_normalization: bool,
}

/// A recording of every call made on it, in the shape of `fixtures/undertone/graph.json`.
#[derive(Debug, Clone)]
pub struct LogGraph {
    sample_rate: f64,
    length: usize,
    channels: u32,
    nodes: Vec<LogNode>,
    buffers: Vec<Buffer>,
    waves: Vec<LogWave>,
}

impl LogGraph {
    /// An empty recording context of `length` frames, as `new OfflineAudioContext(...)`.
    pub fn new(channels: u32, length: usize, sample_rate: f64) -> Self {
        LogGraph {
            sample_rate,
            length,
            channels,
            nodes: vec![LogNode::new("destination")],
            buffers: Vec::new(),
            waves: Vec::new(),
        }
    }

    /// Frames the context renders.
    pub fn length(&self) -> usize {
        self.length
    }

    /// Output channels.
    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// Nodes made so far, the destination included.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The buffers made so far, in order.
    pub fn buffers(&self) -> &[Buffer] {
        &self.buffers
    }

    /// Everything recorded, as the fixture's `graph`: `sampleRate`, `length`, `channels`, `counts`
    /// (nodes by kind), `nodes`, `buffers` and `waves`.
    pub fn describe(&self) -> Json {
        let mut counts: Vec<(String, Json)> = Vec::new();
        for node in &self.nodes {
            match counts.iter_mut().find(|(kind, _)| kind == node.kind) {
                Some((_, Json::Number(n))) => *n += 1.0,
                _ => counts.push((node.kind.to_string(), Json::Number(1.0))),
            }
        }
        Json::object([
            ("sampleRate", Json::Number(self.sample_rate)),
            ("length", Json::Number(self.length as f64)),
            ("channels", Json::Number(f64::from(self.channels))),
            ("counts", Json::Object(counts)),
            (
                "nodes",
                Json::Array(
                    self.nodes
                        .iter()
                        .enumerate()
                        .map(|(id, node)| node.describe(id))
                        .collect(),
                ),
            ),
            (
                "buffers",
                Json::Array(
                    self.buffers
                        .iter()
                        .enumerate()
                        .map(|(id, buffer)| describe_buffer(id, buffer))
                        .collect(),
                ),
            ),
            (
                "waves",
                Json::Array(
                    self.waves
                        .iter()
                        .enumerate()
                        .map(|(id, wave)| {
                            let line = |values: &[f32]| {
                                values
                                    .iter()
                                    .map(|v| num(f64::from(*v)))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            };
                            Json::object([
                                ("id", Json::text(format!("w{id}"))),
                                (
                                    "disableNormalization",
                                    Json::Bool(wave.disable_normalization),
                                ),
                                ("imag", Json::text(line(&wave.imag))),
                                ("real", Json::text(line(&wave.real))),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    fn node_mut(&mut self, node: NodeId) -> &mut LogNode {
        &mut self.nodes[node.0]
    }

    fn name(&self, to: Input) -> String {
        match to {
            Input::Node { node, .. } => format!("n{}", node.0),
            Input::Param(ParamId { node, param }) => format!("n{}.{}", node.0, param.name()),
        }
    }
}

/// A buffer as the recorder summarises it: per channel the root mean square, the peak and the first
/// four samples, all of the `f32` values it holds.
fn describe_buffer(id: usize, buffer: &Buffer) -> Json {
    let channels = buffer
        .channels
        .iter()
        .map(|data| {
            let mut sum = 0.0f64;
            let mut peak = 0.0f64;
            for sample in data {
                let sample = f64::from(*sample);
                sum += sample * sample;
                peak = crate::math::max(peak, sample.abs());
            }
            let rms = if data.is_empty() {
                0.0
            } else {
                (sum / data.len() as f64).sqrt()
            };
            let head: Vec<String> = data.iter().take(4).map(|s| num(f64::from(*s))).collect();
            Json::text(format!(
                "rms {} peak {} head {}",
                num(rms),
                num(peak),
                head.join(" ")
            ))
        })
        .collect();
    Json::object([
        ("id", Json::text(format!("b{id}"))),
        ("channels", Json::Array(channels)),
        ("length", Json::Number(buffer.len() as f64)),
        ("sampleRate", Json::Number(buffer.sample_rate)),
    ])
}

impl Graph for LogGraph {
    fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    fn destination(&self) -> NodeId {
        NodeId(0)
    }

    fn node(&mut self, kind: NodeKind) -> NodeId {
        let node = match kind {
            NodeKind::Oscillator(wave) => {
                let mut node = LogNode::new("oscillator");
                match wave {
                    Wave::Basic(kind) => node.type_name = Some(kind.name()),
                    Wave::Periodic(wave) => {
                        node.type_name = Some("custom");
                        node.wave = Some(wave);
                    }
                }
                node
            }
            NodeKind::BufferSource { buffer, looped } => {
                let mut node = LogNode::new("bufferSource");
                node.buffer = Some(buffer);
                node.looped = looped;
                node
            }
            NodeKind::Gain => LogNode::new("gain"),
            NodeKind::Biquad(filter) => {
                let mut node = LogNode::new("biquad");
                node.type_name = Some(filter.name());
                node
            }
            NodeKind::StereoPanner => {
                let mut node = LogNode::new("stereoPanner");
                node.clamped_max = true;
                node
            }
            NodeKind::Delay { max_seconds } => {
                let mut node = LogNode::new("delay");
                node.extra.push(("maxDelayTime", max_seconds));
                node
            }
            NodeKind::Convolver { normalize } => {
                let mut node = LogNode::new("convolver");
                node.clamped_max = true;
                node.normalize = normalize;
                node
            }
            NodeKind::Compressor => {
                let mut node = LogNode::new("compressor");
                node.clamped_max = true;
                node
            }
            NodeKind::Merger { inputs } => {
                let mut node = LogNode::new("channelMerger");
                node.extra.push(("numberOfInputs", f64::from(inputs)));
                node
            }
        };
        self.nodes.push(node);
        NodeId(self.nodes.len() - 1)
    }

    fn automate(&mut self, param: ParamId, step: Automation) {
        let line = match step {
            Automation::Set { value, at } => format!("set {} @{}", num(value), num(at)),
            Automation::Linear { value, until } => format!("lin {} @{}", num(value), num(until)),
            Automation::Exponential { value, until } => {
                format!("exp {} @{}", num(value), num(until))
            }
            Automation::Target {
                value,
                from,
                time_constant,
            } => format!(
                "target {} @{} tau {}",
                num(value),
                num(from),
                num(time_constant)
            ),
            Automation::Cancel { from } => format!("cancel @{}", num(from)),
        };
        self.node_mut(param.node).calls(param.param).push(line);
    }

    fn connect(&mut self, from: NodeId, output: u32, to: Input) {
        let input = match to {
            Input::Node { input, .. } => input,
            Input::Param(_) => 0,
        };
        let name = self.name(to);
        let entry = if output == 0 && input == 0 {
            name
        } else {
            format!("{name}:{output}>{input}")
        };
        self.node_mut(from).to.push(entry);
    }

    fn set_buffer(&mut self, convolver: NodeId, buffer: BufferId) {
        self.node_mut(convolver).buffer = Some(buffer);
    }

    fn start(&mut self, node: NodeId, at: f64) {
        self.node_mut(node).start.push(at);
    }

    fn stop(&mut self, node: NodeId, at: f64) {
        self.node_mut(node).stop.push(at);
    }

    fn buffer(&mut self, buffer: Buffer) -> BufferId {
        self.buffers.push(buffer);
        BufferId(self.buffers.len() - 1)
    }

    fn periodic_wave(
        &mut self,
        real: Vec<f32>,
        imag: Vec<f32>,
        disable_normalization: bool,
    ) -> WaveId {
        self.waves.push(LogWave {
            real,
            imag,
            disable_normalization,
        });
        WaveId(self.waves.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{FilterType, OscillatorType};

    #[test]
    fn rounds_to_nine_digits_without_negative_zero() {
        assert_eq!(num(659.2551138257398), "659.255114");
        assert_eq!(num(0.10000000000000002), "0.1");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(-1e-30), "-1e-30");
        assert_eq!(num(5.889147443e-17), "5.88914744e-17");
    }

    #[test]
    fn records_nodes_params_and_connections() {
        let mut graph = LogGraph::new(2, 100, 48000.0);
        let osc = graph.node(NodeKind::Oscillator(Wave::Basic(OscillatorType::Square)));
        let filter = graph.node(NodeKind::Biquad(FilterType::Highpass));
        graph.automate(
            ParamId::new(osc, Param::Frequency),
            Automation::Set {
                value: 440.0,
                at: 0.0,
            },
        );
        graph.connect(osc, 0, Input::node(filter));
        graph.connect(
            filter,
            0,
            Input::Node {
                node: graph.destination(),
                input: 1,
            },
        );
        graph.connect(osc, 0, Input::Param(ParamId::new(filter, Param::Frequency)));
        graph.start(osc, 0.5);
        let text = graph.describe().stable();
        assert!(text.contains("\"frequency\": [\"set 440 @0\"]"), "{text}");
        assert!(
            text.contains("\"to\": [\"n2\", \"n2.frequency\"]"),
            "{text}"
        );
        assert!(text.contains("\"to\": [\"n0:0>1\"]"), "{text}");
        assert!(text.contains("\"type\": \"highpass\""), "{text}");
        assert!(
            text.contains("\"counts\": { \"biquad\": 1, \"destination\": 1, \"oscillator\": 1 }"),
            "{text}"
        );
    }
}
