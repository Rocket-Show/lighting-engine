# Conformance cases

The cases the engine is compared with. They were recorded from the online designer, whose preview and Rocket Show's
engine come to the very same numbers for them; this repository holds a copy of `web/projects/designer/conformance` of
`show-designer` (branch `develop`, commit `6a4f12b`, 10 November 2026) and of the fixture profiles they use.

- `cases/*.json`: the cases of one kind in each file, one case per line: its `name`, its `input` and what the designer
  made of it (`expected`).
- `profiles/`: the fixture profiles the projects of the cases name (`"$profile": "stairville/mh-100"`).
- `core-digests.json`: a digest of the bits of every value of every frame of the `channelValues` cases, recorded from
  the native build of the core. Every other build has to come to the same bits.
- `DIFFERENCES.md`: where the core differs from the recorded values, and why.

## The kinds of cases and what runs them

| Kind                                                                             | Compared                                                                                               | Run by                                                |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------- |
| `channelValues`                                                                  | the value of every channel of every fixture and pixel at each time, for a project and how it is played | `crates/compiler/tests/conformance_channel_values.rs` |
| `transitionCurve`                                                                | the shaped position of a fade                                                                          | `crates/compiler/tests/conformance_parts.rs`          |
| `effectCurve`                                                                    | the period, the phase, the end of the run, the loop, the chase and the value at each time              | `crates/compiler/tests/conformance_parts.rs`          |
| `presetSteps`                                                                    | the step a preset is on at each time, its values, its fans and how far its effects are open            | `crates/compiler/tests/conformance_parts.rs`          |
| `presetIntensity`                                                                | how much of a preset reaches the stage                                                                 | `crates/compiler/tests/conformance_parts.rs`          |
| `pixelPattern`                                                                   | where a pattern puts a pixel in the palette, how bright it makes it and its color                      | `crates/pixel/tests/pixel_pattern.rs`                 |
| `profileNumber`, `defaultValue`, `hexColor`, `mixColors`, `pixels`, `pixelGroup` | what the fixture library means (compared exactly)                                                      | `crates/compiler/tests/conformance_model.rs`          |
| `migration`                                                                      | a project of an older version migrated                                                                 | not run: the compiler reads the current version only  |

## How the values are compared

The recorded values come from double precision, the core computes in single precision and on whole nanoseconds
(`concept.md`, sections 5 and 10).

1. **Within ±1.** A value of a `channelValues` case may differ by up to 1 of the units of its channel (1 of 255, or 1 of
   65535 for a channel with a fine channel, which is compared as one number). Which channel has a value at all has to
   match. The largest difference of all cases is printed by the test; it is below 0.01 everywhere but in the pixel
   effects.
2. **Decisions match**: the step a preset is on, the slot of a wheel, the pixel of a pattern. Where a recorded value
   sits exactly on such an edge and is itself decided by the rounding of a double, the difference is listed in
   `DIFFERENCES.md` and named in the test. The test fails if a listed difference is gone, so the list stays true.
3. **Bit for bit between the builds of the core.** `core-digests.json` is compared by the native build (the test
   `the_bits_of_all_frames_are_the_recorded_ones`) and by `engine.wasm` in Node (`tools/conformance-wasm.mjs`).

## Running them

```shell
cargo test --workspace

# engine.wasm against the digests of the native build
cargo build -p lighting-engine-wasm --target wasm32-unknown-unknown --release
node tools/conformance-wasm.mjs

# one case, with more of its differences
CASE=comet-2 SHOW=40 cargo test -p lighting-engine-compiler --test conformance_channel_values pixel_effects -- --nocapture
```

## Changing the engine or the cases

A change of the core which changes a value changes the digests. If that is meant: bump `BEHAVIOUR_VERSION` in
`crates/core/src/format.rs` and record the digests anew.

```shell
RECORD=1 cargo test -p lighting-engine-compiler --test conformance_channel_values the_bits
```

The cases are still recorded in the designer (`npm run conformance:record` in `show-designer/web`) and copied here,
until the designer plays its preview with this engine. From then on this repository owns them (`concept.md`, section
10).
