//! The value a channel of a fixture plays a value for a capability at: a brightness within the range of the channel
//! giving it, a position (a zoom, an iris, a strobe ...) by the capability of the channel doing it, a program, a
//! rotation, the pan and the tilt.

use crate::format::{cap_flag, capv_flag, chan_flag, maintenance, no_name, shutter, ty, Cap, CapV, Chan, Show};

use super::caps::{has, has_percentage, is_pan_tilt, CapVal};

/// what a moving head reaches on an axis, if its profile doesn't say
pub const DEFAULT_PAN_RANGE: f32 = 540.0;
pub const DEFAULT_TILT_RANGE: f32 = 270.0;

/// Two brightnesses this close are as bright (the noise of single-precision floats).
const SAME: f32 = 1e-6;

/// What places a fixture of a preset: its place in the chase and in the fan of the preset, and which fixture it is
/// (to aim it at a spot).
#[derive(Clone, Copy)]
pub struct Ctx {
    pub index: u32,
    pub count: u32,
    pub fan_place: Option<f32>,
    pub fixture: u32,
}

pub fn is_positioned(t: u8) -> bool {
    matches!(
        t,
        ty::SHUTTER_STROBE
            | ty::STROBE_SPEED
            | ty::STROBE_DURATION
            | ty::ZOOM
            | ty::FOCUS
            | ty::IRIS
            | ty::FROST
            | ty::COLOR_TEMPERATURE
            | ty::PRISM
            | ty::PAN_TILT_SPEED
            | ty::FOG_OUTPUT
            | ty::WHEEL_SLOT_ROTATION
            | ty::PRISM_ROTATION
            | ty::ROTATION
            | ty::BLADE_INSERTION
            | ty::BLADE_ROTATION
            | ty::BLADE_SYSTEM_ROTATION
            | ty::IRIS_EFFECT
            | ty::FROST_EFFECT
            | ty::EFFECT_SPEED
    )
}

pub fn is_command(t: u8) -> bool {
    t == ty::EFFECT || t == ty::MAINTENANCE
}

pub fn is_rotation(t: u8) -> bool {
    matches!(t, ty::WHEEL_SLOT_ROTATION | ty::PRISM_ROTATION | ty::ROTATION)
}

pub fn is_overriding(t: u8) -> bool {
    matches!(t, ty::IRIS_EFFECT | ty::FROST_EFFECT | ty::WHEEL_SHAKE)
}

/// is a capability of a channel one a positioned value of the type is played with?
fn plays(t: u8, cap: u8) -> bool {
    cap == t || (t == ty::ZOOM && cap == ty::BEAM_ANGLE) || (t == ty::FOG_OUTPUT && cap == ty::FOG)
}

/// The types of capabilities a channel needs one of to play a value of the type, a bit for each.
pub fn type_mask(t: u8) -> u64 {
    let bit = |t: u8| if t < 64 { 1u64 << t } else { 0 };

    match t {
        ty::ZOOM => bit(ty::ZOOM) | bit(ty::BEAM_ANGLE),
        ty::FOG_OUTPUT => bit(ty::FOG_OUTPUT) | bit(ty::FOG),
        _ => bit(t),
    }
}

fn width(chan: &Chan, cap: &Cap) -> f32 {
    if cap.flags & cap_flag::RANGE_OR_MORE != 0 {
        cap.high - cap.low
    } else {
        chan.max
    }
}

/// the widest capability of a channel passing a test, the first of them as wide
fn widest(show: &Show, chan: &Chan, test: impl Fn(&Cap) -> bool) -> Option<Cap> {
    let mut best: Option<Cap> = None;

    for cap in show.iter(chan.caps) {
        if !test(&cap) {
            continue;
        }

        match &best {
            Some(current) if !(width(chan, &cap) > width(chan, current)) => {}
            _ => best = Some(cap),
        }
    }

    best
}

fn any(show: &Show, chan: &Chan, test: impl Fn(&Cap) -> bool) -> bool {
    show.iter(chan.caps).any(|cap| test(&cap))
}

fn first(show: &Show, chan: &Chan, test: impl Fn(&Cap) -> bool) -> Option<Cap> {
    show.iter(chan.caps).find(|cap| test(cap))
}

/// the capability switching a type off: "no function" on a channel which has the type
fn off(show: &Show, chan: &Chan, t: u8) -> Option<Cap> {
    if chan.no_function < 0 || !any(show, chan, |cap| plays(t, cap.ty)) {
        return None;
    }

    show.get(chan.caps, chan.no_function as usize)
}

/// the middle of a capability, none for one without a range of its own
fn center(cap: &Cap) -> Option<f32> {
    if cap.flags & cap_flag::RANGE != 0 {
        Some(cap.center)
    } else {
        None
    }
}

/// the value at a position (0 to 1) within the range of a capability
fn range_value(chan: &Chan, cap: &Cap, position: f32) -> Option<f32> {
    if chan.caps.len() == 1 {
        // the whole channel
        return Some(chan.max * position);
    }

    if cap.flags & cap_flag::RANGE_OR_MORE == 0 {
        return None;
    }

    Some(cap.low + (cap.high - cap.low) * position)
}

// ---- the brightness ----

/// The value of a channel lighting with a brightness (0 to 1), through the capability passing the test which gives
/// it, or else the closest one (the brighter one, if two are as close).
pub fn brightness_value(show: &Show, chan: &Chan, test: impl Fn(&Cap) -> bool, brightness: f32) -> Option<f32> {
    if chan.caps.len() == 1 {
        // the whole channel, maybe from bright to off
        let cap = first(show, chan, &test)?;

        return Some(if cap.brightness_start > cap.brightness_end {
            chan.max * (1.0 - brightness)
        } else {
            chan.max * brightness
        });
    }

    let mut best: Option<Cap> = None;
    let mut best_distance = f32::MAX;
    let mut best_brightness = 0.0;
    let mut best_is_range = false;

    for cap in show.iter(chan.caps) {
        if !test(&cap) {
            continue;
        }

        let low = cap.brightness_start.min(cap.brightness_end);
        let high = cap.brightness_start.max(cap.brightness_end);
        let reached = low.max(high.min(brightness));
        let distance = (reached - brightness).abs();
        let is_range = low < high;

        let better = if best.is_none() || distance < best_distance - SAME {
            true
        } else if distance > best_distance + SAME {
            false
        } else if distance == 0.0 {
            // both give it: a range rather than a single brightness, else the first one
            is_range && !best_is_range
        } else {
            // as close: the brighter one
            reached > best_brightness
        };

        if better {
            best = Some(cap);
            best_distance = distance;
            best_brightness = reached;
            best_is_range = is_range;
        }
    }

    let cap = best?;

    if cap.flags & cap_flag::RANGE_OR_MORE == 0 {
        return None;
    }

    if cap.brightness_start == cap.brightness_end {
        return Some(cap.center);
    }

    Some(
        cap.low
            + (cap.high - cap.low) * (best_brightness - cap.brightness_start)
                / (cap.brightness_end - cap.brightness_start),
    )
}

/// How bright a dimmer channel lights at a value (0 to 1). Another function sharing the channel counts as lighting.
pub fn brightness_of_value(show: &Show, chan: &Chan, value: f32) -> f32 {
    let cap = match first(show, chan, |cap| {
        cap.flags & cap_flag::RANGE_OR_MORE == 0 || (value >= cap.low && value <= cap.high)
    }) {
        Some(cap) => cap,
        None => return 0.0,
    };

    if cap.ty != ty::INTENSITY {
        return 1.0;
    }

    let position = if cap.flags & cap_flag::RANGE_OR_MORE != 0 {
        if cap.high == cap.low {
            0.0
        } else {
            (value - cap.low) / (cap.high - cap.low)
        }
    } else {
        value / chan.max
    };

    cap.brightness_start + (cap.brightness_end - cap.brightness_start) * position
}

/// Scale the brightness a value gives through a capability of a type (a dimmer or a color) by a factor, within the
/// range of the capability. Values of other capabilities stay as they are.
pub fn dim(show: &Show, chan: &Chan, value: f32, t: u8, factor: f32) -> f32 {
    if factor >= 1.0 {
        return value;
    }

    for cap in show.iter(chan.caps) {
        if cap.ty != t {
            continue;
        }

        let ranged = cap.flags & cap_flag::RANGE_OR_MORE != 0;
        let low = if ranged { cap.low } else { 0.0 };
        let high = if ranged { cap.high } else { chan.max };

        if value < low || value > high {
            continue;
        }

        if cap.brightness_start == cap.brightness_end || high == low {
            // a single brightness can't be dimmed
            return value;
        }

        let brightness =
            cap.brightness_start + (cap.brightness_end - cap.brightness_start) * (value - low) / (high - low);
        let position = (brightness * factor - cap.brightness_start) / (cap.brightness_end - cap.brightness_start);
        return low + (high - low) * position.min(1.0).max(0.0);
    }

    value
}

// ---- the positions ----

/// the capabilities of a channel flashing in an effect: the widest of them
fn strobe_of(show: &Show, chan: &Chan, effect: u8) -> Option<Cap> {
    let shutters = |cap: &Cap| plays(ty::SHUTTER_STROBE, cap.ty);

    if effect == shutter::RANDOM || effect == shutter::SOUND {
        let flag = if effect == shutter::RANDOM {
            cap_flag::RANDOM
        } else {
            cap_flag::SOUND
        };
        let matching = |cap: &Cap| shutters(cap) && cap.flags & flag != 0;
        let strobes = |cap: &Cap| matching(cap) && cap.shutter == shutter::STROBE;

        return if any(show, chan, strobes) {
            widest(show, chan, strobes)
        } else {
            widest(show, chan, matching)
        };
    }

    let effects = |cap: &Cap| shutters(cap) && cap.shutter == effect;
    let regular = |cap: &Cap| effects(cap) && cap.flags & (cap_flag::RANDOM | cap_flag::SOUND) == 0;

    if any(show, chan, regular) {
        widest(show, chan, regular)
    } else {
        widest(show, chan, effects)
    }
}

/// The capability of a channel flashing in an effect (a plain strobe without one), or in the closest one it has.
fn strobe(show: &Show, chan: &Chan, effect: u8) -> Option<Cap> {
    let effect = if (shutter::CLOSED..=shutter::SOUND).contains(&effect) {
        effect
    } else {
        shutter::STROBE
    };

    let fallbacks: &[u8] = match effect {
        shutter::RANDOM => &[shutter::SPIKES, shutter::LIGHTNING],
        shutter::RAMP_UP | shutter::RAMP_DOWN => &[shutter::RAMP_UP_DOWN, shutter::PULSE],
        shutter::RAMP_UP_DOWN => &[shutter::PULSE],
        shutter::LIGHTNING => &[shutter::RANDOM, shutter::SPIKES],
        shutter::SPIKES => &[shutter::RANDOM, shutter::LIGHTNING],
        shutter::BURST => &[shutter::RANDOM],
        _ => &[],
    };

    if let Some(cap) = strobe_of(show, chan, effect) {
        return Some(cap);
    }

    for fallback in fallbacks {
        if let Some(cap) = strobe_of(show, chan, *fallback) {
            return Some(cap);
        }
    }

    strobe_of(show, chan, shutter::STROBE)
}

fn positioned_capability(show: &Show, chan: &Chan, t: u8, percentage: f32, blade: u16, effect: u8) -> Option<Cap> {
    let of_type = |cap: &Cap| plays(t, cap.ty);

    if t == ty::BLADE_INSERTION || t == ty::BLADE_ROTATION {
        return widest(show, chan, |cap| of_type(cap) && (no_name(blade) || blade == cap.blade));
    }

    if (t == ty::IRIS_EFFECT || t == ty::FROST_EFFECT) && percentage <= 0.0 {
        // no pulse leaves the iris or the frost to the rest of the preset
        return None;
    }

    if t == ty::SHUTTER_STROBE {
        if effect == shutter::CLOSED {
            // the shutter closed, whatever the value says, or else nothing to do
            return first(show, chan, |cap| of_type(cap) && cap.shutter == shutter::CLOSED);
        }

        if percentage > 0.0 {
            return strobe(show, chan, effect);
        }

        // the shutter open, or else no strobe
        return first(show, chan, |cap| of_type(cap) && cap.shutter == shutter::OPEN).or_else(|| off(show, chan, t));
    }

    if t == ty::PRISM {
        if percentage > 0.0 {
            // a prism standing still, or else any of them
            return first(show, chan, |cap| of_type(cap) && cap.flags & cap_flag::STILL != 0)
                .or_else(|| first(show, chan, of_type));
        }

        return off(show, chan, t);
    }

    if t == ty::FOG_OUTPUT && percentage <= 0.0 {
        // the fog off, or else no function, or else the low end of what it puts out
        return first(show, chan, |cap| of_type(cap) && cap.flags & cap_flag::NOTHING != 0)
            .or_else(|| off(show, chan, t))
            .or_else(|| widest(show, chan, of_type));
    }

    widest(show, chan, of_type)
}

fn is_single_value(t: u8, percentage: f32, cap: &Cap) -> bool {
    if !plays(t, cap.ty) || t == ty::PRISM {
        return true;
    }

    if t == ty::SHUTTER_STROBE && (percentage <= 0.0 || cap.shutter == shutter::CLOSED) {
        return true;
    }

    t == ty::FOG_OUTPUT && percentage <= 0.0 && cap.flags & cap_flag::NOTHING != 0
}

/// does a capability turn a wheel (any wheel, if none is named)?
fn turns(show: &Show, cap: &Cap, wheel: u16) -> bool {
    no_name(wheel) || show.iter(cap.turned).any(|turned| turned == wheel)
}

fn rotation_ends(cap: &Cap) -> Option<(f32, f32)> {
    if cap.flags & cap_flag::ROTATION_ENDS != 0 {
        Some((cap.rotation_start, cap.rotation_end))
    } else {
        None
    }
}

fn sign(value: f32) -> f32 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// the value of a range turning both ways, from where it stops towards a direction
fn across_value(show: &Show, chan: &Chan, test: impl Fn(&Cap) -> bool, direction: f32, amount: f32) -> Option<f32> {
    let cap = widest(show, chan, test)?;
    let (start, end) = rotation_ends(&cap)?;
    let stop = start / (start - end);
    let towards_end = sign(end) == direction;

    range_value(
        chan,
        &cap,
        if towards_end {
            stop + (1.0 - stop) * amount
        } else {
            stop - stop * amount
        },
    )
}

/// The value of a channel turning something at a speed: anticlockwise below the middle (fastest at 0), stopped in the
/// middle, clockwise above it (fastest at 1).
fn rotation_value(show: &Show, chan: &Chan, t: u8, percentage: f32, wheel: u16) -> Option<f32> {
    let speed = (percentage * 2.0 - 1.0).min(1.0).max(-1.0);
    let rotations = |cap: &Cap| plays(t, cap.ty) && rotation_ends(cap).is_some() && turns(show, cap, wheel);

    if !any(show, chan, rotations) {
        return None;
    }

    // the ranges turning both ways
    let across = |cap: &Cap| {
        rotations(cap)
            && rotation_ends(cap)
                .map(|(start, end)| start * end < 0.0)
                .unwrap_or(false)
    };

    if speed == 0.0 {
        if let Some(cap) = first(show, chan, |cap| {
            rotations(cap) && rotation_ends(cap) == Some((0.0, 0.0))
        }) {
            return center(&cap);
        }

        if any(show, chan, across) {
            return across_value(show, chan, across, 1.0, 0.0);
        }

        return off(show, chan, t).and_then(|cap| center(&cap));
    }

    let direction = sign(speed);
    let one_way = |cap: &Cap| {
        rotations(cap)
            && rotation_ends(cap)
                .map(|(start, end)| start * direction >= 0.0 && end * direction >= 0.0 && (start != 0.0 || end != 0.0))
                .unwrap_or(false)
    };

    if let Some(cap) = widest(show, chan, one_way) {
        let (start, end) = rotation_ends(&cap)?;
        return range_value(
            chan,
            &cap,
            if start.abs() > end.abs() {
                1.0 - speed.abs()
            } else {
                speed.abs()
            },
        );
    }

    if any(show, chan, across) {
        across_value(show, chan, across, direction, speed.abs())
    } else {
        None
    }
}

/// The value of a channel turning a wheel (or anything else) to an angle in degrees and holding it there.
fn rotation_angle_value(show: &Show, chan: &Chan, t: u8, degrees: f32, wheel: u16) -> Option<f32> {
    let mut best: Option<Cap> = None;
    let mut best_distance = f32::MAX;
    let mut best_position = None;

    for cap in show.iter(chan.caps) {
        if !plays(t, cap.ty) || !turns(show, &cap, wheel) || cap.flags & cap_flag::ANGLES == 0 {
            continue;
        }

        let low = cap.angle_start.min(cap.angle_end);
        let high = cap.angle_start.max(cap.angle_end);
        let distance = if degrees < low {
            low - degrees
        } else if degrees > high {
            degrees - high
        } else {
            0.0
        };

        if distance < best_distance {
            best_distance = distance;
            best_position = if cap.angle_start == cap.angle_end {
                None
            } else {
                Some((low.max(high.min(degrees)) - cap.angle_start) / (cap.angle_end - cap.angle_start))
            };
            best = Some(cap);
        }
    }

    let cap = best?;

    match best_position {
        Some(position) => range_value(chan, &cap, position),
        None => center(&cap),
    }
}

/// The value of a channel shaking a slot of a wheel (or the whole wheel) at a speed, from slow to fast.
fn shake_value(show: &Show, chan: &Chan, wheel: u16, slot: Option<f32>, percentage: f32) -> Option<f32> {
    if !(percentage > 0.0) {
        return None;
    }

    let cap_slot = |cap: &Cap| {
        if cap.flags & cap_flag::SLOT != 0 {
            Some(cap.slot)
        } else {
            None
        }
    };
    let shakes = |cap: &Cap| {
        cap.ty == ty::WHEEL_SHAKE
            && (no_name(wheel) || wheel == cap.shake_wheel)
            && (cap_slot(cap).is_none() || cap_slot(cap) == slot)
    };
    let of_slot = |cap: &Cap| shakes(cap) && cap_slot(cap) == slot;

    // the slot shaking on its own, or else the whole wheel
    let shake = if any(show, chan, of_slot) {
        widest(show, chan, of_slot)
    } else {
        widest(show, chan, shakes)
    }?;

    range_value(
        chan,
        &shake,
        if shake.flags & cap_flag::REVERSED != 0 {
            1.0 - percentage
        } else {
            percentage
        },
    )
}

/// The value of a channel playing a positioned value of a type (0 to 1), none if the channel doesn't do it.
pub fn positioned_value(
    show: &Show,
    chan: &Chan,
    t: u8,
    percentage: f32,
    blade: u16,
    effect: u8,
    wheel: u16,
) -> Option<f32> {
    if is_rotation(t) {
        return rotation_value(show, chan, t, percentage, wheel);
    }

    let cap = positioned_capability(show, chan, t, percentage, blade, effect)?;

    if is_single_value(t, percentage, &cap) {
        return center(&cap);
    }

    range_value(
        chan,
        &cap,
        if cap.flags & cap_flag::REVERSED != 0 {
            1.0 - percentage
        } else {
            percentage
        },
    )
}

/// The value of a channel running a program (1 for the first one) at a speed, none (0) as its "no function".
fn program_value(show: &Show, chan: &Chan, number: i32, sound: bool, speed: Option<f32>) -> Option<f32> {
    if number <= 0 {
        return off(show, chan, ty::EFFECT).and_then(|cap| center(&cap));
    }

    let programs = if sound { chan.sound_programs } else { chan.programs };
    let index = show.get(programs, number as usize - 1)?;
    let program = show.get(chan.caps, index as usize)?;

    match speed {
        Some(speed) if program.flags & cap_flag::SPEED != 0 && speed >= 0.0 => range_value(
            chan,
            &program,
            if program.flags & cap_flag::REVERSED != 0 {
                1.0 - speed
            } else {
                speed
            },
        ),
        _ => center(&program),
    }
}

/// The value a positioned value of a step (a program and maintenance as well) drives a channel of a fixture of the
/// passed profile to, none if the channel doesn't do it.
pub fn positioned_step_value(show: &Show, chan: &Chan, v: &CapV, profile: u16) -> Option<f32> {
    if !no_name(v.profile) && v.profile != profile {
        return None;
    }

    if v.ty == ty::EFFECT {
        let speed = if has(v, capv_flag::PERCENTAGE) {
            Some(v.percentage)
        } else {
            None
        };
        return program_value(
            show,
            chan,
            if has(v, capv_flag::PROGRAM) { v.program } else { 0 },
            has(v, capv_flag::SOUND),
            speed,
        );
    }

    if v.ty == ty::MAINTENANCE {
        let (value, action) = match v.maintenance {
            maintenance::RESET => (chan.maintenance_reset, 0),
            maintenance::LAMP_ON => (chan.maintenance_lamp_on, 1),
            maintenance::LAMP_OFF => (chan.maintenance_lamp_off, 2),
            _ => return None,
        };

        return if chan.flags & (chan_flag::MAINTENANCE << action) != 0 {
            Some(value)
        } else {
            None
        };
    }

    if is_rotation(v.ty) && has(v, capv_flag::DEGREES) {
        return rotation_angle_value(show, chan, v.ty, v.degrees, v.wheel);
    }

    if !has_percentage(v) {
        return None;
    }

    if v.ty == ty::WHEEL_SHAKE {
        return shake_value(
            show,
            chan,
            v.wheel,
            if has(v, capv_flag::SLOT) { Some(v.slot) } else { None },
            v.percentage,
        );
    }

    positioned_value(show, chan, v.ty, v.percentage, v.blade, v.shutter, v.wheel)
}

// ---- the pan and the tilt ----

/// The degrees a channel turns its fixture over on an axis, none if it doesn't turn it on that axis.
pub fn axis_range(chan: &Chan, t: u8) -> Option<f32> {
    if t == ty::PAN && chan.flags & chan_flag::PAN != 0 {
        Some(chan.pan_range)
    } else if t == ty::TILT && chan.flags & chan_flag::TILT != 0 {
        Some(chan.tilt_range)
    } else {
        None
    }
}

/// the value of a channel pointing its fixture at degrees from the middle of its range, as far as it reaches
pub fn axis_value(chan: &Chan, range: f32, degrees: f32) -> f32 {
    chan.max * (0.5 + degrees / range).min(1.0).max(0.0)
}

/// the degrees from the middle of its range a value of a channel points its fixture at
pub fn axis_degrees(chan: &Chan, range: f32, value: f32) -> f32 {
    (value / chan.max - 0.5) * range
}

/// The offset of a fixture in a fan spread over a number of degrees around its middle.
fn spread_degrees(v: &CapV, ctx: Option<&Ctx>) -> f32 {
    let ctx = match ctx {
        Some(ctx) if has(v, capv_flag::SPREAD) => ctx,
        _ => return 0.0,
    };

    if has(v, capv_flag::FAN_CENTER) {
        let position = ctx.fan_place.unwrap_or(0.5);
        return v.spread * ((2.0 * position - 1.0).abs() - 0.5);
    }

    match ctx.fan_place {
        Some(place) => v.spread * (place - 0.5),
        None => 0.0,
    }
}

/// where a value points a fixture on its own, without the step it travels from
fn own_degrees(show: &Show, v: &CapV, range: f32, mirror: bool, ctx: Option<&Ctx>) -> Option<f32> {
    let spread = spread_degrees(v, ctx);

    if has(v, capv_flag::AIM) {
        // a spot is where the fixtures point, whatever the mirror of the preset says
        let ctx = ctx?;
        let fixtures = show.root.fixtures.len();
        let aim = show.get(
            show.root.aims,
            (v.aim as usize)
                .checked_mul(fixtures)?
                .checked_add(ctx.fixture as usize)?,
        )?;

        if aim.known == 0 {
            return None;
        }

        return Some(if v.ty == ty::PAN { aim.pan } else { aim.tilt } + spread);
    }

    let degrees = if has(v, capv_flag::DEGREES) {
        v.degrees
    } else if has_percentage(v) {
        (v.percentage - 0.5) * range
    } else {
        return None;
    } + spread;

    Some(if mirror { -degrees } else { degrees })
}

/// Where a pan or tilt value points a fixture on its axis, in degrees from the middle of its range. While a preset
/// travels from one step into the next, each fixture travels from where the value of the step before points it.
pub fn pan_tilt_degrees(show: &Show, value: &CapVal, range: f32, mirror: bool, ctx: Option<&Ctx>) -> Option<f32> {
    let degrees = own_degrees(show, &value.v, range, mirror, ctx);

    let from = match show.get(value.from, 0) {
        Some(from) => from,
        None => return degrees,
    };

    let from_degrees = own_degrees(show, &from, range, mirror, ctx);

    match (degrees, from_degrees) {
        (Some(to), Some(from)) => Some(from + (to - from) * value.position),
        _ => {
            if value.position > 0.0 {
                degrees
            } else {
                from_degrees
            }
        }
    }
}

/// The value of a channel playing a pan or tilt value, none if the channel doesn't turn the fixture on its axis or
/// the value doesn't say where to.
pub fn pan_tilt_value(show: &Show, chan: &Chan, value: &CapVal, mirror: bool, ctx: Option<&Ctx>) -> Option<f32> {
    let range = axis_range(chan, value.v.ty)?;
    let v = &value.v;

    if value.from.is_empty()
        && !has(v, capv_flag::AIM)
        && !has(v, capv_flag::DEGREES)
        && has_percentage(v)
        && spread_degrees(v, ctx) == 0.0
    {
        // a percentage of the range alone is played like it always was
        return Some(chan.max * if mirror { 1.0 - v.percentage } else { v.percentage });
    }

    pan_tilt_degrees(show, value, range, mirror, ctx).map(|degrees| axis_value(chan, range, degrees))
}

/// do two capabilities match: the one of a value and the one of a channel?
pub fn matches(v: &CapV, cap: &Cap, profile: u16) -> bool {
    v.ty == cap.ty
        && (v.color == 0 || v.color == cap.color)
        && (no_name(v.wheel) || v.wheel == cap.wheel)
        && (no_name(v.profile) || v.profile == profile)
}

/// The value a curve on a capability puts on a channel of a fixture, played like a value of a step. None, if the
/// channel doesn't do it.
pub fn curve_value(show: &Show, chan: &Chan, value: &CapVal, mirror: bool) -> Option<f32> {
    let v = &value.v;

    if v.ty == ty::SHUTTER_STROBE {
        // the strobe on a channel of its speed as well, like the strobe of a step
        return positioned_value(show, chan, v.ty, v.percentage, 0, shutter::NONE, 0)
            .or_else(|| positioned_value(show, chan, ty::STROBE_SPEED, v.percentage, 0, shutter::NONE, 0));
    }

    if is_positioned(v.ty) {
        return positioned_value(show, chan, v.ty, v.percentage, 0, shutter::NONE, 0);
    }

    let matching = |cap: &Cap| v.ty == cap.ty && (v.color == 0 || v.color == cap.color);

    if !any(show, chan, matching) {
        return None;
    }

    if (v.ty == ty::INTENSITY || v.ty == ty::COLOR_INTENSITY) && has_percentage(v) {
        return brightness_value(show, chan, matching, v.percentage);
    }

    if is_pan_tilt(v.ty) {
        return pan_tilt_value(show, chan, value, mirror, None);
    }

    None
}
