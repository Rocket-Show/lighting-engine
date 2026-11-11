//! The precision of a pattern does not depend on how long the effect has run: after three periods and after three
//! and a billion (or a million millions) a pattern differs only where it does in Rocket Show, which counts the
//! periods in a double, and never because the part of the running period got lost.
//!
//! - A pattern which only depends on the part of the period is the very same, bit for bit.
//! - So is one whose decimal factors make a billion periods a whole (and even) number of what it counts: the colors
//!   of the chase (a fifth of a turn of the palette each pass), the plasma (`cycle * 0.7`, `cycle * 0.5`) and so on.
//! - The sparkle, the fire, the rain and the wipe differ in Rocket Show as well (another slot of the hash, another
//!   cell of the noise, a speed and a width which are no decimals). They are compared with the formulas of
//!   `PixelPatterns` worked out here in doubles, with the whole periods in integers of 128 bits.

use engine_pixel::math::hash_u32;
use engine_pixel::{sample, Cycle, Direction, Pattern, PatternSettings, PixelContext, Shape};

const FAR: [i64; 3] = [1_000_000_000, 1_000_000_000_000, -1_000_000_000];
const PARTS: [f32; 7] = [0.0, 0.1, 0.37, 0.5, 0.640_625, 0.75, 0.999];

fn pixels() -> Vec<PixelContext> {
    let mut pixels = Vec::new();

    // a bar of 12 pixels and matrices of 8 x 8 and 5 x 3, each with seeds of two fixtures
    for (columns, rows) in [(12, 1), (8, 8), (5, 3)] {
        let longest = columns.max(rows) as f32;

        for seed in [0, 7919 + 131, -4321] {
            for row in 0..rows {
                for column in 0..columns {
                    pixels.push(PixelContext {
                        x: (column as f32 + 0.5) / columns as f32,
                        y: (row as f32 + 0.5) / rows as f32,
                        column,
                        row,
                        columns,
                        rows,
                        aspect_x: columns as f32 / longest,
                        aspect_y: rows as f32 / longest,
                        seed,
                    });
                }
            }
        }
    }

    pixels
}

fn sampled(settings: &PatternSettings<'_>, whole: i64, part: f32, pixel: &PixelContext) -> [f32; 5] {
    let mut out = [0.0; 5];

    sample(settings, Cycle { whole, frac: part }, pixel, None, &mut out);

    out
}

fn variants(pattern: Pattern) -> Vec<PatternSettings<'static>> {
    let mut variants = Vec::new();

    for (size, softness) in [(0.25f32, 0.3f32), (0.512, 0.59), (1.0, 0.0), (0.0, 1.0)] {
        for (direction, shape, bounce) in [
            (Direction::Right, Shape::Circle, false),
            (Direction::Left, Shape::Star, true),
            (Direction::Up, Shape::Heart, false),
            (Direction::In, Shape::Square, false),
            (Direction::Ccw, Shape::Sawtooth, true),
            (Direction::Down, Shape::Figure8, false),
        ] {
            variants.push(PatternSettings {
                pattern,
                size,
                softness,
                direction,
                shape,
                bounce,
                ..PatternSettings::DEFAULT
            });
        }
    }

    variants
}

#[test]
fn is_the_same_after_a_billion_periods_where_rocket_show_is() {
    let patterns = [
        Pattern::Chase,
        Pattern::Comet,
        Pattern::Flow,
        Pattern::Plasma,
        Pattern::Ripple,
        Pattern::Shapes,
        Pattern::Center,
        Pattern::Stripes,
        Pattern::Stack,
        Pattern::Sweep,
        Pattern::Spiral,
        Pattern::Checker,
        Pattern::Tunnel,
        Pattern::Orbit,
        Pattern::Wave,
        Pattern::Meter,
        Pattern::Scope,
    ];
    let pixels = pixels();
    let mut compared = 0;
    let mut lit = 0;

    for pattern in patterns {
        for settings in variants(pattern) {
            for pixel in &pixels {
                for part in PARTS {
                    let near = sampled(&settings, 3, part, pixel);

                    for far in FAR {
                        let far = sampled(&settings, 3 + far, part, pixel);

                        // bit for bit: the place in the palette (modulo 2), the brightness and the color
                        assert_eq!(
                            near.map(f32::to_bits),
                            far.map(f32::to_bits),
                            "{settings:?} {pixel:?} {part}: {near:?} != {far:?}"
                        );
                        compared += 1;
                    }

                    if near[1] > 0.0 {
                        lit += 1;
                    }
                }
            }
        }
    }

    assert!(compared > 500_000 && lit > 50_000, "{compared} compared, {lit} lit");
}

#[test]
fn would_lose_the_part_of_the_period_in_a_float() {
    // what this is about: a billion periods and a third as one float are a billion periods
    assert_eq!(1_000_000_003.37f32, 1_000_000_000.0f32);
    assert_eq!((1_000_000_003.37f64 as f32).fract(), 0.0);

    // a comet on a bar of 12 pixels is somewhere else every tenth of a period, also then
    let comet = PatternSettings {
        pattern: Pattern::Comet,
        ..PatternSettings::DEFAULT
    };
    let bar: Vec<PixelContext> = pixels().into_iter().take(12).collect();
    let levels = |whole: i64, part: f32| {
        bar.iter()
            .map(|pixel| sampled(&comet, whole, part, pixel)[1])
            .collect::<Vec<f32>>()
    };

    for tenth in 0..9 {
        let part = tenth as f32 / 10.0;

        assert_ne!(levels(1_000_000_003, part), levels(1_000_000_003, part + 0.1));
        assert_eq!(levels(1_000_000_003, part), levels(3, part));
    }
}

// --- the formulas of PixelPatterns in doubles, the whole periods in integers --------------------------------------

/// A number with a whole part of any size: `whole + part`, the part between 0 and 1.
#[derive(Clone, Copy, Debug)]
struct Big {
    whole: i128,
    part: f64,
}

impl Big {
    fn new(whole: i128, part: f64) -> Big {
        let floor = part.floor();

        Big {
            whole: whole + floor as i128,
            part: part - floor,
        }
    }

    fn times(self, numerator: i128, denominator: i128) -> Big {
        let product = self.whole * numerator;

        Big::new(
            product.div_euclid(denominator),
            product.rem_euclid(denominator) as f64 / denominator as f64
                + self.part * numerator as f64 / denominator as f64,
        )
    }

    fn plus(self, value: f64) -> Big {
        Big::new(self.whole, self.part + value)
    }

    // PixelMath.toInt of the floor
    fn cell(self) -> i32 {
        self.whole as i32
    }
}

fn hash(a: i32, b: i32, c: i32) -> f64 {
    hash_u32(a, b, c) as f64 / 4294967296.0
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn fract(value: f64) -> f64 {
    value - value.floor()
}

fn smooth_step(from: f64, to: f64, value: f64) -> f64 {
    if !(to > from) {
        return if value < from { 0.0 } else { 1.0 };
    }

    let t = clamp01((value - from) / (to - from));

    t * t * (3.0 - 2.0 * t)
}

fn value_noise(x: Big, y: Big) -> f64 {
    let (xi, yi) = (x.cell(), y.cell());
    let u = x.part * x.part * (3.0 - 2.0 * x.part);
    let v = y.part * y.part * (3.0 - 2.0 * y.part);
    let a = hash(xi, yi, 7);
    let b = hash(xi.wrapping_add(1), yi, 7);
    let c = hash(xi, yi.wrapping_add(1), 7);
    let d = hash(xi.wrapping_add(1), yi.wrapping_add(1), 7);

    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fractal_noise(x: Big, y: Big) -> f64 {
    value_noise(x, y) * 0.55
        + value_noise(x.times(203, 100).plus(17.1), y.times(203, 100).plus(5.3)) * 0.3
        + value_noise(x.times(41, 10).plus(3.7), y.times(41, 10).plus(11.9)) * 0.15
}

// how far two places in a palette are apart, modulo 2
fn place_distance(a: f64, b: f64) -> f64 {
    let distance = (a - b).rem_euclid(2.0);

    distance.min(2.0 - distance)
}

const WHOLES: [i64; 6] = [
    3,
    1234,
    1_000_000_003,
    1_000_000_000_003,
    -1_000_000_003,
    86_400_000_000,
];

#[test]
fn sparkles_like_rocket_show_after_a_billion_periods() {
    let mut compared = 0;
    let mut lit = 0;
    let mut largest = 0.0f64;

    for settings in variants(Pattern::Sparkle) {
        let (size, softness) = (settings.size as f64, settings.softness as f64);

        for pixel in pixels() {
            for whole in WHOLES {
                for part in PARTS {
                    let column = pixel.column.wrapping_add(pixel.seed);
                    let offset = Big::new(whole as i128 * 2, part as f64 * 2.0 + hash(column, pixel.row, 11));
                    let since_flash = offset.part;
                    let slot = offset.cell();
                    let chance = hash(column, pixel.row, slot.wrapping_mul(7).wrapping_add(3));
                    let threshold = size * 0.65 + 0.04;

                    // on the edge of a slot or of the chance the last digit decides
                    if since_flash < 1e-5 || since_flash > 1.0 - 1e-5 || (chance - threshold).abs() < 1e-6 {
                        continue;
                    }

                    let expected = if chance >= threshold {
                        [0.0, 0.0]
                    } else {
                        let fade = 0.15 + softness * 0.85;
                        let left = clamp01(1.0 - (since_flash - 0.04) / fade);

                        [
                            hash(column, pixel.row, slot.wrapping_mul(13).wrapping_add(5)),
                            if since_flash < 0.04 {
                                since_flash / 0.04
                            } else {
                                left * left
                            },
                        ]
                    };
                    let actual = sampled(&settings, whole, part, &pixel);
                    let difference = (actual[0] as f64 - expected[0])
                        .abs()
                        .max((actual[1] as f64 - expected[1]).abs());

                    assert!(
                        difference < 2e-5,
                        "{settings:?} {pixel:?} {whole} + {part}: {actual:?} != {expected:?}"
                    );
                    largest = largest.max(difference);
                    compared += 1;

                    if expected[1] > 0.0 {
                        lit += 1;
                    }
                }
            }
        }
    }

    println!("sparkle: {compared} compared, {lit} lit, largest deviation {largest:e}");
    assert!(compared > 100_000 && lit > 10_000);
}

#[test]
fn burns_like_rocket_show_after_a_billion_periods() {
    let mut compared = 0;
    let mut lit = 0;
    let mut largest = 0.0f64;

    for settings in variants(Pattern::Fire) {
        let size = settings.size as f64;

        for pixel in pixels() {
            let (x, y, aspect_x, aspect_y) = (
                pixel.x as f64,
                pixel.y as f64,
                pixel.aspect_x as f64,
                pixel.aspect_y as f64,
            );
            let offset = Big::new((pixel.seed % 997) as i128, 0.0).times(137, 100);
            let matrix = pixel.rows > 1 && pixel.aspect_y > 0.2;

            for whole in WHOLES {
                for part in PARTS {
                    let cycle = Big::new(whole as i128, part as f64);

                    let expected = if !matrix {
                        let noise = fractal_noise(
                            offset.plus(x * 3.0 * aspect_x * (pixel.columns as f64 / 12.0).max(1.0)),
                            cycle.times(11, 5).plus(y * 3.0),
                        );
                        let heat = clamp01((noise - 0.22) * 1.8);

                        [heat * 0.92, clamp01(heat * 1.35)]
                    } else {
                        let height = 1.0 - y;
                        let noise = fractal_noise(
                            offset.plus(x * 3.2 * aspect_x),
                            cycle.times(-12, 5).plus(height * 2.6 * aspect_y),
                        );
                        let heat = clamp01(noise * 1.3 - height * (1.15 - size * 0.65) + 0.12);

                        [heat, clamp01(heat * 1.7)]
                    };
                    let actual = sampled(&settings, whole, part, &pixel);
                    let difference = (actual[0] as f64 - expected[0])
                        .abs()
                        .max((actual[1] as f64 - expected[1]).abs());

                    assert!(
                        difference < 2e-5,
                        "{settings:?} {pixel:?} {whole} + {part}: {actual:?} != {expected:?}"
                    );
                    largest = largest.max(difference);
                    compared += 1;

                    if expected[1] > 0.0 && expected[1] < 1.0 {
                        lit += 1;
                    }
                }
            }
        }
    }

    println!("fire: {compared} compared, {lit} neither dark nor full, largest deviation {largest:e}");
    assert!(compared > 100_000 && lit > 30_000);
}

#[test]
fn rains_like_rocket_show_after_a_billion_periods() {
    let mut compared = 0;
    let mut lit = 0;
    let mut largest = 0.0f64;

    for settings in variants(Pattern::Rain) {
        let tail = 0.15 + settings.size as f64 * 0.6;

        for pixel in pixels() {
            let column = pixel.column.wrapping_add(pixel.seed);
            // the speed of the column, 0.6 + hash * 0.9, as a fraction: (6 * 2^32 + 9 * hash) / (10 * 2^32)
            let numerator = 6 * (1i128 << 32) + 9 * hash_u32(column, 0, 3) as i128;
            let denominator = 10 * (1i128 << 32);

            'moments: for whole in WHOLES {
                for part in PARTS {
                    let fallen = Big::new(whole as i128, part as f64).times(numerator, denominator).part;
                    let mut brightest = 0.0f64;

                    for drop in 0..2 {
                        let along = fract(fallen + hash(column, drop, 5));
                        let behind = along * (1.0 + tail) - pixel.y as f64;

                        // where a drop arrives or starts over, the last digit decides
                        if behind.abs() < 1e-4 || along < 1e-5 || along > 1.0 - 1e-5 {
                            continue 'moments;
                        }

                        if behind >= 0.0 && behind < tail {
                            brightest = brightest.max(1.0 - behind / tail);
                        }
                    }

                    let actual = sampled(&settings, whole, part, &pixel);
                    let difference = (actual[0] as f64 - brightest)
                        .abs()
                        .max((actual[1] as f64 - brightest * brightest).abs());

                    assert!(
                        difference < 5e-5,
                        "{settings:?} {pixel:?} {whole} + {part}: {actual:?} != {brightest}"
                    );
                    largest = largest.max(difference);
                    compared += 1;

                    if brightest > 0.0 {
                        lit += 1;
                    }
                }
            }
        }
    }

    println!("rain: {compared} compared, {lit} lit, largest deviation {largest:e}");
    assert!(compared > 100_000 && lit > 20_000);
}

#[test]
fn wipes_like_rocket_show_after_a_billion_periods() {
    let mut compared = 0;
    let mut largest = 0.0f64;

    for settings in variants(Pattern::Wipe) {
        // how much of the palette a pass paints: a float of its own, the way the engine works it out
        let width = 0.15f32 + settings.size * 0.6f32;
        // ... which is a whole number over a power of two
        let bits = width.to_bits();
        let exponent = ((bits >> 23) & 0xff) as i32 - 150;
        let mantissa = ((bits & 0x007f_ffff) | 0x0080_0000) as i128;
        assert!(exponent < 0 && width > 0.0);
        let denominator = 1i128 << -exponent;

        for pixel in pixels() {
            let vertical = matches!(settings.direction, Direction::Up | Direction::Down);
            let position = match settings.direction {
                Direction::Left => 1.0 - pixel.x as f64,
                Direction::Up => 1.0 - pixel.y as f64,
                Direction::Down => pixel.y as f64,
                Direction::In => 1.0 - (pixel.x as f64 - 0.5).abs() * 2.0,
                _ => pixel.x as f64,
            };
            let along = if vertical { pixel.rows } else { pixel.columns };
            let edge = (settings.softness as f64 * 0.2).max(0.5 / along as f64);

            for whole in WHOLES {
                for part in PARTS {
                    let part = part as f64;
                    let painted = 1.0 - smooth_step(part - edge, part + edge, position * (1.0 - 2.0 * edge) + edge);
                    // (whole + painted) * width, modulo 2
                    let turns = (whole as i128 * mantissa).rem_euclid(2 * denominator) as f64 / denominator as f64;
                    let expected = turns + painted * width as f64;
                    let actual = sampled(&settings, whole, part as f32, &pixel);
                    let difference = place_distance(actual[0] as f64, expected);

                    assert!(
                        difference < 1e-5,
                        "{settings:?} {pixel:?} {whole} + {part}: {actual:?} != {expected}"
                    );
                    assert_eq!(actual[1], 1.0);
                    assert!((0.0..2.0).contains(&actual[0]));
                    largest = largest.max(difference);
                    compared += 1;
                }
            }
        }
    }

    println!("wipe: {compared} compared, largest deviation {largest:e}");
    assert!(compared > 100_000);
}
