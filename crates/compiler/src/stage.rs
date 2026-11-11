//! Where the fixtures and their pixels are on the stage, worked out once when a project is compiled.
//!
//! Everything is computed in `f64` with the same operations in the same order, so the results
//! don't depend on the machine: the sine and the arc tangent are worked out here and not left to
//! the platform. Nothing here reads the project model: every function takes plain arguments.

// the constants of fdlibm are kept digit for digit
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use std::collections::{BTreeSet, HashMap};

// ---- the sine over a cycle ----

const S1: f64 = -1.66666666666666324348e-01;
const S2: f64 = 8.33333333332248946124e-03;
const S3: f64 = -1.98412698298579493134e-04;
const S4: f64 = 2.75573137070700676789e-06;
const S5: f64 = -2.50507602534068634195e-08;
const S6: f64 = 1.58969099521155010221e-10;

const C1: f64 = 4.16666666666666019037e-02;
const C2: f64 = -1.38888888888741095749e-03;
const C3: f64 = 2.48015872894767294178e-05;
const C4: f64 = -2.75573143513906633035e-07;
const C5: f64 = 2.08757232129817482790e-09;
const C6: f64 = -1.13596475577881948265e-11;

/// π as the standard library has it, for everything but the arc tangent (which has its own,
/// with its low part).
const STD_PI: f64 = std::f64::consts::PI;

// the sine of an angle between -π/4 and π/4
fn sine(angle: f64, squared: f64) -> f64 {
    let cubed = squared * angle;
    let rest = S2 + squared * (S3 + squared * (S4 + squared * (S5 + squared * S6)));
    angle + cubed * (S1 + squared * rest)
}

// the cosine of an angle between -π/4 and π/4
fn cosine(squared: f64) -> f64 {
    let rest = squared * (C1 + squared * (C2 + squared * (C3 + squared * (C4 + squared * (C5 + squared * C6)))));
    1.0 - (0.5 * squared - squared * rest)
}

/// The sine over a cycle, sin(2π · position).
pub fn sin_cycle(position: f64) -> f64 {
    // the nearest quarter of the cycle and the angle left from it
    let quarter = (position * 4.0 + 0.5).floor();
    let angle = 2.0 * STD_PI * (position - quarter / 4.0);
    let squared = angle * angle;

    // cut off towards zero: NaN is 0
    match (((quarter % 4.0) + 4.0) % 4.0) as i32 {
        0 => sine(angle, squared),
        1 => cosine(squared),
        2 => -sine(angle, squared),
        _ => -cosine(squared),
    }
}

// ---- the arc tangent ----

const ATAN_HI: [f64; 4] = [
    4.63647609000806093515e-1,
    7.85398163397448278999e-1,
    9.82793723247329054082e-1,
    1.570796326794896558e0,
];
const ATAN_LO: [f64; 4] = [
    2.26987774529616870924e-17,
    3.06161699786838301793e-17,
    1.39033110312309984516e-17,
    6.12323399573676603587e-17,
];

const AT: [f64; 11] = [
    3.33333333333329318027e-1,
    -1.99999999998764832476e-1,
    1.42857142725034663711e-1,
    -1.1111110405462355788e-1,
    9.09088713343650656196e-2,
    -7.69187620504482999495e-2,
    6.66107313738753120669e-2,
    -5.83357013379057348645e-2,
    4.97687799461593236017e-2,
    -3.6531572744216915527e-2,
    1.62858201153657823623e-2,
];

const PI: f64 = 3.1415926535897931160e0;
const PI_LO: f64 = 1.2246467991473531772e-16;

/// The arc tangent of a number, between -π/2 and π/2.
pub fn atan(value: f64) -> f64 {
    let sign: f64 = if value < 0.0 { -1.0 } else { 1.0 };
    let mut x = value.abs();
    let id: Option<usize>;

    if x >= 7.378697629483820646e19 {
        // beyond 2^66
        return sign * (ATAN_HI[3] + ATAN_LO[3]);
    }

    if x < 0.4375 {
        if x < 1.862645149230957e-9 {
            // below 2^-29
            return value;
        }

        id = None;
    } else if x < 1.1875 {
        if x < 0.6875 {
            id = Some(0);
            x = (2.0 * x - 1.0) / (2.0 + x);
        } else {
            id = Some(1);
            x = (x - 1.0) / (x + 1.0);
        }
    } else if x < 2.4375 {
        id = Some(2);
        x = (x - 1.5) / (1.0 + 1.5 * x);
    } else {
        id = Some(3);
        x = -1.0 / x;
    }

    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));

    match id {
        None => value - value * (s1 + s2),
        Some(id) => sign * (ATAN_HI[id] - (x * (s1 + s2) - ATAN_LO[id] - x)),
    }
}

/// The angle of the point (x, y) from the x axis, between -π and π.
pub fn atan2(y: f64, x: f64) -> f64 {
    if x == 1.0 {
        return atan(y);
    }

    if y == 0.0 {
        return if x < 0.0 {
            if y.to_bits() != 0 {
                -PI
            } else {
                PI
            }
        } else {
            y
        };
    }

    if x == 0.0 {
        return if y < 0.0 { -PI / 2.0 } else { PI / 2.0 };
    }

    let z = atan((y / x).abs());

    if x > 0.0 {
        return if y < 0.0 { -z } else { z };
    }

    if y < 0.0 {
        z - PI_LO - PI
    } else {
        PI - (z - PI_LO)
    }
}

// ---- the lower and the higher of two numbers ----

// the lower one: NaN if either is NaN, and -0.0 is below 0.0 (f64::min does neither)
fn min_or_nan(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }

    if a == 0.0 && b == 0.0 && b.is_sign_negative() {
        return b;
    }

    if a <= b {
        a
    } else {
        b
    }
}

// the higher one: NaN if either is NaN, and 0.0 is above -0.0
fn max_or_nan(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }

    if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        return b;
    }

    if a >= b {
        a
    } else {
        b
    }
}

// ---- where the fixtures hang ----

/// Where the head of a moving head turns, below the place it is fastened at.
pub const HEAD_OFFSET_CM: f64 = 5.4;

/// Where a fixture hangs: its place on the stage (in cm, y up) and how it is turned (in degrees,
/// x, y and z one after the other).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StagePlacement {
    pub position: [f64; 3],
    pub rotation: [f64; 3],
}

impl StagePlacement {
    /// The placement of a fixture [`stage_placements`] has none for: all zeros, as
    /// [`part_stage_position`] takes it.
    pub const ORIGIN: StagePlacement = StagePlacement {
        position: [0.0; 3],
        rotation: [0.0; 3],
    };
}

/// Where a moving head points, in degrees from the middle of its ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aim {
    pub pan: f64,
    pub tilt: f64,
}

/// What [`stage_placements`] reads of a fixture: its uuid, its positioning (`topFront`,
/// `bottomFront`, `topBack`, `bottomBack` or `manual`; anything else is placed nowhere) and where it
/// was placed and turned by hand (`positionX/Y/Z`, `rotationX/Y/Z`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageFixture<'a> {
    pub uuid: &'a str,
    pub positioning: &'a str,
    pub position: [f64; 3],
    pub rotation: [f64; 3],
}

/// The stage of a project, in cm (`stageWidthCm`, `stageHeightCm`, `stageDepthCm`,
/// `stageFloorHeightCm`). The default is the one of a project which doesn't say: 600, 350, 600, 30.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageSize {
    pub width_cm: f64,
    pub height_cm: f64,
    pub depth_cm: f64,
    pub floor_height_cm: f64,
}

impl Default for StageSize {
    fn default() -> Self {
        StageSize {
            width_cm: 600.0,
            height_cm: 350.0,
            depth_cm: 600.0,
            floor_height_cm: 30.0,
        }
    }
}

// the index of a quarter turn in the tables of sin_degrees and cos_degrees (cut off towards zero:
// NaN is 0)
fn quarter_index(degrees: f64) -> usize {
    ((((degrees / 90.0) % 4.0) + 4.0) % 4.0) as i32 as usize
}

/// The sine of an angle in degrees, exact on the quarters.
pub fn sin_degrees(degrees: f64) -> f64 {
    if degrees % 90.0 == 0.0 {
        return [0.0, 1.0, 0.0, -1.0][quarter_index(degrees)];
    }

    sin_cycle(degrees / 360.0)
}

/// The cosine of an angle in degrees, exact on the quarters.
pub fn cos_degrees(degrees: f64) -> f64 {
    if degrees % 90.0 == 0.0 {
        return [1.0, 0.0, -1.0, 0.0][quarter_index(degrees)];
    }

    sin_cycle(degrees / 360.0 + 0.25)
}

// the product of two matrices
fn multiply(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut result = [[0.0; 3]; 3];

    for row in 0..3 {
        for column in 0..3 {
            result[row][column] = a[row][0] * b[0][column] + a[row][1] * b[1][column] + a[row][2] * b[2][column];
        }
    }

    result
}

/// The matrix turning a fixture by the passed degrees around x, y and z, one after the other.
pub fn get_rotation_matrix(rotation: [f64; 3]) -> [[f64; 3]; 3] {
    let sx = sin_degrees(rotation[0]);
    let cx = cos_degrees(rotation[0]);
    let sy = sin_degrees(rotation[1]);
    let cy = cos_degrees(rotation[1]);
    let sz = sin_degrees(rotation[2]);
    let cz = cos_degrees(rotation[2]);

    let x = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let y = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let z = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];

    multiply(&multiply(&x, &y), &z)
}

/// Where the fixtures of a project hang, by their uuid: a fixture placed by hand where it was
/// placed, the others spread evenly over the width of the stage in their row, in the order of
/// the preset fixtures.
///
/// `fixtures` are `project.fixtures` in order, `preset_fixture_uuids` the `fixtureUuid` of each of
/// `project.presetFixtures` in order (one entry per pixel of a fixture, as they are stored). A
/// fixture that is neither placed by hand nor among the preset fixtures has no placement.
pub fn stage_placements(
    fixtures: &[StageFixture],
    preset_fixture_uuids: &[&str],
    stage: &StageSize,
) -> HashMap<String, StagePlacement> {
    let mut placements: HashMap<String, StagePlacement> = HashMap::new();
    // of two fixtures with the same uuid, the later one is looked up
    let mut by_uuid: HashMap<&str, &StageFixture> = HashMap::new();

    for fixture in fixtures {
        by_uuid.insert(fixture.uuid, fixture);

        if fixture.positioning == "manual" {
            placements.insert(
                fixture.uuid.to_string(),
                StagePlacement {
                    position: fixture.position,
                    rotation: fixture.rotation,
                },
            );
        }
    }

    let front = stage.depth_cm / 2.0 - 70.0;
    let back = -stage.depth_cm / 2.0 + 70.0;
    let top = stage.height_cm + stage.floor_height_cm;
    let floor = stage.floor_height_cm;

    // positioning, y, z, hanging
    let rows: [(&str, f64, f64, bool); 4] = [
        ("topFront", top, front, true),
        ("bottomFront", floor, front, false),
        ("topBack", top, back, true),
        ("bottomBack", floor, back, false),
    ];

    for (positioning, y, z, hanging) in rows {
        // the fixtures of the row, each once (the pixels of a fixture follow each other)
        let mut count: i32 = 0;
        let mut last_fixture_uuid: Option<&str> = None;

        for &uuid in preset_fixture_uuids {
            if let Some(fixture) = by_uuid.get(uuid) {
                if fixture.positioning == positioning && Some(uuid) != last_fixture_uuid {
                    count += 1;
                }
            }

            last_fixture_uuid = Some(uuid);
        }

        let mut index: i32 = 0;
        last_fixture_uuid = None;

        for &uuid in preset_fixture_uuids {
            if let Some(fixture) = by_uuid.get(uuid) {
                if fixture.positioning == positioning {
                    if Some(uuid) != last_fixture_uuid {
                        index += 1;
                    }

                    let x = -stage.width_cm / 2.0 + (stage.width_cm / f64::from(count + 1)) * f64::from(index);

                    // a fixture hanging from above hangs below the truss, one on the floor stands on it upside down
                    placements.insert(
                        fixture.uuid.to_string(),
                        StagePlacement {
                            position: [x, if hanging { y - 13.0 } else { y + 13.0 }, z],
                            rotation: [if hanging { 0.0 } else { 180.0 }, 0.0, 0.0],
                        },
                    );
                }
            }

            last_fixture_uuid = Some(uuid);
        }
    }

    placements
}

/// Where a moving head hanging at the passed place points at the passed spot (in cm), in degrees
/// from the middle of its ranges.
pub fn get_aim_degrees(placement: &StagePlacement, target: [f64; 3]) -> Aim {
    let matrix = get_rotation_matrix(placement.rotation);
    let mut origin = [0.0; 3];
    let mut world = [0.0; 3];
    let mut local = [0.0; 3];

    // where the head turns, and the spot as seen from it, in the directions of the fixture
    for row in 0..3 {
        origin[row] = placement.position[row] - matrix[row][1] * HEAD_OFFSET_CM;
        world[row] = target[row] - origin[row];
    }

    for column in 0..3 {
        local[column] = matrix[0][column] * world[0] + matrix[1][column] * world[1] + matrix[2][column] * world[2];
    }

    let length = (local[0] * local[0] + local[1] * local[1] + local[2] * local[2]).sqrt();

    if length == 0.0 {
        return Aim { pan: 0.0, tilt: 0.0 };
    }

    let dx = local[0] / length;
    let dy = local[1] / length;
    let dz = local[2] / length;

    // the head points at (-sin(tilt) sin(pan), -cos(tilt), -sin(tilt) cos(pan))
    let horizontal = (dx * dx + dz * dz).sqrt();
    let tilt = atan2(horizontal, -dy);
    let pan = if horizontal < 1e-9 { 0.0 } else { atan2(-dx, -dz) };
    let other_pan = if pan > 0.0 { pan - STD_PI } else { pan + STD_PI };

    if other_pan.abs() < pan.abs() {
        return Aim {
            pan: (other_pan * 180.0) / STD_PI,
            tilt: (-tilt * 180.0) / STD_PI,
        };
    }

    Aim {
        pan: (pan * 180.0) / STD_PI,
        tilt: (tilt * 180.0) / STD_PI,
    }
}

// ---- the fan over the stage ----

/// Where each of the parts at the passed places on the stage is in a fan in the passed order
/// (`front-back`, `bottom-top`, anything else from the left to the right), from 0 to 1, NaN for
/// all of them if they are all at the same place.
pub fn stage_fan_places(positions: &[[f64; 3]], order: &str) -> Vec<f64> {
    let coordinates: Vec<f64> = positions
        .iter()
        .map(|position| match order {
            "front-back" => -position[2],
            "bottom-top" => position[1],
            _ => position[0],
        })
        .collect();

    let mut min = f64::MAX;
    let mut max = -f64::MAX;

    for &coordinate in &coordinates {
        min = min_or_nan(min, coordinate);
        max = max_or_nan(max, coordinate);
    }

    coordinates
        .iter()
        .map(|&coordinate| {
            if max > min {
                (coordinate - min) / (max - min)
            } else {
                f64::NAN
            }
        })
        .collect()
}

/// Does the preset fan its fixtures by where they are on the stage?
pub fn is_stage_fan_order(order: &str) -> bool {
    order == "left-right" || order == "front-back" || order == "bottom-top"
}

// ---- where a pixel is ----

/// Where a pixel is in a picture: x and y from 0 to 1 (the middle of the pixel), its column and
/// row, how many there are, and the aspect of the picture (the longer side being 1). The default
/// is the one pixel of a picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelPlace {
    pub x: f64,
    pub y: f64,
    pub column: i32,
    pub row: i32,
    pub columns: i32,
    pub rows: i32,
    pub aspect_x: f64,
    pub aspect_y: f64,
}

impl Default for PixelPlace {
    fn default() -> Self {
        PixelPlace {
            x: 0.5,
            y: 0.5,
            column: 0,
            row: 0,
            columns: 1,
            rows: 1,
            aspect_x: 1.0,
            aspect_y: 1.0,
        }
    }
}

// ---- where a part is in its fixture ----

/// The number of pixels a matrix has on x and y, as (columns, rows), 0 where it doesn't say.
///
/// `pixel_keys` is `matrix.pixelKeys` (layers of rows of keys; `None` if null), which wins over
/// `pixel_count`, `matrix.pixelCount` (`None` if null, a null entry `None`).
pub fn matrix_size<S>(pixel_keys: Option<&[Vec<Vec<S>>]>, pixel_count: Option<&[Option<i32>]>) -> (i32, i32) {
    if let Some(layers) = pixel_keys {
        let mut rows: i32 = 0;
        let mut columns: i32 = 0;

        for layer in layers {
            rows = rows.max(layer.len() as i32);

            for row in layer {
                columns = columns.max(row.len() as i32);
            }
        }

        return (columns, rows);
    }

    let pixel_count = pixel_count.unwrap_or(&[]);
    let columns = pixel_count.first().copied().flatten().unwrap_or(0);
    let rows = pixel_count.get(1).copied().flatten().unwrap_or(0);

    (columns, rows)
}

/// What [`pixel_place`] knows of the pixel of a part: a pixel of its own at its column and row
/// (counted from 1), or a pixel group with the (x, y) of each of its pixels (what
/// [`get_pixels_for_key_or_group`](crate::profile::get_pixels_for_key_or_group) resolves, in its
/// order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PixelPosition<'a> {
    Own { x: i32, y: i32 },
    Group(&'a [(i32, i32)]),
}

impl<'a> PixelPosition<'a> {
    /// A pixel with `x > 0` is at a place of its own, any other is a group and takes the passed
    /// pixels (which are not looked at otherwise).
    pub fn of(x: i32, y: i32, group: &'a [(i32, i32)]) -> PixelPosition<'a> {
        if x > 0 {
            PixelPosition::Own { x, y }
        } else {
            PixelPosition::Group(group)
        }
    }
}

/// Where a part of a fixture is in it: a pixel in its column and row of the matrix, a pixel group
/// in the middle of its pixels (in the middle of the matrix if it has none), and a part without a
/// pixel or of a profile without a matrix as the one pixel of the fixture.
///
/// `matrix_size` is what [`matrix_size`] gives for the matrix of the profile, `None` if the
/// profile has no matrix. `pixel` is `None` for a part without a pixel.
pub fn pixel_place(matrix_size: Option<(i32, i32)>, pixel: Option<PixelPosition>) -> PixelPlace {
    let mut place = PixelPlace::default();

    let (Some(pixel), Some(size)) = (pixel, matrix_size) else {
        return place;
    };

    let columns = (if size.0 == 0 { 1 } else { size.0 }).max(1);
    let rows = (if size.1 == 0 { 1 } else { size.1 }).max(1);
    let mut column = f64::from(columns - 1) / 2.0;
    let mut row = f64::from(rows - 1) / 2.0;

    match pixel {
        PixelPosition::Own { x, y } => {
            column = f64::from(x.wrapping_sub(1));
            row = f64::from(y.wrapping_sub(1));
        }
        PixelPosition::Group(group_pixels) => {
            if !group_pixels.is_empty() {
                let mut column_sum = 0.0;
                let mut row_sum = 0.0;

                for &(x, y) in group_pixels {
                    column_sum += f64::from(x.wrapping_sub(1));
                    row_sum += f64::from(y.wrapping_sub(1));
                }

                column = column_sum / group_pixels.len() as i32 as f64;
                row = row_sum / group_pixels.len() as i32 as f64;
            }
        }
    }

    let longest = columns.max(rows);

    place.x = (column + 0.5) / f64::from(columns);
    place.y = (row + 0.5) / f64::from(rows);
    place.column = column.floor() as i32;
    place.row = row.floor() as i32;
    place.columns = columns;
    place.rows = rows;
    place.aspect_x = f64::from(columns) / f64::from(longest);
    place.aspect_y = f64::from(rows) / f64::from(longest);

    place
}

// ---- where the pixels are on the stage ----

/// The room a pixel takes on a fixture which does not say how large it is, in cm.
pub const DEFAULT_PIXEL_PITCH_CM: f64 = 8.0;
/// How deep a fixture is which does not say so, in cm.
pub const DEFAULT_PIXEL_DEPTH_CM: f64 = 7.0;
/// How far the top of the housing of a fixture hanging from the truss is above where it is placed, in cm.
pub const PIXEL_HOUSING_TOP_CM: f64 = 8.0;

/// The width, height and depth of the housing of a fixture (in cm): as large as its profile says
/// (in mm), or as large as its pixels need.
///
/// `dimensions` is `profile.physical.dimensions` (width, height, depth in mm), `None` if the
/// profile has no physical entry or that has no dimensions. A missing entry, a 0 and a NaN are
/// the size the pixels need.
pub fn housing_size(dimensions: Option<&[f64]>, columns: i32, rows: i32) -> [f64; 3] {
    let dimensions = dimensions.unwrap_or(&[]);

    [
        or_else(dimension(dimensions, 0), f64::from(columns) * DEFAULT_PIXEL_PITCH_CM),
        or_else(dimension(dimensions, 1), f64::from(rows) * DEFAULT_PIXEL_PITCH_CM),
        or_else(dimension(dimensions, 2), DEFAULT_PIXEL_DEPTH_CM),
    ]
}

// a dimension of the profile in cm, 0 for one it doesn't give
fn dimension(dimensions: &[f64], index: usize) -> f64 {
    if index < dimensions.len() {
        dimensions[index] / 10.0
    } else {
        0.0
    }
}

// a size the profile does not give (or gives as 0) is the one the pixels need
fn or_else(value: f64, otherwise: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        otherwise
    } else {
        value
    }
}

/// Where the housing is below the place its fixture is fastened at (in cm, y up): 0 for a fixture
/// placed by hand.
pub fn housing_offset(positioning: &str, height: f64) -> f64 {
    if positioning == "manual" {
        0.0
    } else {
        PIXEL_HOUSING_TOP_CM - height / 2.0
    }
}

/// Does the fixture stand on the floor (`bottomFront` or `bottomBack`)?
pub fn is_on_the_floor(positioning: &str) -> bool {
    positioning == "bottomFront" || positioning == "bottomBack"
}

/// Where the LED of a part of a fixture is on the stage, in cm: on the front of the housing of the
/// passed size, the housing as the fixture hangs.
pub fn pixel_position(place: &PixelPlace, size: [f64; 3], placement: &StagePlacement, positioning: &str) -> [f64; 3] {
    let x = (place.x - 0.5) * size[0];
    let y = (0.5 - place.y) * size[1];
    let z = size[2] / 2.0;
    let offset = housing_offset(positioning, size[1]);

    // in the frame of the fixture, which is turned upside down on the floor
    let local = if is_on_the_floor(positioning) {
        [x, offset - y, -z]
    } else {
        [x, offset + y, z]
    };
    let matrix = get_rotation_matrix(placement.rotation);
    let mut position = [0.0; 3];

    for row in 0..3 {
        position[row] =
            placement.position[row] + matrix[row][0] * local[0] + matrix[row][1] * local[1] + matrix[row][2] * local[2];
    }

    position
}

/// Where a part of a fixture is on the stage, in cm.
///
/// `place` is the [`pixel_place`] of the part, whose columns and rows size a housing without
/// dimensions (see [`housing_size`]); `placement` is the entry of [`stage_placements`] for the
/// fixture, `None` if it has none (the fixture is then taken at the origin, not turned).
pub fn part_stage_position(
    place: &PixelPlace,
    dimensions: Option<&[f64]>,
    placement: Option<&StagePlacement>,
    positioning: &str,
) -> [f64; 3] {
    let placement = placement.unwrap_or(&StagePlacement::ORIGIN);

    pixel_position(
        place,
        housing_size(dimensions, place.columns, place.rows),
        placement,
        positioning,
    )
}

/// A coordinate rounded to a millimeter, which tells the columns and rows of the picture apart.
pub fn millimeters(value: f64) -> i64 {
    // cut off towards zero, saturating: NaN is 0, beyond the range the nearest whole number
    (value * 10.0 + 0.5).floor() as i64
}

/// The places of the LEDs in the picture they make up together, seen from the front (x to the
/// right, y up) or from above (`top` as the view, the back of the stage up; anything else is the
/// front): one place for each position, in the same order.
pub fn pixel_places(positions: &[[f64; 3]], view: &str) -> Vec<PixelPlace> {
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(positions.len());
    let mut column_values: BTreeSet<i64> = BTreeSet::new();
    let mut row_values: BTreeSet<i64> = BTreeSet::new();
    let mut min_x = f64::MAX;
    let mut max_x = -f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_y = -f64::MAX;

    for position in positions {
        let point = if view == "top" {
            [position[0], -position[2]]
        } else {
            [position[0], position[1]]
        };
        points.push(point);
        column_values.insert(millimeters(point[0]));
        row_values.insert(millimeters(point[1]).wrapping_neg());
        min_x = min_or_nan(min_x, point[0]);
        max_x = max_or_nan(max_x, point[0]);
        min_y = min_or_nan(min_y, point[1]);
        max_y = max_or_nan(max_y, point[1]);
    }

    let column_of = indices(&column_values);
    let row_of = indices(&row_values);
    let columns = column_values.len() as i32;
    let rows = row_values.len() as i32;
    let width = if columns > 1 {
        (max_x - min_x) * f64::from(columns) / f64::from(columns - 1)
    } else {
        0.0
    };
    let height = if rows > 1 {
        (max_y - min_y) * f64::from(rows) / f64::from(rows - 1)
    } else {
        0.0
    };
    let longest = max_or_nan(width, height);

    points
        .iter()
        .map(|point| PixelPlace {
            column: column_of[&millimeters(point[0])],
            row: row_of[&millimeters(point[1]).wrapping_neg()],
            columns,
            rows,
            x: if columns > 1 {
                (0.5 + ((point[0] - min_x) / (max_x - min_x)) * f64::from(columns - 1)) / f64::from(columns)
            } else {
                0.5
            },
            y: if rows > 1 {
                (0.5 + ((max_y - point[1]) / (max_y - min_y)) * f64::from(rows - 1)) / f64::from(rows)
            } else {
                0.5
            },
            aspect_x: if longest > 0.0 { width / longest } else { 1.0 },
            aspect_y: if longest > 0.0 { height / longest } else { 1.0 },
        })
        .collect()
}

/// The place of each of the values, from the lowest to the highest.
pub fn indices(values: &BTreeSet<i64>) -> HashMap<i64, i32> {
    let mut indices = HashMap::new();

    for &value in values {
        let index = indices.len() as i32;
        indices.insert(value, index);
    }

    indices
}
