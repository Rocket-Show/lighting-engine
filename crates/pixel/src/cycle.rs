//! A number of periods of an effect which keeps its whole part exact.
//!
//! The moment of a pattern is the number of periods run so far. After hours that is a huge number, and a single
//! precision float of it has no part of a period left. A [`Cycle`] keeps the whole periods as an integer and the part
//! of the running one as a float between 0 and 1. Every operation the patterns need (times a factor, plus an offset,
//! the other way round) works on the integer exactly and only rounds the part.

/// A number of periods: `whole + frac`, with `0 <= frac < 1`.
///
/// The fields are public, so a caller can build one from an integer phase without any arithmetic. Every method
/// accepts a `frac` outside of its range (it is brought back into it) and never panics, also on a NaN.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    pub whole: i64,
    pub frac: f32,
}

// (`neg`, `add` and `mul` are methods of their own, not the operators: they take a float, wrap around and bring
// the part back into its range, and a call reads the same with and without the traits in scope)
#[allow(clippy::should_implement_trait)]
impl Cycle {
    pub const ZERO: Cycle = Cycle { whole: 0, frac: 0.0 };

    /// `whole + frac` for any `frac`: its whole periods are moved to `whole`. A `frac` which is not a number or not
    /// finite counts as 0.
    pub fn new(whole: i64, frac: f32) -> Cycle {
        if (0.0..1.0).contains(&frac) {
            // (+ 0.0 turns a negative zero into a positive one)
            return Cycle {
                whole,
                frac: frac + 0.0,
            };
        }

        if !frac.is_finite() {
            return Cycle { whole, frac: 0.0 };
        }

        let floor = libm::floorf(frac);
        let rest = frac - floor;
        let whole = whole.wrapping_add(floor as i64);

        // a tiny negative part rounds up to a whole period
        if rest < 1.0 {
            Cycle { whole, frac: rest }
        } else {
            Cycle {
                whole: whole.wrapping_add(1),
                frac: 0.0,
            }
        }
    }

    /// A whole number of periods.
    pub const fn from_whole(whole: i64) -> Cycle {
        Cycle { whole, frac: 0.0 }
    }

    /// The periods of a float (exactly, as far as they fit an `i64`).
    pub fn from_f32(value: f32) -> Cycle {
        if !value.is_finite() {
            return Cycle::ZERO;
        }

        let floor = libm::floorf(value);

        Cycle::new(floor as i64, value - floor)
    }

    /// The periods of an `f64`, the part rounded to single precision. For tests and for the compiler.
    #[cfg(any(test, feature = "std"))]
    pub fn from_f64(value: f64) -> Cycle {
        if !value.is_finite() {
            return Cycle::ZERO;
        }

        let floor = value.floor();

        Cycle::new(floor as i64, (value - floor) as f32)
    }

    /// The part of the running period, between 0 and 1.
    pub fn fract(self) -> f32 {
        Cycle::new(self.whole, self.frac).frac
    }

    /// The whole periods.
    pub fn floor(self) -> i64 {
        Cycle::new(self.whole, self.frac).whole
    }

    /// The whole periods as a 32 bit integer, wrapped around.
    pub fn floor_i32(self) -> i32 {
        self.floor() as i32
    }

    /// The periods as one float. Loses the part of the period once the whole periods are many: only for values which
    /// are known to be small.
    pub fn to_f32(self) -> f32 {
        self.whole as f32 + self.frac
    }

    /// The periods modulo 2, between 0 and 2, exactly. A place in a palette only counts modulo 2 (see
    /// [`crate::palette::color_of`]).
    pub fn mod2(self) -> f32 {
        let cycle = Cycle::new(self.whole, self.frac);

        (cycle.whole & 1) as f32 + cycle.frac
    }

    /// `-self`.
    pub fn neg(self) -> Cycle {
        Cycle::new(self.whole.wrapping_neg(), -self.frac)
    }

    /// `self + value`. The whole part of the value is added as an integer.
    pub fn add(self, value: f32) -> Cycle {
        if !value.is_finite() {
            return Cycle::new(self.whole, self.frac);
        }

        let floor = libm::floorf(value);

        Cycle::new(self.whole.wrapping_add(floor as i64), self.frac + (value - floor))
    }

    /// `self + other`.
    pub fn add_cycle(self, other: Cycle) -> Cycle {
        Cycle::new(self.whole.wrapping_add(other.whole), self.frac + other.frac)
    }

    /// `self * factor`, exact for the whole part: a float is a whole number times a power of two, so the product
    /// with the whole periods is worked out in integers and split into its whole periods and the rest. Only the
    /// product with the part of the period is rounded. A whole part beyond 64 bits wraps around.
    pub fn mul(self, factor: f32) -> Cycle {
        if !factor.is_finite() {
            return Cycle::ZERO;
        }

        // factor = mantissa * 2^exponent, exactly
        let bits = factor.to_bits();
        let biased = ((bits >> 23) & 0xff) as i32;
        let fraction = (bits & 0x007f_ffff) as i64;
        let (magnitude, exponent) = if biased == 0 {
            (fraction, -149)
        } else {
            (fraction | 0x0080_0000, biased - 150)
        };
        let mantissa = if bits >> 31 != 0 { -magnitude } else { magnitude };
        let rounded = self.frac * factor;

        if exponent >= 0 {
            let whole = if exponent < 64 {
                self.whole.wrapping_mul(mantissa).wrapping_shl(exponent as u32)
            } else {
                0
            };

            return Cycle::new(whole, rounded);
        }

        let shift = exponent.unsigned_abs();

        // the usual case in 64 bits: the periods of a show fit 32 bits by far
        if let Ok(small) = i32::try_from(self.whole) {
            if shift < 63 {
                let product = (small as i64).wrapping_mul(mantissa);
                let whole = product >> shift;
                let rest = product.wrapping_sub(whole.wrapping_shl(shift)) as u64;

                return Cycle::new(whole, libm::scalbnf(rest as f32, exponent)).add_part(rounded);
            }
        }

        let product = (self.whole as i128).wrapping_mul(mantissa as i128);

        if shift < 100 {
            let whole = product >> shift;
            let rest = product.wrapping_sub(whole.wrapping_shl(shift));

            Cycle::new(whole as i64, libm::scalbnf(rest as f32, exponent)).add_part(rounded)
        } else {
            // less than a period in all (the product is below 2^87)
            Cycle::new(0, libm::scalbnf(product as f32, exponent)).add_part(rounded)
        }
    }

    /// `self * numerator / denominator`, exact for the whole part. For the factors the patterns write as decimals
    /// (`cycle * 0.7` is `mul_ratio(7, 10)`): a decimal like 0.7 is no float, and the difference would add up to whole
    /// periods over the hours. A denominator below 1 counts as 1. A whole part beyond 64 bits wraps around.
    pub fn mul_ratio(self, numerator: i32, denominator: i32) -> Cycle {
        let denominator = if denominator > 0 { denominator } else { 1 };
        let rounded = self.frac * (numerator as f32 / denominator as f32);

        if denominator == 1 {
            return Cycle::new(self.whole.wrapping_mul(numerator as i64), rounded);
        }

        // the usual case with the division of the processor: the periods of a show fit 32 bits by far
        if let Ok(small) = i32::try_from(self.whole) {
            let product = small as i64 * numerator as i64;

            let (whole, rest) = if let Ok(product) = i32::try_from(product) {
                (
                    product.checked_div_euclid(denominator).unwrap_or(0) as i64,
                    product.checked_rem_euclid(denominator).unwrap_or(0),
                )
            } else {
                (
                    product.checked_div_euclid(denominator as i64).unwrap_or(0),
                    product.checked_rem_euclid(denominator as i64).unwrap_or(0) as i32,
                )
            };

            return Cycle::new(whole, rest as f32 / denominator as f32).add_part(rounded);
        }

        let product = self.whole as i128 * numerator as i128;
        let whole = product.checked_div_euclid(denominator as i128).unwrap_or(0) as i64;
        let rest = product.checked_rem_euclid(denominator as i128).unwrap_or(0) as i32;

        Cycle::new(whole, rest as f32 / denominator as f32).add_part(rounded)
    }

    /// `self * numerator / 2^32`, exact for the whole part: times a number between 0 and 1 which is known by its 32
    /// bits, like a hash ([`crate::math::hash_u32`]), where a float of it would only hold 24 of them.
    pub fn mul_u32_fraction(self, numerator: u32) -> Cycle {
        const SCALE: f32 = 1.0 / 4_294_967_296.0;
        let rounded = self.frac * (numerator as f32 * SCALE);

        // the usual case in 64 bits: the periods of a show fit 32 bits by far
        let (whole, rest) = if let Ok(small) = i32::try_from(self.whole) {
            let product = small as i64 * numerator as i64;

            (product >> 32, product as u32)
        } else {
            let product = self.whole as i128 * numerator as i128;

            ((product >> 32) as i64, product as u32)
        };

        Cycle::new(whole, rest as f32 * SCALE).add_part(rounded)
    }

    // plus a part which is not split into its whole periods first (the rounded product of a part and a factor)
    fn add_part(self, part: f32) -> Cycle {
        Cycle::new(self.whole, self.frac + part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(cycle: Cycle) -> f64 {
        cycle.whole as f64 + cycle.frac as f64
    }

    #[test]
    fn brings_the_part_into_its_range() {
        assert_eq!(Cycle::new(3, 0.25), Cycle { whole: 3, frac: 0.25 });
        assert_eq!(Cycle::new(3, 1.25), Cycle { whole: 4, frac: 0.25 });
        assert_eq!(Cycle::new(3, -0.25), Cycle { whole: 2, frac: 0.75 });
        assert_eq!(Cycle::new(3, 1.0), Cycle { whole: 4, frac: 0.0 });
        assert_eq!(Cycle::new(3, -1e-12), Cycle { whole: 3, frac: 0.0 });
        assert_eq!(Cycle::new(3, f32::NAN), Cycle { whole: 3, frac: 0.0 });
        assert_eq!(Cycle::new(3, f32::INFINITY), Cycle { whole: 3, frac: 0.0 });
        assert!(Cycle::new(0, -0.0).frac.is_sign_positive());
        assert_eq!(Cycle::new(i64::MAX, 1.5).whole, i64::MIN);
    }

    #[test]
    fn reads_floats() {
        assert_eq!(Cycle::from_f32(2.75), Cycle { whole: 2, frac: 0.75 });
        assert_eq!(Cycle::from_f32(-2.75), Cycle { whole: -3, frac: 0.25 });
        assert_eq!(Cycle::from_f64(1234.5), Cycle { whole: 1234, frac: 0.5 });
        assert_eq!(Cycle::from_f64(-0.25), Cycle { whole: -1, frac: 0.75 });
        assert_eq!(
            Cycle::from_f64(1e12 + 0.125),
            Cycle {
                whole: 1_000_000_000_000,
                frac: 0.125
            }
        );
        assert_eq!(Cycle::from_f64(f64::NAN), Cycle::ZERO);
        assert_eq!(Cycle::from_f32(f32::NAN), Cycle::ZERO);
        // a part which rounds up to a whole period
        assert_eq!(Cycle::from_f64(2.999999999999), Cycle { whole: 3, frac: 0.0 });
    }

    #[test]
    fn floors_and_wraps_around_at_32_bits() {
        assert_eq!(Cycle::from_f64(-0.5).floor(), -1);
        assert_eq!(Cycle::from_f64(4294967301.5).floor_i32(), 5);
        assert_eq!(Cycle::from_f64(2147483648.0).floor_i32(), i32::MIN);
        assert_eq!(Cycle::from_f64(7.25).fract(), 0.25);
        assert_eq!(Cycle { whole: 7, frac: 1.25 }.floor(), 8);
    }

    #[test]
    fn counts_modulo_two() {
        assert_eq!(Cycle::new(3, 0.25).mod2(), 1.25);
        assert_eq!(Cycle::new(-3, 0.25).mod2(), 1.25);
        assert_eq!(Cycle::new(-4, 0.5).mod2(), 0.5);
        assert_eq!(Cycle::new(1_000_000_000_001, 0.5).mod2(), 1.5);
    }

    #[test]
    fn adds_and_negates() {
        assert_eq!(Cycle::new(3, 0.25).neg(), Cycle { whole: -4, frac: 0.75 });
        assert_eq!(Cycle::new(3, 0.0).neg(), Cycle { whole: -3, frac: 0.0 });
        assert_eq!(Cycle::new(3, 0.25).add(2.5), Cycle { whole: 5, frac: 0.75 });
        assert_eq!(Cycle::new(3, 0.75).add(2.5), Cycle { whole: 6, frac: 0.25 });
        assert_eq!(Cycle::new(3, 0.25).add(-2.5), Cycle { whole: 0, frac: 0.75 });
        assert_eq!(Cycle::new(3, 0.25).add(f32::NAN), Cycle { whole: 3, frac: 0.25 });
        assert_eq!(
            Cycle::new(3, 0.75).add_cycle(Cycle::new(-1, 0.5)),
            Cycle { whole: 3, frac: 0.25 }
        );
        // the part survives a huge whole part
        assert_eq!(
            Cycle::new(1 << 50, 0.25).add(1000.5),
            Cycle {
                whole: (1 << 50) + 1000,
                frac: 0.75
            }
        );
    }

    #[test]
    fn multiplies_the_whole_part_exactly() {
        assert_eq!(Cycle::new(3, 0.5).mul(2.0), Cycle { whole: 7, frac: 0.0 });
        assert_eq!(Cycle::new(3, 0.5).mul(0.5), Cycle { whole: 1, frac: 0.75 });
        assert_eq!(Cycle::new(3, 0.5).mul(-0.5), Cycle { whole: -2, frac: 0.25 });
        assert_eq!(Cycle::new(3, 0.5).mul(0.0), Cycle::ZERO);
        assert_eq!(Cycle::new(3, 0.5).mul(f32::NAN), Cycle::ZERO);

        // against the same product in integers of 128 bits, for wholes far beyond what a float holds
        let factors = [
            0.3f32,
            1.37,
            0.6 + 0.9 * 0.123,
            2.2,
            1e-3,
            123.456,
            -0.7,
            3e-20,
            5e9,
            1e-40,
            f32::MIN_POSITIVE,
        ];
        let wholes = [
            0i64,
            1,
            3,
            -5,
            1_000_000_003,
            (1 << 30) + 3,
            86_400_000_000,
            -86_400_000_001,
            1 << 52,
            i32::MAX as i64,
            i32::MIN as i64,
        ];

        for factor in factors {
            for whole in wholes {
                for frac in [0.0f32, 0.3, 0.999] {
                    let product = Cycle::new(whole, frac).mul(factor);

                    // the exact periods: whole * factor as a fraction over 2^160, plus the rounded part
                    let exact = exact_product(whole, factor);
                    let expected = (exact.1 + frac as f64 * factor as f64).rem_euclid(1.0);
                    let carried = (exact.1 + frac as f64 * factor as f64).div_euclid(1.0) as i128;
                    let difference = (product.frac as f64 - expected).abs();
                    let difference = difference.min(1.0 - difference);
                    let scale = (frac as f64 * factor as f64).abs().max(1.0);

                    assert!(
                        difference < 3e-7 * scale,
                        "{whole} * {factor} + {frac}: {product:?} != {expected}"
                    );

                    // the whole periods, unless the part sits on the edge of one (or they left 64 bits, or the
                    // rounded product with the part is too big to tell)
                    if expected > 1e-4 && expected < 1.0 - 1e-4 && scale < 200.0 {
                        if let Ok(expected_whole) = i64::try_from(exact.0 + carried) {
                            assert_eq!(product.whole, expected_whole, "{whole} * {factor} + {frac}");
                        }
                    }
                }
            }
        }
    }

    // whole * factor exactly: its whole periods and the rest as a double
    fn exact_product(whole: i64, factor: f32) -> (i128, f64) {
        let bits = factor.to_bits();
        let biased = ((bits >> 23) & 0xff) as i32;
        let fraction = (bits & 0x007f_ffff) as i128;
        let (magnitude, exponent) = if biased == 0 {
            (fraction, -149)
        } else {
            (fraction | 0x0080_0000, biased - 150)
        };
        let mantissa = if bits >> 31 != 0 { -magnitude } else { magnitude };
        let product = whole as i128 * mantissa;

        if exponent >= 0 {
            (product << exponent, 0.0)
        } else if -exponent < 120 {
            let floor = product >> -exponent;
            let rest = product - (floor << -exponent);
            (floor, rest as f64 / 2f64.powi(-exponent))
        } else {
            let part = product as f64 * 2f64.powi(exponent);
            (part.floor() as i128, part - part.floor())
        }
    }

    #[test]
    fn multiplies_by_a_ratio_exactly() {
        assert_eq!(Cycle::new(3, 0.5).mul_ratio(2, 1), Cycle { whole: 7, frac: 0.0 });
        assert_eq!(Cycle::new(5, 0.0).mul_ratio(1, 5), Cycle { whole: 1, frac: 0.0 });
        assert_eq!(Cycle::new(-5, 0.0).mul_ratio(1, 5), Cycle { whole: -1, frac: 0.0 });
        assert_eq!(Cycle::new(10, 0.0).mul_ratio(-7, 10), Cycle { whole: -7, frac: 0.0 });
        assert_eq!(Cycle::new(3, 0.0).mul_ratio(1, 4), Cycle { whole: 0, frac: 0.75 });
        assert_eq!(Cycle::new(3, 0.0).mul_ratio(1, 0), Cycle { whole: 3, frac: 0.0 });

        for (numerator, denominator) in [
            (1, 5),
            (7, 10),
            (-7, 10),
            (11, 5),
            (-12, 5),
            (203, 100),
            (41, 10),
            (27, 100),
            (17, 100),
            (137, 100),
            (1, 2),
            (2, 1),
        ] {
            for whole in [
                0i64,
                1,
                3,
                -5,
                99,
                -101,
                1_000_000_003,
                -1_000_000_003,
                86_400_000_000,
                1 << 52,
                -(1 << 61),
                i32::MAX as i64,
                i32::MIN as i64,
            ] {
                for frac in [0.0f32, 0.3, 0.999] {
                    let product = Cycle::new(whole, frac).mul_ratio(numerator, denominator);
                    let exact = whole as i128 * numerator as i128;
                    let part = exact.rem_euclid(denominator as i128) as f64 / denominator as f64
                        + frac as f64 * numerator as f64 / denominator as f64;
                    let expected = part.rem_euclid(1.0);
                    let difference = (product.frac as f64 - expected).abs();
                    let difference = difference.min(1.0 - difference);

                    assert!(
                        difference < 3e-7,
                        "{whole} * {numerator} / {denominator} + {frac}: {product:?} != {expected}"
                    );

                    if expected > 1e-6 && expected < 1.0 - 1e-6 {
                        let expected_whole = exact.div_euclid(denominator as i128) + part.div_euclid(1.0) as i128;

                        if let Ok(expected_whole) = i64::try_from(expected_whole) {
                            assert_eq!(
                                product.whole, expected_whole,
                                "{whole} * {numerator} / {denominator} + {frac}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn multiplies_by_the_32_bits_of_a_hash_exactly() {
        assert_eq!(Cycle::new(3, 0.5).mul_u32_fraction(0), Cycle::ZERO);
        assert_eq!(
            Cycle::new(3, 0.5).mul_u32_fraction(1 << 31),
            Cycle { whole: 1, frac: 0.75 }
        );
        assert_eq!(
            Cycle::new(-3, 0.0).mul_u32_fraction(1 << 31),
            Cycle { whole: -2, frac: 0.5 }
        );

        for numerator in [1u32, 3, 0x12B2_6953, 0xF056_F1B1, 0x8000_0001, u32::MAX] {
            for whole in [
                0i64,
                1,
                3,
                -5,
                1_000_000_003,
                -1_000_000_003,
                86_400_000_000,
                -(1 << 52),
                i32::MAX as i64,
                i32::MIN as i64,
            ] {
                for frac in [0.0f32, 0.3, 0.999] {
                    let product = Cycle::new(whole, frac).mul_u32_fraction(numerator);
                    let exact = whole as i128 * numerator as i128;
                    let part =
                        exact.rem_euclid(1 << 32) as f64 / 4294967296.0 + frac as f64 * numerator as f64 / 4294967296.0;
                    let expected = part.rem_euclid(1.0);
                    let difference = (product.frac as f64 - expected).abs();
                    let difference = difference.min(1.0 - difference);

                    assert!(
                        difference < 3e-7,
                        "{whole} * {numerator} / 2^32 + {frac}: {product:?} != {expected}"
                    );

                    if expected > 1e-6 && expected < 1.0 - 1e-6 {
                        assert_eq!(
                            product.whole as i128,
                            exact.div_euclid(1 << 32) + part.div_euclid(1.0) as i128,
                            "{whole} * {numerator} / 2^32 + {frac}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn keeps_the_part_after_a_billion_periods() {
        // what a float of the periods would lose
        let near = Cycle::new(3, 0.3);
        let far = Cycle::new(3 + 1_000_000_000, 0.3);

        assert_eq!(near.fract(), far.fract());
        // a billion periods are a whole number of tenths, fifths and hundredths: the part stays the same bit for bit
        for (numerator, denominator) in [
            (2, 1),
            (7, 10),
            (1, 2),
            (11, 5),
            (-12, 5),
            (1, 5),
            (27, 100),
            (1, 4),
            (17, 100),
            (203, 100),
            (41, 10),
        ] {
            assert_eq!(
                near.mul_ratio(numerator, denominator).frac,
                far.mul_ratio(numerator, denominator).frac
            );
        }

        assert_eq!(near.neg().add(0.4).frac, far.neg().add(0.4).frac);
        assert_eq!(near.mul_ratio(1, 5).mod2(), far.mul_ratio(1, 5).mod2());
        assert!((value(far.mul_ratio(11, 5)) - 2_200_000_007.26).abs() < 1e-6);
    }
}
