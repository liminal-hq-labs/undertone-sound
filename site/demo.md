---
aside: false
---

# Demo

The `undertone-wasm` crate runs in this page. It plays Title Theme three ways so you can compare engines, imports any song you drop in as OPX with `oplstream`, validates it, shows its header and text twin, and plays it. Everything happens in your browser. Nothing is uploaded.

<ClientOnly>
	<Demo />
</ClientOnly>

## What you are looking at

- **Undertone engine** is the original, software-synthesised version. It is the recorded `title-theme` score from `fixtures/undertone/graph.json`, the same one the Rust tests check against Undertone's TypeScript. The page books it with `undertone-dsp` at 112 ticks a second and streams it through the engine as stereo through the master bus. The recording holds the first two bars (46 events, six voices, 3.43 s) of the studio's eight-bar arrangement, not the whole song.
- **OPL2 chip** and **OPL3 chip** are the DOS arrangement (`title-theme-dos`, 36 bars at 140 bpm) lowered to register writes by `undertone-opl` under the game's profile, written as OPX, and played through [`oplon`](https://crates.io/crates/oplon). Each is 34,560 ticks on a 560 Hz grid, 61.7 s. The OPL3 file uses the second register bank and both outputs.
- **Different arrangements.** The Undertone version and the chip versions are not the same notes: rich software voices on one side, DOS chip music on the other. Only the two chip files share a timeline, so switching between them while one plays carries on from the same moment. The Undertone version starts from its beginning.
- **Stats** are read from each stream and from the rendered samples: the peak is the loudest sample, and the left and right peaks show whether a version is stereo. The OPL2 has one output, so its channels are equal. The OPL3 file puts every note on both outputs at the same level, so its channels are equal in level too.
- **Binary header** is the 32-byte OPX header, field by field. **Text twin** is the same stream as text, which assembles back to the same bytes. Both are available for the OPL2 file, the OPL3 file and your own.
- **Your own file** can be IMF (Type 0 or Type 1), DRO, VGM or OPX. IMF files do not say how fast they play, so set the rate they were written for.
