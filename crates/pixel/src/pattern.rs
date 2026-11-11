//! The patterns of the pixel effects: where a pattern puts a pixel in the palette and how bright it makes it, at a
//! moment (`PixelPatterns` of Rocket Show, `pixel-pattern.ts` of the designer).
//!
//! # The moment
//!
//! The moment is a [`Cycle`]: the periods of the effect since it started. Rocket Show counts them in a `double`;
//! here the whole periods stay an integer wherever a pattern uses them:
//!
//! - what only depends on the part of the period (most of every pattern) reads [`Cycle::fract`]
//! - the pass a pattern is in (the color of a chase, the slot of a sparkle, the cell of the noise of the fire) is the
//!   exact integer, wrapped around at 32 bits where Rocket Show does so
//! - a decimal factor of the moment (`cycle * 0.7` of the plasma, `cycle * 2.2` of the fire) is worked out as a
//!   fraction of whole numbers ([`Cycle::mul_ratio`]), the speed of a column of the rain (a hash) by the 32 bits of
//!   the hash ([`Cycle::mul_u32_fraction`]), and the width of a color of the wipe, which comes from the size of the
//!   effect, exactly for the float it is ([`Cycle::mul`])
//!
//! So a pattern looks the same after a billion periods as after three, wherever it does in Rocket Show.
//!
//! # The place in the palette
//!
//! A place which grows with the periods (of the chase, the wipe, the flow, the shapes, the center and the stripes)
//! is given **modulo 2**, between 0 and 2: a palette repeats after 1 ([`Repeat::Wrap`](crate::Repeat)) or after 2
//! ([`Repeat::Mirror`](crate::Repeat)), so [`color_of`](crate::palette::color_of) gives the very same color for it,
//! and the place does not lose its precision as a float. The other places are the ones of Rocket Show.

use crate::audio::{PixelAudio, BANDS};
use crate::bitmap::Bitmap;
use crate::cycle::Cycle;
use crate::math::{band, clamp01, fract, hash, hash_u32, maxf, minf, sin_cycle, smooth_step, to_int, triangle, TAU};
use crate::settings::{
    Align, Direction, Fit, ImageColors, Pattern, PatternSettings, PixelContext, Scroll, Shape, TextColors,
};

/// The outline of a heart, as how far it is from its middle in each of 180 directions (the longest one being 1):
/// (x² + y² - 1)³ = x² y³, found by halving the distance 24 times, like `PixelPatterns.heartOutline` does in doubles
/// when it is loaded. A test works it out again.
pub(crate) const HEART_OUTLINE: [f32; 180] = [
    0.7890306, 0.80000526, 0.81130874, 0.82290494, 0.83475137, 0.8467991, 0.85899127, 0.8712636, 0.8835439, 0.89575154,
    0.90779763, 0.9195859, 0.93101245, 0.94196695, 0.952334, 0.96199423, 0.97082657, 0.9787102, 0.98552674, 0.9911633,
    0.995515, 0.9984874, 1.0, 0.9999874, 0.9984024, 0.9952171, 0.99042386, 0.9840358, 0.9760862, 0.9666277, 0.9557303,
    0.94347906, 0.92997074, 0.9153107, 0.8996085, 0.88297385, 0.8655113, 0.8473147, 0.82846045, 0.80899763, 0.78893346,
    0.76820666, 0.74663115, 0.7237469, 0.6982594, 0.6590919, 0.6982594, 0.7237469, 0.74663115, 0.76820666, 0.78893346,
    0.80899763, 0.82846045, 0.8473147, 0.8655113, 0.88297385, 0.8996085, 0.9153107, 0.92997074, 0.94347906, 0.9557303,
    0.9666277, 0.9760862, 0.9840358, 0.99042386, 0.9952171, 0.9984024, 0.9999874, 1.0, 0.9984874, 0.995515, 0.9911633,
    0.98552674, 0.9787102, 0.97082657, 0.96199423, 0.952334, 0.94196695, 0.93101245, 0.9195859, 0.90779763, 0.89575154,
    0.8835439, 0.8712636, 0.85899127, 0.8467991, 0.83475137, 0.82290494, 0.81130874, 0.80000526, 0.7890306, 0.778415,
    0.7681835, 0.7583568, 0.7489514, 0.7399803, 0.7314536, 0.72337866, 0.71576095, 0.70860416, 0.70191026, 0.69568056,
    0.6899153, 0.6846144, 0.6797772, 0.67540294, 0.67149115, 0.66804117, 0.665053, 0.66252726, 0.66046494, 0.6588677,
    0.6577386, 0.6570813, 0.65690094, 0.6572039, 0.6579982, 0.6592936, 0.6611021, 0.6634381, 0.6663187, 0.66976464,
    0.67380065, 0.6784569, 0.6837698, 0.6897846, 0.6965574, 0.7041602, 0.7126867, 0.722264, 0.7330708, 0.74537456,
    0.7596107, 0.7765885, 0.7982464, 0.8388442, 0.7982464, 0.7765885, 0.7596107, 0.74537456, 0.7330708, 0.722264,
    0.7126867, 0.7041602, 0.6965574, 0.6897846, 0.6837698, 0.6784569, 0.67380065, 0.66976464, 0.6663187, 0.6634381,
    0.6611021, 0.6592936, 0.6579982, 0.6572039, 0.65690094, 0.6570813, 0.6577386, 0.6588677, 0.66046494, 0.66252726,
    0.665053, 0.66804117, 0.67149115, 0.67540294, 0.6797772, 0.6846144, 0.6899153, 0.69568056, 0.70191026, 0.70860416,
    0.71576095, 0.72337866, 0.7314536, 0.7399803, 0.7489514, 0.7583568, 0.7681835, 0.778415,
];

// how far along the direction a pixel is, from 0 to 1
fn along(context: &PixelContext, direction: Direction) -> f32 {
    match direction {
        Direction::Left => 1.0 - context.x,
        Direction::Up => 1.0 - context.y,
        Direction::Down => context.y,
        Direction::Out => libm::fabsf(context.x - 0.5) * 2.0,
        Direction::In => 1.0 - libm::fabsf(context.x - 0.5) * 2.0,
        Direction::Right | Direction::Cw | Direction::Ccw => context.x,
    }
}

// how many pixels there are along the direction, to soften an edge over at least half of one
fn pixels_along(context: &PixelContext, direction: Direction) -> i32 {
    if is_vertical(direction) {
        context.rows
    } else {
        context.columns
    }
}

fn is_vertical(direction: Direction) -> bool {
    matches!(direction, Direction::Up | Direction::Down)
}

// left for left, right for every other direction (of the wave and the wave of the audio)
fn left_or_right(direction: Direction) -> Direction {
    if direction == Direction::Left {
        Direction::Left
    } else {
        Direction::Right
    }
}

// Is the picture a matrix, rather than a bar? A matrix has rows, which are not much shorter than its columns.
fn is_matrix(context: &PixelContext) -> bool {
    context.rows > 1 && context.aspect_y > 0.2
}

// where the dot of an orbit is at a moment of its period, from -1 to 1 on the y axis (up being positive): the shapes
// of the movements of the moving heads. On the x axis it is the sine of the moment.
fn orbit_y(shape: Shape, moment: f32) -> f32 {
    match shape {
        Shape::Figure8 => sin_cycle(moment * 2.0),
        Shape::Line => sin_cycle(moment),
        Shape::Wave => sin_cycle(moment * 3.0),
        _ => sin_cycle(moment + 0.25),
    }
}

// a curve of a wave at a moment of its period, between 0 and 1
fn wave_curve(shape: Shape, moment: f32) -> f32 {
    match shape {
        Shape::Triangle => triangle(moment),
        Shape::Square => {
            if moment < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        Shape::Sawtooth => moment,
        _ => 0.5 + 0.5 * sin_cycle(moment),
    }
}

// the place of a pixel counted from the end a stack piles up at, in pixels
fn stack_index(context: &PixelContext, direction: Direction) -> i32 {
    match direction {
        Direction::Left => context.column,
        Direction::Up => context.row,
        Direction::Down => context.rows.wrapping_sub(1).wrapping_sub(context.row),
        _ => context.columns.wrapping_sub(1).wrapping_sub(context.column),
    }
}

// the angle of a point from the x axis, in turns (between -0.5 and 0.5)
fn turn_of(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x) / TAU
}

// how far a point (from the middle, -1 to 1 on each axis, up being positive) is out in a shape: 1 on its outline
fn shape_distance(x: f32, y: f32, shape: Shape) -> f32 {
    match shape {
        Shape::Square => maxf(libm::fabsf(x), libm::fabsf(y)),
        Shape::Diamond => (libm::fabsf(x) + libm::fabsf(y)) * 0.72,
        Shape::Star => {
            let angle = turn_of(y, x);
            // five points, one of them up
            let outline = 0.62 + 0.38 * sin_cycle(5.0 * (angle - 0.25) + 0.25);

            (libm::sqrtf(x * x + y * y) / outline) * 0.78
        }
        Shape::Heart => {
            let angle = fract(turn_of(y, x));
            // (the cast of an angle which is not a number is 0; the angle is never negative)
            let direction = (libm::floorf(angle * HEART_OUTLINE.len() as f32) as u32 as usize) % HEART_OUTLINE.len();
            let outline = HEART_OUTLINE.get(direction).copied().unwrap_or(1.0);

            libm::sqrtf(x * x + y * y) / outline
        }
        _ => libm::sqrtf(x * x + y * y),
    }
}

// half of a difference of whole numbers, rounded down (Math.floor((a - b) / 2.0))
fn half_of_difference(a: i32, b: i32) -> i32 {
    a.wrapping_sub(b) >> 1
}

// Where a pixel shows a text: the column and the row of the text in the picture. The text moves a pixel at a time,
// through the whole picture in a period, or stands aligned in it.
fn text_position(
    settings: &PatternSettings<'_>,
    moment: f32,
    context: &PixelContext,
    bitmap: &Bitmap<'_>,
) -> (i32, i32) {
    let width = bitmap.width();
    let height = bitmap.height();
    let across = libm::floorf(moment * width.wrapping_add(context.columns) as f32) as i32;
    let down = libm::floorf(moment * height.wrapping_add(context.rows) as f32) as i32;
    let top = half_of_difference(context.rows, height).wrapping_sub(to_int(settings.offset_y));
    let x = match settings.align {
        Align::Left => context.column,
        Align::Right => context.column.wrapping_sub(context.columns.wrapping_sub(width)),
        Align::Center => context.column.wrapping_sub(half_of_difference(context.columns, width)),
    };

    match settings.scroll {
        Scroll::Right => (
            context.column.wrapping_sub(across).wrapping_add(width),
            context.row.wrapping_sub(top),
        ),
        Scroll::Up => (x, context.row.wrapping_add(down).wrapping_sub(context.rows)),
        Scroll::Down => (x, context.row.wrapping_sub(down).wrapping_add(height)),
        Scroll::None => (x, context.row.wrapping_sub(top)),
        // a text scrolls to the left on its own
        Scroll::Left | Scroll::Auto => (
            context.column.wrapping_add(across).wrapping_sub(context.columns),
            context.row.wrapping_sub(top),
        ),
    }
}

// Where a pixel shows an image, from 0 to 1 on each axis (outside of it beyond): fitted into the picture whole
// ('contain', with dark edges where it is narrower), filling it ('cover'), stretched over it or one pixel of the
// image for each pixel ('pixels', in the middle), scrolling around
fn image_position(
    settings: &PatternSettings<'_>,
    moment: f32,
    context: &PixelContext,
    bitmap: &Bitmap<'_>,
) -> (f32, f32) {
    let width = bitmap.width();
    let height = bitmap.height();
    let mut u = context.x;
    let mut v = context.y;
    let picture_ratio = if context.aspect_y > 0.0 {
        context.aspect_x / context.aspect_y
    } else {
        context.columns as f32
    };
    let image_ratio = width as f32 / height as f32;

    match settings.fit {
        Fit::Cover => {
            if image_ratio > picture_ratio {
                u = (u - 0.5) * (picture_ratio / image_ratio) + 0.5;
            } else {
                v = (v - 0.5) * (image_ratio / picture_ratio) + 0.5;
            }
        }
        Fit::Stretch => {}
        Fit::Pixels => {
            // (the whole pixels in 64 bits, where the difference of a column and a half can not leave them)
            let column = context.column as i64 - half_of_difference(context.columns, width) as i64;
            let row = context.row as i64 - half_of_difference(context.rows, height) as i64;

            u = (column as f32 + 0.5) / width as f32;
            v = (row as f32 + 0.5) / height as f32;
        }
        Fit::Contain => {
            if image_ratio > picture_ratio {
                v = (v - 0.5) * (image_ratio / picture_ratio) + 0.5;
            } else {
                u = (u - 0.5) * (picture_ratio / image_ratio) + 0.5;
            }
        }
    }

    match settings.scroll {
        Scroll::Left => (fract_sum(u, moment), v),
        Scroll::Right => (fract(u - moment), v),
        Scroll::Up => (u, fract_sum(v, moment)),
        Scroll::Down => (u, fract(v - moment)),
        // an image does not scroll on its own
        Scroll::None | Scroll::Auto => (u, v),
    }
}

// a place in the palette and a brightness, the color taken from the palette
fn set(out: &mut [f32; 5], place: f32, level: f32) {
    *out = [place, level, -1.0, 0.0, 0.0];
}

/// A pattern at a pixel and a moment (`PixelPatterns.sample`).
///
/// - `out[0]`: where the pattern puts the pixel in the palette. Any number, the palette repeats; a place which grows
///   with the periods is given modulo 2 (see the module).
/// - `out[1]`: how bright the pixel is, between 0 and 1.
/// - `out[2]` to `out[4]`: an image showing its own colors gives the color of the pixel here (red, green and blue,
///   between 0 and 1) in place of the palette. Every other pattern sets `out[2]` to -1: the color comes from the
///   palette, at `out[0]`.
///
/// The moment is counted in periods of the effect since it started. With the audio the effect follows (none for one
/// which follows none), the level meter and the wave of the audio show it; without, a curve.
///
/// Nothing in here can panic, whatever the settings, the pixel and the bytes of the bitmap are, and no result is
/// not a number.
pub fn sample(
    settings: &PatternSettings<'_>,
    cycle: Cycle,
    context: &PixelContext,
    audio: Option<&PixelAudio<'_>>,
    out: &mut [f32; 5],
) {
    let cycle = Cycle::new(cycle.whole, cycle.frac);
    // the part of the running period, and the periods run
    let moment = cycle.frac;
    let passes = Cycle::from_whole(cycle.whole);
    let size = settings.size;
    let softness = settings.softness;
    let direction = settings.direction;

    match settings.pattern {
        Pattern::Chase => {
            // a block running along, every pass in the next color
            let position = along(context, direction);
            let width = maxf(size, 0.02);
            let middle = moment * (1.0 + width) - width / 2.0;
            let edge = maxf(softness * width * 0.5, 0.5 / pixels_along(context, direction) as f32);

            set(
                out,
                passes.mul_ratio(1, 5).mod2(),
                band(libm::fabsf(position - middle), width / 2.0, edge),
            );
        }
        Pattern::Comet => {
            // a head with a tail fading behind it through the palette
            let position = along(context, direction);
            let tail = maxf(size, 0.05);

            let behind = if settings.bounce {
                let head = triangle(moment);

                if moment < 0.5 {
                    head - position
                } else {
                    position - head
                }
            } else {
                moment * (1.0 + tail) - position
            };

            if behind < 0.0 {
                // a glow in front of the head, over about a pixel
                let pixels = behind * pixels_along(context, direction) as f32;
                let glow = maxf(0.0, 1.0 - pixels * pixels * 0.8);

                set(out, 1.0, glow * glow);
            } else if behind > tail {
                set(out, 0.0, 0.0);
            } else {
                let left = 1.0 - behind / tail;

                set(out, left, left * left);
            }
        }
        Pattern::Wipe => {
            // the next color of the palette paints over the last one
            let position = along(context, direction);
            let edge = maxf(softness * 0.2, 0.5 / pixels_along(context, direction) as f32);
            let painted = 1.0 - smooth_step(moment - edge, moment + edge, position * (1.0 - 2.0 * edge) + edge);

            set(out, Cycle::new(cycle.whole, painted).mul(0.15 + size * 0.6).mod2(), 1.0);
        }
        Pattern::Flow => {
            // the palette moving along
            let place = cycle.neg().add(along(context, direction) * (0.35 + (1.0 - size) * 2.2));

            set(out, place.mod2(), 1.0);
        }
        Pattern::Sparkle => {
            // pixels flashing up at random and fading out
            let column = context.column.wrapping_add(context.seed);
            let offset = cycle.mul_ratio(2, 1).add(hash(column, context.row, 11));
            let since_flash = offset.frac;
            let slot = offset.floor_i32();

            if hash(column, context.row, slot.wrapping_mul(7).wrapping_add(3)) >= size * 0.65 + 0.04 {
                set(out, 0.0, 0.0);
            } else {
                let fade = 0.15 + softness * 0.85;
                let left = clamp01(1.0 - (since_flash - 0.04) / fade);
                let level = if since_flash < 0.04 {
                    since_flash / 0.04
                } else {
                    left * left
                };

                set(
                    out,
                    hash(column, context.row, slot.wrapping_mul(13).wrapping_add(5)),
                    level,
                );
            }
        }
        Pattern::Plasma => {
            // soft clouds of color drifting and swirling, the scale in periods of the sine across the picture
            let scale = 0.5 + (1.0 - size) * 1.0;
            let x = context.x * scale * context.aspect_x;
            let y = context.y * scale * context.aspect_y;
            let dx = x - scale / 2.0;
            let dy = y - scale / 2.0;
            // (a sine is the same a whole period on: the parts of the periods are enough)
            let value = sin_cycle(x + moment)
                + sin_cycle(y * 0.9 - cycle.mul_ratio(7, 10).frac + 0.16)
                + sin_cycle((x + y) * 0.7 + cycle.mul_ratio(1, 2).frac)
                + sin_cycle(libm::sqrtf(dx * dx + dy * dy) * 1.3 - moment);

            set(out, value / 8.0 + 0.5, 1.0);
        }
        Pattern::Ripple => {
            // rings spreading out from the middle
            let dx = (context.x - 0.5) * context.aspect_x;
            let dy = (context.y - 0.5) * context.aspect_y;
            let distance = libm::sqrtf(dx * dx + dy * dy) * core::f32::consts::SQRT_2;
            let ring = fract(distance * (0.8 + size * 3.0) - moment);
            let tail = 0.15 + softness * 0.7;

            set(
                out,
                distance,
                smooth_step(0.0, 0.06, 1.0 - ring) * smooth_step(1.0 - tail, 1.0, ring),
            );
        }
        Pattern::Shapes => {
            // an outline growing out of the middle and fading
            let x = (context.x - 0.5) * 2.0 * context.aspect_x;
            let y = -(context.y - 0.5) * 2.0 * context.aspect_y;
            let distance = shape_distance(x, y, settings.shape);
            let width = 0.1 + size * 0.45;
            let edge = 0.03 + softness * 0.18;

            set(
                out,
                passes.mul_ratio(27, 100).add(distance * 0.25).mod2(),
                band(libm::fabsf(distance - moment * 1.35), width / 2.0, edge) * (1.0 - moment * 0.55),
            );
        }
        Pattern::Fire => {
            // flames rising from the bottom, embers flickering on a bar
            // (the place the fixture starts in the noise, (seed % 997) * 1.37, as a fraction of whole numbers)
            let offset = Cycle::from_whole(context.seed.checked_rem(997).unwrap_or(0) as i64).mul_ratio(137, 100);

            if !is_matrix(context) {
                let x = offset.add(context.x * 3.0 * context.aspect_x * maxf(1.0, context.columns as f32 / 12.0));
                let y = cycle.mul_ratio(11, 5).add(context.y * 3.0);
                let noise = crate::math::fractal_noise_at(x, y);
                let heat = clamp01((noise - 0.22) * 1.8);

                set(out, heat * 0.92, clamp01(heat * 1.35));
            } else {
                let height = 1.0 - context.y;
                let x = offset.add(context.x * 3.2 * context.aspect_x);
                let y = cycle.mul_ratio(-12, 5).add(height * 2.6 * context.aspect_y);
                let noise = crate::math::fractal_noise_at(x, y);
                let heat = clamp01(noise * 1.3 - height * (1.15 - size * 0.65) + 0.12);

                set(out, heat, clamp01(heat * 1.7));
            }
        }
        Pattern::Center => {
            // pulses running from the middle out to the ends (or the edges of a matrix), or in from them
            let dx = (context.x - 0.5) * 2.0 * context.aspect_x;
            let dy = (context.y - 0.5) * 2.0 * context.aspect_y;
            let distance = if is_matrix(context) {
                minf(1.0, libm::sqrtf(dx * dx + dy * dy))
            } else {
                libm::fabsf(context.x - 0.5) * 2.0
            };
            let position = if direction == Direction::In {
                1.0 - distance
            } else {
                distance
            };
            let width = maxf(size, 0.03);
            let middle = moment * (1.0 + width) - width / 2.0;
            let longest = if context.columns > context.rows {
                context.columns
            } else {
                context.rows
            };
            let edge = maxf(softness * width * 0.5, 1.0 / longest as f32);

            set(
                out,
                passes.mul_ratio(1, 4).add(position * 0.3).mod2(),
                band(libm::fabsf(position - middle), width / 2.0, edge),
            );
        }
        Pattern::Stripes => {
            // evenly spaced stripes marching along, each in its own color
            let spacing = 0.08 + size * 0.5;
            let stripes = cycle.neg().add(along(context, direction) / spacing).add(0.25);
            let edge = maxf(
                0.04 + softness * 0.2,
                0.5 / (pixels_along(context, direction) as f32 * spacing),
            );

            set(
                out,
                Cycle::from_whole(stripes.whole).mul_ratio(17, 100).mod2(),
                band(libm::fabsf(stripes.frac - 0.5), 0.25, edge),
            );
        }
        Pattern::Stack => {
            // pixels dropping in one by one and piling up at the far end, until all of them are there (each row or
            // column of a matrix on its own)
            let along = pixels_along(context, direction);
            let count = if along > 2 { along } else { 2 };
            let index = stack_index(context, direction);
            let index = if index < 0 { 0 } else { index };
            let index = if index > count - 1 { count - 1 } else { index };
            let vertical = is_vertical(direction);
            let lane = if is_matrix(context) {
                hash(
                    if vertical { context.column } else { 0 },
                    if vertical { 0 } else { context.row },
                    9,
                )
            } else {
                0.0
            };
            let progress = fract(moment + lane);
            // (the whole numbers as floats: a float holds every count of pixels there is)
            let last = (count - 1) as f32;
            let steps = progress * (last + 2.0);
            let step = libm::floorf(steps);
            let place = index as f32 / last;

            if step >= count as f32 || (index as f32) < step {
                set(out, place, 1.0);
            } else {
                // the one dropping in, from the near end down to the pile
                let falling = last - (steps - step) * (last - step);

                set(
                    out,
                    place,
                    if index as f32 == libm::floorf(falling + 0.5) {
                        1.0
                    } else {
                        0.0
                    },
                );
            }
        }
        Pattern::Sweep => {
            // a beam turning around the middle with a trail behind it, like a radar
            let turn = turn_of(
                (context.y - 0.5) * context.aspect_y,
                (context.x - 0.5) * context.aspect_x,
            );
            let behind = fract(moment - if direction == Direction::Ccw { -turn } else { turn });
            let trail = 0.12 + size * 0.7;
            let left = 1.0 - behind / trail;

            set(
                out,
                1.0 - (behind / trail) * 0.8,
                if behind < trail { left * left } else { 0.0 },
            );
        }
        Pattern::Spiral => {
            // arms of a spiral turning and seeming to pull inwards
            let dx = (context.x - 0.5) * 2.0 * context.aspect_x;
            let dy = (context.y - 0.5) * 2.0 * context.aspect_y;
            let distance = libm::sqrtf(dx * dx + dy * dy);
            let turn = turn_of(dy, dx);
            let arms = libm::floorf(1.5 + size * 5.0);
            let arm = fract(turn * arms + distance * 1.4 + if direction == Direction::Ccw { moment } else { -moment });

            set(
                out,
                distance * 0.7,
                band(libm::fabsf(arm - 0.5), 0.25, 0.04 + softness * 0.22),
            );
        }
        Pattern::Checker => {
            // squares swapping every half period, fading over to the other color where it is soft
            let cells = libm::floorf(1.5 + size * 6.0);
            let odd = libm::fmodf(
                libm::floorf(context.column as f32 / cells) + libm::floorf(context.row as f32 / cells),
                2.0,
            );
            let half = if moment < 0.5 { 0.0 } else { 1.0 };
            let fade = smooth_step(0.0, softness * 0.5 + 0.001, fract(moment * 2.0));

            set(out, odd, if odd == half { fade } else { 1.0 - fade });
        }
        Pattern::Rain => {
            // drops falling down the columns, each column at a speed of its own
            let column = context.column.wrapping_add(context.seed);
            let tail = 0.15 + size * 0.6;
            // The periods times the speed of the column, 0.6 + hash * 0.9: three fifths of them, and nine tenths of
            // them times the hash by its 32 bits. As a float the speed would be off by a period after some millions.
            let fallen = cycle
                .mul_ratio(3, 5)
                .add_cycle(cycle.mul_ratio(9, 10).mul_u32_fraction(hash_u32(column, 0, 3)))
                .frac;
            let mut brightest = 0.0;

            for drop in 0..2 {
                let behind = fract(fallen + hash(column, drop, 5)) * (1.0 + tail) - context.y;

                if behind >= 0.0 && behind < tail {
                    brightest = maxf(brightest, 1.0 - behind / tail);
                }
            }

            set(out, brightest, brightest * brightest);
        }
        Pattern::Tunnel => {
            // frames flying outwards, as if driving through a tunnel: the deeper the closer together
            let x = (context.x - 0.5) * 2.0 * context.aspect_x;
            let y = -(context.y - 0.5) * 2.0 * context.aspect_y;
            let distance = shape_distance(x, y, settings.shape);
            let depth = cycle.add((0.25 + size * 0.75) / (distance + 0.15));
            let frame = band(libm::fabsf(depth.frac - 0.5), 0.22, 0.05 + softness * 0.3);

            set(
                out,
                Cycle::from_whole(depth.whole).mul_ratio(27, 100).frac,
                frame * clamp01(distance * 1.6 + 0.15),
            );
        }
        Pattern::Orbit => {
            // a glowing dot moving along the shape of a movement of the moving heads, with a trail
            let radius = 0.08 + size * 0.25;
            let gap = 0.004 + softness * 0.03;
            let mut level = 0.0;
            let mut place = 0.0;

            for behind in 0..7 {
                let behind = behind as f32;
                let at = fract(moment - behind * gap);
                let dx = (context.x - (0.5 + 0.36 * sin_cycle(at))) * context.aspect_x;
                let dy = (context.y - (0.5 - 0.36 * orbit_y(settings.shape, at))) * context.aspect_y;
                let distance = libm::sqrtf(dx * dx + dy * dy) / radius;
                let glow = clamp01(1.0 - distance * distance * 0.5);
                let weight = glow * glow * (1.0 - behind / 7.0);

                if weight > level {
                    level = weight;
                    place = at;
                }
            }

            set(out, place, level);
        }
        Pattern::Wave => {
            // a matrix filled up to a curve running along, a bar showing the curve itself
            let at = fract(along(context, left_or_right(direction)) * (0.4 + (1.0 - size) * 2.2) - moment);
            let value = 0.08 + 0.84 * wave_curve(settings.shape, at);

            if !is_matrix(context) {
                set(out, value, value);
            } else {
                let height = 1.0 - context.y;
                let edge = 0.5 / context.rows as f32;

                set(out, height, 1.0 - smooth_step(value - edge, value + edge, height));
            }
        }
        Pattern::Meter => {
            // A bar filled up to how loud the audio is, a matrix with a column for each band from the lowest to the
            // highest, like an equalizer. Without audio, a curve going up and down fills it.
            if !is_matrix(context) {
                let position = along(context, direction);
                let edge = 0.5 / pixels_along(context, direction) as f32;
                let level = match audio {
                    Some(audio) => audio.level(),
                    None => triangle(moment),
                };

                set(out, position, 1.0 - smooth_step(level - edge, level + edge, position));
            } else {
                let level = match audio {
                    Some(audio) => audio.band(band_of_column(context)),
                    None => triangle(moment + hash(context.column, 0, 4) * 0.5),
                };
                let height = 1.0 - context.y;
                let edge = 0.5 / context.rows as f32;

                set(out, height, 1.0 - smooth_step(level - edge, level + edge, height));
            }
        }
        Pattern::Scope => {
            // How loud the audio was, scrolling along: the newest at the end it runs to, a matrix filled up to it.
            // Across the pixels from half a second up to five. Without audio, a curve running along.
            let frames = 25i32.saturating_add(libm::floorf(size * 225.0) as i32);
            let back = libm::floorf((1.0 - along(context, left_or_right(direction))) * frames as f32) as i32;
            let level = match audio {
                Some(audio) => audio.level_ago(back),
                None => triangle(moment - back as f32 / frames as f32),
            };

            if !is_matrix(context) {
                set(out, level, level);
            } else {
                let height = 1.0 - context.y;
                let edge = 0.5 / context.rows as f32;

                set(out, height, 1.0 - smooth_step(level - edge, level + edge, height));
            }
        }
        Pattern::Text => {
            // a text of a pixel for each pixel, scrolling through the picture
            set(out, 0.0, 0.0);

            if let Some(bitmap) = settings
                .bitmap
                .as_ref()
                .filter(|bitmap| bitmap.width() > 0 && bitmap.height() > 0)
            {
                let (x, y) = text_position(settings, moment, context, bitmap);
                let value = bitmap.value(bitmap.frame(0), x, y, 0);

                if value > 0 {
                    let place = match settings.text_colors {
                        TextColors::Letters => bitmap.letters_up_to(x) as f32 * 0.17,
                        TextColors::One => 0.0,
                        TextColors::Along => x as f32 / bitmap.width() as f32,
                    };

                    set(out, place, value as f32 / 255.0);
                }
            }
        }
        Pattern::Image => {
            // an image or an animation, fitted into the picture
            set(out, 0.0, 0.0);

            if let Some(bitmap) = settings
                .bitmap
                .as_ref()
                .filter(|bitmap| bitmap.width() > 0 && bitmap.height() > 0)
            {
                let (u, v) = image_position(settings, moment, context, bitmap);

                if !(u < 0.0 || u >= 1.0 || v < 0.0 || v >= 1.0) {
                    let frame = bitmap.frame(bitmap.frame_at(moment));
                    let x = libm::floorf(u * bitmap.width() as f32) as i32;
                    let x = if x > bitmap.width() - 1 { bitmap.width() - 1 } else { x };
                    let y = libm::floorf(v * bitmap.height() as f32) as i32;
                    let y = if y > bitmap.height() - 1 {
                        bitmap.height() - 1
                    } else {
                        y
                    };
                    let red = bitmap.value(frame, x, y, 0);
                    let green = if bitmap.channels() == 3 {
                        bitmap.value(frame, x, y, 1)
                    } else {
                        red
                    };
                    let blue = if bitmap.channels() == 3 {
                        bitmap.value(frame, x, y, 2)
                    } else {
                        red
                    };

                    if settings.image_colors == ImageColors::Palette {
                        // how bright it is, through the palette
                        set(out, (red * 299 + green * 587 + blue * 114) as f32 / 255_000.0, 1.0);
                    } else {
                        *out = [0.0, 1.0, red as f32 / 255.0, green as f32 / 255.0, blue as f32 / 255.0];
                    }
                }
            }
        }
        // a pattern of a newer designer
        Pattern::Unknown => set(out, 0.0, 0.0),
    }

    // settings or a pixel out of every range (a size which makes no cells, a picture of no pixels) may end in what is
    // no number: nothing of it leaves
    if !out[0].is_finite() {
        out[0] = 0.0;
    }

    if out[1].is_nan() {
        out[1] = 0.0;
    }
}

// the band of the 16 a column of a level meter shows, from the lowest to the highest: where its middle is
fn band_of_column(context: &PixelContext) -> i32 {
    let highest = BANDS as i32 - 1;

    if context.columns > 0 {
        // floor(((column + 0.5) / columns) * 16), in whole numbers
        let band = ((2 * context.column as i64 + 1) * (BANDS as i64 / 2))
            .checked_div_euclid(context.columns as i64)
            .unwrap_or(0);

        if band > highest as i64 {
            highest
        } else if band < i32::MIN as i64 {
            i32::MIN
        } else {
            band as i32
        }
    } else {
        // a picture without columns: as the floats fall
        let band = libm::floorf(((context.column as f32 + 0.5) / context.columns as f32) * BANDS as f32) as i32;

        if band > highest {
            highest
        } else {
            band
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioFrame, AudioFrames, AudioSettings, FIRST_BAND, FRAME_LENGTH, RMS};
    use crate::encode::{encode_bitmap, BitmapSource};

    fn settings(pattern: Pattern, direction: Direction) -> PatternSettings<'static> {
        PatternSettings {
            pattern,
            direction,
            ..PatternSettings::DEFAULT
        }
    }

    // a pixel of a bar
    fn bar(column: i32, columns: i32) -> PixelContext {
        PixelContext {
            x: (column as f32 + 0.5) / columns as f32,
            column,
            columns,
            aspect_y: 1.0 / columns as f32,
            ..PixelContext::DEFAULT
        }
    }

    // a pixel of a matrix
    fn matrix(column: i32, row: i32, columns: i32, rows: i32) -> PixelContext {
        let longest = columns.max(rows) as f32;

        PixelContext {
            x: (column as f32 + 0.5) / columns as f32,
            y: (row as f32 + 0.5) / rows as f32,
            column,
            row,
            columns,
            rows,
            aspect_x: columns as f32 / longest,
            aspect_y: rows as f32 / longest,
            seed: 0,
        }
    }

    fn sampled(settings: &PatternSettings<'_>, cycle: f64, context: &PixelContext) -> [f32; 5] {
        let mut out = [9.0; 5];

        sample(settings, Cycle::from_f64(cycle), context, None, &mut out);

        out
    }

    fn levels(settings: &PatternSettings<'_>, cycle: f64, pixels: i32) -> Vec<f32> {
        (0..pixels)
            .map(|pixel| sampled(settings, cycle, &bar(pixel, pixels))[1])
            .collect()
    }

    #[test]
    fn knows_the_outline_of_a_heart() {
        // PixelPatterns.heartOutline, in doubles
        let inside = |x: f64, y: f64| {
            let a = x * x + y * y - 1.0;
            a * a * a - x * x * y * y * y <= 0.0
        };
        let mut outline = [0.0f64; 180];
        let mut longest = 0.0f64;

        for (direction, distance) in outline.iter_mut().enumerate() {
            let angle = direction as f64 / 180.0 * std::f64::consts::TAU;
            let (dy, dx) = angle.sin_cos();
            let mut low = 0.0;
            let mut high = 2.0;

            for _ in 0..24 {
                let middle = (low + high) / 2.0;

                if inside(dx * middle, 0.12 + dy * middle) {
                    low = middle;
                } else {
                    high = middle;
                }
            }

            *distance = low;
            longest = longest.max(low);
        }

        for (direction, distance) in outline.iter().enumerate() {
            assert!(
                (HEART_OUTLINE[direction] as f64 - distance / longest).abs() < 1e-6,
                "direction {direction}"
            );
        }
    }

    #[test]
    fn stacks_the_pixels_at_the_far_end_until_all_of_them_are_there() {
        // (PixelPatternsTest)
        assert_eq!(
            levels(&settings(Pattern::Stack, Direction::Right), 0.3, 4),
            [0.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(
            levels(&settings(Pattern::Stack, Direction::Left), 0.3, 4),
            [1.0, 0.0, 1.0, 0.0]
        );
        assert_eq!(
            levels(&settings(Pattern::Stack, Direction::Right), 0.9, 4),
            [1.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn runs_pulses_from_the_middle_out_to_the_ends_of_a_bar() {
        // (PixelPatternsTest)
        let early = levels(&settings(Pattern::Center, Direction::Out), 0.1, 9);
        let late = levels(&settings(Pattern::Center, Direction::Out), 0.9, 9);

        assert!(early[4] > early[0]);
        assert!(late[0] > late[4]);
    }

    #[test]
    fn lights_nothing_for_a_pattern_it_does_not_know_or_without_its_picture() {
        for pattern in [Pattern::Unknown, Pattern::Text, Pattern::Image] {
            assert_eq!(
                sampled(&settings(pattern, Direction::Right), 0.4, &bar(2, 8)),
                [0.0, 0.0, -1.0, 0.0, 0.0]
            );
        }
    }

    #[test]
    fn scrolls_a_text_through_the_picture() {
        // two columns of a letter: the first one bright, the second one half as bright
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(2),
            height: Some(1),
            frames: &[Some("/4A=")],
            letter_starts: &[Some(0), Some(1)],
            ..BitmapSource::default()
        })
        .unwrap();
        let bitmap = Bitmap::parse(&bytes).unwrap();
        let text = PatternSettings {
            bitmap: Some(bitmap),
            ..settings(Pattern::Text, Direction::Right)
        };

        // through 2 + 4 columns in a period, to the left: at its start the text is just outside on the right
        assert_eq!(levels(&text, 0.0, 4), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(levels(&text, 0.25, 4), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(levels(&text, 0.5, 4), [0.0, 1.0, 128.0 / 255.0, 0.0]);
        assert_eq!(levels(&text, 0.99, 4), [128.0 / 255.0, 0.0, 0.0, 0.0]);
        // the same after any number of periods
        assert_eq!(levels(&text, 1_000_000.5, 4), levels(&text, 0.5, 4));

        // standing, in the middle or at an end
        let standing = PatternSettings {
            scroll: Scroll::None,
            ..text
        };
        assert_eq!(levels(&standing, 0.7, 4), [0.0, 1.0, 128.0 / 255.0, 0.0]);
        assert_eq!(
            levels(
                &PatternSettings {
                    align: Align::Left,
                    ..standing
                },
                0.7,
                4
            ),
            [1.0, 128.0 / 255.0, 0.0, 0.0]
        );
        assert_eq!(
            levels(
                &PatternSettings {
                    align: Align::Right,
                    ..standing
                },
                0.7,
                4
            ),
            [0.0, 0.0, 1.0, 128.0 / 255.0]
        );

        // the colors: along the text, of its letters or one
        let place = |text_colors: TextColors, column: i32| {
            sampled(
                &PatternSettings {
                    text_colors,
                    ..standing
                },
                0.0,
                &bar(column, 4),
            )[0]
        };
        assert_eq!([place(TextColors::Along, 1), place(TextColors::Along, 2)], [0.0, 0.5]);
        assert_eq!(
            [place(TextColors::Letters, 1), place(TextColors::Letters, 2)],
            [0.17, 0.34]
        );
        assert_eq!([place(TextColors::One, 1), place(TextColors::One, 2)], [0.0, 0.0]);
    }

    #[test]
    fn shows_an_image_in_its_own_colors_or_through_the_palette() {
        // two pixels, a red and a blue one, and a second frame with both white
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(2),
            height: Some(1),
            channels: Some(3),
            frames: &[Some("/wAAAAD/"), Some("////////")],
            durations: &[Some(100.0), Some(300.0)],
            ..BitmapSource::default()
        })
        .unwrap();
        let bitmap = Bitmap::parse(&bytes).unwrap();
        let image = PatternSettings {
            bitmap: Some(bitmap),
            fit: Fit::Stretch,
            ..settings(Pattern::Image, Direction::Right)
        };

        assert_eq!(sampled(&image, 0.1, &bar(0, 4)), [0.0, 1.0, 1.0, 0.0, 0.0]);
        assert_eq!(sampled(&image, 0.1, &bar(3, 4)), [0.0, 1.0, 0.0, 0.0, 1.0]);
        // the second frame from a quarter of the period on
        assert_eq!(sampled(&image, 0.3, &bar(0, 4)), [0.0, 1.0, 1.0, 1.0, 1.0]);
        assert_eq!(sampled(&image, 7.3, &bar(0, 4)), [0.0, 1.0, 1.0, 1.0, 1.0]);

        // through the palette: how bright the pixel is
        let through = PatternSettings {
            image_colors: ImageColors::Palette,
            ..image
        };
        assert_eq!(sampled(&through, 0.1, &bar(0, 4)), [0.299, 1.0, -1.0, 0.0, 0.0]);
        assert_eq!(sampled(&through, 0.1, &bar(3, 4)), [0.114, 1.0, -1.0, 0.0, 0.0]);

        // contained in a bar of four pixels, an image of two takes them all; scrolled half a period, its halves swap
        let contained = PatternSettings {
            fit: Fit::Contain,
            ..image
        };
        assert_eq!(sampled(&contained, 0.1, &bar(1, 4))[2..], [1.0, 0.0, 0.0]);
        let scrolled = PatternSettings {
            scroll: Scroll::Left,
            ..image
        };
        assert_eq!(sampled(&scrolled, 0.5, &bar(0, 4))[2..], [1.0, 1.0, 1.0]);
        assert_eq!(
            sampled(
                &PatternSettings {
                    bitmap: Some(Bitmap::parse(&bytes[..]).unwrap()),
                    ..scrolled
                },
                0.125,
                &bar(3, 4)
            )[2..],
            [1.0, 0.0, 0.0]
        );
    }

    // frames as loud as their index says: the RMS rises by 1 dB with each frame, the bands by 2 dB with each band
    struct Rising;

    impl AudioFrames for Rising {
        fn frame(&self, index: i64) -> Option<AudioFrame> {
            let mut frame = [-60.0 + index as f32 * 0.1; FRAME_LENGTH];

            for band in 0..BANDS {
                frame[FIRST_BAND + band] = -50.0 + 3.0 * band as f32;
            }

            frame[RMS] = -60.0 + index as f32 * 0.1;

            Some(frame)
        }
    }

    #[test]
    fn shows_the_audio() {
        let frames = Rising;
        // at once up and down: the level is the one of the frame
        let instant = AudioSettings {
            attack_millis: 0.0,
            release_millis: 0.0,
            ..AudioSettings::DEFAULT
        };
        let audio = PixelAudio::new(&frames, 300 * 20_000, instant);
        let mut out = [0.0; 5];

        // the level meter on a bar: filled up to the level, (-60 + 30 - -60) / 60 = 0.5
        let meter = settings(Pattern::Meter, Direction::Right);
        let filled: Vec<f32> = (0..8)
            .map(|pixel| {
                sample(&meter, Cycle::ZERO, &bar(pixel, 8), Some(&audio), &mut out);
                out[1]
            })
            .collect();

        assert_eq!(&filled[..3], &[1.0, 1.0, 1.0]);
        assert_eq!(&filled[5..], &[0.0, 0.0, 0.0]);
        assert!((audio.level() - 0.5).abs() < 1e-6);

        // on a matrix, a column for each band: 16 columns show the 16 bands, the level of band b is (10 + 3 b) / 60
        let mut heights = [0; 16];

        for (column, height) in heights.iter_mut().enumerate() {
            for row in 0..20 {
                sample(
                    &meter,
                    Cycle::ZERO,
                    &matrix(column as i32, row, 16, 20),
                    Some(&audio),
                    &mut out,
                );

                if out[1] > 0.5 {
                    *height += 1;
                }
            }
        }

        for (band, height) in heights.iter().enumerate() {
            assert_eq!(
                *height,
                ((10.0 + 3.0 * band as f32) / 60.0 * 20.0).round() as i32,
                "band {band}"
            );
        }

        // four columns show the bands 2, 6, 10 and 14, eight columns the odd ones
        assert_eq!(
            (0..4)
                .map(|column| band_of_column(&matrix(column, 0, 4, 4)))
                .collect::<Vec<_>>(),
            [2, 6, 10, 14]
        );
        assert_eq!(
            (0..8)
                .map(|column| band_of_column(&matrix(column, 0, 8, 4)))
                .collect::<Vec<_>>(),
            [1, 3, 5, 7, 9, 11, 13, 15]
        );
        assert_eq!(band_of_column(&matrix(40, 0, 32, 4)), 15);
        assert_eq!(
            band_of_column(&PixelContext {
                columns: 0,
                ..PixelContext::DEFAULT
            }),
            15
        );

        // the wave of the audio: the newest at the end it runs to, the oldest 25 + 225 * size frames ago
        let scope = PatternSettings {
            size: 1.0,
            ..settings(Pattern::Scope, Direction::Right)
        };
        let level_ago = |frames: f32| (30.0 - frames * 0.1) / 60.0;

        for (pixel, ago) in [(9, 12.0), (5, 112.0), (0, 237.0)] {
            sample(&scope, Cycle::ZERO, &bar(pixel, 10), Some(&audio), &mut out);
            assert!((out[1] - level_ago(ago)).abs() < 1e-5, "pixel {pixel}: {}", out[1]);
            assert_eq!(out[0], out[1]);
        }

        // without audio a curve fills them: the same whatever the periods run
        sample(&meter, Cycle::new(5, 0.25), &bar(0, 8), None, &mut out);
        assert_eq!(out[1], 1.0);
        sample(&meter, Cycle::new(5, 0.25), &bar(7, 8), None, &mut out);
        assert_eq!(out[1], 0.0);
    }

    #[test]
    fn never_panics_and_gives_numbers_whatever_it_is_asked() {
        let picture = encode_bitmap(&BitmapSource {
            width: Some(3),
            height: Some(2),
            channels: Some(3),
            frames: &[Some("/wAAAAD/"), None, Some("AAAA")],
            durations: &[Some(1.0), Some(2.0)],
            letter_starts: &[Some(0), Some(2)],
        })
        .unwrap();
        let huge = encode_bitmap(&BitmapSource {
            width: Some(i32::MAX),
            height: Some(i32::MAX),
            frames: &[Some("/wAAAAD/")],
            ..BitmapSource::default()
        })
        .unwrap();
        let negative = encode_bitmap(&BitmapSource {
            width: Some(-4),
            height: Some(i32::MIN),
            ..BitmapSource::default()
        })
        .unwrap();
        let bitmaps = [
            None,
            Bitmap::parse(&picture),
            Bitmap::parse(&huge),
            Bitmap::parse(&negative),
        ];

        let floats = [
            0.0f32,
            -0.0,
            0.5,
            1.0,
            -1.0,
            2.5,
            1e-30,
            1e30,
            -1e30,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ];
        let wholes = [
            0i32,
            1,
            -1,
            2,
            7,
            16,
            997,
            -997,
            i32::MAX,
            i32::MIN,
            i32::MAX - 1,
            i32::MIN + 1,
        ];
        let cycles = [
            Cycle::ZERO,
            Cycle::new(3, 0.5),
            Cycle {
                whole: i64::MAX,
                frac: 0.999_999_94,
            },
            Cycle {
                whole: i64::MIN,
                frac: 0.0,
            },
            Cycle {
                whole: -1,
                frac: f32::NAN,
            },
            Cycle {
                whole: 5,
                frac: f32::INFINITY,
            },
            Cycle { whole: 5, frac: -7.25 },
            Cycle {
                whole: 1 << 40,
                frac: 1e30,
            },
        ];
        let frames = Rising;
        let audio = PixelAudio::new(
            &frames,
            u64::MAX,
            AudioSettings {
                gain_db: f32::NAN,
                floor_db: f32::INFINITY,
                attack_millis: f32::NAN,
                ..AudioSettings::DEFAULT
            },
        );
        let mut out = [0.0; 5];
        let mut count = 0u32;
        // a walk through the values which looks random, the same every time
        let mut pick = 0usize;
        let mut next = |length: usize| {
            pick = pick
                .wrapping_mul(6_364_136_223_846_793_005usize)
                .wrapping_add(1_442_695_040_888_963_407usize);
            (pick >> 33) % length
        };

        for pattern in 0..=23u8 {
            for round in 0..6000 {
                let settings = PatternSettings {
                    pattern: Pattern::from_u8(pattern),
                    size: floats[next(floats.len())],
                    softness: floats[next(floats.len())],
                    direction: Direction::from_u8(next(8) as u8),
                    shape: Shape::from_u8(next(11) as u8),
                    bounce: round % 2 == 0,
                    bitmap: bitmaps[next(bitmaps.len())],
                    scroll: Scroll::from_u8(next(6) as u8),
                    align: Align::from_u8(next(3) as u8),
                    offset_y: floats[next(floats.len())],
                    fit: Fit::from_u8(next(4) as u8),
                    text_colors: TextColors::from_u8(next(3) as u8),
                    image_colors: ImageColors::from_u8(next(2) as u8),
                };
                let context = PixelContext {
                    x: floats[next(floats.len())],
                    y: floats[next(floats.len())],
                    column: wholes[next(wholes.len())],
                    row: wholes[next(wholes.len())],
                    columns: wholes[next(wholes.len())],
                    rows: wholes[next(wholes.len())],
                    aspect_x: floats[next(floats.len())],
                    aspect_y: floats[next(floats.len())],
                    seed: wholes[next(wholes.len())],
                };
                let cycle = cycles[next(cycles.len())];

                sample(
                    &settings,
                    cycle,
                    &context,
                    if round % 3 == 0 { Some(&audio) } else { None },
                    &mut out,
                );

                assert!(
                    out.iter().all(|value| !value.is_nan()),
                    "{settings:?} {context:?} {cycle:?}: {out:?}"
                );
                assert!(out[0].is_finite(), "{settings:?} {context:?} {cycle:?}: {out:?}");
                count += 1;
            }
        }

        assert_eq!(count, 24 * 6000);
    }
}

/// The part after the whole of `value + moment`, for a moment between 0 and 1. A moment just below 1 (a run held just
/// before its end) stays just below the border: the sum itself would round onto it.
fn fract_sum(value: f32, moment: f32) -> f32 {
    if moment >= 0.5 {
        fract(value - (1.0 - moment))
    } else {
        fract(value + moment)
    }
}
