//! The conformance cases of the pixel patterns (`conformance/cases/pixel-pattern.json`): a pattern at pixels and
//! moments, through a palette. The recorded values come from double precision.
//!
//! - The brightness and the color become 8 bit DMX values: they may differ by 1/255.
//! - The place in the palette is compared modulo 2 (see `engine_pixel::pattern`), where the palette is back where it
//!   was.
//! - A difference beyond that means a decision (a floor, a hash, a comparison) went the other way. Each of those is
//!   listed by its name in [`ALLOWED`], with the reason: no tolerance covers them.

use engine_pixel::encode::{encode_bitmap, encode_stops, BitmapSource};
use engine_pixel::{
    color_of, sample, Align, Between, Bitmap, Cycle, Direction, Fit, ImageColors, Pattern, PatternSettings,
    PixelContext, Repeat, Scroll, Shape, Stops, TextColors,
};
use serde_json::Value;

/// What a brightness or a part of a color may differ by: a step of a DMX channel.
const TOLERANCE: f64 = 1.0 / 255.0;

/// What a place in the palette may differ by, modulo 2. The color it gives is compared with [`TOLERANCE`]; this only
/// tells a place which is off from one which is rounded.
const PLACE_TOLERANCE: f64 = 2e-3;

/// The samples which differ from the recorded values by more than the tolerance, because single precision lands on
/// the other side of an edge than double precision does: (the name of the case, the index of the sample, why).
///
/// All of them are pixels of the wave of the audio (`scope`) of the case "scope 5", whose size of 0.957 spreads
/// 25 + floor(0.957 * 225) = 240 frames of the audio over the pixels. A pixel shows the level
/// `floor((1 - x) * 240)` frames ago, and for a pixel in the middle of column c of n columns, `(1 - x) * 240` is
/// `(2 (n - c) - 1) * 120 / n`: a whole number for the bars of 12 and 24 pixels. Neither a double nor a float holds
/// such an x exactly, so the product ends a hair above or below the whole number, in the two precisions on different
/// sides, and the floor is one frame (20 ms) apart. Without audio a triangle stands in for the level, which rises by
/// 2 / 240 = 0.00833 with each frame: that is the difference, a bit more than two steps of a DMX channel. With the
/// audio it is the level of the frame before or after.
///
/// The double is no more right than the float here: in sample 5 it falls below the whole number itself.
/// (Sample 13, a matrix of 3 columns, goes the other way as well, 39.99999999999999 against 40.000004, but its pixel
/// is dark either way.)
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "scope 5",
        1,
        "column 6 of 12, x = 0.541666..: (1 - x) * 240 is 110; the double gives 110.00000000000001 (110 frames ago), \
         the float 109.99999 (109 frames ago). Expected level 0.2986667, actual 0.2903334",
    ),
    (
        "scope 5",
        4,
        "column 9 of 24, x = 0.395833..: (1 - x) * 240 is 145; the double gives 145.00000000000003 (145 frames ago), \
         the float 144.99998 (144 frames ago). Expected level 0.7896667, actual 0.798",
    ),
    (
        "scope 5",
        5,
        "column 18 of 24, x = 0.770833..: (1 - x) * 240 is 55; the double gives 54.99999999999999 (54 frames ago), \
         the float 55.000004 (55 frames ago). Expected level 0.452, actual 0.46033335",
    ),
    (
        "scope 5",
        17,
        "column 20 of 24, x = 0.854166..: (1 - x) * 240 is 35; the double gives 35.00000000000001 (35 frames ago), \
         the float 34.999996 (34 frames ago). Expected level 0.7916667, actual 0.7833333",
    ),
];

fn text<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.get(key).and_then(Value::as_str)
}

// a number of the project, the default where it is missing or null
fn number(node: &Value, key: &str, default: f64) -> f64 {
    node.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn whole(node: &Value, key: &str) -> i32 {
    node.get(key).and_then(Value::as_i64).expect(key) as i32
}

// the bytes of the bitmap of the settings, the way the compiler makes them
fn bitmap_bytes(node: &Value) -> Option<Vec<u8>> {
    let bitmap = node.get("bitmap").filter(|bitmap| bitmap.is_object())?;
    let list = |key: &str| bitmap.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let frames = list("frames");
    let frames: Vec<Option<&str>> = frames.iter().map(Value::as_str).collect();
    let durations: Vec<Option<f64>> = list("durations").iter().map(Value::as_f64).collect();
    let letter_starts: Vec<Option<i32>> = list("letterStarts")
        .iter()
        .map(|start| start.as_i64().map(|start| start as i32))
        .collect();
    let integer = |key: &str| bitmap.get(key).and_then(Value::as_i64).map(|value| value as i32);

    Some(
        encode_bitmap(&BitmapSource {
            width: integer("width"),
            height: integer("height"),
            channels: integer("channels"),
            frames: &frames,
            durations: &durations,
            letter_starts: &letter_starts,
        })
        .expect("a bitmap the bytes can hold"),
    )
}

// the settings of a pattern, read like the ones of an effect
fn settings<'a>(node: &Value, bitmap: Option<Bitmap<'a>>) -> PatternSettings<'a> {
    PatternSettings {
        pattern: Pattern::from_name(text(node, "pattern")),
        size: number(node, "size", 0.25) as f32,
        softness: number(node, "softness", 0.3) as f32,
        direction: Direction::from_name(text(node, "direction")),
        shape: Shape::from_name(text(node, "shape")),
        bounce: node.get("bounce").and_then(Value::as_bool).unwrap_or(false),
        bitmap,
        scroll: Scroll::from_name(text(node, "scroll")),
        align: Align::from_name(text(node, "align")),
        offset_y: number(node, "offsetY", 0.0) as f32,
        fit: Fit::from_name(text(node, "fit")),
        text_colors: TextColors::from_name(text(node, "textColors")),
        image_colors: ImageColors::from_name(text(node, "imageColors")),
    }
}

fn context(pixel: &Value) -> PixelContext {
    PixelContext {
        x: number(pixel, "x", 0.5) as f32,
        y: number(pixel, "y", 0.5) as f32,
        column: whole(pixel, "column"),
        row: whole(pixel, "row"),
        columns: whole(pixel, "columns"),
        rows: whole(pixel, "rows"),
        aspect_x: number(pixel, "aspectX", 1.0) as f32,
        aspect_y: number(pixel, "aspectY", 1.0) as f32,
        seed: whole(pixel, "seed"),
    }
}

// how far two places in a palette are apart, modulo 2
fn place_distance(a: f64, b: f64) -> f64 {
    let distance = (a - b).rem_euclid(2.0);

    distance.min(2.0 - distance)
}

struct Deviation {
    case: String,
    sample: usize,
    what: &'static str,
    by: f64,
    detail: String,
}

#[test]
fn plays_the_conformance_cases_of_the_pixel_patterns() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../conformance/cases/pixel-pattern.json"
    );
    let file: Value = serde_json::from_str(&std::fs::read_to_string(path).expect(path)).expect("the cases as JSON");

    assert_eq!(file["kind"], "pixelPattern");

    let cases = file["cases"].as_array().expect("cases");
    let mut samples = 0;
    let mut largest_level = (0.0f64, String::new());
    let mut largest_color = (0.0f64, String::new());
    let mut largest_place = (0.0f64, String::new());
    let mut beyond: Vec<Deviation> = Vec::new();
    // the largest deviations of each pattern: of its level, its color and its place
    let mut by_pattern: std::collections::BTreeMap<String, [f64; 3]> = std::collections::BTreeMap::new();

    for case in cases {
        let name = case["name"].as_str().expect("name");
        let input = &case["input"];
        let bitmap_bytes = bitmap_bytes(&input["settings"]);
        let bitmap = bitmap_bytes
            .as_deref()
            .map(|bytes| Bitmap::parse(bytes).expect("the bytes of a bitmap"));
        let settings = settings(&input["settings"], bitmap);

        // the palette, its stops sorted once
        let palette = &input["palette"];
        let stops: Vec<(f64, &str)> = palette["stops"]
            .as_array()
            .map(|stops| {
                stops
                    .iter()
                    .map(|stop| (number(stop, "position", 0.0), text(stop, "color").unwrap_or("")))
                    .collect()
            })
            .unwrap_or_default();
        let stop_bytes = encode_stops(&stops);
        let stops = Stops::from_bytes(&stop_bytes).expect("whole stops");
        let between = Between::from_name(text(palette, "between"));
        let repeat = Repeat::from_name(text(palette, "repeat"));

        let inputs = input["samples"].as_array().expect("samples");
        let expected = case["expected"].as_array().expect("expected");

        assert_eq!(inputs.len(), expected.len(), "{name}");

        for (index, (input, expected)) in inputs.iter().zip(expected).enumerate() {
            let context = context(&input["pixel"]);
            let cycle = input["cycle"].as_f64().expect("cycle");
            let expected: Vec<f64> = expected
                .as_array()
                .expect("five numbers")
                .iter()
                .map(|value| value.as_f64().expect("a number"))
                .collect();
            let mut out = [0.0f32; 5];

            sample(&settings, Cycle::from_f64(cycle), &context, None, &mut out);

            // the color of the palette, or the own one of an image
            let color = if out[2] >= 0.0 {
                [out[2], out[3], out[4]]
            } else {
                color_of(&stops, between, repeat, out[0])
            };
            let detail = format!(
                "{name} [{index}] cycle {cycle} pixel {:?}\n    expected {expected:?}\n    actual   [{}, {}, {}, {}, {}]",
                input["pixel"].to_string(),
                out[0],
                out[1],
                color[0],
                color[1],
                color[2]
            );

            samples += 1;

            let place = place_distance(out[0] as f64, expected[0]);
            let level = (out[1] as f64 - expected[1]).abs();
            let color = (0..3)
                .map(|part| (color[part] as f64 - expected[2 + part]).abs())
                .fold(0.0, f64::max);

            if level > largest_level.0 && level <= TOLERANCE {
                largest_level = (level, detail.clone());
            }

            if color > largest_color.0 && color <= TOLERANCE {
                largest_color = (color, detail.clone());
            }

            if place > largest_place.0 && place <= PLACE_TOLERANCE {
                largest_place = (place, detail.clone());
            }

            let largest = by_pattern
                .entry(text(&case["input"]["settings"], "pattern").unwrap_or("").to_string())
                .or_default();

            for (index, (by, tolerance)) in [(level, TOLERANCE), (color, TOLERANCE), (place, PLACE_TOLERANCE)]
                .into_iter()
                .enumerate()
            {
                if by <= tolerance && by > largest[index] {
                    largest[index] = by;
                }
            }

            for (what, by, tolerance) in [
                ("level", level, TOLERANCE),
                ("color", color, TOLERANCE),
                ("place", place, PLACE_TOLERANCE),
            ] {
                if by > tolerance {
                    beyond.push(Deviation {
                        case: name.to_string(),
                        sample: index,
                        what,
                        by,
                        detail: detail.clone(),
                    });
                }
            }
        }
    }

    println!("{} cases, {samples} samples", cases.len());
    println!(
        "largest deviation of a level within the tolerance: {:e}\n  {}",
        largest_level.0, largest_level.1
    );
    println!(
        "largest deviation of a color within the tolerance: {:e}\n  {}",
        largest_color.0, largest_color.1
    );
    println!(
        "largest deviation of a place (modulo 2) within the tolerance: {:e}\n  {}",
        largest_place.0, largest_place.1
    );

    println!("largest deviations within the tolerance by pattern (level, color, place):");

    for (pattern, [level, color, place]) in &by_pattern {
        println!("  {pattern:10} {level:.2e} {color:.2e} {place:.2e}");
    }

    let mut unexplained = 0;

    for deviation in &beyond {
        let allowed = ALLOWED
            .iter()
            .find(|(case, sample, _)| *case == deviation.case && *sample == deviation.sample);

        match allowed {
            Some((_, _, why)) => println!(
                "allowed: {} by {:e}: {why}\n  {}",
                deviation.what, deviation.by, deviation.detail
            ),
            None => {
                unexplained += 1;
                println!(
                    "BEYOND THE TOLERANCE: {} by {:e}\n  {}",
                    deviation.what, deviation.by, deviation.detail
                );
            }
        }
    }

    // every allowed sample still differs: one which does not anymore has to leave the list
    for (case, sample, _) in ALLOWED {
        assert!(
            beyond
                .iter()
                .any(|deviation| deviation.case == *case && deviation.sample == *sample),
            "{case} [{sample}] is allowed to differ, but does not"
        );
    }

    assert_eq!(
        unexplained, 0,
        "{unexplained} samples differ by more than the tolerance"
    );
    assert!(samples > 4000, "only {samples} samples");
}
