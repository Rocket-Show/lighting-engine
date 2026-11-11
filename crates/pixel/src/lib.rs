//! The maths of the pixel patterns, in single precision: where a pattern puts a pixel in the palette and how bright
//! it makes it, at a moment.
//!
//! Rules of this crate (see section 5 of `concept.md`):
//!
//! - no standard library and no heap, unless the `std` feature is on (the helpers of the compiler: the names of the
//!   project JSON, the decoding of bitmaps and palettes)
//! - `f32` only, and every function of the maths library from the `libm` crate
//! - nothing in the path of a frame can panic: no indexing, no `unwrap`, wrapping integer arithmetic
//! - the moment of an effect is a [`Cycle`]: the whole periods as an integer and the part of the running one as a
//!   float, so that the precision of a frame does not depend on how long the show has run
//!
//! The parts:
//!
//! - [`math`]: the arithmetic (`fract`, `triangle`, the hash of whole numbers, the noise, the sine over a cycle)
//! - [`cycle`]: [`Cycle`], a number of periods which keeps its whole part exact
//! - [`settings`]: what an effect sets for its pattern, the strings of the project as `u8` enums
//! - [`palette`]: the color at a place of a palette
//! - [`bitmap`]: the picture of a text or an image, a view over bytes
//! - [`audio`]: the audio a pattern follows
//! - [`pattern`]: [`sample`], a pattern at a pixel and a moment
//! - `encode` (`std` only): what the compiler does once at load time
#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod audio;
pub mod bitmap;
pub mod cycle;
#[cfg(feature = "std")]
pub mod encode;
pub mod math;
pub mod palette;
pub mod pattern;
pub mod settings;

pub use audio::{AudioFrames, AudioSettings, PixelAudio};
pub use bitmap::Bitmap;
pub use cycle::Cycle;
pub use palette::{color_of, shades_of, Stop, Stops};
pub use pattern::sample;
pub use settings::{
    Align, AudioBand, AudioFollows, AudioSource, Between, Blend, Direction, Fit, ImageColors, Pattern, PatternSettings,
    PixelContext, Repeat, Scroll, Shape, Spread, TextColors, View,
};
