//! The audio a pixel effect follows at a moment.
//!
//! The host analyzes the audio of the composition in frames of 20 ms. A frame holds [`FRAME_LENGTH`] values, all in dB
//! (0 at full scale): how loud it is (the [`RMS`] and the [`PEAK`]), how loud the [`BASS`], the [`MID`]s and the
//! [`HIGH`]s are and each of [`BANDS`] bands from the lowest to the highest (from [`FIRST_BAND`] on). The frame from 0
//! to 20 ms has the index 0. The host hands the frames over through [`AudioFrames`]; a frame it does not have is
//! silent.
//!
//! How loud the audio is for a pattern is an envelope: it starts from silence a second (50 frames) before the frame
//! it is asked for, and rises as fast as the attack and falls as fast as the release of the effect lets it.
//!
//! # How many frames the host has to keep
//!
//! A [`PixelAudio`] of a frame at index `n` reads the frames from `n - 300` to `n`, never a later one and never one
//! below 0: [`MAX_FRAMES_BACK`] frames back from the current one, 301 frames (6.02 s) in all.
//!
//! - the level and the bands of the moment read the frames `n - 50` to `n` ([`ENVELOPE_FRAMES`])
//! - the wave of the audio (the pattern `scope`) shows the level up to `25 + 225 × size` frames ago, which is 250
//!   frames for a size of 1, each with its envelope of 50 frames before it
//!
//! A size above 1 (which is out of its range) reaches further back: `75 + 225 × size` frames. Frames the host does
//! not have anymore are silent, nothing else happens.

use core::cell::Cell;

use crate::math::{clamp01, maxf, minf};
use crate::settings::AudioBand;

/// How long a frame of the analysis is, in µs.
pub const FRAME_MICROS: u64 = 20_000;

/// How many bands a frame has, from the lowest to the highest.
pub const BANDS: usize = 16;

/// Where the RMS is in a frame.
pub const RMS: usize = 0;
/// Where the peak is in a frame.
pub const PEAK: usize = 1;
/// Where the bass is in a frame.
pub const BASS: usize = 2;
/// Where the mids are in a frame.
pub const MID: usize = 3;
/// Where the highs are in a frame.
pub const HIGH: usize = 4;
/// Where the first of the bands is in a frame.
pub const FIRST_BAND: usize = 5;
/// How many values a frame has.
pub const FRAME_LENGTH: usize = FIRST_BAND + BANDS;

/// As quiet as a frame which is missing, in dB.
pub const SILENCE_DB: f32 = -100.0;

/// How many frames before a moment the envelope starts from silence: a second.
pub const ENVELOPE_FRAMES: i64 = 50;

/// How many frames back from the current one a [`PixelAudio`] reads at most, for a pattern of a size up to 1 (see the
/// module).
pub const MAX_FRAMES_BACK: i64 = 300;

/// A frame of the analysis: [`FRAME_LENGTH`] values in dB.
pub type AudioFrame = [f32; FRAME_LENGTH];

/// The frames of the analysis of the audio an effect follows, kept by the host.
pub trait AudioFrames {
    /// The frame with the passed index (the one from 0 to 20 ms is 0), none where there is none (yet, or anymore),
    /// which is as quiet as silence.
    fn frame(&self, index: i64) -> Option<AudioFrame>;
}

/// No audio at all: every frame is missing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Silence;

impl AudioFrames for Silence {
    fn frame(&self, _index: i64) -> Option<AudioFrame> {
        None
    }
}

/// What an effect sets for the audio it follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSettings {
    /// What of the audio it follows (`audioBand`).
    pub band: AudioBand,
    /// How much louder the audio is taken, in dB (`audioGainDb`, 0 by default).
    pub gain_db: f32,
    /// How quiet is silent, in dB (`audioFloorDb`, -60 by default). Above -1 counts as -1.
    pub floor_db: f32,
    /// How fast the level follows the audio up, in ms (`audioAttackMillis`, 20 by default).
    pub attack_millis: f32,
    /// How fast the level follows the audio down again, in ms (`audioReleaseMillis`, 300 by default).
    pub release_millis: f32,
}

impl AudioSettings {
    /// The defaults of a pixel effect.
    pub const DEFAULT: AudioSettings = AudioSettings {
        band: AudioBand::Full,
        gain_db: 0.0,
        floor_db: -60.0,
        attack_millis: 20.0,
        release_millis: 300.0,
    };
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// How loud an audio is between 0 (as quiet as the floor or quieter) and 1 (at full scale), with the gain.
pub fn level_of(db: f32, settings: &AudioSettings) -> f32 {
    let floor = minf(settings.floor_db, -1.0);

    clamp01((db + settings.gain_db - floor) / -floor)
}

/// The audio an effect follows at the moment of a frame of the lights.
///
/// Made once for an effect and a frame, and asked for every pixel: what was worked out last is kept, so the pixels of
/// a level meter or of a column of the wave of the audio do not work it out again.
pub struct PixelAudio<'a> {
    frames: &'a dyn AudioFrames,
    settings: AudioSettings,
    // the frame of the moment
    frame_index: i64,
    // the level of the band the effect follows, at the frame it was asked for last
    followed: Cell<Option<(i64, f32)>>,
    // the level of each band at the frame of the moment, not a number until it is worked out
    bands: [Cell<f32>; BANDS],
}

impl<'a> PixelAudio<'a> {
    /// The audio at a time of the composition, in µs.
    pub fn new(frames: &'a dyn AudioFrames, time_micros: u64, settings: AudioSettings) -> PixelAudio<'a> {
        PixelAudio {
            frames,
            settings,
            frame_index: (time_micros / FRAME_MICROS) as i64,
            followed: Cell::new(None),
            bands: [const { Cell::new(f32::NAN) }; BANDS],
        }
    }

    /// The index of the frame of the moment.
    pub fn frame_index(&self) -> i64 {
        self.frame_index
    }

    /// How loud the band the effect follows is, between 0 and 1.
    pub fn level(&self) -> f32 {
        self.level_ago(0)
    }

    /// How loud the band the effect follows was the passed number of frames before.
    pub fn level_ago(&self, frames: i32) -> f32 {
        let frame_index = self.frame_index.saturating_sub(frames as i64);

        if let Some((known_index, known)) = self.followed.get() {
            if known_index == frame_index {
                return known;
            }
        }

        let level = self.envelope(self.followed_index(), frame_index);

        self.followed.set(Some((frame_index, level)));

        level
    }

    /// How loud a band of the 16 is (0 the lowest).
    pub fn band(&self, band: i32) -> f32 {
        let band = if band < 0 { 0 } else { band as usize };
        let band = if band >= BANDS { BANDS - 1 } else { band };

        let Some(known) = self.bands.get(band) else {
            return 0.0;
        };

        if known.get().is_nan() {
            known.set(self.envelope(FIRST_BAND + band, self.frame_index));
        }

        known.get()
    }

    fn followed_index(&self) -> usize {
        match self.settings.band {
            AudioBand::Bass => BASS,
            AudioBand::Mid => MID,
            AudioBand::High => HIGH,
            AudioBand::Full => RMS,
        }
    }

    // the envelope of a value of the frames at a frame: rising as fast as the attack and falling as fast as the
    // release lets it, from silence a second before
    fn envelope(&self, value_index: usize, frame_index: i64) -> f32 {
        let frame_millis = (FRAME_MICROS / 1000) as f32;
        let attack = frame_millis / (maxf(self.settings.attack_millis, 0.0) + frame_millis);
        let release = frame_millis / (maxf(self.settings.release_millis, 0.0) + frame_millis);
        let first = frame_index.saturating_sub(ENVELOPE_FRAMES);
        let mut envelope = 0.0f32;

        for step in 0..=ENVELOPE_FRAMES {
            let index = first.saturating_add(step);
            let frame = if index >= 0 { self.frames.frame(index) } else { None };
            let db = frame
                .and_then(|frame| frame.get(value_index).copied())
                .unwrap_or(SILENCE_DB);
            let level = level_of(db, &self.settings);

            envelope += (level - envelope) * if level > envelope { attack } else { release };
        }

        // (settings which are no numbers: silence)
        if envelope.is_nan() {
            0.0
        } else {
            envelope
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // frames worked out from their index, which keep the lowest and the highest one asked for
    struct Recorded<F: Fn(i64) -> Option<AudioFrame>> {
        frame: F,
        asked: RefCell<Option<(i64, i64)>>,
    }

    impl<F: Fn(i64) -> Option<AudioFrame>> Recorded<F> {
        fn new(frame: F) -> Self {
            Recorded {
                frame,
                asked: RefCell::new(None),
            }
        }
    }

    impl<F: Fn(i64) -> Option<AudioFrame>> AudioFrames for Recorded<F> {
        fn frame(&self, index: i64) -> Option<AudioFrame> {
            let asked = self
                .asked
                .borrow()
                .map_or((index, index), |(low, high)| (low.min(index), high.max(index)));
            *self.asked.borrow_mut() = Some(asked);

            (self.frame)(index)
        }
    }

    // a frame with the passed value at every place, the bands each 2 dB quieter than the one before
    fn frame_of(db: f32) -> AudioFrame {
        let mut frame = [db; FRAME_LENGTH];
        frame[PEAK] = db + 3.0;
        frame[BASS] = db - 5.0;
        frame[MID] = db - 10.0;
        frame[HIGH] = db - 20.0;

        for band in 0..BANDS {
            frame[FIRST_BAND + band] = db - 2.0 * band as f32;
        }

        frame
    }

    // the envelope of a value of the frames at a frame, in doubles
    fn envelope_in_doubles(
        frame: impl Fn(i64) -> Option<AudioFrame>,
        value_index: usize,
        frame_index: i64,
        settings: &AudioSettings,
    ) -> f64 {
        let attack = 20.0 / ((settings.attack_millis as f64).max(0.0) + 20.0);
        let release = 20.0 / ((settings.release_millis as f64).max(0.0) + 20.0);
        let mut envelope = 0.0;

        for index in frame_index - 50..=frame_index {
            let db = if index >= 0 {
                frame(index).map(|frame| frame[value_index] as f64)
            } else {
                None
            }
            .unwrap_or(-100.0);
            let floor = (settings.floor_db as f64).min(-1.0);
            let level = ((db + settings.gain_db as f64 - floor) / -floor).clamp(0.0, 1.0);

            envelope = envelope + (level - envelope) * if level > envelope { attack } else { release };
        }

        envelope
    }

    #[test]
    fn says_how_loud_a_level_is() {
        let settings = AudioSettings::DEFAULT;

        assert_eq!(level_of(0.0, &settings), 1.0);
        assert_eq!(level_of(-30.0, &settings), 0.5);
        assert_eq!(level_of(-60.0, &settings), 0.0);
        assert_eq!(level_of(-100.0, &settings), 0.0);
        assert_eq!(level_of(6.0, &settings), 1.0);
        // with a gain, and a floor above -1 which counts as -1
        assert_eq!(
            level_of(
                -36.0,
                &AudioSettings {
                    gain_db: 6.0,
                    ..settings
                }
            ),
            0.5
        );
        assert_eq!(
            level_of(
                -0.5,
                &AudioSettings {
                    floor_db: 0.0,
                    ..settings
                }
            ),
            0.5
        );
        assert_eq!(
            level_of(
                -0.5,
                &AudioSettings {
                    floor_db: 20.0,
                    ..settings
                }
            ),
            0.5
        );
    }

    #[test]
    fn finds_the_frame_of_a_time() {
        let silence = Silence;

        assert_eq!(PixelAudio::new(&silence, 0, AudioSettings::DEFAULT).frame_index(), 0);
        assert_eq!(
            PixelAudio::new(&silence, 19_999, AudioSettings::DEFAULT).frame_index(),
            0
        );
        assert_eq!(
            PixelAudio::new(&silence, 20_000, AudioSettings::DEFAULT).frame_index(),
            1
        );
        assert_eq!(
            PixelAudio::new(&silence, 1_234_567, AudioSettings::DEFAULT).frame_index(),
            61
        );
        // a month in, where a float of the milliseconds would not count them anymore
        assert_eq!(
            PixelAudio::new(&silence, 2_678_400_000_000 + 39_999, AudioSettings::DEFAULT).frame_index(),
            133_920_001
        );
        assert_eq!(
            PixelAudio::new(&silence, u64::MAX, AudioSettings::DEFAULT).frame_index(),
            (u64::MAX / 20_000) as i64
        );
    }

    #[test]
    fn is_silent_without_frames() {
        let silence = Silence;
        let audio = PixelAudio::new(&silence, 5_000_000, AudioSettings::DEFAULT);

        assert_eq!(audio.level(), 0.0);
        assert_eq!(audio.level_ago(100), 0.0);
        assert_eq!(audio.band(3), 0.0);
    }

    #[test]
    fn follows_the_audio_as_in_doubles() {
        // louder and quieter again, with a gap of frames which are missing
        let frame = |index: i64| {
            if (130..140).contains(&index) {
                None
            } else {
                Some(frame_of(-40.0 + 35.0 * libm::sinf(index as f32 * 0.21)))
            }
        };

        for settings in [
            AudioSettings::DEFAULT,
            AudioSettings {
                band: AudioBand::Bass,
                gain_db: 6.0,
                floor_db: -48.0,
                attack_millis: 0.0,
                release_millis: 1000.0,
            },
            AudioSettings {
                band: AudioBand::Mid,
                gain_db: -3.0,
                floor_db: -72.0,
                attack_millis: 5.0,
                release_millis: 50.0,
            },
            AudioSettings {
                band: AudioBand::High,
                gain_db: 12.0,
                floor_db: 0.0,
                attack_millis: -10.0,
                release_millis: -10.0,
            },
        ] {
            let value_index = match settings.band {
                AudioBand::Full => RMS,
                AudioBand::Bass => BASS,
                AudioBand::Mid => MID,
                AudioBand::High => HIGH,
            };

            for frame_index in [0i64, 1, 7, 49, 50, 51, 120, 135, 145, 200, 1000] {
                let frames = Recorded::new(frame);
                let audio = PixelAudio::new(&frames, frame_index as u64 * 20_000 + 7_000, settings);
                let expected = envelope_in_doubles(frame, value_index, frame_index, &settings);

                assert!(
                    (audio.level() as f64 - expected).abs() < 2e-6,
                    "level at {frame_index}: {} != {expected}",
                    audio.level()
                );

                for ago in [0, 1, 10, 60, 250] {
                    let expected = envelope_in_doubles(frame, value_index, frame_index - ago as i64, &settings);

                    assert!(
                        (audio.level_ago(ago) as f64 - expected).abs() < 2e-6,
                        "level {ago} before {frame_index}"
                    );
                }

                for band in [-1, 0, 5, 15, 16] {
                    let expected =
                        envelope_in_doubles(frame, FIRST_BAND + band.clamp(0, 15) as usize, frame_index, &settings);

                    assert!(
                        (audio.band(band) as f64 - expected).abs() < 2e-6,
                        "band {band} at {frame_index}"
                    );
                    // asked again: the same
                    assert_eq!(audio.band(band), audio.band(band));
                }
            }
        }
    }

    #[test]
    fn rises_with_the_attack_and_falls_with_the_release() {
        // silence, then full scale from frame 100 on, then silence again from frame 200 on
        let frame = |index: i64| Some(frame_of(if (100..200).contains(&index) { 0.0 } else { -100.0 }));
        let level = |frame_index: u64| {
            PixelAudio::new(&Recorded::new(frame), frame_index * 20_000, AudioSettings::DEFAULT).level()
        };

        assert_eq!(level(99), 0.0);
        // an attack of 20 ms: half of the way with each frame
        assert_eq!(level(100), 0.5);
        assert_eq!(level(101), 0.75);
        assert!(level(150) > 0.999_999);
        // a release of 300 ms: a 16th of the way with each frame
        assert!((level(200) - 0.9375).abs() < 1e-6);
        assert!((level(201) - 0.9375 * 0.9375).abs() < 1e-6);
        assert!(level(240) < 0.08);
    }

    #[test]
    fn reads_no_frame_further_back_than_it_says() {
        let frame = |_index: i64| Some(frame_of(-20.0));

        // the level and the bands of the moment: a second back
        let frames = Recorded::new(frame);
        let audio = PixelAudio::new(&frames, 1000 * 20_000, AudioSettings::DEFAULT);
        audio.level();
        audio.band(0);
        audio.band(15);
        assert_eq!(*frames.asked.borrow(), Some((1000 - ENVELOPE_FRAMES, 1000)));

        // the wave of the audio at its longest: 250 frames ago
        audio.level_ago(250);
        assert_eq!(*frames.asked.borrow(), Some((1000 - MAX_FRAMES_BACK, 1000)));

        // never a frame before the first one
        let frames = Recorded::new(frame);
        let audio = PixelAudio::new(&frames, 10 * 20_000, AudioSettings::DEFAULT);
        audio.level_ago(250);
        audio.level();
        assert_eq!(*frames.asked.borrow(), Some((0, 10)));

        // and no panic at the ends of the numbers
        let audio = PixelAudio::new(&frames, u64::MAX, AudioSettings::DEFAULT);
        audio.level_ago(i32::MIN);
        audio.level_ago(i32::MAX);
    }

    #[test]
    fn works_a_level_out_once() {
        let frames = Recorded::new(|_index: i64| Some(frame_of(-20.0)));
        let audio = PixelAudio::new(&frames, 1000 * 20_000, AudioSettings::DEFAULT);
        let level = audio.level_ago(30);

        *frames.asked.borrow_mut() = None;
        assert_eq!(audio.level_ago(30), level);
        assert_eq!(audio.band(4), audio.band(4));
        audio.band(4);
        // only the band was read, once
        assert_eq!(*frames.asked.borrow(), Some((950, 1000)));
    }
}
