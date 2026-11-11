//! The compiler: a project of the designer in, a compiled show for the core out.
//!
//! Everything fixed is worked out here, once, in double precision and in the order Rocket Show works it out in: the
//! parts of the fixtures with their channels and what each capability does, where the fixtures hang and where a spot
//! points each of them, the place of each fixture in the chase, the fan and the pixel picture of each preset, when
//! the clips of a timeline play. Names become numbers and times whole nanoseconds; a float of the project is rounded
//! to single precision once, here.

use std::collections::{BTreeMap, HashMap, HashSet};

use engine_core::engine::{memory_words, MAX_ORDER, MAX_STEP_CAPS};
use engine_core::format::{self as f, Field, List, Writer};
use engine_pixel as px;

use crate::model::{
    Composition, Effect, EffectCurve, Fixture, FixtureCapability, FixtureCapabilityType as Type,
    FixtureCapabilityValue, FixtureChannel, FixtureMode, FixtureProfile, FixtureWheel, FixtureWheelSlotType, Preset,
    PresetFixture, PresetStep, Project, Scene, Wheel,
};
use crate::profile::{self, MaintenanceAction, MaintenanceMatch, Pixel};
use crate::stage::{self, PixelPlace, PixelPosition, StageFixture, StagePlacement, StageSize};

const VIRTUAL_DIMMER: &str = "$virtualDimmer";

/// What a compiled show names by a number: the uuids and names of the project, by their number.
#[derive(Debug, Clone, Default)]
pub struct Index {
    /// the uuid of each fixture
    pub fixtures: Vec<String>,
    /// each part of the fixtures: the number of its fixture, its pixel (none for the fixture itself) and its channels
    pub parts: Vec<PartIndex>,
    pub presets: Vec<String>,
    /// the uuids of the steps of each preset
    pub steps: Vec<Vec<String>>,
    pub scenes: Vec<String>,
    /// the uuid and the name of each composition
    pub compositions: Vec<(String, String)>,
    /// the uuid of each universe
    pub universes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PartIndex {
    pub fixture: u32,
    pub pixel_key: Option<String>,
    pub channels: Vec<String>,
}

impl Index {
    fn find(list: &[String], uuid: &str) -> Option<u32> {
        list.iter()
            .position(|candidate| candidate == uuid)
            .map(|index| index as u32)
    }

    pub fn fixture(&self, uuid: &str) -> Option<u32> {
        Self::find(&self.fixtures, uuid)
    }

    pub fn preset(&self, uuid: &str) -> Option<u32> {
        Self::find(&self.presets, uuid)
    }

    pub fn scene(&self, uuid: &str) -> Option<u32> {
        Self::find(&self.scenes, uuid)
    }

    pub fn step(&self, preset: u32, uuid: &str) -> Option<u32> {
        Self::find(self.steps.get(preset as usize)?, uuid)
    }

    pub fn composition(&self, uuid: &str) -> Option<u32> {
        self.compositions
            .iter()
            .position(|(candidate, _)| candidate == uuid)
            .map(|index| index as u32)
    }

    pub fn composition_named(&self, name: &str) -> Option<u32> {
        self.compositions
            .iter()
            .position(|(_, candidate)| candidate == name)
            .map(|index| index as u32)
    }

    pub fn universe(&self, uuid: &str) -> Option<u32> {
        Self::find(&self.universes, uuid)
    }
}

/// What to compile for.
#[derive(Debug, Clone)]
pub struct Options<'a> {
    /// what the engine the show is for can play (see `engine_core::format::feature`)
    pub features: u32,
    /// the project JSON, to carry it in the show (the engine never reads it)
    pub project_json: Option<&'a [u8]>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options {
            features: f::feature::SUPPORTED,
            project_json: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Compiled {
    /// the compiled show
    pub show: Vec<u8>,
    pub index: Index,
    /// the features the show needs
    pub features: u32,
    /// what was left out, and why
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompileError {
    /// the project can't be read
    Project(String),
    /// the show needs something the engine it is compiled for can't do: the bits of the features missing
    Unsupported(u32),
    /// the show is larger than the format (or the engine) holds
    Limit(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Project(message) => write!(formatter, "the project can't be read: {message}"),
            CompileError::Unsupported(features) => write!(
                formatter,
                "the show needs features the engine doesn't have: {}",
                feature_names(*features).join(", ")
            ),
            CompileError::Limit(message) => write!(formatter, "the show is too large: {message}"),
        }
    }
}

/// The names of the features of a set of feature bits.
pub fn feature_names(features: u32) -> Vec<&'static str> {
    let names = [
        (f::feature::STEPS, "presets with steps"),
        (f::feature::STEP_ORDERS, "steps played in another order"),
        (f::feature::CURVES, "effect curves"),
        (f::feature::MOVEMENTS, "movements"),
        (f::feature::PIXELS, "pixel effects"),
        (f::feature::PIXEL_BITMAPS, "texts and images"),
        (f::feature::AUDIO, "effects following the audio"),
        (f::feature::FANS, "fans"),
        (f::feature::AIMS, "moving heads aimed at a spot"),
        (f::feature::TIMELINES, "timelines"),
        (f::feature::BEATS, "steps and curves following the beat"),
    ];

    let mut result: Vec<&'static str> = names
        .iter()
        .filter(|(bit, _)| features & bit != 0)
        .map(|(_, name)| *name)
        .collect();

    if features & !f::feature::SUPPORTED != 0 {
        result.push("features of a newer compiler");
    }

    result
}

// ---- numbers ----

fn ns(millis: f64) -> i64 {
    if millis.is_nan() {
        return 0;
    }

    (millis * 1e6).round() as i64
}

fn fix(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }

    (value * 4_294_967_296.0).round() as i64
}

/// Math.round of Java
fn java_round(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }

    (value + 0.5).floor() as i64
}

fn is_number(value: Option<f64>) -> bool {
    matches!(value, Some(value) if !value.is_nan())
}

fn is_empty(text: &Option<String>) -> bool {
    text.as_deref().map(str::is_empty).unwrap_or(true)
}

fn empty_to_none(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.is_empty())
}

fn type_of(t: Option<Type>) -> u8 {
    t.map(Type::as_u8).unwrap_or(f::ty::NONE)
}

/// The number of the shape of a fade or a transition (see `engine_core::format::transition`).
pub fn curve_of(name: &str) -> u8 {
    match name {
        "ease-in" => f::transition::EASE_IN,
        "ease-out" => f::transition::EASE_OUT,
        "ease-in-out" => f::transition::EASE_IN_OUT,
        "snap" => f::transition::SNAP,
        _ => f::transition::LINEAR,
    }
}

/// The names the engine compares, each with its number.
#[derive(Default)]
struct Names {
    numbers: HashMap<String, u16>,
}

impl Names {
    fn new() -> Self {
        let mut names = Names::default();
        names.numbers.insert(String::new(), f::NAME_EMPTY);
        names
    }

    fn of(&mut self, name: Option<&str>) -> Result<u16, CompileError> {
        let name = match name {
            Some(name) => name,
            None => return Ok(f::NAME_NONE),
        };

        if let Some(number) = self.numbers.get(name) {
            return Ok(*number);
        }

        let number = self.numbers.len() + 1;

        if number > u16::MAX as usize {
            return Err(CompileError::Limit(
                "more than 65535 names of profiles, channels, wheels and blades".to_string(),
            ));
        }

        self.numbers.insert(name.to_string(), number as u16);
        Ok(number as u16)
    }
}

// ---- the parts of the fixtures (the cached fixtures of Rocket Show) ----

#[derive(Clone)]
struct CachedCap<'p> {
    cap: FixtureCapability,
    wheel_name: Option<String>,
    wheel: Option<&'p FixtureWheel>,
    wheel_is_color: bool,
    center: f64,
}

#[derive(Clone)]
struct CachedChan<'p> {
    channel: FixtureChannel,
    name: String,
    pixel_key: Option<String>,
    caps: Vec<CachedCap<'p>>,
    default: Option<f64>,
    max: f64,
    color_wheel: bool,
}

struct CachedPart<'p> {
    fixture: usize,
    /// which part of which mode of which profile it is: the parts of two fixtures of the same kind are the same
    kind: (usize, usize, usize),
    profile: &'p FixtureProfile,
    mode: &'p FixtureMode,
    pixel: Option<Pixel>,
    channels: Vec<CachedChan<'p>>,
}

fn has_range(cap: &FixtureCapability) -> bool {
    cap.dmx_range.as_ref().map(|range| range.len() == 2).unwrap_or(false)
}

fn cached_cap<'p>(
    capability: &FixtureCapability,
    channel_name: &str,
    profile: &'p FixtureProfile,
    multiplier: f64,
) -> CachedCap<'p> {
    let cap = FixtureChannel::get_capability_in_full_resolution(capability, multiplier).into_owned();
    let mut wheel_name = None;
    let mut wheel = None;
    let mut wheel_is_color = false;

    if let Some(_slot) = capability.slot_number {
        // there is a wheel connected to this capability
        wheel_name = match &capability.wheel {
            Some(Wheel::Name(name)) => Some(name.clone()),
            // a capability of several wheels names them in a list: the first one
            Some(Wheel::Names(names)) => names.first().cloned(),
            _ => Some(channel_name.to_string()),
        };

        wheel = wheel_name.as_deref().and_then(|name| profile.wheels.get(name));
        wheel_is_color = wheel
            .map(|wheel| {
                wheel
                    .slots
                    .iter()
                    .any(|slot| slot.type_ == Some(FixtureWheelSlotType::Color))
            })
            .unwrap_or(false);
    }

    let center = match cap.dmx_range.as_deref() {
        // (worked out in single precision, like Rocket Show does)
        Some([low, high]) => ((low.wrapping_add(*high)) as f32 / 2f32).floor() as i32 as f64,
        _ => 0.0,
    };

    CachedCap {
        cap,
        wheel_name,
        wheel,
        wheel_is_color,
        center,
    }
}

fn cached_chan<'p>(
    channel: &FixtureChannel,
    template_name: &str,
    pixel_key: Option<&str>,
    profile: &'p FixtureProfile,
) -> CachedChan<'p> {
    let name = profile::get_channel_name_with_pixel_key(template_name, pixel_key);
    let multiplier = channel.resolution_multiplier();

    let caps: Vec<CachedCap> = if let Some(capability) = &channel.capability {
        vec![cached_cap(capability, &name, profile, multiplier)]
    } else if let Some(capabilities) = &channel.capabilities {
        capabilities
            .iter()
            .map(|capability| cached_cap(capability, &name, profile, multiplier))
            .collect()
    } else {
        Vec::new()
    };

    let color_wheel = caps.iter().any(|cap| cap.wheel_is_color);

    CachedChan {
        channel: channel.clone(),
        name,
        pixel_key: pixel_key.map(str::to_string),
        default: profile::get_default_value_by_channel(channel),
        max: profile::get_max_value_by_channel(channel),
        caps,
        color_wheel,
    }
}

/// the channels of a part of a fixture: of the fixture itself (no pixel key) or of one of its pixels
fn cached_channels<'p>(
    profile: &'p FixtureProfile,
    mode: &'p FixtureMode,
    pixel_key: Option<&str>,
) -> Vec<CachedChan<'p>> {
    let mut channels = Vec::new();
    let pixel_key = empty_to_none(pixel_key);

    // the pixels and the pixel groups, which the template channels can be named after
    let mut keys_and_groups: Vec<String> = profile::get_all_pixel_keys(profile)
        .into_iter()
        .map(|pixel| pixel.key)
        .collect();
    keys_and_groups.extend(profile::get_pixel_group_names(profile));

    for channel in &mode.channels {
        if !channel.name.is_empty() && pixel_key.is_none() {
            // direct reference to a channel (only the coarse channel is added)
            if let Some((name, available)) = profile
                .available_channels
                .iter()
                .find(|(name, _)| *name == channel.name)
            {
                channels.push(cached_chan(available, name, None, profile));
                continue;
            }

            // check the template channels, if not found in the available channels
            for (template_name, template) in profile.template_channels.iter() {
                for key in &keys_and_groups {
                    let name = profile::get_channel_name_with_pixel_key(template_name, Some(key));

                    if channel.name == name {
                        channels.push(cached_chan(template, &name, Some(key), profile));
                    }
                }
            }
        } else {
            // reference a channel through a pixel matrix
            let pixels = profile::get_pixels_in_order(profile, &channel.repeat_for);
            let per_pixel = channel.channel_order.as_deref() == Some("perPixel");
            let per_channel = channel.channel_order.as_deref() == Some("perChannel");

            let mut add = |template_name: &str, pixel: &Pixel| {
                if Some(pixel.key.as_str()) == pixel_key {
                    for (name, template) in profile.template_channels.iter() {
                        if name == template_name {
                            channels.push(cached_chan(template, name, Some(&pixel.key), profile));
                        }
                    }
                }
            };

            if per_pixel {
                for pixel in &pixels {
                    for template_name in &channel.template_channels {
                        add(template_name, pixel);
                    }
                }
            } else if per_channel {
                for template_name in &channel.template_channels {
                    for pixel in &pixels {
                        add(template_name, pixel);
                    }
                }
            }
        }
    }

    channels
}

fn has_type(part: &CachedPart, t: Type) -> bool {
    part.channels
        .iter()
        .any(|channel| channel.caps.iter().any(|cap| cap.cap.type_ == Some(t)))
}

fn brightness_range(channel: &CachedChan, cap: &FixtureCapability) -> [f64; 2] {
    profile::get_brightness_range(cap, profile::get_max_lumens(channel.caps.iter().map(|cap| &cap.cap)))
}

/// The value of a channel lighting with a brightness, through the dimmers of the channel (see the core).
fn brightness_value(channel: &CachedChan, test: impl Fn(&CachedCap) -> bool, brightness: f64) -> Option<f64> {
    let matching: Vec<&CachedCap> = channel.caps.iter().filter(|cap| test(cap)).collect();

    if matching.is_empty() {
        return None;
    }

    if channel.caps.len() == 1 {
        let range = brightness_range(channel, &matching[0].cap);
        return Some(if range[0] > range[1] {
            channel.max * (1.0 - brightness)
        } else {
            channel.max * brightness
        });
    }

    let mut best: Option<&CachedCap> = None;
    let mut best_distance = f64::MAX;
    let mut best_brightness = 0.0;
    let mut best_is_range = false;

    for cap in matching {
        let range = brightness_range(channel, &cap.cap);
        let low = range[0].min(range[1]);
        let high = range[0].max(range[1]);
        let reached = low.max(high.min(brightness));
        let distance = (reached - brightness).abs();
        let is_range = low < high;

        let better = if best.is_none() || distance < best_distance - 1e-9 {
            true
        } else if distance > best_distance + 1e-9 {
            false
        } else if distance == 0.0 {
            is_range && !best_is_range
        } else {
            reached > best_brightness
        };

        if better {
            best = Some(cap);
            best_distance = distance;
            best_brightness = reached;
            best_is_range = is_range;
        }
    }

    let best = best?;
    let dmx_range = best.cap.dmx_range.as_deref().filter(|range| range.len() >= 2)?;
    let range = brightness_range(channel, &best.cap);

    if range[0] == range[1] {
        return Some(best.center);
    }

    Some(
        dmx_range[0] as f64
            + (dmx_range[1] - dmx_range[0]) as f64 * (best_brightness - range[0]) / (range[1] - range[0]),
    )
}

fn has_color(parts_channels: &[&CachedChan], colors: &[crate::model::FixtureCapabilityColor]) -> bool {
    parts_channels.iter().any(|channel| {
        channel.caps.iter().any(|cap| {
            cap.cap.type_ == Some(Type::ColorIntensity)
                && cap.cap.color.map(|color| colors.contains(&color)).unwrap_or(false)
        })
    })
}

/// A fixture lighting in colors without a dimmer gets a virtual one, which starts at full.
fn add_virtual_dimmers(parts: &mut [CachedPart]) {
    if parts.iter().any(|part| has_type(part, Type::Intensity)) {
        return;
    }

    for part in parts.iter_mut() {
        if !has_type(part, Type::ColorIntensity) {
            continue;
        }

        let channel = FixtureChannel {
            capability: Some(FixtureCapability {
                type_: Some(Type::Intensity),
                ..FixtureCapability::default()
            }),
            // at full (a percentage would come out a little below it)
            default_value: Some("255".to_string()),
            ..FixtureChannel::default()
        };

        part.channels
            .push(cached_chan(&channel, VIRTUAL_DIMMER, None, part.profile));
    }
}

/// The dimmers of a fixture which lights in colors only start at full, where its profile does not say where they
/// start.
fn open_color_dimmers(parts: &mut [CachedPart]) {
    use crate::model::FixtureCapabilityColor as Color;

    {
        let channels: Vec<&CachedChan> = parts.iter().flat_map(|part| part.channels.iter()).collect();
        let mut lights_in_colors = false;

        for channel in &channels {
            if channel.color_wheel {
                return;
            }

            if channel
                .caps
                .iter()
                .any(|cap| cap.cap.type_ == Some(Type::ColorIntensity))
            {
                lights_in_colors = true;
            }
        }

        let subtractive = !has_color(&channels, &[Color::Red, Color::Green, Color::Blue])
            && has_color(&channels, &[Color::Cyan, Color::Magenta, Color::Yellow]);

        if !lights_in_colors || subtractive {
            return;
        }
    }

    for part in parts.iter_mut() {
        for channel in part.channels.iter_mut() {
            if channel.default.is_some() {
                continue;
            }

            if let Some(value) = brightness_value(channel, |cap| cap.cap.type_ == Some(Type::Intensity), 1.0) {
                channel.default = Some(value);
            }
        }
    }
}

// ---- what a capability does (the ranges of the fixture library) ----

/// the property telling where the range of a capability starts and where it ends, and whether it runs the other way
/// round than the capability does
fn range_ends(cap: &FixtureCapability) -> (Option<&str>, Option<&str>, bool) {
    fn ends<'a>(
        start: &'a Option<String>,
        end: &'a Option<String>,
        all: &'a Option<String>,
    ) -> (Option<&'a str>, Option<&'a str>, bool) {
        (
            if is_empty(start) {
                all.as_deref()
            } else {
                start.as_deref()
            },
            if is_empty(end) { all.as_deref() } else { end.as_deref() },
            false,
        )
    }

    match cap.type_ {
        Some(Type::Zoom) | Some(Type::BeamAngle) => (cap.angle_start.as_deref(), cap.angle_end.as_deref(), false),
        Some(Type::Focus) => (cap.distance_start.as_deref(), cap.distance_end.as_deref(), false),
        Some(Type::Iris) => (
            cap.open_percent_start.as_deref(),
            cap.open_percent_end.as_deref(),
            false,
        ),
        Some(Type::Frost) => (
            cap.frost_intensity_start.as_deref(),
            cap.frost_intensity_end.as_deref(),
            false,
        ),
        Some(Type::ColorTemperature) => (
            cap.color_temperature_start.as_deref(),
            cap.color_temperature_end.as_deref(),
            false,
        ),
        Some(Type::ShutterStrobe) | Some(Type::StrobeSpeed) => {
            (cap.speed_start.as_deref(), cap.speed_end.as_deref(), false)
        }
        Some(Type::StrobeDuration) => (cap.duration_start.as_deref(), cap.duration_end.as_deref(), false),
        Some(Type::PanTiltSpeed) => {
            if !is_empty(&cap.speed_start) || !is_empty(&cap.speed_end) {
                (cap.speed_start.as_deref(), cap.speed_end.as_deref(), false)
            } else {
                // a movement given by the time it takes is the slower the longer it takes
                (cap.duration_start.as_deref(), cap.duration_end.as_deref(), true)
            }
        }
        Some(Type::FogOutput) | Some(Type::Fog) => ends(&cap.fog_output_start, &cap.fog_output_end, &cap.fog_output),
        Some(Type::BladeInsertion) => ends(&cap.insertion_start, &cap.insertion_end, &cap.insertion),
        Some(Type::BladeRotation) | Some(Type::BladeSystemRotation) => {
            ends(&cap.angle_start, &cap.angle_end, &cap.angle)
        }
        Some(Type::IrisEffect) | Some(Type::FrostEffect) | Some(Type::Effect) | Some(Type::EffectSpeed) => {
            ends(&cap.speed_start, &cap.speed_end, &cap.speed)
        }
        Some(Type::WheelShake) => ends(&cap.shake_speed_start, &cap.shake_speed_end, &cap.shake_speed),
        _ => (None, None, false),
    }
}

fn is_range_reversed(cap: &FixtureCapability) -> bool {
    let (start, end, inverted) = range_ends(cap);

    match (profile::get_property_rank(start), profile::get_property_rank(end)) {
        (Some(start), Some(end)) => {
            if inverted {
                start < end
            } else {
                start > end
            }
        }
        _ => false,
    }
}

fn puts_out_nothing(cap: &FixtureCapability) -> bool {
    let (start, end, _) = range_ends(cap);
    profile::is_nothing(start) && profile::is_nothing(end)
}

fn shutter_of_capability(effect: Option<&str>) -> u8 {
    match effect {
        None => f::shutter::NONE,
        Some("Open") => f::shutter::OPEN,
        Some("Closed") => f::shutter::CLOSED,
        Some("Strobe") => f::shutter::STROBE,
        Some("Pulse") => f::shutter::PULSE,
        Some("RampUp") => f::shutter::RAMP_UP,
        Some("RampDown") => f::shutter::RAMP_DOWN,
        Some("RampUpDown") => f::shutter::RAMP_UP_DOWN,
        Some("Lightning") => f::shutter::LIGHTNING,
        Some("Spikes") => f::shutter::SPIKES,
        Some("Burst") => f::shutter::BURST,
        Some(_) => f::shutter::OTHER,
    }
}

fn shutter_of_value(effect: Option<&str>) -> u8 {
    match effect {
        Some("Random") => f::shutter::RANDOM,
        Some("Sound") => f::shutter::SOUND,
        other => shutter_of_capability(other),
    }
}

fn width(channel: &CachedChan, cap: &CachedCap) -> f64 {
    match cap.cap.dmx_range.as_deref() {
        Some(range) if range.len() >= 2 => (range[1] - range[0]) as f64,
        _ => channel.max,
    }
}

/// The degrees a channel turns its fixture over on an axis, none if it doesn't turn it on that axis.
fn axis_range(channel: &CachedChan, t: Type) -> Option<f64> {
    let mut widest: Option<&CachedCap> = None;

    for cap in channel.caps.iter().filter(|cap| cap.cap.type_ == Some(t)) {
        if widest
            .map(|widest| width(channel, cap) > width(channel, widest))
            .unwrap_or(true)
        {
            widest = Some(cap);
        }
    }

    let cap = &widest?.cap;
    let fallback = if t == Type::Pan { 540.0 } else { 270.0 };
    let start = profile::get_property_rank(cap.angle_start.as_deref());
    let end = profile::get_property_rank(cap.angle_end.as_deref());
    let range = end.unwrap_or(fallback) - start.unwrap_or(0.0);

    Some(if range == 0.0 { fallback } else { range })
}

/// the channel and the capability of a part doing a maintenance action
fn maintenance_capability<'a, 'p>(
    channels: &'a [CachedChan<'p>],
    action: MaintenanceAction,
) -> Option<(usize, &'a CachedCap<'p>)> {
    let mut found = None;

    for (index, channel) in channels.iter().enumerate() {
        for cap in &channel.caps {
            if cap.cap.type_ != Some(Type::Maintenance) {
                continue;
            }

            match profile::maintenance_match(action, cap.cap.comment.as_deref()) {
                MaintenanceMatch::Exactly => return Some((index, cap)),
                MaintenanceMatch::Among if found.is_none() => found = Some((index, cap)),
                _ => {}
            }
        }
    }

    found
}

fn place_record(place: &PixelPlace) -> f::Place {
    f::Place {
        x: place.x as f32,
        y: place.y as f32,
        aspect_x: place.aspect_x as f32,
        aspect_y: place.aspect_y as f32,
        column: place.column,
        row: place.row,
        columns: place.columns,
        rows: place.rows,
    }
}

// ---- the compiler ----

struct Compiler<'p> {
    project: &'p Project,
    writer: Writer,
    names: Names,
    features: u32,
    warnings: Vec<String>,
    parts: Vec<CachedPart<'p>>,
    /// where each part is in its fixture, for the pixel effects
    places: Vec<PixelPlace>,
    /// the number of the fixture of each uuid (the first one with it)
    placements: HashMap<String, StagePlacement>,
    /// the spots the moving heads are aimed at
    aims: Vec<[f64; 3]>,
    /// the addresses of the channels of each mode worked out so far
    addresses: HashMap<(usize, usize), HashMap<String, Vec<u32>>>,
}

fn presets_of(project: &Project) -> &[Preset] {
    project.presets.as_deref().unwrap_or(&[])
}

fn scenes_of(project: &Project) -> &[Scene] {
    project.scenes.as_deref().unwrap_or(&[])
}

fn fixtures_of(project: &Project) -> &[Fixture] {
    project.fixtures.as_deref().unwrap_or(&[])
}

fn compositions_of(project: &Project) -> &[Composition] {
    project.compositions.as_deref().unwrap_or(&[])
}

fn preset_fixtures_of(project: &Project) -> &[PresetFixture] {
    project.preset_fixtures.as_deref().unwrap_or(&[])
}

fn fixture_by_uuid<'p>(project: &'p Project, uuid: Option<&str>) -> Option<&'p Fixture> {
    fixtures_of(project)
        .iter()
        .find(|fixture| fixture.uuid.as_deref() == uuid)
}

/// a fixture or one of its pixels (an empty pixel key is none)
type PartKey = (Option<String>, Option<String>);

fn part_key(fixture_uuid: Option<&str>, pixel_key: Option<&str>) -> PartKey {
    (
        fixture_uuid.map(str::to_string),
        empty_to_none(pixel_key).map(str::to_string),
    )
}

/// The place of each fixture (or pixel) of a preset in its chase and the number of steps of the chase, and the same
/// for the fixtures with all of their pixels.
struct Chase {
    indices: HashMap<PartKey, i32>,
    count: i32,
    whole_indices: HashMap<Option<String>, i32>,
    whole_count: i32,
}

fn chase_of(project: &Project, preset: &Preset) -> Chase {
    let preset_parts: HashSet<PartKey> = preset
        .fixtures
        .iter()
        .map(|fixture| part_key(fixture.fixture_uuid.as_deref(), fixture.pixel_key.as_deref()))
        .collect();

    // either the global fixture order of the project or the preset's own one defines the order to chase in
    let ordered: &[PresetFixture] = if preset.use_global_fixture_order {
        preset_fixtures_of(project)
    } else {
        &preset.fixtures
    };

    // the lamp a fixture (or a pixel of it) is: fixtures sharing a universe and an address are the same lamp
    let lamp = |fixture: &PresetFixture| {
        fixture_by_uuid(project, fixture.fixture_uuid.as_deref()).map(|found| {
            (
                found.dmx_universe_uuid.clone(),
                found.dmx_first_channel,
                empty_to_none(fixture.pixel_key.as_deref()).map(str::to_string),
            )
        })
    };
    let whole_lamp = |fixture: &PresetFixture| {
        fixture_by_uuid(project, fixture.fixture_uuid.as_deref())
            .map(|found| (found.dmx_universe_uuid.clone(), found.dmx_first_channel))
    };

    let mut indices = HashMap::new();
    let mut counted = HashSet::new();
    let mut index = 0;
    let mut whole_indices = HashMap::new();
    let mut whole_counted = HashSet::new();
    let mut whole_index = 0;

    for fixture in ordered {
        let key = part_key(fixture.fixture_uuid.as_deref(), fixture.pixel_key.as_deref());

        // when using the global order, skip all fixtures which are not in this preset
        if !preset_parts.contains(&key) {
            continue;
        }

        // a fixture listed more than once chases at its first place
        indices.entry(key).or_insert(index);

        // don't count fixtures on the same channel as already counted ones
        if let Some(lamp) = lamp(fixture) {
            if counted.insert(lamp) {
                index += 1;
            }
        }

        // a fixture with all of its pixels takes one step, the same for the fixtures on its channel
        whole_indices.entry(fixture.fixture_uuid.clone()).or_insert(whole_index);

        if let Some(lamp) = whole_lamp(fixture) {
            if whole_counted.insert(lamp) {
                whole_index += 1;
            }
        }
    }

    let lamps: HashSet<_> = preset.fixtures.iter().filter_map(lamp).collect();
    let whole_lamps: HashSet<_> = preset.fixtures.iter().filter_map(whole_lamp).collect();

    Chase {
        indices,
        count: lamps.len() as i32,
        whole_indices,
        whole_count: whole_lamps.len() as i32,
    }
}

/// The place of a fixture in the pattern of its preset: its fixtures chase and fan in groups, in wings and repeated.
fn pattern_place(index: i32, count: i32, group_size: f64, wings: f64, every: f64) -> (i32, i32) {
    let rounded = |value: f64, fallback: f64| {
        java_round(if value == 0.0 || value.is_nan() {
            fallback
        } else {
            value
        })
    };
    let mut pattern_index = index;
    let mut pattern_count = count;
    let size = rounded(group_size, 1.0).max(1);

    if size > 1 {
        pattern_index = (pattern_index as f64 / size as f64).floor() as i32;
        pattern_count = (pattern_count as f64 / size as f64).ceil() as i32;
    }

    let wing_count = rounded(wings, 1.0).max(1);

    if wing_count > 1 && pattern_count > 1 {
        let wing_size = (pattern_count as f64 / wing_count as f64).ceil() as i32;
        let wing = (pattern_index as f64 / wing_size as f64).floor() as i32;

        if wing % 2 == 1 {
            let wing_end = ((wing + 1) * wing_size).min(pattern_count) - 1;
            pattern_index = wing_end - pattern_index;
        } else {
            pattern_index -= wing * wing_size;
        }

        pattern_count = wing_size;
    }

    let repeat = rounded(every, 0.0);

    if repeat > 1 && pattern_count > 1 {
        pattern_index = (pattern_index as i64 % repeat) as i32;
        pattern_count = repeat.min(pattern_count as i64) as i32;
    }

    (pattern_index, pattern_count)
}

fn is_ordered(preset: &Preset) -> bool {
    matches!(preset.steps_order.as_str(), "backward" | "pingpong" | "random")
}

/// The steps a preset plays: its own, or the single look an old preset keeps on itself.
fn steps_of(preset: &Preset) -> Vec<PresetStep> {
    if !preset.steps.is_empty() {
        return preset.steps.clone();
    }

    vec![PresetStep {
        fixture_channel_values: preset.fixture_channel_values.clone().unwrap_or_default(),
        fixture_capability_values: preset.fixture_capability_values.clone().unwrap_or_default(),
        ..PresetStep::default()
    }]
}

struct StepTimes {
    starts: Vec<i64>,
    lengths: Vec<i64>,
    loop_length: i64,
    pass_length: i64,
    /// the steps lasting any time, in the order a pass plays them in (in the order written down for a random order)
    playable: Vec<u16>,
}

fn step_times(preset: &Preset, steps: &[PresetStep]) -> StepTimes {
    let starts: Vec<i64> = steps.iter().map(|step| ns(step.start_millis)).collect();
    let count = starts.len();

    // the length of one pass through the steps: without a length of its own, the last step holds as long as the one
    // before it lasted
    let loop_length = match preset.steps_loop_millis {
        Some(millis) if millis > 0.0 => ns(millis),
        _ if count < 2 => 0,
        _ => starts[count - 1] - starts[0] + (starts[count - 1] - starts[count - 2]),
    };

    let loop_end = starts.first().copied().unwrap_or(0) + loop_length;
    let lengths: Vec<i64> = (0..count)
        .map(|index| if index < count - 1 { starts[index + 1] } else { loop_end } - starts[index])
        .collect();
    let mut playable: Vec<u16> = (0..count)
        .filter(|index| lengths[*index] > 0)
        .map(|index| index as u16)
        .collect();
    let mut pass_length = loop_length;

    if is_ordered(preset) {
        match preset.steps_order.as_str() {
            "backward" => playable.reverse(),
            "pingpong" => {
                // back again without the last step, which was just played. A sequence which starts over leaves out
                // the first step as well, which the next pass opens with.
                let mut back: Vec<u16> = playable[..playable.len().saturating_sub(1)].to_vec();
                back.reverse();

                if preset.steps_loop && !back.is_empty() {
                    back.pop();
                }

                playable.extend(back);
            }
            _ => {}
        }

        pass_length = playable.iter().map(|index| lengths[*index as usize]).sum();
    }

    StepTimes {
        starts,
        lengths,
        loop_length,
        pass_length,
        playable,
    }
}

fn steps_are_phased(preset: &Preset, steps: &[PresetStep]) -> bool {
    if steps.len() < 2 {
        return false;
    }

    if preset.steps_phasing_mode == "spread" {
        return preset.steps_phasing_cycles != 0.0;
    }

    preset.steps_phasing_millis != 0.0
}

/// the time the sequence of the steps is shifted by for a fixture of the preset, in ms
fn steps_phasing(preset: &Preset, index: i32, count: i32, pass_millis: f64) -> f64 {
    let group_size = java_round(preset.steps_phasing_group_size).max(1);
    let step = (index as f64 / group_size as f64).floor();

    if preset.steps_phasing_mode == "spread" {
        let count = if count == 0 { 1 } else { count };
        let steps = (count as f64 / group_size as f64).ceil().max(1.0);
        return step / steps * preset.steps_phasing_cycles * pass_millis;
    }

    step * preset.steps_phasing_millis
}

impl<'p> Compiler<'p> {
    fn profile(&self, uuid: Option<&str>) -> Option<&'p FixtureProfile> {
        let uuid = uuid?;
        self.project
            .fixture_profiles
            .as_ref()?
            .iter()
            .find(|profile| profile.uuid.as_deref() == Some(uuid))
    }

    fn build_parts(&mut self) {
        let project = self.project;
        let mut kinds: HashMap<(usize, usize), std::ops::Range<usize>> = HashMap::new();

        for (fixture_index, fixture) in fixtures_of(project).iter().enumerate() {
            let (profile, mode) = match self
                .profile(fixture.profile_uuid.as_deref())
                .and_then(|profile| Some((profile, profile::get_mode_by_fixture(profile, fixture)?)))
            {
                Some(found) => found,
                None => {
                    self.warnings.push(format!(
                        "fixture {:?} is left out: its profile or its mode is missing",
                        fixture.uuid.as_deref().unwrap_or("")
                    ));
                    continue;
                }
            };

            // (the parts of a fixture are worked out once for all fixtures of its profile and mode)
            let kind = (
                profile as *const FixtureProfile as usize,
                mode as *const FixtureMode as usize,
            );

            if let Some(known) = kinds.get(&kind) {
                let parts: Vec<CachedPart> = self.parts[known.clone()]
                    .iter()
                    .map(|part| CachedPart {
                        fixture: fixture_index,
                        kind: part.kind,
                        profile,
                        mode,
                        pixel: part.pixel.clone(),
                        channels: part.channels.clone(),
                    })
                    .collect();
                self.parts.extend(parts);
                continue;
            }

            let mut parts = Vec::new();

            if profile::mode_has_general_channel(profile, mode) {
                parts.push(CachedPart {
                    fixture: fixture_index,
                    kind: (kind.0, kind.1, 0),
                    profile,
                    mode,
                    pixel: None,
                    channels: cached_channels(profile, mode, None),
                });
            }

            for pixel in profile::mode_get_unique_pixels(profile, mode) {
                let channels = cached_channels(profile, mode, Some(&pixel.key));
                parts.push(CachedPart {
                    fixture: fixture_index,
                    kind: (kind.0, kind.1, parts.len()),
                    profile,
                    mode,
                    pixel: Some(pixel),
                    channels,
                });
            }

            for (ordinal, part) in parts.iter_mut().enumerate() {
                part.kind.2 = ordinal;
            }

            add_virtual_dimmers(&mut parts);

            if project.color_dimmers_at_full {
                open_color_dimmers(&mut parts);
            }

            kinds.insert(kind, self.parts.len()..self.parts.len() + parts.len());
            self.parts.extend(parts);
        }

        // where each part is in its fixture
        let mut places: HashMap<(usize, usize, usize), PixelPlace> = HashMap::new();

        self.places = self
            .parts
            .iter()
            .map(|part| {
                let matrix = part.profile.matrix.as_ref();

                if let Some(place) = places.get(&part.kind) {
                    return *place;
                }

                let place = match (&part.pixel, matrix) {
                    (Some(pixel), Some(matrix)) => {
                        let group: Vec<(i32, i32)> = if pixel.x > 0 {
                            Vec::new()
                        } else {
                            profile::get_pixels_for_key_or_group(
                                Some(matrix),
                                &profile::get_all_pixel_keys(part.profile),
                                &pixel.key,
                            )
                            .unwrap_or_default()
                            .iter()
                            .map(|pixel| (pixel.x, pixel.y))
                            .collect()
                        };

                        stage::pixel_place(
                            Some(profile::get_matrix_size(matrix)),
                            Some(PixelPosition::of(pixel.x, pixel.y, &group)),
                        )
                    }
                    _ => PixelPlace::default(),
                };

                places.insert(part.kind, place);
                place
            })
            .collect();
    }

    fn stage_placements(&mut self) {
        let project = self.project;

        let fixtures: Vec<StageFixture> = fixtures_of(project)
            .iter()
            .map(|fixture| StageFixture {
                uuid: fixture.uuid.as_deref().unwrap_or(""),
                positioning: &fixture.positioning,
                position: [fixture.position_x, fixture.position_y, fixture.position_z],
                rotation: [fixture.rotation_x, fixture.rotation_y, fixture.rotation_z],
            })
            .collect();

        let preset_fixtures: Vec<&str> = preset_fixtures_of(project)
            .iter()
            .map(|fixture| fixture.fixture_uuid.as_deref().unwrap_or(""))
            .collect();

        let size = StageSize {
            width_cm: project.stage_width_cm,
            height_cm: project.stage_height_cm,
            depth_cm: project.stage_depth_cm,
            floor_height_cm: project.stage_floor_height_cm,
        };

        self.placements = stage::stage_placements(&fixtures, &preset_fixtures, &size);
    }

    fn fixture_of(&self, part: &CachedPart) -> &'p Fixture {
        &fixtures_of(self.project)[part.fixture]
    }

    /// where a part of a fixture is on the stage (in cm)
    fn part_stage_position(&self, part_index: usize) -> [f64; 3] {
        let part = &self.parts[part_index];
        let fixture = self.fixture_of(part);
        let dimensions = part
            .profile
            .physical
            .as_ref()
            .and_then(|physical| physical.dimensions.as_deref());
        let placement = self.placements.get(fixture.uuid.as_deref().unwrap_or(""));

        stage::part_stage_position(&self.places[part_index], dimensions, placement, &fixture.positioning)
    }

    // ---- the layouts ----

    fn cap_record(&mut self, channel: &CachedChan, cached: &CachedCap) -> Result<f::Cap, CompileError> {
        let cap = &cached.cap;
        let mut flags = 0;
        let mut record = f::Cap {
            ty: type_of(cap.type_),
            color: cap.color.map(|color| color.as_u8()).unwrap_or(0),
            ..f::Cap::default()
        };

        if has_range(cap) {
            flags |= f::cap_flag::RANGE;
        }

        if let Some(range) = cap.dmx_range.as_deref().filter(|range| range.len() >= 2) {
            flags |= f::cap_flag::RANGE_OR_MORE;
            record.low = range[0] as f32;
            record.high = range[1] as f32;
        }

        record.center = cached.center as f32;

        let brightness = brightness_range(channel, cap);
        record.brightness_start = brightness[0] as f32;
        record.brightness_end = brightness[1] as f32;

        if is_range_reversed(cap) {
            flags |= f::cap_flag::REVERSED;
        }

        if puts_out_nothing(cap) {
            flags |= f::cap_flag::NOTHING;
        }

        record.shutter = shutter_of_capability(cap.shutter_effect.as_deref());

        if cap.random_timing == Some(true) {
            flags |= f::cap_flag::RANDOM;
        }

        if cap.sound_controlled == Some(true) {
            flags |= f::cap_flag::SOUND;
        }

        if is_empty(&cap.speed_start) && is_empty(&cap.speed) {
            flags |= f::cap_flag::STILL;
        }

        if !is_empty(&cap.speed_start) || !is_empty(&cap.speed_end) {
            flags |= f::cap_flag::SPEED;
        }

        // the speeds a rotation runs between
        let rotation_start = profile::get_rotation_rank(if is_empty(&cap.speed_start) {
            cap.speed.as_deref()
        } else {
            cap.speed_start.as_deref()
        });
        let rotation_end = profile::get_rotation_rank(if is_empty(&cap.speed_end) {
            cap.speed.as_deref()
        } else {
            cap.speed_end.as_deref()
        });

        if let (Some(start), Some(end)) = (rotation_start, rotation_end) {
            flags |= f::cap_flag::ROTATION_ENDS;
            record.rotation_start = start as f32;
            record.rotation_end = end as f32;
        }

        let angle_start = profile::get_property_rank(cap.angle_start.as_deref().or(cap.angle.as_deref()));
        let angle_end = profile::get_property_rank(cap.angle_end.as_deref().or(cap.angle.as_deref()));

        if let (Some(start), Some(end)) = (angle_start, angle_end) {
            flags |= f::cap_flag::ANGLES;
            record.angle_start = start as f32;
            record.angle_end = end as f32;
        }

        if let Some(slot) = cap.slot_number {
            flags |= f::cap_flag::SLOT;
            record.slot = slot as f32;

            // the color of its slot, which a color of a preset is approximated with
            if let Some(color) = profile::get_mixed_wheel_slot_color(cached.wheel, slot) {
                flags |= f::cap_flag::SLOT_COLOR;
                record.slot_red = color.red as f32;
                record.slot_green = color.green as f32;
                record.slot_blue = color.blue as f32;
            }
        }

        if cached.wheel_is_color {
            flags |= f::cap_flag::WHEEL_IS_COLOR;
        }

        if cap.type_ == Some(Type::ColorPreset) && has_range(cap) {
            if let Some(color) = cap.colors.as_ref().and_then(|colors| profile::mix_hex_colors(colors)) {
                flags |= f::cap_flag::PRESET_COLOR;
                record.preset_red = color.red as f32;
                record.preset_green = color.green as f32;
                record.preset_blue = color.blue as f32;
            }
        }

        record.wheel = self.names.of(cached.wheel_name.as_deref())?;

        // the wheel a capability shakes: the one of its slot, the one it names or else the one named like its channel
        let shake_wheel = match (&cached.wheel_name, &cap.wheel) {
            (Some(name), _) if !name.is_empty() => name.as_str(),
            (_, Some(Wheel::Name(name))) => name.as_str(),
            _ => channel.name.as_str(),
        };

        record.shake_wheel = self.names.of(Some(shake_wheel))?;
        record.blade = self.names.of(cap.blade.as_deref())?;

        // the wheels a capability turns: the ones it names, or else the one named like its channel
        let turned: Vec<&str> = match &cap.wheel {
            Some(Wheel::Names(names)) => names.iter().map(String::as_str).collect(),
            Some(Wheel::Name(name)) => vec![name.as_str()],
            _ => vec![channel.name.as_str()],
        };

        let mut turned_numbers = Vec::new();

        for name in turned {
            turned_numbers.push(self.names.of(Some(name))?);
        }

        record.turned = self.writer.list(&turned_numbers);
        record.flags = flags;

        Ok(record)
    }

    fn chan_record(&mut self, channel: &CachedChan) -> Result<f::Chan, CompileError> {
        let mut caps = Vec::new();
        let mut types = 0u64;

        for cached in &channel.caps {
            let record = self.cap_record(channel, cached)?;

            if record.ty < 64 {
                types |= 1 << record.ty;
            }

            caps.push(record);
        }

        let mut flags = 0;

        if channel.default.is_some() {
            flags |= f::chan_flag::DEFAULT;
        }

        if channel.color_wheel {
            flags |= f::chan_flag::COLOR_WHEEL;
        }

        let pan = axis_range(channel, Type::Pan);
        let tilt = axis_range(channel, Type::Tilt);

        if pan.is_some() {
            flags |= f::chan_flag::PAN;
        }

        if tilt.is_some() {
            flags |= f::chan_flag::TILT;
        }

        // the value doing each maintenance action, on a channel doing it
        let mut maintenance = [0f32; 3];

        for (index, action) in MaintenanceAction::ALL.iter().enumerate() {
            if let Some((_, cap)) = maintenance_capability(std::slice::from_ref(channel), *action) {
                if has_range(&cap.cap) {
                    flags |= f::chan_flag::MAINTENANCE << index;
                    maintenance[index] = cap.center as f32;
                }
            }
        }

        // the programs of the channel, to the sound or not, in the order of their values
        let programs = |sound: bool| -> Vec<u16> {
            let all: Vec<usize> = channel
                .caps
                .iter()
                .enumerate()
                .filter(|(_, cap)| cap.cap.type_ == Some(Type::Effect))
                .map(|(index, _)| index)
                .collect();
            let kind: Vec<usize> = all
                .iter()
                .copied()
                .filter(|index| (channel.caps[*index].cap.sound_controlled == Some(true)) == sound)
                .collect();
            let mut ordered = if kind.is_empty() { all } else { kind };
            ordered.sort_by_key(|index| {
                channel.caps[*index]
                    .cap
                    .dmx_range
                    .as_ref()
                    .and_then(|range| range.first().copied())
                    .unwrap_or(0)
            });
            ordered.into_iter().map(|index| index as u16).collect()
        };

        let plain_programs = programs(false);
        let sound_programs = programs(true);

        Ok(f::Chan {
            name: self.names.of(Some(&channel.name))?,
            fine: channel.channel.fine_channel_aliases.len() as u16,
            flags,
            types_low: types as u32,
            types_high: (types >> 32) as u32,
            max: channel.max as f32,
            default: channel.default.unwrap_or(0.0) as f32,
            pan_range: pan.unwrap_or(0.0) as f32,
            tilt_range: tilt.unwrap_or(0.0) as f32,
            maintenance_reset: maintenance[0],
            maintenance_lamp_on: maintenance[1],
            maintenance_lamp_off: maintenance[2],
            no_function: channel
                .caps
                .iter()
                .position(|cap| cap.cap.type_ == Some(Type::NoFunction))
                .map(|index| index as i32)
                .unwrap_or(-1),
            caps: self.writer.list(&caps),
            programs: self.writer.list(&plain_programs),
            sound_programs: self.writer.list(&sound_programs),
        })
    }

    /// the addresses of the channels of a mode (from the first one of the fixture), by the names of the channels
    fn mode_addresses(profile: &FixtureProfile, mode: &FixtureMode) -> HashMap<String, Vec<u32>> {
        let mut addresses: HashMap<String, Vec<u32>> = HashMap::new();
        let mut address: u32 = 0;

        let mut add = |address: u32, template_name: &str, pixel_key: Option<&str>| {
            addresses
                .entry(profile::get_channel_name_with_pixel_key(template_name, pixel_key))
                .or_default()
                .push(address);
        };

        for channel in &mode.channels {
            if profile::is_unused_channel(channel) {
                // an unused channel still takes its DMX address
                address += 1;
            } else if channel.name.is_empty() {
                // reference a channel through a pixel matrix
                let pixels = profile::get_pixels_in_order(profile, &channel.repeat_for);

                if channel.channel_order.as_deref() == Some("perPixel") {
                    for pixel in &pixels {
                        for template_name in &channel.template_channels {
                            add(address, template_name, Some(&pixel.key));
                            address += 1;
                        }
                    }
                } else if channel.channel_order.as_deref() == Some("perChannel") {
                    for template_name in &channel.template_channels {
                        for pixel in &pixels {
                            add(address, template_name, Some(&pixel.key));
                            address += 1;
                        }
                    }
                }
            } else {
                add(address, &channel.name, None);
                address += 1;
            }
        }

        addresses
    }

    /// where a part of a fixture sends its channels, in the order of the channels of its mode
    fn outputs(&mut self, part: &CachedPart) -> Vec<f::Out> {
        let addresses = self
            .addresses
            .entry((part.kind.0, part.kind.1))
            .or_insert_with(|| Self::mode_addresses(part.profile, part.mode));
        let mut outputs = Vec::new();
        let mut claimed: HashSet<String> = HashSet::new();

        // Fine channels are not parts of their own: a name of the mode is the channel named so or the one it is a fine
        // channel of (the byte it is of it), the first channel of the part which is either
        for (index, channel) in part.channels.iter().enumerate() {
            let fine = channel.channel.fine_channel_aliases.len() as i32;
            let mut names: Vec<(String, i32)> = channel
                .channel
                .fine_channel_aliases
                .iter()
                .enumerate()
                .map(|(alias_index, alias)| {
                    (
                        profile::get_channel_name_with_pixel_key(alias, channel.pixel_key.as_deref()),
                        alias_index as i32,
                    )
                })
                .collect();

            if !names.iter().any(|(name, _)| *name == channel.name) {
                names.push((channel.name.clone(), -1));
            }

            // (the first of its fine channels of a name is the one found)
            let mut own: HashSet<&str> = HashSet::new();

            for (name, fine_index) in &names {
                if claimed.contains(name) || !own.insert(name.as_str()) {
                    continue;
                }

                for address in addresses.get(name).map(Vec::as_slice).unwrap_or(&[]) {
                    if *address <= u16::MAX as u32 {
                        outputs.push(f::Out {
                            address: *address as u16,
                            chan: index as u16,
                            shift: ((fine - (fine_index + 1)).max(0) as u32 * 8).min(31),
                        });
                    }
                }
            }

            claimed.extend(names.into_iter().map(|(name, _)| name));
        }

        outputs.sort_by_key(|output| output.address);
        outputs
    }

    fn layout_record(&mut self, part: &CachedPart, place: f::Place) -> Result<f::Layout, CompileError> {
        use crate::model::FixtureCapabilityColor as Color;

        let mut channels = Vec::new();

        for channel in &part.channels {
            channels.push(self.chan_record(channel)?);
        }

        if part.channels.len() > i16::MAX as usize {
            return Err(CompileError::Limit(
                "a part of a fixture has more than 32767 channels".to_string(),
            ));
        }

        let first_with = |test: &dyn Fn(&CachedCap) -> bool| {
            part.channels
                .iter()
                .position(|channel| channel.caps.iter().any(test))
                .map(|index| index as i16)
                .unwrap_or(-1)
        };
        let color_channel = |color: Color| {
            first_with(&|cap: &CachedCap| cap.cap.type_ == Some(Type::ColorIntensity) && cap.cap.color == Some(color))
        };
        let dimmer = first_with(&|cap: &CachedCap| cap.cap.type_ == Some(Type::Intensity));

        let mut colors = 0u32;
        let mut flags = 0;

        for channel in &part.channels {
            if channel.color_wheel {
                flags |= f::layout_flag::COLOR_WHEEL;
            }

            for cap in &channel.caps {
                if cap.cap.type_ == Some(Type::ColorIntensity) {
                    if let Some(color) = cap.cap.color {
                        colors |= 1 << color.as_u8();
                    }
                }

                if cap.cap.type_ == Some(Type::ColorTemperature) {
                    flags |= f::layout_flag::COLOR_TEMPERATURE;
                }
            }
        }

        // the channel resetting the fixture and the one switching its lamp on
        let maintenance = |action: MaintenanceAction| match maintenance_capability(&part.channels, action) {
            Some((index, cap)) if has_range(&cap.cap) => (index as i16, cap.center as f32),
            _ => (-1, 0.0),
        };

        let (reset, reset_value) = maintenance(MaintenanceAction::Reset);
        let (lamp_on, lamp_on_value) = maintenance(MaintenanceAction::LampOn);

        // the open slots of the color wheels, which identifying the fixture turns to
        let mut open_wheels: Vec<Option<&str>> = Vec::new();
        let mut open_slots = Vec::new();

        for channel in &part.channels {
            for cap in &channel.caps {
                let slots = profile::get_wheel_slots(cap.wheel, cap.cap.slot_number.unwrap_or(0.0));
                let open = cap.wheel_is_color
                    && cap.cap.slot_number.is_some()
                    && !slots.is_empty()
                    && slots.iter().all(|slot| slot.type_ == Some(FixtureWheelSlotType::Open));

                if open && !open_wheels.contains(&cap.wheel_name.as_deref()) {
                    open_wheels.push(cap.wheel_name.as_deref());
                    open_slots.push((cap.wheel_name.clone(), cap.cap.slot_number.unwrap_or(0.0) as f32));
                }
            }
        }

        let outputs = self.outputs(part);
        let pixel = part.pixel.is_some();
        let virtual_dimmer = part
            .channels
            .iter()
            .position(|channel| channel.name == VIRTUAL_DIMMER)
            .map(|index| index as i16)
            .unwrap_or(-1);

        let mut open_slot_records = Vec::new();

        for (wheel, slot) in open_slots {
            open_slot_records.push(f::OpenSlot {
                wheel: self.names.of(wheel.as_deref())?,
                reserved: 0,
                slot,
            });
        }

        Ok(f::Layout {
            channels: self.writer.list(&channels),
            outputs: self.writer.list(&outputs),
            open_slots: self.writer.list(&open_slot_records),
            red: color_channel(Color::Red),
            green: color_channel(Color::Green),
            blue: color_channel(Color::Blue),
            white: color_channel(Color::White),
            dimmer,
            white_pixel_dimmer: if pixel { dimmer } else { -1 },
            virtual_dimmer,
            reset,
            lamp_on,
            reserved: 0,
            reset_value,
            lamp_on_value,
            colors,
            flags,
            place,
        })
    }

    // ---- the presets ----

    fn capv_record(&mut self, value: &FixtureCapabilityValue) -> Result<f::CapV, CompileError> {
        let mut flags = 0;
        let mut record = f::CapV {
            ty: type_of(value.type_),
            color: value.color.map(|color| color.as_u8()).unwrap_or(0),
            ..f::CapV::default()
        };

        if let Some(percentage) = value.value_percentage {
            flags |= f::capv_flag::PERCENTAGE;
            record.percentage = percentage as f32;
        }

        if let Some(slot) = value.slot_number {
            flags |= f::capv_flag::SLOT;
            record.slot = slot as f32;
        }

        if is_number(value.value_degrees) {
            flags |= f::capv_flag::DEGREES;
            record.degrees = value.value_degrees.unwrap_or(0.0) as f32;
        }

        if is_number(value.spread_degrees) {
            flags |= f::capv_flag::SPREAD;
            record.spread = value.spread_degrees.unwrap_or(0.0) as f32;
            self.features |= if value.spread_degrees != Some(0.0) {
                f::feature::FANS
            } else {
                0
            };
        }

        if let (true, Some(x), Some(y), Some(z)) = (
            is_number(value.aim_x) && is_number(value.aim_y) && is_number(value.aim_z),
            value.aim_x,
            value.aim_y,
            value.aim_z,
        ) {
            flags |= f::capv_flag::AIM;
            self.features |= f::feature::AIMS;

            let spot = [x, y, z];
            record.aim = match self
                .aims
                .iter()
                .position(|known| known.map(f64::to_bits) == spot.map(f64::to_bits))
            {
                Some(index) => index as u32,
                None => {
                    self.aims.push(spot);
                    self.aims.len() as u32 - 1
                }
            };
        }

        if value.program_number.is_some() {
            flags |= f::capv_flag::PROGRAM;
            record.program =
                java_round(value.program_number.unwrap_or(0.0)).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        }

        if value.sound_controlled == Some(true) {
            flags |= f::capv_flag::SOUND;
        }

        if let Some(stops) = value.fan.as_ref().filter(|stops| !stops.is_empty()) {
            flags |= f::capv_flag::FAN;
            self.features |= f::feature::FANS;

            let mut sorted: Vec<f::FanStop> = stops
                .iter()
                .map(|stop| f::FanStop {
                    position: stop.position as f32,
                    value: stop.value_percentage as f32,
                })
                .collect();
            // (a stable sort, like the one of Rocket Show)
            let mut order: Vec<usize> = (0..stops.len()).collect();
            order.sort_by(|a, b| {
                stops[*a]
                    .position
                    .partial_cmp(&stops[*b].position)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            sorted = order.into_iter().map(|index| sorted[index]).collect();
            record.fan = self.writer.list(&sorted);

            if value.fan_between.as_deref() == Some("steps") {
                flags |= f::capv_flag::FAN_STEPS;
            }
        }

        if value.fan_shape.as_deref() == Some("center") {
            flags |= f::capv_flag::FAN_CENTER;
        }

        record.shutter = shutter_of_value(value.shutter_effect.as_deref());
        record.maintenance = match MaintenanceAction::of(value.maintenance_action.as_deref()) {
            Some(MaintenanceAction::Reset) => f::maintenance::RESET,
            Some(MaintenanceAction::LampOn) => f::maintenance::LAMP_ON,
            Some(MaintenanceAction::LampOff) => f::maintenance::LAMP_OFF,
            None => f::maintenance::NONE,
        };
        record.profile = self.names.of(value.profile_uuid.as_deref())?;
        record.wheel = self.names.of(value.wheel.as_deref())?;
        record.blade = self.names.of(value.blade.as_deref())?;
        record.flags = flags;

        Ok(record)
    }

    fn curve_record(&mut self, curve: &EffectCurve, kind: u8) -> Result<f::Effect, CompileError> {
        let beat_synced = curve.length_mode == "beats";
        let mut flags = 0;

        if beat_synced {
            flags |= f::effect_flag::BEAT_SYNCED;
        }

        if beat_synced || curve.phasing_mode == "beats" {
            self.features |= f::feature::BEATS;
        }

        if curve.pan_tilt_in_degrees {
            flags |= f::effect_flag::PAN_TILT_IN_DEGREES;
        }

        if curve.end_mode == "base" {
            flags |= f::effect_flag::END_BASE;
        }

        if curve
            .capabilities
            .iter()
            .any(|capability| capability.type_ == Some(Type::Intensity))
        {
            flags |= f::effect_flag::INTENSITY;
        }

        if matches!(curve.fan_mode.as_str(), "linear" | "center" | "side") {
            self.features |= f::feature::FANS;
        }

        let caps: Vec<f::CurveCap> = curve
            .capabilities
            .iter()
            .map(|capability| f::CurveCap {
                ty: type_of(capability.type_),
                color: capability.color.map(|color| color.as_u8()).unwrap_or(0),
                reserved: 0,
            })
            .collect();

        // the channels of the first entry of each profile
        let mut chans = Vec::new();
        let mut profiles: Vec<Option<&str>> = Vec::new();

        for entry in &curve.channels {
            if profiles.contains(&entry.profile_uuid.as_deref()) {
                continue;
            }

            profiles.push(entry.profile_uuid.as_deref());

            for channel in &entry.channels {
                chans.push(f::ChanV {
                    profile: self.names.of(entry.profile_uuid.as_deref())?,
                    name: self.names.of(Some(channel))?,
                    value: 0.0,
                });
            }
        }

        Ok(f::Effect {
            kind,
            curve: match curve.curve_type.as_str() {
                "sine" => f::curve::SINE,
                "square" => f::curve::SQUARE,
                "triangle" => f::curve::TRIANGLE,
                "sawtooth" => f::curve::SAWTOOTH,
                "reverse-sawtooth" => f::curve::REVERSE_SAWTOOTH,
                _ => f::curve::NONE,
            },
            phasing_mode: match curve.phasing_mode.as_str() {
                "spread" => f::phasing::SPREAD,
                "beats" => f::phasing::BEATS,
                _ => f::phasing::MILLIS,
            },
            run_mode: match curve.run_mode.as_str() {
                "cycles" => f::run::CYCLES,
                "duration" => f::run::DURATION,
                _ => f::run::INFINITE,
            },
            fan_mode: match curve.fan_mode.as_str() {
                "linear" => f::fan_mode::LINEAR,
                "center" => f::fan_mode::CENTER,
                "side" => f::fan_mode::SIDE,
                _ => f::fan_mode::NONE,
            },
            shape: 0,
            reserved: 0,
            flags,
            length: ns(curve.length_millis),
            phase: ns(curve.phase_millis),
            phasing: ns(curve.phasing_millis),
            run_duration: ns(curve.run_duration_millis.max(0.0)),
            length_beats: fix(curve.length_beats),
            phase_beats: fix(curve.phase_beats),
            phasing_beats: fix(curve.phasing_beats),
            phasing_cycles: fix(curve.phasing_cycles),
            group_size: java_round(curve.phasing_group_size).clamp(1, u32::MAX as i64) as u32,
            run_cycles: java_round(curve.run_cycles).clamp(1, u32::MAX as i64) as u32,
            amplitude: curve.amplitude as f32,
            position: curve.position as f32,
            duty: curve.duty_cycle as f32,
            size_pan: 0.0,
            size_tilt: 0.0,
            caps: self.writer.list(&caps),
            chans: self.writer.list(&chans),
            pixel: List::EMPTY,
        })
    }

    fn effect_record(&mut self, effect: &Effect) -> Result<Option<f::Effect>, CompileError> {
        if !effect.base().visible {
            // an effect switched off plays nothing
            return Ok(None);
        }

        Ok(Some(match effect {
            Effect::Unknown(unknown) => {
                self.warnings.push(format!(
                    "effects of the type {:?} are left out: this compiler doesn't know them",
                    unknown.type_.as_deref().unwrap_or("")
                ));
                return Ok(None);
            }
            Effect::Curve(curve) => {
                self.features |= f::feature::CURVES;
                self.curve_record(curve, f::effect_kind::CURVE)?
            }
            Effect::PanTilt(movement) => {
                self.features |= f::feature::MOVEMENTS;
                let mut record = self.curve_record(&movement.curve, f::effect_kind::MOVEMENT)?;
                record.shape = match movement.shape.as_str() {
                    "figure8" => f::movement::FIGURE8,
                    "line" => f::movement::LINE,
                    "wave" => f::movement::WAVE,
                    _ => f::movement::CIRCLE,
                };
                record.size_pan = movement.size_pan_degrees as f32;
                record.size_tilt = movement.size_tilt_degrees as f32;
                record
            }
            Effect::Pixel(pixel) => {
                self.features |= f::feature::PIXELS;
                let mut record = self.curve_record(&pixel.curve, f::effect_kind::PIXEL)?;
                let mut flags = 0;

                for (on, flag) in [
                    (pixel.bounce, f::pixel_flag::BOUNCE),
                    (pixel.mirror, f::pixel_flag::MIRROR),
                    (pixel.white_led, f::pixel_flag::WHITE_LED),
                    (pixel.still, f::pixel_flag::STILL),
                    (pixel.spread == "stage", f::pixel_flag::STAGE),
                    (pixel.view == "top", f::pixel_flag::VIEW_TOP),
                    (pixel.audio_source == "composition", f::pixel_flag::AUDIO),
                    (pixel.audio_follows == "size", f::pixel_flag::FOLLOWS_SIZE),
                    (
                        pixel.palette.ref_.as_deref() == Some("preset"),
                        f::pixel_flag::PRESET_PALETTE,
                    ),
                ] {
                    if on {
                        flags |= flag;
                    }
                }

                if flags & f::pixel_flag::AUDIO != 0 {
                    self.features |= f::feature::AUDIO;
                }

                let stops: Vec<(f64, &str)> = pixel
                    .palette
                    .stops
                    .iter()
                    .map(|stop| (stop.position, stop.color.as_str()))
                    .collect();
                let stop_bytes = px::encode::encode_stops(&stops);

                let bitmap_bytes = match &pixel.bitmap {
                    Some(bitmap) => {
                        self.features |= f::feature::PIXEL_BITMAPS;
                        let frames: Vec<Option<&str>> = bitmap.frames.iter().map(|frame| frame.as_deref()).collect();

                        px::encode::encode_bitmap(&px::encode::BitmapSource {
                            width: Some(bitmap.width),
                            height: Some(bitmap.height),
                            channels: Some(bitmap.channels),
                            frames: &frames,
                            durations: &bitmap.durations,
                            letter_starts: &bitmap.letter_starts,
                        })
                        .ok_or_else(|| CompileError::Limit("the picture of a pixel effect is too large".to_string()))?
                    }
                    None => Vec::new(),
                };

                let record_pixel = f::PixelFx {
                    flags,
                    pattern: px::Pattern::from_name(Some(&pixel.pattern)).as_u8(),
                    direction: px::Direction::from_name(Some(&pixel.direction)).as_u8(),
                    shape: px::Shape::from_name(Some(&pixel.shape)).as_u8(),
                    scroll: px::Scroll::from_name(Some(&pixel.scroll)).as_u8(),
                    align: px::Align::from_name(Some(&pixel.align)).as_u8(),
                    fit: px::Fit::from_name(Some(&pixel.fit)).as_u8(),
                    text_colors: px::TextColors::from_name(Some(&pixel.text_colors)).as_u8(),
                    image_colors: px::ImageColors::from_name(Some(&pixel.image_colors)).as_u8(),
                    between: px::Between::from_name(Some(&pixel.palette.between)).as_u8(),
                    repeat: px::Repeat::from_name(Some(&pixel.palette.repeat)).as_u8(),
                    blend: px::Blend::from_name(Some(&pixel.blend)).as_u8(),
                    audio_band: px::AudioBand::from_name(Some(&pixel.audio_band)).as_u8(),
                    size: pixel.size as f32,
                    softness: pixel.softness as f32,
                    offset_y: pixel.offset_y as f32,
                    audio_gain: pixel.audio_gain_db as f32,
                    audio_floor: pixel.audio_floor_db as f32,
                    audio_attack: pixel.audio_attack_millis as f32,
                    audio_release: pixel.audio_release_millis as f32,
                    seed: fix(pixel.seed * 7919.0),
                    stops: self.writer.list(&stop_bytes),
                    bitmap: self.writer.list(&bitmap_bytes),
                };

                // (never shared with another effect: the audio of a frame is kept by where this record is)
                record.pixel = self.unique_list(&[record_pixel]);
                record
            }
        }))
    }

    /// a list which is written on its own, even if the same bytes are in the show already
    fn unique_list<T: Field>(&mut self, items: &[T]) -> List<T> {
        self.writer.list_unshared(items)
    }

    fn preset_record(
        &mut self,
        preset: &Preset,
        layouts: &[f::Layout],
        part_layouts: &[u32],
    ) -> Result<(f::Preset, Vec<String>, usize), CompileError> {
        let project = self.project;
        let steps = steps_of(preset);
        let times = step_times(preset, &steps);
        let chase = chase_of(project, preset);
        let phased = steps_are_phased(preset, &steps);
        let stage_fan = stage::is_stage_fan_order(&preset.fan_order);

        if steps.len() > 1 {
            self.features |= f::feature::STEPS;
        }

        if is_ordered(preset) && steps.len() > 1 {
            self.features |= f::feature::STEP_ORDERS;

            if times.playable.len() > MAX_ORDER {
                return Err(CompileError::Limit(format!(
                    "a preset plays more than {MAX_ORDER} steps in a pass in another order than written down"
                )));
            }
        }

        if preset.steps_length_mode == "beats" {
            self.features |= f::feature::BEATS;
        }

        // the effects played (the ones switched off and the ones unknown are left out), with their uuids
        let mut effects = Vec::new();
        let mut effect_uuids: Vec<Option<&str>> = Vec::new();
        let mut stage_views = [false, false];

        for effect in &preset.effects {
            if let Some(record) = self.effect_record(effect)? {
                if let Effect::Pixel(pixel) = effect {
                    if pixel.spread == "stage" {
                        stage_views[(pixel.view == "top") as usize] = true;
                    }
                }

                effects.push(record);
                effect_uuids.push(effect.uuid());
            }
        }

        // the parts of the preset, in the order of the parts
        let mut members: Vec<(usize, i32)> = Vec::new();

        for (part_index, part) in self.parts.iter().enumerate() {
            let fixture = self.fixture_of(part);
            let key = part_key(
                fixture.uuid.as_deref(),
                part.pixel.as_ref().map(|pixel| pixel.key.as_str()),
            );

            if let Some(index) = chase.indices.get(&key) {
                members.push((part_index, *index));
            }
        }

        // where the parts are in the fan of a preset fanning by where they are on the stage
        let fan_places: Option<Vec<f64>> = if stage_fan {
            let positions: Vec<[f64; 3]> = members
                .iter()
                .map(|(part_index, _)| self.part_stage_position(*part_index))
                .collect();
            Some(stage::stage_fan_places(&positions, &preset.fan_order))
        } else {
            None
        };

        let pass_millis = if is_ordered(preset) {
            times.pass_length as f64 / 1e6
        } else {
            times.loop_length as f64 / 1e6
        };
        let mut parts = Vec::new();

        for (member, (part_index, preset_index)) in members.iter().enumerate() {
            let (index, count) = pattern_place(
                *preset_index,
                chase.count,
                preset.pattern_group_size,
                preset.pattern_wings,
                preset.pattern_every,
            );

            // its place on the stage (none if the fixtures can't be told apart there), or else in the chase
            let fan_place = match &fan_places {
                Some(places) => Some(places[member]).filter(|place| !place.is_nan()),
                None if count >= 2 => Some(index as f64 / (count - 1) as f64),
                None => None,
            };

            let fixture = self.fixture_of(&self.parts[*part_index]);

            parts.push(f::PresetPart {
                part: *part_index as u32,
                index: index.clamp(0, u16::MAX as i32) as u16,
                count: count.clamp(0, u16::MAX as i32) as u16,
                whole_index: chase
                    .whole_indices
                    .get(&fixture.uuid)
                    .copied()
                    .unwrap_or(0)
                    .clamp(0, u16::MAX as i32) as u16,
                flags: if fan_place.is_some() {
                    f::preset_part_flag::FAN_PLACE
                } else {
                    0
                },
                fan_place: fan_place.unwrap_or(0.0) as f32,
                phasing: if phased {
                    ns(steps_phasing(preset, index, count, pass_millis))
                } else {
                    0
                },
            });
        }

        // the places of the parts in the picture of the stage a pixel effect spread over it paints
        let mut stage_places: [List<f::StagePlace>; 2] = [List::EMPTY, List::EMPTY];

        for (view_index, view) in ["front", "top"].iter().enumerate() {
            if !stage_views[view_index] {
                continue;
            }

            // the parts a pixel effect lights: in red, green and blue, or in white only
            let lit: Vec<usize> = (0..members.len())
                .filter(|member| {
                    let layout = &layouts[part_layouts[members[*member].0] as usize];
                    layout.red >= 0 || layout.green >= 0 || layout.blue >= 0 || layout.white_pixel_dimmer >= 0
                })
                .collect();

            let positions: Vec<[f64; 3]> = lit
                .iter()
                .map(|member| self.part_stage_position(members[*member].0))
                .collect();
            let places = stage::pixel_places(&positions, view);
            let mut records = vec![f::StagePlace::default(); members.len()];

            for (place, member) in places.iter().zip(&lit) {
                records[*member] = f::StagePlace {
                    known: 1,
                    place: place_record(place),
                };
            }

            stage_places[view_index] = self.writer.list(&records);
        }

        // the steps
        let mut step_records = Vec::new();
        let mut step_uuids = Vec::new();
        // (each effect may follow the presets once on a fixture)
        let following = effects.len();

        for (index, step) in steps.iter().enumerate() {
            let mut caps = Vec::new();

            for value in crate::model::played_capability_values(&step.fixture_capability_values) {
                caps.push(self.capv_record(value)?);
            }

            if caps.len() > MAX_STEP_CAPS {
                return Err(CompileError::Limit(format!(
                    "a step sets more than {MAX_STEP_CAPS} capabilities"
                )));
            }

            let mut chans = Vec::new();

            for value in crate::model::played_channel_values(&step.fixture_channel_values) {
                if let Some(number) = value.value {
                    chans.push(f::ChanV {
                        profile: self.names.of(value.profile_uuid.as_deref())?,
                        name: self.names.of(value.channel_name.as_deref())?,
                        value: number as f32,
                    });
                }
            }

            let mut amounts = Vec::new();

            for amount in &step.effect_amounts {
                if amount.effect_uuid.is_none() {
                    continue;
                }

                for (effect_index, uuid) in effect_uuids.iter().enumerate() {
                    if *uuid == amount.effect_uuid.as_deref() {
                        amounts.push(f::Amount {
                            effect: effect_index as u32,
                            amount: amount.amount as f32,
                        });
                    }
                }
            }

            step_records.push(f::Step {
                start: times.starts[index],
                transition: step.transition_millis.map(ns).unwrap_or(0),
                length: times.lengths[index],
                flags: if step.transition_millis.is_some() {
                    f::step_flag::TRANSITION
                } else {
                    0
                },
                curve: curve_of(&step.transition_curve) as u32,
                caps: self.writer.list(&caps),
                chans: self.writer.list(&chans),
                amounts: self.writer.list(&amounts),
            });

            step_uuids.push(step.uuid.clone().unwrap_or_default());
        }

        let mut flags = 0;

        for (on, flag) in [
            (preset.mirror_pan, f::preset_flag::MIRROR_PAN),
            (preset.mirror_tilt, f::preset_flag::MIRROR_TILT),
            (preset.emitters_from_color, f::preset_flag::EMITTERS_FROM_COLOR),
            (preset.steps_loop, f::preset_flag::LOOP),
            (phased, f::preset_flag::PHASED),
            (preset.steps_length_mode == "beats", f::preset_flag::BEATS),
            (preset.start_millis.is_some(), f::preset_flag::START),
            (preset.end_millis.is_some(), f::preset_flag::END),
            (preset.fade_in_pre, f::preset_flag::FADE_IN_PRE),
            (preset.fade_out_post, f::preset_flag::FADE_OUT_POST),
        ] {
            if on {
                flags |= flag;
            }
        }

        let record = f::Preset {
            flags,
            order: if steps.len() < 2 {
                f::order::FORWARD
            } else {
                match preset.steps_order.as_str() {
                    "backward" => f::order::BACKWARD,
                    "pingpong" => f::order::PINGPONG,
                    "random" => f::order::RANDOM,
                    _ => f::order::FORWARD,
                }
            },
            fade_in_curve: curve_of(&preset.fade_in_curve),
            fade_out_curve: curve_of(&preset.fade_out_curve),
            reserved: 0,
            start: preset.start_millis.map(ns).unwrap_or(0),
            end: preset.end_millis.map(ns).unwrap_or(0),
            fade_in: ns(preset.fade_in_millis),
            fade_out: ns(preset.fade_out_millis),
            loop_length: times.loop_length,
            pass_length: times.pass_length,
            steps_bpm: fix(preset.steps_beats_per_minute),
            seed: preset.steps_seed as i64 as i32,
            whole_count: chase.whole_count.max(0) as u32,
            steps: self.writer.list(&step_records),
            effects: self.writer.list(&effects),
            parts: self.writer.list(&parts),
            playable: if is_ordered(preset) {
                self.writer.list(&times.playable)
            } else {
                List::EMPTY
            },
            stage_front: stage_places[0],
            stage_top: stage_places[1],
        };

        Ok((record, step_uuids, following))
    }
}

fn scene_start(scene: &Scene, start: i64) -> i64 {
    if scene.fade_in_pre {
        start - ns(scene.fade_in_millis)
    } else {
        start
    }
}

fn scene_end(scene: &Scene, end: i64) -> i64 {
    if scene.fade_out_post {
        end + ns(scene.fade_out_millis)
    } else {
        end
    }
}

/// when a preset played in the scene of a clip plays, its own fades outside its times included
fn preset_play(preset: &Preset, scene: &Scene, start: i64, end: i64) -> (i64, i64) {
    let play_start = match preset.start_millis {
        Some(millis) => start + ns(millis),
        None => scene_start(scene, start),
    } - if preset.fade_in_pre {
        ns(preset.fade_in_millis)
    } else {
        0
    };

    let play_end = match preset.end_millis {
        Some(millis) => start + ns(millis),
        None => scene_end(scene, end),
    } + if preset.fade_out_post {
        ns(preset.fade_out_millis)
    } else {
        0
    };

    (play_start, play_end)
}

/// the most of a weight at once over a list of spans
fn most_at_once(spans: &[(i64, i64, usize)]) -> usize {
    let mut events: Vec<(i64, i32, usize)> = Vec::new();

    for (start, end, weight) in spans {
        if end >= start {
            events.push((*start, 0, *weight));
            events.push((*end, 1, *weight));
        }
    }

    // a span ending where another one starts plays along with it at that moment
    events.sort();

    let mut current = 0usize;
    let mut most = 0;

    for (_, kind, weight) in events {
        if kind == 0 {
            current += weight;
            most = most.max(current);
        } else {
            current -= weight;
        }
    }

    most
}

/// Compile a project (of the current version) for an engine.
pub fn compile(project: &Project, options: &Options) -> Result<Compiled, CompileError> {
    let with_project = options.project_json.is_some();

    let mut compiler = Compiler {
        project,
        writer: Writer::new(f::show_chunk_offset(with_project)),
        names: Names::new(),
        features: 0,
        warnings: Vec::new(),
        parts: Vec::new(),
        places: Vec::new(),
        placements: HashMap::new(),
        aims: Vec::new(),
        addresses: HashMap::new(),
    };

    compiler.build_parts();
    compiler.stage_placements();

    let fixtures = fixtures_of(project);
    let mut index = Index::default();

    // the universes, in the order the fixtures name them in
    let mut patch = Vec::new();
    let mut fixture_records = Vec::new();

    for fixture in fixtures {
        let universe = match index
            .universes
            .iter()
            .position(|uuid| *uuid == fixture.dmx_universe_uuid)
        {
            Some(universe) => universe,
            None => {
                index.universes.push(fixture.dmx_universe_uuid.clone());
                index.universes.len() - 1
            }
        };

        patch.push(f::Patch {
            universe: universe as u32,
            address: fixture.dmx_first_channel,
            brightness: 1.0,
        });
        fixture_records.push(f::Fixture {
            profile: compiler.names.of(fixture.profile_uuid.as_deref())?,
            reserved: 0,
        });
        index.fixtures.push(fixture.uuid.clone().unwrap_or_default());
    }

    // the layouts of the parts, each once
    let mut layouts: Vec<f::Layout> = Vec::new();
    let mut layout_numbers: BTreeMap<Vec<u8>, u32> = BTreeMap::new();
    let mut part_layouts = Vec::new();

    // (the parts are taken out of the compiler while their records are written)
    let parts = std::mem::take(&mut compiler.parts);

    let mut kind_layouts: HashMap<(usize, usize, usize), u32> = HashMap::new();

    for (part_index, part) in parts.iter().enumerate() {
        // (the same for all fixtures of a profile and a mode)
        if let Some(number) = kind_layouts.get(&part.kind) {
            part_layouts.push(*number);
            continue;
        }

        let place = place_record(&compiler.places[part_index]);
        let layout = compiler.layout_record(part, place)?;
        let mut bytes = Vec::new();
        layout.write(&mut bytes);

        let number = *layout_numbers.entry(bytes).or_insert_with(|| {
            layouts.push(layout);
            layouts.len() as u32 - 1
        });

        kind_layouts.insert(part.kind, number);
        part_layouts.push(number);
    }

    compiler.parts = parts;

    // the parts: a part on the same universe, address and pixel as one before is the same lamp
    let mut part_records = Vec::new();
    let mut first_of_lamp: HashMap<(String, i32, Option<String>), usize> = HashMap::new();
    let mut value_base = 0u32;
    let mut max_part_channels = 0;

    for (part_index, part) in compiler.parts.iter().enumerate() {
        let fixture = &fixtures[part.fixture];
        let pixel_key = part
            .pixel
            .as_ref()
            .and_then(|pixel| empty_to_none(Some(&pixel.key)))
            .map(str::to_string);
        let key = (
            fixture.dmx_universe_uuid.clone(),
            fixture.dmx_first_channel,
            pixel_key.clone(),
        );
        let mut same_lamp = -1;

        match first_of_lamp.get(&key) {
            Some(first) => {
                let other = &compiler.parts[*first];

                // (it takes the values of the other one channel by channel, which have to be the same ones)
                if other.channels.len() == part.channels.len()
                    && other.channels.iter().zip(&part.channels).all(|(a, b)| a.name == b.name)
                {
                    same_lamp = *first as i32;
                } else {
                    compiler.warnings.push(format!(
                        "fixture {:?} shares its address with another fixture with other channels: it is played on its own",
                        fixture.uuid.as_deref().unwrap_or("")
                    ));
                }
            }
            None => {
                first_of_lamp.insert(key, part_index);
            }
        }

        let channels = part.channels.len() as u32;
        max_part_channels = max_part_channels.max(channels);

        part_records.push(f::Part {
            fixture: part.fixture as u32,
            layout: part_layouts[part_index],
            same_lamp,
            value_base,
            flags: if part.pixel.is_some() { f::part_flag::PIXEL } else { 0 },
        });

        index.parts.push(PartIndex {
            fixture: part.fixture as u32,
            pixel_key: part.pixel.as_ref().map(|pixel| pixel.key.clone()),
            channels: part.channels.iter().map(|channel| channel.name.clone()).collect(),
        });
        value_base += channels;
    }

    // the presets
    let presets = presets_of(project);
    let mut preset_records = Vec::new();
    let mut preset_following = Vec::new();

    if presets.len() > u16::MAX as usize {
        return Err(CompileError::Limit("more than 65535 presets".to_string()));
    }

    for preset in presets {
        let (record, step_uuids, following) = compiler.preset_record(preset, &layouts, &part_layouts)?;
        preset_records.push(record);
        preset_following.push(following);
        index.presets.push(preset.uuid.clone().unwrap_or_default());
        index.steps.push(step_uuids);
    }

    let preset_by_uuid = |uuid: &str| presets.iter().position(|preset| preset.uuid.as_deref() == Some(uuid));

    // the scenes, with their presets in their layer order (each once)
    let scenes = scenes_of(project);
    let mut scene_records = Vec::new();
    let mut scene_presets: Vec<Vec<u16>> = Vec::new();

    for scene in scenes {
        let mut numbers: Vec<u16> = Vec::new();

        for uuid in scene.preset_uuids.as_deref().unwrap_or(&[]) {
            if let Some(number) = preset_by_uuid(uuid) {
                if !numbers.contains(&(number as u16)) {
                    numbers.push(number as u16);
                }
            }
        }

        scene_records.push(f::Scene {
            dimmer: scene.dimmer as f32,
            flags: if scene.fade_in_pre {
                f::scene_flag::FADE_IN_PRE
            } else {
                0
            } | if scene.fade_out_post {
                f::scene_flag::FADE_OUT_POST
            } else {
                0
            },
            fade_in: ns(scene.fade_in_millis),
            fade_out: ns(scene.fade_out_millis),
            fade_in_curve: curve_of(&scene.fade_in_curve),
            fade_out_curve: curve_of(&scene.fade_out_curve),
            reserved: 0,
            presets: compiler.writer.list(&numbers),
        });

        scene_presets.push(numbers);
        index.scenes.push(scene.uuid.clone().unwrap_or_default());
    }

    // the compositions: the presets of their clips in the order they are mixed in
    let mut composition_records = Vec::new();
    let mut max_layers = 1usize;
    let mut max_following = preset_following.iter().copied().max().unwrap_or(0);
    let mut max_regions = 0;
    let mut max_holds = 0;

    // the scenes selected in the designer or picked by hand play all at once at most (picked and flashed)
    let scene_layers: usize = scene_presets.iter().map(Vec::len).sum();
    let scene_following: usize = scene_presets
        .iter()
        .flat_map(|numbers| numbers.iter())
        .map(|number| preset_following[*number as usize])
        .sum();
    max_layers = max_layers.max(2 * scene_layers);
    max_following = max_following.max(2 * scene_following);

    for composition in compositions_of(project) {
        let regions = composition.scene_playback_regions.as_deref().unwrap_or(&[]);
        let region_records: Vec<f::Region> = regions
            .iter()
            .map(|region| f::Region {
                start: ns(region.start_millis),
                end: ns(region.end_millis),
            })
            .collect();

        if regions.len() > u16::MAX as usize || scenes.len() > u16::MAX as usize {
            return Err(CompileError::Limit("more than 65535 clips or scenes".to_string()));
        }

        // The tracks the preview plays: a hidden track is left out, and once a track is soloed only the soloed ones
        // are played. The show itself always plays every track.
        let shown: Vec<bool> = composition.tracks.iter().map(|track| !track.hidden).collect();
        let any_solo = composition.tracks.iter().any(|track| !track.hidden && track.solo);
        let mut clips = Vec::new();
        let mut spans = Vec::new();
        let mut following_spans = Vec::new();

        // the topmost track is processed last, so it overwrites the ones below it
        for (track_index, track) in composition.tracks.iter().enumerate().rev() {
            let preview = shown[track_index] && (!any_solo || track.solo);

            // the clips of the track, in the order they are played
            let mut track_regions: Vec<usize> = (0..regions.len())
                .filter(|region| track.uuid.is_some() && regions[*region].track_uuid == track.uuid)
                .collect();
            track_regions.sort_by(|a, b| {
                regions[*a]
                    .start_millis
                    .partial_cmp(&regions[*b].start_millis)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for region_index in track_regions {
                let region = &regions[region_index];

                // a clip without its scene is not played
                let scene_index = match scenes
                    .iter()
                    .position(|scene| scene.uuid.is_some() && scene.uuid == region.scene_uuid)
                {
                    Some(scene_index) => scene_index,
                    None => continue,
                };

                // the first preset of a scene is its topmost layer: it is processed last
                for preset_number in scene_presets[scene_index].iter().rev() {
                    let (play_start, play_end) = preset_play(
                        &presets[*preset_number as usize],
                        &scenes[scene_index],
                        region_records[region_index].start,
                        region_records[region_index].end,
                    );

                    clips.push(f::Clip {
                        play_start,
                        play_end,
                        preset: *preset_number,
                        scene: scene_index as u16,
                        region: region_index as u16,
                        flags: if preview { f::clip_flag::PREVIEW } else { 0 },
                    });

                    spans.push((play_start, play_end, 1));
                    following_spans.push((play_start, play_end, preset_following[*preset_number as usize]));
                }
            }
        }

        if !clips.is_empty() {
            compiler.features |= f::feature::TIMELINES;
        }

        max_layers = max_layers.max(most_at_once(&spans));
        max_following = max_following.max(most_at_once(&following_spans));
        max_regions = max_regions.max(regions.len());

        let mut holds: Vec<i64> = composition
            .markers
            .iter()
            .filter(|marker| marker.is_hold())
            .map(|marker| ns(marker.position_millis))
            .collect();
        holds.sort();
        max_holds = max_holds.max(holds.len());

        composition_records.push(f::Composition {
            bpm: fix(composition.beats_per_minute),
            flags: if composition.sync_type.as_deref() == Some("none") {
                f::composition_flag::SYNC_NONE
            } else {
                0
            },
            beats_per_bar: composition.time_signature_upper.max(1) as u32,
            clips: compiler.writer.list(&clips),
            regions: compiler.writer.list(&region_records),
            holds: compiler.writer.list(&holds),
        });

        index.compositions.push((
            composition.uuid.clone().unwrap_or_default(),
            composition.name.clone().unwrap_or_default(),
        ));
    }

    // where each spot the moving heads are aimed at points each fixture
    let mut aim_records = Vec::new();

    for spot in &compiler.aims {
        for fixture in fixtures {
            aim_records.push(match compiler.placements.get(fixture.uuid.as_deref().unwrap_or("")) {
                Some(placement) => {
                    let aim = stage::get_aim_degrees(placement, *spot);
                    f::Aim {
                        known: 1,
                        pan: aim.pan as f32,
                        tilt: aim.tilt as f32,
                    }
                }
                None => f::Aim::default(),
            });
        }
    }

    if compiler.features & !options.features != 0 {
        return Err(CompileError::Unsupported(compiler.features & !options.features));
    }

    let selected_composition = project
        .selected_composition_uuid
        .as_deref()
        .and_then(|uuid| {
            compositions_of(project)
                .iter()
                .position(|composition| composition.uuid.as_deref() == Some(uuid))
        })
        .map(|number| number as i32)
        .unwrap_or(-1);

    let root = f::Root {
        master_dimmer: project.master_dimmer_value as f32,
        selected_composition,
        fixtures: compiler.writer.list(&fixture_records),
        parts: compiler.writer.list(&part_records),
        layouts: compiler.writer.list(&layouts),
        presets: compiler.writer.list(&preset_records),
        scenes: compiler.writer.list(&scene_records),
        compositions: compiler.writer.list(&composition_records),
        aims: compiler.writer.list(&aim_records),
        total_channels: value_base,
        max_part_channels,
        max_layers: max_layers as u32,
        max_following: max_following as u32,
        max_regions: max_regions as u32,
        max_holds: max_holds as u32,
        universes: index.universes.len() as u32,
        audio_history: if compiler.features & f::feature::AUDIO != 0 {
            px::audio::MAX_FRAMES_BACK as u32 + 1
        } else {
            0
        },
    };

    let data = compiler.writer.into_data();

    if f::show_chunk_offset(with_project) as u64
        + f::Root::SIZE as u64
        + data.len() as u64
        + (patch.len() * f::Patch::SIZE) as u64
        + options.project_json.map(|json| json.len()).unwrap_or(0) as u64
        > u32::MAX as u64
    {
        return Err(CompileError::Limit("the show is larger than 4 GB".to_string()));
    }

    let show = f::assemble(
        compiler.features,
        memory_words(&root),
        &root,
        &data,
        &patch,
        options.project_json,
    );

    Ok(Compiled {
        show,
        index,
        features: compiler.features,
        warnings: compiler.warnings,
    })
}

/// Compile the JSON of a project (of the current version) for an engine.
pub fn compile_json(json: &str, options: &Options) -> Result<Compiled, CompileError> {
    let project = crate::model::parse_project(json).map_err(CompileError::Project)?;
    compile(&project, options)
}
