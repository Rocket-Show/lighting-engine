//! A frame: the values of every part of every fixture at a moment, and the universes they are sent in.
//!
//! Each part starts out with the defaults of its channels. The presets playing are mixed over them, the ones at the
//! bottom first: the values of their step, then their effects. The effects following what the presets give a fixture
//! (a movement, a curve on the pan, the tilt or the dimmer, a pixel effect in the colors of the presets) are played
//! once all presets are mixed.

use crate::fix::{beats_to_ns, rem_floor, Fix, Ns, NS_PER_MS};
use crate::format::{
    cap_flag, capv_flag, chan_flag, color, effect_flag, effect_kind, layout_flag, part_flag, pixel_flag, preset_flag,
    preset_part_flag, ty, CapV, Chan, Effect, Field, Layout, Preset, PresetPart, Show,
};

use super::caps::{
    apply_emitters, apply_fan, effect_amount, for_each_channel_value, has, has_percentage, is_pan_tilt, materialize,
    CapList, CapVal,
};
use super::channel::{
    axis_degrees, axis_range, axis_value, brightness_of_value, brightness_value, curve_value, dim, is_command,
    is_overriding, is_positioned, matches, pan_tilt_degrees, pan_tilt_value, positioned_step_value, type_mask, Ctx,
};
use super::curve::{capability_value, fanned, movement_at, run_loop, value_at};
use super::pixelfx::{mix_pixel, Base, RingFrames, Shared};
use super::steps::{state_at, StepSel};
use super::values::{Snapshot, Vals};
use super::{
    fixture_flag, get, put, Frame, Layer, Memory, Runtime, FOLLOWING_WORDS, LAYER_LIVE, LAYER_ON_BEAT, LAYER_PHASED,
    LAYER_REGION, UNIVERSE_SIZE,
};

/// The most channels of a part turning it on an axis.
const MAX_AXES: usize = 8;

/// how long a fixture being identified stays lit, and then dark, while it blinks
const BLINK: Ns = 500 * NS_PER_MS;

/// What a part of a fixture is, for everything played on it.
pub(super) struct PartInfo {
    pub index: u32,
    pub fixture: u32,
    pub profile: u16,
    pub layout: Layout,
    pub pixel: bool,
}

/// the part of a preset a part of a fixture is, if it is in the preset
fn find_part(show: &Show, preset: &Preset, part: u32) -> Option<(usize, PresetPart)> {
    let mut low = 0;
    let mut high = preset.parts.len();

    while low < high {
        let middle = (low + high) / 2;
        let candidate = show.get(preset.parts, middle)?;

        if candidate.part == part {
            return Some((middle, candidate));
        }

        if candidate.part < part {
            low = middle + 1;
        } else {
            high = middle;
        }
    }

    None
}

fn context(info: &PartInfo, part: &PresetPart) -> Ctx {
    Ctx {
        index: part.index as u32,
        count: part.count as u32,
        fan_place: if part.flags & preset_part_flag::FAN_PLACE != 0 {
            Some(part.fan_place)
        } else {
            None
        },
        fixture: info.fixture,
    }
}

/// the tempo the curves of a preset playing follow
pub(super) fn curve_bpm(layer: &Layer, frame: &Frame) -> Option<Fix> {
    if layer.has(LAYER_ON_BEAT) {
        Some(frame.clock_bpm)
    } else {
        frame.programmed_bpm
    }
}

/// The time an effect has run for. A preset which is not on a clip has no moment it starts at: an effect which stops
/// after a while repeats its run then.
pub(super) fn curve_time(effect: &Effect, layer: &Layer, count: u32, bpm: Option<Fix>) -> Ns {
    if layer.has(LAYER_REGION) {
        if effect.flags & effect_flag::BEAT_SYNCED != 0 && layer.has(LAYER_ON_BEAT) {
            // a curve following the beat runs on the beats of the tempo from outside, where it does
            return beats_to_ns(layer.beats_played, bpm.unwrap_or(0));
        }

        return layer.effect_time;
    }

    match run_loop(effect, count, bpm) {
        Some(loop_length) => rem_floor(layer.effect_time, loop_length),
        None => layer.effect_time,
    }
}

/// A color of a fading preset on a fixture with a dimmer doesn't fade on its own: the dimmer carries the fade. The
/// color blends with the one below instead, each of them weighted by how bright it lights.
fn mix_color(vals: &mut Vals, channel: usize, value: f32, intensity: f32, below: Option<f32>, preset_brightness: f32) {
    let weight_below = below.map(|below| (1.0 - intensity) * below).unwrap_or(0.0);
    let weight = intensity * preset_brightness;

    if below.is_none() || weight_below + weight <= 0.0 {
        vals.mix(channel, value, intensity);
        return;
    }

    if weight_below == 0.0 {
        // nothing to blend with (e.g. the preset doesn't fade)
        vals.set(channel, value);
        return;
    }

    let value_below = vals.get(channel).unwrap_or(0.0);
    vals.set(
        channel,
        (value_below * weight_below + value * weight) / (weight_below + weight),
    );
}

/// the color the values light (0 to 255 for each of red, green and blue), none if they don't name all three
fn approximated_color(caps: &CapList) -> Option<[f32; 3]> {
    let find = |wanted: u8| {
        caps.iter()
            .find(|value| {
                let v = &value.v;
                v.ty == ty::COLOR_INTENSITY
                    && (v.color == color::NONE || v.color == wanted)
                    && crate::format::no_name(v.wheel)
                    && crate::format::no_name(v.profile)
            })
            .and_then(|value| {
                if has(&value.v, capv_flag::PERCENTAGE) {
                    Some(255.0 * value.v.percentage)
                } else {
                    None
                }
            })
    };

    Some([find(color::RED)?, find(color::GREEN)?, find(color::BLUE)?])
}

fn slot_of(v: &CapV) -> Option<f32> {
    if has(v, capv_flag::SLOT) {
        Some(v.slot)
    } else {
        None
    }
}

/// Mix the values for the capabilities a preset gives a fixture over its channels. `curve_on_dimmer` tells whether an
/// effect of the preset plays on the dimmer (which counts as lighting it).
#[allow(clippy::too_many_arguments)]
pub(super) fn mix_caps(
    show: &Show,
    info: &PartInfo,
    caps: &mut CapList,
    vals: &mut Vals,
    intensity: f32,
    preset_flags: u32,
    curve_on_dimmer: &dyn Fn() -> bool,
    ctx: &Ctx,
) {
    let layout = &info.layout;
    let channels = layout.channels;
    let count = channels.len();
    let mut has_color = false;

    // how bright the fixture lights below this preset and how bright the preset lights it, to blend the colors
    let below = if layout.dimmer >= 0 {
        let dimmer = layout.dimmer as usize;

        Some(match (vals.get(dimmer), show.get(channels, dimmer)) {
            (Some(value), Some(chan)) => brightness_of_value(show, &chan, value),
            _ => 0.0,
        })
    } else {
        None
    };

    let preset_brightness = match below {
        None => 0.0,
        Some(below) => {
            let mut brightness = None;

            for value in caps.iter() {
                let v = &value.v;

                if v.ty == ty::INTENSITY
                    && has_percentage(v)
                    && (crate::format::no_name(v.profile) || v.profile == info.profile)
                {
                    // a later value overwrites an earlier one, like when they are mixed
                    brightness = Some(v.percentage);
                }
            }

            match brightness {
                Some(brightness) => brightness,
                None if curve_on_dimmer() => 1.0,
                None => below,
            }
        }
    };

    let approximated = approximated_color(caps);

    // a fixture mixing its colors with filters is given the colors the other way round, and the colors play on the
    // emitters besides red, green and blue
    apply_emitters(
        caps,
        layout.colors,
        layout.flags & layout_flag::COLOR_TEMPERATURE != 0,
        preset_flags & preset_flag::EMITTERS_FROM_COLOR != 0,
    );

    for value_index in 0..caps.len() {
        let value = match caps.get(value_index) {
            Some(value) => value,
            None => continue,
        };

        let v = &value.v;

        if is_overriding(v.ty) {
            // played over the rest below
            continue;
        }

        let pan_tilt = is_pan_tilt(v.ty);
        let positioned = is_positioned(v.ty) || is_command(v.ty);
        let brightness = (v.ty == ty::INTENSITY || v.ty == ty::COLOR_INTENSITY) && has_percentage(v);
        let mask = type_mask(v.ty);

        if pan_tilt && !crate::format::no_name(v.profile) && v.profile != info.profile {
            continue;
        }

        for channel in 0..count {
            let (_, flags, types) = match show.chan_head(channels, channel) {
                Some(head) => head,
                None => continue,
            };

            if pan_tilt {
                if flags & (chan_flag::PAN | chan_flag::TILT) == 0 {
                    continue;
                }

                // a position of a moving head is played on the channel as a whole, by where the fixture is placed
                let mirror = (v.ty == ty::PAN && preset_flags & preset_flag::MIRROR_PAN != 0)
                    || (v.ty == ty::TILT && preset_flags & preset_flag::MIRROR_TILT != 0);

                if let Some(chan) = show.get(channels, channel) {
                    if let Some(played) = pan_tilt_value(show, &chan, &value, mirror, Some(ctx)) {
                        vals.set(channel, played);
                    }
                }

                continue;
            }

            if types & mask == 0 {
                continue;
            }

            let chan = match show.get(channels, channel) {
                Some(chan) => chan,
                None => continue,
            };

            if positioned {
                // a position (and a program or maintenance) is played on the channel as a whole
                if let Some(played) = positioned_step_value(show, &chan, v, info.profile) {
                    vals.set(channel, played);
                }

                continue;
            }

            if brightness {
                // a dimmer and a color: the capabilities of the channel giving it
                if let Some(played) = brightness_value(show, &chan, |cap| matches(v, cap, info.profile), v.percentage) {
                    if v.ty == ty::COLOR_INTENSITY {
                        mix_color(vals, channel, played, intensity, below, preset_brightness);
                        has_color = true;
                    } else {
                        vals.mix(channel, played, intensity);
                    }
                }

                continue;
            }

            if v.ty != ty::WHEEL_SLOT {
                continue;
            }

            for cap in show.iter(chan.caps) {
                let cap_slot = if cap.flags & cap_flag::SLOT != 0 {
                    Some(cap.slot)
                } else {
                    None
                };

                if matches(v, &cap, info.profile) && cap_slot == slot_of(v) {
                    // a slot of a wheel (a color, a gobo ...)
                    vals.set(channel, cap.center);

                    if cap.flags & cap_flag::WHEEL_IS_COLOR != 0 {
                        has_color = true;
                    }
                }
            }
        }
    }

    // The pulse of an iris or a frost and the shake of a gobo play over what the rest puts on the same channel
    for value in caps.iter() {
        if !is_overriding(value.v.ty) {
            continue;
        }

        let mask = type_mask(value.v.ty);

        for channel in 0..count {
            match show.chan_head(channels, channel) {
                Some((_, _, types)) if types & mask != 0 => {}
                _ => continue,
            }

            if let Some(chan) = show.get(channels, channel) {
                if let Some(played) = positioned_step_value(show, &chan, &value.v, info.profile) {
                    vals.set(channel, played);
                }
            }
        }
    }

    // A fixture which lights no color of its own shows the color on its color wheel, or else on its color presets,
    // approximated from the colors set
    if has_color {
        return;
    }

    let wanted = match approximated {
        Some(wanted) => wanted,
        None => return,
    };

    let closest = |chan: &Chan, flag: u32| {
        let mut lowest = f32::MAX;
        let mut found = None;

        for cap in show.iter(chan.caps) {
            if cap.flags & flag == 0 {
                continue;
            }

            let (red, green, blue) = if flag == cap_flag::SLOT_COLOR {
                (cap.slot_red, cap.slot_green, cap.slot_blue)
            } else {
                (cap.preset_red, cap.preset_green, cap.preset_blue)
            };
            let difference = (red - wanted[0]).abs() + (green - wanted[1]).abs() + (blue - wanted[2]).abs();

            if difference < lowest {
                lowest = difference;
                found = Some(cap.center);
            }
        }

        found
    };

    if layout.flags & layout_flag::COLOR_WHEEL != 0 {
        for channel in 0..count {
            match show.chan_head(channels, channel) {
                Some((_, flags, _)) if flags & chan_flag::COLOR_WHEEL != 0 => {}
                _ => continue,
            }

            if let Some(center) = show
                .get(channels, channel)
                .and_then(|chan| closest(&chan, cap_flag::SLOT_COLOR))
            {
                vals.set(channel, center);
            }
        }

        return;
    }

    for channel in 0..count {
        match show.chan_head(channels, channel) {
            Some((_, _, types)) if types & type_mask(ty::COLOR_PRESET) != 0 => {}
            _ => continue,
        }

        if let Some(center) = show
            .get(channels, channel)
            .and_then(|chan| closest(&chan, cap_flag::PRESET_COLOR))
        {
            vals.set(channel, center);
        }
    }
}

/// the channel of a part with a name
fn channel_named(show: &Show, layout: &Layout, name: u16) -> Option<usize> {
    (0..layout.channels.len()).find(
        |channel| matches!(show.chan_head(layout.channels, *channel), Some((candidate, _, _)) if candidate == name),
    )
}

fn push_following(following: &mut [u32], count: &mut usize, layer: usize, effect: usize, intensity: f32) {
    if (*count + 1) * FOLLOWING_WORDS <= following.len() {
        put(following, *count * FOLLOWING_WORDS, layer as u32);
        put(following, *count * FOLLOWING_WORDS + 1, effect as u32);
        put(following, *count * FOLLOWING_WORDS + 2, intensity.to_bits());
        *count += 1;
    }
}

/// whether a curve on the capability follows what the presets give a fixture
fn follows_presets(t: u8) -> bool {
    t == ty::INTENSITY || t == ty::PAN || t == ty::TILT
}

/// The effects of a preset on a part of a fixture: the ones following the presets are kept for later.
#[allow(clippy::too_many_arguments)]
fn mix_effects(
    show: &Show,
    info: &PartInfo,
    frame: &Frame,
    layer: &Layer,
    layer_index: usize,
    preset: &Preset,
    sel: &StepSel,
    part: &PresetPart,
    place_index: usize,
    ctx: &Ctx,
    vals: &mut Vals,
    following: &mut [u32],
    following_count: &mut usize,
    covering: &mut i32,
    shared: &mut Shared,
) {
    let mirror_pan = preset.flags & preset_flag::MIRROR_PAN != 0;
    let mirror_tilt = preset.flags & preset_flag::MIRROR_TILT != 0;

    for (effect_index, effect) in show.iter(preset.effects).enumerate() {
        // the effect keeps running through the steps, they only open or close it
        let amount = effect_amount(show, preset, sel, effect_index as u32);
        let intensity = layer.intensity * amount;

        if !(amount > 0.0) {
            continue;
        }

        if effect.kind == effect_kind::MOVEMENT {
            // a movement runs around where all the presets point the heads
            push_following(following, following_count, layer_index, effect_index, intensity);
            continue;
        }

        if effect.kind == effect_kind::PIXEL {
            let pixel = match show.get(effect.pixel, 0) {
                Some(pixel) => pixel,
                None => continue,
            };

            if pixel.flags & pixel_flag::PRESET_PALETTE != 0
                || engine_pixel::Blend::from_u8(pixel.blend).follows_presets()
            {
                push_following(following, following_count, layer_index, effect_index, intensity);
            } else if mix_pixel(
                show,
                info,
                frame,
                layer,
                (layer_index, effect_index),
                preset,
                &effect,
                &pixel,
                part,
                place_index,
                vals,
                Base::Own,
                intensity,
                shared,
            ) {
                *covering = (*covering).max(layer_index as i32);
            }

            continue;
        }

        if show.iter(effect.caps).any(|cap| follows_presets(cap.ty)) {
            push_following(following, following_count, layer_index, effect_index, intensity);

            // a curve only on what follows the presets is worked out where it is played, after them: its value
            // would not be used here
            if effect.chans.is_empty() && show.iter(effect.caps).all(|cap| follows_presets(cap.ty)) {
                continue;
            }
        }

        let bpm = curve_bpm(layer, frame);
        let time = curve_time(&effect, layer, ctx.count, bpm);

        // swung as far as the place of the fixture in the fan of the preset says
        let value = match fanned(
            &effect,
            value_at(&effect, time, ctx.index, ctx.count, bpm),
            ctx.fan_place,
        ) {
            Some(value) => value,
            // the curve does not apply anymore after it has finished running
            None => continue,
        };

        for cap in show.iter(effect.caps) {
            if follows_presets(cap.ty) {
                continue;
            }

            let played = capability_value(&effect, &cap, value);
            let mask = type_mask(cap.ty)
                | if cap.ty == ty::SHUTTER_STROBE {
                    type_mask(ty::STROBE_SPEED)
                } else {
                    0
                };
            let mirror = (cap.ty == ty::PAN && mirror_pan) || (cap.ty == ty::TILT && mirror_tilt);

            for channel in 0..info.layout.channels.len() {
                match show.chan_head(info.layout.channels, channel) {
                    Some((_, _, types)) if types & mask != 0 => {}
                    _ => continue,
                }

                if let Some(chan) = show.get(info.layout.channels, channel) {
                    if let Some(channel_value) = curve_value(show, &chan, &played, mirror) {
                        vals.mix(channel, channel_value, intensity);
                    }
                }
            }
        }

        for curve_channel in show.iter(effect.chans) {
            if curve_channel.profile != info.profile {
                continue;
            }

            for channel in 0..info.layout.channels.len() {
                match show.chan_head(info.layout.channels, channel) {
                    Some((name, _, _)) if name == curve_channel.name => {}
                    _ => continue,
                }

                if let Some(chan) = show.get(info.layout.channels, channel) {
                    vals.mix(channel, chan.max * value, intensity);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Axis {
    channel: usize,
    ty: u8,
    range: f32,
    center: f32,
}

/// The effects following what the presets give a fixture, in the order of their presets, the one on top over the
/// others.
#[allow(clippy::too_many_arguments)]
fn mix_following(
    show: &Show,
    info: &PartInfo,
    frame: &Frame,
    layers: &[u32],
    following: &[u32],
    following_count: usize,
    covering: i32,
    vals: &mut Vals,
    snapshot_values: &mut [u32],
    snapshot_flags: &mut [u32],
    shared: &mut Shared,
) {
    if following_count == 0 {
        return;
    }

    let layout = &info.layout;
    let count = layout.channels.len();

    // what the presets give the fixture, before the effects following them
    for channel in 0..count {
        put(snapshot_values, channel, get(vals.values, channel));
        put(snapshot_flags, channel, get(vals.flags, channel));
    }

    let snapshot = Snapshot {
        values: snapshot_values,
        flags: snapshot_flags,
    };

    // where the presets point the fixture on each axis
    let mut axes = [Axis {
        channel: 0,
        ty: 0,
        range: 0.0,
        center: 0.0,
    }; MAX_AXES];
    let mut axis_count = 0;

    for channel in 0..count {
        match show.chan_head(layout.channels, channel) {
            Some((_, flags, _)) if flags & (chan_flag::PAN | chan_flag::TILT) != 0 => {}
            _ => continue,
        }

        let chan = match show.get(layout.channels, channel) {
            Some(chan) => chan,
            None => continue,
        };

        for t in [ty::PAN, ty::TILT] {
            if let Some(range) = axis_range(&chan, t) {
                if let Some(axis) = axes.get_mut(axis_count) {
                    *axis = Axis {
                        channel,
                        ty: t,
                        range,
                        center: snapshot
                            .get(channel)
                            .map(|value| axis_degrees(&chan, range, value))
                            .unwrap_or(0.0),
                    };
                    axis_count += 1;
                }
            }
        }
    }

    // how bright the presets light the dimmer: full, if none of them sets it (it still has its default value)
    let mut brightness = 1.0;

    if layout.dimmer >= 0 {
        let dimmer = layout.dimmer as usize;

        if let (Some(value), true, Some(chan)) = (
            snapshot.get(dimmer),
            vals.touched(dimmer),
            show.get(layout.channels, dimmer),
        ) {
            brightness = brightness_of_value(show, &chan, value);
        }
    }

    for item in 0..following_count {
        let layer_index = get(following, item * FOLLOWING_WORDS) as usize;
        let effect_index = get(following, item * FOLLOWING_WORDS + 1) as usize;
        let intensity = f32::from_bits(get(following, item * FOLLOWING_WORDS + 2));
        let layer = Layer::load(layers, layer_index);

        let preset = match show.get(show.root.presets, layer.preset as usize) {
            Some(preset) => preset,
            None => continue,
        };

        let (effect, (place_index, part)) = match (
            show.get(preset.effects, effect_index),
            find_part(show, &preset, info.index),
        ) {
            (Some(effect), Some(found)) => (effect, found),
            _ => continue,
        };

        let ctx = context(info, &part);

        if effect.kind == effect_kind::PIXEL {
            if layer_index as i32 >= covering {
                if let Some(pixel) = show.get(effect.pixel, 0) {
                    mix_pixel(
                        show,
                        info,
                        frame,
                        &layer,
                        (layer_index, effect_index),
                        &preset,
                        &effect,
                        &pixel,
                        &part,
                        place_index,
                        vals,
                        Base::Snapshot(&snapshot),
                        intensity,
                        shared,
                    );
                }
            }

            continue;
        }

        let bpm = curve_bpm(&layer, frame);
        let time = curve_time(&effect, &layer, ctx.count, bpm);
        let mirrors = |t: u8| {
            (t == ty::PAN && preset.flags & preset_flag::MIRROR_PAN != 0)
                || (t == ty::TILT && preset.flags & preset_flag::MIRROR_TILT != 0)
        };

        if effect.kind == effect_kind::MOVEMENT {
            // the movement does not apply anymore after it has finished running
            let (pan, tilt) = match movement_at(&effect, time, ctx.index, ctx.count, bpm, ctx.fan_place) {
                Some(offset) => offset,
                None => continue,
            };

            for axis in axes.iter().take(axis_count) {
                let movement = if axis.ty == ty::PAN { pan } else { tilt };
                let degrees = axis.center + if mirrors(axis.ty) { -movement } else { movement };

                if let Some(chan) = show.get(layout.channels, axis.channel) {
                    vals.mix(axis.channel, axis_value(&chan, axis.range, degrees), intensity);
                }
            }

            continue;
        }

        // a curve, swung as far as the place of the fixture in the fan of the preset says
        let value = match fanned(
            &effect,
            value_at(&effect, time, ctx.index, ctx.count, bpm),
            ctx.fan_place,
        ) {
            Some(value) => value,
            None => continue,
        };

        for cap in show.iter(effect.caps) {
            if !follows_presets(cap.ty) {
                continue;
            }

            if cap.ty == ty::INTENSITY {
                // pulses the brightness the presets light the fixture with
                let played = capability_value(&effect, &cap, value * brightness);

                for channel in 0..count {
                    match show.chan_head(layout.channels, channel) {
                        Some((_, _, types)) if types & type_mask(ty::INTENSITY) != 0 => {}
                        _ => continue,
                    }

                    if let Some(chan) = show.get(layout.channels, channel) {
                        if let Some(channel_value) = curve_value(show, &chan, &played, false) {
                            vals.mix(channel, channel_value, intensity);
                        }
                    }
                }

                continue;
            }

            // the pan or the tilt: as far from where the presets point the heads as the curve says from the middle
            let played = capability_value(&effect, &cap, value);

            for axis in axes.iter().take(axis_count) {
                if axis.ty != cap.ty {
                    continue;
                }

                if let (Some(degrees), Some(chan)) = (
                    pan_tilt_degrees(show, &played, axis.range, mirrors(cap.ty), None),
                    show.get(layout.channels, axis.channel),
                ) {
                    vals.mix(
                        axis.channel,
                        axis_value(&chan, axis.range, axis.center + degrees),
                        intensity,
                    );
                }
            }
        }
    }
}

/// what identifies a fixture: full white or dark, the shutter open, the iris open and the color wheels on their open slot
fn identify(show: &Show, info: &PartInfo, vals: &mut Vals, time: Ns) {
    let lit = rem_floor(crate::fix::div_floor(time, BLINK), 2) == 0;
    let level = if lit { 1.0 } else { 0.0 };
    let mut caps = CapList::new();

    caps.push(CapVal::percentage(ty::INTENSITY, color::NONE, level));

    for c in [
        color::RED,
        color::GREEN,
        color::BLUE,
        color::WHITE,
        color::WARM_WHITE,
        color::COLD_WHITE,
    ] {
        caps.push(CapVal::percentage(ty::COLOR_INTENSITY, c, level));
    }

    caps.push(CapVal::percentage(ty::SHUTTER_STROBE, color::NONE, 0.0));
    caps.push(CapVal::percentage(ty::IRIS, color::NONE, 1.0));

    for slot in show.iter(info.layout.open_slots) {
        caps.push(CapVal::of(CapV {
            ty: ty::WHEEL_SLOT,
            flags: capv_flag::SLOT,
            wheel: slot.wheel,
            slot: slot.slot,
            profile: info.profile,
            ..CapV::default()
        }));
    }

    let ctx = Ctx {
        index: 0,
        count: 1,
        fan_place: None,
        fixture: u32::MAX,
    };
    mix_caps(show, info, &mut caps, vals, 1.0, 0, &|| false, &ctx);
}

/// Work out the values of every part of every fixture for the moment of a frame.
/// Work out the values of the parts on the first so many universes: the ones the host takes of the frame.
pub(super) fn render_parts(show: &Show, rt: &Runtime, m: &mut Memory, frame: &Frame, universes: usize) {
    let root = show.root;
    let all = root.universes as usize <= universes;
    let frames = RingFrames {
        audio: &*m.audio,
        newest: rt.audio_newest,
        count: rt.audio_count,
    };
    let mut shared = Shared::new(&frames, frame.time);
    let mut caps = CapList::new();

    for (index, part) in show.iter(root.parts).enumerate() {
        // A part on a universe the host doesn't take is not worked out: nothing of another part depends on it (the
        // same lamp is on the same universe)
        if !all {
            if let Some(patch) = show.patch(part.fixture as usize) {
                if patch.universe as usize >= universes {
                    continue;
                }
            }
        }

        let (layout, fixture) = match (
            show.get(root.layouts, part.layout as usize),
            show.get(root.fixtures, part.fixture as usize),
        ) {
            (Some(layout), Some(fixture)) => (layout, fixture),
            _ => continue,
        };

        let count = layout.channels.len();
        let base = part.value_base as usize;

        if part.same_lamp >= 0 {
            // a part on the same address as one before is the same lamp: it takes its values
            if let Some(other) = show.get(root.parts, part.same_lamp as usize) {
                let from = other.value_base as usize;

                if from + count <= base && base + count <= m.values.len() {
                    m.values.copy_within(from..from + count, base);
                    m.flags.copy_within(from..from + count, base);
                }
            }
        }

        let (values, flags) = match (
            m.values.get_mut(base..base + count),
            m.flags.get_mut(base..base + count),
        ) {
            (Some(values), Some(flags)) => (values, flags),
            _ => continue,
        };

        let mut vals = Vals { values, flags };
        let info = PartInfo {
            index: index as u32,
            fixture: part.fixture,
            profile: fixture.profile,
            layout,
            pixel: part.flags & part_flag::PIXEL != 0,
        };

        if part.same_lamp < 0 {
            vals.clear();

            // the values the channels start out with
            for channel in 0..count {
                if let Some(default) = show.chan_default(layout.channels, channel) {
                    vals.set_default(channel, default);
                }
            }

            let mut following_count = 0;
            let mut covering = -1;

            for layer_index in 0..frame.layers {
                let layer = Layer::load(m.layers, layer_index);

                let preset = match show.get(root.presets, layer.preset as usize) {
                    Some(preset) => preset,
                    None => continue,
                };

                // this part is in the preset as well: mix its values, overwriting the ones below
                let (place_index, preset_part) = match find_part(show, &preset, index as u32) {
                    Some(found) => found,
                    None => continue,
                };

                // a preset chasing its steps over its fixtures is at a different point of the sequence for each of them
                let sel = if layer.has(LAYER_PHASED) {
                    state_at(show, &preset, layer.steps_base - preset_part.phasing)
                } else {
                    layer.sel
                };
                let ctx = context(&info, &preset_part);

                // what the preset puts on this fixture, the fans worked out for its place in them
                materialize(show, &preset, &sel, &mut caps);
                apply_fan(show, &mut caps, ctx.fan_place);

                let curve_on_dimmer = || {
                    show.iter(preset.effects).enumerate().any(|(effect_index, effect)| {
                        effect.flags & effect_flag::INTENSITY != 0
                            && effect_amount(show, &preset, &sel, effect_index as u32) > 0.0
                    })
                };

                mix_caps(
                    show,
                    &info,
                    &mut caps,
                    &mut vals,
                    layer.intensity,
                    preset.flags,
                    &curve_on_dimmer,
                    &ctx,
                );

                for_each_channel_value(show, &preset, &sel, |profile, name, value| {
                    if profile == info.profile {
                        if let Some(channel) = channel_named(show, &info.layout, name) {
                            vals.mix(channel, value, layer.intensity);
                        }
                    }
                });

                mix_effects(
                    show,
                    &info,
                    frame,
                    &layer,
                    layer_index,
                    &preset,
                    &sel,
                    &preset_part,
                    place_index,
                    &ctx,
                    &mut vals,
                    m.following,
                    &mut following_count,
                    &mut covering,
                    &mut shared,
                );
            }

            mix_following(
                show,
                &info,
                frame,
                m.layers,
                m.following,
                following_count,
                covering,
                &mut vals,
                m.snapshot_values,
                m.snapshot_flags,
                &mut shared,
            );
        }

        let fixture_flags = get(m.fixture_flags, part.fixture as usize);

        // a fixture being identified blinks, whatever the presets light it with
        if fixture_flags & fixture_flag::IDENTIFY != 0 {
            identify(show, &info, &mut vals, frame.time);
        }

        // a fixture asked to reset itself or to switch its lamp on does it, whatever the presets do with it
        if fixture_flags & fixture_flag::RESET != 0 && info.layout.reset >= 0 {
            vals.set(info.layout.reset as usize, info.layout.reset_value);
        }

        if fixture_flags & fixture_flag::LAMP_ON != 0 && info.layout.lamp_on >= 0 {
            vals.set(info.layout.lamp_on as usize, info.layout.lamp_on_value);
        }
    }

    let _ = (LAYER_LIVE, Chan::SIZE);
}

/// Write the universes from the values of the last frame: each channel of each part at its address, the brightness
/// dimmed by the master dimmers, a channel with fine channels as its bytes.
pub(super) fn write_universes(show: &Show, rt: &Runtime, m: &Memory, out: &mut [u8]) {
    let root = show.root;
    let universes = (root.universes as usize).min(out.len() / UNIVERSE_SIZE);

    for byte in out.iter_mut() {
        *byte = 0;
    }

    let master_dimmer = if rt.blackout {
        0.0
    } else {
        root.master_dimmer * rt.master
    };

    for part in show.iter(root.parts) {
        let (layout, patch) = match (
            show.get(root.layouts, part.layout as usize),
            show.patch(part.fixture as usize),
        ) {
            (Some(layout), Some(patch)) => (layout, patch),
            _ => continue,
        };

        if patch.universe as usize >= universes {
            continue;
        }

        let base = part.value_base as usize;
        let count = layout.channels.len();

        let snapshot = match (m.values.get(base..base + count), m.flags.get(base..base + count)) {
            (Some(values), Some(flags)) => Snapshot { values, flags },
            _ => continue,
        };

        let master = master_dimmer * patch.brightness;

        // How much the colors of a fixture with a virtual dimmer are dimmed when they are sent: by the virtual dimmer
        // and by the master dimmer
        let color_dimmer = if layout.virtual_dimmer >= 0 {
            let dimmer = layout.virtual_dimmer as usize;

            Some(
                match (snapshot.get(dimmer), show.get(layout.channels, dimmer)) {
                    (Some(value), Some(chan)) => brightness_of_value(show, &chan, value),
                    _ => 0.0,
                } * master,
            )
        } else {
            None
        };

        let dimmed = master < 1.0 || color_dimmer.map(|dimmer| dimmer < 1.0).unwrap_or(false);

        for output in show.iter(layout.outputs) {
            let mut value = match snapshot.get(output.chan as usize) {
                Some(value) => value,
                None => continue,
            };

            if dimmed {
                if let Some(chan) = show.get(layout.channels, output.chan as usize) {
                    value = dim(show, &chan, value, ty::INTENSITY, master);

                    if let Some(color_dimmer) = color_dimmer {
                        value = dim(show, &chan, value, ty::COLOR_INTENSITY, color_dimmer);
                    }
                }
            }

            let address = patch.address as i64 + output.address as i64;

            if (0..UNIVERSE_SIZE as i64).contains(&address) {
                if let Some(slot) = out.get_mut(patch.universe as usize * UNIVERSE_SIZE + address as usize) {
                    *slot = ((value as u32) >> output.shift.min(31)) as u8;
                }
            }
        }
    }

    // a channel held by the host sends the value it is held at
    for (index, slot) in out.iter_mut().enumerate().take(universes * UNIVERSE_SIZE) {
        if get(m.override_mask, index / 32) & (1 << (index % 32)) != 0 {
            *slot = (get(m.override_values, index / 4) >> ((index % 4) * 8)) as u8;
        }
    }
}
