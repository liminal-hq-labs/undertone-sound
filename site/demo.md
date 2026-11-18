---
aside: false
---

# Demo

The `undertone-wasm` crate runs in this page. It imports a song as OPX with `oplstream`, validates it, shows its header and text twin, and plays it through [`oplon`](https://crates.io/crates/oplon), an OPL2 and OPL3 emulator. Everything happens in your browser. Nothing is uploaded.

<ClientOnly>
	<Demo />
</ClientOnly>

## What you are looking at

- **Title Theme** is song 0 of Fizz 1's DOS sound bundle, an IMF Type-1 chunk written for a 560 Hz timer. Importing it is exact: the 3,312 records become 3,312 register writes on a 34,560 tick grid.
- **Binary header** is the 32-byte OPX header, field by field. **Text twin** is the same stream as text, which assembles back to the same bytes.
- **Play** runs the stream through the emulator, a block at a time, then plays the result. The peak is the loudest sample rendered.
- **Your own file** can be IMF (Type 0 or Type 1), DRO, VGM or OPX. IMF files do not say how fast they play, so set the rate they were written for.
