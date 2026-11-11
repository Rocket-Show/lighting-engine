//! What a pixel effect sets for its pattern ([`PatternSettings`]), and where a pixel is ([`PixelContext`]).
//!
//! The project writes the choices as strings. Here each is a `u8` enum: `from_u8` reads the byte of the compiled show
//! (a value it does not know behaves like a name which is not known), `as_u8` writes it. With the `std` feature,
//! `from_name` reads the string of the project: a value missing, null or empty is the default of the effect, an
//! unknown one takes the default branch of the pattern.

use crate::bitmap::Bitmap;

macro_rules! names {
    (
        $(#[$meta:meta])*
        $name:ident, missing $missing:ident, unknown $unknown:ident, { $($variant:ident = $value:literal => $text:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $name {
            $($variant = $value),+
        }

        impl $name {
            /// The value of a byte of the compiled show. An unknown byte behaves like an unknown name of the project.
            pub const fn from_u8(value: u8) -> Self {
                match value {
                    $($value => Self::$variant,)+
                    _ => Self::$unknown,
                }
            }

            /// The byte of the compiled show.
            pub const fn as_u8(self) -> u8 {
                self as u8
            }

            /// The value of a string of the project, read like a project is read: missing, null or empty is the
            /// default of the effect, an unknown name what the pattern does with one.
            #[cfg(feature = "std")]
            pub fn from_name(name: Option<&str>) -> Self {
                match name {
                    None | Some("") => Self::$missing,
                    $(Some($text) => Self::$variant,)+
                    Some(_) => Self::$unknown,
                }
            }

            /// The string of the project.
            #[cfg(feature = "std")]
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }
    };
}

/// The pattern of a pixel effect (`pattern` of the effect, `"chase"` by default). A name this engine doesn't know
/// lights nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Pattern {
    Chase = 0,
    Comet = 1,
    Wipe = 2,
    Flow = 3,
    Sparkle = 4,
    Plasma = 5,
    Ripple = 6,
    Shapes = 7,
    Fire = 8,
    Center = 9,
    Stripes = 10,
    Stack = 11,
    Sweep = 12,
    Spiral = 13,
    Checker = 14,
    Rain = 15,
    Tunnel = 16,
    Orbit = 17,
    Wave = 18,
    Meter = 19,
    Scope = 20,
    Text = 21,
    Image = 22,
    /// A pattern this engine doesn't know: nothing is lit.
    Unknown = 255,
}

impl Pattern {
    /// The value of a byte of the compiled show. An unknown byte is [`Pattern::Unknown`].
    pub const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Chase,
            1 => Self::Comet,
            2 => Self::Wipe,
            3 => Self::Flow,
            4 => Self::Sparkle,
            5 => Self::Plasma,
            6 => Self::Ripple,
            7 => Self::Shapes,
            8 => Self::Fire,
            9 => Self::Center,
            10 => Self::Stripes,
            11 => Self::Stack,
            12 => Self::Sweep,
            13 => Self::Spiral,
            14 => Self::Checker,
            15 => Self::Rain,
            16 => Self::Tunnel,
            17 => Self::Orbit,
            18 => Self::Wave,
            19 => Self::Meter,
            20 => Self::Scope,
            21 => Self::Text,
            22 => Self::Image,
            _ => Self::Unknown,
        }
    }

    /// The byte of the compiled show.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// The pattern of a string of the project: missing, null or empty is the chase, an unknown name
    /// [`Pattern::Unknown`].
    #[cfg(feature = "std")]
    pub fn from_name(name: Option<&str>) -> Self {
        match name {
            None | Some("") | Some("chase") => Self::Chase,
            Some("comet") => Self::Comet,
            Some("wipe") => Self::Wipe,
            Some("flow") => Self::Flow,
            Some("sparkle") => Self::Sparkle,
            Some("plasma") => Self::Plasma,
            Some("ripple") => Self::Ripple,
            Some("shapes") => Self::Shapes,
            Some("fire") => Self::Fire,
            Some("center") => Self::Center,
            Some("stripes") => Self::Stripes,
            Some("stack") => Self::Stack,
            Some("sweep") => Self::Sweep,
            Some("spiral") => Self::Spiral,
            Some("checker") => Self::Checker,
            Some("rain") => Self::Rain,
            Some("tunnel") => Self::Tunnel,
            Some("orbit") => Self::Orbit,
            Some("wave") => Self::Wave,
            Some("meter") => Self::Meter,
            Some("scope") => Self::Scope,
            Some("text") => Self::Text,
            Some("image") => Self::Image,
            Some(_) => Self::Unknown,
        }
    }

    /// The string of the project, none for [`Pattern::Unknown`].
    #[cfg(feature = "std")]
    pub fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::Chase => "chase",
            Self::Comet => "comet",
            Self::Wipe => "wipe",
            Self::Flow => "flow",
            Self::Sparkle => "sparkle",
            Self::Plasma => "plasma",
            Self::Ripple => "ripple",
            Self::Shapes => "shapes",
            Self::Fire => "fire",
            Self::Center => "center",
            Self::Stripes => "stripes",
            Self::Stack => "stack",
            Self::Sweep => "sweep",
            Self::Spiral => "spiral",
            Self::Checker => "checker",
            Self::Rain => "rain",
            Self::Tunnel => "tunnel",
            Self::Orbit => "orbit",
            Self::Wave => "wave",
            Self::Meter => "meter",
            Self::Scope => "scope",
            Self::Text => "text",
            Self::Image => "image",
            Self::Unknown => return None,
        })
    }

    /// Whether the pattern shows the audio itself: the level meter and the wave of the audio. Any other pattern
    /// follows the audio with its brightness or its size, which the caller does.
    pub const fn shows_audio(self) -> bool {
        matches!(self, Self::Meter | Self::Scope)
    }

    /// Whether the pattern reads the picture of the effect (see [`Bitmap`]).
    pub const fn shows_bitmap(self) -> bool {
        matches!(self, Self::Text | Self::Image)
    }
}

names! {
    /// Where a pattern runs to (`right`, `left`, `up`, `down`, `out` from the middle, `in` to it), or which way it
    /// turns (`cw`, `ccw`). `"right"` by default. A pattern which does not know a direction takes it as `right`
    /// (or as clockwise).
    Direction, missing Right, unknown Right, {
        Right = 0 => "right",
        Left = 1 => "left",
        Up = 2 => "up",
        Down = 3 => "down",
        Out = 4 => "out",
        In = 5 => "in",
        Cw = 6 => "cw",
        Ccw = 7 => "ccw",
    }
}

names! {
    /// The shape of the shapes and the tunnel (`circle`, `square`, `diamond`, `star`, `heart`), the path of the orbit
    /// (`circle`, `figure8`, `line`, `wave`) and the curve of the wave (`sine`, `triangle`, `square`, `sawtooth`).
    /// `"circle"` by default. A pattern which does not know a shape takes its own default: the circle, or the sine.
    Shape, missing Circle, unknown Circle, {
        Circle = 0 => "circle",
        Square = 1 => "square",
        Diamond = 2 => "diamond",
        Star = 3 => "star",
        Heart = 4 => "heart",
        Figure8 = 5 => "figure8",
        Line = 6 => "line",
        Wave = 7 => "wave",
        Sine = 8 => "sine",
        Triangle = 9 => "triangle",
        Sawtooth = 10 => "sawtooth",
    }
}

names! {
    /// How a text or an image scrolls. `"auto"` by default: a text to the left, an image not at all (which is also
    /// what an unknown name does).
    Scroll, missing Auto, unknown Auto, {
        Auto = 0 => "auto",
        Left = 1 => "left",
        Right = 2 => "right",
        Up = 3 => "up",
        Down = 4 => "down",
        None = 5 => "none",
    }
}

names! {
    /// Where a text which does not scroll across is aligned. `"center"` by default.
    Align, missing Center, unknown Center, {
        Center = 0 => "center",
        Left = 1 => "left",
        Right = 2 => "right",
    }
}

names! {
    /// How an image is fitted into the pixels. `"contain"` by default.
    Fit, missing Contain, unknown Contain, {
        Contain = 0 => "contain",
        Cover = 1 => "cover",
        Stretch = 2 => "stretch",
        Pixels = 3 => "pixels",
    }
}

names! {
    /// The colors of a text: `along` it, of its `letters` or `one`. `"along"` by default.
    TextColors, missing Along, unknown Along, {
        Along = 0 => "along",
        Letters = 1 => "letters",
        One = 2 => "one",
    }
}

names! {
    /// The colors of an image: its `own`, or its brightness through the `palette`. `"own"` by default.
    ImageColors, missing Own, unknown Own, {
        Own = 0 => "own",
        Palette = 1 => "palette",
    }
}

names! {
    /// `between` of a palette: `smooth` blends from one color to the next, `steps` jumps. `"smooth"` by default.
    Between, missing Smooth, unknown Smooth, {
        Smooth = 0 => "smooth",
        Steps = 1 => "steps",
    }
}

names! {
    /// `repeat` of a palette: `mirror` runs it back and forth, `wrap` starts over. `"mirror"` by default.
    Repeat, missing Mirror, unknown Mirror, {
        Mirror = 0 => "mirror",
        Wrap = 1 => "wrap",
    }
}

names! {
    /// `blend` of an effect, what it does with the color the preset puts on a pixel: `replace` it, `mask` it (the
    /// pattern only sets how bright it is), `add` to it or take the `max` of both. `"replace"` by default.
    ///
    /// An unknown name is [`Blend::Other`]: it mixes like `replace`, but lets the effect follow the presets like
    /// every blend which is not `replace` (see [`Blend::follows_presets`]).
    Blend, missing Replace, unknown Other, {
        Replace = 0 => "replace",
        Mask = 1 => "mask",
        Add = 2 => "add",
        Max = 3 => "max",
        Other = 255 => "other",
    }
}

impl Blend {
    /// Whether the blend alone makes the effect follow the color the presets give a pixel (anything but `replace`;
    /// an effect with the palette of the preset follows them as well).
    pub const fn follows_presets(self) -> bool {
        !matches!(self, Self::Replace)
    }

    /// A color of a pixel the effect lights, blended with the one the preset puts on it: `base` the color of the
    /// preset, `lit` the light of the pattern and `level` how bright the pattern is, all between 0 and 1.
    pub fn mix(self, base: f32, lit: f32, level: f32) -> f32 {
        match self {
            Self::Mask => base * level,
            Self::Add => crate::math::minf(base + lit, 1.0),
            Self::Max => crate::math::maxf(base, lit),
            Self::Replace | Self::Other => lit,
        }
    }
}

names! {
    /// `audioSource` of an effect: none (`off`) or the audio of the `composition` playing. `"off"` by default.
    AudioSource, missing Off, unknown Off, {
        Off = 0 => "off",
        Composition = 1 => "composition",
    }
}

names! {
    /// `audioBand` of an effect, what of the audio it follows: all of it (`full`, the RMS), the `bass`, the `mid`s or
    /// the `high`s. `"full"` by default.
    AudioBand, missing Full, unknown Full, {
        Full = 0 => "full",
        Bass = 1 => "bass",
        Mid = 2 => "mid",
        High = 3 => "high",
    }
}

names! {
    /// `audioFollows` of an effect, what follows the audio in a pattern which does not show it: its `brightness` or
    /// its `size`. `"brightness"` by default.
    AudioFollows, missing Brightness, unknown Brightness, {
        Brightness = 0 => "brightness",
        Size = 1 => "size",
    }
}

names! {
    /// `spread` of an effect: the pattern over each `fixture` on its own, or one picture over the `stage`.
    /// `"fixture"` by default.
    Spread, missing Fixture, unknown Fixture, {
        Fixture = 0 => "fixture",
        Stage = 1 => "stage",
    }
}

names! {
    /// `view` of an effect spread over the stage: seen from the `front` or from the `top`. `"front"` by default.
    View, missing Front, unknown Front, {
        Front = 0 => "front",
        Top = 1 => "top",
    }
}

/// Where a pixel is: its place in the picture (x from the left to the right, y from the top to the bottom, both from 0
/// to 1, the middle of the pixel), the column and the row it is in and how many there are. A bar is a matrix with one
/// row. The aspect says how wide and high the picture is, the longer side being 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelContext {
    pub x: f32,
    pub y: f32,
    pub column: i32,
    pub row: i32,
    pub columns: i32,
    pub rows: i32,
    pub aspect_x: f32,
    pub aspect_y: f32,
    /// A whole number the random looking patterns start from (of the effect and the fixture).
    pub seed: i32,
}

impl PixelContext {
    /// A single pixel in the middle of a picture of one.
    pub const DEFAULT: PixelContext = PixelContext {
        x: 0.5,
        y: 0.5,
        column: 0,
        row: 0,
        columns: 1,
        rows: 1,
        aspect_x: 1.0,
        aspect_y: 1.0,
        seed: 0,
    };

    /// The pixel of an effect which is mirrored (`mirror` of the effect): the pattern runs out from the middle of
    /// each fixture to both of its ends.
    pub fn mirrored(self) -> PixelContext {
        PixelContext {
            x: if self.x < 0.5 {
                self.x * 2.0
            } else {
                (1.0 - self.x) * 2.0
            },
            ..self
        }
    }
}

impl Default for PixelContext {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What a pixel effect sets for its pattern (see [`crate::pattern::sample`]).
#[derive(Clone, Copy, Debug)]
pub struct PatternSettings<'a> {
    pub pattern: Pattern,
    /// How big the pattern is (a block, a tail, rings, ...), between 0 and 1. What it means depends on the pattern.
    pub size: f32,
    /// How soft its edges are, between 0 (hard) and 1.
    pub softness: f32,
    pub direction: Direction,
    pub shape: Shape,
    /// Whether a comet runs back and forth instead of starting over.
    pub bounce: bool,
    /// The picture of a text or an image.
    pub bitmap: Option<Bitmap<'a>>,
    pub scroll: Scroll,
    pub align: Align,
    /// How many pixels a text is moved up (cut off to a whole number).
    pub offset_y: f32,
    pub fit: Fit,
    pub text_colors: TextColors,
    pub image_colors: ImageColors,
}

impl PatternSettings<'_> {
    /// The defaults of a pixel effect: a chase of a quarter of the pixels to the right.
    pub const DEFAULT: PatternSettings<'static> = PatternSettings {
        pattern: Pattern::Chase,
        size: 0.25,
        softness: 0.3,
        direction: Direction::Right,
        shape: Shape::Circle,
        bounce: false,
        bitmap: None,
        scroll: Scroll::Auto,
        align: Align::Center,
        offset_y: 0.0,
        fit: Fit::Contain,
        text_colors: TextColors::Along,
        image_colors: ImageColors::Own,
    };
}

impl Default for PatternSettings<'_> {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_names_of_a_project() {
        assert_eq!(Pattern::from_name(None), Pattern::Chase);
        assert_eq!(Pattern::from_name(Some("")), Pattern::Chase);
        assert_eq!(Pattern::from_name(Some("scope")), Pattern::Scope);
        assert_eq!(Pattern::from_name(Some("fireworks")), Pattern::Unknown);
        assert_eq!(Direction::from_name(None), Direction::Right);
        assert_eq!(Direction::from_name(Some("ccw")), Direction::Ccw);
        assert_eq!(Direction::from_name(Some("sideways")), Direction::Right);
        assert_eq!(Shape::from_name(Some("sawtooth")), Shape::Sawtooth);
        assert_eq!(Shape::from_name(Some("blob")), Shape::Circle);
        assert_eq!(Scroll::from_name(None), Scroll::Auto);
        assert_eq!(Scroll::from_name(Some("none")), Scroll::None);
        assert_eq!(Scroll::from_name(Some("diagonal")), Scroll::Auto);
        assert_eq!(Align::from_name(Some("justify")), Align::Center);
        assert_eq!(Fit::from_name(Some("pixels")), Fit::Pixels);
        assert_eq!(TextColors::from_name(Some("letters")), TextColors::Letters);
        assert_eq!(ImageColors::from_name(Some("palette")), ImageColors::Palette);
        assert_eq!(Between::from_name(Some("steps")), Between::Steps);
        assert_eq!(Between::from_name(None), Between::Smooth);
        assert_eq!(Repeat::from_name(Some("wrap")), Repeat::Wrap);
        assert_eq!(Repeat::from_name(Some("bounce")), Repeat::Mirror);
        assert_eq!(Blend::from_name(None), Blend::Replace);
        assert_eq!(Blend::from_name(Some("screen")), Blend::Other);
        assert_eq!(AudioSource::from_name(Some("composition")), AudioSource::Composition);
        assert_eq!(AudioSource::from_name(Some("microphone")), AudioSource::Off);
        assert_eq!(AudioBand::from_name(Some("high")), AudioBand::High);
        assert_eq!(AudioFollows::from_name(Some("size")), AudioFollows::Size);
        assert_eq!(Spread::from_name(Some("stage")), Spread::Stage);
        assert_eq!(View::from_name(Some("top")), View::Top);
    }

    #[test]
    fn keeps_the_bytes() {
        for value in 0..=255u8 {
            let pattern = Pattern::from_u8(value);
            assert_eq!(pattern.as_u8(), if value <= 22 { value } else { 255 });
            assert_eq!(
                Pattern::from_name(pattern.name()),
                if pattern == Pattern::Unknown {
                    Pattern::Chase
                } else {
                    pattern
                }
            );

            let direction = Direction::from_u8(value);
            assert_eq!(direction.as_u8(), if value <= 7 { value } else { 0 });
            assert_eq!(Direction::from_name(Some(direction.name())), direction);

            let shape = Shape::from_u8(value);
            assert_eq!(shape.as_u8(), if value <= 10 { value } else { 0 });
            assert_eq!(Shape::from_name(Some(shape.name())), shape);

            assert_eq!(Scroll::from_u8(value).as_u8(), if value <= 5 { value } else { 0 });
            assert_eq!(Align::from_u8(value).as_u8(), if value <= 2 { value } else { 0 });
            assert_eq!(Fit::from_u8(value).as_u8(), if value <= 3 { value } else { 0 });
            assert_eq!(TextColors::from_u8(value).as_u8(), if value <= 2 { value } else { 0 });
            assert_eq!(ImageColors::from_u8(value).as_u8(), if value <= 1 { value } else { 0 });
            assert_eq!(Between::from_u8(value).as_u8(), if value <= 1 { value } else { 0 });
            assert_eq!(Repeat::from_u8(value).as_u8(), if value <= 1 { value } else { 0 });
            assert_eq!(Blend::from_u8(value).as_u8(), if value <= 3 { value } else { 255 });
            assert_eq!(AudioSource::from_u8(value).as_u8(), if value <= 1 { value } else { 0 });
            assert_eq!(AudioBand::from_u8(value).as_u8(), if value <= 3 { value } else { 0 });
            assert_eq!(AudioFollows::from_u8(value).as_u8(), if value <= 1 { value } else { 0 });
            assert_eq!(Spread::from_u8(value).as_u8(), if value <= 1 { value } else { 0 });
            assert_eq!(View::from_u8(value).as_u8(), if value <= 1 { value } else { 0 });
        }
    }

    #[test]
    fn mirrors_a_pixel() {
        let pixel = |x: f32| {
            PixelContext {
                x,
                ..PixelContext::DEFAULT
            }
            .mirrored()
            .x
        };

        assert_eq!(
            [pixel(0.0), pixel(0.25), pixel(0.5), pixel(0.75), pixel(1.0)],
            [0.0, 0.5, 1.0, 0.5, 0.0]
        );
    }

    #[test]
    fn says_which_patterns_show_the_audio() {
        assert!(Pattern::Meter.shows_audio());
        assert!(Pattern::Scope.shows_audio());
        assert!(!Pattern::Chase.shows_audio());
        assert!(!Pattern::Unknown.shows_audio());
    }

    #[test]
    fn blends_with_the_color_of_the_preset() {
        assert_eq!(Blend::Replace.mix(0.5, 0.25, 0.5), 0.25);
        assert_eq!(Blend::Other.mix(0.5, 0.25, 0.5), 0.25);
        assert_eq!(Blend::Mask.mix(0.5, 0.25, 0.5), 0.25);
        assert_eq!(Blend::Mask.mix(1.0, 0.25, 0.5), 0.5);
        assert_eq!(Blend::Add.mix(0.5, 0.25, 0.5), 0.75);
        assert_eq!(Blend::Add.mix(0.9, 0.25, 0.5), 1.0);
        assert_eq!(Blend::Max.mix(0.5, 0.25, 0.5), 0.5);
        assert!(!Blend::Replace.follows_presets());
        assert!(Blend::Other.follows_presets());
        assert!(Blend::Mask.follows_presets());
    }
}
