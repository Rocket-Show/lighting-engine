# Where the core differs from the recorded values

The core passes every `channelValues` case within ±1 (3,811 frames, 209,059 values), every `effectCurve`, `presetSteps`,
`presetIntensity` and `transitionCurve` case, and all but four samples of the `pixelPattern` cases. The differences
beyond that are listed here. All of them have the same cause: a value of the case sits exactly on an edge where the
recorded number is decided by the rounding of a double, so the engines the values were recorded from come to it by
chance themselves (they agree with each other because they round alike). The core rounds single-precision floats and
comes out on the other side. None of them is a difference a show could rely on.

## `channelValues`: 136 values in 5 cases

| File                     | Case                                                                  | Parts                                                    | Why                                                                                                                                                                                                                                                                                                     |
| ------------------------ | --------------------------------------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `...-show.json`          | white and amber from the colors, and the color temperature as a color | `rgbwa`, the channel `Green`                             | The green left over once the amber LED took its part is `0.4 - (0.4 / 0.75) * 0.75`: exactly 0. In doubles it comes to a hair below 0, which the recorded values take as "no value", leaving the channel alone; the core puts 0 on it. The universe is the same unless a preset below lights the green. |
| `...-pixel-effects.json` | `comet-2`                                                             | `matrix-groups/Top`, `matrix-groups/Corners` at 5000 ms  | The place in the palette is exactly 0.5, the border of the two stepped stops.                                                                                                                                                                                                                           |
| `...-pixel-effects.json` | `image-1`                                                             | `matrix/4`, `matrix/8`, `matrix/12` at 0 ms              | An image of 5 columns shown pixel for pixel and scrolled by half a period: the pixels are exactly on the border of two columns of the image.                                                                                                                                                            |
| `...-pixel-effects.json` | `meter-1`                                                             | the first and the last pixel of the bars of three pixels | The place in the palette is exactly 1/3, the border of two of the three stepped stops.                                                                                                                                                                                                                  |
| `...-pixel-effects.json` | `scope-1`                                                             | the first and the last pixel of the bars of three pixels | The pixel is exactly 42 frames of the wave back (a third of 126), the border of two frames.                                                                                                                                                                                                             |

The same cases at other parts and times, and the 294 other `channelValues` cases, are within ±1.

## `pixelPattern`: 4 samples of 4,610

The samples 1, 4, 5 and 17 of the case `scope 5`: the pixel is exactly a whole number of frames of the wave back (110,
145, 55 and 35), the border of two frames. See `crates/pixel/tests/pixel_pattern.rs`.

## What follows from them

An edge a float is floored on is a place where the targets of the core could differ as well, were they to round
differently. They don't: the native build and `engine.wasm` come to the same bits (`core-digests.json`), because the
core uses nothing but single-precision addition, subtraction, multiplication, division and square root, and one maths
library in software for the rest. The differences above are between doubles and floats, not between builds.
