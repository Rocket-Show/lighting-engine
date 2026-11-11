//! The lighting engine of Rocket Show, the online designer and Ember: a compiled show, a time and the inputs of the
//! host in, the values of every channel of every fixture and the DMX universes out.
//!
//! No standard library and no heap (the working memory is handed over by the caller), single-precision floats with one
//! software maths library on every target, time as whole numbers. See `concept.md` and `docs/format.md`.
#![cfg_attr(not(any(test, feature = "writer")), no_std)]
#![forbid(unsafe_code)]
// nothing in the path of a frame may panic: no indexing, no unwrap
#![cfg_attr(
    not(test),
    deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
// a comparison written the way Rocket Show writes it says what happens with a value which is not a number, and a
// minimum of a maximum never gives one back (a clamp would)
#![allow(
    clippy::neg_cmp_op_on_partial_ord,
    clippy::manual_clamp,
    clippy::new_without_default,
    clippy::too_many_arguments
)]

pub mod engine;
pub mod fix;
pub mod format;

pub use engine::{memory_words, Engine};
pub use format::{Show, ShowError};
