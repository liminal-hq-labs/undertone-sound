# undertone-sound

`undertone-sound` is the song document: its types, its validation, its stable formatting, pitch and pan, and a compiler that lowers a song to timed, engine-neutral events. It makes no sound. An engine crate (`undertone-opl` for the chips, `undertone-dsp` through a host) turns the events into audio. The crate depends on `undertone-mini` for pattern takes and on nothing else here.

A song file has the `format` `undertone-sound/song` and a `version`, which is 1. The files in `fixtures/song/` are written by this repository, and the tests read them back byte for byte.

## Read and validate

<<< @/../crates/undertone-sound/examples/sound_guide.rs#read{rust}

## Authors

A take or a message says who wrote it with an author object: `{ "kind": "human" }`, `{ "kind": "agent" }` or `{ "kind": "agent", "name": "Claude" }`. Only an agent has a name, which is free text.

<<< @/../crates/undertone-sound/examples/sound_guide.rs#author{rust}

## Compile

<<< @/../crates/undertone-sound/examples/sound_guide.rs#compile{rust}

## Format

<<< @/../crates/undertone-sound/examples/sound_guide.rs#format{rust}

## Presets and host sounds

An instrument names one of ten presets: `lead`, `square-lead`, `soft-lead`, `pluck`, `bell`, `pad`, `organ`, `bass`, `sub-bass` and `kit`. A sound of a host's own is not a preset of the format. The host supplies it as data for the engine, such as `OplProfile::with_patch` in `undertone-opl`, and the song keeps saying `bass` or `pad` (see [decisions](/decisions) D51).

## TypeScript types

With the `ts` cargo feature, `cargo test -p undertone-sound --features ts` writes one `.ts` file for each wire type (`Song.ts`, `Take.ts`, `Author.ts` and so on), each importing the files it uses, into the crate's `bindings/` directory. There is no index file. Set `TS_RS_EXPORT_DIR` to write them somewhere else, for example `TS_RS_EXPORT_DIR=/path/to/types cargo test -p undertone-sound --features ts`. A relative path is taken from the crate's directory. The crate's [README](https://github.com/liminal-hq-labs/undertone-sound/blob/main/crates/undertone-sound/README.md) has the details.

The [Rust API](/api/undertone_sound/index.html) lists every public item.
