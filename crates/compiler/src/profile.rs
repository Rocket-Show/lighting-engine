//! What the Java engine works out of the fixture profiles when a project is loaded, in `f64` and
//! with the same operations in the same order.
//!
//! A port of the profile helpers of `DefaultDesignerService` (the numbers and the default values
//! of the fixture library, the colors of the wheels, the pixels of a matrix and the order the
//! channels are repeated for them in, the brightness of a capability), of
//! `DefaultFixturePixelGroupResolveService` (the pixels of a group), of the texts
//! `CapabilityPositions` ranks and of the comments `Maintenance` finds its capabilities by. The
//! regular expressions of Java are matched by hand (see [`regex_find`] for the ones a profile
//! brings along itself).

use std::cell::Cell;
use std::cmp::Ordering;

use crate::model::{
    java_trim, Fixture, FixtureCapability, FixtureChannel, FixtureMatrix, FixtureMatrixPixelGroup,
    FixtureMatrixPixelGroupConstraints, FixtureMode, FixtureModeChannel, FixtureProfile, FixtureWheel,
    FixtureWheelSlot,
};

// ---------------------------------------------------------------------------------------------
// Numbers and default values (DefaultDesignerService)
// ---------------------------------------------------------------------------------------------

// How long the number a text starts with is, like `[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?` takes
// it: None, if it doesn't start with one.
fn scan_number(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let digits = |mut index: usize| {
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        index
    };
    let mut index = 0;
    if let Some(b'+' | b'-') = bytes.first() {
        index += 1;
    }
    let whole_end = digits(index);
    if whole_end > index {
        index = whole_end;
        if bytes.get(index) == Some(&b'.') {
            index = digits(index + 1);
        }
    } else {
        if bytes.get(index) != Some(&b'.') {
            return None;
        }
        let fraction_end = digits(index + 1);
        if fraction_end == index + 1 {
            return None;
        }
        index = fraction_end;
    }
    if let Some(b'e' | b'E') = bytes.get(index) {
        let mut exponent = index + 1;
        if let Some(b'+' | b'-') = bytes.get(exponent) {
            exponent += 1;
        }
        let exponent_end = digits(exponent);
        if exponent_end > exponent {
            index = exponent_end;
        }
    }
    Some(index)
}

/// A number written in a fixture profile (e.g. "12.5", surrounding spaces allowed), or None if the
/// text is none (`parseNumber`): neither "1d" and "NaN" nor "0x10" are one.
pub fn parse_number(text: &str) -> Option<f64> {
    let trimmed = java_trim(text);
    if scan_number(trimmed)? != trimmed.len() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

/// The highest value of a channel with all of its fine channels (`getMaxValueByChannel`).
pub fn get_max_value_by_channel(channel: &FixtureChannel) -> f64 {
    256f64.powi(1 + channel.fine_channel_aliases.len() as i32) - 1.0
}

/// The value a channel starts out with, or None for none: a DMX value or a percentage of the
/// channel (`getDefaultValueByChannel`). One which can't be read is none.
pub fn get_default_value_by_channel(channel: &FixtureChannel) -> Option<f64> {
    let default_value = channel.default_value.as_deref()?;
    if let Some(percentage) = default_value.strip_suffix('%') {
        let percentage = parse_number(percentage)?;
        return Some(get_max_value_by_channel(channel) / 100.0 * percentage);
    }
    // a DMX value, of a profile writing it in fewer bits than the channel has as well
    let value = parse_number(default_value)?;
    Some(value * channel.resolution_multiplier())
}

// ---------------------------------------------------------------------------------------------
// Colors and wheels (DefaultDesignerService)
// ---------------------------------------------------------------------------------------------

/// A color, red, green and blue from 0 to 255 (`Color`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

/// A color like "#ff8000", also without the "#" or in short ("#f80"). None for anything else
/// (`hexToRgb`).
pub fn hex_to_rgb(hex: &str) -> Option<Color> {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |index: usize| (digits.as_bytes()[index] as char).to_digit(16).unwrap_or(0) as f64;
    match digits.len() {
        3 => Some(Color {
            red: digit(0) * 17.0,
            green: digit(1) * 17.0,
            blue: digit(2) * 17.0,
        }),
        6 => Some(Color {
            red: digit(0) * 16.0 + digit(1),
            green: digit(2) * 16.0 + digit(3),
            blue: digit(4) * 16.0 + digit(5),
        }),
        _ => None,
    }
}

/// The passed colors mixed: the mean of each of red, green and blue (`mixColors`). No colors mix
/// to no number (0 / 0), like in Java.
pub fn mix_colors(colors: &[Color]) -> Color {
    let mut red = 0.0;
    let mut green = 0.0;
    let mut blue = 0.0;
    for color in colors {
        red += color.red;
        green += color.green;
        blue += color.blue;
    }
    let count = colors.len() as f64;
    Color {
        red: red / count,
        green: green / count,
        blue: blue / count,
    }
}

/// The passed colors mixed, or None for none. A color which can't be read is left out
/// (`mixHexColors`).
pub fn mix_hex_colors<S: AsRef<str>>(colors: &[S]) -> Option<Color> {
    let colors: Vec<Color> = colors.iter().filter_map(|color| hex_to_rgb(color.as_ref())).collect();
    if colors.is_empty() {
        None
    } else {
        Some(mix_colors(&colors))
    }
}

/// The slot of a wheel by its number, counted from 1 and around the wheel (9 is the first slot of
/// a wheel with 8 slots, 0 the last). None for a wheel without slots (`getWheelSlot`).
pub fn get_wheel_slot(wheel: &FixtureWheel, slot_number: i32) -> Option<&FixtureWheelSlot> {
    let slot_count = i32::try_from(wheel.slots.len()).ok().filter(|count| *count > 0)?;
    wheel
        .slots
        .get(slot_number.wrapping_sub(1).rem_euclid(slot_count) as usize)
}

/// One slot, or two, if they are split: slot number 2.5 gives the slots 2 and 3 (`getWheelSlots`).
pub fn get_wheel_slots(wheel: Option<&FixtureWheel>, slot_number: f64) -> Vec<&FixtureWheelSlot> {
    let mut slots = Vec::new();
    let Some(wheel) = wheel.filter(|wheel| !wheel.slots.is_empty()) else {
        return slots;
    };
    // cut to an int like the cast of Java
    slots.extend(get_wheel_slot(wheel, slot_number.floor() as i32));
    if slot_number != slot_number.floor() {
        slots.extend(get_wheel_slot(wheel, slot_number.ceil() as i32));
    }
    slots
}

/// The colors of the slot (or the two split slots) of a wheel (`getWheelSlotColors`).
pub fn get_wheel_slot_colors(wheel: Option<&FixtureWheel>, slot_number: f64) -> Vec<&str> {
    get_wheel_slots(wheel, slot_number)
        .into_iter()
        .flat_map(|slot| slot.colors.iter().flatten().map(String::as_str))
        .collect()
}

/// The colors of the slot (or the two split slots) of a wheel, mixed (`getMixedWheelSlotColor`).
pub fn get_mixed_wheel_slot_color(wheel: Option<&FixtureWheel>, slot_number: f64) -> Option<Color> {
    mix_hex_colors(&get_wheel_slot_colors(wheel, slot_number))
}

// ---------------------------------------------------------------------------------------------
// The pixels of a matrix (DefaultDesignerService)
// ---------------------------------------------------------------------------------------------

/// A pixel of a matrix at its position, counted from 1 (`CachedFixturePixel`). A pixel group a
/// channel is repeated for is one as well, at 0, 0, 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixel {
    pub key: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Pixel {
    pub fn new(key: impl Into<String>, x: i32, y: i32, z: i32) -> Pixel {
        Pixel {
            key: key.into(),
            x,
            y,
            z,
        }
    }
}

/// The key the fixture library gives a pixel of a matrix defined by its pixel count: "3" in a row
/// of pixels, "(2, 3)" on a plane or "(1, 2, 3)" in a cube (`getDefaultPixelKey`). The count has
/// to name all three axes.
pub fn get_default_pixel_key(pixel_count: &[i32], x: i32, y: i32, z: i32) -> String {
    let x_defined = pixel_count[0] > 1;
    let y_defined = pixel_count[1] > 1;
    let z_defined = pixel_count[2] > 1;
    match x_defined as u8 + y_defined as u8 + z_defined as u8 {
        2 => format!("({}, {})", if x_defined { x } else { y }, if y_defined { y } else { z }),
        3 => format!("({x}, {y}, {z})"),
        _ => x.max(y).max(z).to_string(),
    }
}

/// All pixels of the matrix of a profile at their positions: layer by layer and row by row, X
/// changes fastest (`getAllPixelKeys`).
pub fn get_all_pixel_keys(profile: &FixtureProfile) -> Vec<Pixel> {
    let mut result = Vec::new();
    let Some(matrix) = &profile.matrix else {
        return result;
    };
    if let Some(pixel_keys) = &matrix.pixel_keys {
        for (z, layer) in pixel_keys.iter().enumerate() {
            for (y, row) in layer.iter().enumerate() {
                for (x, key) in row.iter().enumerate() {
                    if let Some(key) = key {
                        result.push(Pixel::new(key.clone(), x as i32 + 1, y as i32 + 1, z as i32 + 1));
                    }
                }
            }
        }
    } else if let Some(pixel_count) = matrix.pixel_count.as_deref().filter(|count| count.len() == 3) {
        for z in 1..=pixel_count[2] {
            for y in 1..=pixel_count[1] {
                for x in 1..=pixel_count[0] {
                    result.push(Pixel::new(get_default_pixel_key(pixel_count, x, y, z), x, y, z));
                }
            }
        }
    }
    result
}

/// The pixels in the order of the passed axes, e.g. "YXZ": Y changes fastest, Z slowest
/// (`getPixelsByAxes`). The axes have to be three letters.
pub fn get_pixels_by_axes(pixels: &[Pixel], axes: &str) -> Vec<Pixel> {
    let axes: Vec<char> = axes.chars().collect();
    let position = |pixel: &Pixel, axis: char| match axis {
        'X' => pixel.x,
        'Y' => pixel.y,
        _ => pixel.z,
    };
    let mut result = pixels.to_vec();
    result.sort_by(|pixel1, pixel2| {
        position(pixel1, axes[2])
            .cmp(&position(pixel2, axes[2]))
            .then(position(pixel1, axes[1]).cmp(&position(pixel2, axes[1])))
            .then(position(pixel1, axes[0]).cmp(&position(pixel2, axes[0])))
    });
    result
}

/// The names of the pixel groups of a profile, in their order (`getPixelGroupNames`).
pub fn get_pixel_group_names(profile: &FixtureProfile) -> Vec<String> {
    profile
        .matrix
        .as_ref()
        .and_then(|matrix| matrix.pixel_groups.as_ref())
        .map(|groups| groups.iter().map(|group| group.name.clone()).collect())
        .unwrap_or_default()
}

/// The pixels a channel of a mode is repeated for, in the order of their channels
/// (`getPixelsInOrder`). A pixel group counts as one pixel (at 0, 0, 0): its channels control all
/// of its pixels at once. None for a channel of its own.
pub fn get_pixels_in_order(profile: &FixtureProfile, repeat_for: &[String]) -> Vec<Pixel> {
    let mut result = Vec::new();
    if repeat_for.is_empty() || profile.matrix.is_none() {
        // a channel of its own, no pixels
        return result;
    }
    let all_pixels = get_all_pixel_keys(profile);
    let group_names = get_pixel_group_names(profile);
    if let (1, Some(order)) = (repeat_for.len(), repeat_for[0].strip_prefix("eachPixel")) {
        if order == "ABC" {
            // the pixel keys in alphanumerical order
            result = all_pixels;
            result.sort_by(|pixel1, pixel2| compare_keys(&pixel1.key, &pixel2.key));
        } else if order == "Group" {
            // the pixel groups in the order of the profile
            result.extend(group_names.into_iter().map(|name| Pixel::new(name, 0, 0, 0)));
        } else if order.chars().count() == 3 && order.contains('X') && order.contains('Y') && order.contains('Z') {
            // by position, e.g. eachPixelXYZ
            result = get_pixels_by_axes(&all_pixels, order);
        }
        return result;
    }
    // pixel keys and pixel groups
    for entry in repeat_for {
        if let Some(pixel) = all_pixels.iter().find(|pixel| pixel.key == *entry) {
            result.push(pixel.clone());
        } else if group_names.contains(entry) {
            result.push(Pixel::new(entry.clone(), 0, 0, 0));
        }
    }
    result
}

/// The number of pixels a matrix has on X and Y: the columns and the rows, 0 where it doesn't say
/// (`getMatrixSize`).
pub fn get_matrix_size(matrix: &FixtureMatrix) -> (i32, i32) {
    if let Some(pixel_keys) = &matrix.pixel_keys {
        let mut rows = 0;
        let mut columns = 0;
        for layer in pixel_keys {
            rows = rows.max(layer.len() as i32);
            for row in layer {
                columns = columns.max(row.len() as i32);
            }
        }
        return (columns, rows);
    }
    let count = |index: usize| {
        matrix
            .pixel_count
            .as_ref()
            .and_then(|count| count.get(index).copied())
            .unwrap_or(0)
    };
    (count(0), count(1))
}

/// The mode of the profile a fixture uses: by the short name of the mode, or by its name if it
/// has no short one (`getModeByFixture`).
pub fn get_mode_by_fixture<'a>(profile: &'a FixtureProfile, fixture: &Fixture) -> Option<&'a FixtureMode> {
    let wanted = fixture.mode_short_name.as_deref()?;
    profile
        .modes
        .as_ref()?
        .iter()
        .find(|mode| match mode.short_name.as_deref() {
            Some(short_name) if !short_name.is_empty() => short_name == wanted,
            _ => mode.name.as_deref() == Some(wanted),
        })
}

/// The name of a template channel for a pixel: "Red $pixelKey" is "Red 3" for the pixel "3"
/// (`getChannelNameWithPixelKey`).
pub fn get_channel_name_with_pixel_key(template_channel_name: &str, pixel_key: Option<&str>) -> String {
    match pixel_key {
        Some(pixel_key) if !pixel_key.is_empty() => template_channel_name.replace("$pixelKey", pixel_key),
        _ => template_channel_name.to_string(),
    }
}

/// The pixels the channels of a mode are repeated for, each once, in the order the mode comes to
/// them in (`fixtureGetUniquePixels` for a mode).
pub fn mode_get_unique_pixels(profile: &FixtureProfile, mode: &FixtureMode) -> Vec<Pixel> {
    let mut pixels: Vec<Pixel> = Vec::new();
    for channel in &mode.channels {
        for pixel in get_pixels_in_order(profile, &channel.repeat_for) {
            if !pixels.iter().any(|known| known.key == pixel.key) {
                pixels.push(pixel);
            }
        }
    }
    pixels
}

/// All pixels used by the channels of the mode of a fixture (`fixtureGetUniquePixels`). None for
/// a fixture without a mode in the profile, where Java fails.
pub fn fixture_get_unique_pixels(profile: &FixtureProfile, fixture: &Fixture) -> Option<Vec<Pixel>> {
    Some(mode_get_unique_pixels(profile, get_mode_by_fixture(profile, fixture)?))
}

/// Does the mode have a channel which is not repeated for pixels (`fixtureHasGeneralChannel` for
/// a mode)?
pub fn mode_has_general_channel(profile: &FixtureProfile, mode: &FixtureMode) -> bool {
    mode.channels
        .iter()
        .any(|channel| get_pixels_in_order(profile, &channel.repeat_for).is_empty())
}

/// Does the fixture have a channel without a reference to pixels in its mode
/// (`fixtureHasGeneralChannel`)? None for a fixture without a mode in the profile, where Java
/// fails.
pub fn fixture_has_general_channel(profile: &FixtureProfile, fixture: &Fixture) -> Option<bool> {
    Some(mode_has_general_channel(
        profile,
        get_mode_by_fixture(profile, fixture)?,
    ))
}

/// A channel of a mode nobody uses: without a name and not repeated for pixels (`isUnusedChannel`).
pub fn is_unused_channel(channel: &FixtureModeChannel) -> bool {
    channel.name.is_empty() && channel.repeat_for.is_empty()
}

// ---------------------------------------------------------------------------------------------
// The pixels of a group (DefaultFixturePixelGroupResolveService)
// ---------------------------------------------------------------------------------------------

// Character.toLowerCase of a char of Java
fn java_lower_case(unit: u16) -> u16 {
    let Some(character) = char::from_u32(unit as u32) else {
        // half a pair
        return unit;
    };
    if character == '\u{130}' {
        return 'i' as u16;
    }
    let mut lower = character.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(lower), None) => u16::try_from(lower as u32).unwrap_or(unit),
        _ => unit,
    }
}

// two runs of digits by their value, however many of them
fn compare_digits(digits1: &[u16], digits2: &[u16]) -> Ordering {
    let significant = |digits: &[u16]| {
        digits
            .iter()
            .position(|digit| *digit != '0' as u16)
            .unwrap_or(digits.len())
    };
    let digits1 = &digits1[significant(digits1)..];
    let digits2 = &digits2[significant(digits2)..];
    digits1.len().cmp(&digits2.len()).then_with(|| digits1.cmp(digits2))
}

/// Pixel keys like the fixture library sorts them: numbers by their value (2 before 10), the rest
/// alphabetically, whatever its case (`compareKeys`).
pub fn compare_keys(key1: &str, key2: &str) -> Ordering {
    let key1: Vec<u16> = key1.encode_utf16().collect();
    let key2: Vec<u16> = key2.encode_utf16().collect();
    // the digits 0 to 9 only
    let is_digit = |unit: u16| ('0' as u16..='9' as u16).contains(&unit);
    let mut index1 = 0;
    let mut index2 = 0;
    while index1 < key1.len() && index2 < key2.len() {
        let unit1 = key1[index1];
        let unit2 = key2[index2];
        if is_digit(unit1) && is_digit(unit2) {
            let start1 = index1;
            let start2 = index2;
            while index1 < key1.len() && is_digit(key1[index1]) {
                index1 += 1;
            }
            while index2 < key2.len() && is_digit(key2[index2]) {
                index2 += 1;
            }
            let result = compare_digits(&key1[start1..index1], &key2[start2..index2]);
            if result != Ordering::Equal {
                return result;
            }
        } else {
            let result = java_lower_case(unit1).cmp(&java_lower_case(unit2));
            if result != Ordering::Equal {
                return result;
            }
            index1 += 1;
            index2 += 1;
        }
    }
    (key1.len() - index1).cmp(&(key2.len() - index2))
}

fn sorted_by_key(pixels: &[Pixel]) -> Vec<Pixel> {
    let mut result = pixels.to_vec();
    result.sort_by(|pixel1, pixel2| compare_keys(&pixel1.key, &pixel2.key));
    result
}

// `\s` of a regular expression of Java
fn is_java_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r')
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

// Integer.parseInt of digits (with a "-" before them or not)
fn parse_int(digits: &str, rule: &str) -> Result<i32, String> {
    digits
        .parse::<i32>()
        .map_err(|_| format!("The number of the numeric rule \"{rule}\" is too large"))
}

/// Whether a position meets a rule of a pixel group: "<=5", ">=5", "=5", "even", "odd", "3n",
/// "3n+1" or a plain number (`matchNumberRule`). A rule which can't be read is an error.
pub fn match_number_rule(value: i32, rule_raw: &str) -> Result<bool, String> {
    let rule = java_trim(rule_raw).to_lowercase();

    // comparisons: <=5, >=5, =5
    for operator in ["<=", ">=", "="] {
        let Some(rest) = rule.strip_prefix(operator) else {
            continue;
        };
        let number = rest.trim_start_matches(is_java_space);
        if !all_digits(number.strip_prefix('-').unwrap_or(number)) {
            continue;
        }
        let number = parse_int(number, rule_raw)?;
        return Ok(match operator {
            "<=" => value <= number,
            ">=" => value >= number,
            _ => value == number,
        });
    }

    // even / odd
    if rule == "even" {
        return Ok(value % 2 == 0);
    }
    if rule == "odd" {
        return Ok((value % 2).abs() == 1);
    }

    // 3n, 3n+1, ...
    if let Some((base, rest)) = rule.split_once('n') {
        let remainder = if rest.is_empty() {
            Some("0")
        } else {
            rest.strip_prefix('+')
        };
        if let Some(remainder) = remainder.filter(|remainder| all_digits(base) && all_digits(remainder)) {
            let base = parse_int(base, rule_raw)?;
            let remainder = parse_int(remainder, rule_raw)?;
            if base == 0 {
                return Err(format!("The numeric rule \"{rule_raw}\" divides by zero"));
            }
            // like the ints of Java, which wrap around
            let modulo = (value % base).wrapping_add(base) % base;
            let target = (remainder % base).wrapping_add(base) % base;
            return Ok(modulo == target);
        }
    }

    // plain number => "=number" (no "+5")
    if all_digits(rule.strip_prefix('-').unwrap_or(&rule)) {
        return Ok(value == parse_int(&rule, rule_raw)?);
    }

    Err(format!("Unknown numeric rule: \"{rule_raw}\""))
}

// Whether a pixel meets all constraints. Like in Java, a rule is only read once a pixel gets to
// it: the first rule a pixel doesn't meet ends it.
fn matches_all_constraints(pixel: &Pixel, constraints: &FixtureMatrixPixelGroupConstraints) -> Result<bool, String> {
    for (position, rules) in [
        (pixel.x, &constraints.x),
        (pixel.y, &constraints.y),
        (pixel.z, &constraints.z),
    ] {
        for rule in rules.iter().flatten() {
            if !match_number_rule(position, rule)? {
                return Ok(false);
            }
        }
    }
    // all of the regular expressions have to be found in the key
    for expression in constraints.name.iter().flatten() {
        if !regex_find(expression, &pixel.key)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The pixels of a group in the order of the fixture library: the listed keys as listed, all
/// pixels and pixels constrained by their name alphanumerically, the others by their position, as
/// the passed pixels come (`resolveGroupPixels`). A rule which can't be read is an error.
pub fn resolve_group_pixels(
    group: Option<&FixtureMatrixPixelGroup>,
    all_pixels: &[Pixel],
) -> Result<Vec<Pixel>, String> {
    let Some(group) = group else {
        return Ok(Vec::new());
    };

    // 1) all
    if group.is_all {
        return Ok(sorted_by_key(all_pixels));
    }
    let Some(constraints) = &group.constraints else {
        return Ok(Vec::new());
    };

    // 2) explicit keys (if present, they are the definition)
    if let Some(keys) = constraints.keys.as_ref().filter(|keys| !keys.is_empty()) {
        return Ok(keys
            .iter()
            .filter_map(|key| all_pixels.iter().find(|pixel| pixel.key == *key).cloned())
            .collect());
    }

    // 3) constraints by x/y/z/name
    let by_name = constraints.name.as_ref().is_some_and(|name| !name.is_empty());
    let candidates = if by_name {
        sorted_by_key(all_pixels)
    } else {
        all_pixels.to_vec()
    };
    let mut result = Vec::new();
    for pixel in candidates {
        if matches_all_constraints(&pixel, constraints)? {
            result.push(pixel);
        }
    }
    Ok(result)
}

/// The pixel of the passed key, or else the pixels of the group of the passed name
/// (`getPixelsForKeyOrGroup`).
pub fn get_pixels_for_key_or_group(
    matrix: Option<&FixtureMatrix>,
    all_pixels: &[Pixel],
    key_or_group: &str,
) -> Result<Vec<Pixel>, String> {
    let Some(matrix) = matrix else {
        return Ok(Vec::new());
    };

    // 1) exact key first
    if let Some(pixel) = all_pixels.iter().find(|pixel| pixel.key == key_or_group) {
        return Ok(vec![pixel.clone()]);
    }

    // 2) group by name
    let group = matrix
        .pixel_groups
        .iter()
        .flatten()
        .find(|group| group.name == key_or_group);
    resolve_group_pixels(group, all_pixels)
}

// ---------------------------------------------------------------------------------------------
// Regular expressions of Java
// ---------------------------------------------------------------------------------------------

// where `.` stops and `$` looks ahead to
fn is_line_terminator(character: char) -> bool {
    matches!(character, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

// `$` of Java: at the end, or before a line terminator which ends the text
fn is_dollar(text: &[char], index: usize) -> bool {
    let end = text.len();
    if index + 2 < end {
        return false;
    }
    if index + 2 == end {
        return text[index] == '\r' && text[index + 1] == '\n';
    }
    if index < end {
        if text[index] == '\n' {
            // not between \r and \n
            return index == 0 || text[index - 1] != '\r';
        }
        return is_line_terminator(text[index]);
    }
    true
}

// a character of a word of `\b` (of a JDK from 19 on: the ones of `\w`)
fn is_word_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

// The combining marks (the category Mn of Unicode) `\b` takes for a part of the word before them.
// The common ones only: the standard library doesn't know the categories.
fn is_combining_mark(character: char) -> bool {
    matches!(
        character as u32,
        0x300..=0x36f | 0x483..=0x487 | 0x591..=0x5bd | 0x610..=0x61a | 0x64b..=0x65f | 0x1ab0..=0x1abd | 0x1dc0..=0x1dff | 0x20d0..=0x20dc | 0xfe20..=0xfe2f
    )
}

// is there a letter or a digit before the combining marks before the index?
fn has_base_character(text: &[char], mut index: usize) -> bool {
    while index > 0 {
        let character = text[index - 1];
        if character.is_alphanumeric() {
            return true;
        }
        if !is_combining_mark(character) {
            return false;
        }
        index -= 1;
    }
    false
}

// `\b` of Java
fn is_word_boundary(text: &[char], index: usize) -> bool {
    let in_word =
        |at: usize| is_word_character(text[at]) || (is_combining_mark(text[at]) && has_base_character(text, at));
    let left = index > 0 && in_word(index - 1);
    let right = index < text.len() && in_word(index);
    left != right
}

enum Node {
    Empty,
    Character(char),
    // `.`
    Any,
    Class {
        negated: bool,
        items: Vec<ClassItem>,
    },
    // `^` and `\A`
    Start,
    // `\z`
    End,
    // `$` and `\Z`
    Dollar,
    // `\b`, or `\B` (false)
    WordBoundary(bool),
    Sequence(Vec<Node>),
    Alternatives(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
        lazy: bool,
    },
}

enum ClassItem {
    Range(char, char),
    // `\d`, `\w` and `\s`, or (true) all but them
    Digit(bool),
    Word(bool),
    Space(bool),
}

impl ClassItem {
    fn holds(&self, character: char) -> bool {
        match self {
            ClassItem::Range(first, last) => (*first..=*last).contains(&character),
            ClassItem::Digit(negated) => character.is_ascii_digit() != *negated,
            ClassItem::Word(negated) => is_word_character(character) != *negated,
            ClassItem::Space(negated) => is_java_space(character) != *negated,
        }
    }
}

enum Escape {
    Character(char),
    Class(ClassItem),
    Node(Node),
}

// as many repetitions as an expression may ask for, as deep as its groups may be nested, and as
// many steps and as deep as a search may go
const MAX_REPETITIONS: u32 = 1000;
const MAX_GROUPS: usize = 100;
const MAX_STEPS: u32 = 1_000_000;
const MAX_DEPTH: u32 = 1500;

struct RegexParser<'a> {
    pattern: &'a str,
    characters: Vec<char>,
    position: usize,
    depth: usize,
}

impl RegexParser<'_> {
    fn error(&self, message: &str) -> String {
        format!(
            "The regular expression \"{}\" can't be read: {message} (at {})",
            self.pattern, self.position
        )
    }

    fn unsupported(&self, what: &str) -> String {
        format!(
            "The regular expression \"{}\" is not supported: {what} (at {})",
            self.pattern, self.position
        )
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.position).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.characters.get(self.position + offset).copied()
    }

    fn alternatives(&mut self) -> Result<Node, String> {
        let mut alternatives = vec![self.sequence()?];
        while self.peek() == Some('|') {
            self.position += 1;
            alternatives.push(self.sequence()?);
        }
        Ok(if alternatives.len() == 1 {
            alternatives.remove(0)
        } else {
            Node::Alternatives(alternatives)
        })
    }

    fn sequence(&mut self) -> Result<Node, String> {
        let mut nodes = Vec::new();
        loop {
            match self.peek() {
                None | Some('|') => break,
                Some(')') => {
                    if self.depth == 0 {
                        return Err(self.error("a ) without a ("));
                    }
                    break;
                }
                Some('*' | '+' | '?') => return Err(self.error("nothing to repeat")),
                Some(_) => {
                    let atom = self.atom()?;
                    nodes.push(self.repeated(atom)?);
                }
            }
        }
        Ok(Node::Sequence(nodes))
    }

    fn atom(&mut self) -> Result<Node, String> {
        let Some(character) = self.peek() else {
            return Ok(Node::Empty);
        };
        match character {
            // what stands before a repetition without anything to repeat (e.g. "{1}") is nothing
            '{' => Ok(Node::Empty),
            '(' => {
                self.position += 1;
                if self.peek() == Some('?') {
                    match (self.peek_at(1), self.peek_at(2)) {
                        (Some(':'), _) => self.position += 2,
                        (Some('<'), Some(letter)) if letter.is_ascii_alphabetic() => {
                            // a named group
                            self.position += 2;
                            while self.peek().is_some_and(|character| character.is_ascii_alphanumeric()) {
                                self.position += 1;
                            }
                            if self.peek() != Some('>') {
                                return Err(self.error("a name of a group without an end"));
                            }
                            self.position += 1;
                        }
                        _ => return Err(self.unsupported("groups looking around or with flags")),
                    }
                }
                self.depth += 1;
                if self.depth > MAX_GROUPS {
                    return Err(self.unsupported("groups nested that deeply"));
                }
                let node = self.alternatives()?;
                if self.peek() != Some(')') {
                    return Err(self.error("a ( without a )"));
                }
                self.position += 1;
                self.depth -= 1;
                Ok(node)
            }
            '[' => {
                self.position += 1;
                self.class()
            }
            '.' => {
                self.position += 1;
                Ok(Node::Any)
            }
            '^' => {
                self.position += 1;
                Ok(Node::Start)
            }
            '$' => {
                self.position += 1;
                Ok(Node::Dollar)
            }
            '\\' => {
                self.position += 1;
                match self.escape(false)? {
                    Escape::Character(character) => Ok(Node::Character(character)),
                    Escape::Class(item) => Ok(Node::Class {
                        negated: false,
                        items: vec![item],
                    }),
                    Escape::Node(node) => Ok(node),
                }
            }
            character => {
                self.position += 1;
                Ok(Node::Character(character))
            }
        }
    }

    fn number(&mut self) -> Result<Option<u32>, String> {
        let start = self.position;
        while self.peek().is_some_and(|character| character.is_ascii_digit()) {
            self.position += 1;
        }
        if start == self.position {
            return Ok(None);
        }
        let digits: String = self.characters[start..self.position].iter().collect();
        match digits.parse::<u32>() {
            Ok(number) if number <= MAX_REPETITIONS => Ok(Some(number)),
            _ => Err(self.unsupported("that many repetitions")),
        }
    }

    // the repetition after an atom, if there is one
    fn repeated(&mut self, atom: Node) -> Result<Node, String> {
        let (min, max) = match self.peek() {
            Some('?') => {
                self.position += 1;
                (0, Some(1))
            }
            Some('*') => {
                self.position += 1;
                (0, None)
            }
            Some('+') => {
                self.position += 1;
                (1, None)
            }
            Some('{') => {
                self.position += 1;
                let Some(min) = self.number()? else {
                    return Err(self.error("a { without a number"));
                };
                let max = match self.peek() {
                    Some(',') => {
                        self.position += 1;
                        self.number()?
                    }
                    _ => Some(min),
                };
                if self.peek() != Some('}') {
                    return Err(self.error("a { without a }"));
                }
                self.position += 1;
                if max.is_some_and(|max| max < min) {
                    return Err(self.error("a repetition from more to less"));
                }
                (min, max)
            }
            _ => return Ok(atom),
        };
        let lazy = match self.peek() {
            Some('?') => {
                self.position += 1;
                true
            }
            Some('+') => return Err(self.unsupported("possessive repetitions")),
            _ => false,
        };
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            lazy,
        })
    }

    fn hex(&mut self, digits: usize) -> Result<char, String> {
        let mut code = 0;
        for _ in 0..digits {
            let digit = self
                .peek()
                .and_then(|character| character.to_digit(16))
                .ok_or_else(|| self.error("not a hexadecimal digit"))?;
            code = code * 16 + digit;
            self.position += 1;
        }
        char::from_u32(code).ok_or_else(|| self.unsupported("half a pair of characters"))
    }

    // what stands after a backslash
    fn escape(&mut self, in_class: bool) -> Result<Escape, String> {
        let Some(character) = self.peek() else {
            return Err(self.error("a \\ at the end"));
        };
        self.position += 1;
        Ok(match character {
            'd' => Escape::Class(ClassItem::Digit(false)),
            'D' => Escape::Class(ClassItem::Digit(true)),
            'w' => Escape::Class(ClassItem::Word(false)),
            'W' => Escape::Class(ClassItem::Word(true)),
            's' => Escape::Class(ClassItem::Space(false)),
            'S' => Escape::Class(ClassItem::Space(true)),
            't' => Escape::Character('\t'),
            'n' => Escape::Character('\n'),
            'r' => Escape::Character('\r'),
            'f' => Escape::Character('\u{c}'),
            'a' => Escape::Character('\u{7}'),
            'e' => Escape::Character('\u{1b}'),
            'x' if self.peek() != Some('{') => Escape::Character(self.hex(2)?),
            'u' => Escape::Character(self.hex(4)?),
            'b' if !in_class && self.peek() != Some('{') => Escape::Node(Node::WordBoundary(true)),
            'B' if !in_class => Escape::Node(Node::WordBoundary(false)),
            'A' if !in_class => Escape::Node(Node::Start),
            'z' if !in_class => Escape::Node(Node::End),
            'Z' if !in_class => Escape::Node(Node::Dollar),
            character if character.is_ascii_alphanumeric() => return Err(self.unsupported(&format!("\\{character}"))),
            character => Escape::Character(character),
        })
    }

    // a class of characters, after its [
    fn class(&mut self) -> Result<Node, String> {
        let negated = self.peek() == Some('^');
        if negated {
            self.position += 1;
        }
        let mut items = Vec::new();
        loop {
            let Some(character) = self.peek() else {
                return Err(self.error("a [ without a ]"));
            };
            let first = match character {
                // a ] right at the start is one of the characters
                ']' if !items.is_empty() => {
                    self.position += 1;
                    return Ok(Node::Class { negated, items });
                }
                '[' => return Err(self.unsupported("a class in a class")),
                '&' if self.peek_at(1) == Some('&') => return Err(self.unsupported("classes cut with &&")),
                '\\' => {
                    self.position += 1;
                    match self.escape(true)? {
                        Escape::Character(character) => character,
                        Escape::Class(item) => {
                            items.push(item);
                            continue;
                        }
                        Escape::Node(_) => return Err(self.error("not a character")),
                    }
                }
                character => {
                    self.position += 1;
                    character
                }
            };
            // a range, unless the - is the last character of the class
            let last = match (self.peek(), self.peek_at(1)) {
                (Some('-'), Some('[')) => return Err(self.unsupported("a class in a class")),
                (Some('-'), Some(next)) if next != ']' => {
                    self.position += 1;
                    if next == '\\' {
                        self.position += 1;
                        match self.escape(true)? {
                            Escape::Character(character) => character,
                            _ => return Err(self.error("a range without an end")),
                        }
                    } else {
                        self.position += 1;
                        next
                    }
                }
                _ => first,
            };
            if last < first {
                return Err(self.error("a range from a later character to an earlier one"));
            }
            items.push(ClassItem::Range(first, last));
        }
    }
}

struct RegexMatcher<'a> {
    text: &'a [char],
    steps: Cell<u32>,
    depth: Cell<u32>,
}

type Matched = Result<bool, String>;

impl RegexMatcher<'_> {
    // whether the node matches at the index and the rest (what comes after it) from where it ends
    fn node(&self, node: &Node, index: usize, rest: &mut dyn FnMut(usize) -> Matched) -> Matched {
        self.steps.set(self.steps.get() + 1);
        if self.steps.get() > MAX_STEPS || self.depth.get() >= MAX_DEPTH {
            return Err("The regular expression takes too long".to_string());
        }
        self.depth.set(self.depth.get() + 1);
        let matched = self.node_from(node, index, rest);
        self.depth.set(self.depth.get() - 1);
        matched
    }

    fn node_from(&self, node: &Node, index: usize, rest: &mut dyn FnMut(usize) -> Matched) -> Matched {
        let character = self.text.get(index).copied();
        match node {
            Node::Empty => rest(index),
            Node::Character(expected) => {
                if character == Some(*expected) {
                    rest(index + 1)
                } else {
                    Ok(false)
                }
            }
            Node::Any => match character {
                Some(character) if !is_line_terminator(character) => rest(index + 1),
                _ => Ok(false),
            },
            Node::Class { negated, items } => match character {
                Some(character) if items.iter().any(|item| item.holds(character)) != *negated => rest(index + 1),
                _ => Ok(false),
            },
            Node::Start => {
                if index == 0 {
                    rest(index)
                } else {
                    Ok(false)
                }
            }
            Node::End => {
                if index == self.text.len() {
                    rest(index)
                } else {
                    Ok(false)
                }
            }
            Node::Dollar => {
                if is_dollar(self.text, index) {
                    rest(index)
                } else {
                    Ok(false)
                }
            }
            Node::WordBoundary(expected) => {
                if is_word_boundary(self.text, index) == *expected {
                    rest(index)
                } else {
                    Ok(false)
                }
            }
            Node::Sequence(nodes) => self.sequence(nodes, index, rest),
            Node::Alternatives(alternatives) => {
                for alternative in alternatives {
                    if self.node(alternative, index, &mut *rest)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Node::Repeat { node, min, max, lazy } => self.repeat(node, *min, *max, *lazy, 0, index, rest),
        }
    }

    fn sequence(&self, nodes: &[Node], index: usize, rest: &mut dyn FnMut(usize) -> Matched) -> Matched {
        match nodes.split_first() {
            None => rest(index),
            Some((first, others)) => self.node(first, index, &mut |next| self.sequence(others, next, &mut *rest)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn repeat(
        &self,
        node: &Node,
        min: u32,
        max: Option<u32>,
        lazy: bool,
        count: u32,
        index: usize,
        rest: &mut dyn FnMut(usize) -> Matched,
    ) -> Matched {
        let once_more = |rest: &mut dyn FnMut(usize) -> Matched| {
            if max.is_some_and(|max| count >= max) {
                return Ok(false);
            }
            self.node(node, index, &mut |next| {
                if next == index && count >= min {
                    // a round which takes nothing doesn't get any further
                    return Ok(false);
                }
                self.repeat(node, min, max, lazy, count + 1, next, &mut *rest)
            })
        };
        if count < min {
            return once_more(rest);
        }
        // as few rounds as it takes, or as many as there are
        let (first, second) = if lazy { (false, true) } else { (true, false) };
        for more in [first, second] {
            if if more { once_more(rest)? } else { rest(index)? } {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Whether a regular expression of Java is found in a text (`Pattern.compile(pattern)
/// .matcher(text).find()`), for the names a pixel group constrains its pixels by.
///
/// It knows what such names are written with: characters, `.`, classes (`[a-z]`, `[^0-9]`, `\d`,
/// `\w`, `\s` and their opposites), `^`, `$`, `\b`, `\B`, `\A`, `\z`, `\Z`, groups, `|` and the
/// repetitions `?`, `*`, `+`, `{n}`, `{n,}`, `{n,m}`, greedy or lazy. An expression Java can't read
/// is an error, and so is one written with anything else Java knows (looking around, references
/// to groups, flags, possessive repetitions, classes in classes, `\p{…}`).
pub fn regex_find(pattern: &str, text: &str) -> Result<bool, String> {
    let mut parser = RegexParser {
        pattern,
        characters: pattern.chars().collect(),
        position: 0,
        depth: 0,
    };
    let root = parser.alternatives()?;
    if parser.position != parser.characters.len() {
        return Err(parser.error("a ) without a ("));
    }
    let text: Vec<char> = text.chars().collect();
    let matcher = RegexMatcher {
        text: &text,
        steps: Cell::new(0),
        depth: Cell::new(0),
    };
    for start in 0..=text.len() {
        if matcher.node(&root, start, &mut |_| Ok(true))? {
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------------------------
// Where the properties of the fixture library lie (CapabilityPositions)
// ---------------------------------------------------------------------------------------------

// where the words of the fixture library lie among the numbers it gives the same property in
fn word_rank(word: &str) -> Option<f64> {
    Some(match word {
        "closed" => -2e9,
        "narrow" => -1e9,
        "wide" => 1e9,
        "open" => 1e9,
        "near" => -1e9,
        "far" => 1e9,
        "off" => -2e9,
        "low" => -1e9,
        "high" => 1e9,
        "warm" => -2e9,
        "CTO" => -1e9,
        "default" => 6500.0,
        "CTB" => 1e9,
        "cold" => 2e9,
        "stop" => -2e9,
        "slow" => -1e9,
        "fast" => 1e9,
        "instant" => -2e9,
        "short" => -1e9,
        "long" => 1e9,
        "weak" => -1e9,
        "strong" => 1e9,
        "out" => -1e9,
        "in" => 1e9,
        _ => return None,
    })
}

/// Where a property of the fixture library lies (e.g. "10deg", "wide", "120bpm", which is 2 Hz,
/// or "500ms", which is half a second), or None for one which can't be read (`getPropertyRank`).
pub fn get_property_rank(text: Option<&str>) -> Option<f64> {
    let trimmed = java_trim(text?);
    if let Some(rank) = word_rank(trimmed) {
        return Some(rank);
    }
    let (number, unit) = trimmed.split_at(scan_number(trimmed)?);
    let value = number.parse::<f64>().ok()?;
    match unit {
        "bpm" | "rpm" => Some(value / 60.0),
        "ms" => Some(value / 1000.0),
        "" | "deg" | "%" | "m^3/min" | "m" | "K" | "Hz" | "s" | "lm" => Some(value),
        _ => None,
    }
}

/// Is a property of the fixture library nothing at all, e.g. no fog ("off" or 0) (`isNothing`)?
pub fn is_nothing(text: Option<&str>) -> bool {
    let Some(text) = text else {
        return false;
    };
    java_trim(text) == "off" || get_property_rank(Some(text)) == Some(0.0)
}

/// Where a speed of a rotation lies (e.g. "fast CW", "stop", "-10rpm" or "50%"), clockwise above
/// 0, or None for one which can't be read (`getRotationRank`).
pub fn get_rotation_rank(text: Option<&str>) -> Option<f64> {
    let trimmed = java_trim(text?);
    match trimmed {
        "fast CW" => Some(1e9),
        "slow CW" => Some(1e-9),
        "stop" => Some(0.0),
        "slow CCW" => Some(-1e-9),
        "fast CCW" => Some(-1e9),
        _ => get_property_rank(Some(trimmed)),
    }
}

// ---------------------------------------------------------------------------------------------
// The capabilities a fixture is reset and its lamp is switched with (Maintenance)
// ---------------------------------------------------------------------------------------------

/// What a fixture is asked to do through a maintenance capability (`Maintenance.Action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaintenanceAction {
    Reset,
    LampOn,
    LampOff,
}

impl MaintenanceAction {
    pub const ALL: [MaintenanceAction; 3] = [
        MaintenanceAction::Reset,
        MaintenanceAction::LampOn,
        MaintenanceAction::LampOff,
    ];

    /// The action a preset names ("reset", "lamp-on", "lamp-off"), or None for one nobody knows.
    pub fn of(name: Option<&str>) -> Option<MaintenanceAction> {
        match name? {
            "reset" => Some(MaintenanceAction::Reset),
            "lamp-on" => Some(MaintenanceAction::LampOn),
            "lamp-off" => Some(MaintenanceAction::LampOff),
            _ => None,
        }
    }
}

/// How the comment of a maintenance capability names an action: the capability does nothing but
/// the action, it does it among other things, or it doesn't do it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceMatch {
    None,
    Among,
    Exactly,
}

/// The comment of a capability like `Maintenance` reads it: without the spaces around it and in
/// lower case, empty for none.
pub fn maintenance_comment(comment: Option<&str>) -> String {
    java_trim(comment.unwrap_or("")).to_lowercase()
}

// does the text start with the word, and end after it like `$` wants it?
fn ends_with_only(text: &str, word: &str) -> bool {
    text.strip_prefix(word)
        .is_some_and(|rest| is_dollar(&rest.chars().collect::<Vec<char>>(), 0))
}

/// Does the comment of a capability say that it does nothing but the action (`EXACTLY`:
/// `^(fixture |total |full )?reset( all)?$`, `^lamp on$`, `^lamp off$`)?
pub fn maintenance_matches_exactly(action: MaintenanceAction, comment: Option<&str>) -> bool {
    let comment = maintenance_comment(comment);
    match action {
        MaintenanceAction::Reset => ["fixture ", "total ", "full ", ""].iter().any(|prefix| {
            comment
                .strip_prefix(prefix)
                .is_some_and(|rest| ends_with_only(rest, "reset all") || ends_with_only(rest, "reset"))
        }),
        MaintenanceAction::LampOn => ends_with_only(&comment, "lamp on"),
        MaintenanceAction::LampOff => ends_with_only(&comment, "lamp off"),
    }
}

// the index after the word, if the text has it at the index
fn word_at(text: &[char], index: usize, word: &str) -> Option<usize> {
    let mut end = index;
    for character in word.chars() {
        if text.get(end) != Some(&character) {
            return None;
        }
        end += 1;
    }
    Some(end)
}

/// Does the comment of a capability say that it does the action, maybe among other things
/// (`AMONG`: `\breset\b`, `\blamp\s*on\b`, `\blamp\s*off\b`)?
pub fn maintenance_matches_among(action: MaintenanceAction, comment: Option<&str>) -> bool {
    let text: Vec<char> = maintenance_comment(comment).chars().collect();
    (0..text.len()).any(|start| {
        if !is_word_boundary(&text, start) {
            return false;
        }
        let end = match action {
            MaintenanceAction::Reset => word_at(&text, start, "reset"),
            MaintenanceAction::LampOn | MaintenanceAction::LampOff => {
                word_at(&text, start, "lamp").and_then(|mut index| {
                    while text.get(index).is_some_and(|character| is_java_space(*character)) {
                        index += 1;
                    }
                    word_at(
                        &text,
                        index,
                        if action == MaintenanceAction::LampOn {
                            "on"
                        } else {
                            "off"
                        },
                    )
                })
            }
        };
        end.is_some_and(|end| is_word_boundary(&text, end))
    })
}

/// How the comment of a maintenance capability names the action. `Maintenance.getCapability` takes
/// the first capability naming it exactly, or else the first one naming it among other things.
pub fn maintenance_match(action: MaintenanceAction, comment: Option<&str>) -> MaintenanceMatch {
    if maintenance_matches_exactly(action, comment) {
        MaintenanceMatch::Exactly
    } else if maintenance_matches_among(action, comment) {
        MaintenanceMatch::Among
    } else {
        MaintenanceMatch::None
    }
}

// ---------------------------------------------------------------------------------------------
// How bright a capability lights (DefaultDesignerService)
// ---------------------------------------------------------------------------------------------

/// The number of a brightness before its unit, e.g. of "50%" (`parseBrightnessNumber`). None as
/// well for a brightness which doesn't end with the unit (Java is never asked for one).
pub fn parse_brightness_number(brightness: &str, unit: &str) -> Option<f64> {
    parse_number(brightness.strip_suffix(unit)?)
}

/// How bright a capability lights, from 0 to 1, like the fixture library gives it: "off", "dark",
/// "bright", in percent or in lumens, compared to the brightest capability of the channel
/// (`getBrightness`).
pub fn get_brightness(brightness: Option<&str>, max_lumens: f64) -> Option<f64> {
    let brightness = brightness?;
    match brightness {
        "off" => Some(0.0),
        "dark" => Some(0.01),
        "bright" => Some(1.0),
        _ => {
            if brightness.ends_with('%') {
                return Some(parse_brightness_number(brightness, "%")? / 100.0);
            }
            if brightness.ends_with("lm") && max_lumens > 0.0 {
                return Some(parse_brightness_number(brightness, "lm")? / max_lumens);
            }
            None
        }
    }
}

/// The most lumens any of the capabilities of a channel gives (`getMaxLumens`), 0 for none.
pub fn get_max_lumens<'a>(capabilities: impl IntoIterator<Item = &'a FixtureCapability>) -> f64 {
    let mut max_lumens: f64 = 0.0;
    for capability in capabilities {
        for brightness in [
            &capability.brightness,
            &capability.brightness_start,
            &capability.brightness_end,
        ]
        .into_iter()
        .flatten()
        {
            if brightness.ends_with("lm") {
                if let Some(lumens) = parse_brightness_number(brightness, "lm") {
                    max_lumens = max_lumens.max(lumens);
                }
            }
        }
    }
    max_lumens
}

/// The brightness at the start and at the end of a capability: from off (0) to bright (1), if not
/// given (`getBrightnessRange`).
pub fn get_brightness_range(capability: &FixtureCapability, max_lumens: f64) -> [f64; 2] {
    let (start, end) = match capability.brightness.as_deref() {
        Some(brightness) => {
            let brightness = get_brightness(Some(brightness), max_lumens);
            (brightness, brightness)
        }
        None => (
            get_brightness(capability.brightness_start.as_deref(), max_lumens),
            get_brightness(capability.brightness_end.as_deref(), max_lumens),
        ),
    };
    [start.unwrap_or(0.0), end.unwrap_or(1.0)]
}
