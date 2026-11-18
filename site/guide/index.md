# Guide

Undertone Sound is a workspace of Rust crates on two tracks. The first, the Undertone engine in Rust, leads. The second, synth backends and DOS, extends the engine to sound chips.

## Track 1: the Undertone engine

| Crate | What it does |
|---|---|
| [`undertone-mini`](/guide/undertone-mini) | Parses Undertone's mini-notation into patterns over exact fractions |
| [`undertone-dsp`](/guide/undertone-dsp) | Renders Undertone's voices and Web Audio graph to samples |
| `undertone-wasm` | Binds the crates for JavaScript: OPX tools, an OPL2 and OPL3 player and a streaming Undertone score player. It runs the [demo](/demo), which plays Title Theme three ways |

## Track 2: synth backends, OPX and DOS

| Crate | What it does |
|---|---|
| [`oplstream`](/guide/oplstream) | Reads, writes, steps, validates and converts OPX streams of OPL register writes |
| [`undertone-sound`](/guide/undertone-sound) | The song document, its validation and formatting, and the compiler to engine-neutral events |
| `undertone-opl` | OPL2 and OPL3 sequencing and channel allocation, with the host's profile |

## Using the crates

Nothing is published yet, so use a path or git dependency on a checkout:

```toml
[dependencies]
oplstream = { git = "https://github.com/liminal-hq-labs/undertone-sound" }
```

Each guide page shows a real snippet. They live in an example in each crate (`examples/*_guide.rs`), which `cargo test` and `cargo clippy --all-targets` compile, so they cannot drift from the API. Run one with `cargo run -p oplstream --example oplstream_guide`.

## Reading further

- [OPX specification](/opx) is the format, and `oplstream` is its reference implementation.
- [Behaviour](/spec) lists what the project promises.
- [Rust API](/api) has the full reference for every public item.
