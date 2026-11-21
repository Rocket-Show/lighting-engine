//! The engine: a compiled show, the inputs of the host with their times, and the values for a moment.
//!
//! The state is a value: a host may run several engines at once. The working memory is handed over by the caller, as
//! many 32-bit words as the header of the show asks for. One call at a time per engine; locks stay in the host.

pub mod caps;
pub mod channel;
pub mod curve;
mod frame;
pub mod live;
mod pixelfx;
pub mod steps;
pub mod values;

pub use caps::{MAX_CAPS, MAX_STEP_CAPS};
pub use steps::MAX_ORDER;

use crate::fix::{beats_to_ns, fix_from_f32, ns_to_beats, ratio, BeatClock, Fix, Ns, DEFAULT_BPM, FIX_ONE, NS_PER_US};
use crate::format::{
    clip_flag, composition_flag, preset_flag, scene_flag, Composition, Preset, Root, Scene, Show, ShowError,
};

use live::{LiveClock, LiveSlot, LIVE_WORDS, OPEN_END};
use steps::{state_at, steps_time, transition_curve, StepSel};

/// The values of a frame of audio: the RMS, the peak, the bass, the mids, the highs and 16 bands, in dB.
pub const AUDIO_VALUES: usize = engine_pixel::audio::FRAME_LENGTH;

const LAYER_WORDS: usize = 18;
const FOLLOWING_WORDS: usize = 3;
const HOLD_WORDS: usize = 4;
const REGION_WORDS: usize = 3;
const MIN_HOLDS: usize = 8;
const UNIVERSE_SIZE: usize = 512;

pub mod fixture_flag {
    pub const IDENTIFY: u32 = 1;
    pub const RESET: u32 = 2;
    pub const LAMP_ON: u32 = 4;
}

/// How a timeline is played.
pub mod timeline_flag {
    /// played as the show (every track), not as a preview (which leaves hidden tracks out)
    pub const PLAYER: u32 = 1;
    /// the composition played in between, which follows a tempo from outside
    pub const DEFAULT_COMPOSITION: u32 = 2;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// nothing plays: the fixtures stand at what their channels start out with
    Idle,
    /// the timeline of a composition
    Timeline,
    /// the preset or the scenes selected for a preview
    Preview,
    /// the scenes picked by hand
    Live,
}

/// The sizes of the parts of the working memory, in words.
struct Sizes {
    values: usize,
    part: usize,
    fixtures: usize,
    scenes: usize,
    live: usize,
    regions: usize,
    holds: usize,
    layers: usize,
    following: usize,
    override_mask: usize,
    override_values: usize,
    audio: usize,
}

impl Sizes {
    fn of(root: &Root) -> Sizes {
        let scenes = root.scenes.len();
        let universes = root.universes as usize;

        Sizes {
            values: root.total_channels as usize,
            part: root.max_part_channels as usize,
            fixtures: root.fixtures.len(),
            scenes,
            live: scenes * 2 * LIVE_WORDS,
            regions: root.max_regions as usize * REGION_WORDS,
            holds: (root.max_holds as usize).max(MIN_HOLDS) * HOLD_WORDS,
            layers: root.max_layers as usize * LAYER_WORDS,
            following: root.max_following as usize * FOLLOWING_WORDS,
            override_mask: universes * (UNIVERSE_SIZE / 32),
            override_values: universes * (UNIVERSE_SIZE / 4),
            audio: root.audio_history as usize * AUDIO_VALUES,
        }
    }

    fn total(&self) -> usize {
        2 * self.values
            + 2 * self.part
            + self.fixtures
            + 2 * self.scenes
            + self.live
            + self.regions
            + self.holds
            + self.layers
            + self.following
            + self.override_mask
            + self.override_values
            + self.audio
    }
}

/// The working memory a show needs, in 32-bit words.
pub fn memory_words(root: &Root) -> u32 {
    Sizes::of(root).total().min(u32::MAX as usize) as u32
}

/// The working memory, cut into its parts.
struct Memory<'a> {
    values: &'a mut [u32],
    flags: &'a mut [u32],
    snapshot_values: &'a mut [u32],
    snapshot_flags: &'a mut [u32],
    fixture_flags: &'a mut [u32],
    scene_selected: &'a mut [u32],
    scene_dimmers: &'a mut [u32],
    live: &'a mut [u32],
    region_beats: &'a mut [u32],
    holds: &'a mut [u32],
    layers: &'a mut [u32],
    following: &'a mut [u32],
    override_mask: &'a mut [u32],
    override_values: &'a mut [u32],
    audio: &'a mut [u32],
}

fn take<'a>(memory: &mut &'a mut [u32], words: usize) -> Option<&'a mut [u32]> {
    if memory.len() < words {
        return None;
    }

    let (head, tail) = core::mem::take(memory).split_at_mut(words);
    *memory = tail;
    Some(head)
}

impl<'a> Memory<'a> {
    fn cut(memory: &'a mut [u32], sizes: &Sizes) -> Option<Memory<'a>> {
        let mut rest = memory;

        Some(Memory {
            values: take(&mut rest, sizes.values)?,
            flags: take(&mut rest, sizes.values)?,
            snapshot_values: take(&mut rest, sizes.part)?,
            snapshot_flags: take(&mut rest, sizes.part)?,
            fixture_flags: take(&mut rest, sizes.fixtures)?,
            scene_selected: take(&mut rest, sizes.scenes)?,
            scene_dimmers: take(&mut rest, sizes.scenes)?,
            live: take(&mut rest, sizes.live)?,
            region_beats: take(&mut rest, sizes.regions)?,
            holds: take(&mut rest, sizes.holds)?,
            layers: take(&mut rest, sizes.layers)?,
            following: take(&mut rest, sizes.following)?,
            override_mask: take(&mut rest, sizes.override_mask)?,
            override_values: take(&mut rest, sizes.override_values)?,
            audio: take(&mut rest, sizes.audio)?,
        })
    }
}

fn get_i64(words: &[u32], at: usize) -> i64 {
    let low = words.get(at).copied().unwrap_or(0) as u64;
    let high = words.get(at + 1).copied().unwrap_or(0) as u64;
    (low | high << 32) as i64
}

fn put_i64(words: &mut [u32], at: usize, value: i64) {
    if let Some(slot) = words.get_mut(at) {
        *slot = value as u64 as u32;
    }

    if let Some(slot) = words.get_mut(at + 1) {
        *slot = (value as u64 >> 32) as u32;
    }
}

fn put(words: &mut [u32], at: usize, value: u32) {
    if let Some(slot) = words.get_mut(at) {
        *slot = value;
    }
}

fn get(words: &[u32], at: usize) -> u32 {
    words.get(at).copied().unwrap_or(0)
}

const LAYER_REGION: u32 = 1;
const LAYER_LIVE: u32 = 2;
const LAYER_PHASED: u32 = 4;
const LAYER_ON_BEAT: u32 = 8;

/// A preset playing at the moment of a frame, with what is the same for all of its fixtures.
#[derive(Clone, Copy, Debug)]
struct Layer {
    preset: u32,
    flags: u32,
    /// its scene and the clip it is on (-1: none)
    scene: i32,
    region: i32,
    region_start: Ns,
    region_end: Ns,
    /// how much of the preset reaches the stage
    intensity: f32,
    /// what it plays, unless its steps chase over its fixtures
    sel: StepSel,
    /// the time its steps have run for, before the chase of a fixture is taken away
    steps_base: Ns,
    /// the time its effects have run for
    effect_time: Ns,
    /// the beats it has played for, where what follows the beat runs on the beat clock
    beats_played: Fix,
}

impl Layer {
    fn load(memory: &[u32], index: usize) -> Layer {
        let at = index * LAYER_WORDS;

        Layer {
            preset: get(memory, at),
            flags: get(memory, at + 1),
            scene: get(memory, at + 2) as i32,
            region: get(memory, at + 3) as i32,
            region_start: get_i64(memory, at + 4),
            region_end: get_i64(memory, at + 6),
            intensity: f32::from_bits(get(memory, at + 8)),
            sel: StepSel {
                from: get(memory, at + 9) as i32,
                to: get(memory, at + 10),
                position: f32::from_bits(get(memory, at + 11)),
            },
            steps_base: get_i64(memory, at + 12),
            effect_time: get_i64(memory, at + 14),
            beats_played: get_i64(memory, at + 16),
        }
    }

    fn store(&self, memory: &mut [u32], index: usize) {
        let at = index * LAYER_WORDS;
        put(memory, at, self.preset);
        put(memory, at + 1, self.flags);
        put(memory, at + 2, self.scene as u32);
        put(memory, at + 3, self.region as u32);
        put_i64(memory, at + 4, self.region_start);
        put_i64(memory, at + 6, self.region_end);
        put(memory, at + 8, self.intensity.to_bits());
        put(memory, at + 9, self.sel.from as u32);
        put(memory, at + 10, self.sel.to);
        put(memory, at + 11, self.sel.position.to_bits());
        put_i64(memory, at + 12, self.steps_base);
        put_i64(memory, at + 14, self.effect_time);
        put_i64(memory, at + 16, self.beats_played);
    }

    fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// What is the same for everything in a frame.
#[derive(Clone, Copy)]
struct Frame {
    /// the position played
    time: Ns,
    layers: usize,
    /// the tempo of the composition playing (or selected in the project), none without one
    programmed_bpm: Option<Fix>,
    /// the tempo of the beat clock
    clock_bpm: Fix,
}

/// What the host has set, besides what is in the working memory.
#[derive(Clone, Copy)]
struct Runtime {
    mode: Mode,
    composition: i32,
    timeline_flags: u32,
    solo: bool,
    preset: i32,
    step: i32,
    step_preview_running: bool,
    step_preview_start: Ns,
    position: Ns,
    position_at: Ns,
    running: bool,
    beat: BeatClock,
    /// the tempo from outside: tapped, set or of a MIDI clock (0: none)
    outside_bpm: Fix,
    holding: bool,
    hold_start: Ns,
    holds: usize,
    master: f32,
    blackout: bool,
    live: LiveClock,
    /// the newest frame of audio handed over, and how many there are
    audio_newest: i64,
    audio_count: usize,
}

/// The engine of a show.
pub struct Engine<'a> {
    show: Show<'a>,
    rt: Runtime,
    m: Memory<'a>,
}

fn to_ns(us: u64) -> Ns {
    (us.min(i64::MAX as u64 / 1000) as i64) * NS_PER_US
}

fn us_to_ns(us: i64) -> Ns {
    us.saturating_mul(NS_PER_US)
}

// ---- when a preset of a clip plays, and how much of it reaches the stage ----

fn scene_start(scene: Option<&Scene>, start: Ns) -> Ns {
    match scene {
        Some(scene) if scene.flags & scene_flag::FADE_IN_PRE != 0 => start.saturating_sub(scene.fade_in),
        _ => start,
    }
}

fn scene_end(scene: Option<&Scene>, end: Ns) -> Ns {
    match scene {
        Some(scene) if scene.flags & scene_flag::FADE_OUT_POST != 0 => end.saturating_add(scene.fade_out),
        _ => end,
    }
}

fn preset_play_start(preset: &Preset, scene: Option<&Scene>, start: Ns) -> Ns {
    let own = if preset.flags & preset_flag::START != 0 {
        start.saturating_add(preset.start)
    } else {
        scene_start(scene, start)
    };
    own.saturating_sub(if preset.flags & preset_flag::FADE_IN_PRE != 0 {
        preset.fade_in
    } else {
        0
    })
}

fn preset_play_end(preset: &Preset, scene: Option<&Scene>, start: Ns, end: Ns) -> Ns {
    let own = if preset.flags & preset_flag::END != 0 {
        start.saturating_add(preset.end)
    } else {
        scene_end(scene, end)
    };
    own.saturating_add(if preset.flags & preset_flag::FADE_OUT_POST != 0 {
        preset.fade_out
    } else {
        0
    })
}

/// How much of a preset reaches the stage at a moment: the dimmer of its scene, and the fades of the scene and of the
/// preset on its clip (start and end).
pub fn preset_intensity(preset: &Preset, scene: Option<&Scene>, region: Option<(Ns, Ns)>, time: Ns) -> f32 {
    let mut scene_intensity = scene.map(|scene| scene.dimmer).unwrap_or(1.0);
    let mut own_intensity = 1.0;

    if let Some((start, end)) = region {
        if let Some(scene) = scene {
            // fade out is stronger than fade in (if they overlap)
            let scene_start = scene_start(Some(scene), start);
            let scene_end = scene_end(Some(scene), end);

            if time > scene_end.saturating_sub(scene.fade_out) && time < scene_end {
                scene_intensity *= transition_curve(scene.fade_out_curve, ratio(scene_end - time, scene.fade_out));
            } else if time < scene_start.saturating_add(scene.fade_in) && time > scene_start {
                scene_intensity *= transition_curve(scene.fade_in_curve, ratio(time - scene_start, scene.fade_in));
            }
        }

        let play_start = preset_play_start(preset, scene, start);
        let play_end = preset_play_end(preset, scene, start, end);

        if time > play_end.saturating_sub(preset.fade_out) && time < play_end {
            own_intensity = transition_curve(preset.fade_out_curve, ratio(play_end - time, preset.fade_out));
        } else if time < play_start.saturating_add(preset.fade_in) && time > play_start {
            own_intensity = transition_curve(preset.fade_in_curve, ratio(time - play_start, preset.fade_in));
        }
    }

    scene_intensity * own_intensity
}

impl<'a> Engine<'a> {
    /// Build the working state of a checked show in the memory handed over.
    pub fn new(show: Show<'a>, memory: &'a mut [u32], now_us: u64) -> Result<Engine<'a>, ShowError> {
        let sizes = Sizes::of(&show.root);
        let needed = memory_words(&show.root);

        for word in memory.iter_mut() {
            *word = 0;
        }

        let m = Memory::cut(memory, &sizes).ok_or(ShowError::Memory(needed))?;

        for dimmer in m.scene_dimmers.iter_mut() {
            *dimmer = 1.0f32.to_bits();
        }

        let now = to_ns(now_us);

        Ok(Engine {
            show,
            rt: Runtime {
                mode: Mode::Idle,
                composition: -1,
                timeline_flags: 0,
                solo: false,
                preset: -1,
                step: -1,
                step_preview_running: false,
                step_preview_start: 0,
                position: 0,
                position_at: now,
                running: false,
                beat: BeatClock::new(now),
                outside_bpm: 0,
                holding: false,
                hold_start: 0,
                holds: 0,
                master: 1.0,
                blackout: false,
                live: LiveClock::new(),
                audio_newest: 0,
                audio_count: 0,
            },
            m,
        })
    }

    pub fn show(&self) -> &Show<'a> {
        &self.show
    }

    // ---- what plays ----

    /// Play the timeline of a composition (see `timeline_flag`). Its position is set with `set_position`.
    pub fn play_timeline(&mut self, composition: u32, flags: u32) {
        if (composition as usize) < self.show.root.compositions.len() {
            self.rt.mode = Mode::Timeline;
            self.rt.composition = composition as i32;
            self.rt.timeline_flags = flags;

            for word in self.m.region_beats.iter_mut() {
                *word = 0;
            }
        }
    }

    /// Preview what is selected: the selected preset alone (solo, or while no scene is selected), or
    /// else the selected scenes.
    pub fn play_preview(&mut self, solo: bool) {
        self.rt.mode = Mode::Preview;
        self.rt.solo = solo;
    }

    /// Play the scenes picked by hand, from now on (see `live_scene`).
    pub fn play_live(&mut self, now_us: u64) {
        self.live_reset();
        self.rt.mode = Mode::Live;
        self.sync_beat_clock(to_ns(now_us));
    }

    /// Play nothing: the fixtures stand at what their channels start out with.
    pub fn stop(&mut self) {
        self.rt.mode = Mode::Idle;
    }

    /// The preset selected for the preview and the step being edited in it (-1: none).
    pub fn select_preset(&mut self, preset: i32, step: i32) {
        self.rt.preset = preset;
        self.rt.step = step;
    }

    /// Whether the steps of the previewed presets run (from a position on) or hold at the step being edited.
    pub fn set_step_preview(&mut self, running: bool, start_us: i64) {
        self.rt.step_preview_running = running;
        self.rt.step_preview_start = us_to_ns(start_us);
    }

    pub fn select_scene(&mut self, scene: u32, selected: bool) {
        put(self.m.scene_selected, scene as usize, selected as u32);
    }

    pub fn clear_scenes(&mut self) {
        for word in self.m.scene_selected.iter_mut() {
            *word = 0;
        }
    }

    /// Where the timeline (or the clock of the preview) is at a moment, and whether it runs on from there.
    pub fn set_position(&mut self, now_us: u64, position_us: i64, running: bool) {
        self.rt.position = us_to_ns(position_us);
        self.rt.position_at = to_ns(now_us);
        self.rt.running = running;
    }

    // ---- the holds of the timeline ----

    fn hold_capacity(&self) -> usize {
        self.m.holds.len() / HOLD_WORDS
    }

    fn push_hold(&mut self, position: Ns, held: Ns) {
        if self.hold_capacity() == 0 {
            return;
        }

        if self.rt.holds >= self.hold_capacity() {
            // the oldest hold makes room
            self.m.holds.copy_within(HOLD_WORDS.., 0);
            self.rt.holds -= 1;
        }

        put_i64(self.m.holds, self.rt.holds * HOLD_WORDS, position);
        put_i64(self.m.holds, self.rt.holds * HOLD_WORDS + 2, held);
        self.rt.holds += 1;
    }

    /// The timeline holds at a hold point until `go`: the position stands still, the clips playing run on.
    pub fn hold(&mut self, now_us: u64, position_us: i64) {
        self.push_hold(us_to_ns(position_us), 0);
        self.rt.hold_start = to_ns(now_us);
        self.rt.holding = true;
    }

    /// The timeline goes on from the hold point.
    pub fn go(&mut self, now_us: u64) {
        if self.rt.holding {
            self.update_hold(to_ns(now_us));
            self.rt.holding = false;
        }
    }

    /// The position jumped: the holds from there on weren't gone through anymore.
    pub fn forget_holds(&mut self, from_position_us: i64) {
        let from = us_to_ns(from_position_us);
        let mut kept = 0;

        for index in 0..self.rt.holds {
            let position = get_i64(self.m.holds, index * HOLD_WORDS);
            let held = get_i64(self.m.holds, index * HOLD_WORDS + 2);

            if position < from {
                put_i64(self.m.holds, kept * HOLD_WORDS, position);
                put_i64(self.m.holds, kept * HOLD_WORDS + 2, held);
                kept += 1;
            }
        }

        self.rt.holds = kept;
        self.rt.holding = false;
    }

    /// A hold the timeline went through: its position and the time held there.
    pub fn add_hold(&mut self, position_us: i64, held_us: i64) {
        self.push_hold(us_to_ns(position_us), us_to_ns(held_us));
    }

    /// Whether the timeline is holding at the last hold added (for as long already as that hold says).
    pub fn set_holding(&mut self, now_us: u64, holding: bool) {
        self.rt.holding = holding && self.rt.holds > 0;

        if self.rt.holding {
            self.rt.hold_start = to_ns(now_us) - get_i64(self.m.holds, (self.rt.holds - 1) * HOLD_WORDS + 2);
        }
    }

    pub fn is_holding(&self) -> bool {
        self.rt.holding
    }

    fn update_hold(&mut self, now: Ns) {
        if self.rt.holding && self.rt.holds > 0 {
            put_i64(
                self.m.holds,
                (self.rt.holds - 1) * HOLD_WORDS + 2,
                now - self.rt.hold_start,
            );
        }
    }

    /// the time the timeline has held for at the hold points between two positions
    fn held(&self, from: Ns, to: Ns) -> Ns {
        let mut held: Ns = 0;

        for index in 0..self.rt.holds {
            let position = get_i64(self.m.holds, index * HOLD_WORDS);

            if position >= from && position <= to {
                held = held.saturating_add(get_i64(self.m.holds, index * HOLD_WORDS + 2));
            }
        }

        held
    }

    // ---- the tempo ----

    fn tempo_composition(&self) -> Option<Composition> {
        let index = if self.rt.mode == Mode::Timeline {
            self.rt.composition
        } else {
            self.show.root.selected_composition
        };

        if index < 0 {
            return None;
        }

        self.show.get(self.show.root.compositions, index as usize)
    }

    /// the tempo of the composition playing, or else of the one selected in the project
    fn programmed_bpm(&self) -> Option<Fix> {
        self.tempo_composition().map(|composition| composition.bpm)
    }

    fn is_player(&self) -> bool {
        self.rt.mode == Mode::Timeline && self.rt.timeline_flags & timeline_flag::PLAYER != 0
    }

    /// the beat clock runs at the tempo playing (on at the last one, while none plays)
    fn sync_beat_clock(&mut self, now: Ns) {
        let tempo = if self.rt.outside_bpm > 0 {
            Some(self.rt.outside_bpm)
        } else if self.is_player() || self.rt.mode == Mode::Live {
            self.programmed_bpm()
        } else {
            None
        };

        if let Some(bpm) = tempo {
            self.rt.beat.set_bpm(bpm, now);
        }
    }

    /// The tempo from outside: tapped, set or of a MIDI clock, in beats per minute (0 or less: none, the programmed one).
    pub fn set_tempo(&mut self, now_us: u64, bpm: f32) {
        self.rt.outside_bpm = if bpm > 0.0 { fix_from_f32(bpm) } else { 0 };
        self.sync_beat_clock(to_ns(now_us));
    }

    /// A beat falls on a moment (a beat tapped): the beats move onto it over the next beat.
    pub fn tap(&mut self, now_us: u64) {
        self.rt.beat.align_beat(to_ns(now_us));
    }

    /// The first beat of a bar falls on a moment.
    pub fn beat_one(&mut self, now_us: u64, jump: bool) {
        let beats_per_bar = self
            .tempo_composition()
            .map(|composition| composition.beats_per_bar as i32)
            .unwrap_or(4);
        self.rt.beat.beat_one(beats_per_bar, jump, to_ns(now_us));
    }

    /// Where the beats are at a moment, at once (a song position of a MIDI clock).
    pub fn set_beats(&mut self, now_us: u64, beats: f32) {
        self.rt.beat.set_beats(fix_from_f32(beats), to_ns(now_us));
    }

    /// The beats of the tempo playing at a moment, as a 32.32 fixed-point number.
    pub fn beats(&self, now_us: u64) -> i64 {
        self.rt.beat.beats(to_ns(now_us))
    }

    // ---- the scenes picked by hand ----

    fn live_set(&mut self, now_us: u64, scene: u32, flash: bool, on: bool) {
        let now = to_ns(now_us);
        let record = match self.show.get(self.show.root.scenes, scene as usize) {
            Some(record) => record,
            None => return,
        };

        let clock = self.rt.live.advance(&self.rt.beat, now);
        let index = scene as usize * 2 + flash as usize;
        let mut slot = LiveSlot::load(self.m.live, index);
        live::set_scene(&mut slot, &record, flash, on, clock, self.rt.live.beats, now);
        slot.store(self.m.live, index);
    }

    /// A scene is picked by hand or let go: it fades in, or fades out.
    pub fn live_scene(&mut self, now_us: u64, scene: u32, on: bool) {
        self.live_set(now_us, scene, false, on);
    }

    /// A scene is flashed or let go: on at once, on top of the others.
    pub fn live_flash(&mut self, now_us: u64, scene: u32, on: bool) {
        self.live_set(now_us, scene, true, on);
    }

    /// The dimmer of a scene picked by hand.
    pub fn live_dimmer(&mut self, scene: u32, dimmer: f32) {
        put(self.m.scene_dimmers, scene as usize, dimmer.to_bits());
    }

    /// The speed master of the scenes picked by hand (1: as written).
    pub fn live_speed(&mut self, now_us: u64, speed: f32) {
        self.rt
            .live
            .set_speed(&self.rt.beat, fix_from_f32(speed), to_ns(now_us));
    }

    /// Turn all scenes picked by hand off at once and start their clock anew.
    pub fn live_reset(&mut self) {
        for word in self.m.live.iter_mut() {
            *word = 0;
        }

        self.rt.live.reset();
    }

    /// Whether no scene picked by hand plays anymore: none is on, flashed or still fades out.
    pub fn live_idle(&self, now_us: u64) -> bool {
        let now = to_ns(now_us);

        for (index, scene) in self.show.iter(self.show.root.scenes).enumerate() {
            for flash in [false, true] {
                if !LiveSlot::load(self.m.live, index * 2 + flash as usize).is_over(&scene, flash, now) {
                    return false;
                }
            }
        }

        true
    }

    // ---- the rest of the inputs ----

    /// The master dimmer of the host, on top of the one of the project.
    pub fn set_master_dimmer(&mut self, dimmer: f32) {
        self.rt.master = dimmer;
    }

    pub fn set_blackout(&mut self, blackout: bool) {
        self.rt.blackout = blackout;
    }

    fn set_fixture_flag(&mut self, fixture: u32, flag: u32, on: bool) {
        if let Some(flags) = self.m.fixture_flags.get_mut(fixture as usize) {
            *flags = if on { *flags | flag } else { *flags & !flag };
        }
    }

    /// A fixture being identified blinks at full white, whatever the presets light it with.
    pub fn identify(&mut self, fixture: u32, on: bool) {
        self.set_fixture_flag(fixture, fixture_flag::IDENTIFY, on);
    }

    /// A fixture is asked to reset itself, for as long as this is on.
    pub fn reset_fixture(&mut self, fixture: u32, on: bool) {
        self.set_fixture_flag(fixture, fixture_flag::RESET, on);
    }

    /// A fixture is asked to switch its lamp on, for as long as this is on.
    pub fn lamp_on(&mut self, fixture: u32, on: bool) {
        self.set_fixture_flag(fixture, fixture_flag::LAMP_ON, on);
    }

    /// Hold a DMX channel (from 0) of a universe at a value, whatever the show sends there (a negative value: let go).
    pub fn set_override(&mut self, universe: u32, channel: u32, value: i32) {
        if universe >= self.show.root.universes || channel as usize >= UNIVERSE_SIZE {
            return;
        }

        let index = universe as usize * UNIVERSE_SIZE + channel as usize;

        if let Some(mask) = self.m.override_mask.get_mut(index / 32) {
            if value < 0 {
                *mask &= !(1 << (index % 32));
            } else {
                *mask |= 1 << (index % 32);
            }
        }

        if let Some(word) = self.m.override_values.get_mut(index / 4) {
            let shift = (index % 4) * 8;
            *word = (*word & !(0xFF << shift)) | ((value.clamp(0, 255) as u32) << shift);
        }
    }

    pub fn clear_overrides(&mut self) {
        for word in self.m.override_mask.iter_mut() {
            *word = 0;
        }
    }

    /// A frame of the analysis of the audio playing (see `AUDIO_VALUES`), by its number: the frame `index` covers the
    /// 20 ms from `index * 20 ms` of the position played.
    pub fn audio_frame(&mut self, index: i64, values: &[f32]) {
        let capacity = self.m.audio.len() / AUDIO_VALUES;

        if capacity == 0 || index < 0 {
            return;
        }

        if self.rt.audio_count > 0
            && (index > self.rt.audio_newest + capacity as i64 || index + capacity as i64 <= self.rt.audio_newest)
        {
            // far from the frames kept: start over
            self.rt.audio_count = 0;
        }

        // frames skipped are silence
        if self.rt.audio_count > 0 {
            let mut skipped = self.rt.audio_newest + 1;

            while skipped < index {
                self.store_audio(skipped, None);
                skipped += 1;
            }
        }

        self.store_audio(index, Some(values));

        if self.rt.audio_count == 0 || index > self.rt.audio_newest {
            self.rt.audio_newest = index;
        }

        self.rt.audio_count = (self.rt.audio_count + 1).min(capacity);
    }

    fn store_audio(&mut self, index: i64, values: Option<&[f32]>) {
        let capacity = self.m.audio.len() / AUDIO_VALUES;
        let slot = (index.rem_euclid(capacity.max(1) as i64)) as usize * AUDIO_VALUES;

        for value in 0..AUDIO_VALUES {
            let bits = match values {
                Some(values) => values.get(value).copied().unwrap_or(pixelfx::SILENCE).to_bits(),
                None => pixelfx::SILENCE.to_bits(),
            };

            put(self.m.audio, slot + value, bits);
        }
    }

    /// Forget the frames of audio handed over (the audio stopped, or another one plays).
    pub fn clear_audio(&mut self) {
        self.rt.audio_count = 0;
    }

    // ---- a frame ----

    fn position(&mut self, now: Ns) -> Ns {
        match self.rt.mode {
            Mode::Live => self.rt.live.advance(&self.rt.beat, now),
            _ if self.rt.running => self.rt.position.saturating_add(now - self.rt.position_at),
            _ => self.rt.position,
        }
    }

    fn push_layer(&mut self, count: &mut usize, layer: &Layer) {
        if (*count + 1) * LAYER_WORDS <= self.m.layers.len() {
            layer.store(self.m.layers, *count);
            *count += 1;
        }
    }

    /// The tempo the steps following the beat are played at: the one of the composition, or else the default one.
    fn steps_bpm(&self) -> Fix {
        self.programmed_bpm().unwrap_or(DEFAULT_BPM)
    }

    /// Work out what is the same for all fixtures of a preset playing: how much of it reaches the stage, how long its
    /// steps and its effects have run for.
    fn finish_layer(&self, layer: &mut Layer, time: Ns, scene_level: f32) {
        let preset = match self.show.get(self.show.root.presets, layer.preset as usize) {
            Some(preset) => preset,
            None => return,
        };

        let scene = if layer.scene >= 0 {
            self.show.get(self.show.root.scenes, layer.scene as usize)
        } else {
            None
        };
        let live = layer.has(LAYER_LIVE);
        let follows_beat = preset.flags & preset_flag::BEATS != 0;
        let phased = preset.flags & preset_flag::PHASED != 0;

        if !layer.has(LAYER_REGION) {
            layer.intensity = preset_intensity(&preset, scene.as_ref(), None, time);
            layer.effect_time = time;

            if self.rt.step_preview_running {
                // the sequence was started by hand, so it runs from the point it was started at
                let preset_time = time - self.rt.step_preview_start;
                layer.steps_base = if follows_beat {
                    steps_time(&preset, preset_time, self.steps_bpm())
                } else {
                    preset_time
                };

                if phased {
                    layer.flags |= LAYER_PHASED;
                } else {
                    layer.sel = state_at(&self.show, &preset, layer.steps_base);
                }
            } else {
                // the step being edited, or else the first one
                let mut step = 0;

                if self.rt.step >= 0
                    && layer.preset as i32 == self.rt.preset
                    && (self.rt.step as usize) < preset.steps.len()
                {
                    step = self.rt.step as u32;
                }

                layer.sel = StepSel::single(step);
            }

            return;
        }

        let region = (layer.region_start, layer.region_end);

        layer.intensity = if live {
            // the fades of the scene are worked out in real time, the ones of the preset on the clock of the scenes
            scene_level * preset_intensity(&preset, None, Some(region), time)
        } else {
            preset_intensity(&preset, scene.as_ref(), Some(region), time)
        };

        let start = if preset.flags & preset_flag::START != 0 {
            layer.region_start.saturating_add(preset.start)
        } else {
            layer.region_start
        };
        let held_steps = if live { 0 } else { self.held(start, time) };
        let held_effects = if live { 0 } else { self.held(layer.region_start, time) };

        layer.effect_time = time - layer.region_start + held_effects;

        layer.steps_base = if follows_beat {
            let on_beat = layer.has(LAYER_ON_BEAT);
            let bpm = if on_beat { self.rt.beat.bpm() } else { self.steps_bpm() };

            // on the beats of the tempo from outside, from where the preset starts in its clip
            let preset_time = if on_beat {
                beats_to_ns(layer.beats_played, bpm) - (start - layer.region_start)
            } else {
                time - start + held_steps
            };
            steps_time(&preset, preset_time, bpm)
        } else {
            time - start + held_steps
        };

        if phased {
            layer.flags |= LAYER_PHASED;
        } else {
            layer.sel = state_at(&self.show, &preset, layer.steps_base);
        }
    }

    fn new_layer(preset: u32, scene: i32) -> Layer {
        Layer {
            preset,
            flags: 0,
            scene,
            region: -1,
            region_start: 0,
            region_end: 0,
            intensity: 1.0,
            sel: StepSel::single(0),
            steps_base: 0,
            effect_time: 0,
            beats_played: 0,
        }
    }

    // (a function of its own, see the module frame)
    #[cfg_attr(target_family = "wasm", inline(never))]
    fn timeline_layers(&mut self, time: Ns, now: Ns) -> usize {
        let mut count = 0;

        let composition = match self
            .show
            .get(self.show.root.compositions, self.rt.composition.max(0) as usize)
        {
            Some(composition) => composition,
            None => return 0,
        };

        let player = self.is_player();

        for clip in self.show.iter(composition.clips) {
            // the preview leaves out the tracks hidden, the show plays every track
            if !player && clip.flags & clip_flag::PREVIEW == 0 {
                continue;
            }

            // the fades running outside the boundaries included
            if clip.play_start <= time && clip.play_end >= time {
                if let Some(region) = self.show.get(composition.regions, clip.region as usize) {
                    let mut layer = Self::new_layer(clip.preset as u32, clip.scene as i32);
                    layer.flags = LAYER_REGION;
                    layer.region = clip.region as i32;
                    layer.region_start = region.start;
                    layer.region_end = region.end;
                    self.push_layer(&mut count, &layer);
                }
            }
        }

        // The clips of a composition following the tempo from outside start on the beat of it they began on (a whole
        // one), the others forget theirs
        let frame_beats = self.rt.beat.beats(now);
        let following = self.rt.outside_bpm > 0
            && player
            && (self.rt.timeline_flags & timeline_flag::DEFAULT_COMPOSITION != 0
                || composition.flags & composition_flag::SYNC_NONE != 0);

        if !following {
            for word in self.m.region_beats.iter_mut() {
                *word = 0;
            }
        } else {
            let bpm = self.steps_bpm();

            for index in 0..count {
                let layer = Layer::load(self.m.layers, index);
                let at = layer.region.max(0) as usize * REGION_WORDS;

                if get(self.m.region_beats, at) == 0 {
                    // a clip joined after it began started as many beats ago
                    let since = ns_to_beats((time - layer.region_start).max(0), bpm);
                    put_i64(
                        self.m.region_beats,
                        at + 1,
                        ((frame_beats.wrapping_sub(since).wrapping_add(4)) >> 32) << 32,
                    );
                }

                put(self.m.region_beats, at, 2);
            }

            // the clips not playing anymore forget their beat
            for region in 0..self.m.region_beats.len() / REGION_WORDS {
                let state = get(self.m.region_beats, region * REGION_WORDS);
                put(
                    self.m.region_beats,
                    region * REGION_WORDS,
                    if state == 2 { 1 } else { 0 },
                );
            }
        }

        for index in 0..count {
            let mut layer = Layer::load(self.m.layers, index);
            let at = layer.region.max(0) as usize * REGION_WORDS;

            if get(self.m.region_beats, at) != 0 {
                layer.flags |= LAYER_ON_BEAT;
                layer.beats_played = frame_beats.wrapping_sub(get_i64(self.m.region_beats, at + 1));
            }

            self.finish_layer(&mut layer, time, 1.0);
            layer.store(self.m.layers, index);
        }

        count
    }

    // (a function of its own, see the module frame)
    #[cfg_attr(target_family = "wasm", inline(never))]
    fn preview_layers(&mut self, time: Ns) -> usize {
        let mut count = 0;
        let any_scene = self.m.scene_selected.iter().any(|selected| *selected != 0);

        if self.rt.solo || !any_scene {
            // only the selected preset (if it's still there)
            if self.rt.preset >= 0 && (self.rt.preset as usize) < self.show.root.presets.len() {
                let mut layer = Self::new_layer(self.rt.preset as u32, -1);
                self.finish_layer(&mut layer, time, 1.0);
                self.push_layer(&mut count, &layer);
            }

            return count;
        }

        // the scenes at the bottom first, the first preset of a scene is its topmost layer
        for scene_index in (0..self.show.root.scenes.len()).rev() {
            if get(self.m.scene_selected, scene_index) == 0 {
                continue;
            }

            let scene = match self.show.get(self.show.root.scenes, scene_index) {
                Some(scene) => scene,
                None => continue,
            };

            for preset_index in (0..scene.presets.len()).rev() {
                if let Some(preset) = self.show.get(scene.presets, preset_index) {
                    let mut layer = Self::new_layer(preset as u32, scene_index as i32);
                    self.finish_layer(&mut layer, time, 1.0);
                    self.push_layer(&mut count, &layer);
                }
            }
        }

        count
    }

    // (a function of its own, see the module frame)
    #[cfg_attr(target_family = "wasm", inline(never))]
    fn live_layers(&mut self, time: Ns, now: Ns) -> usize {
        let mut count = 0;

        for flash in [false, true] {
            for scene_index in (0..self.show.root.scenes.len()).rev() {
                let scene = match self.show.get(self.show.root.scenes, scene_index) {
                    Some(scene) => scene,
                    None => continue,
                };

                let slot_index = scene_index * 2 + flash as usize;
                let slot = LiveSlot::load(self.m.live, slot_index);

                if !slot.is_playing() {
                    continue;
                }

                if slot.is_over(&scene, flash, now) {
                    // the scenes which have faded out are forgotten
                    LiveSlot::default().store(self.m.live, slot_index);
                    continue;
                }

                let level = slot.level(&scene, flash, now)
                    * scene.dimmer
                    * f32::from_bits(get(self.m.scene_dimmers, scene_index));

                for preset_index in (0..scene.presets.len()).rev() {
                    let preset_number = match self.show.get(scene.presets, preset_index) {
                        Some(preset) => preset as u32,
                        None => continue,
                    };

                    let preset = match self.show.get(self.show.root.presets, preset_number as usize) {
                        Some(preset) => preset,
                        None => continue,
                    };

                    // a preset with times of its own only plays within them, from the moment the scene was picked
                    if preset_play_start(&preset, None, slot.region_start) > time
                        || preset_play_end(&preset, None, slot.region_start, OPEN_END) < time
                    {
                        continue;
                    }

                    let mut layer = Self::new_layer(preset_number, scene_index as i32);
                    layer.flags = LAYER_REGION | LAYER_LIVE | LAYER_ON_BEAT;
                    layer.region_start = slot.region_start;
                    layer.region_end = OPEN_END;
                    layer.beats_played = self.rt.live.beats.wrapping_sub(slot.start_beats);
                    self.finish_layer(&mut layer, time, level);
                    self.push_layer(&mut count, &layer);
                }
            }
        }

        count
    }

    /// Work out the values of all fixtures for a moment and write the universes: 512 bytes for each universe of the
    /// show, one after the other. A shorter buffer gets the universes fitting into it, and the fixtures on the others
    /// are not worked out: a host which sends fewer universes than a show has doesn't pay for the rest ([`value`] of
    /// their parts stays what it was).
    ///
    /// [`value`]: Engine::value
    pub fn render(&mut self, now_us: u64, universes: &mut [u8]) {
        let now = to_ns(now_us);
        self.sync_beat_clock(now);
        self.update_hold(now);

        let time = self.position(now);

        let layers = match self.rt.mode {
            Mode::Idle => 0,
            Mode::Timeline => self.timeline_layers(time, now),
            Mode::Preview => self.preview_layers(time),
            Mode::Live => self.live_layers(time, now),
        };

        let frame = Frame {
            time,
            layers,
            programmed_bpm: self.programmed_bpm(),
            clock_bpm: self.rt.beat.bpm(),
        };

        frame::render_parts(
            &self.show,
            &self.rt,
            &mut self.m,
            &frame,
            universes.len() / UNIVERSE_SIZE,
        );
        frame::write_universes(&self.show, &self.rt, &self.m, universes);
    }

    /// The value the last frame came to for a channel of a part of a fixture, none for a channel nothing put a value on.
    pub fn value(&self, part: u32, channel: u32) -> Option<f32> {
        let record = self.show.get(self.show.root.parts, part as usize)?;
        let layout = self.show.get(self.show.root.layouts, record.layout as usize)?;

        if channel >= layout.channels.count {
            return None;
        }

        let index = record.value_base as usize + channel as usize;

        if get(self.m.flags, index) & values::SET == 0 {
            return None;
        }

        self.m.values.get(index).map(|bits| f32::from_bits(*bits))
    }

    /// The values of all parts of the last frame, one after the other, as the bits of single-precision floats, and
    /// whether each has a value (bit 0 of the flags).
    pub fn values(&self) -> (&[u32], &[u32]) {
        (self.m.values, self.m.flags)
    }
}

const _: Fix = FIX_ONE;
