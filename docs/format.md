# The compiled show format, version 1.0

A compiled show is a project with everything fixed already worked out, laid out so the engine reads it in place. The
compiler (`crates/compiler`) writes it, the core (`crates/core`) reads it. This document specifies the bytes;
`concept.md`, section 6, says why the format is the way it is.

Status: version 1.0, 11 November 2026. No device has shipped with it, so it may still change without a new major version
until the first firmware is released. The tables of the records below are written from `crates/core/src/format.rs` by
`tools/format-tables.py`, which CI checks.

## 1. Conventions

- **Little-endian** throughout. Floats are IEEE 754 single precision.
- **Offsets, not pointers.** A _list_ is 8 bytes: the offset of its first record from the start of the file (`u32`) and
  the number of records (`u32`). An empty list is offset 0, count 0. The records of a list follow each other without
  gaps; each list starts on a multiple of 4 bytes and is padded with zeros to one.
- **Records have a fixed size**, a multiple of 4 bytes, and are read field by field at the offsets given below. A field
  called `reserved` is written as 0 and not read.
- **Names are numbers.** A name the engine compares (a profile, a channel, a wheel, a blade) is a `u16` given by the
  compiler: 0 is no name (null in the project), 1 is the empty name, the others count up from 2. Equal names have equal
  numbers, across the whole show. The names themselves are not in the file.
- **Things are named by their index**: a fixture, a part, a preset, a scene, a composition by its place in its list. The
  compiler returns the table from the uuids of the project to these indices; it is not a part of the file.
- **Times are whole nanoseconds** in an `i64`. A time of the project (milliseconds, a double) is rounded to the nearest
  nanosecond once, by the compiler.
- **Beats, tempos and other factors of times are 32.32 fixed-point numbers** in an `i64` (the value times 2^32,
  rounded).
- **Flags** are bits of a `u32` (or `u16`), bit 0 the lowest. Bits not listed are 0.
- **Values of channels** are in the units of the channel: 0 to 255 for a channel of one byte, 0 to 65535 for one with a
  fine channel, with fractions.

## 2. Layout of the file

| Part        | Bytes                 | Holds                                       |
| ----------- | --------------------- | ------------------------------------------- |
| Header      | 48                    | see below                                   |
| Chunk table | 12 for each chunk     | the type, the offset and the length of each |
| Chunks      | as the table says     | `SHOW`, `PTCH` and, if it is there, `PROJ`  |
| Padding     | up to a multiple of 4 | zeros                                       |

### Header

| Offset | Type     | Holds                                                            |
| -----: | -------- | ---------------------------------------------------------------- |
|      0 | 4 bytes  | the magic number, the ASCII letters `RSLE`                       |
|      4 | `u16`    | the major version of the format (1)                              |
|      6 | `u16`    | the minor version of the format (0)                              |
|      8 | `u32`    | the feature bits the show needs (section 4)                      |
|     12 | `u32`    | the behaviour version of the compiler which wrote it (section 3) |
|     16 | `u32`    | the length of the whole file in bytes                            |
|     20 | `u32`    | the CRC-32 of the whole file, these four bytes taken as zero     |
|     24 | `u32`    | the number of chunks                                             |
|     28 | `u32`    | the working memory the show needs, in 32-bit words (section 6)   |
|     32 | `u32`    | the number of universes                                          |
|     36 | 12 bytes | reserved, zeros                                                  |

The CRC-32 is the one of zlib and PNG (polynomial 0xEDB88320 reflected, start and final XOR 0xFFFFFFFF).

### Chunk table

An entry is the type of a chunk (four ASCII letters), its offset from the start of the file (`u32`) and its length in
bytes (`u32`). A reader skips a chunk whose type it doesn't know.

| Type   | Needed | Holds                                                                                          |
| ------ | ------ | ---------------------------------------------------------------------------------------------- |
| `SHOW` | yes    | the [Root](#root) record at its start, followed by all the lists the root and its records name |
| `PTCH` | yes    | a [Patch](#patch) record for each fixture, in the order of the fixtures                        |
| `PROJ` | no     | the project JSON as it was compiled, which the engine never reads                              |

The patch is a chunk of its own so that a new address at the venue rewrites only this chunk (and the checksum). What the
compiler works out from the addresses stays as it was compiled: which parts are the same lamp (`same_lamp` of a
[Part](#part)) and how many steps a chase has. A patch written at the venue must not put two fixtures on one address
which were on two, or the other way round.

## 3. Versions

1. **The major version** changes when a reader of the old one could not safely skip what is new. A reader refuses a show
   of another major version.
2. **The minor version** counts additions a reader may skip: a new optional chunk, a new field in the reserved bytes of
   a record. A reader accepts any minor version.
3. **A feature bit for each new ability** (section 4). The compiler sets only the bits a show uses, and a reader refuses
   a show with a bit it doesn't know. This is what lets a newer compiler write a show an older engine plays.
4. **The behaviour version** (12 in the header, `BEHAVIOUR_VERSION` in the core) counts up with every change of the
   engine which changes the values it comes to. A host can tell from it whether two engines play a show alike.

## 4. Feature bits

| Bit | Name            | The show has                                      |
| --: | --------------- | ------------------------------------------------- |
|   0 | `STEPS`         | presets with more than one step                   |
|   1 | `STEP_ORDERS`   | steps played backward, back and forth or shuffled |
|   2 | `CURVES`        | effect curves                                     |
|   3 | `MOVEMENTS`     | movements of the pan and the tilt                 |
|   4 | `PIXELS`        | pixel effects                                     |
|   5 | `PIXEL_BITMAPS` | pixel effects showing a text or an image          |
|   6 | `AUDIO`         | pixel effects following the audio                 |
|   7 | `FANS`          | values or effects fanned out over the fixtures    |
|   8 | `AIMS`          | moving heads aimed at a spot on the stage         |
|   9 | `TIMELINES`     | compositions with clips                           |
|  10 | `BEATS`         | steps or curves following the beat                |

## 5. What a reader checks

`Show::check` validates a show once, before it plays, and refuses it with the reason: no magic number, another major
version, feature bits it doesn't know, a length other than the header says, a wrong checksum, a missing `SHOW` or `PTCH`
chunk, or a damaged show. A show is damaged if a list reaches outside the file or doesn't start on a multiple of 4, or
if a number naming something else names nothing: the fixture, the layout and the lamp of a part, the universe of a
fixture, a channel of a layout, a capability of a channel, a step or an effect of a preset, a part of a preset (which
are ordered by their number), a preset of a scene, a preset, a scene and a clip of a timeline, a spot the heads are
aimed at. It also checks the limits of section 7 and that the working memory of the header is enough.

A show which passes can't send a read outside of itself. The engine still reads every record through a bounds check, so
even a show changed after its check plays wrong values at worst.

## 6. Working memory

The engine keeps its state in memory the caller hands over: `memory_words` 32-bit words (28 in the header). The core
works the number out from the counts of the [Root](#root) (`engine::memory_words`):

| Words                               | For                                                    |
| ----------------------------------- | ------------------------------------------------------ |
| 2 × `total_channels`                | the value and the state of each channel of each part   |
| 2 × `max_part_channels`             | the values of a part before the effects following them |
| the number of fixtures              | identify, reset and lamp on                            |
| 2 × the number of scenes            | selected, and the dimmer it is given by hand           |
| 18 × the number of scenes           | picked by hand and flashed                             |
| 3 × `max_regions`                   | the beat each clip started on                          |
| 4 × the larger of `max_holds` and 8 | the holds the timeline went through                    |
| 18 × `max_layers`                   | the presets playing in a frame                         |
| 3 × `max_following`                 | the effects following the presets on a fixture         |
| 144 × `universes`                   | the channels held by the host                          |
| 21 × `audio_history`                | the frames of audio the effects look back over         |

## 7. Limits

| Limit                                                             |  Value |
| ----------------------------------------------------------------- | -----: |
| values for capabilities in a step                                 |     28 |
| steps a preset plays in a pass in another order than written down |    256 |
| channels of a part                                                | 32,767 |
| presets, scenes, clips of a composition                           | 65,535 |
| names                                                             | 65,534 |
| the address of a channel from the first one of its fixture        | 65,535 |

The compiler refuses a project beyond them.

## 8. Numbers with a meaning

**Types of capabilities** (`ty`), numbered in the order the compiler lists them in: 0 NoFunction, 1 ShutterStrobe, 2
StrobeSpeed, 3 StrobeDuration, 4 Intensity, 5 ColorIntensity, 6 ColorPreset, 7 ColorTemperature, 8 Pan, 9 PanContinuous,
10 Tilt, 11 TiltContinuous, 12 PanTiltSpeed, 13 WheelSlot, 14 WheelShake, 15 WheelSlotRotation, 16 WheelRotation, 17
EffectSpeed, 18 EffectDuration, 19 EffectParameter, 20 SoundSensitivity, 21 Focus, 22 Zoom, 23 Iris, 24 IrisEffect, 25
Frost, 26 FrostEffect, 27 Prism, 28 PrismRotation, 29 BladeInsertion, 30 BladeRotation, 31 BladeSystemRotation, 32 Fog,
33 FogOutput, 34 FogType, 35 BeamAngle, 36 Rotation, 37 Speed, 38 Time, 39 Maintenance, 40 Generic, 41 Effect, 42
BeamPosition; 255 none or unknown.

**Colors** (`color`): 0 none, 1 Red, 2 Green, 3 Blue, 4 Cyan, 5 Magenta, 6 Yellow, 7 Amber, 8 White, 9 Warm White, 10
Cold White, 11 UV, 12 Lime, 13 Indigo.

**What a shutter does** (`shutter`): 0 nothing said, 1 Open, 2 Closed, 3 Strobe, 4 Pulse, 5 RampUp, 6 RampDown, 7
RampUpDown, 8 Lightning, 9 Spikes, 10 Burst; of a value of a step also 11 Random and 12 Sound; 255 anything else.

**Maintenance** (`maintenance`): 0 reset, 1 lamp on, 2 lamp off, 255 none.

**Shapes of a fade or a transition** (`curve` of a step, `fade_in_curve`, `fade_out_curve`): 0 linear, 1 ease-in, 2
ease-out, 3 ease-in-out, 4 snap.

**Order of the steps** (`order` of a preset): 0 forward, 1 backward, 2 pingpong, 3 random.

**Effects**: `kind` 1 curve, 2 movement, 3 pixel effect. `curve` 0 sine, 1 square, 2 triangle, 3 sawtooth, 4
reverse-sawtooth, 255 none. `phasing_mode` 0 a time, 1 beats, 2 spread over the fixtures. `run_mode` 0 infinite, 1 a
number of cycles, 2 a duration. `fan_mode` 0 none, 1 linear, 2 center, 3 side. `shape` of a movement 0 circle, 1
figure8, 2 line, 3 wave.

**Pixel effects** (the numbers of `crates/pixel`): `pattern` 0 chase, 1 comet, 2 wipe, 3 flow, 4 sparkle, 5 plasma, 6
ripple, 7 shapes, 8 fire, 9 center, 10 stripes, 11 stack, 12 sweep, 13 spiral, 14 checker, 15 rain, 16 tunnel, 17 orbit,
18 wave, 19 meter, 20 scope, 21 text, 22 image, 255 unknown (lights nothing). `direction` 0 right, 1 left, 2 up, 3 down,
4 out, 5 in, 6 cw, 7 ccw. `shape` 0 circle, 1 square, 2 diamond, 3 star, 4 heart, 5 figure8, 6 line, 7 wave, 8 sine, 9
triangle, 10 sawtooth. `scroll` 0 auto, 1 left, 2 right, 3 up, 4 down, 5 none. `align` 0 center, 1 left, 2 right. `fit`
0 contain, 1 cover, 2 stretch, 3 pixels. `text_colors` 0 along, 1 letters, 2 one. `image_colors` 0 own, 1 palette.
`between` 0 smooth, 1 steps. `repeat` 0 mirror, 1 wrap. `blend` 0 replace, 1 mask, 2 add, 3 max, 255 another one (mixed
like replace, but following the presets). `audio_band` 0 full, 1 bass, 2 mid, 3 high.

### Flags

| Record                      | Bit | Name                  | Says                                                                              |
| --------------------------- | --: | --------------------- | --------------------------------------------------------------------------------- |
| [Part](#part)               |   0 | `PIXEL`               | a pixel of its fixture (not the fixture itself)                                   |
| [Layout](#layout)           |   0 | `COLOR_WHEEL`         | one of its channels turns a color wheel                                           |
|                             |   1 | `COLOR_TEMPERATURE`   | one of its channels sets the color temperature                                    |
| [Chan](#chan)               |   0 | `DEFAULT`             | it starts out with `default`                                                      |
|                             |   1 | `COLOR_WHEEL`         | it turns a color wheel                                                            |
|                             |   2 | `PAN`                 | it turns the fixture over `pan_range` degrees                                     |
|                             |   3 | `TILT`                | it turns the fixture over `tilt_range` degrees                                    |
|                             | 4–6 | `MAINTENANCE`         | it resets the fixture (4), switches its lamp on (5) or off (6) at the value given |
| [Cap](#cap)                 |   0 | `RANGE`               | it has a range of exactly two values (`low`, `high`, and `center` in its middle)  |
|                             |   1 | `RANGE_OR_MORE`       | it has a range of at least two values                                             |
|                             |   2 | `REVERSED`            | its range runs from its high end to its low end (from wide to narrow)             |
|                             |   3 | `NOTHING`             | it puts out nothing over all of its range (a fog machine switched off)            |
|                             |   4 | `RANDOM`              | it flashes at random                                                              |
|                             |   5 | `SOUND`               | it follows the sound                                                              |
|                             |   6 | `STILL`               | it has no speed (a prism standing still)                                          |
|                             |   7 | `ROTATION_ENDS`       | it turns at speeds from `rotation_start` to `rotation_end` (clockwise above 0)    |
|                             |   8 | `ANGLES`              | it turns to angles from `angle_start` to `angle_end`                              |
|                             |   9 | `SLOT`                | it is the slot `slot` of a wheel                                                  |
|                             |  10 | `WHEEL_IS_COLOR`      | its wheel is a color wheel                                                        |
|                             |  11 | `SLOT_COLOR`          | its slot has the color `slot_red`, `slot_green`, `slot_blue` (0 to 255)           |
|                             |  12 | `PRESET_COLOR`        | it is a color preset of the color `preset_red`, `preset_green`, `preset_blue`     |
|                             |  13 | `SPEED`               | it is a program with a speed                                                      |
| [Preset](#preset)           |   0 | `MIRROR_PAN`          | its pan is mirrored                                                               |
|                             |   1 | `MIRROR_TILT`         | its tilt is mirrored                                                              |
|                             |   2 | `EMITTERS_FROM_COLOR` | the white and the amber part of its colors play on their LEDs                     |
|                             |   3 | `LOOP`                | its steps start over                                                              |
|                             |   4 | `PHASED`              | its steps chase over its fixtures (`phasing` of a [PresetPart](#presetpart))      |
|                             |   5 | `BEATS`               | its steps follow the beat, written at `steps_bpm`                                 |
|                             |   6 | `START`               | it starts `start` after its clip does                                             |
|                             |   7 | `END`                 | it ends `end` after its clip starts                                               |
|                             |   8 | `FADE_IN_PRE`         | it fades in before its start                                                      |
|                             |   9 | `FADE_OUT_POST`       | it fades out after its end                                                        |
| [PresetPart](#presetpart)   |   0 | `FAN_PLACE`           | it can be told apart from the others in the fan, at `fan_place` (0 to 1)          |
| [Step](#step)               |   0 | `TRANSITION`          | it travels from the step before over `transition` (else over all of its length)   |
| [CapV](#capv)               |   0 | `PERCENTAGE`          | it has a `percentage`                                                             |
|                             |   1 | `SLOT`                | it names the slot `slot`                                                          |
|                             |   2 | `DEGREES`             | it has `degrees`                                                                  |
|                             |   3 | `SPREAD`              | the fixtures are fanned out over `spread` degrees                                 |
|                             |   4 | `AIM`                 | it aims the fixtures at the spot `aim`                                            |
|                             |   5 | `PROGRAM`             | it names the program `program`                                                    |
|                             |   6 | `SOUND`               | the program runs to the sound                                                     |
|                             |   7 | `FAN`                 | it is fanned out over the fixtures by the stops of `fan`                          |
|                             |   8 | `FAN_STEPS`           | the fan jumps from stop to stop                                                   |
|                             |   9 | `FAN_CENTER`          | the spread runs from the middle out to both ends                                  |
| [Effect](#effect)           |   0 | `BEAT_SYNCED`         | its period is `length_beats` (and its phase `phase_beats`) at the tempo playing   |
|                             |   1 | `PAN_TILT_IN_DEGREES` | a curve on the pan or the tilt turns every head by the same degrees               |
|                             |   2 | `END_BASE`            | once it has run it doesn't apply anymore (else it holds its last value)           |
|                             |   3 | `INTENSITY`           | it plays on the dimmer                                                            |
| [PixelFx](#pixelfx)         |   0 | `BOUNCE`              | a comet runs back and forth                                                       |
|                             |   1 | `MIRROR`              | the pattern runs out from the middle of each fixture                              |
|                             |   2 | `WHITE_LED`           | the white part of the light plays on the white LED                                |
|                             |   3 | `STILL`               | the pattern stands where its phase and the chase put it                           |
|                             |   4 | `STAGE`               | one picture is spread over the stage (else over each fixture)                     |
|                             |   5 | `VIEW_TOP`            | the stage is seen from above (else from the front)                                |
|                             |   6 | `AUDIO`               | it follows the audio of the composition                                           |
|                             |   7 | `FOLLOWS_SIZE`        | it follows the audio with its size (else with its brightness)                     |
|                             |   8 | `PRESET_PALETTE`      | its palette is the shades of the color the presets put on a pixel                 |
| [Scene](#scene)             |   0 | `FADE_IN_PRE`         | it fades in before its clip                                                       |
|                             |   1 | `FADE_OUT_POST`       | it fades out after its clip                                                       |
| [Composition](#composition) |   0 | `SYNC_NONE`           | it follows no audio or video, so it may follow a tempo from outside               |
| [Clip](#clip)               |   0 | `PREVIEW`             | the preview plays it too (its track is neither hidden nor left out by a solo)     |

### The stops of a palette

`stops` of a [PixelFx](#pixelfx) is a list of bytes, 16 for each stop: its place (`f32`) and its red, green and blue
(`f32` each, 0 to 1). The stops are ordered by their place; stops on the same place stay in the order of the project.

### The picture of a text or an image

`bitmap` of a [PixelFx](#pixelfx) is a list of bytes (empty for an effect without a picture):

|       Offset | Type      | Holds                                                                          |
| -----------: | --------- | ------------------------------------------------------------------------------ |
|            0 | `i32`     | the width in pixels                                                            |
|            4 | `i32`     | the height in pixels                                                           |
|            8 | `u32`     | the bytes of a pixel: 1 (its brightness) or 3 (red, green and blue)            |
|           12 | `u32`     | F, the number of frames                                                        |
|           16 | `u32`     | L, the number of letters                                                       |
|           20 | `u32`     | B, the bytes kept of each frame                                                |
|           24 | F × `f32` | when each frame ends, in ms from the start of the first one                    |
|      24 + 4F | L × `i32` | the column each letter of a text starts at, ascending                          |
| 24 + 4F + 4L | F × S     | the frames row by row, each B bytes padded with zeros to S (B rounded up to 4) |

## 9. How the records play

- **Parts.** A fixture has a part for itself, if its mode has channels of its own, and a part for each of its pixels (or
  pixel groups), in this order. The parts of all fixtures follow each other in the order of the fixtures. The values of
  a frame are the values of the channels of all parts, one after the other: `value_base` of a part is where its values
  start. A part with `same_lamp` takes the values of that part (it has the same channels).
- **Layouts.** Parts with the same channels at the same places share a layout. An [Out](#out) sends a byte of a channel:
  the whole part of its value shifted right by `shift` bits, at the address of the fixture plus `address`.
- **Presets.** `parts` of a preset are its parts ordered by their number, each with its place in the chase (`index` of
  `count`), in the chase of whole fixtures (`whole_index` of `whole_count` of the preset, for the pixel effects) and in
  the fan. `stage_front` and `stage_top` are empty or have a [StagePlace](#stageplace) for each part of `parts`.
- **Steps.** A step lasts `length` as it is written down; the steps of a preset are in the order they are written down
  in. `playable` of a preset with another order is the steps a pass plays, in the order of the pass (for a random order:
  the steps lasting any time, which the engine shuffles for each pass).
- **Aims.** `aims` of the root has, for each spot the heads are aimed at, an [Aim](#aim) for each fixture: the spot
  `aim` of a [CapV](#capv) points the fixture `f` as the record `aim × fixtures + f` says.
- **Timelines.** `clips` of a composition are the presets of its clips in the order they are mixed in, the ones at the
  bottom first: the tracks from the last one to the first, the clips of a track by their start, the presets of a scene
  from the last one to the first. `play_start` and `play_end` include the fades reaching outside the clip.
- **Scenes.** `presets` of a scene are its presets, the topmost first, each once.

## 10. The records

<!-- records:start -->

<!-- prettier-ignore-start -->

### Root

Where everything is. 96 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `master_dimmer` | `f32` |  |
| 4 | `selected_composition` | `i32` | the composition selected in the project, whose tempo a preview follows (-1: none) |
| 8 | `fixtures` | list of [Fixture](#fixture) |  |
| 16 | `parts` | list of [Part](#part) |  |
| 24 | `layouts` | list of [Layout](#layout) |  |
| 32 | `presets` | list of [Preset](#preset) |  |
| 40 | `scenes` | list of [Scene](#scene) |  |
| 48 | `compositions` | list of [Composition](#composition) |  |
| 56 | `aims` | list of [Aim](#aim) | for each spot the moving heads are aimed at, where it points each fixture |
| 64 | `total_channels` | `u32` | the channels of all parts |
| 68 | `max_part_channels` | `u32` | the most channels a part has |
| 72 | `max_layers` | `u32` | the most presets playing at once |
| 76 | `max_following` | `u32` | the most effects following the presets on a fixture at once |
| 80 | `max_regions` | `u32` | the most clips a composition has |
| 84 | `max_holds` | `u32` | the most hold points a composition has |
| 88 | `universes` | `u32` |  |
| 92 | `audio_history` | `u32` | the frames of audio the pixel effects look back over (0 for a show which follows none) |

### Fixture

A fixture of the project. 4 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `profile` | `u16` | the name of its profile |
| 2 | `reserved` | `u16` |  |

### Patch

Where a fixture is patched (in the patch chunk, one for each fixture). 12 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `universe` | `u32` |  |
| 4 | `address` | `i32` | its first channel in the universe, from 0 |
| 8 | `brightness` | `f32` | how bright it lights at most, a master dimmer of its own |

### Part

A part of a fixture with channels of its own: the fixture itself, or one of its pixels. 20 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `fixture` | `u32` |  |
| 4 | `layout` | `u32` |  |
| 8 | `same_lamp` | `i32` | the part before it on the same address and pixel, which it takes the values of (-1: none) |
| 12 | `value_base` | `u32` | where its values start among the values of all parts |
| 16 | `flags` | `u32` |  |

### Place

Where a part of a fixture is in the picture a pixel effect paints. 32 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `x` | `f32` |  |
| 4 | `y` | `f32` |  |
| 8 | `aspect_x` | `f32` |  |
| 12 | `aspect_y` | `f32` |  |
| 16 | `column` | `i32` |  |
| 20 | `row` | `i32` |  |
| 24 | `columns` | `i32` |  |
| 28 | `rows` | `i32` |  |

### StagePlace

The place of a part of a preset in the picture of the stage, if it has one. 36 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `known` | `u32` |  |
| 4 | `place` | [Place](#place) |  |

### Layout

The channels of a part and what is worked out from them once. 92 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `channels` | list of [Chan](#chan) |  |
| 8 | `outputs` | list of [Out](#out) |  |
| 16 | `open_slots` | list of [OpenSlot](#openslot) | the open slots of its color wheels, which identifying it turns to |
| 24 | `red` | `i16` | the first channel lighting in red, green, blue and white (-1: none) |
| 26 | `green` | `i16` |  |
| 28 | `blue` | `i16` |  |
| 30 | `white` | `i16` |  |
| 32 | `dimmer` | `i16` | the first channel with a dimmer |
| 34 | `white_pixel_dimmer` | `i16` | the dimmer of a pixel lighting in white only |
| 36 | `virtual_dimmer` | `i16` |  |
| 38 | `reset` | `i16` | the channel resetting the fixture and the one switching its lamp on, with the value doing it |
| 40 | `lamp_on` | `i16` |  |
| 42 | `reserved` | `i16` |  |
| 44 | `reset_value` | `f32` |  |
| 48 | `lamp_on_value` | `f32` |  |
| 52 | `colors` | `u32` | the colors it has a channel for, a bit for each |
| 56 | `flags` | `u32` |  |
| 60 | `place` | [Place](#place) |  |

### Chan

A channel of a part. 72 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `name` | `u16` |  |
| 2 | `fine` | `u16` | the number of its fine channels |
| 4 | `flags` | `u32` |  |
| 8 | `types_low` | `u32` | the types of its capabilities, a bit for each (the types 0 to 31, and 32 and above) |
| 12 | `types_high` | `u32` |  |
| 16 | `max` | `f32` |  |
| 20 | `default` | `f32` |  |
| 24 | `pan_range` | `f32` | the degrees it turns the fixture over |
| 28 | `tilt_range` | `f32` |  |
| 32 | `maintenance_reset` | `f32` |  |
| 36 | `maintenance_lamp_on` | `f32` |  |
| 40 | `maintenance_lamp_off` | `f32` |  |
| 44 | `no_function` | `i32` | its first capability without a function (-1: none) |
| 48 | `caps` | list of [Cap](#cap) |  |
| 56 | `programs` | list of `u16` | its programs, in the order of their values: the ones not to the sound and the ones to it |
| 64 | `sound_programs` | list of `u16` |  |

### Cap

A capability of a channel. 88 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `ty` | `u8` |  |
| 1 | `color` | `u8` |  |
| 2 | `shutter` | `u8` |  |
| 3 | `reserved` | `u8` |  |
| 4 | `flags` | `u32` |  |
| 8 | `low` | `f32` |  |
| 12 | `high` | `f32` |  |
| 16 | `center` | `f32` |  |
| 20 | `brightness_start` | `f32` |  |
| 24 | `brightness_end` | `f32` |  |
| 28 | `rotation_start` | `f32` |  |
| 32 | `rotation_end` | `f32` |  |
| 36 | `angle_start` | `f32` |  |
| 40 | `angle_end` | `f32` |  |
| 44 | `slot` | `f32` |  |
| 48 | `slot_red` | `f32` | the color of its slot of a color wheel |
| 52 | `slot_green` | `f32` |  |
| 56 | `slot_blue` | `f32` |  |
| 60 | `preset_red` | `f32` | the color of a color preset |
| 64 | `preset_green` | `f32` |  |
| 68 | `preset_blue` | `f32` |  |
| 72 | `wheel` | `u16` | the wheel of its slot, the wheel it shakes and its blade |
| 74 | `shake_wheel` | `u16` |  |
| 76 | `blade` | `u16` |  |
| 78 | `reserved2` | `u16` |  |
| 80 | `turned` | list of `u16` | the wheels it turns |

### Out

Where a channel of a part is sent: its address from the first one of the fixture, and the byte of the value. 8 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `address` | `u16` |  |
| 2 | `chan` | `u16` |  |
| 4 | `shift` | `u32` | the bits the value is shifted by for this byte |

### OpenSlot

An open slot of a color wheel. 8 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `wheel` | `u16` |  |
| 2 | `reserved` | `u16` |  |
| 4 | `slot` | `f32` |  |

### Aim

Where a spot points a fixture, in degrees from the middle of its ranges. 12 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `known` | `u32` |  |
| 4 | `pan` | `f32` |  |
| 8 | `tilt` | `f32` |  |

### Preset

120 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `flags` | `u32` |  |
| 4 | `order` | `u8` |  |
| 5 | `fade_in_curve` | `u8` |  |
| 6 | `fade_out_curve` | `u8` |  |
| 7 | `reserved` | `u8` |  |
| 8 | `start` | `i64` |  |
| 16 | `end` | `i64` |  |
| 24 | `fade_in` | `i64` |  |
| 32 | `fade_out` | `i64` |  |
| 40 | `loop_length` | `i64` | the length of a pass through the steps as they are written down, and in the order they are played in |
| 48 | `pass_length` | `i64` |  |
| 56 | `steps_bpm` | `i64` | the tempo steps following the beat are written at |
| 64 | `seed` | `i32` |  |
| 68 | `whole_count` | `u32` | the number of fixtures a pixel effect chases over |
| 72 | `steps` | list of [Step](#step) |  |
| 80 | `effects` | list of [Effect](#effect) |  |
| 88 | `parts` | list of [PresetPart](#presetpart) | its parts, ordered by their number |
| 96 | `playable` | list of `u16` | the steps lasting any time, in the order they are played in (shuffled anew for each pass by a random order) |
| 104 | `stage_front` | list of [StagePlace](#stageplace) | the places of its parts in the picture of the stage, seen from the front and from above (or none) |
| 112 | `stage_top` | list of [StagePlace](#stageplace) |  |

### PresetPart

A part of a preset: where it is in its chase and in its fan. 24 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `part` | `u32` |  |
| 4 | `index` | `u16` |  |
| 6 | `count` | `u16` |  |
| 8 | `whole_index` | `u16` |  |
| 10 | `flags` | `u16` |  |
| 12 | `fan_place` | `f32` |  |
| 16 | `phasing` | `i64` | how far behind its steps run |

### Step

56 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `start` | `i64` |  |
| 8 | `transition` | `i64` |  |
| 16 | `length` | `i64` | how long it lasts as it is written down |
| 24 | `flags` | `u32` |  |
| 28 | `curve` | `u32` |  |
| 32 | `caps` | list of [CapV](#capv) |  |
| 40 | `chans` | list of [ChanV](#chanv) |  |
| 48 | `amounts` | list of [Amount](#amount) |  |

### CapV

A value of a step for a capability. 48 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `ty` | `u8` |  |
| 1 | `color` | `u8` |  |
| 2 | `shutter` | `u8` |  |
| 3 | `maintenance` | `u8` |  |
| 4 | `flags` | `u32` |  |
| 8 | `profile` | `u16` |  |
| 10 | `wheel` | `u16` |  |
| 12 | `blade` | `u16` |  |
| 14 | `reserved` | `u16` |  |
| 16 | `percentage` | `f32` |  |
| 20 | `slot` | `f32` |  |
| 24 | `degrees` | `f32` |  |
| 28 | `spread` | `f32` |  |
| 32 | `program` | `i32` |  |
| 36 | `aim` | `u32` |  |
| 40 | `fan` | list of [FanStop](#fanstop) | the stops of its fan, ordered by their place |

### FanStop

A stop of a fan: a value at a place along it (0 to 1). 8 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `position` | `f32` |  |
| 4 | `value` | `f32` |  |

### ChanV

A value of a step for a channel. 8 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `profile` | `u16` |  |
| 2 | `name` | `u16` |  |
| 4 | `value` | `f32` |  |

### Amount

How far a step opens an effect of its preset. 8 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `effect` | `u32` |  |
| 4 | `amount` | `f32` |  |

### Effect

An effect of a preset: a curve, a movement or a pixel effect, which both run on the timing of a curve. 128 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `kind` | `u8` |  |
| 1 | `curve` | `u8` |  |
| 2 | `phasing_mode` | `u8` |  |
| 3 | `run_mode` | `u8` |  |
| 4 | `fan_mode` | `u8` |  |
| 5 | `shape` | `u8` |  |
| 6 | `reserved` | `u16` |  |
| 8 | `flags` | `u32` |  |
| 12 | `length` | `i64` |  |
| 20 | `phase` | `i64` |  |
| 28 | `phasing` | `i64` |  |
| 36 | `run_duration` | `i64` |  |
| 44 | `length_beats` | `i64` |  |
| 52 | `phase_beats` | `i64` |  |
| 60 | `phasing_beats` | `i64` |  |
| 68 | `phasing_cycles` | `i64` |  |
| 76 | `group_size` | `u32` |  |
| 80 | `run_cycles` | `u32` |  |
| 84 | `amplitude` | `f32` |  |
| 88 | `position` | `f32` |  |
| 92 | `duty` | `f32` |  |
| 96 | `size_pan` | `f32` |  |
| 100 | `size_tilt` | `f32` |  |
| 104 | `caps` | list of [CurveCap](#curvecap) |  |
| 112 | `chans` | list of [ChanV](#chanv) |  |
| 120 | `pixel` | list of [PixelFx](#pixelfx) | what a pixel effect paints (one, or none) |

### CurveCap

A capability a curve plays on. 4 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `ty` | `u8` |  |
| 1 | `color` | `u8` |  |
| 2 | `reserved` | `u16` |  |

### PixelFx

What a pixel effect paints. The numbers of the pattern and its settings are the ones of the pixel crate. 68 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `flags` | `u32` |  |
| 4 | `pattern` | `u8` |  |
| 5 | `direction` | `u8` |  |
| 6 | `shape` | `u8` |  |
| 7 | `scroll` | `u8` |  |
| 8 | `align` | `u8` |  |
| 9 | `fit` | `u8` |  |
| 10 | `text_colors` | `u8` |  |
| 11 | `image_colors` | `u8` |  |
| 12 | `between` | `u8` |  |
| 13 | `repeat` | `u8` |  |
| 14 | `blend` | `u8` |  |
| 15 | `audio_band` | `u8` |  |
| 16 | `size` | `f32` |  |
| 20 | `softness` | `f32` |  |
| 24 | `offset_y` | `f32` |  |
| 28 | `audio_gain` | `f32` |  |
| 32 | `audio_floor` | `f32` |  |
| 36 | `audio_attack` | `f32` |  |
| 40 | `audio_release` | `f32` |  |
| 44 | `seed` | `i64` | the seed times 7919, as a 32.32 fixed-point number |
| 52 | `stops` | list of `u8` | the stops of its palette, 16 bytes each |
| 60 | `bitmap` | list of `u8` | its picture, in the layout of the pixel crate |

### Scene

36 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `dimmer` | `f32` |  |
| 4 | `flags` | `u32` |  |
| 8 | `fade_in` | `i64` |  |
| 16 | `fade_out` | `i64` |  |
| 24 | `fade_in_curve` | `u8` |  |
| 25 | `fade_out_curve` | `u8` |  |
| 26 | `reserved` | `u16` |  |
| 28 | `presets` | list of `u16` | its presets, the topmost first |

### Composition

40 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `bpm` | `i64` | its tempo, in 32.32 fixed-point beats per minute |
| 8 | `flags` | `u32` |  |
| 12 | `beats_per_bar` | `u32` |  |
| 16 | `clips` | list of [Clip](#clip) | the presets of its clips in the order they are mixed in, the ones at the bottom first |
| 24 | `regions` | list of [Region](#region) |  |
| 32 | `holds` | list of `i64` | its hold points, ordered |

### Clip

A preset of a scene on a clip of a timeline, with when it plays (its fades included). 24 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `play_start` | `i64` |  |
| 8 | `play_end` | `i64` |  |
| 16 | `preset` | `u16` |  |
| 18 | `scene` | `u16` |  |
| 20 | `region` | `u16` |  |
| 22 | `flags` | `u16` |  |

### Region

A clip of a timeline: when it starts and when it ends. 16 bytes.

| Offset | Field | Type | Holds |
| -----: | ----- | ---- | ----- |
| 0 | `start` | `i64` | when it starts, in nanoseconds of the timeline |
| 8 | `end` | `i64` | when it ends, in nanoseconds of the timeline |

<!-- prettier-ignore-end -->

<!-- records:end -->
