//! The values a preset puts on the capabilities of a fixture at a moment: the ones of its step, or travelling from
//! one step into the next, fanned out over the fixtures and turned into what the emitters of the fixture play.

use crate::format::{capv_flag, color, ty, Amount, CapV, FanStop, Field, List, Preset, Show, Step};

use super::steps::StepSel;

/// The most values for capabilities a step can have.
pub const MAX_STEP_CAPS: usize = 28;

/// The most values a fixture is given by a preset at a moment: the ones of two steps and what the emitters add.
pub const MAX_CAPS: usize = 2 * MAX_STEP_CAPS + 8;

/// A value for a capability while it is played: travelling from the value of the step before, if it does so for each
/// fixture on its own.
#[derive(Clone, Copy, Default)]
pub struct CapVal {
    pub v: CapV,
    /// the value travelled from (one, or none)
    pub from: List<CapV>,
    pub position: f32,
}

impl CapVal {
    pub fn of(v: CapV) -> Self {
        CapVal {
            v,
            from: List::EMPTY,
            position: 0.0,
        }
    }

    /// a value of a type (and a color) at a percentage
    pub fn percentage(ty: u8, color: u8, percentage: f32) -> Self {
        CapVal::of(CapV {
            ty,
            color,
            flags: capv_flag::PERCENTAGE,
            percentage,
            ..CapV::default()
        })
    }
}

pub struct CapList {
    items: [CapVal; MAX_CAPS],
    len: usize,
}

impl CapList {
    pub fn new() -> Self {
        CapList {
            items: [CapVal::default(); MAX_CAPS],
            len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push(&mut self, value: CapVal) {
        if let Some(slot) = self.items.get_mut(self.len) {
            *slot = value;
            self.len += 1;
        }
    }

    pub fn get(&self, index: usize) -> Option<CapVal> {
        if index < self.len {
            self.items.get(index).copied()
        } else {
            None
        }
    }

    pub fn set(&mut self, index: usize, value: CapVal) {
        if index < self.len {
            if let Some(slot) = self.items.get_mut(index) {
                *slot = value;
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapVal> {
        self.items.iter().take(self.len)
    }
}

pub fn is_pan_tilt(t: u8) -> bool {
    t == ty::PAN || t == ty::TILT
}

/// whether a value gives a percentage (a missing one and one below 0 don't)
pub fn has_percentage(v: &CapV) -> bool {
    v.flags & capv_flag::PERCENTAGE != 0 && v.percentage >= 0.0
}

pub fn has(v: &CapV, flag: u32) -> bool {
    v.flags & flag != 0
}

fn has_pan_tilt_value(v: &CapV) -> bool {
    has(v, capv_flag::AIM) || has(v, capv_flag::DEGREES) || has_percentage(v)
}

fn is_plain_percentage(v: &CapV) -> bool {
    !has(v, capv_flag::AIM)
        && !has(v, capv_flag::DEGREES)
        && has_percentage(v)
        && (!has(v, capv_flag::SPREAD) || v.spread == 0.0)
}

/// Does a preset travelling from the one value to the other turn each moving head on its own?
fn is_pan_tilt_transition(from: &CapV, to: &CapV) -> bool {
    is_pan_tilt(to.ty)
        && has_pan_tilt_value(from)
        && has_pan_tilt_value(to)
        && !(is_plain_percentage(from) && is_plain_percentage(to))
}

fn says(v: &CapV) -> bool {
    has(v, capv_flag::FAN) || has_percentage(v)
}

/// Does a preset travelling from the one value to the other work it out for each fixture on its own (a fan)?
fn is_fan_transition(from: &CapV, to: &CapV) -> bool {
    if is_pan_tilt(to.ty) || has(to, capv_flag::SLOT) || has(from, capv_flag::SLOT) {
        return false;
    }

    (has(from, capv_flag::FAN) || has(to, capv_flag::FAN)) && says(from) && says(to)
}

/// a value is known by what it drives, not by the value it holds
fn same_capability(a: &CapV, b: &CapV) -> bool {
    a.ty == b.ty && a.color == b.color && a.wheel == b.wheel && a.profile == b.profile && a.blade == b.blade
}

fn find(show: &Show, list: List<CapV>, value: &CapV) -> Option<(usize, CapV)> {
    show.iter(list)
        .enumerate()
        .find(|(_, candidate)| same_capability(candidate, value))
}

/// The values of the capabilities a preset plays at the passed place of its steps.
pub fn materialize(show: &Show, preset: &Preset, sel: &StepSel, out: &mut CapList) {
    out.clear();

    let to = match show.get(preset.steps, sel.to as usize) {
        Some(step) => step,
        None => return,
    };

    let from = if sel.from >= 0 {
        show.get(preset.steps, sel.from as usize)
    } else {
        None
    };

    let from = match from {
        Some(step) => step,
        None => {
            for value in show.iter(to.caps) {
                out.push(CapVal::of(value));
            }

            return;
        }
    };

    let position = sel.position;

    for mut value in show.iter(to.caps) {
        let (index, from_value) = match find(show, from.caps, &value) {
            Some(found) => found,
            None => {
                out.push(CapVal::of(value));
                continue;
            }
        };

        if is_pan_tilt_transition(&from_value, &value) || is_fan_transition(&from_value, &value) {
            // travels for each fixture on its own, see the pan and the tilt and the fans
            let offset = from.caps.offset.wrapping_add((index * CapV::SIZE) as u32);
            out.push(CapVal {
                v: value,
                from: List::new(offset, 1),
                position,
            });
            continue;
        }

        if has(&value, capv_flag::SLOT)
            || has(&from_value, capv_flag::SLOT)
            || !has(&value, capv_flag::PERCENTAGE)
            || !has(&from_value, capv_flag::PERCENTAGE)
        {
            // a wheel cannot stand between two of its slots: it turns as soon as the transition starts
            out.push(CapVal::of(if position > 0.0 { value } else { from_value }));
            continue;
        }

        value.percentage = from_value.percentage + (value.percentage - from_value.percentage) * position;
        out.push(CapVal::of(value));
    }

    for from_value in show.iter(from.caps) {
        if find(show, to.caps, &from_value).is_none() {
            out.push(CapVal::of(from_value));
        }
    }
}

fn step_amount(show: &Show, step: &Step, effect: u32) -> Option<f32> {
    show.iter(step.amounts)
        .find(|amount: &Amount| amount.effect == effect)
        .map(|amount| amount.amount)
}

/// How far a preset opens one of its effects at the passed place of its steps (1: all of it).
pub fn effect_amount(show: &Show, preset: &Preset, sel: &StepSel, effect: u32) -> f32 {
    let to = match show.get(preset.steps, sel.to as usize) {
        Some(step) => step,
        None => return 1.0,
    };

    let from = if sel.from >= 0 {
        show.get(preset.steps, sel.from as usize)
    } else {
        None
    };

    let from = match from {
        Some(step) => step,
        None => return step_amount(show, &to, effect).unwrap_or(1.0),
    };

    let mut to_amount = None;

    for amount in show.iter(to.amounts) {
        if amount.effect == effect {
            to_amount = Some(amount.amount);
        }
    }

    match to_amount {
        Some(amount) => {
            let from_amount = step_amount(show, &from, effect).unwrap_or(1.0);
            from_amount + (amount - from_amount) * sel.position
        }
        // the step travelled to says nothing about this effect, so it opens fully
        None => match step_amount(show, &from, effect) {
            Some(amount) => amount + (1.0 - amount) * sel.position,
            None => 1.0,
        },
    }
}

/// Call a function with each value for a channel a preset plays at the passed place of its steps: the profile, the
/// name of the channel and the value.
pub fn for_each_channel_value(show: &Show, preset: &Preset, sel: &StepSel, mut each: impl FnMut(u16, u16, f32)) {
    let to = match show.get(preset.steps, sel.to as usize) {
        Some(step) => step,
        None => return,
    };

    let from = if sel.from >= 0 {
        show.get(preset.steps, sel.from as usize)
    } else {
        None
    };

    let from = match from {
        Some(step) => step,
        None => {
            for value in show.iter(to.chans) {
                each(value.profile, value.name, value.value);
            }

            return;
        }
    };

    for value in show.iter(to.chans) {
        match show
            .iter(from.chans)
            .find(|candidate| candidate.name == value.name && candidate.profile == value.profile)
        {
            Some(from_value) => each(
                value.profile,
                value.name,
                from_value.value + (value.value - from_value.value) * sel.position,
            ),
            None => each(value.profile, value.name, value.value),
        }
    }

    for from_value in show.iter(from.chans) {
        if !show
            .iter(to.chans)
            .any(|candidate| candidate.name == from_value.name && candidate.profile == from_value.profile)
        {
            each(from_value.profile, from_value.name, from_value.value);
        }
    }
}

/// The value of a fan at a place along it (the middle for a fixture which can't be told apart from the others).
pub fn fan_value(show: &Show, stops: List<FanStop>, place: Option<f32>, steps: bool) -> f32 {
    let position = place.unwrap_or(0.5);
    let count = stops.len();

    if count == 0 {
        return 0.0;
    }

    if steps {
        let index = ((position * count as f32) as usize).min(count - 1);
        return show.get(stops, index).map(|stop| stop.value).unwrap_or(0.0);
    }

    let mut previous = match show.get(stops, 0) {
        Some(stop) => stop,
        None => return 0.0,
    };

    if position <= previous.position {
        return previous.value;
    }

    for stop in show.iter(stops).skip(1) {
        if position == stop.position {
            return stop.value;
        }

        if position < stop.position {
            return previous.value
                + (stop.value - previous.value)
                    * ((position - previous.position) / (stop.position - previous.position));
        }

        previous = stop;
    }

    previous.value
}

fn placed_percentage(show: &Show, v: &CapV, place: Option<f32>) -> f32 {
    if has(v, capv_flag::FAN) {
        fan_value(show, v.fan, place, has(v, capv_flag::FAN_STEPS))
    } else {
        v.percentage
    }
}

/// Work the fans out for a fixture at the passed place in the fan of its preset. The pan and the tilt are left as
/// they are: they are worked out where they are played.
pub fn apply_fan(show: &Show, caps: &mut CapList, place: Option<f32>) {
    for index in 0..caps.len() {
        let mut value = match caps.get(index) {
            Some(value) => value,
            None => continue,
        };

        if is_pan_tilt(value.v.ty) {
            continue;
        }

        let from = show.get(value.from, 0);
        let from_fanned = from.map(|from| has(&from, capv_flag::FAN)).unwrap_or(false);

        if !has(&value.v, capv_flag::FAN) && !from_fanned {
            continue;
        }

        let to = placed_percentage(show, &value.v, place);

        value.v.percentage = match from {
            Some(from) => {
                let from = placed_percentage(show, &from, place);
                from + (to - from) * value.position
            }
            None => to,
        };

        value.v.flags = (value.v.flags | capv_flag::PERCENTAGE) & !(capv_flag::FAN | capv_flag::FAN_STEPS);
        value.v.fan = List::EMPTY;
        value.from = List::EMPTY;
        value.position = 0.0;
        caps.set(index, value);
    }
}

fn has_color(colors: u32, c: u8) -> bool {
    colors & (1 << c) != 0
}

/// the last value of a type and a color saying what to put on the fixture
fn find_value(caps: &CapList, t: u8, c: u8) -> Option<usize> {
    let mut found = None;

    for (index, value) in caps.iter().enumerate() {
        if value.v.ty == t && value.v.color == c && has_percentage(&value.v) {
            found = Some(index);
        }
    }

    found
}

// the color a color temperature is shown with on red, green and blue: at the warm end, in the middle, at the cold end
const TEMPERATURE_COLORS: [[f32; 3]; 3] = [[1.0, 0.6, 0.3], [1.0, 1.0, 1.0], [0.8, 0.88, 1.0]];

// how much green the light of an amber LED has, the red at full
const AMBER_GREEN: f32 = 0.75;

const RGB: [u8; 3] = [color::RED, color::GREEN, color::BLUE];

/// Turn the values into what the emitters of a fixture play: the color temperature on a warm and a cold white (or as
/// a color), the white and the amber part of a color on their LEDs, and the colors the other way round on a fixture
/// mixing them with filters. `colors` are the colors the fixture has channels for, `color_temperature` whether it
/// has a channel for the color temperature.
pub fn apply_emitters(caps: &mut CapList, colors: u32, color_temperature: bool, emitters_from_color: bool) {
    let has_rgb = RGB.iter().all(|c| has_color(colors, *c));
    let original = caps.len();

    if let Some(temperature) = find_value(caps, ty::COLOR_TEMPERATURE, color::NONE) {
        if !color_temperature {
            let cold = caps
                .get(temperature)
                .map(|value| value.v.percentage)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);

            if has_color(colors, color::WARM_WHITE) && has_color(colors, color::COLD_WHITE) {
                let warm_missing = find_value(caps, ty::COLOR_INTENSITY, color::WARM_WHITE).is_none();
                let cold_missing = find_value(caps, ty::COLOR_INTENSITY, color::COLD_WHITE).is_none();

                if warm_missing {
                    caps.push(CapVal::percentage(
                        ty::COLOR_INTENSITY,
                        color::WARM_WHITE,
                        (2.0 * (1.0 - cold)).min(1.0),
                    ));
                }

                if cold_missing {
                    caps.push(CapVal::percentage(
                        ty::COLOR_INTENSITY,
                        color::COLD_WHITE,
                        (2.0 * cold).min(1.0),
                    ));
                }
            } else if has_rgb && RGB.iter().all(|c| find_value(caps, ty::COLOR_INTENSITY, *c).is_none()) {
                let half = if cold < 0.5 { 0 } else { 1 };
                let share = if cold < 0.5 { cold * 2.0 } else { cold * 2.0 - 1.0 };

                for (index, c) in RGB.iter().enumerate() {
                    let from = TEMPERATURE_COLORS
                        .get(half)
                        .and_then(|row| row.get(index))
                        .copied()
                        .unwrap_or(1.0);
                    let to = TEMPERATURE_COLORS
                        .get(half + 1)
                        .and_then(|row| row.get(index))
                        .copied()
                        .unwrap_or(1.0);
                    caps.push(CapVal::percentage(ty::COLOR_INTENSITY, *c, from + (to - from) * share));
                }
            }
        }
    }

    let _ = original;

    // the white and the amber parts of the colors on their LEDs
    if emitters_from_color && has_rgb {
        let white = has_color(colors, color::WHITE) && find_value(caps, ty::COLOR_INTENSITY, color::WHITE).is_none();
        let amber = has_color(colors, color::AMBER) && find_value(caps, ty::COLOR_INTENSITY, color::AMBER).is_none();
        let found = [
            find_value(caps, ty::COLOR_INTENSITY, color::RED),
            find_value(caps, ty::COLOR_INTENSITY, color::GREEN),
            find_value(caps, ty::COLOR_INTENSITY, color::BLUE),
        ];

        if let (true, [Some(red_at), Some(green_at), Some(blue_at)]) = (white || amber, found) {
            let percentage = |index: usize| caps.get(index).map(|value| value.v.percentage).unwrap_or(0.0);
            let mut red = percentage(red_at);
            let mut green = percentage(green_at);
            let mut blue = percentage(blue_at);
            let mut white_part = 0.0;
            let mut amber_part = 0.0;

            if white {
                white_part = red.min(green).min(blue);
                red -= white_part;
                green -= white_part;
                blue -= white_part;
            }

            if amber {
                amber_part = red.min(green / AMBER_GREEN);
                red -= amber_part;
                green -= amber_part * AMBER_GREEN;
            }

            for (index, rest) in [(red_at, red), (green_at, green), (blue_at, blue)] {
                if let Some(mut value) = caps.get(index) {
                    value.v.percentage = rest;
                    caps.set(index, value);
                }
            }

            if white {
                caps.push(CapVal::percentage(ty::COLOR_INTENSITY, color::WHITE, white_part));
            }

            if amber {
                caps.push(CapVal::percentage(ty::COLOR_INTENSITY, color::AMBER, amber_part));
            }
        }
    }

    // a fixture mixing its colors with filters is given the colors the other way round
    let subtractive = !RGB.iter().any(|c| has_color(colors, *c))
        && [color::CYAN, color::MAGENTA, color::YELLOW]
            .iter()
            .any(|c| has_color(colors, *c));

    if !subtractive {
        return;
    }

    for index in 0..caps.len() {
        let value = match caps.get(index) {
            Some(value) => value,
            None => continue,
        };

        let filter = match value.v.color {
            color::RED => color::CYAN,
            color::GREEN => color::MAGENTA,
            color::BLUE => color::YELLOW,
            _ => continue,
        };

        if value.v.ty != ty::COLOR_INTENSITY || !has_percentage(&value.v) {
            continue;
        }

        caps.set(
            index,
            CapVal::of(CapV {
                ty: value.v.ty,
                color: filter,
                flags: capv_flag::PERCENTAGE | (value.v.flags & capv_flag::SLOT),
                profile: value.v.profile,
                wheel: value.v.wheel,
                slot: value.v.slot,
                percentage: 1.0 - value.v.percentage,
                ..CapV::default()
            }),
        );
    }
}
