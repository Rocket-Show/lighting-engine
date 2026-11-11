# Lighting engine

The lighting engine of Rocket Show, written once in Rust for its three clients:

- **Rocket Show** (Java, on SPARK and the Raspberry Pi), which loads it as WebAssembly inside the JVM,
- the **online designer** (TypeScript, in the browser), which loads the same WebAssembly file,
- **Ember** (the DMX stick, on an RP2350), whose firmware links it as a crate.

There is no code yet. [concept.md](concept.md) says what is built and why: the language, the core without the standard
library, single-precision floats, integer time, the versioned compiled show format, each client, the verification and
the plan.
