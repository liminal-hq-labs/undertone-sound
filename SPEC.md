# Undertone Sound specification

## Purpose

Shared Rust sound core for Undertone.

## Users and scenarios

- A game or tool that plays OPL2, OPL3 or PC speaker music and effects from a timer interrupt, reading OPX with `oplstream`.
- A composer's tool that lowers songs to register writes and needs one exact, steppable, loopable stream.
- A browser or native app that renders the same song through a software renderer, from the same grammar.
- A converter between IMF, DRO, VGM and OPX.

## Non-goals

- Fizz-specific concepts. They belong to a separate layer.
- Capturing chip output. OPX is not a capture format and logs no other chips.
- Describing instruments or notes in OPX: it holds the register writes a tool has already lowered.

## Components

| Crate | Role | State |
|---|---|---|
| `oplstream` | The OPX reader, writer, stepper, validator, text twin and importers. `no_std` core | Header only |
| `undertone-opl` | OPL2 and OPL3 sequencing and channel allocation | Empty shell |
| `undertone-dsp` | Voices and node graph, native and WebAssembly | Empty shell |
| `undertone-sound` | Song model and compiler | Empty shell |

## Behaviour

OPX behaviour is specified in [docs/OPX.md](docs/OPX.md), which is canonical for the format.

## Interfaces

- The OPX file format, version 0.1, a draft: [docs/OPX.md](docs/OPX.md).
- `oplstream::Header`: parse and write of the 32-byte header.

## Settings and defaults

None yet.

## Security and privacy

Readers treat input as untrusted: every length and offset is checked against the buffer, and no `unsafe` code is allowed in the workspace. Nothing touches the network or the file system except the importers and tools, which read the paths they are given.

## Limits

The header declares the stream length, the grid and the tick count; readers refuse a header that disagrees with the bytes. Limits for the stepper and importers are listed in [docs/OPX.md](docs/OPX.md) as they land.
