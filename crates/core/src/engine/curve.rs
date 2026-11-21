//! The timing of an effect (a curve, a movement and a pixel effect all run on it) and the shape of a curve.
//!
//! Where an effect is in its period is worked out on whole nanoseconds: the time less its phase, folded into the
//! period, and only then turned into a float between 0 and 1. A frame is as precise after a month as after a second.

use engine_pixel::math::sin_cycle;
use engine_pixel::Cycle;

use crate::fix::{beat_ns, beats_to_ns, div_floor, mul_div, mul_div2, ratio, rem_floor, Fix, Ns, FIX_ONE, NS_PER_MS};
use crate::format::capv_flag;
use crate::format::{curve, effect_flag, fan_mode, movement, phasing, run, ty, CapV, CurveCap, Effect};

use super::caps::CapVal;
use super::channel::{DEFAULT_PAN_RANGE, DEFAULT_TILT_RANGE};

/// The largest float below 1: where a run of whole cycles is held, just before the curve starts over.
const BEFORE_ONE: f32 = 0.999_999_94;

fn beat_synced(effect: &Effect) -> bool {
    effect.flags & effect_flag::BEAT_SYNCED != 0
}

/// the period of an effect at the tempo it is played at (none: the default tempo)
pub fn length(effect: &Effect, bpm: Option<Fix>) -> Ns {
    if !beat_synced(effect) {
        return effect.length;
    }

    // a period of zero would divide by zero when the cycle is sampled
    beats_to_ns(effect.length_beats, bpm.unwrap_or(0)).max(NS_PER_MS)
}

/// the time an effect is shifted by inside its period
pub fn phase(effect: &Effect, bpm: Option<Fix>) -> Ns {
    if !beat_synced(effect) {
        return effect.phase;
    }

    beats_to_ns(effect.phase_beats, bpm.unwrap_or(0))
}

/// the time an effect is shifted by for a fixture of its preset (chasing)
pub fn phasing(effect: &Effect, index: u32, count: u32, bpm: Option<Fix>) -> Ns {
    let group = effect.group_size.max(1) as i64;
    let step = index as i64 / group;

    match effect.phasing_mode {
        phasing::SPREAD => {
            // distribute the cycles over all chase steps of the preset
            let count = if count == 0 { 1 } else { count as i64 };
            let steps = ((count + group - 1) / group).max(1);
            mul_div2(
                step.saturating_mul(effect.phasing_cycles),
                length(effect, bpm),
                steps,
                FIX_ONE,
            )
        }
        phasing::BEATS => mul_div(
            step.saturating_mul(effect.phasing_beats),
            beat_ns(bpm.unwrap_or(0)),
            FIX_ONE,
        ),
        _ => step.saturating_mul(effect.phasing),
    }
}

/// how long an effect runs, from the moment its fixture starts, none if it never stops
pub fn run_end(effect: &Effect, bpm: Option<Fix>) -> Option<Ns> {
    match effect.run_mode {
        run::CYCLES => Some((effect.run_cycles.max(1) as i64).saturating_mul(length(effect, bpm))),
        run::DURATION => Some(effect.run_duration.max(0)),
        _ => None,
    }
}

/// How long one pass of an effect that stops takes, from the first fixture starting until the last one has finished,
/// plus a rest. A preset which isn't on a timeline is previewed by repeating this pass.
pub fn run_loop(effect: &Effect, count: u32, bpm: Option<Fix>) -> Option<Ns> {
    let run_end = run_end(effect, bpm)?;
    let last = if count == 0 { 0 } else { count - 1 };
    let chase = phasing(effect, last, count, bpm).saturating_abs();

    Some(mul_div(run_end.saturating_add(chase), 5, 4).max(NS_PER_MS))
}

fn cycle_of(shifted: Ns, length: Ns) -> Cycle {
    let frac = ratio(rem_floor(shifted, length), length);

    Cycle {
        whole: div_floor(shifted, length),
        frac: if frac >= 1.0 { BEFORE_ONE } else { frac },
    }
}

/// How many periods an effect has run through at a moment on a fixture, counted from where its phase puts its start,
/// or none if it does not apply at this moment.
pub fn cycles_at(effect: &Effect, time: Ns, index: u32, count: u32, bpm: Option<Fix>) -> Option<Cycle> {
    cycles_phased(effect, time, phasing(effect, index, count, bpm), bpm)
}

/// The same, for a fixture the chase shifts by a time (see `phasing`): all the fixtures it shifts by the same time
/// are at the same place.
pub fn cycles_phased(effect: &Effect, time: Ns, phasing: Ns, bpm: Option<Fix>) -> Option<Cycle> {
    let length = length(effect, bpm);
    let phase = phase(effect, bpm);

    if let Some(run_end) = run_end(effect, bpm) {
        // the chase delays the whole run of a fixture, so each of them runs its cycles completely
        let run = time - phasing;

        if run < 0 {
            // this fixture has not started yet
            return None;
        }

        if run >= run_end {
            if effect.flags & effect_flag::END_BASE != 0 {
                return None;
            }

            let shifted = run_end - phase;

            if rem_floor(shifted, length) == 0 {
                // a run of whole cycles ends on a cycle border: hold what it showed just before it
                return Some(Cycle {
                    whole: div_floor(shifted, length) - 1,
                    frac: BEFORE_ONE,
                });
            }

            return Some(cycle_of(shifted, length));
        }
    }

    Some(cycle_of(time - phase - phasing, length))
}

/// How many periods a still pattern stands at on a fixture: where its phase and the chase put it.
pub fn still_cycles(effect: &Effect, index: u32, count: u32, bpm: Option<Fix>) -> Cycle {
    cycle_of(
        -(phase(effect, bpm) + phasing(effect, index, count, bpm)),
        length(effect, bpm),
    )
}

fn duty(effect: &Effect) -> f32 {
    effect.duty.min(1.0).max(0.0)
}

fn fract(value: f32) -> f32 {
    value - libm::floorf(value)
}

/// remaps the position inside the cycle of a sine or a triangle, so the curve is on over the duty cycle
fn warp(effect: &Effect, position: f32) -> f32 {
    let duty = duty(effect);

    if duty == 0.5 {
        return position;
    }

    if duty <= 0.0 {
        return 0.75;
    }

    if duty >= 1.0 {
        return 0.25;
    }

    let from_peak = (fract(position + 0.25) - 0.5).abs();
    let fall = duty.min(1.0 - duty);
    let warped = (0.25 + (from_peak - duty / 2.0) / fall * 0.5).min(0.5).max(0.0);

    0.25 + warped
}

/// how far a sawtooth has ramped up at a position inside its cycle
fn ramp(effect: &Effect, position: f32) -> f32 {
    let duty = duty(effect);

    if duty == 0.5 {
        return position;
    }

    if duty <= 0.0 {
        return 0.0;
    }

    if duty >= 1.0 {
        return 1.0;
    }

    let ramp_length = 2.0 * duty.min(1.0 - duty);
    (0.5 + (position - (1.0 - duty)) / ramp_length).min(1.0).max(0.0)
}

/// The value of a curve at a position inside its cycle, between 0 and 1.
pub fn curve_value(effect: &Effect, position: f32) -> f32 {
    let half = effect.amplitude / 2.0;

    let value = match effect.curve {
        curve::SINE => effect.position + half * sin_cycle(warp(effect, position)),
        curve::SQUARE => {
            // on while the sine is above its position: over the duty cycle, centered on its peak
            let duty = duty(effect);
            let from_on = fract(position - 0.25 + duty / 2.0);

            if from_on < duty {
                effect.position + half
            } else {
                effect.position - half
            }
        }
        curve::TRIANGLE => {
            let warped = warp(effect, position);
            effect.position + half * (1.0 - 4.0 * (fract(warped + 0.25) - 0.5).abs())
        }
        curve::SAWTOOTH => effect.position + half * (2.0 * ramp(effect, position) - 1.0),
        curve::REVERSE_SAWTOOTH => effect.position + half * (2.0 * ramp(effect, 1.0 - position) - 1.0),
        _ => 0.0,
    };

    value.min(1.0).max(0.0)
}

/// The value a curve puts on its channels at a moment on a fixture, none if it does not apply at this moment.
pub fn value_at(effect: &Effect, time: Ns, index: u32, count: u32, bpm: Option<Fix>) -> Option<f32> {
    value_phased(effect, time, phasing(effect, index, count, bpm), bpm)
}

/// The value a curve puts on its channels at a moment on a fixture the chase shifts by a time, none if it does not
/// apply at this moment.
pub fn value_phased(effect: &Effect, time: Ns, phasing: Ns, bpm: Option<Fix>) -> Option<f32> {
    cycles_phased(effect, time, phasing, bpm).map(|cycle| curve_value(effect, cycle.frac))
}

/// How much of its movement an effect makes on a fixture by its place in the fan: from -1 to 1.
pub fn fan_factor(effect: &Effect, place: Option<f32>) -> f32 {
    if !matches!(effect.fan_mode, fan_mode::LINEAR | fan_mode::CENTER | fan_mode::SIDE) {
        return 1.0;
    }

    let place = match place {
        Some(place) => place,
        None => return 0.0,
    };

    match effect.fan_mode {
        fan_mode::LINEAR => 2.0 * place - 1.0,
        fan_mode::CENTER => (2.0 * place - 1.0).abs(),
        _ => place,
    }
}

/// The value of a curve on a fixture at a place in the fan, swung around its position as far as the place says.
pub fn fanned(effect: &Effect, value: Option<f32>, place: Option<f32>) -> Option<f32> {
    let value = value?;

    if !matches!(effect.fan_mode, fan_mode::LINEAR | fan_mode::CENTER | fan_mode::SIDE) {
        return Some(value);
    }

    Some(effect.position + (value - effect.position) * fan_factor(effect, place))
}

/// The value of a step a curve on a capability plays at a value of the curve: a percentage, the pan and the tilt in
/// degrees around the middle of what the heads reach.
pub fn capability_value(effect: &Effect, cap: &CurveCap, value: f32) -> CapVal {
    let mut v = CapV {
        ty: cap.ty,
        color: cap.color,
        ..CapV::default()
    };

    if effect.flags & effect_flag::PAN_TILT_IN_DEGREES != 0 && (cap.ty == ty::PAN || cap.ty == ty::TILT) {
        v.flags = capv_flag::DEGREES;
        v.degrees = (value - 0.5)
            * if cap.ty == ty::PAN {
                DEFAULT_PAN_RANGE
            } else {
                DEFAULT_TILT_RANGE
            };
    } else {
        v.flags = capv_flag::PERCENTAGE;
        v.percentage = value;
    }

    CapVal::of(v)
}

/// How far a movement moves a fixture away from where the presets point it (pan and tilt, in degrees), none if it
/// does not apply at this moment.
pub fn movement_at(
    effect: &Effect,
    time: Ns,
    index: u32,
    count: u32,
    bpm: Option<Fix>,
    place: Option<f32>,
) -> Option<(f32, f32)> {
    let position = cycles_at(effect, time, index, count, bpm)?.frac;

    let (pan, tilt) = match effect.shape {
        movement::FIGURE8 => (sin_cycle(position), sin_cycle(fract(position * 2.0))),
        movement::LINE => (sin_cycle(position), sin_cycle(position)),
        movement::WAVE => (sin_cycle(position), sin_cycle(fract(position * 3.0))),
        // a circle: the cosine is the sine a quarter of a cycle ahead
        _ => (sin_cycle(position), sin_cycle(fract(position + 0.25))),
    };

    let fan = fan_factor(effect, place);
    Some((effect.size_pan / 2.0 * pan * fan, effect.size_tilt / 2.0 * tilt * fan))
}
