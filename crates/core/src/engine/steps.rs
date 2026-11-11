//! Which step a preset is on at a moment, and how far it has travelled into it.

use crate::fix::{div_floor, mul_div, ratio, rem_floor, Ns};
use crate::format::{order, preset_flag, step_flag, transition, Preset, Show, Step};

/// The most steps a preset playing its steps in another order than written down can play in a pass.
pub const MAX_ORDER: usize = 256;

/// Shape how far a fade or a transition has come (0 to 1).
pub fn transition_curve(curve: u8, position: f32) -> f32 {
    if !(position > 0.0) {
        return 0.0;
    }

    if position >= 1.0 {
        return 1.0;
    }

    match curve {
        transition::EASE_IN => position * position,
        transition::EASE_OUT => 1.0 - (1.0 - position) * (1.0 - position),
        transition::EASE_IN_OUT => {
            if position < 0.5 {
                2.0 * position * position
            } else {
                1.0 - (-2.0 * position + 2.0) * (-2.0 * position + 2.0) / 2.0
            }
        }
        transition::SNAP => 0.0,
        _ => position,
    }
}

/// The 32 bits which look random for three whole numbers, the same on every machine (see the pixel maths).
pub fn hash_bits(a: i32, b: i32, c: i32) -> u32 {
    let mut h = (a as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((b as u32).wrapping_mul(668_265_263))
        .wrapping_add((c as u32).wrapping_mul(1_440_670_441));
    h = (h ^ (h >> 15)).wrapping_mul(0x85EB_CA77);
    h = (h ^ (h >> 13)).wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

/// What a preset plays at a moment: a step, or on its way from one step into another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepSel {
    /// the step travelled from (-1: none, the step is played as it is)
    pub from: i32,
    pub to: u32,
    pub position: f32,
}

impl StepSel {
    pub const fn single(step: u32) -> Self {
        StepSel {
            from: -1,
            to: step,
            position: 0.0,
        }
    }
}

/// How fast the steps of a preset following the beat run at a tempo: the time they have run for.
pub fn steps_time(preset: &Preset, time: Ns, bpm: i64) -> Ns {
    if preset.flags & preset_flag::BEATS == 0 || preset.steps_bpm <= 0 || bpm <= 0 {
        return time;
    }

    mul_div(time, bpm, preset.steps_bpm)
}

fn transition_end(step: &Step, start: Ns, end: Ns) -> Ns {
    if end <= start {
        return start;
    }

    if step.flags & step_flag::TRANSITION == 0 {
        return end;
    }

    start.saturating_add(step.transition).min(end)
}

struct Order {
    steps: [u16; MAX_ORDER],
    len: usize,
}

impl Order {
    fn get(&self, index: usize) -> Option<u32> {
        if index < self.len {
            self.steps.get(index).map(|step| *step as u32)
        } else {
            None
        }
    }
}

fn shuffle(order: &mut Order, seed: i32, pass: i64) {
    let mut i = order.len;

    while i > 1 {
        i -= 1;
        // floor(hash * (i + 1)), on whole numbers
        let j = ((hash_bits(pass as i32, seed, i as i32) as u64 * (i as u64 + 1)) >> 32) as usize;

        if j < order.len {
            order.steps.swap(i, j);
        }
    }
}

/// The steps a pass plays, in the order they are played in.
fn order_of(show: &Show, preset: &Preset, pass: i64) -> Order {
    let mut order = Order {
        steps: [0; MAX_ORDER],
        len: 0,
    };

    for step in show.iter(preset.playable).take(MAX_ORDER) {
        if let Some(slot) = order.steps.get_mut(order.len) {
            *slot = step;
            order.len += 1;
        }
    }

    if preset.order == order::RANDOM && order.len >= 3 {
        let mut previous = Order {
            steps: order.steps,
            len: order.len,
        };
        shuffle(&mut order, preset.seed, pass);
        shuffle(&mut previous, preset.seed, pass.wrapping_sub(1));

        // a pass doesn't open with the step the one before it ended with
        let previous_last = previous.steps.get(previous.len - 1).copied();

        if order.steps.first().copied() == previous_last {
            order.steps.swap(0, 1);
        }
    }

    order
}

fn ordered_state(show: &Show, preset: &Preset, time: Ns) -> Option<StepSel> {
    let first = show.get(preset.steps, 0)?;
    let pass_length = preset.pass_length;
    let loops = preset.flags & preset_flag::LOOP != 0 && pass_length > 0;
    let mut offset = time - first.start;
    let mut pass = 0;

    if loops {
        pass = div_floor(offset, pass_length);
        offset = (offset - pass * pass_length).max(0);
    }

    let order = order_of(show, preset, pass);

    if order.len == 0 {
        return Some(StepSel::single(0));
    }

    if offset < 0 {
        return Some(StepSel::single(order.get(0)?));
    }

    let mut position = 0;
    let mut start: Ns = 0;

    while position < order.len - 1 {
        let length = show.get(preset.steps, order.get(position)? as usize)?.length;

        if start + length > offset {
            break;
        }

        start += length;
        position += 1;
    }

    let current_index = order.get(position)?;
    let current = show.get(preset.steps, current_index as usize)?;
    let end = start + current.length;

    let previous = if position > 0 {
        order.get(position - 1)
    } else if loops {
        let before = order_of(show, preset, pass - 1);
        before.get(before.len.checked_sub(1)?)
    } else {
        None
    };

    let transition_end = transition_end(&current, start, end);

    match previous {
        Some(previous) if offset < transition_end => Some(StepSel {
            from: previous as i32,
            to: current_index,
            position: transition_curve(current.curve as u8, ratio(offset - start, transition_end - start)),
        }),
        _ => Some(StepSel::single(current_index)),
    }
}

/// What a preset plays at a time counted from its own start.
pub fn state_at(show: &Show, preset: &Preset, time: Ns) -> StepSel {
    state_at_checked(show, preset, time).unwrap_or(StepSel::single(0))
}

fn state_at_checked(show: &Show, preset: &Preset, time: Ns) -> Option<StepSel> {
    let count = preset.steps.len();

    if count <= 1 {
        return Some(StepSel::single(0));
    }

    if preset.order != order::FORWARD {
        return ordered_state(show, preset, time);
    }

    let first = show.get(preset.steps, 0)?;
    let mut time = time;
    let mut loop_length = 0;

    if preset.flags & preset_flag::LOOP != 0 {
        loop_length = preset.loop_length;

        if loop_length > 0 {
            // fold the time into a single pass, starting over at the first step
            time = first.start + rem_floor(time - first.start, loop_length);
        }
    }

    // the step started last
    let mut index = None;
    let mut current = first;

    for (i, step) in show.iter(preset.steps).enumerate() {
        if step.start > time {
            break;
        }

        index = Some(i);
        current = step;
    }

    let index = match index {
        Some(index) => index,
        // before the first step: it is what the preset starts out with
        None => return Some(StepSel::single(0)),
    };

    let end = if index < count - 1 {
        show.get(preset.steps, index + 1)?.start
    } else {
        first.start + preset.loop_length
    };

    let previous = if index > 0 {
        Some(index - 1)
    } else if loop_length > 0 {
        Some(count - 1)
    } else {
        None
    };

    let transition_end = transition_end(&current, current.start, end);

    match previous {
        Some(previous) if time < transition_end => Some(StepSel {
            from: previous as i32,
            to: index as u32,
            position: transition_curve(
                current.curve as u8,
                ratio(time - current.start, transition_end - current.start),
            ),
        }),
        _ => Some(StepSel::single(index as u32)),
    }
}
