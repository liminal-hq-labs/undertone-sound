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
| `oplstream` | The OPX reader, writer, stepper, validator, text twin and importers. `no_std` core | Working: header, decoder and stepper, validator, text twin, `alloc` writer and `std` importers and re-timing. Not yet run against real files or on hardware |
| `undertone-opl` | OPL2 and OPL3 register sequencing and channel allocation, lost-note accounting, the PC speaker engine and an `oplon` renderer, under a host-supplied profile | Carved from the studio; matches the recorded `fixtures/chips/` register writes, speaker frames, lost notes and rendered samples. Compiles for `wasm32-unknown-unknown` |
| `undertone-mini` | Undertone's mini-notation: parser, pattern combinators and exact fractions, plus JavaScript number semantics (`js`) | Carved from the studio; passes the recorded `fixtures/mini/` cases |
| `undertone-dsp` | Voices, the Web Audio graph Undertone books, and an in-house renderer, offline or streaming, native and WebAssembly | Carved from the studio; `LogGraph` matches the recorded `fixtures/undertone/graph.json` byte for byte and the engine streams in any block size. Compiles for `wasm32-unknown-unknown`; no bindings yet |
| `undertone-sound` | The song document: model, validation, stable formatting, pitch and pan, and the compiler to timed, engine-neutral events. Optional `ts` feature for TypeScript bindings | Carved from the studio; passes the recorded `fixtures/song/` cases for validation, formatting, pitch and scales, and compilation. No sound is made here |

## Behaviour

The song document is specified by its types, `validate_song` and the recorded cases in `fixtures/song/`, which the TypeScript implementation wrote. `compile_song` repeats that implementation's floating-point operations in the same order, so a performance matches bit for bit.

OPX behaviour is specified in [docs/OPX.md](docs/OPX.md), which is canonical for the format.

## Interfaces

- `undertone-sound`: `model` (`Song` and its parts, `SONG_FORMAT`, `SONG_VERSION`), `validate::validate_song` (a JSON value in, a `Validation` with the song and every problem by path out), `format::format_song` and `format_document`, `music`, `pan`, `takes::take_notes` and `compile::compile_song`, which lowers a song to a `Performance`.
- `undertone-opl`: `sequence::sequence_opl` (a `Performance` to timed register writes, with `dropped_events` and `cut_events`), `limits::analyse_limits`, `engine::render_song` and `render_track_in_mix`, `speaker::sequence::sequence_speaker`, and `profile::OplProfile`.
- The OPX file format, version 0.1, a draft: [docs/OPX.md](docs/OPX.md).
- `oplstream` (`no_std` core): `Header`, `Decoder` and `Stepper` over the command stream, `validate`, and `print_twin`.
- `oplstream` with `alloc`: `Writer` and the twin `assemble`.
- `oplstream` with `std`: `import_imf`, `import_dro`, `import_vgm` and the explicit, reported `retime`.

## Settings and defaults

- `undertone-opl` takes an `OplProfile` from the host: the channels the music must never touch (`reserve`) and the patches of presets the crate does not hold (`with_patch`). The default reserves nothing and holds the patches of ten presets; `bass-saw`, `pad-bright` and `drone` have none here and play as `lead` until the host supplies them ([decisions.md](docs/decisions.md), D35).
- `undertone-sound` cargo feature `ts` (off by default): derives `ts-rs` bindings for the wire types, written to `bindings/` by `cargo test -p undertone-sound --features ts`.
- The song format id is `fizz-audio-studio/song`, version 1, kept byte for byte until a version gate changes it ([decisions.md](docs/decisions.md), D31).

## Security and privacy

Readers treat input as untrusted: every length and offset is checked against the buffer, and no `unsafe` code is allowed in the workspace. Nothing touches the network or the file system except the importers and tools, which read the paths they are given.

## Limits

The header declares the stream length, the grid and the tick count; readers refuse a header that disagrees with the bytes. Limits for the stepper and importers are listed in [docs/OPX.md](docs/OPX.md) as they land.
