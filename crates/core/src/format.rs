//! The compiled show: the project with everything fixed already worked out, read in place.
//!
//! The layout is specified in `docs/format.md`. Everything is little-endian, lists are 4-byte aligned and named by an
//! offset from the start of the file and a count, and a name the engine compares (a profile, a channel, a wheel, a
//! blade) is a number given by the compiler. A record is decoded field by field, so no read depends on the alignment
//! or the byte order of the machine, and a read outside the file is an error instead of memory which isn't the show's.

use core::marker::PhantomData;

#[cfg(feature = "writer")]
extern crate alloc;
#[cfg(feature = "writer")]
use alloc::vec::Vec;

pub const MAGIC: [u8; 4] = *b"RSLE";
pub const FORMAT_MAJOR: u16 = 1;
pub const FORMAT_MINOR: u16 = 0;

/// Counts up with every change of the engine which changes the values it comes to.
pub const BEHAVIOUR_VERSION: u32 = 1;

pub const HEADER_SIZE: usize = 48;
pub const CHUNK_ENTRY_SIZE: usize = 12;

pub const CHUNK_SHOW: [u8; 4] = *b"SHOW";
pub const CHUNK_PATCH: [u8; 4] = *b"PTCH";
/// The project JSON, which the engine never reads (optional).
pub const CHUNK_PROJECT: [u8; 4] = *b"PROJ";

/// What a show needs the engine to be able to do. The compiler sets only the bits a show uses.
pub mod feature {
    /// presets with more than one step
    pub const STEPS: u32 = 1 << 0;
    /// steps played backward, back and forth or shuffled
    pub const STEP_ORDERS: u32 = 1 << 1;
    /// effect curves
    pub const CURVES: u32 = 1 << 2;
    /// movements of the pan and the tilt
    pub const MOVEMENTS: u32 = 1 << 3;
    /// pixel effects
    pub const PIXELS: u32 = 1 << 4;
    /// pixel effects showing a text or an image
    pub const PIXEL_BITMAPS: u32 = 1 << 5;
    /// pixel effects following the audio
    pub const AUDIO: u32 = 1 << 6;
    /// values fanned out over the fixtures
    pub const FANS: u32 = 1 << 7;
    /// moving heads aimed at a spot on the stage
    pub const AIMS: u32 = 1 << 8;
    /// compositions with a timeline
    pub const TIMELINES: u32 = 1 << 9;
    /// steps and curves following the beat
    pub const BEATS: u32 = 1 << 10;

    /// Everything this engine plays.
    pub const SUPPORTED: u32 =
        STEPS | STEP_ORDERS | CURVES | MOVEMENTS | PIXELS | PIXEL_BITMAPS | AUDIO | FANS | AIMS | TIMELINES | BEATS;
}

/// The number of a name which isn't there (null in the project).
pub const NAME_NONE: u16 = 0;
/// The number of the empty name, which counts as none where a name may be left empty.
pub const NAME_EMPTY: u16 = 1;

pub fn no_name(name: u16) -> bool {
    name == NAME_NONE || name == NAME_EMPTY
}

/// The types of the capabilities of the fixture library, numbered in the order the compiler lists them in.
pub mod ty {
    pub const NO_FUNCTION: u8 = 0;
    pub const SHUTTER_STROBE: u8 = 1;
    pub const STROBE_SPEED: u8 = 2;
    pub const STROBE_DURATION: u8 = 3;
    pub const INTENSITY: u8 = 4;
    pub const COLOR_INTENSITY: u8 = 5;
    pub const COLOR_PRESET: u8 = 6;
    pub const COLOR_TEMPERATURE: u8 = 7;
    pub const PAN: u8 = 8;
    pub const PAN_CONTINUOUS: u8 = 9;
    pub const TILT: u8 = 10;
    pub const TILT_CONTINUOUS: u8 = 11;
    pub const PAN_TILT_SPEED: u8 = 12;
    pub const WHEEL_SLOT: u8 = 13;
    pub const WHEEL_SHAKE: u8 = 14;
    pub const WHEEL_SLOT_ROTATION: u8 = 15;
    pub const WHEEL_ROTATION: u8 = 16;
    pub const EFFECT_SPEED: u8 = 17;
    pub const EFFECT_DURATION: u8 = 18;
    pub const EFFECT_PARAMETER: u8 = 19;
    pub const SOUND_SENSITIVITY: u8 = 20;
    pub const FOCUS: u8 = 21;
    pub const ZOOM: u8 = 22;
    pub const IRIS: u8 = 23;
    pub const IRIS_EFFECT: u8 = 24;
    pub const FROST: u8 = 25;
    pub const FROST_EFFECT: u8 = 26;
    pub const PRISM: u8 = 27;
    pub const PRISM_ROTATION: u8 = 28;
    pub const BLADE_INSERTION: u8 = 29;
    pub const BLADE_ROTATION: u8 = 30;
    pub const BLADE_SYSTEM_ROTATION: u8 = 31;
    pub const FOG: u8 = 32;
    pub const FOG_OUTPUT: u8 = 33;
    pub const FOG_TYPE: u8 = 34;
    pub const BEAM_ANGLE: u8 = 35;
    pub const ROTATION: u8 = 36;
    pub const SPEED: u8 = 37;
    pub const TIME: u8 = 38;
    pub const MAINTENANCE: u8 = 39;
    pub const GENERIC: u8 = 40;
    pub const EFFECT: u8 = 41;
    pub const BEAM_POSITION: u8 = 42;
    /// no type, or one this engine doesn't know
    pub const NONE: u8 = 255;
}

/// The colors of the capabilities, numbered in the order the compiler lists them in (0: none).
pub mod color {
    pub const NONE: u8 = 0;
    pub const RED: u8 = 1;
    pub const GREEN: u8 = 2;
    pub const BLUE: u8 = 3;
    pub const CYAN: u8 = 4;
    pub const MAGENTA: u8 = 5;
    pub const YELLOW: u8 = 6;
    pub const AMBER: u8 = 7;
    pub const WHITE: u8 = 8;
    pub const WARM_WHITE: u8 = 9;
    pub const COLD_WHITE: u8 = 10;
    pub const UV: u8 = 11;
    pub const LIME: u8 = 12;
    pub const INDIGO: u8 = 13;
}

/// What a shutter does, of a capability and of a value.
pub mod shutter {
    pub const NONE: u8 = 0;
    pub const OPEN: u8 = 1;
    pub const CLOSED: u8 = 2;
    pub const STROBE: u8 = 3;
    pub const PULSE: u8 = 4;
    pub const RAMP_UP: u8 = 5;
    pub const RAMP_DOWN: u8 = 6;
    pub const RAMP_UP_DOWN: u8 = 7;
    pub const LIGHTNING: u8 = 8;
    pub const SPIKES: u8 = 9;
    pub const BURST: u8 = 10;
    /// of a value only: a strobe flashing at random or to the sound
    pub const RANDOM: u8 = 11;
    pub const SOUND: u8 = 12;
    /// anything else the fixture library may name
    pub const OTHER: u8 = 255;
}

/// What a fixture is asked to do: the index of the action.
pub mod maintenance {
    pub const RESET: u8 = 0;
    pub const LAMP_ON: u8 = 1;
    pub const LAMP_OFF: u8 = 2;
    pub const NONE: u8 = 255;
}

/// How a fade or a transition is shaped.
pub mod transition {
    pub const LINEAR: u8 = 0;
    pub const EASE_IN: u8 = 1;
    pub const EASE_OUT: u8 = 2;
    pub const EASE_IN_OUT: u8 = 3;
    pub const SNAP: u8 = 4;
}

// ---- reading and writing fields ----

/// Something of a fixed size in the file.
pub trait Field: Sized + Copy {
    const SIZE: usize;

    fn read(bytes: &[u8], at: usize) -> Option<Self>;

    #[cfg(feature = "writer")]
    fn write(&self, out: &mut Vec<u8>);
}

macro_rules! number_field {
    ($($t:ty),*) => {
        $(impl Field for $t {
            const SIZE: usize = core::mem::size_of::<$t>();

            fn read(bytes: &[u8], at: usize) -> Option<Self> {
                let end = at.checked_add(Self::SIZE)?;
                let mut raw = [0u8; core::mem::size_of::<$t>()];
                raw.copy_from_slice(bytes.get(at..end)?);
                Some(<$t>::from_le_bytes(raw))
            }

            #[cfg(feature = "writer")]
            fn write(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
        })*
    };
}

number_field!(u8, u16, u32, i16, i32, i64, f32);

/// A list of records somewhere in the file: where it starts and how many there are.
pub struct List<T> {
    pub offset: u32,
    pub count: u32,
    kind: PhantomData<T>,
}

impl<T> List<T> {
    pub const EMPTY: List<T> = List {
        offset: 0,
        count: 0,
        kind: PhantomData,
    };

    pub const fn new(offset: u32, count: u32) -> Self {
        List {
            offset,
            count,
            kind: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl<T> Clone for List<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for List<T> {}

impl<T> Default for List<T> {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl<T> core::fmt::Debug for List<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "List({}, {})", self.offset, self.count)
    }
}

impl<T: Copy> Field for List<T> {
    const SIZE: usize = 8;

    fn read(bytes: &[u8], at: usize) -> Option<Self> {
        Some(List::new(u32::read(bytes, at)?, u32::read(bytes, at + 4)?))
    }

    #[cfg(feature = "writer")]
    fn write(&self, out: &mut Vec<u8>) {
        self.offset.write(out);
        self.count.write(out);
    }
}

macro_rules! record {
    ($(#[$meta:meta])* $name:ident { $($(#[$field_meta:meta])* $field:ident : $t:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $name {
            $($(#[$field_meta])* pub $field: $t),*
        }

        impl Field for $name {
            const SIZE: usize = 0 $(+ <$t as Field>::SIZE)*;

            fn read(bytes: &[u8], at: usize) -> Option<Self> {
                let mut at = at;
                $(
                    let $field = <$t as Field>::read(bytes, at)?;
                    at += <$t as Field>::SIZE;
                )*
                let _ = at;
                Some($name { $($field),* })
            }

            #[cfg(feature = "writer")]
            fn write(&self, out: &mut Vec<u8>) {
                $(self.$field.write(out);)*
            }
        }

        const _: () = assert!(<$name as Field>::SIZE % 4 == 0);
    };
}

// ---- the records ----

record!(
    /// Where everything is.
    Root {
        master_dimmer: f32,
        /// the composition selected in the project, whose tempo a preview follows (-1: none)
        selected_composition: i32,
        fixtures: List<Fixture>,
        parts: List<Part>,
        layouts: List<Layout>,
        presets: List<Preset>,
        scenes: List<Scene>,
        compositions: List<Composition>,
        /// for each spot the moving heads are aimed at, where it points each fixture
        aims: List<Aim>,
        /// the channels of all parts
        total_channels: u32,
        /// the most channels a part has
        max_part_channels: u32,
        /// the most presets playing at once
        max_layers: u32,
        /// the most effects following the presets on a fixture at once
        max_following: u32,
        /// the most clips a composition has
        max_regions: u32,
        /// the most hold points a composition has
        max_holds: u32,
        universes: u32,
        /// the frames of audio the pixel effects look back over (0 for a show which follows none)
        audio_history: u32,
    }
);

record!(
    /// A fixture of the project.
    Fixture {
        /// the name of its profile
        profile: u16,
        reserved: u16,
    }
);

record!(
    /// Where a fixture is patched (in the patch chunk, one for each fixture).
    Patch {
        universe: u32,
        /// its first channel in the universe, from 0
        address: i32,
        /// how bright it lights at most, a master dimmer of its own
        brightness: f32,
    }
);

pub mod part_flag {
    pub const PIXEL: u32 = 1;
}

record!(
    /// A part of a fixture with channels of its own: the fixture itself, or one of its pixels.
    Part {
        fixture: u32,
        layout: u32,
        /// the part before it on the same address and pixel, which it takes the values of (-1: none)
        same_lamp: i32,
        /// where its values start among the values of all parts
        value_base: u32,
        flags: u32,
    }
);

record!(
    /// Where a part of a fixture is in the picture a pixel effect paints.
    Place {
        x: f32,
        y: f32,
        aspect_x: f32,
        aspect_y: f32,
        column: i32,
        row: i32,
        columns: i32,
        rows: i32,
    }
);

record!(
    /// The place of a part of a preset in the picture of the stage, if it has one.
    StagePlace {
        known: u32,
        place: Place,
    }
);

pub mod layout_flag {
    pub const COLOR_WHEEL: u32 = 1;
    pub const COLOR_TEMPERATURE: u32 = 2;
}

record!(
    /// The channels of a part and what is worked out from them once.
    Layout {
        channels: List<Chan>,
        outputs: List<Out>,
        /// the open slots of its color wheels, which identifying it turns to
        open_slots: List<OpenSlot>,
        /// the first channel lighting in red, green, blue and white (-1: none)
        red: i16,
        green: i16,
        blue: i16,
        white: i16,
        /// the first channel with a dimmer
        dimmer: i16,
        /// the dimmer of a pixel lighting in white only
        white_pixel_dimmer: i16,
        virtual_dimmer: i16,
        /// the channel resetting the fixture and the one switching its lamp on, with the value doing it
        reset: i16,
        lamp_on: i16,
        reserved: i16,
        reset_value: f32,
        lamp_on_value: f32,
        /// the colors it has a channel for, a bit for each
        colors: u32,
        flags: u32,
        place: Place,
    }
);

pub mod chan_flag {
    pub const DEFAULT: u32 = 1 << 0;
    pub const COLOR_WHEEL: u32 = 1 << 1;
    pub const PAN: u32 = 1 << 2;
    pub const TILT: u32 = 1 << 3;
    /// it has a value for the maintenance action 0, 1 and 2
    pub const MAINTENANCE: u32 = 1 << 4;
}

record!(
    /// A channel of a part.
    Chan {
        name: u16,
        /// the number of its fine channels
        fine: u16,
        flags: u32,
        /// the types of its capabilities, a bit for each (the types 0 to 31, and 32 and above)
        types_low: u32,
        types_high: u32,
        max: f32,
        default: f32,
        /// the degrees it turns the fixture over
        pan_range: f32,
        tilt_range: f32,
        maintenance_reset: f32,
        maintenance_lamp_on: f32,
        maintenance_lamp_off: f32,
        /// its first capability without a function (-1: none)
        no_function: i32,
        caps: List<Cap>,
        /// its programs, in the order of their values: the ones not to the sound and the ones to it
        programs: List<u16>,
        sound_programs: List<u16>,
    }
);

pub mod cap_flag {
    /// a range of exactly two values, and one of at least two
    pub const RANGE: u32 = 1 << 0;
    pub const RANGE_OR_MORE: u32 = 1 << 1;
    pub const REVERSED: u32 = 1 << 2;
    pub const NOTHING: u32 = 1 << 3;
    pub const RANDOM: u32 = 1 << 4;
    pub const SOUND: u32 = 1 << 5;
    /// a prism standing still
    pub const STILL: u32 = 1 << 6;
    pub const ROTATION_ENDS: u32 = 1 << 7;
    pub const ANGLES: u32 = 1 << 8;
    pub const SLOT: u32 = 1 << 9;
    pub const WHEEL_IS_COLOR: u32 = 1 << 10;
    pub const SLOT_COLOR: u32 = 1 << 11;
    pub const PRESET_COLOR: u32 = 1 << 12;
    /// a program with a speed
    pub const SPEED: u32 = 1 << 13;
}

record!(
    /// A capability of a channel.
    Cap {
        ty: u8,
        color: u8,
        shutter: u8,
        reserved: u8,
        flags: u32,
        low: f32,
        high: f32,
        center: f32,
        brightness_start: f32,
        brightness_end: f32,
        rotation_start: f32,
        rotation_end: f32,
        angle_start: f32,
        angle_end: f32,
        slot: f32,
        /// the color of its slot of a color wheel
        slot_red: f32,
        slot_green: f32,
        slot_blue: f32,
        /// the color of a color preset
        preset_red: f32,
        preset_green: f32,
        preset_blue: f32,
        /// the wheel of its slot, the wheel it shakes and its blade
        wheel: u16,
        shake_wheel: u16,
        blade: u16,
        reserved2: u16,
        /// the wheels it turns
        turned: List<u16>,
    }
);

record!(
    /// Where a channel of a part is sent: its address from the first one of the fixture, and the byte of the value.
    Out {
        address: u16,
        chan: u16,
        /// the bits the value is shifted by for this byte
        shift: u32,
    }
);

record!(
    /// An open slot of a color wheel.
    OpenSlot {
        wheel: u16,
        reserved: u16,
        slot: f32,
    }
);

record!(
    /// Where a spot points a fixture, in degrees from the middle of its ranges.
    Aim {
        known: u32,
        pan: f32,
        tilt: f32,
    }
);

pub mod preset_flag {
    pub const MIRROR_PAN: u32 = 1 << 0;
    pub const MIRROR_TILT: u32 = 1 << 1;
    pub const EMITTERS_FROM_COLOR: u32 = 1 << 2;
    pub const LOOP: u32 = 1 << 3;
    pub const PHASED: u32 = 1 << 4;
    pub const BEATS: u32 = 1 << 5;
    pub const START: u32 = 1 << 6;
    pub const END: u32 = 1 << 7;
    pub const FADE_IN_PRE: u32 = 1 << 8;
    pub const FADE_OUT_POST: u32 = 1 << 9;
}

pub mod order {
    pub const FORWARD: u8 = 0;
    pub const BACKWARD: u8 = 1;
    pub const PINGPONG: u8 = 2;
    pub const RANDOM: u8 = 3;
}

record!(
    Preset {
        flags: u32,
        order: u8,
        fade_in_curve: u8,
        fade_out_curve: u8,
        reserved: u8,
        start: i64,
        end: i64,
        fade_in: i64,
        fade_out: i64,
        /// the length of a pass through the steps as they are written down, and in the order they are played in
        loop_length: i64,
        pass_length: i64,
        /// the tempo steps following the beat are written at
        steps_bpm: i64,
        seed: i32,
        /// the number of fixtures a pixel effect chases over
        whole_count: u32,
        steps: List<Step>,
        effects: List<Effect>,
        /// its parts, ordered by their number
        parts: List<PresetPart>,
        /// the steps lasting any time, in the order they are played in (shuffled anew for each pass by a random order)
        playable: List<u16>,
        /// the places of its parts in the picture of the stage, seen from the front and from above (or none)
        stage_front: List<StagePlace>,
        stage_top: List<StagePlace>,
    }
);

pub mod preset_part_flag {
    pub const FAN_PLACE: u16 = 1;
}

record!(
    /// A part of a preset: where it is in its chase and in its fan.
    PresetPart {
        part: u32,
        index: u16,
        count: u16,
        whole_index: u16,
        flags: u16,
        fan_place: f32,
        /// how far behind its steps run
        phasing: i64,
    }
);

pub mod step_flag {
    pub const TRANSITION: u32 = 1;
}

record!(
    Step {
        start: i64,
        transition: i64,
        /// how long it lasts as it is written down
        length: i64,
        flags: u32,
        curve: u32,
        caps: List<CapV>,
        chans: List<ChanV>,
        amounts: List<Amount>,
    }
);

pub mod capv_flag {
    pub const PERCENTAGE: u32 = 1 << 0;
    pub const SLOT: u32 = 1 << 1;
    pub const DEGREES: u32 = 1 << 2;
    pub const SPREAD: u32 = 1 << 3;
    pub const AIM: u32 = 1 << 4;
    pub const PROGRAM: u32 = 1 << 5;
    pub const SOUND: u32 = 1 << 6;
    pub const FAN: u32 = 1 << 7;
    pub const FAN_STEPS: u32 = 1 << 8;
    pub const FAN_CENTER: u32 = 1 << 9;
}

record!(
    /// A value of a step for a capability.
    CapV {
        ty: u8,
        color: u8,
        shutter: u8,
        maintenance: u8,
        flags: u32,
        profile: u16,
        wheel: u16,
        blade: u16,
        reserved: u16,
        percentage: f32,
        slot: f32,
        degrees: f32,
        spread: f32,
        program: i32,
        aim: u32,
        /// the stops of its fan, ordered by their place
        fan: List<FanStop>,
    }
);

record!(
    /// A stop of a fan: a value at a place along it (0 to 1).
    FanStop {
        position: f32,
        value: f32,
    }
);

record!(
    /// A value of a step for a channel.
    ChanV {
        profile: u16,
        name: u16,
        value: f32,
    }
);

record!(
    /// How far a step opens an effect of its preset.
    Amount {
        effect: u32,
        amount: f32,
    }
);

pub mod effect_kind {
    pub const CURVE: u8 = 1;
    pub const MOVEMENT: u8 = 2;
    pub const PIXEL: u8 = 3;
}

pub mod effect_flag {
    pub const BEAT_SYNCED: u32 = 1 << 0;
    pub const PAN_TILT_IN_DEGREES: u32 = 1 << 1;
    pub const END_BASE: u32 = 1 << 2;
    /// a curve playing on the dimmer
    pub const INTENSITY: u32 = 1 << 3;
}

pub mod curve {
    pub const SINE: u8 = 0;
    pub const SQUARE: u8 = 1;
    pub const TRIANGLE: u8 = 2;
    pub const SAWTOOTH: u8 = 3;
    pub const REVERSE_SAWTOOTH: u8 = 4;
    pub const NONE: u8 = 255;
}

pub mod phasing {
    pub const MILLIS: u8 = 0;
    pub const BEATS: u8 = 1;
    pub const SPREAD: u8 = 2;
}

pub mod run {
    pub const INFINITE: u8 = 0;
    pub const CYCLES: u8 = 1;
    pub const DURATION: u8 = 2;
}

pub mod fan_mode {
    pub const NONE: u8 = 0;
    pub const LINEAR: u8 = 1;
    pub const CENTER: u8 = 2;
    pub const SIDE: u8 = 3;
}

pub mod movement {
    pub const CIRCLE: u8 = 0;
    pub const FIGURE8: u8 = 1;
    pub const LINE: u8 = 2;
    pub const WAVE: u8 = 3;
}

record!(
    /// An effect of a preset: a curve, a movement or a pixel effect, which both run on the timing of a curve.
    Effect {
        kind: u8,
        curve: u8,
        phasing_mode: u8,
        run_mode: u8,
        fan_mode: u8,
        shape: u8,
        reserved: u16,
        flags: u32,
        length: i64,
        phase: i64,
        phasing: i64,
        run_duration: i64,
        length_beats: i64,
        phase_beats: i64,
        phasing_beats: i64,
        phasing_cycles: i64,
        group_size: u32,
        run_cycles: u32,
        amplitude: f32,
        position: f32,
        duty: f32,
        size_pan: f32,
        size_tilt: f32,
        caps: List<CurveCap>,
        chans: List<ChanV>,
        /// what a pixel effect paints (one, or none)
        pixel: List<PixelFx>,
    }
);

record!(
    /// A capability a curve plays on.
    CurveCap {
        ty: u8,
        color: u8,
        reserved: u16,
    }
);

pub mod pixel_flag {
    pub const BOUNCE: u32 = 1 << 0;
    pub const MIRROR: u32 = 1 << 1;
    pub const WHITE_LED: u32 = 1 << 2;
    pub const STILL: u32 = 1 << 3;
    pub const STAGE: u32 = 1 << 4;
    pub const VIEW_TOP: u32 = 1 << 5;
    pub const AUDIO: u32 = 1 << 6;
    pub const FOLLOWS_SIZE: u32 = 1 << 7;
    /// in the shades of the color of the preset
    pub const PRESET_PALETTE: u32 = 1 << 8;
}

pub mod blend {
    pub const REPLACE: u8 = 0;
    pub const MASK: u8 = 1;
    pub const ADD: u8 = 2;
    pub const MAX: u8 = 3;
}

record!(
    /// What a pixel effect paints. The numbers of the pattern and its settings are the ones of the pixel crate.
    PixelFx {
        flags: u32,
        pattern: u8,
        direction: u8,
        shape: u8,
        scroll: u8,
        align: u8,
        fit: u8,
        text_colors: u8,
        image_colors: u8,
        between: u8,
        repeat: u8,
        blend: u8,
        audio_band: u8,
        size: f32,
        softness: f32,
        offset_y: f32,
        audio_gain: f32,
        audio_floor: f32,
        audio_attack: f32,
        audio_release: f32,
        /// the seed times 7919, as a 32.32 fixed-point number
        seed: i64,
        /// the stops of its palette, 16 bytes each
        stops: List<u8>,
        /// its picture, in the layout of the pixel crate
        bitmap: List<u8>,
    }
);

pub mod scene_flag {
    pub const FADE_IN_PRE: u32 = 1;
    pub const FADE_OUT_POST: u32 = 2;
}

record!(
    Scene {
        dimmer: f32,
        flags: u32,
        fade_in: i64,
        fade_out: i64,
        fade_in_curve: u8,
        fade_out_curve: u8,
        reserved: u16,
        /// its presets, the topmost first
        presets: List<u16>,
    }
);

pub mod composition_flag {
    /// it follows no audio or video, so it may follow a tempo from outside
    pub const SYNC_NONE: u32 = 1;
}

record!(
    Composition {
        /// its tempo, in 32.32 fixed-point beats per minute
        bpm: i64,
        flags: u32,
        beats_per_bar: u32,
        /// the presets of its clips in the order they are mixed in, the ones at the bottom first
        clips: List<Clip>,
        regions: List<Region>,
        /// its hold points, ordered
        holds: List<i64>,
    }
);

pub mod clip_flag {
    /// played by the preview as well: its track is neither hidden nor left out by a solo
    pub const PREVIEW: u16 = 1;
}

record!(
    /// A preset of a scene on a clip of a timeline, with when it plays (its fades included).
    Clip {
        play_start: i64,
        play_end: i64,
        preset: u16,
        scene: u16,
        region: u16,
        flags: u16,
    }
);

record!(
    /// A clip of a timeline: when it starts and when it ends.
    Region {
        /// when it starts, in nanoseconds of the timeline
        start: i64,
        /// when it ends, in nanoseconds of the timeline
        end: i64,
    }
);

// ---- the show ----

/// Why a show can't be played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowError {
    /// not a compiled show
    Magic,
    /// written for another major version of the format
    Version { major: u16, minor: u16 },
    /// it needs something this engine can't do: the bits missing
    Features(u32),
    /// shorter or longer than it says
    Length,
    /// damaged
    Checksum,
    /// a chunk the engine needs is missing
    Chunk,
    /// an offset or a number of the show points outside of it
    Corrupt,
    /// the working memory handed over is too small: the words needed
    Memory(u32),
}

/// What the header of a show says.
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub major: u16,
    pub minor: u16,
    pub features: u32,
    pub behaviour: u32,
    pub length: u32,
    pub checksum: u32,
    pub chunks: u32,
    /// the working memory the show needs, in 32-bit words
    pub memory_words: u32,
    pub universes: u32,
}

impl Header {
    pub fn read(bytes: &[u8]) -> Result<Header, ShowError> {
        if bytes.get(0..4) != Some(&MAGIC[..]) {
            return Err(ShowError::Magic);
        }

        let field = |at| u32::read(bytes, at).ok_or(ShowError::Length);
        let major = u16::read(bytes, 4).ok_or(ShowError::Length)?;
        let minor = u16::read(bytes, 6).ok_or(ShowError::Length)?;

        Ok(Header {
            major,
            minor,
            features: field(8)?,
            behaviour: field(12)?,
            length: field(16)?,
            checksum: field(20)?,
            chunks: field(24)?,
            memory_words: field(28)?,
            universes: field(32)?,
        })
    }
}

// (worked out when the engine is built: nothing of it runs later)
#[allow(clippy::indexing_slicing)]
const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;

    while i < 256 {
        let mut value = i as u32;
        let mut bit = 0;

        while bit < 8 {
            value = if value & 1 != 0 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }

        table[i] = value;
        i += 1;
    }

    table
}

static CRC_TABLE: [u32; 256] = crc_table();

/// The CRC-32 of a show: of all of its bytes, the four of the checksum itself taken as zero.
pub fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;

    for (index, byte) in bytes.iter().enumerate() {
        let byte = if (20..24).contains(&index) { 0 } else { *byte };
        let entry = CRC_TABLE
            .get(((crc ^ byte as u32) & 0xFF) as usize)
            .copied()
            .unwrap_or(0);
        crc = entry ^ (crc >> 8);
    }

    !crc
}

/// A compiled show, read in place.
#[derive(Clone, Copy)]
pub struct Show<'a> {
    bytes: &'a [u8],
    pub header: Header,
    pub root: Root,
    patch: List<Patch>,
}

/// The records of a list, one after the other.
pub struct ListIter<'a, T> {
    bytes: &'a [u8],
    at: usize,
    left: usize,
    kind: PhantomData<T>,
}

impl<'a, T: Field> Iterator for ListIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.left == 0 {
            return None;
        }

        let item = T::read(self.bytes, self.at)?;
        self.at += T::SIZE;
        self.left -= 1;
        Some(item)
    }
}

impl<'a> Show<'a> {
    /// Read the header and find the chunks, without checking the rest (see `check`).
    pub fn open(bytes: &'a [u8]) -> Result<Show<'a>, ShowError> {
        let header = Header::read(bytes)?;

        if header.major != FORMAT_MAJOR {
            return Err(ShowError::Version {
                major: header.major,
                minor: header.minor,
            });
        }

        if header.features & !feature::SUPPORTED != 0 {
            return Err(ShowError::Features(header.features & !feature::SUPPORTED));
        }

        if header.length as usize != bytes.len() {
            return Err(ShowError::Length);
        }

        let mut root = None;
        let mut patch = None;

        for index in 0..header.chunks as usize {
            let at = HEADER_SIZE + index * CHUNK_ENTRY_SIZE;
            let kind = bytes.get(at..at + 4).ok_or(ShowError::Length)?;
            let offset = u32::read(bytes, at + 4).ok_or(ShowError::Length)?;
            let length = u32::read(bytes, at + 8).ok_or(ShowError::Length)?;

            if offset as usize + length as usize > bytes.len() {
                return Err(ShowError::Corrupt);
            }

            if kind == CHUNK_SHOW {
                root = Some(Root::read(bytes, offset as usize).ok_or(ShowError::Corrupt)?);
            } else if kind == CHUNK_PATCH {
                patch = Some(List::<Patch>::new(offset, length / Patch::SIZE as u32));
            }
            // any other chunk is skipped
        }

        let root = root.ok_or(ShowError::Chunk)?;
        let patch = patch.ok_or(ShowError::Chunk)?;

        if patch.count != root.fixtures.count {
            return Err(ShowError::Corrupt);
        }

        Ok(Show {
            bytes,
            header,
            root,
            patch,
        })
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The record at a place of a list.
    pub fn get<T: Field>(&self, list: List<T>, index: usize) -> Option<T> {
        if index >= list.count as usize {
            return None;
        }

        T::read(
            self.bytes,
            (list.offset as usize).checked_add(index.checked_mul(T::SIZE)?)?,
        )
    }

    pub fn iter<T: Field>(&self, list: List<T>) -> ListIter<'a, T> {
        ListIter {
            bytes: self.bytes,
            at: list.offset as usize,
            left: list.count as usize,
            kind: PhantomData,
        }
    }

    /// The bytes of a list of bytes.
    pub fn blob(&self, list: List<u8>) -> Option<&'a [u8]> {
        let start = list.offset as usize;
        self.bytes.get(start..start.checked_add(list.count as usize)?)
    }

    /// The name, the flags and the types of the capabilities of a channel, without reading the rest of it.
    pub fn chan_head(&self, list: List<Chan>, index: usize) -> Option<(u16, u32, u64)> {
        if index >= list.count as usize {
            return None;
        }

        let at = (list.offset as usize).checked_add(index.checked_mul(Chan::SIZE)?)?;
        let name = u16::read(self.bytes, at)?;
        let flags = u32::read(self.bytes, at + 4)?;
        let low = u32::read(self.bytes, at + 8)?;
        let high = u32::read(self.bytes, at + 12)?;
        Some((name, flags, low as u64 | (high as u64) << 32))
    }

    /// The value a channel starts out with, none for a channel without one.
    pub fn chan_default(&self, list: List<Chan>, index: usize) -> Option<f32> {
        if index >= list.count as usize {
            return None;
        }

        let at = (list.offset as usize).checked_add(index.checked_mul(Chan::SIZE)?)?;

        if u32::read(self.bytes, at + 4)? & chan_flag::DEFAULT == 0 {
            return None;
        }

        f32::read(self.bytes, at + 20)
    }

    pub fn patch(&self, fixture: usize) -> Option<Patch> {
        self.get(self.patch, fixture)
    }

    fn list_ok<T: Field>(&self, list: List<T>) -> Result<(), ShowError> {
        let size = (list.count as usize).checked_mul(T::SIZE).ok_or(ShowError::Corrupt)?;
        let end = (list.offset as usize).checked_add(size).ok_or(ShowError::Corrupt)?;

        if end > self.bytes.len() || (list.count > 0 && !list.offset.is_multiple_of(4)) {
            return Err(ShowError::Corrupt);
        }

        Ok(())
    }

    /// Validate a show once, before it plays: the checksum, every list and every number naming something else. A
    /// show passing it can't send a read outside of itself, and names nothing which isn't there.
    pub fn check(bytes: &'a [u8]) -> Result<Show<'a>, ShowError> {
        let show = Show::open(bytes)?;

        if checksum(bytes) != show.header.checksum {
            return Err(ShowError::Checksum);
        }

        let root = show.root;
        show.list_ok(root.fixtures)?;
        show.list_ok(root.parts)?;
        show.list_ok(root.layouts)?;
        show.list_ok(root.presets)?;
        show.list_ok(root.scenes)?;
        show.list_ok(root.compositions)?;
        show.list_ok(root.aims)?;

        let corrupt = |ok: bool| if ok { Ok(()) } else { Err(ShowError::Corrupt) };
        let mut channels = 0u32;

        for (index, part) in show.iter(root.parts).enumerate() {
            corrupt(part.fixture < root.fixtures.count && part.layout < root.layouts.count)?;
            corrupt(part.same_lamp < index as i32)?;
            let layout = show.get(root.layouts, part.layout as usize).ok_or(ShowError::Corrupt)?;
            corrupt(part.value_base == channels && layout.channels.count <= root.max_part_channels)?;
            channels = channels.checked_add(layout.channels.count).ok_or(ShowError::Corrupt)?;

            if part.same_lamp >= 0 {
                // the same lamp has the same channels
                let other = show
                    .get(root.parts, part.same_lamp as usize)
                    .ok_or(ShowError::Corrupt)?;
                let other_layout = show
                    .get(root.layouts, other.layout as usize)
                    .ok_or(ShowError::Corrupt)?;
                corrupt(other_layout.channels.count == layout.channels.count)?;
            }
        }

        corrupt(channels == root.total_channels)?;

        for fixture in 0..root.fixtures.count as usize {
            let patch = show.patch(fixture).ok_or(ShowError::Corrupt)?;
            corrupt(patch.universe < root.universes.max(1))?;
        }

        for layout in show.iter(root.layouts) {
            show.list_ok(layout.channels)?;
            show.list_ok(layout.outputs)?;
            show.list_ok(layout.open_slots)?;
            let count = layout.channels.count as i32;

            for index in [
                layout.red,
                layout.green,
                layout.blue,
                layout.white,
                layout.dimmer,
                layout.white_pixel_dimmer,
                layout.virtual_dimmer,
                layout.reset,
                layout.lamp_on,
            ] {
                corrupt((index as i32) < count)?;
            }

            for output in show.iter(layout.outputs) {
                corrupt((output.chan as i32) < count && output.shift < 32)?;
            }

            for chan in show.iter(layout.channels) {
                show.list_ok(chan.caps)?;
                show.list_ok(chan.programs)?;
                show.list_ok(chan.sound_programs)?;
                corrupt(chan.no_function < chan.caps.count as i32)?;

                for program in show.iter(chan.programs).chain(show.iter(chan.sound_programs)) {
                    corrupt((program as u32) < chan.caps.count)?;
                }

                for cap in show.iter(chan.caps) {
                    show.list_ok(cap.turned)?;
                }
            }
        }

        let aim_sets = root.aims.count.checked_div(root.fixtures.count).unwrap_or(0);

        for preset in show.iter(root.presets) {
            show.list_ok(preset.steps)?;
            show.list_ok(preset.effects)?;
            show.list_ok(preset.parts)?;
            show.list_ok(preset.playable)?;
            show.list_ok(preset.stage_front)?;
            show.list_ok(preset.stage_top)?;
            corrupt(preset.steps.count > 0)?;
            corrupt(preset.stage_front.count == 0 || preset.stage_front.count == preset.parts.count)?;
            corrupt(preset.stage_top.count == 0 || preset.stage_top.count == preset.parts.count)?;

            let mut last = -1i64;

            for part in show.iter(preset.parts) {
                corrupt(part.part < root.parts.count && part.part as i64 > last)?;
                last = part.part as i64;
            }

            for playable in show.iter(preset.playable) {
                corrupt((playable as u32) < preset.steps.count)?;
            }

            for step in show.iter(preset.steps) {
                show.list_ok(step.caps)?;
                show.list_ok(step.chans)?;
                show.list_ok(step.amounts)?;
                corrupt(step.caps.count as usize <= crate::engine::MAX_STEP_CAPS)?;

                for value in show.iter(step.caps) {
                    show.list_ok(value.fan)?;
                    // (a show without fixtures aims nothing)
                    corrupt(value.flags & capv_flag::AIM == 0 || root.fixtures.count == 0 || value.aim < aim_sets)?;
                    corrupt(value.flags & capv_flag::FAN == 0 || value.fan.count > 0)?;
                }

                for amount in show.iter(step.amounts) {
                    corrupt(amount.effect < preset.effects.count)?;
                }
            }

            for effect in show.iter(preset.effects) {
                show.list_ok(effect.caps)?;
                show.list_ok(effect.chans)?;
                show.list_ok(effect.pixel)?;
                corrupt(effect.pixel.count <= 1)?;

                for pixel in show.iter(effect.pixel) {
                    show.list_ok(pixel.stops)?;
                    show.list_ok(pixel.bitmap)?;
                    corrupt(pixel.stops.count % 16 == 0)?;
                }
            }
        }

        for scene in show.iter(root.scenes) {
            show.list_ok(scene.presets)?;

            for preset in show.iter(scene.presets) {
                corrupt((preset as u32) < root.presets.count)?;
            }
        }

        for composition in show.iter(root.compositions) {
            show.list_ok(composition.clips)?;
            show.list_ok(composition.regions)?;
            show.list_ok(composition.holds)?;
            corrupt(composition.regions.count <= root.max_regions)?;

            for clip in show.iter(composition.clips) {
                corrupt(
                    (clip.preset as u32) < root.presets.count
                        && (clip.scene as u32) < root.scenes.count
                        && (clip.region as u32) < composition.regions.count,
                )?;
            }
        }

        corrupt(root.selected_composition < root.compositions.count as i32)?;
        corrupt(show.header.memory_words >= crate::engine::memory_words(&root))?;

        Ok(show)
    }
}

// ---- writing a show (the compiler) ----

/// Collects the lists of a show and puts it together.
#[cfg(feature = "writer")]
pub struct Writer {
    data: Vec<u8>,
    /// lists written already, by their bytes: the same list is written once
    known: alloc::collections::BTreeMap<Vec<u8>, u32>,
    base: u32,
}

#[cfg(feature = "writer")]
impl Writer {
    /// A writer for the data following a root record: the lists start right after it.
    pub fn new(chunk_offset: u32) -> Self {
        Writer {
            data: Vec::new(),
            known: alloc::collections::BTreeMap::new(),
            base: chunk_offset + Root::SIZE as u32,
        }
    }

    /// Write a list and return where it is. The same bytes are written once.
    pub fn list<T: Field>(&mut self, items: &[T]) -> List<T> {
        if items.is_empty() {
            return List::EMPTY;
        }

        let mut bytes = Vec::with_capacity(items.len() * T::SIZE);

        for item in items {
            item.write(&mut bytes);
        }

        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }

        if let Some(offset) = self.known.get(&bytes) {
            return List::new(*offset, items.len() as u32);
        }

        let offset = self.base + self.data.len() as u32;
        self.data.extend_from_slice(&bytes);
        self.known.insert(bytes, offset);
        List::new(offset, items.len() as u32)
    }

    /// Write a list on its own, even if the same bytes are in the show already: what it is found by is where it is.
    pub fn list_unshared<T: Field>(&mut self, items: &[T]) -> List<T> {
        if items.is_empty() {
            return List::EMPTY;
        }

        let offset = self.base + self.data.len() as u32;

        for item in items {
            item.write(&mut self.data);
        }

        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }

        List::new(offset, items.len() as u32)
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

/// Put a show together: the header, the chunk table, the show chunk (the root and the data of a `Writer` made with
/// `show_chunk_offset`), the patch and, if passed, the project.
#[cfg(feature = "writer")]
pub fn assemble(
    features: u32,
    memory_words: u32,
    root: &Root,
    data: &[u8],
    patch: &[Patch],
    project: Option<&[u8]>,
) -> Vec<u8> {
    let chunks = if project.is_some() { 3 } else { 2 };
    let show_offset = show_chunk_offset(project.is_some());
    let show_length = Root::SIZE + data.len();
    let patch_offset = show_offset as usize + show_length;
    let patch_length = patch.len() * Patch::SIZE;
    let project_offset = patch_offset + patch_length;
    let project_length = project.map(|bytes| bytes.len()).unwrap_or(0);
    let mut total = project_offset + project_length;

    while total % 4 != 0 {
        total += 1;
    }

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&MAGIC);
    FORMAT_MAJOR.write(&mut out);
    FORMAT_MINOR.write(&mut out);
    features.write(&mut out);
    BEHAVIOUR_VERSION.write(&mut out);
    (total as u32).write(&mut out);
    0u32.write(&mut out);
    (chunks as u32).write(&mut out);
    memory_words.write(&mut out);
    root.universes.write(&mut out);

    while out.len() < HEADER_SIZE {
        out.push(0);
    }

    out.extend_from_slice(&CHUNK_SHOW);
    show_offset.write(&mut out);
    (show_length as u32).write(&mut out);
    out.extend_from_slice(&CHUNK_PATCH);
    (patch_offset as u32).write(&mut out);
    (patch_length as u32).write(&mut out);

    if project.is_some() {
        out.extend_from_slice(&CHUNK_PROJECT);
        (project_offset as u32).write(&mut out);
        (project_length as u32).write(&mut out);
    }

    root.write(&mut out);
    out.extend_from_slice(data);

    for entry in patch {
        entry.write(&mut out);
    }

    if let Some(project) = project {
        out.extend_from_slice(project);
    }

    while out.len() < total {
        out.push(0);
    }

    let crc = checksum(&out);
    if let Some(slot) = out.get_mut(20..24) {
        slot.copy_from_slice(&crc.to_le_bytes());
    }
    out
}

/// Where the show chunk starts in a show with or without the project.
#[cfg(feature = "writer")]
pub const fn show_chunk_offset(with_project: bool) -> u32 {
    (HEADER_SIZE + if with_project { 3 } else { 2 } * CHUNK_ENTRY_SIZE) as u32
}
