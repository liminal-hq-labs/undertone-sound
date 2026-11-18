# undertone-mini

`undertone-mini` is Undertone's mini-notation in Rust: a string such as `c3 [e3 g3]*2 <a3 b3> x(3,8)` becomes a pattern that is queried over spans of cycles. Time is an exact fraction, so triplets never drift. The port matches Undertone's TypeScript library, error messages and positions included, and is tested against the recorded cases in `fixtures/mini/`.

The crate has no dependencies. The caller supplies the rule for what a word means (a pitch, a drum name), so the parser stays independent of any host.

## Parse and query

<<< @/../crates/undertone-mini/examples/mini_guide.rs#parse{rust}

## Euclidean rhythms

<<< @/../crates/undertone-mini/examples/mini_guide.rs#euclid{rust}

## Errors

<<< @/../crates/undertone-mini/examples/mini_guide.rs#errors{rust}

## JavaScript semantics

The `js` module holds JavaScript's number and string behaviours (`String(n)`, `toFixed(2)`, `Math.round`, `trim`, UTF-16 lengths and `JSON.stringify` quoting), so Rust output matches the TypeScript fixtures byte for byte (see [decisions](https://github.com/liminal-hq-labs/undertone-sound/blob/main/docs/decisions.md) D12).

The [Rust API](/api/undertone_mini/index.html) lists every public item.
