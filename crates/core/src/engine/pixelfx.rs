//! A pixel effect on a part of a fixture (a pixel, or a fixture without pixels): the color of its pattern where the
//! part is in its fixture or on the stage, blended with the color the presets put on it.

use engine_pixel::audio::{AudioFrame, AudioFrames, AudioSettings, PixelAudio, FRAME_LENGTH, SILENCE_DB};
use engine_pixel::{
    color_of, sample_with, shades_of, Align, AudioBand, Between, Bitmap, Blend, Cycle, Direction, Fit, ImageColors,
    Pattern, PatternSettings, PixelContext, Repeat, Scratch, Scroll, Shape, Stops, TextColors,
};

use crate::fix::{Ns, FIX_ONE};
use crate::format::{pixel_flag, Effect, PixelFx, Place, Preset, PresetPart, Show};

use super::curve::{cycles_at, still_cycles};
use super::frame::{curve_bpm, curve_time, PartInfo};
use super::values::{Snapshot, Vals};
use super::{get, Frame, Layer};

/// the level of a frame of audio which isn't there
pub(super) const SILENCE: f32 = SILENCE_DB;

/// The frames of audio the host handed over, the newest ones.
pub(super) struct RingFrames<'m> {
    pub audio: &'m [u32],
    pub newest: i64,
    pub count: usize,
}

impl AudioFrames for RingFrames<'_> {
    fn frame(&self, index: i64) -> Option<AudioFrame> {
        let capacity = self.audio.len() / FRAME_LENGTH;

        if capacity == 0
            || self.count == 0
            || index < 0
            || index > self.newest
            || index <= self.newest - self.count as i64
        {
            return None;
        }

        let slot = index.rem_euclid(capacity as i64) as usize * FRAME_LENGTH;
        let mut frame = [SILENCE_DB; FRAME_LENGTH];

        for (value, target) in frame.iter_mut().enumerate() {
            *target = f32::from_bits(get(self.audio, slot + value));
        }

        Some(frame)
    }
}

const CACHED: usize = 4;

/// The audio the effects of a frame follow: worked out once for an effect and a frame, asked for every pixel.
pub(super) struct AudioCache<'f> {
    frames: &'f RingFrames<'f>,
    time_micros: u64,
    entries: [Option<(u32, PixelAudio<'f>)>; CACHED],
    next: usize,
}

impl<'f> AudioCache<'f> {
    pub fn new(frames: &'f RingFrames<'f>, time: Ns) -> Self {
        AudioCache {
            frames,
            time_micros: (time.max(0) / 1000) as u64,
            entries: [None, None, None, None],
            next: 0,
        }
    }

    fn get(&mut self, key: u32, settings: AudioSettings) -> Option<&PixelAudio<'f>> {
        let found = self
            .entries
            .iter()
            .position(|entry| matches!(entry, Some((known, _)) if *known == key));

        let index = match found {
            Some(index) => index,
            None => {
                let index = self.next % CACHED;
                self.next = index + 1;

                if let Some(entry) = self.entries.get_mut(index) {
                    *entry = Some((key, PixelAudio::new(self.frames, self.time_micros, settings)));
                }

                index
            }
        };

        self.entries
            .get(index)
            .and_then(|entry| entry.as_ref())
            .map(|(_, audio)| audio)
    }
}

/// A pixel effect on a fixture: which layer plays it, which effect of its preset it is, and the fixture among the
/// fixtures of the preset.
#[derive(Clone, Copy, PartialEq)]
struct Running {
    layer: u32,
    effect: u32,
    fixture: u32,
}

/// What the parts of a frame share: the audio the effects follow, where a pixel effect is in its period on a fixture
/// (the same for all the pixels of the fixture, and worked out with divisions a small processor takes long for), and
/// the sines of the patterns which several pixels ask for.
pub(super) struct Shared<'f> {
    audio: AudioCache<'f>,
    cycles: [Option<(Running, Option<Cycle>)>; CACHED],
    next_cycle: usize,
    curves: [Option<(Curve, Option<f32>)>; CURVES],
    next_curve: usize,
    mappings: [Option<(Mapping, Option<f32>)>; MAPPINGS],
    next_mapping: usize,
    scratch: Scratch,
}

/// A curve on fixtures the chase shifts by the same time: which layer plays it, which effect of its preset it is,
/// and the time. In a frame, they all come to the same value (before the fan swings it).
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Curve {
    pub layer: u32,
    pub effect: u32,
    pub phasing: Ns,
}

/// The curves a frame keeps the value of: the fixtures of a preset are worked out one after the other, so the curves
/// of the presets playing at once are enough.
const CURVES: usize = 8;

/// A value of a curve on a channel: which channel of the show (where its list starts and its place in it), which
/// capability of the curve (where the list of them starts and its place in it), the value of the curve, and whether
/// the preset mirrors it. The same come to the same value of the channel.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Mapping {
    pub chans: u32,
    pub channel: u32,
    pub caps: u32,
    pub cap: u32,
    pub value: u32,
    pub mirror: bool,
}

/// The values on channels a frame keeps: the channels a curve plays on in one fixture.
const MAPPINGS: usize = 4;

impl<'f> Shared<'f> {
    pub fn new(frames: &'f RingFrames<'f>, time: Ns) -> Self {
        Shared {
            audio: AudioCache::new(frames, time),
            cycles: [None; CACHED],
            next_cycle: 0,
            curves: [None; CURVES],
            next_curve: 0,
            mappings: [None; MAPPINGS],
            next_mapping: 0,
            scratch: Scratch::new(),
        }
    }

    /// Where a pixel effect is in its period on a fixture (none: it does not apply at this moment), worked out the
    /// first time it is asked for in the frame.
    fn cycle(&mut self, running: Running, work_out: impl FnOnce() -> Option<Cycle>) -> Option<Cycle> {
        for (known, cycle) in self.cycles.iter().flatten() {
            if *known == running {
                return *cycle;
            }
        }

        let cycle = work_out();
        let index = self.next_cycle % CACHED;

        self.next_cycle = index + 1;

        if let Some(entry) = self.cycles.get_mut(index) {
            *entry = Some((running, cycle));
        }

        cycle
    }

    /// The value of a curve on the fixtures the chase shifts by the same time (none: it does not apply at this
    /// moment), worked out the first time it is asked for in the frame. A curve without a chase is worked out once for
    /// all the fixtures of its preset.
    pub fn curve(&mut self, curve: Curve, work_out: impl FnOnce() -> Option<f32>) -> Option<f32> {
        for (known, value) in self.curves.iter().flatten() {
            if *known == curve {
                return *value;
            }
        }

        let value = work_out();
        let index = self.next_curve % CURVES;

        self.next_curve = index + 1;

        if let Some(entry) = self.curves.get_mut(index) {
            *entry = Some((curve, value));
        }

        value
    }

    /// The value a curve puts on a channel (none: the channel doesn't take it), worked out the first time it is asked
    /// for in the frame: the fixtures of one model which a curve without a chase plays all ask for the same.
    pub fn mapping(&mut self, mapping: Mapping, work_out: impl FnOnce() -> Option<f32>) -> Option<f32> {
        for (known, value) in self.mappings.iter().flatten() {
            if *known == mapping {
                return *value;
            }
        }

        let value = work_out();
        let index = self.next_mapping % MAPPINGS;

        self.next_mapping = index + 1;

        if let Some(entry) = self.mappings.get_mut(index) {
            *entry = Some((mapping, value));
        }

        value
    }
}

/// Where the color the presets put on a part is read from: the values being mixed, or the ones all presets came to.
pub(super) enum Base<'s> {
    Own,
    Snapshot(&'s Snapshot<'s>),
}

fn context_of(place: &Place) -> PixelContext {
    PixelContext {
        x: place.x,
        y: place.y,
        column: place.column,
        row: place.row,
        columns: place.columns,
        rows: place.rows,
        aspect_x: place.aspect_x,
        aspect_y: place.aspect_y,
        seed: 0,
    }
}

/// Play a pixel effect on a part of a fixture: the effect of a number of the preset a layer of a number plays.
/// Whether the effect lit the part.
#[allow(clippy::too_many_arguments)]
pub(super) fn mix_pixel(
    show: &Show,
    info: &PartInfo,
    frame: &Frame,
    layer: &Layer,
    (layer_index, effect_index): (usize, usize),
    preset: &Preset,
    effect: &Effect,
    pixel: &PixelFx,
    part: &PresetPart,
    place_index: usize,
    vals: &mut Vals,
    base: Base,
    intensity: f32,
    shared: &mut Shared,
) -> bool {
    let layout = &info.layout;
    let channels = [layout.red, layout.green, layout.blue];
    let lights_in_colors = channels.iter().any(|channel| *channel >= 0);

    // a pixel lighting in white only has a dimmer of its own, which the effect lights
    let white_dimmer = if !lights_in_colors && info.pixel && layout.white_pixel_dimmer >= 0 {
        Some(layout.white_pixel_dimmer as usize)
    } else {
        None
    };

    if !lights_in_colors && white_dimmer.is_none() {
        return false;
    }

    let fixture_index = part.whole_index as u32;
    let running = Running {
        layer: layer_index as u32,
        effect: effect_index as u32,
        fixture: fixture_index,
    };
    let cycle = shared.cycle(running, || {
        let fixture_count = preset.whole_count;
        let bpm = curve_bpm(layer, frame);
        let time = curve_time(effect, layer, fixture_count, bpm);

        // a still pattern stands where its phase and the chase put it
        if pixel.flags & pixel_flag::STILL != 0 {
            Some(still_cycles(effect, fixture_index, fixture_count, bpm))
        } else {
            cycles_at(effect, time, fixture_index, fixture_count, bpm)
        }
    });
    let cycle = match cycle {
        Some(cycle) => cycle,
        // the effect does not apply anymore after it has finished running
        None => return false,
    };

    let max_of = |channel: usize| show.get(layout.channels, channel).map(|chan| chan.max).unwrap_or(255.0);

    let base_value = |vals: &Vals, channel: usize| match &base {
        Base::Own => vals.get(channel),
        Base::Snapshot(snapshot) => snapshot.get(channel),
    };

    // the color the presets put on the part, between 0 and 1 (white at full on a pixel lighting in white only)
    let mut base_color = [1.0f32; 3];

    if white_dimmer.is_none() {
        for (index, channel) in channels.iter().enumerate() {
            let value = if *channel >= 0 {
                base_value(vals, *channel as usize)
                    .map(|value| value / max_of(*channel as usize))
                    .unwrap_or(0.0)
            } else {
                0.0
            };

            if let Some(slot) = base_color.get_mut(index) {
                *slot = value;
            }
        }
    }

    // Where the part is in its fixture, every fixture starting the random looking patterns somewhere else. Spread
    // across the stage, where it is in the picture of the stage, which is one for all of them.
    let stage = pixel.flags & pixel_flag::STAGE != 0;
    let mut context = context_of(&layout.place);

    if stage {
        let places = if pixel.flags & pixel_flag::VIEW_TOP != 0 {
            preset.stage_top
        } else {
            preset.stage_front
        };

        if let Some(place) = show.get(places, place_index) {
            if place.known != 0 {
                context = context_of(&place.place);
            }
        }
    }

    let seed = pixel.seed.wrapping_add(if stage {
        0
    } else {
        (fixture_index as i64 * 131).wrapping_mul(FIX_ONE)
    });
    context.seed = (seed / FIX_ONE) as i32;

    if pixel.flags & pixel_flag::MIRROR != 0 {
        context.x = if context.x < 0.5 {
            context.x * 2.0
        } else {
            (1.0 - context.x) * 2.0
        };
    }

    let pattern = Pattern::from_u8(pixel.pattern);

    // The audio it follows: shown by the level meter and the wave of the audio, followed with its brightness or its
    // size by the other patterns
    let followed = if pixel.flags & pixel_flag::AUDIO != 0 {
        let settings = AudioSettings {
            band: AudioBand::from_u8(pixel.audio_band),
            gain_db: pixel.audio_gain,
            floor_db: pixel.audio_floor,
            attack_millis: pixel.audio_attack,
            release_millis: pixel.audio_release,
        };

        shared.audio.get(effect.pixel.offset, settings)
    } else {
        None
    };

    let audio_level = match followed {
        Some(audio) if !pattern.shows_audio() => audio.level(),
        _ => 1.0,
    };

    let follows_size = followed.is_some() && !pattern.shows_audio() && pixel.flags & pixel_flag::FOLLOWS_SIZE != 0;

    let settings = PatternSettings {
        pattern,
        size: if follows_size {
            pixel.size * audio_level
        } else {
            pixel.size
        },
        softness: pixel.softness,
        direction: Direction::from_u8(pixel.direction),
        shape: Shape::from_u8(pixel.shape),
        bounce: pixel.flags & pixel_flag::BOUNCE != 0,
        bitmap: show.blob(pixel.bitmap).and_then(Bitmap::parse),
        scroll: Scroll::from_u8(pixel.scroll),
        align: Align::from_u8(pixel.align),
        offset_y: pixel.offset_y,
        fit: Fit::from_u8(pixel.fit),
        text_colors: TextColors::from_u8(pixel.text_colors),
        image_colors: ImageColors::from_u8(pixel.image_colors),
    };

    // the color of the pixel through the palette, or the own one of an image
    let mut sampled = [0.0f32; 5];
    sample_with(&settings, cycle, &context, followed, &mut shared.scratch, &mut sampled);

    let [place, mut level, red, green, blue] = sampled;

    if !follows_size {
        level *= audio_level;
    }

    let color = if red >= 0.0 {
        [red, green, blue]
    } else if pixel.flags & pixel_flag::PRESET_PALETTE != 0 {
        let shades = shades_of(base_color);
        color_of(
            &Stops::from_slice(&shades),
            Between::from_u8(pixel.between),
            Repeat::from_u8(pixel.repeat),
            place,
        )
    } else {
        let stops = show
            .blob(pixel.stops)
            .and_then(Stops::from_bytes)
            .unwrap_or(Stops::EMPTY);
        color_of(
            &stops,
            Between::from_u8(pixel.between),
            Repeat::from_u8(pixel.repeat),
            place,
        )
    };

    let mut lit = [color[0] * level, color[1] * level, color[2] * level];
    let blend = Blend::from_u8(pixel.blend);

    // A pixel lighting in white only lights as bright as the brightest of the red, green and blue of the light,
    // dimmed by its dimmer
    if let Some(channel) = white_dimmer {
        let max = max_of(channel);
        let dimmer = base_value(vals, channel).map(|value| value / max).unwrap_or(0.0);
        let brightest = lit[0].max(lit[1]).max(lit[2]);

        vals.mix(channel, max * blend.mix(1.0, brightest, level) * dimmer, intensity);
        return true;
    }

    // the white part of the light on the white LED, if it is to light it
    let white = if pixel.flags & pixel_flag::WHITE_LED != 0 && layout.white >= 0 {
        Some(layout.white as usize)
    } else {
        None
    };
    let mut lit_white = 0.0;

    if white.is_some() {
        lit_white = lit[0].min(lit[1]).min(lit[2]);

        for value in lit.iter_mut() {
            *value -= lit_white;
        }
    }

    for (index, channel) in channels.iter().enumerate() {
        if *channel >= 0 {
            let channel = *channel as usize;
            let target = blend.mix(
                base_color.get(index).copied().unwrap_or(0.0),
                lit.get(index).copied().unwrap_or(0.0),
                level,
            );
            vals.mix(channel, max_of(channel) * target, intensity);
        }
    }

    if let Some(channel) = white {
        let max = max_of(channel);
        let base_white = base_value(vals, channel).map(|value| value / max).unwrap_or(0.0);
        vals.mix(channel, max * blend.mix(base_white, lit_white, level), intensity);
    }

    true
}
