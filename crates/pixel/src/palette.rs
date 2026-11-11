//! The palette of a pixel effect: the colors a pattern gives its pixels, from the place it puts them at.
//!
//! # Stops as bytes
//!
//! The compiled show holds the stops of a palette as 16 bytes each, little-endian, one after the other:
//!
//! | offset | type  | value                         |
//! | ------ | ----- | ----------------------------- |
//! | 0      | `f32` | position, between 0 and 1     |
//! | 4      | `f32` | red, between 0 and 1          |
//! | 8      | `f32` | green                         |
//! | 12     | `f32` | blue                          |
//!
//! The stops are **sorted by their position** (the ones on the same position in the order the project lists them):
//! the compiler does it once (`encode_stops` of the `std` feature, or [`sort_stops`]), so that it is not done for
//! every color which is read.
//!
//! # The palette of the preset
//!
//! An effect without a palette of its own (`ref: "preset"`, the default of a pixel effect) takes the shades of the
//! color the preset puts on each pixel: [`shades_of`] that color, with [`Between::Smooth`] and [`Repeat::Mirror`]
//! unless the palette says otherwise.

use crate::math::{clamp01, fract, triangle};
use crate::settings::{Between, Repeat};

/// The bytes of a stop.
pub const STOP_LEN: usize = 16;

/// A color of a palette (red, green and blue between 0 and 1) at its place between 0 and 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub position: f32,
    pub color: [f32; 3],
}

impl Stop {
    /// The 16 bytes of the stop (see the layout of the module).
    pub fn to_bytes(self) -> [u8; STOP_LEN] {
        let mut bytes = [0u8; STOP_LEN];
        let values = [self.position, self.color[0], self.color[1], self.color[2]];

        for (chunk, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(values) {
            *chunk = value.to_le_bytes();
        }

        bytes
    }
}

#[derive(Clone, Copy, Debug)]
enum Source<'a> {
    Bytes(&'a [u8]),
    Slice(&'a [Stop]),
}

/// The stops of a palette, sorted by their position: a view over the bytes of the compiled show, or over stops in
/// memory.
#[derive(Clone, Copy, Debug)]
pub struct Stops<'a> {
    source: Source<'a>,
}

impl<'a> Stops<'a> {
    /// No stops: every color is black.
    pub const EMPTY: Stops<'static> = Stops {
        source: Source::Slice(&[]),
    };

    /// The stops of the bytes of a compiled show (see the layout of the module), none if they are not whole stops.
    pub fn from_bytes(bytes: &'a [u8]) -> Option<Stops<'a>> {
        if !bytes.len().is_multiple_of(STOP_LEN) {
            return None;
        }

        Some(Stops {
            source: Source::Bytes(bytes),
        })
    }

    /// Stops in memory, which have to be sorted by their position (see [`sort_stops`]).
    pub const fn from_slice(stops: &'a [Stop]) -> Stops<'a> {
        Stops {
            source: Source::Slice(stops),
        }
    }

    pub fn len(&self) -> usize {
        match self.source {
            Source::Bytes(bytes) => bytes.len() / STOP_LEN,
            Source::Slice(stops) => stops.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A stop, none beyond the last one.
    pub fn get(&self, index: usize) -> Option<Stop> {
        match self.source {
            Source::Bytes(bytes) => {
                let start = index.checked_mul(STOP_LEN)?;
                let stop = bytes.get(start..start.checked_add(STOP_LEN)?)?;

                Some(Stop {
                    position: float_at(stop, 0)?,
                    color: [float_at(stop, 4)?, float_at(stop, 8)?, float_at(stop, 12)?],
                })
            }
            Source::Slice(stops) => stops.get(index).copied(),
        }
    }
}

fn float_at(bytes: &[u8], offset: usize) -> Option<f32> {
    let bytes = bytes.get(offset..offset.checked_add(4)?)?;

    Some(f32::from_le_bytes(bytes.try_into().ok()?))
}

/// The color at a place of a palette, as red, green and blue between 0 and 1. The palette repeats beyond 0 and 1:
/// [`Repeat::Wrap`] starts over at every whole number, [`Repeat::Mirror`] runs back and forth (and is back where it
/// was after 2). So a place only counts modulo 2, which [`sample`](crate::pattern::sample) makes use of. No stops are
/// black.
///
/// The stops have to be sorted by their position.
pub fn color_of(stops: &Stops<'_>, between: Between, repeat: Repeat, place: f32) -> [f32; 3] {
    let count = stops.len();

    let Some(first) = stops.get(0) else {
        return [0.0, 0.0, 0.0];
    };

    let position = match repeat {
        Repeat::Wrap => fract(place),
        Repeat::Mirror => triangle(place * 0.5),
    };

    if between == Between::Steps {
        // (the cast of a place which is not a number is 0)
        let step = libm::floorf(position * count as f32) as i32;
        let index = if step < 0 { 0 } else { step as usize };
        let index = if index >= count { count - 1 } else { index };

        return stops.get(index).map(|stop| stop.color).unwrap_or([0.0, 0.0, 0.0]);
    }

    if position <= first.position {
        return first.color;
    }

    let mut from = first;

    for index in 1..count {
        let Some(to) = stops.get(index) else {
            break;
        };

        if position <= to.position {
            let width = to.position - from.position;
            let share = if width > 0.0 {
                (position - from.position) / width
            } else {
                1.0
            };

            return [
                from.color[0] + (to.color[0] - from.color[0]) * share,
                from.color[1] + (to.color[1] - from.color[1]) * share,
                from.color[2] + (to.color[2] - from.color[2]) * share,
            ];
        }

        from = to;
    }

    from.color
}

/// A part of a color as one of its 256 steps over 255, like a color written as `#rrggbb` and read again.
fn through_hex(component: f32) -> f32 {
    // (half a step goes up, and what is not a number is 0)
    let step = libm::floorf(clamp01(component) * 255.0 + 0.5);

    if step.is_nan() {
        0.0
    } else {
        step / 255.0
    }
}

/// The shades of a color (red, green and blue between 0 and 1), for the palette which follows the color of the
/// preset: from the color itself most of the way to white. Both are rounded to the nearest of the 256 steps a color of
/// a project has.
pub fn shades_of(color: [f32; 3]) -> [Stop; 2] {
    let light = [
        color[0] + (1.0 - color[0]) * 0.75,
        color[1] + (1.0 - color[1]) * 0.75,
        color[2] + (1.0 - color[2]) * 0.75,
    ];

    [
        Stop {
            position: 0.0,
            color: [through_hex(color[0]), through_hex(color[1]), through_hex(color[2])],
        },
        Stop {
            position: 1.0,
            color: [through_hex(light[0]), through_hex(light[1]), through_hex(light[2])],
        },
    ]
}

/// Sorts stops by their position, the ones on the same position staying in their order (a position which is not a
/// number goes last). Without a heap, by insertion: for the load time, not for a frame.
pub fn sort_stops(stops: &mut [Stop]) {
    for sorted in 1..stops.len() {
        let mut index = sorted;

        while index > 0 {
            let (Some(before), Some(current)) = (stops.get(index - 1).copied(), stops.get(index).copied()) else {
                break;
            };

            if !comes_after(before.position, current.position) {
                break;
            }

            stops.swap(index - 1, index);
            index -= 1;
        }
    }
}

// whether a is above b: what is not a number is above every number, -0 below 0
fn comes_after(a: f32, b: f32) -> bool {
    if a.is_nan() {
        return !b.is_nan();
    }

    if b.is_nan() {
        return false;
    }

    a > b || (a == 0.0 && b == 0.0 && b.is_sign_negative() && a.is_sign_positive())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [f32; 3] = [1.0, 0.0, 0.0];
    const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
    const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

    fn rainbow() -> [Stop; 3] {
        [
            Stop {
                position: 0.0,
                color: RED,
            },
            Stop {
                position: 0.5,
                color: GREEN,
            },
            Stop {
                position: 1.0,
                color: BLUE,
            },
        ]
    }

    #[test]
    fn has_no_color_without_stops() {
        assert_eq!(
            color_of(&Stops::EMPTY, Between::Smooth, Repeat::Mirror, 0.5),
            [0.0, 0.0, 0.0]
        );
        assert_eq!(
            color_of(&Stops::from_slice(&[]), Between::Steps, Repeat::Wrap, 0.5),
            [0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn blends_from_one_color_to_the_next() {
        let stops = rainbow();
        let stops = Stops::from_slice(&stops);

        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 0.0), RED);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 0.25), [0.5, 0.5, 0.0]);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 0.5), GREEN);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 0.75), [0.0, 0.5, 0.5]);
        // wrap starts over
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 1.0), RED);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 7.25), [0.5, 0.5, 0.0]);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, -0.75), [0.5, 0.5, 0.0]);
        // mirror runs back
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Mirror, 1.0), BLUE);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Mirror, 1.25), [0.0, 0.5, 0.5]);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Mirror, 1.75), [0.5, 0.5, 0.0]);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Mirror, 2.0), RED);
        assert_eq!(
            color_of(&stops, Between::Smooth, Repeat::Mirror, -0.25),
            [0.5, 0.5, 0.0]
        );
        // a place which is not a number: the last color
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Mirror, f32::NAN), BLUE);
    }

    #[test]
    fn jumps_from_one_color_to_the_next() {
        let stops = rainbow();
        let stops = Stops::from_slice(&stops);

        assert_eq!(color_of(&stops, Between::Steps, Repeat::Wrap, 0.0), RED);
        assert_eq!(color_of(&stops, Between::Steps, Repeat::Wrap, 0.3), RED);
        assert_eq!(color_of(&stops, Between::Steps, Repeat::Wrap, 0.34), GREEN);
        assert_eq!(color_of(&stops, Between::Steps, Repeat::Wrap, 0.99), BLUE);
        assert_eq!(color_of(&stops, Between::Steps, Repeat::Wrap, 1.0), RED);
        // the end of a mirrored palette is its last color
        assert_eq!(color_of(&stops, Between::Steps, Repeat::Mirror, 1.0), BLUE);
        assert_eq!(color_of(&stops, Between::Steps, Repeat::Mirror, f32::NAN), RED);
    }

    #[test]
    fn stays_on_the_ends_and_jumps_over_a_stop_of_no_width() {
        let stops = [
            Stop {
                position: 0.2,
                color: RED,
            },
            Stop {
                position: 0.5,
                color: GREEN,
            },
            Stop {
                position: 0.5,
                color: BLUE,
            },
            Stop {
                position: 0.8,
                color: RED,
            },
        ];
        let stops = Stops::from_slice(&stops);

        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 0.1), RED);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 0.5), GREEN);
        let between = color_of(&stops, Between::Smooth, Repeat::Wrap, 0.65);

        assert!((between[0] - 0.5).abs() < 1e-6 && between[1] == 0.0 && (between[2] - 0.5).abs() < 1e-6);
        assert_eq!(color_of(&stops, Between::Smooth, Repeat::Wrap, 0.9), RED);
    }

    #[test]
    fn counts_a_place_modulo_two() {
        let stops = rainbow();
        let stops = Stops::from_slice(&stops);

        for between in [Between::Smooth, Between::Steps] {
            for repeat in [Repeat::Mirror, Repeat::Wrap] {
                for place in [0.0f32, 0.125, 0.5, 0.875, 1.0, 1.375, 1.75] {
                    let color = color_of(&stops, between, repeat, place);

                    for turns in [-6.0f32, -2.0, 2.0, 4.0, 1024.0] {
                        assert_eq!(
                            color_of(&stops, between, repeat, place + turns),
                            color,
                            "{place} + {turns}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn reads_the_stops_from_bytes() {
        let stops = rainbow();
        let mut bytes = [0u8; 48];

        for (chunk, stop) in bytes.chunks_exact_mut(STOP_LEN).zip(stops) {
            chunk.copy_from_slice(&stop.to_bytes());
        }

        // position 0.5 is 0x3F000000, written from its lowest byte
        assert_eq!(&bytes[16..24], &[0, 0, 0, 0x3F, 0, 0, 0, 0]);

        let view = Stops::from_bytes(&bytes).unwrap();

        assert_eq!(view.len(), 3);
        assert_eq!(view.get(1), Some(stops[1]));
        assert_eq!(view.get(3), None);
        assert_eq!(view.get(usize::MAX), None);
        assert_eq!(color_of(&view, Between::Smooth, Repeat::Wrap, 0.25), [0.5, 0.5, 0.0]);
        assert!(Stops::from_bytes(&bytes[..47]).is_none());
        assert!(Stops::from_bytes(&[]).unwrap().is_empty());
    }

    #[test]
    fn makes_the_shades_of_a_color() {
        // #ff8000 and #ffdfbf
        let shades = shades_of([1.0, 0.5, 0.0]);

        assert_eq!(
            shades[0],
            Stop {
                position: 0.0,
                color: [1.0, 128.0 / 255.0, 0.0]
            }
        );
        assert_eq!(
            shades[1],
            Stop {
                position: 1.0,
                color: [1.0, 223.0 / 255.0, 191.0 / 255.0]
            }
        );

        // out of range and not a number: the nearest end of the range, and 0
        let shades = shades_of([2.0, -1.0, f32::NAN]);

        assert_eq!(shades[0].color, [1.0, 0.0, 0.0]);
        assert!(shades[1].color.iter().all(|component| component.is_finite()));
    }

    #[test]
    fn sorts_the_stops_keeping_the_order_of_the_ones_on_the_same_place() {
        let mut stops = [
            Stop {
                position: 0.8,
                color: RED,
            },
            Stop {
                position: 0.2,
                color: GREEN,
            },
            Stop {
                position: 0.5,
                color: BLUE,
            },
            Stop {
                position: 0.5,
                color: RED,
            },
            Stop {
                position: f32::NAN,
                color: BLUE,
            },
            Stop {
                position: 0.0,
                color: GREEN,
            },
        ];

        sort_stops(&mut stops);

        let positions: Vec<f32> = stops.iter().map(|stop| stop.position).collect();

        assert_eq!(&positions[..5], &[0.0, 0.2, 0.5, 0.5, 0.8]);
        assert!(positions[5].is_nan());
        assert_eq!(stops[2].color, BLUE);
        assert_eq!(stops[3].color, RED);
    }
}
