# oplstream

`oplstream` is the reference implementation of [OPX](/opx), a stream of timed OPL2, OPL3 and PC speaker register writes on an exact tick grid. It is one crate with three tiers, chosen by features:

| Feature | Adds |
|---|---|
| none (`no_std`, no allocation) | Header, command decoder and stepper, validator, text twin printer |
| `alloc` | The `Writer` and the text twin assembler |
| `std` (default) | IMF, DRO and VGM import, and the re-timing step |

```toml
[dependencies]
oplstream = { git = "https://github.com/liminal-hq-labs/undertone-sound", default-features = false }
```

## Import and validate

An importer turns a capture into an exact OPX file, and the validator checks it against every rule in the specification. This is Fizz 1's Title Theme, an IMF chunk written for a 560 Hz timer:

<<< @/../crates/oplstream/examples/oplstream_guide.rs#import{rust}

## Decode

The decoder yields each command with its byte offset, tick and tier. The `Stepper` runs the same stream tick by tick, without allocating, as a timer interrupt would.

<<< @/../crates/oplstream/examples/oplstream_guide.rs#decode{rust}

## The text twin

Every OPX file has a text twin, a readable form that assembles back to the same bytes. It is the form to diff and to write by hand.

<<< @/../crates/oplstream/examples/oplstream_guide.rs#twin{rust}

## Write a stream

The writer takes commands in stream order, and `finish` fills in the derived header fields. A problem with a command is remembered and reported by `finish`, so calls chain.

<<< @/../crates/oplstream/examples/oplstream_guide.rs#write{rust}

## What the importers do not do

An import never changes a wait; it only leaves things out, and says so in its `ImportReport`. To move a stream to another grid, `retime` rounds halves up and reports how many writes did not land on a tick exactly. See [decisions](https://github.com/liminal-hq-labs/undertone-sound/blob/main/docs/decisions.md) D15 and D17.

The [Rust API](/api/oplstream/index.html) lists every public item.
