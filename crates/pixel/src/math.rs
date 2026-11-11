//! The arithmetic of the pixel patterns, in single precision.
//!
//! What looks random comes from a hash of whole numbers, never from a random source: a moment of an effect always
//! looks the same.

use crate::cycle::Cycle;

/// A full turn, 2π.
pub const TAU: f32 = core::f32::consts::TAU;

/// The bigger one of two numbers (the second one, if the first is not a number).
pub(crate) fn maxf(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// The smaller one of two numbers (the second one, if the first is not a number).
pub(crate) fn minf(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

/// A value between 0 and 1: 0 below, 1 above (and what is not a number stays so).
pub fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// The part of a number after its whole cycles, between 0 and 1 (also for a negative one).
pub fn fract(value: f32) -> f32 {
    value - libm::floorf(value)
}

/// Up from 0 to 1 over the first half of each cycle, back down over the second.
pub fn triangle(value: f32) -> f32 {
    1.0 - libm::fabsf(2.0 * fract(value) - 1.0)
}

/// 0 below `from`, 1 above `to` and a smooth step in between. A step of no width jumps at `from`.
pub fn smooth_step(from: f32, to: f32, value: f32) -> f32 {
    // (not "to <= from": a border which is not a number jumps as well)
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(to > from) {
        return if value < from { 0.0 } else { 1.0 };
    }

    let t = clamp01((value - from) / (to - from));

    t * t * (3.0 - 2.0 * t)
}

/// 1 within `half_width` of a point, 0 beyond, softened over `edge` on both sides of the border.
pub fn band(distance: f32, half_width: f32, edge: f32) -> f32 {
    1.0 - smooth_step(half_width - edge, half_width + edge, distance)
}

/// A number as a 32 bit integer, cut off towards zero and wrapped around.
pub fn to_int(value: f32) -> i32 {
    (value as i64) as i32
}

/// The hash of three whole numbers (a pixel, a moment and what it is for) as its 32 bits, worked out in integers
/// with multiplications which wrap around at 32 bits.
pub fn hash_u32(a: i32, b: i32, c: i32) -> u32 {
    let mut h = (a as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((b as u32).wrapping_mul(668_265_263))
        .wrapping_add((c as u32).wrapping_mul(1_440_670_441));

    h = (h ^ (h >> 15)).wrapping_mul(0x85EB_CA77);
    h = (h ^ (h >> 13)).wrapping_mul(0xC2B2_AE35);

    h ^ (h >> 16)
}

/// A number between 0 and 1 (never 1) which looks random, the same for the same three whole numbers.
///
/// The 32 bits of [`hash_u32`] over 2^32, cut off to the 24 bits a float holds: rounding to the nearest float instead
/// would turn the 128 highest hashes into 1, which a hash never is.
pub fn hash(a: i32, b: i32, c: i32) -> f32 {
    (hash_u32(a, b, c) >> 8) as f32 / 16_777_216.0
}

/// The sine over a cycle, sin(2π · position): the cycle is reduced to a quarter around its nearest multiple of 90°,
/// and the sine or the cosine of what is left (between -π/4 and π/4) comes from `libm`.
pub fn sin_cycle(position: f32) -> f32 {
    // the nearest quarter of the cycle and the angle left from it
    let quarter = libm::floorf(position * 4.0 + 0.5);
    let angle = TAU * (position - quarter / 4.0);

    // a float this big is a multiple of four (and a cast would stop at the highest integer)
    let quadrant = if libm::fabsf(quarter) < 9.0e18 {
        (quarter as i64) & 3
    } else {
        0
    };

    match quadrant {
        0 => libm::sinf(angle),
        1 => libm::cosf(angle),
        2 => -libm::sinf(angle),
        _ => -libm::cosf(angle),
    }
}

/// A smooth landscape of values between 0 and 1: random heights on the whole numbers, smoothly blended in between.
pub fn value_noise(x: f32, y: f32) -> f32 {
    value_noise_at(Cycle::from_f32(x), Cycle::from_f32(y))
}

/// [`value_noise`] at a place whose whole part is exact: the cell of the landscape is taken from the integers
/// (wrapped around at 32 bits), the place in the cell from the parts.
pub fn value_noise_at(x: Cycle, y: Cycle) -> f32 {
    let x = Cycle::new(x.whole, x.frac);
    let y = Cycle::new(y.whole, y.frac);
    let xi = x.whole as i32;
    let yi = y.whole as i32;
    let xf = x.frac;
    let yf = y.frac;
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    let a = hash(xi, yi, 7);
    let b = hash(xi.wrapping_add(1), yi, 7);
    let c = hash(xi, yi.wrapping_add(1), 7);
    let d = hash(xi.wrapping_add(1), yi.wrapping_add(1), 7);

    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// The landscape with finer ones laid over it, for the look of flames and clouds.
pub fn fractal_noise(x: f32, y: f32) -> f32 {
    fractal_noise_at(Cycle::from_f32(x), Cycle::from_f32(y))
}

/// [`fractal_noise`] at a place whose whole part is exact. The finer landscapes are at `2.03` and `4.1` times the
/// place, which are worked out as 203/100 and 41/10 of it (see [`Cycle::mul_ratio`]).
pub fn fractal_noise_at(x: Cycle, y: Cycle) -> f32 {
    value_noise_at(x, y) * 0.55
        + value_noise_at(x.mul_ratio(203, 100).add(17.1), y.mul_ratio(203, 100).add(5.3)) * 0.3
        + value_noise_at(x.mul_ratio(41, 10).add(3.7), y.mul_ratio(41, 10).add(11.9)) * 0.15
}

#[cfg(test)]
mod tests {
    use super::*;

    // the hash again in 64 bit integers, cut back to 32 bits after every step, as a double
    fn hash_in_64_bits(a: i32, b: i32, c: i32) -> f64 {
        let wrap = |value: i64| value as i32;
        let unsigned_shift = |value: i32, bits: u32| ((value as u32) >> bits) as i32;
        let mut h = wrap(a as i64 * 374761393 + b as i64 * 668265263 + c as i64 * 1440670441);
        h = wrap((h ^ unsigned_shift(h, 15)) as i64 * 0x85EBCA77u32 as i32 as i64);
        h = wrap((h ^ unsigned_shift(h, 13)) as i64 * 0xC2B2AE35u32 as i32 as i64);
        h ^= unsigned_shift(h, 16);

        (h as i64 & 0xFFFF_FFFF) as f64 / 4294967296.0
    }

    #[test]
    fn hashes_three_whole_numbers() {
        // worked out by hand from the formula:
        // hash(0, 0, 0): h stays 0
        assert_eq!(hash_u32(0, 0, 0), 0);
        assert_eq!(hash(0, 0, 0), 0.0);

        // hash(1, 0, 0): h = 374761393 = 0x165667B1
        //   h ^ (h >> 15) = 0x165667B1 ^ 0x00002CAC = 0x16564B1D, times 0x85EBCA77 (mod 2^32) = 0x7E00CC7B
        //   h ^ (h >> 13) = 0x7E00CC7B ^ 0x0003F006 = 0x7E033C7D, times 0xC2B2AE35 (mod 2^32) = 0x12B27BE1
        //   h ^ (h >> 16) = 0x12B27BE1 ^ 0x000012B2 = 0x12B26953, over 2^32 = 0.0730348...
        assert_eq!(hash_u32(1, 0, 0), 0x12B2_6953);
        assert_eq!(hash(1, 0, 0), 0x12_B269 as f32 / 16_777_216.0);
        assert!((hash(1, 0, 0) - 0.073_034_84).abs() < 1e-7);

        // hash(0, 1, 0): h = 668265263 = 0x27D4EB2F -> 0x27D4A486 -> 0xF2AC364A -> 0xF2ABA32B -> 0xF05601E7
        //   -> 0xF056F1B1, over 2^32 = 0.9388266...
        assert_eq!(hash_u32(0, 1, 0), 0xF056_F1B1);
        assert!((hash(0, 1, 0) - 0.938_826_66).abs() < 1e-7);

        // hash(10, 10, 11), a pixel of a sparkle: h = 0x1E44FCC3 (wrapped) -> 0x1E44C04A -> 0x479DC666 -> 0x479FFA88
        //   -> 0xD2F74E28 -> 0xD2F79CDF, over 2^32 = 0.8240907...
        assert_eq!(hash_u32(10, 10, 11), 0xD2F7_9CDF);
        assert!((hash(10, 10, 11) - 0.824_090_77).abs() < 1e-7);

        // hash(-1, 5, 7), with a negative number: h = 0x09EA6499 -> ... -> 0xEEA92E73, over 2^32 = 0.9322690...
        assert_eq!(hash_u32(-1, 5, 7), 0xEEA9_2E73);
        assert!((hash(-1, 5, 7) - 0.932_269).abs() < 1e-7);

        let mut highest = 0.0f32;

        for a in [0, 1, -1, 7, 335, -210, 694, 123_456_789, i32::MAX, i32::MIN] {
            for b in [0, 1, -1, 5, 16, -77, i32::MAX, i32::MIN] {
                for c in [0, 3, 4, 5, 7, 9, 11, -3, 2_000_000_011, i32::MIN] {
                    let expected = hash_in_64_bits(a, b, c);
                    let value = hash(a, b, c);

                    assert_eq!(hash_u32(a, b, c) as f64 / 4294967296.0, expected, "hash({a}, {b}, {c})");
                    assert!((0.0..1.0).contains(&value));
                    assert!(
                        (value as f64 - expected).abs() < 1.0 / 16_777_216.0,
                        "hash({a}, {b}, {c})"
                    );
                    highest = highest.max(value);
                }
            }
        }

        assert!(highest > 0.99);
        // the highest hash there is stays below 1
        assert!(((u32::MAX >> 8) as f32 / 16_777_216.0) < 1.0);
    }

    #[test]
    fn cuts_a_number_to_32_bits() {
        assert_eq!(to_int(3.9), 3);
        assert_eq!(to_int(-3.9), -3);
        assert_eq!(to_int(4294967296.0 + 4096.0), 4096);
        assert_eq!(to_int(f32::NAN), 0);
        assert_eq!(to_int(f32::INFINITY), -1);
    }

    #[test]
    fn works_out_the_basics() {
        assert_eq!(clamp01(1.5), 1.0);
        assert_eq!(clamp01(-0.5), 0.0);
        assert_eq!(clamp01(0.25), 0.25);
        assert_eq!(fract(2.75), 0.75);
        assert_eq!(fract(-0.25), 0.75);
        assert_eq!(triangle(0.25), 0.5);
        assert_eq!(triangle(0.75), 0.5);
        assert_eq!(triangle(1.5), 1.0);
        assert_eq!(smooth_step(0.0, 1.0, 0.5), 0.5);
        assert_eq!(smooth_step(0.0, 1.0, -1.0), 0.0);
        assert_eq!(smooth_step(0.0, 1.0, 2.0), 1.0);
        // a step of no width jumps at from
        assert_eq!(smooth_step(0.5, 0.5, 0.4), 0.0);
        assert_eq!(smooth_step(0.5, 0.5, 0.5), 1.0);
        assert_eq!(band(0.0, 0.25, 0.1), 1.0);
        assert_eq!(band(0.25, 0.25, 0.1), 0.5);
        assert_eq!(band(0.5, 0.25, 0.1), 0.0);
    }

    #[test]
    fn follows_the_sine_over_a_cycle() {
        assert_eq!(sin_cycle(0.0), 0.0);
        assert_eq!(sin_cycle(0.25), 1.0);
        assert_eq!(sin_cycle(0.5), 0.0);
        assert_eq!(sin_cycle(0.75), -1.0);
        assert_eq!(sin_cycle(1.0), 0.0);
        assert_eq!(sin_cycle(-0.25), -1.0);
        assert!(sin_cycle(f32::NAN).is_nan());
        assert!(sin_cycle(1e30).abs() <= 1.0);

        let mut largest = 0.0f64;

        for step in -4000..4000 {
            let position = step as f32 / 1000.0 + 0.000_37;
            let expected = (position as f64 * std::f64::consts::TAU).sin();

            largest = largest.max((sin_cycle(position) as f64 - expected).abs());
        }

        assert!(largest < 5e-7, "{largest}");
    }

    // the noise in doubles, at a cell and a place in it
    fn value_noise_in_doubles(xi: i32, xf: f64, yi: i32, yf: f64) -> f64 {
        let u = xf * xf * (3.0 - 2.0 * xf);
        let v = yf * yf * (3.0 - 2.0 * yf);
        let a = hash_in_64_bits(xi, yi, 7);
        let b = hash_in_64_bits(xi.wrapping_add(1), yi, 7);
        let c = hash_in_64_bits(xi, yi.wrapping_add(1), 7);
        let d = hash_in_64_bits(xi.wrapping_add(1), yi.wrapping_add(1), 7);

        a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
    }

    #[test]
    fn makes_the_same_noise_as_in_doubles() {
        for (x, y) in [
            (0.0f32, 0.0f32),
            (0.5, 0.5),
            (3.25, 7.75),
            (-2.5, 4.125),
            (1366.3, -12.7),
            (100.0, 100.0),
        ] {
            let expected = value_noise_in_doubles(
                x.floor() as i32,
                (x - x.floor()) as f64,
                y.floor() as i32,
                (y - y.floor()) as f64,
            );

            assert!((value_noise(x, y) as f64 - expected).abs() < 1e-6, "noise({x}, {y})");
        }

        // on the whole numbers the heights themselves
        assert_eq!(value_noise(3.0, 4.0), hash(3, 4, 7));

        // the fractal one: the same landscapes at 2.03 and 4.1 times the place
        for (x, y) in [(0.3f64, 0.7f64), (5.61, 2.2 * 4.748 + 1.5), (12.0, -3.3)] {
            let noise = |x: f64, y: f64| {
                value_noise_in_doubles(x.floor() as i32, x - x.floor(), y.floor() as i32, y - y.floor())
            };
            let expected = noise(x, y) * 0.55
                + noise(x * 2.03 + 17.1, y * 2.03 + 5.3) * 0.3
                + noise(x * 4.1 + 3.7, y * 4.1 + 11.9) * 0.15;

            assert!(
                (fractal_noise(x as f32, y as f32) as f64 - expected).abs() < 2e-5,
                "fractal noise({x}, {y})"
            );
        }
    }

    #[test]
    fn keeps_the_cell_of_the_noise_far_out() {
        // a place 2^40 cells out: the cell wraps around at 32 bits, the place in the cell is kept
        let x = Cycle::new((1 << 40) + 3, 0.25);
        let y = Cycle::new(-(1 << 40) + 7, 0.75);
        let expected = value_noise_in_doubles(3, 0.25, 7, 0.75);

        assert!((value_noise_at(x, y) as f64 - expected).abs() < 1e-6);
        assert_eq!(value_noise_at(x, y), value_noise(3.25, 7.75));
    }
}
