//! The compiler of the lighting engine: the project JSON of the designer in, a compiled show for the core out.

pub mod compile;
pub mod model;
pub mod profile;
pub mod stage;

pub use compile::{compile, compile_json, CompileError, Compiled, Index, Options};
