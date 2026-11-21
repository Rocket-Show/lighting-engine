//! Whole-number time and beats.
//!
//! Time is a count of nanoseconds in an `i64` (292 years), beats and tempos are 32.32 fixed-point numbers in an `i64`.
//! Everything that decides something (which step, which cycle, whether a fade has started) is worked out on these
//! whole numbers; a float only appears once a phase has been reduced to a value between 0 and 1.

/// Nanoseconds.
pub type Ns = i64;

/// A 32.32 fixed-point number: beats, beats per minute, cycles, a speed.
pub type Fix = i64;

pub const FIX_ONE: Fix = 1 << 32;
pub const NS_PER_US: i64 = 1_000;
pub const NS_PER_MS: i64 = 1_000_000;
pub const NS_PER_MINUTE: i64 = 60_000_000_000;

/// The tempo an effect following the beat falls back to: 120 beats per minute.
pub const DEFAULT_BPM: Fix = 120 * FIX_ONE;

fn saturate(value: i128) -> i64 {
    if value > i64::MAX as i128 {
        i64::MAX
    } else if value < i64::MIN as i128 {
        i64::MIN
    } else {
        value as i64
    }
}

/// a product divided by a divisor, rounded to the nearest whole number (halves away from zero)
fn divide_rounded(product: i128, divisor: i128) -> i64 {
    if divisor == 0 {
        return 0;
    }

    let negative = (product < 0) != (divisor < 0);
    let product = product.unsigned_abs();
    let divisor = divisor.unsigned_abs();

    // A factor of two which both have doesn't change the quotient rounded half up, as long as the divisor stays whole:
    // (2p + d) / 2d can't be a whole number for an odd d, so rounding p by d comes to the same. Taken out, the numbers
    // often fit into 64 bits: the place of a fixture in a chase is its step times whole cycles in 32.32 over the steps
    // in 32.32.
    let shift = product.trailing_zeros().min(divisor.trailing_zeros());
    let (product, divisor) = (product >> shift, divisor >> shift);

    // The same division in 64 bits where both fit into them: a processor of 32 bits takes many times as long for one
    // of 128 bits, and this is asked for a fixture, an effect and a frame at a time.
    let magnitude = match (u64::try_from(product), u64::try_from(divisor)) {
        (Ok(product), Ok(divisor)) if product <= u64::MAX - divisor / 2 => ((product + divisor / 2) / divisor) as u128,
        _ => product.saturating_add(divisor / 2) / divisor,
    }
    .min(i64::MAX as u128) as i128;

    saturate(if negative { -magnitude } else { magnitude })
}

/// `a * b / c`, rounded to the nearest whole number (halves away from zero). 0 for a division by zero.
pub fn mul_div(a: i64, b: i64, c: i64) -> i64 {
    divide_rounded(a as i128 * b as i128, c as i128)
}

/// `a * b / (c * d)`, rounded to the nearest whole number. 0 for a division by zero.
pub fn mul_div2(a: i64, b: i64, c: i64, d: i64) -> i64 {
    divide_rounded(a as i128 * b as i128, c as i128 * d as i128)
}

/// A whole number of nanoseconds times a fixed-point factor.
pub fn scale(value: i64, factor: Fix) -> i64 {
    mul_div(value, factor, FIX_ONE)
}

/// The floor of a division (towards minus infinity), 0 for a division by zero.
pub fn div_floor(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0;
    }

    let quotient = a.wrapping_div(b);

    if (a.wrapping_rem(b) != 0) && ((a < 0) != (b < 0)) {
        quotient.wrapping_sub(1)
    } else {
        quotient
    }
}

/// The rest of a division, never negative for a positive divisor. 0 for a division by zero.
pub fn rem_floor(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0;
    }

    let rest = a.wrapping_rem(b);

    if rest != 0 && ((rest < 0) != (b < 0)) {
        rest.wrapping_add(b)
    } else {
        rest
    }
}

/// A ratio of two whole numbers as a float.
pub fn ratio(a: i64, b: i64) -> f32 {
    if b == 0 {
        return 0.0;
    }

    // the quotient in 32.32 fixed-point first: the precision of the result doesn't depend on how large both are
    let quotient = match (u64::try_from(a), u64::try_from(b)) {
        // The same quotient by a long division in two steps of 16 bits, each in 64 bits, where the numbers allow it
        // (a part of a whole below 2^48, as a place inside the period of an effect is): a processor of 32 bits takes
        // many times as long for a division of 128 bits.
        (Ok(a), Ok(b)) if a < b && b < 1 << 48 => {
            let first = (a << 16) / b;
            let rest = (a << 16) % b;
            let second = (rest << 16) / b;

            ((first << 16) | second) as i128
        }
        _ => ((a as i128) << 32) / b as i128,
    };
    saturate(quotient) as f32 * (1.0 / 4_294_967_296.0)
}

/// How long a beat lasts at a tempo, in nanoseconds.
pub fn beat_ns(bpm: Fix) -> Ns {
    let bpm = if bpm > 0 { bpm } else { DEFAULT_BPM };
    mul_div(NS_PER_MINUTE, FIX_ONE, bpm)
}

/// How long a number of beats lasts at a tempo.
pub fn beats_to_ns(beats: Fix, bpm: Fix) -> Ns {
    let bpm = if bpm > 0 { bpm } else { DEFAULT_BPM };
    mul_div(beats, NS_PER_MINUTE, bpm)
}

/// The beats passing in a time at a tempo.
pub fn ns_to_beats(ns: Ns, bpm: Fix) -> Fix {
    mul_div(ns, bpm, NS_PER_MINUTE)
}

/// The whole part of a fixed-point number, towards minus infinity.
pub fn fix_floor(value: Fix) -> i64 {
    value >> 32
}

/// A fixed-point number as a float.
pub fn fix_to_f32(value: Fix) -> f32 {
    value as f32 * (1.0 / 4_294_967_296.0)
}

/// A float as a fixed-point number (0 for anything which is not a number).
pub fn fix_from_f32(value: f32) -> Fix {
    if value.is_nan() {
        return 0;
    }

    (value * 4_294_967_296.0) as i64
}

/// The beats of the tempo playing: where the clock is, how fast it runs, and a shift it still catches up.
///
/// The clock keeps its position as a fixed-point number of beats and re-anchors on every change, so its precision
/// never depends on how long it has run.
#[derive(Clone, Copy, Debug)]
pub struct BeatClock {
    anchor_beats: Fix,
    anchor_ns: Ns,
    bpm: Fix,
    shift_beats: Fix,
    shift_end_ns: Ns,
}

impl BeatClock {
    pub const fn new(now: Ns) -> Self {
        BeatClock {
            anchor_beats: 0,
            anchor_ns: now,
            bpm: DEFAULT_BPM,
            shift_beats: 0,
            shift_end_ns: 0,
        }
    }

    /// the share of the shift made at a moment, as a numerator and a denominator
    fn progress(&self, now: Ns) -> (i64, i64) {
        let shift_ns = self.shift_end_ns - self.anchor_ns;

        if shift_ns <= 0 {
            return (1, 1);
        }

        let elapsed = now - self.anchor_ns;
        (elapsed.clamp(0, shift_ns), shift_ns)
    }

    /// Where the clock is at a moment, in beats.
    pub fn beats(&self, now: Ns) -> Fix {
        let mut beats = self
            .anchor_beats
            .wrapping_add(ns_to_beats(now - self.anchor_ns, self.bpm));

        if self.shift_beats != 0 {
            let (done, of) = self.progress(now);
            beats = beats.wrapping_add(mul_div(self.shift_beats, done, of));
        }

        beats
    }

    pub fn bpm(&self) -> Fix {
        self.bpm
    }

    fn rebase(&mut self, now: Ns) {
        let beats = self.beats(now);

        if self.shift_beats != 0 {
            let (done, of) = self.progress(now);
            self.shift_beats = if done >= of {
                0
            } else {
                self.shift_beats - mul_div(self.shift_beats, done, of)
            };
        }

        self.anchor_beats = beats;
        self.anchor_ns = now;
    }

    /// Run at another tempo from a moment on, from where the clock is.
    pub fn set_bpm(&mut self, bpm: Fix, now: Ns) {
        if bpm <= 0 || bpm == self.bpm {
            return;
        }

        self.rebase(now);
        self.bpm = bpm;
    }

    fn shift(&mut self, beats: Fix, over_beats: Fix, now: Ns) {
        self.anchor_beats = self.beats(now);
        self.anchor_ns = now;
        self.shift_beats = beats;

        let over = over_beats.max(2 * beats.abs());
        self.shift_end_ns = now + beats_to_ns(over, self.bpm);
    }

    fn round(beats: Fix) -> Fix {
        // to the nearest whole beat, halves up (like Math.round)
        ((beats.wrapping_add(FIX_ONE / 2)) >> 32) << 32
    }

    /// the first whole number of bars at or above a number of beats (a hair below a bar still counts as that bar)
    fn bar_start(beats: Fix, bar: i64) -> Fix {
        // ceil(beats / bar - 1e-9) * bar
        let epsilon = 4 * bar; // about 1e-9 of a bar, in 2^-32 beats
        let whole_bar = bar * FIX_ONE;
        let bars = div_floor(beats - epsilon + whole_bar - 1, whole_bar);
        bars * whole_bar
    }

    /// A beat falls on a moment (e.g. a beat tapped): the clock moves onto the closest whole beat over the next beat.
    pub fn align_beat(&mut self, now: Ns) {
        let beats = self.beats(now);
        self.shift(Self::round(beats) - beats, FIX_ONE, now);
    }

    /// The first beat of a bar falls on a moment.
    pub fn beat_one(&mut self, beats_per_bar: i32, jump: bool, now: Ns) {
        let bar = beats_per_bar.max(1) as i64;
        let beats = self.beats(now);

        if jump {
            self.set_beats(Self::bar_start(beats, bar), now);
            return;
        }

        let closest = Self::round(beats);
        let bar_start = Self::bar_start(closest, bar);
        self.anchor_beats = beats + (bar_start - closest);
        self.anchor_ns = now;
        self.shift_beats = 0;
        self.shift(closest - beats, FIX_ONE, now);
    }

    /// Where the clock is at a moment, at once (e.g. a song position of a MIDI clock).
    pub fn set_beats(&mut self, beats: Fix, now: Ns) {
        self.anchor_beats = beats;
        self.anchor_ns = now;
        self.shift_beats = 0;
    }

    /// The beat of the bar the clock is in at a moment, from 0.
    pub fn beat_of_bar(&self, beats_per_bar: i32, now: Ns) -> i32 {
        let bar = beats_per_bar.max(1) as i64;
        let beat = fix_floor(self.beats(now) + 4);
        rem_floor(beat, bar) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the divisions as they are written down, in 128 bits throughout
    fn divide_rounded_wide(product: i128, divisor: i128) -> i64 {
        if divisor == 0 {
            return 0;
        }

        let negative = (product < 0) != (divisor < 0);
        let magnitude = (product.unsigned_abs().saturating_add(divisor.unsigned_abs() / 2) / divisor.unsigned_abs())
            .min(i64::MAX as u128) as i128;

        saturate(if negative { -magnitude } else { magnitude })
    }

    fn ratio_wide(a: i64, b: i64) -> f32 {
        if b == 0 {
            return 0.0;
        }

        saturate(((a as i128) << 32) / b as i128) as f32 * (1.0 / 4_294_967_296.0)
    }

    #[test]
    fn the_short_ways_divide_like_the_long_one() {
        let edges = [
            0i64,
            1,
            2,
            3,
            999,
            1_000_000,
            2_000_000_000,
            (1 << 31) - 1,
            1 << 31,
            (1 << 32) - 1,
            1 << 32,
            (1 << 32) + 1,
            (1 << 47) + 12_345,
            (1 << 48) - 1,
            1 << 48,
            (1 << 48) + 1,
            1 << 62,
            i64::MAX - 1,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
            -1,
            -2,
            -1_000_000_007,
            -(1 << 40),
        ];
        // a walk through numbers of every size which looks random, the same every time
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // (of a random length, so small numbers come as often as large ones)
            let bits = (state >> 58) as u32 % 64;
            ((state >> 1) as i64) >> bits
        };
        let mut numbers = edges.to_vec();

        for _ in 0..400 {
            numbers.push(next());
        }

        let mut compared = 0u32;

        for &a in &numbers {
            for &b in &numbers {
                assert_eq!(ratio(a, b).to_bits(), ratio_wide(a, b).to_bits(), "ratio({a}, {b})");

                for &c in &edges {
                    let product = a as i128 * b as i128;

                    assert_eq!(
                        mul_div(a, b, c),
                        divide_rounded_wide(product, c as i128),
                        "mul_div({a}, {b}, {c})"
                    );
                    assert_eq!(
                        mul_div2(a, b, c, FIX_ONE),
                        divide_rounded_wide(product, c as i128 * FIX_ONE as i128),
                        "mul_div2({a}, {b}, {c}, one)"
                    );
                    compared += 1;
                }
            }
        }

        assert_eq!(compared, 425 * 425 * 25);

        // the place of a fixture in a chase: its step times the cycles (32.32) times the period, over the steps (32.32)
        for &cycles in &[FIX_ONE, 2 * FIX_ONE, FIX_ONE / 2, 3 * FIX_ONE / 7, 1] {
            for &length in &[1i64, 2_000_000_000, 3_600_000_000_000, 1 << 50] {
                for &steps in &[1i64, 3, 512, 513, 100_000] {
                    for step in [0i64, 1, 2, 255, 511, 99_999] {
                        assert_eq!(
                            mul_div2(step * cycles, length, steps, FIX_ONE),
                            divide_rounded_wide(
                                (step * cycles) as i128 * length as i128,
                                steps as i128 * FIX_ONE as i128
                            ),
                            "the chase: step {step} of {steps}, {cycles} cycles of {length}"
                        );
                    }
                }
            }
        }

        // the place inside a period, as an effect asks for it: a rest below its divisor
        for &length in &[
            1i64,
            7,
            1_000_000,
            2_000_000_000,
            3_600_000_000_000,
            (1 << 48) - 1,
            1 << 48,
            1 << 50,
        ] {
            for step in 0..2000i64 {
                let rest = (length / 2000).max(1) * step % length;

                assert_eq!(
                    ratio(rest, length).to_bits(),
                    ratio_wide(rest, length).to_bits(),
                    "ratio({rest}, {length})"
                );
            }

            assert_eq!(
                ratio(length - 1, length).to_bits(),
                ratio_wide(length - 1, length).to_bits()
            );
        }
    }

    #[test]
    fn divisions_round_and_floor() {
        assert_eq!(mul_div(7, 1, 2), 4);
        assert_eq!(mul_div(-7, 1, 2), -4);
        assert_eq!(mul_div(7, 1, -2), -4);
        assert_eq!(mul_div(1, 1, 0), 0);
        assert_eq!(div_floor(-1, 4), -1);
        assert_eq!(rem_floor(-1, 4), 3);
        assert_eq!(div_floor(8, 4), 2);
        assert_eq!(rem_floor(8, 4), 0);
    }

    #[test]
    fn a_beat_at_120_lasts_half_a_second() {
        assert_eq!(beat_ns(DEFAULT_BPM), 500_000_000);
        assert_eq!(beats_to_ns(4 * FIX_ONE, DEFAULT_BPM), 2_000_000_000);
        assert_eq!(ns_to_beats(1_000_000_000, DEFAULT_BPM), 2 * FIX_ONE);
    }

    #[test]
    fn the_clock_keeps_its_place_through_a_change_of_tempo() {
        let mut clock = BeatClock::new(0);
        assert_eq!(clock.beats(1_000_000_000), 2 * FIX_ONE);
        clock.set_bpm(60 * FIX_ONE, 1_000_000_000);
        assert_eq!(clock.beats(1_000_000_000), 2 * FIX_ONE);
        assert_eq!(clock.beats(2_000_000_000), 3 * FIX_ONE);
    }

    #[test]
    fn a_tap_moves_the_clock_onto_the_beat_over_a_beat() {
        let mut clock = BeatClock::new(0);
        // 0.2 beats late
        clock.align_beat(600_000_000);
        assert_eq!(
            clock.beats(600_000_000),
            FIX_ONE + FIX_ONE / 5 - (FIX_ONE + FIX_ONE / 5 - clock.beats(600_000_000))
        );
        let later = clock.beats(600_000_000 + 500_000_000);
        // a beat later the shift of -0.2 beats is made
        assert!((later - 2 * FIX_ONE).abs() < 16);
    }

    #[test]
    fn beat_one_jumps_to_the_next_bar() {
        let mut clock = BeatClock::new(0);
        clock.beat_one(4, true, 750_000_000);
        assert_eq!(clock.beats(750_000_000), 4 * FIX_ONE);
        assert_eq!(clock.beat_of_bar(4, 750_000_000), 0);
        assert_eq!(clock.beat_of_bar(4, 1_250_000_000), 1);
    }
}
