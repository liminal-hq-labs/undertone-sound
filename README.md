# Undertone Sound

<p align="center"><img src="assets/hero.svg" alt="Undertone Sound — Shared Rust sound core for Undertone" width="100%"></p>

<p align="center">
  <a href="https://github.com/liminal-hq-labs/undertone-sound/actions/workflows/ci.yml"><img src="https://github.com/liminal-hq-labs/undertone-sound/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <img src="https://img.shields.io/badge/status-experimental-a3e635" alt="Status: experimental">
  <img src="https://img.shields.io/badge/licence-Apache--2.0%20OR%20MIT-3fb950" alt="Licence: Apache-2.0 OR MIT">
</p>

Shared Rust sound core for Undertone.

Undertone Sound is the Rust side of the [Undertone](https://github.com/liminal-hq/undertone) family: one sound grammar for songs and effects, with a renderer per target, from OPL2 and OPL3 chips to the browser. It starts with `oplstream`, the reference implementation of OPX, a small, exact, loopable stream of timed OPL register writes.

> **Status:** experimental. Only the OPX header parses and writes so far, and nothing is released or published. The [roadmap](docs/ROADMAP.md) lists what comes next.

## What it's for

- Playing music and effects on an AdLib or OPL3 card, or an emulator, from a timer interrupt.
- Converting IMF, DRO and VGM captures to one exact format, and back.
- Writing a composer's tool that lowers songs to chip register writes.
- Keeping a browser renderer and a chip renderer in step from one song.

## Install and run

From source, with the pinned Rust toolchain:

```sh
cargo test --workspace
```

The crates are not published yet.

## Documentation

- [SPEC.md](SPEC.md) — behaviour and interfaces
- [docs/OPX.md](docs/OPX.md) — the OPX format
- [docs/ROADMAP.md](docs/ROADMAP.md) — what is built and what is next
- [docs/decisions.md](docs/decisions.md) — decisions and why
- [AGENTS.md](AGENTS.md) — contributor and agent rules

## Contributing

Issues and pull requests are welcome. The house rules are in [AGENTS.md](AGENTS.md); run the checks it lists before opening a pull request.

## Part of Liminal HQ Labs

Undertone Sound is a [Liminal HQ Labs](https://github.com/liminal-hq-labs) experiment. Experiments from Liminal HQ — early, rough and open. If it proves itself, it [graduates](https://github.com/liminal-hq-labs/.github/blob/main/docs/labs-charter.md#graduation) to [Liminal HQ](https://github.com/liminal-hq).

## Licence

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT licence](LICENSE-MIT), at your option.
