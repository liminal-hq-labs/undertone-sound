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
| `undertone-opl` | OPL2 and OPL3 register sequencing and channel allocation, lost-note accounting, the PC speaker engine and an `oplon` renderer, under a host-supplied profile | Carved from the studio; matches the recorded `fixtures/chips/` register writes, speaker frames, lost notes and rendered samples. Writes OPX (`opx`) and reads notes and drum hits back out of a stream (`analyse`). The Title Theme exports for the OPL2 and OPL3 as the golden files in `fixtures/opx/`, and the OPL2 stream sounds what the DOS game's chunk sounds, with four carrier levels apart. Not played in any player or on hardware. Compiles for `wasm32-unknown-unknown` |
| `undertone-mini` | Undertone's mini-notation: parser, pattern combinators and exact fractions, plus JavaScript number semantics (`js`) | Carved from the studio; passes the recorded `fixtures/mini/` cases |
| `undertone-dsp` | Voices, the Web Audio graph Undertone books, and an in-house renderer, offline or streaming, native and WebAssembly | Carved from the studio; `LogGraph` matches the recorded `fixtures/undertone/graph.json` byte for byte and the engine streams in any block size. Compiles for `wasm32-unknown-unknown`; `undertone-wasm` renders one voice |
| `undertone-sound` | The song document: model, validation, stable formatting, pitch and pan, and the compiler to timed, engine-neutral events. Optional `ts` feature for TypeScript bindings | Carved from the studio, and its format is now its own: the recorded `fixtures/song/` cases for validation, formatting, pitch and scales, and compilation are written by this crate and checked byte for byte. No sound is made here |
| `undertone-wasm` | WebAssembly bindings: OPX import, validation, header and text twin views, register-write list, an OPL2 and OPL3 player through the `oplon` emulator (stereo), one `undertone-dsp` voice, and a recorded Undertone score streamed through `undertone-dsp`'s engine | Working; native tests cover its logic and the demo page drives it in a browser. Not published |

## Behaviour

The song document is specified by its types, `validate_song` and the recorded cases in `fixtures/song/`, which this repository writes (`UPDATE_FIXTURES=1 cargo test -p undertone-sound -- --test-threads=1`) and its tests require byte for byte. `compile_song` repeats Undertone's TypeScript floating-point operations in the same order, so a performance matches bit for bit.

A song's `by` fields (on a take and on a message) are authors: `{ "kind": "human" }`, `{ "kind": "agent" }` or `{ "kind": "agent", "name": "Claude" }`. The kind is `human` or `agent`; only an agent has a name, which is non-empty free text. A host maps its own words onto the two kinds.

An instrument's `preset` is one of ten: `lead`, `square-lead`, `soft-lead`, `pluck`, `bell`, `pad`, `organ`, `bass`, `sub-bass` and `kit`. A host that wants a sound of its own supplies it as data for an engine (on the chips, `OplProfile::with_patch` replaces the patch a preset plays); the song says nothing of it.

OPX behaviour is specified in [docs/OPX.md](docs/OPX.md), which is canonical for the format.

## Interfaces

- `undertone-sound`: `model` (`Song` and its parts, `SONG_FORMAT`, `SONG_VERSION`), `validate::validate_song` (a JSON value in, a `Validation` with the song and every problem by path out), `format::format_song` and `format_document`, `music`, `pan`, `takes::take_notes` and `compile::compile_song`, which lowers a song to a `Performance`.
- `undertone-opl`: `sequence::sequence_opl` (a `Performance` to timed register writes, with `dropped_events` and `cut_events`), `limits::analyse_limits`, `engine::render_song` and `render_track_in_mix`, `speaker::sequence::sequence_speaker`, and `profile::OplProfile`, `opx::export_opx` (an `OplSequence` to an OPX file under `OpxOptions`) and `analyse::decode_opx` (the notes and drum hits a stream sounds).
- The OPX file format, version 0.1, a draft: [docs/OPX.md](docs/OPX.md).
- `oplstream` (`no_std` core): `Header`, `Decoder` and `Stepper` over the command stream, `validate`, and `print_twin`.
- `oplstream` with `alloc`: `Writer` and the twin `assemble`.
- `oplstream` with `std`: `import_imf`, `import_dro`, `import_vgm` and the explicit, reported `retime`.
- `oplstream` errors (`HeaderError`, `DecodeError`, `ValidationError`) implement `Display` with one readable sentence each.
- `undertone-wasm` (JavaScript, after `wasm-bindgen --target web`): `detectFormat`, `importToOpx`, `summarize`, `headerTable`, `textTwin`, `registerWrites`, `renderVoice`, the `Player` class (`render`, `ended`, `totalFrames`, `setLooping`) and the `ScorePlayer` class (`new ScorePlayer(scoreJson, caseId, ticksPerSecond, sampleRate)`, then `render`, `totalFrames`, `passSeconds`, `events`, `voices`, `bars`). A failure is a thrown string with a readable message. `Player` places tick `n` on output frame `n * sample_rate / grid_hz`, rounded down, and plays OPL2 and OPL3 streams; a PC speaker stream is refused. An OPL3 stream switches the chip to OPL3 mode, writes bank 1 to the second register bank and renders left and right from the chip's two outputs; an OPL2 stream renders the same signal on both channels. `ScorePlayer` reads the text of a recorded score file (`fixtures/undertone/scores/title-theme.score.json`, case `title-theme-full`, or a case of `fixtures/undertone/graph.json`), takes the case's recorded score (events, voices and seconds) at the tick rate the caller names (112 a second for both Title Theme cases), books it with `undertone-dsp`'s `book_score_render` at the export's settings (stereo, seed 0, master bus, two seconds of tail) and streams it through `Engine`, so its blocks are the same bits for any block size.

## Settings and defaults

- `undertone-opl` takes an `OplProfile` from the host: the channels the music must never touch (`reserve`) and the patches of presets the crate does not hold (`with_patch`). `export_opx` never writes registers 0x104 and 0x105, which belong to the host's audio service, and sets the header's `reserved_channels` from the profile, limited to the chip's own channels. The default reserves nothing and holds a patch for each of the ten presets; a host replaces the patch of the preset a sound belongs to ([decisions.md](docs/decisions.md), D35 and D51).
- `undertone-sound` cargo feature `ts` (off by default): derives `ts-rs` bindings for the wire types, one file per type and no index, written to `bindings/` by `cargo test -p undertone-sound --features ts`, or to the directory `TS_RS_EXPORT_DIR` names ([decisions.md](docs/decisions.md), D53).
- The song format id is `undertone-sound/song`, version 1 ([decisions.md](docs/decisions.md), D50). The engine ids `undertone`, `opl2`, `opl3` and `pc-speaker` are kept as they are until a backend interface owns them.

## Security and privacy

Readers treat input as untrusted: every length and offset is checked against the buffer, and no `unsafe` code is allowed in the workspace. Nothing touches the network or the file system except the importers and tools, which read the paths they are given.

## Limits

The header declares the stream length, the grid and the tick count; readers refuse a header that disagrees with the bytes. Limits for the stepper and importers are listed in [docs/OPX.md](docs/OPX.md) as they land.
