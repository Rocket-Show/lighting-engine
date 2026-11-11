//! The scenes picked by hand while they play: their fades in real time, and the clock their presets run on.
//!
//! A scene picked fades in over its fade in time and fades out over its fade out time when it is let go. Let go while
//! it still fades in, it fades out from where it got to; picked again while it fades out, it fades in from where it is.
//! A scene flashed is on at once while it is held, on top of the others. What is timed in milliseconds runs on a clock
//! of its own, as fast as the speed master makes it run; what follows the beat runs on the beats of the tempo playing.

use crate::fix::{fix_floor, ratio, scale, BeatClock, Fix, Ns, FIX_ONE};
use crate::format::Scene;

use super::steps::transition_curve;

/// the words a scene picked by hand takes in the working memory
pub const LIVE_WORDS: usize = 9;

/// where the clips of the scenes end: they play until they are let go
pub const OPEN_END: Ns = i64::MAX / 4;

const NONE: u32 = 0;
const ON: u32 = 1;
const RELEASED: u32 = 2;

/// The clock of the scenes picked by hand.
#[derive(Clone, Copy, Debug)]
pub struct LiveClock {
    pub clock: Ns,
    pub clock_real: Option<Ns>,
    pub beats: Fix,
    pub tempo_beats: Fix,
    pub speed: Fix,
}

impl LiveClock {
    pub const fn new() -> Self {
        LiveClock {
            clock: 0,
            clock_real: None,
            beats: 0,
            tempo_beats: 0,
            speed: FIX_ONE,
        }
    }

    /// where the clock is at a moment
    pub fn advance(&mut self, beat: &BeatClock, now: Ns) -> Ns {
        let tempo_beats = beat.beats(now);

        match self.clock_real {
            // started on the beats of the tempo
            None => self.beats = tempo_beats,
            Some(real) => {
                self.clock = self.clock.saturating_add(scale(now - real, self.speed));
                self.beats = self
                    .beats
                    .wrapping_add(scale(tempo_beats.wrapping_sub(self.tempo_beats), self.speed));
            }
        }

        self.clock_real = Some(now);
        self.tempo_beats = tempo_beats;
        self.clock
    }

    /// The speed master. Back at 1, the beats of the scenes fall on the ones of the tempo again.
    pub fn set_speed(&mut self, beat: &BeatClock, speed: Fix, now: Ns) {
        self.advance(beat, now);
        self.speed = speed;

        if speed == FIX_ONE {
            let apart = self.beats.wrapping_sub(self.tempo_beats);
            self.beats = self
                .tempo_beats
                .wrapping_add(((apart.wrapping_add(FIX_ONE / 2)) >> 32) << 32);
        }
    }

    pub fn reset(&mut self) {
        self.clock = 0;
        self.clock_real = None;
    }
}

/// A scene picked by hand (or flashed).
#[derive(Clone, Copy, Debug, Default)]
pub struct LiveSlot {
    pub state: u32,
    /// where its presets play from, on the clock of the live scenes and on its beats (a whole one)
    pub region_start: Ns,
    pub start_beats: Fix,
    /// when it was picked and let go, in real time
    pub picked: Ns,
    pub released: Ns,
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

impl LiveSlot {
    pub fn load(memory: &[u32], index: usize) -> LiveSlot {
        let at = index * LIVE_WORDS;

        LiveSlot {
            state: memory.get(at).copied().unwrap_or(NONE),
            region_start: get_i64(memory, at + 1),
            start_beats: get_i64(memory, at + 3),
            picked: get_i64(memory, at + 5),
            released: get_i64(memory, at + 7),
        }
    }

    pub fn store(&self, memory: &mut [u32], index: usize) {
        let at = index * LIVE_WORDS;

        if let Some(slot) = memory.get_mut(at) {
            *slot = self.state;
        }

        put_i64(memory, at + 1, self.region_start);
        put_i64(memory, at + 3, self.start_beats);
        put_i64(memory, at + 5, self.picked);
        put_i64(memory, at + 7, self.released);
    }

    pub fn is_playing(&self) -> bool {
        self.state != NONE
    }

    /// how far the fade in has come at a moment, 1 without one
    fn fade_in(&self, scene: &Scene, flash: bool, now: Ns) -> f32 {
        if flash || scene.fade_in <= 0 || now >= self.picked.saturating_add(scene.fade_in) {
            return 1.0;
        }

        transition_curve(scene.fade_in_curve, ratio(now - self.picked, scene.fade_in))
    }

    /// how much of the scene reaches the stage through its fades at a moment
    pub fn level(&self, scene: &Scene, flash: bool, now: Ns) -> f32 {
        if self.state != RELEASED {
            return self.fade_in(scene, flash, now);
        }

        if flash || scene.fade_out <= 0 || now >= self.released.saturating_add(scene.fade_out) {
            return 0.0;
        }

        // from where the fade in got to
        let remaining = 1.0 - ratio(now - self.released, scene.fade_out);
        self.fade_in(scene, flash, self.released) * transition_curve(scene.fade_out_curve, remaining)
    }

    pub fn is_over(&self, scene: &Scene, flash: bool, now: Ns) -> bool {
        self.state == NONE || (self.state == RELEASED && self.level(scene, flash, now) <= 0.0)
    }
}

/// when the fade in of a scene would have started to have come to a level by now
fn fade_in_start(scene: &Scene, level: f32, now: Ns) -> Ns {
    if scene.fade_in <= 0 || level >= 1.0 {
        return now - scene.fade_in.max(0);
    }

    // the curves only ever rise: find the point on it reaching the level
    let mut low = 0.0f32;
    let mut high = 1.0f32;

    for _ in 0..30 {
        let middle = (low + high) / 2.0;

        if transition_curve(scene.fade_in_curve, middle) < level {
            low = middle;
        } else {
            high = middle;
        }
    }

    now - (high * scene.fade_in as f32) as i64
}

/// A scene is picked (or flashed) or let go at a moment.
pub fn set_scene(slot: &mut LiveSlot, scene: &Scene, flash: bool, on: bool, clock: Ns, beats: Fix, now: Ns) {
    if !on {
        if slot.state == ON {
            slot.state = RELEASED;
            slot.released = now;
        }

        return;
    }

    if slot.is_over(scene, flash, now) {
        // picked anew: its presets play from here, on a whole beat
        *slot = LiveSlot {
            state: ON,
            region_start: clock,
            start_beats: fix_floor(beats.wrapping_add(4)) << 32,
            picked: now,
            released: 0,
        };
    } else if slot.state == RELEASED {
        // picked again while it fades out: it keeps playing, and fades in from where it is
        let level = slot.level(scene, flash, now);
        slot.state = ON;
        slot.picked = fade_in_start(scene, level, now);
    }
}
