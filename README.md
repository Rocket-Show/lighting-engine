# Lighting engine

The lighting engine of Rocket Show, written once in Rust for its three clients:

- **Rocket Show** (Java, on SPARK and the Raspberry Pi), which loads it as WebAssembly inside the JVM,
- the **online designer** (TypeScript, in the browser), which loads the same WebAssembly file,
- **Ember** (the DMX stick, on an RP2350), whose firmware links it as a crate.

[concept.md](concept.md) says what is built and why. [docs/format.md](docs/format.md) specifies the compiled show a
project is turned into. This file says what is there and how to use it.

## What is there

The engine of phases 1 and 2 of the concept: everything the Java engine of Rocket Show (`lighting/designer`) and the
preview of the designer play, ported from the Java engine and compared with the conformance cases both of them pass.

| Crate             | What it is                                                                                                                                                                                                                          |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/core`     | The core: a compiled show, the inputs of the host with their times and a moment in, the value of every channel of every fixture and the DMX universes out. No standard library, no heap, single-precision floats, whole-number time |
| `crates/pixel`    | The maths of the pixel patterns, a part of the core in a crate of its own (the same rules)                                                                                                                                          |
| `crates/compiler` | The compiler: the project JSON of the designer in, a compiled show and the table from uuids to numbers out. Works everything fixed out once, in double precision                                                                    |
| `crates/wasm`     | `engine.wasm`: the core and the compiler behind a boundary of integers, floats and byte ranges, for the browser and the JVM                                                                                                         |

What the core plays: fixtures with pixels and pixel groups, virtual dimmers and dimmers of fixtures lighting in colors;
presets with steps (forward, backward, back and forth, shuffled; chasing; following the beat) and their transitions;
dimmers, colors (with white, amber, warm and cold white LEDs and filters), color wheels and color presets, positions
(zoom, focus, iris, frost, prism, strobe, blades, fog, rotations, programs, maintenance), pan and tilt in degrees,
fanned out and aimed at a spot; fans; effect curves, movements and pixel effects (23 patterns, palettes, texts and
images, audio); scenes with dimmers and fades; timelines with tracks, clips, fades and holds; the scenes picked by hand
with their fades, flashes, dimmers and speed; the beat clock with tap and beat one; identify, reset and lamp on; the
master dimmers and channels held by the host.

### How it is verified

|                                    |                                                                                                                                                                                                                                                                                     |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Against today's engines            | 299 `channelValues` cases: 3,811 frames and 209,059 values, all within ±1, 136 of them on an edge decided by rounding ([conformance/DIFFERENCES.md](conformance/DIFFERENCES.md)). 3,018 samples of effect curves, 3,441 of preset steps, 2,417 intensities, 4,610 of pixel patterns |
| Between the builds                 | `engine.wasm` in Node comes to the very same bits as the native build for every value of every frame ([conformance/core-digests.json](conformance/core-digests.json))                                                                                                               |
| Integer time                       | A frame and the frame a month and a year of whole periods later are the same bits                                                                                                                                                                                                   |
| The format                         | A damaged show is refused by its check; 1,664 shows changed by someone who fixes the checksum play without a crash                                                                                                                                                                  |
| The stage geometry of the compiler | Bit-identical to the Java code over sweeps of generated inputs                                                                                                                                                                                                                      |
| Builds                             | Native, `wasm32-unknown-unknown` and `thumbv8m.main-none-eabihf` (the Cortex-M33 of the RP2350)                                                                                                                                                                                     |

### What it costs

Measured with `cargo run --release -p lighting-engine-compiler --example bench` on an Apple M6, one core, native:

| Rig                                       | Read the JSON | Compile |  A frame |   Show | Working memory |
| ----------------------------------------- | ------------: | ------: | -------: | -----: | -------------: |
| 30 moving heads (17 channels), two scenes |       0.12 ms | 0.55 ms | 0.023 ms |   8 KB |           4 KB |
| 60 moving heads (17 channels), two scenes |       0.07 ms | 0.48 ms | 0.036 ms |  10 KB |           7 KB |
| 2,048 RGB pixels with a plasma            |       0.40 ms |  4.1 ms |  0.43 ms | 174 KB |          73 KB |

`engine.wasm` is 0.8 MB, not yet built for size. None of this is the measurement the concept asks for: the frame time on
the RP2350 and in the JVM runtime on SPARK's hardware, and the compile time in the browser, are still open.

## Using it

```shell
# once: the toolchain with the two other targets
rustup target add wasm32-unknown-unknown thumbv8m.main-none-eabihf

cargo test --workspace                                                           # everything, natively
cargo build -p lighting-engine-wasm --target wasm32-unknown-unknown --release    # target/wasm32-unknown-unknown/release/engine.wasm
node tools/conformance-wasm.mjs                                                  # engine.wasm against the native build
cargo build -p lighting-engine-core --target thumbv8m.main-none-eabihf --release # the core for the stick
python3 tools/format-tables.py --check                                           # the specification against the code
```

### From Rust (the firmware, a test)

```rust
use engine_compiler::{compile_json, Options};
use engine_core::{Engine, Show};

// the designer or Rocket Show: compile the project (of the current version) when it is loaded
let compiled = compile_json(&project_json, &Options::default())?;

// every host: check the show once, hand the working memory over, set what plays and render
let show = Show::check(&compiled.show)?;
let mut memory = vec![0u32; show.header.memory_words as usize];
let mut universes = vec![0u8; 512 * show.root.universes as usize];
let mut engine = Engine::new(show, &mut memory, now_us)?;

engine.play_timeline(compiled.index.composition_named("Song").unwrap(), engine_core::engine::timeline_flag::PLAYER);
engine.set_position(now_us, 0, true);
engine.render(now_us, &mut universes);
```

The inputs, each with its time in microseconds where it has one (`crates/core/src/engine/mod.rs`):

| Inputs         | Calls                                                                                                                |
| -------------- | -------------------------------------------------------------------------------------------------------------------- |
| What plays     | `play_timeline`, `play_preview` with `select_preset`, `set_step_preview`, `select_scene`, `play_live`, `stop`        |
| The timeline   | `set_position`, `hold`, `go`, `forget_holds`, `add_hold`, `set_holding`                                              |
| The tempo      | `set_tempo`, `tap`, `beat_one`, `set_beats`                                                                          |
| Scenes by hand | `live_scene`, `live_flash`, `live_dimmer`, `live_speed`, `live_reset`, `live_idle`                                   |
| The rest       | `set_master_dimmer`, `set_blackout`, `identify`, `reset_fixture`, `lamp_on`, `set_override`, `audio_frame`           |
| A frame        | `render` writes the universes; `value` and `values` give the value of each channel of each part (for the 3D preview) |

### Through `engine.wasm` (the designer, Rocket Show)

The module imports nothing and exports `memory` and the functions of `crates/wasm/src/lib.rs`, all starting with `le_`.
The host writes bytes into memory from `le_alloc`, calls `le_compile`, reads the show and the index (JSON: which uuid
has which number) at `le_result_ptr`, makes an engine with `le_engine_new` and calls the inputs above with its handle.
`tools/conformance-wasm.mjs` is a complete host.

## What is not there yet

- **The spike of phase 0**: the frame time on a Pico 2 and in Endive or Chicory on SPARK's hardware, the compile time in
  the browser, the firmware check. The core builds for the Cortex-M33 but has not run on one, nor on an emulated one in
  CI.
- **The three clients** (phases 3 to 5): the designer and Rocket Show still play with their own engines, and nothing
  publishes `engine.wasm` to npm or Maven. There is no firmware.
- **Old projects**: the compiler reads projects of version 11. The hosts migrate older ones before they hand them over.
- **The audio buses of an effect**: the core takes one stream of audio frames. Adding the buses an effect names stays in
  the host, which today hands over all of them.
- **What the hosts ask a project besides frames**, like where the lights of a composition have gone out
  (`getCompositionEndMillis`).
- **The licence** (concept, section 13).

## Where the code settles what the concept left open

- Time crosses the boundary as microseconds, and is counted in nanoseconds inside.
- Where a spot points each moving head is worked out by the compiler and stored in the show, so the core needs no arc
  tangent, and aiming is as precise as today.
- The core has no audio chunk in the format, as the concept says: audio is an input of the hosts which have it. A show
  with effects following the audio sets a feature bit and asks for the memory of six seconds of frames.
- The patch chunk carries a brightness for each fixture, a master dimmer of its own, which the compiler sets to 1.
