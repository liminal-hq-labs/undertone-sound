# Guide

Undertone Sound is a workspace of Rust crates. Three hold code today, and two are waiting for theirs.

| Crate | What it does |
|---|---|
| [`oplstream`](/guide/oplstream) | Reads, writes, steps, validates and converts OPX streams of OPL register writes |
| [`undertone-mini`](/guide/undertone-mini) | Parses Undertone's mini-notation into patterns over exact fractions |
| [`undertone-dsp`](/guide/undertone-dsp) | Renders Undertone's voices and Web Audio graph to samples |
| `undertone-opl` | Planned: OPL sequencing and channel allocation |
| `undertone-sound` | Planned: the song model and compiler |
| `undertone-wasm` | Binds the crates for JavaScript; it runs the [demo](/demo) |

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
