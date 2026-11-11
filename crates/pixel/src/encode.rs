//! What the compiler does once at load time, so that it is not done for every frame: the colors of a palette from
//! their `#rrggbb`, its stops sorted, the frames of a bitmap decoded from base 64. Only with the `std` feature: this
//! is not part of the core on the chip.
//!
//! The numbers of the project are doubles. They are worked on as doubles here and rounded to single precision when
//! they are written.

use crate::bitmap::HEADER_LEN;
use crate::palette::STOP_LEN;

/// A color like `#ff8000` as red, green and blue between 0 and 1. None for anything else, which a palette takes as
/// black.
pub fn hex_to_rgb(hex: &str) -> Option<[f64; 3]> {
    let digits = hex.strip_prefix('#')?.as_bytes();

    if digits.len() != 6 {
        return None;
    }

    let digit = |index: usize| -> Option<u32> { (*digits.get(index)? as char).to_digit(16) };
    let component = |index: usize| -> Option<f64> { Some((digit(index)? * 16 + digit(index + 1)?) as f64 / 255.0) };

    Some([component(0)?, component(2)?, component(4)?])
}

/// Red, green and blue between 0 and 1 as a color like `#ff8000`, rounded to the nearest of the 256 steps.
pub fn rgb_to_hex(color: [f64; 3]) -> String {
    let mut hex = String::from("#");

    for component in color {
        // (half a step goes up, what is not a number is 0)
        let step = if component.is_nan() {
            0
        } else {
            (component.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
        };

        hex.push_str(&format!("{step:02x}"));
    }

    hex
}

// the order of two numbers: what is not a number is above every number, -0 below 0
fn compare(a: f64, b: f64) -> core::cmp::Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => core::cmp::Ordering::Equal,
        (true, false) => core::cmp::Ordering::Greater,
        (false, true) => core::cmp::Ordering::Less,
        (false, false) => a.total_cmp(&b),
    }
}

/// The stops of a palette of the project (the `position` and the `color` of each, a color which is `null` as `""`)
/// as the bytes [`Stops::from_bytes`](crate::palette::Stops::from_bytes) reads: sorted by their position, the ones on
/// the same position in the order they are listed in, each color as red, green and blue, black for one which is no
/// `#rrggbb`.
pub fn encode_stops(stops: &[(f64, &str)]) -> Vec<u8> {
    let mut sorted: Vec<&(f64, &str)> = stops.iter().collect();
    // (a stable sort)
    sorted.sort_by(|a, b| compare(a.0, b.0));

    let mut bytes = Vec::with_capacity(sorted.len() * STOP_LEN);

    for (position, color) in sorted {
        let color = hex_to_rgb(color).unwrap_or([0.0, 0.0, 0.0]);

        bytes.extend((*position as f32).to_le_bytes());

        for component in color {
            bytes.extend((component as f32).to_le_bytes());
        }
    }

    bytes
}

/// The bytes of a text in base 64: the alphabet with `+` and `/`, no line breaks or blanks, the `=` at the end may be
/// left out (but not half of them). None for a text which is no base 64 (such a frame of a bitmap has no bytes).
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let source = text.as_bytes();
    let mut bytes = Vec::with_capacity(source.len() / 4 * 3 + 3);
    let mut bits: u32 = 0;
    let mut shift: i32 = 18;
    let mut position = 0;

    while let Some(&character) = source.get(position) {
        position += 1;

        if character == b'=' {
            // "="    a group which starts with it: wrong
            // "xx="  the second one is missing: wrong
            // "xx=y" something else follows: wrong
            if shift == 18 {
                return None;
            }

            if shift == 6 {
                if source.get(position) != Some(&b'=') {
                    return None;
                }

                position += 1;
            }

            break;
        }

        let value = match character {
            b'A'..=b'Z' => character - b'A',
            b'a'..=b'z' => character - b'a' + 26,
            b'0'..=b'9' => character - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };

        bits |= (value as u32) << shift;
        shift -= 6;

        if shift < 0 {
            bytes.extend([(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            bits = 0;
            shift = 18;
        }
    }

    match shift {
        // two characters: a byte
        6 => bytes.push((bits >> 16) as u8),
        // three characters: two bytes
        0 => bytes.extend([(bits >> 16) as u8, (bits >> 8) as u8]),
        // a single character holds no byte
        12 => return None,
        _ => {}
    }

    // nothing may follow the end
    if position < source.len() {
        return None;
    }

    Some(bytes)
}

/// Bytes as a text in base 64 (with its `=` at the end), like a project holds the frames of a bitmap.
pub fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for group in bytes.chunks(3) {
        let bits = group
            .iter()
            .enumerate()
            .fold(0u32, |bits, (index, byte)| bits | (*byte as u32) << (16 - 8 * index));

        for character in 0..4 {
            if character <= group.len() {
                text.push(
                    ALPHABET
                        .get((bits >> (18 - 6 * character)) as usize & 63)
                        .copied()
                        .unwrap_or(b'=') as char,
                );
            } else {
                text.push('=');
            }
        }
    }

    text
}

/// The values of a bitmap of the project (`bitmap` of a pixel effect): a value which is missing or `null` is `None`
/// (an empty list for a list).
#[derive(Clone, Copy, Debug, Default)]
pub struct BitmapSource<'a> {
    /// `width`, 0 by default.
    pub width: Option<i32>,
    /// `height`, 0 by default.
    pub height: Option<i32>,
    /// `channels`: 3 for red, green and blue, anything else is 1.
    pub channels: Option<i32>,
    /// `frames`, each the bytes of its pixels in base 64. A frame which is `null` or no base 64 has no bytes.
    pub frames: &'a [Option<&'a str>],
    /// `durations`, how long each frame shows in ms. A frame without one, with `null` or with nothing above 0 lasts
    /// 100 ms.
    pub durations: &'a [Option<f64>],
    /// `letterStarts`, the column each letter of a text starts at. The ones which are `null` do not count.
    pub letter_starts: &'a [Option<i32>],
}

/// The bitmap of a project as the bytes [`Bitmap::parse`](crate::bitmap::Bitmap::parse) reads (see the layout in
/// [`crate::bitmap`]): the frames decoded, the durations added up to the ends of the frames, the starts of the
/// letters sorted.
///
/// None for a bitmap the bytes can not hold: more than 2^32 frames, letters or bytes in a frame, or durations which
/// add up to more than a float holds.
pub fn encode_bitmap(source: &BitmapSource<'_>) -> Option<Vec<u8>> {
    let width = source.width.unwrap_or(0);
    let height = source.height.unwrap_or(0);
    let channels: u32 = if source.channels == Some(3) { 3 } else { 1 };

    let frames: Vec<Vec<u8>> = source
        .frames
        .iter()
        .map(|frame| frame.and_then(decode_base64).unwrap_or_default())
        .collect();

    // no pixel reads a byte beyond the picture
    let visible = if width > 0 && height > 0 {
        width as u64 * height as u64 * channels as u64
    } else {
        0
    };
    let longest = frames.iter().map(|frame| frame.len() as u64).max().unwrap_or(0);
    let frame_len = u32::try_from(longest.min(visible)).ok()?;
    let stride = (frame_len as usize).checked_add(3)? & !3;

    let mut letters: Vec<i32> = source.letter_starts.iter().flatten().copied().collect();
    letters.sort_unstable();

    let mut bytes = Vec::with_capacity(HEADER_LEN + frames.len() * (4 + stride) + letters.len() * 4);

    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(u32::try_from(frames.len()).ok()?.to_le_bytes());
    bytes.extend(u32::try_from(letters.len()).ok()?.to_le_bytes());
    bytes.extend(frame_len.to_le_bytes());

    // when each frame ends
    let mut end = 0.0f64;

    for index in 0..frames.len() {
        end += match source.durations.get(index) {
            Some(Some(duration)) if *duration > 0.0 => *duration,
            _ => 100.0,
        };

        let rounded = end as f32;

        if !rounded.is_finite() {
            return None;
        }

        bytes.extend(rounded.to_le_bytes());
    }

    for letter in letters {
        bytes.extend(letter.to_le_bytes());
    }

    for frame in &frames {
        let kept = frame.len().min(frame_len as usize);
        let start = bytes.len();

        bytes.extend(frame.iter().take(kept));
        bytes.resize(start + stride, 0);
    }

    Some(bytes)
}

/// The seed of the random looking patterns for a fixture: the `seed` of the effect (any number, not only a whole
/// one) and the index of the fixture in the preset (which does not count for an effect spread over the stage), cut to
/// a whole number of 32 bits. Worked out in doubles: it is the input of a hash.
pub fn seed_of(effect_seed: f64, fixture_index: i32, stage: bool) -> i32 {
    let fixture = if stage {
        0.0
    } else {
        fixture_index.wrapping_mul(131) as f64
    };

    // cut off towards zero and wrapped around at 32 bits
    (effect_seed * 7919.0 + fixture) as i64 as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitmap::Bitmap;
    use crate::palette::{color_of, Stop, Stops};
    use crate::settings::{Between, Repeat};

    #[test]
    fn reads_a_color() {
        assert_eq!(hex_to_rgb("#ff8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(hex_to_rgb("#FFE14A"), Some([1.0, 225.0 / 255.0, 74.0 / 255.0]));
        assert_eq!(hex_to_rgb("#000000"), Some([0.0, 0.0, 0.0]));

        for no_color in [
            "",
            "#",
            "ff8000",
            "#ff800",
            "#ff80000",
            "#ff800g",
            "#+f8000",
            "#ff8000\n",
            " #ff8000",
            "#ffé00",
            "red",
        ] {
            assert_eq!(hex_to_rgb(no_color), None, "{no_color:?}");
        }
    }

    #[test]
    fn writes_a_color() {
        assert_eq!(rgb_to_hex([1.0, 0.5, 0.0]), "#ff8000");
        assert_eq!(rgb_to_hex([1.0, 0.875, 0.75]), "#ffdfbf");
        assert_eq!(rgb_to_hex([2.0, -1.0, f64::NAN]), "#ff0000");
    }

    #[test]
    fn sorts_the_stops_of_a_palette_once() {
        // the palette of the conformance cases which is not sorted and has two stops on a place
        let bytes = encode_stops(&[
            (0.8, "#FFE14A"),
            (0.2, "#00d85a"),
            (0.5, "#ff2b2b"),
            (0.5, "#2c6bff"),
            (f64::NAN, "nothing"),
        ]);
        let stops = Stops::from_bytes(&bytes).unwrap();

        assert_eq!(stops.len(), 5);
        assert_eq!(
            stops.get(0),
            Some(Stop {
                position: 0.2,
                color: [0.0, 216.0 / 255.0, 90.0 / 255.0]
            })
        );
        assert_eq!(
            stops.get(1),
            Some(Stop {
                position: 0.5,
                color: [1.0, 43.0 / 255.0, 43.0 / 255.0]
            })
        );
        assert_eq!(
            stops.get(2),
            Some(Stop {
                position: 0.5,
                color: [44.0 / 255.0, 107.0 / 255.0, 1.0]
            })
        );
        assert_eq!(
            stops.get(3),
            Some(Stop {
                position: 0.8,
                color: [1.0, 225.0 / 255.0, 74.0 / 255.0]
            })
        );
        assert!(stops.get(4).unwrap().position.is_nan());
        assert_eq!(stops.get(4).unwrap().color, [0.0, 0.0, 0.0]);

        // like the stops in memory, sorted without a heap
        let mut unsorted = [
            Stop {
                position: 0.8,
                color: [1.0, 225.0 / 255.0, 74.0 / 255.0],
            },
            Stop {
                position: 0.2,
                color: [0.0, 216.0 / 255.0, 90.0 / 255.0],
            },
            Stop {
                position: 0.5,
                color: [1.0, 43.0 / 255.0, 43.0 / 255.0],
            },
            Stop {
                position: 0.5,
                color: [44.0 / 255.0, 107.0 / 255.0, 1.0],
            },
        ];
        crate::palette::sort_stops(&mut unsorted);
        let bytes = encode_stops(&[(0.8, "#FFE14A"), (0.2, "#00d85a"), (0.5, "#ff2b2b"), (0.5, "#2c6bff")]);
        let stops = Stops::from_bytes(&bytes).unwrap();

        for place in [0.0f32, 0.1, 0.3, 0.5, 0.6, 0.9, 1.3, 1.9] {
            assert_eq!(
                color_of(&stops, Between::Smooth, Repeat::Mirror, place),
                color_of(&Stops::from_slice(&unsorted), Between::Smooth, Repeat::Mirror, place)
            );
        }

        assert!(encode_stops(&[]).is_empty());
    }

    #[test]
    fn decodes_base_64() {
        assert_eq!(decode_base64(""), Some(vec![]));
        assert_eq!(decode_base64("ChQeKDI="), Some(vec![10, 20, 30, 40, 50]));
        assert_eq!(decode_base64("/wAAAA=="), Some(vec![255, 0, 0, 0]));
        assert_eq!(decode_base64("/4AACg=="), Some(vec![255, 128, 0, 10]));
        assert_eq!(decode_base64("AA=="), Some(vec![0]));
        assert_eq!(decode_base64("+/+/"), Some(vec![0xfb, 0xff, 0xbf]));
        // the = at the end left out
        assert_eq!(decode_base64("AA"), Some(vec![0]));
        assert_eq!(decode_base64("ChQeKDI"), Some(vec![10, 20, 30, 40, 50]));
        assert_eq!(decode_base64("AAA"), Some(vec![0, 0]));
        // bits which are left over do not matter
        assert_eq!(decode_base64("AB=="), Some(vec![0]));

        // what is no base 64
        for wrong in [
            "!!!",
            "A",
            "AAAAA",
            "A===",
            "AA=",
            "AA=A",
            "AA==A",
            "AAA=A",
            "=",
            "====",
            "AAAA=",
            "AA A",
            "AAAA\n",
            "ChQe KDI=",
            "-_-_",
            "ÄÄÄÄ",
        ] {
            assert_eq!(decode_base64(wrong), None, "{wrong:?}");
        }

        // there and back
        let bytes: Vec<u8> = (0..=255).collect();

        for length in 0..bytes.len() {
            assert_eq!(
                decode_base64(&encode_base64(&bytes[..length])).as_deref(),
                Some(&bytes[..length])
            );
        }

        assert_eq!(encode_base64(&[10, 20, 30, 40, 50]), "ChQeKDI=");
    }

    #[test]
    fn decodes_each_frame_and_none_which_can_not_be_read() {
        // the bytes 10, 20, 30, 40 and 50, a frame which is no base 64 and one which is missing
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(2),
            height: Some(2),
            frames: &[Some("ChQeKDI="), Some("!!!"), None],
            ..BitmapSource::default()
        })
        .unwrap();
        let bitmap = Bitmap::parse(&bytes).unwrap();

        // (of the five bytes, the four of the pixels are kept)
        assert_eq!(bitmap.frame(0), &[10, 20, 30, 40]);
        assert_eq!(bitmap.frame(1), &[0, 0, 0, 0]);
        assert_eq!(bitmap.frame(2), &[0, 0, 0, 0]);
        assert_eq!(bitmap.frame(7).len(), 0);
        assert_eq!(bitmap.channels(), 1);
        assert_eq!(bitmap.value(bitmap.frame(0), 1, 1, 0), 40);
        assert_eq!(bitmap.value(bitmap.frame(1), 1, 1, 0), 0);
        assert_eq!(bytes.len(), 24 + 3 * 4 + 3 * 4);
    }

    #[test]
    fn writes_the_layout_it_says() {
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(2),
            height: Some(1),
            channels: Some(3),
            frames: &[Some("/wAAAA=="), Some("AQIDBAUGBwg=")],
            durations: &[Some(40.0)],
            letter_starts: &[Some(6), None, Some(0)],
        })
        .unwrap();

        #[rustfmt::skip]
        let expected: &[u8] = &[
            2, 0, 0, 0, // width
            1, 0, 0, 0, // height
            3, 0, 0, 0, // channels
            2, 0, 0, 0, // frames
            2, 0, 0, 0, // letters
            6, 0, 0, 0, // bytes of a frame: 2 x 1 pixels of 3 bytes
            0, 0, 0x20, 0x42, // the first frame ends after 40 ms
            0, 0, 0x0c, 0x43, // the second one after 140 ms
            0, 0, 0, 0, // the letters, sorted
            6, 0, 0, 0,
            255, 0, 0, 0, 0, 0, 0, 0, // the first frame: four bytes, filled up to six, and up to a multiple of four
            1, 2, 3, 4, 5, 6, 0, 0, // the second one: six of its eight bytes
        ];

        assert_eq!(bytes, expected);

        let bitmap = Bitmap::parse(&bytes).unwrap();
        assert_eq!((bitmap.width(), bitmap.height(), bitmap.channels()), (2, 1, 3));
        assert_eq!(bitmap.letters_up_to(5), 1);
        assert_eq!(bitmap.frame_at(0.5), 1);
    }

    #[test]
    fn shows_each_frame_as_long_as_it_lasts() {
        // 100 ms, 300 ms and the default of 100 ms for the one without a length
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(1),
            height: Some(1),
            frames: &[Some("AA=="), Some("AA=="), Some("AA==")],
            durations: &[Some(100.0), Some(300.0)],
            ..BitmapSource::default()
        })
        .unwrap();
        let bitmap = Bitmap::parse(&bytes).unwrap();

        assert_eq!(bitmap.frame_at(0.0), 0);
        assert_eq!(bitmap.frame_at(0.19), 0);
        assert_eq!(bitmap.frame_at(0.21), 1);
        assert_eq!(bitmap.frame_at(0.79), 1);
        assert_eq!(bitmap.frame_at(0.81), 2);
        assert_eq!(bitmap.frame_at(1.0), 2);

        // a length which is null, nothing or below: 100 ms
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(1),
            height: Some(1),
            frames: &[Some("AA=="), Some("AA=="), Some("AA=="), Some("AA==")],
            durations: &[None, Some(0.0), Some(-5.0), Some(f64::NAN)],
            ..BitmapSource::default()
        })
        .unwrap();
        let bitmap = Bitmap::parse(&bytes).unwrap();

        assert_eq!(
            (0..4).map(|frame| bitmap.frame_end(frame)).collect::<Vec<_>>(),
            [100.0, 200.0, 300.0, 400.0]
        );
    }

    #[test]
    fn keeps_the_defaults_of_what_is_missing() {
        let bytes = encode_bitmap(&BitmapSource::default()).unwrap();
        let bitmap = Bitmap::parse(&bytes).unwrap();

        assert_eq!(bytes.len(), 24);
        assert_eq!(
            (bitmap.width(), bitmap.height(), bitmap.channels(), bitmap.frame_count()),
            (0, 0, 1, 0)
        );
        assert_eq!(bitmap.frame_at(0.7), 0);

        // other channels than 3 are 1
        let bytes = encode_bitmap(&BitmapSource {
            channels: Some(4),
            ..BitmapSource::default()
        })
        .unwrap();
        assert_eq!(Bitmap::parse(&bytes).unwrap().channels(), 1);

        // a picture of no size keeps no bytes, a huge one only the bytes there are
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(0),
            height: Some(5),
            frames: &[Some("AAAA")],
            ..BitmapSource::default()
        })
        .unwrap();
        assert_eq!(bytes.len(), 24 + 4);
        let bytes = encode_bitmap(&BitmapSource {
            width: Some(i32::MAX),
            height: Some(i32::MAX),
            frames: &[Some("AAAA")],
            ..BitmapSource::default()
        })
        .unwrap();
        assert_eq!(bytes.len(), 24 + 4 + 4);
        assert!(Bitmap::parse(&bytes).is_some());

        // durations no float holds
        assert!(encode_bitmap(&BitmapSource {
            frames: &[Some("AA==")],
            durations: &[Some(1e300)],
            ..BitmapSource::default()
        })
        .is_none());
    }

    #[test]
    fn works_out_the_seed_of_a_fixture() {
        assert_eq!(seed_of(1.0, 0, false), 7919);
        assert_eq!(seed_of(1.0, 2, false), 7919 + 262);
        assert_eq!(seed_of(1.0, 2, true), 7919);
        assert_eq!(seed_of(0.5, 1, false), 4090);
        assert_eq!(seed_of(-0.0001, 1, false), 130);
        // wrapped around at 32 bits
        assert_eq!(seed_of(1e6, 0, false), (7_919_000_000i64 as i32));
    }
}
