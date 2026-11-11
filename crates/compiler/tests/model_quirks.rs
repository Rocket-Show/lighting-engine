//! The defaults of the model and the rules its JSON is read by.

use engine_compiler::model::*;

fn json(text: &str) -> Json {
    Json::parse(text).expect("JSON")
}

fn preset(text: &str) -> Preset {
    Preset::from_json(&json(text)).expect("a preset")
}

fn profile(text: &str) -> FixtureProfile {
    parse_profile(text).expect("a profile")
}

// ---- the JSON ----

#[test]
fn reads_the_numbers_to_the_closest_double() {
    for (text, expected) in [
        ("127.49999999999999", 127.49999999999999),
        ("96.66666666666667", 96.66666666666667),
        ("0.30000000000000004", 0.30000000000000004),
        ("10.200000000000001", 10.200000000000001),
        ("2.2250738585072011e-308", 2.225073858507201e-308),
        ("5e-324", 5e-324),
        ("-0", -0.0),
        ("1E3", 1000.0),
        ("1e+3", 1000.0),
        ("123456789012345678901234567890", 1.2345678901234568e29),
    ] {
        let number = json(text).as_f64().expect("a number");
        assert_eq!(number.to_bits(), f64::to_bits(expected), "{text}");
    }
}

#[test]
fn reads_json_strictly() {
    for text in [
        "",
        "{",
        "[1,]",
        "{\"a\":1,}",
        "{a:1}",
        "01",
        "1.",
        ".5",
        "+1",
        "1e",
        "NaN",
        "'a'",
        "\"a\nb\"",
        "{} x",
        "[1 2]",
        "tru",
        "\"\\x\"",
        "// a\n1",
    ] {
        assert!(Json::parse(text).is_err(), "{text:?} is no JSON");
    }
    assert_eq!(
        json(" [1, \"a\\n\\u00e4\\ud83d\\ude00\\\"\", true, null, {}] "),
        Json::Array(vec![
            Json::Number(JsonNumber::from_i64(1)),
            Json::string("a\n\u{e4}\u{1f600}\""),
            Json::Bool(true),
            Json::Null,
            Json::Object(Vec::new()),
        ])
    );
    assert_eq!(json("\u{feff}1").as_f64(), Some(1.0));
}

#[test]
fn keeps_the_properties_as_they_are_written() {
    let object = json(r#"{"b": 1, "a": 2, "b": 3}"#);
    assert_eq!(object.as_object().expect("an object").len(), 3);
    // each name once: at the first place, with the last value
    assert_eq!(
        object
            .properties()
            .into_iter()
            .map(|(name, value)| (name, value.as_f64().unwrap()))
            .collect::<Vec<_>>(),
        vec![("b", 3.0), ("a", 2.0)]
    );
    assert_eq!(object.get("b").and_then(Json::as_f64), Some(3.0));
    let mut object = object;
    object.set("b", Json::Null);
    object.set("c", Json::Bool(true));
    assert_eq!(object, json(r#"{"b": null, "a": 2, "c": true}"#));
}

#[test]
fn writes_the_texts_of_scalars() {
    let texts: Vec<Result<String, String>> =
        json(r#"["a", 1, 1.50, 1e2, true, null, 1e30, 0.1, 12345678.9, 1E-5, -0]"#)
            .as_array()
            .unwrap()
            .iter()
            .map(Json::as_text)
            .collect();
    let expected = [
        "a",
        "1",
        "1.5",
        "100.0",
        "true",
        "",
        "1.0E30",
        "0.1",
        "1.23456789E7",
        "1.0E-5",
        "0",
    ];
    assert_eq!(
        texts,
        expected.iter().map(|text| Ok(text.to_string())).collect::<Vec<_>>()
    );
    assert!(json("[1]").as_text().is_err());
    assert!(json("{}").as_text().is_err());
}

#[test]
fn writes_doubles_in_their_shortest_digits() {
    for (value, expected) in [
        (1e7, "1.0E7"),
        (0.001, "0.001"),
        (9999999.0, "9999999.0"),
        (12345678.9, "1.23456789E7"),
        (1e21, "1.0E21"),
        (1.5e-7, "1.5E-7"),
        (-0.0, "-0.0"),
        (100.0, "100.0"),
        (2e-323, "2.0E-323"),
        (1.0e-4, "1.0E-4"),
        (123456789012345680.0, "1.2345678901234568E17"),
        (f64::NAN, "NaN"),
        (f64::NEG_INFINITY, "-Infinity"),
    ] {
        assert_eq!(double_to_text(value), expected);
    }
}

#[test]
fn trims_the_controls_and_the_spaces() {
    assert_eq!(trim_controls_and_spaces(" \t\r\n\u{0}a b\u{1f} "), "a b");
    // a no-break space is not trimmed
    assert_eq!(trim_controls_and_spaces("\u{a0}a\u{2028}"), "\u{a0}a\u{2028}");
}

// ---- the defaults ----

#[test]
fn a_project_without_anything_has_the_defaults() {
    let project = parse_project("{}").expect("a project");
    assert_eq!(project.version, 0);
    assert_eq!(project.uuid, None);
    assert_eq!(project.master_dimmer_value, 1.0);
    assert!(!project.step_preview_running);
    assert_eq!(project.selected_scene_uuids, Vec::<String>::new());
    assert_eq!(
        (
            project.stage_width_cm,
            project.stage_height_cm,
            project.stage_depth_cm,
            project.stage_floor_height_cm
        ),
        (600.0, 350.0, 600.0, 30.0)
    );
    assert!(!project.preview_preset);
    assert!(project.color_dimmers_at_full);
    assert_eq!(project.compositions, None);
    assert_eq!(project.fixture_profiles, None);
    assert_eq!(project.fixtures, None);
    assert_eq!(project.preset_fixtures, None);
    assert_eq!(project.scenes, None);
    assert_eq!(project.scene_folders, None);
    assert_eq!(project.presets, None);
    assert_eq!(project, Project::default());
    assert_eq!(SUPPORTED_PROJECT_VERSION, 11);
}

#[test]
fn a_preset_without_anything_has_the_defaults() {
    let preset = preset("{}");
    assert_eq!(preset, Preset::default());
    assert_eq!(preset.uuid, None);
    assert_eq!(preset.fixture_uuids, None);
    assert!(preset.fixtures.is_empty());
    assert!(preset.use_global_fixture_order);
    assert_eq!(preset.fixture_channel_values, None);
    assert_eq!(preset.fixture_capability_values, None);
    assert!(preset.steps.is_empty());
    assert!(!preset.steps_loop);
    assert_eq!(preset.steps_loop_millis, None);
    assert_eq!(preset.steps_phasing_mode, "millis");
    assert_eq!(
        (
            preset.steps_phasing_millis,
            preset.steps_phasing_cycles,
            preset.steps_phasing_group_size
        ),
        (0.0, 1.0, 1.0)
    );
    assert_eq!(preset.steps_length_mode, "millis");
    assert_eq!(preset.steps_beats_per_minute, 120.0);
    assert_eq!(preset.steps_order, "forward");
    assert_eq!(preset.steps_seed, 1.0);
    assert!(preset.effects.is_empty());
    assert!(!preset.mirror_pan && !preset.mirror_tilt);
    assert_eq!(preset.fan_order, "preset");
    assert_eq!(
        (preset.pattern_group_size, preset.pattern_wings, preset.pattern_every),
        (1.0, 1.0, 0.0)
    );
    assert!(!preset.emitters_from_color);
    assert_eq!((preset.start_millis, preset.end_millis), (None, None));
    assert_eq!((preset.fade_in_millis, preset.fade_out_millis), (0.0, 0.0));
    assert!(!preset.fade_in_pre && !preset.fade_out_post);
    assert_eq!(
        (preset.fade_in_curve.as_str(), preset.fade_out_curve.as_str()),
        ("linear", "linear")
    );
}

#[test]
fn the_other_structs_have_their_defaults() {
    let fixture = Fixture::default();
    assert_eq!(
        (
            fixture.dmx_universe_uuid.as_str(),
            fixture.dmx_first_channel,
            fixture.positioning.as_str()
        ),
        ("", 0, "topFront")
    );
    assert_eq!(fixture.mode_short_name, None);

    let step = PresetStep::default();
    assert_eq!(
        (
            step.start_millis,
            step.transition_millis,
            step.transition_curve.as_str()
        ),
        (0.0, None, "linear")
    );
    assert_eq!(PresetStepEffectAmount::default().amount, 1.0);

    let scene = Scene::default();
    assert_eq!(
        (
            scene.sort_index,
            scene.color_auto,
            scene.dimmer,
            scene.preset_uuids.clone()
        ),
        (0.0, true, 1.0, None)
    );
    assert_eq!(
        (
            scene.fade_in_millis,
            scene.fade_out_millis,
            scene.fade_in_curve.as_str()
        ),
        (0.0, 0.0, "linear")
    );

    let composition = Composition::default();
    assert_eq!(
        (
            composition.duration_millis,
            composition.beats_per_minute,
            composition.time_signature_upper
        ),
        (0, 120.0, 4)
    );
    assert_eq!(composition.scene_playback_regions, None);
    assert!(composition.tracks.is_empty() && composition.markers.is_empty());
    assert_eq!(CompositionMarker::default().type_, "label");
    assert!(!CompositionMarker::default().is_hold());

    let curve = EffectCurve::default();
    assert!(curve.base.visible);
    assert_eq!(curve.base.effect_channels, None);
    assert_eq!(
        (
            curve.curve_type.as_str(),
            curve.length_mode.as_str(),
            curve.length_millis,
            curve.length_beats
        ),
        ("sine", "millis", 2500.0, 4.0)
    );
    assert_eq!(
        (
            curve.phase_millis,
            curve.phase_beats,
            curve.amplitude,
            curve.position,
            curve.duty_cycle
        ),
        (0.0, 0.0, 1.0, 0.5, 0.5)
    );
    assert_eq!(
        (
            curve.run_mode.as_str(),
            curve.run_cycles,
            curve.run_duration_millis,
            curve.end_mode.as_str()
        ),
        ("infinite", 1.0, 5000.0, "hold")
    );
    assert_eq!(
        (
            curve.phasing_mode.as_str(),
            curve.phasing_millis,
            curve.phasing_beats,
            curve.phasing_cycles,
            curve.phasing_group_size
        ),
        ("millis", 0.0, 0.0, 1.0, 1.0)
    );
    assert_eq!((curve.fan_mode.as_str(), curve.pan_tilt_in_degrees), ("none", false));

    let pan_tilt = EffectPanTilt::default();
    assert_eq!(
        (
            pan_tilt.shape.as_str(),
            pan_tilt.size_pan_degrees,
            pan_tilt.size_tilt_degrees
        ),
        ("circle", 60.0, 30.0)
    );

    let pixel = EffectPixel::default();
    assert_eq!(
        (
            pixel.pattern.as_str(),
            pixel.size,
            pixel.softness,
            pixel.direction.as_str(),
            pixel.shape.as_str()
        ),
        ("chase", 0.25, 0.3, "right", "circle")
    );
    assert_eq!(
        (pixel.bounce, pixel.seed, pixel.mirror, pixel.white_led, pixel.still),
        (false, 1.0, false, false, false)
    );
    assert_eq!(pixel.palette.ref_.as_deref(), Some("preset"));
    assert_eq!(
        (
            pixel.palette.between.as_str(),
            pixel.palette.repeat.as_str(),
            pixel.palette.stops.len()
        ),
        ("smooth", "mirror", 0)
    );
    assert_eq!(
        (pixel.spread.as_str(), pixel.view.as_str(), pixel.blend.as_str()),
        ("fixture", "front", "replace")
    );
    assert_eq!(pixel.bitmap, None);
    assert_eq!(
        (
            pixel.scroll.as_str(),
            pixel.align.as_str(),
            pixel.offset_y,
            pixel.fit.as_str()
        ),
        ("auto", "center", 0.0, "contain")
    );
    assert_eq!(
        (pixel.text_colors.as_str(), pixel.image_colors.as_str()),
        ("along", "own")
    );
    assert_eq!(
        (
            pixel.audio_source.as_str(),
            pixel.audio_band.as_str(),
            pixel.audio_follows.as_str()
        ),
        ("off", "full", "brightness")
    );
    assert_eq!(
        (
            pixel.audio_gain_db,
            pixel.audio_floor_db,
            pixel.audio_attack_millis,
            pixel.audio_release_millis
        ),
        (0.0, -60.0, 20.0, 300.0)
    );
    assert!(pixel.audio_buses.is_empty());

    assert_eq!(PixelPalette::default().ref_, None);
    assert_eq!(
        (
            PixelPaletteStop::default().position,
            PixelPaletteStop::default().color.as_str()
        ),
        (0.0, "#000000")
    );
    let bitmap = PixelBitmap::default();
    assert_eq!((bitmap.width, bitmap.height, bitmap.channels), (0, 0, 1));

    let profile = FixtureProfile::default();
    assert!(profile.available_channels.is_empty() && profile.template_channels.is_empty() && profile.wheels.is_empty());
    assert_eq!(profile.modes, None);
    assert_eq!(FixtureModeChannel::default().name, "");
    assert_eq!(FixtureChannel::default().default_value, None);
    assert_eq!(FixtureCapability::default().dmx_range, None);
}

// ---- null, names nobody knows and what is written twice ----

#[test]
fn null_keeps_the_default() {
    let preset = preset(
        r#"{"uuid": null, "useGlobalFixtureOrder": null, "stepsPhasingCycles": null, "fanOrder": null, "steps": null, "effects": null,
            "fadeInCurve": null, "startMillis": null, "fixtures": null}"#,
    );
    assert_eq!(preset, Preset::default());
    let curve = Effect::from_json(&json(
        r#"{"type": "curve", "lengthMillis": null, "amplitude": null, "visible": null, "capabilities": null}"#,
    ))
    .unwrap();
    assert_eq!(curve, Effect::Curve(EffectCurve::default()));
    let stop = FanStop::from_json(&json(r#"{"position": null}"#)).unwrap();
    assert_eq!((stop.position, stop.value_percentage), (0.0, 0.0));
}

#[test]
fn unknown_properties_are_ignored() {
    let fixture = Fixture::from_json(&json(
        r#"{"uuid": "f", "somethingNew": {"a": [1]}, "DmxFirstChannel": 5, "dmxFirstChannel": 7}"#,
    ))
    .unwrap();
    assert_eq!((fixture.uuid.as_deref(), fixture.dmx_first_channel), (Some("f"), 7));
}

#[test]
fn the_last_of_two_properties_wins_but_null_never() {
    let fixture = Fixture::from_json(&json(
        r#"{"name": "a", "name": "b", "dmxFirstChannel": 1, "dmxFirstChannel": null}"#,
    ))
    .unwrap();
    assert_eq!((fixture.name.as_deref(), fixture.dmx_first_channel), (Some("b"), 1));
}

#[test]
fn some_fields_replace_empty_values() {
    assert_eq!(preset(r#"{"fanOrder": ""}"#).fan_order, "preset");
    assert_eq!(preset(r#"{"fanOrder": "left-right"}"#).fan_order, "left-right");
    // any other field keeps an empty text
    assert_eq!(preset(r#"{"stepsOrder": "", "fadeInCurve": ""}"#).steps_order, "");

    let value = FixtureCapabilityValue::from_json(&json(r#"{"type": "Pan", "fan": []}"#)).unwrap();
    assert_eq!(value.fan, None);
    let value = FixtureCapabilityValue::from_json(&json(
        r#"{"fan": [{"position": 1, "valuePercentage": 0.5}, {}], "fan": []}"#,
    ))
    .unwrap();
    assert_eq!(value.fan, None);
    let value = FixtureCapabilityValue::from_json(&json(
        r#"{"fan": [{"position": 1, "valuePercentage": 0.5}, {"valuePercentage": ""}]}"#,
    ))
    .unwrap();
    assert_eq!(
        value.fan,
        Some(vec![
            FanStop {
                position: 1.0,
                value_percentage: 0.5
            },
            FanStop {
                position: 0.0,
                value_percentage: 0.0
            }
        ])
    );

    let Effect::Curve(curve) =
        Effect::from_json(&json(r#"{"type": "curve", "fanMode": "", "curveType": ""}"#)).unwrap()
    else {
        panic!("a curve");
    };
    assert_eq!((curve.fan_mode.as_str(), curve.curve_type.as_str()), ("none", ""));

    let Effect::Pixel(pixel) = Effect::from_json(&json(
        r#"{"type": "pixel", "pattern": "", "direction": "", "shape": "", "spread": "", "view": "", "blend": "", "scroll": "", "align": "",
            "fit": "", "textColors": "", "imageColors": "", "audioSource": "", "audioBand": "", "audioFollows": "", "offsetY": "",
            "audioFloorDb": "", "whiteLed": "", "still": "", "palette": {"between": ""}}"#,
    ))
    .unwrap() else {
        panic!("a pixel effect");
    };
    // the palette is the one written, which has no ref and keeps its empty text
    let expected = EffectPixel {
        palette: PixelPalette {
            between: String::new(),
            ..PixelPalette::default()
        },
        ..EffectPixel::default()
    };
    assert_eq!(pixel, expected);

    let Effect::PanTilt(pan_tilt) = Effect::from_json(&json(
        r#"{"type": "pan-tilt", "shape": "", "sizePanDegrees": "", "lengthBeats": 8}"#,
    ))
    .unwrap() else {
        panic!("a movement");
    };
    assert_eq!(
        (
            pan_tilt.shape.as_str(),
            pan_tilt.size_pan_degrees,
            pan_tilt.curve.length_beats
        ),
        ("circle", 60.0, 8.0)
    );

    let composition = Composition::from_json(&json(r#"{"timeSignatureUpper": 0, "durationMillis": 1999.9}"#)).unwrap();
    assert_eq!(
        (composition.time_signature_upper, composition.duration_millis),
        (4, 1999)
    );
    assert_eq!(
        Composition::from_json(&json(r#"{"timeSignatureUpper": 3}"#))
            .unwrap()
            .time_signature_upper,
        3
    );

    let bitmap = |text: &str| PixelBitmap::from_json(&json(text)).unwrap();
    assert_eq!(bitmap(r#"{"channels": 3}"#).channels, 3);
    assert_eq!(bitmap(r#"{"channels": 2}"#).channels, 1);
    assert_eq!(
        bitmap(r#"{"channels": 3, "channels": 4, "width": "", "height": 2.9}"#),
        PixelBitmap {
            height: 2,
            ..PixelBitmap::default()
        }
    );
    let bitmap =
        bitmap(r#"{"frames": ["AAEC", null], "durations": [100, null, "40", ""], "letterStarts": [0, 4.7, null]}"#);
    assert_eq!(bitmap.frames, vec![Some("AAEC".to_string()), None]);
    assert_eq!(bitmap.durations, vec![Some(100.0), None, Some(40.0), None]);
    assert_eq!(bitmap.letter_starts, vec![Some(0), Some(4), None]);
}

#[test]
fn scalars_are_coerced() {
    let fixture = |text: &str| Fixture::from_json(&json(text));
    // a whole number
    assert_eq!(fixture(r#"{"dmxFirstChannel": 1.9}"#).unwrap().dmx_first_channel, 1);
    assert_eq!(fixture(r#"{"dmxFirstChannel": -1.9}"#).unwrap().dmx_first_channel, -1);
    assert_eq!(fixture(r#"{"dmxFirstChannel": 1e3}"#).unwrap().dmx_first_channel, 1000);
    assert_eq!(fixture(r#"{"dmxFirstChannel": " 12 "}"#).unwrap().dmx_first_channel, 12);
    assert_eq!(
        fixture(r#"{"dmxFirstChannel": "+0012"}"#).unwrap().dmx_first_channel,
        12
    );
    assert_eq!(fixture(r#"{"dmxFirstChannel": "-5"}"#).unwrap().dmx_first_channel, -5);
    for wrong in [
        r#""1.5""#,
        r#""""#,
        r#"" ""#,
        "true",
        "3000000000",
        r#""12345678901""#,
        "2147483647.9",
        "-2147483648.9",
        r#""abc""#,
        "[1]",
        "{}",
    ] {
        assert!(
            fixture(&format!(r#"{{"dmxFirstChannel": {wrong}}}"#)).is_err(),
            "{wrong}"
        );
    }
    // a number
    assert_eq!(fixture(r#"{"positionX": " 2.5 "}"#).unwrap().position_x, 2.5);
    assert_eq!(fixture(r#"{"positionX": "1e2"}"#).unwrap().position_x, 100.0);
    assert_eq!(fixture(r#"{"positionX": 7}"#).unwrap().position_x, 7.0);
    assert!(fixture(r#"{"positionX": "NaN"}"#).unwrap().position_x.is_nan());
    assert!(fixture(r#"{"positionX": "-NaN"}"#).unwrap().position_x.is_nan());
    assert_eq!(
        fixture(r#"{"positionX": "-Infinity"}"#).unwrap().position_x,
        f64::NEG_INFINITY
    );
    assert_eq!(
        fixture(r#"{"positionX": "-INF"}"#).unwrap().position_x,
        f64::NEG_INFINITY
    );
    assert_eq!(
        fixture(r#"{"positionX": "+Infinity"}"#).unwrap().position_x,
        f64::INFINITY
    );
    for (written, expected) in [
        ("1d", 1.0),
        ("1F", 1.0),
        ("1e5d", 100000.0),
        ("+5", 5.0),
        (".5", 0.5),
        ("5.", 5.0),
    ] {
        assert_eq!(
            fixture(&format!(r#"{{"positionX": "{written}"}}"#)).unwrap().position_x,
            expected,
            "{written}"
        );
    }
    for wrong in [
        r#""""#,
        "true",
        r#""abc""#,
        r#""0x10""#,
        "[1]",
        "{}",
        r#""+INF""#,
        r#""infinity""#,
        r#""nan""#,
        r#""1e""#,
        r#""1,5""#,
        r#""1 5""#,
        r#""1L""#,
        r#""--1""#,
    ] {
        assert!(fixture(&format!(r#"{{"positionX": {wrong}}}"#)).is_err(), "{wrong}");
    }
    // a text
    assert_eq!(fixture(r#"{"name": 1.50}"#).unwrap().name.as_deref(), Some("1.50"));
    assert_eq!(fixture(r#"{"name": 1e2}"#).unwrap().name.as_deref(), Some("1e2"));
    assert_eq!(fixture(r#"{"name": true}"#).unwrap().name.as_deref(), Some("true"));
    assert_eq!(fixture(r#"{"name": ""}"#).unwrap().name.as_deref(), Some(""));
    assert!(fixture(r#"{"name": ["a"]}"#).is_err());
    assert!(fixture(r#"{"name": {}}"#).is_err());

    // a number and a boolean which may be left empty take an empty text for null
    let value = |text: &str| FixtureChannelValue::from_json(&json(text));
    assert_eq!(
        value(r#"{"value": "", "deactivated": ""}"#).unwrap(),
        FixtureChannelValue::default()
    );
    assert_eq!(
        value(r#"{"value": " ", "deactivated": 0}"#).unwrap().deactivated,
        Some(false)
    );
    assert_eq!(
        value(r#"{"value": "2.5", "deactivated": "TRUE"}"#).unwrap(),
        FixtureChannelValue {
            value: Some(2.5),
            deactivated: Some(true),
            ..Default::default()
        }
    );
    assert!(value(r#"{"value": false}"#).is_err());
    assert!(value(r#"{"deactivated": "x"}"#).is_err());

    // a boolean
    let track = |text: &str| Track::from_json(&json(text));
    assert!(track(r#"{"hidden": "true", "solo": 2}"#).unwrap().hidden);
    assert!(track(r#"{"hidden": "true", "solo": 2}"#).unwrap().solo);
    assert!(!track(r#"{"hidden": 0, "solo": "False"}"#).unwrap().hidden);
    for wrong in [r#""""#, r#""yes""#, "1.0", "[]"] {
        assert!(track(&format!(r#"{{"hidden": {wrong}}}"#)).is_err(), "{wrong}");
    }

    // lists and objects are nothing else
    assert!(preset(r#"{"steps": [{}]}"#).steps.len() == 1);
    for wrong in [
        r#"{"steps": {}}"#,
        r#"{"steps": "a"}"#,
        r#"{"steps": [1]}"#,
        r#"{"steps": [""]}"#,
        r#"{"steps": [[]]}"#,
        r#"{"steps": [null]}"#,
        r#"{"fixtureUuids": [["a"]]}"#,
    ] {
        assert!(Preset::from_json(&json(wrong)).is_err(), "{wrong}");
    }
    assert!(parse_project("null").is_err());
    assert!(parse_project("[]").is_err());
    assert!(parse_project("{} {}").is_err());
}

// ---- the enums ----

#[test]
fn the_enums_have_their_names_and_their_numbers() {
    assert_eq!(FixtureCapabilityType::ALL.len(), 43);
    assert_eq!(FixtureCapabilityType::NoFunction.as_u8(), 0);
    assert_eq!(FixtureCapabilityType::Intensity.as_u8(), 4);
    assert_eq!(FixtureCapabilityType::ColorIntensity.as_u8(), 5);
    assert_eq!(FixtureCapabilityType::Pan.as_u8(), 8);
    assert_eq!(FixtureCapabilityType::Tilt.as_u8(), 10);
    assert_eq!(FixtureCapabilityType::Maintenance.as_u8(), 39);
    assert_eq!(FixtureCapabilityType::BeamPosition.as_u8(), 42);
    assert_eq!(
        FixtureCapabilityType::from_u8(42),
        Some(FixtureCapabilityType::BeamPosition)
    );
    assert_eq!(FixtureCapabilityType::from_u8(43), None);
    for (index, capability_type) in FixtureCapabilityType::ALL.iter().enumerate() {
        assert_eq!(capability_type.as_u8() as usize, index);
        assert_eq!(FixtureCapabilityType::from_u8(index as u8), Some(*capability_type));
        assert_eq!(
            FixtureCapabilityType::from_json_name(capability_type.json_name()),
            Some(*capability_type)
        );
        assert_eq!(format!("{capability_type:?}"), capability_type.json_name());
    }

    let colors = [
        "Red",
        "Green",
        "Blue",
        "Cyan",
        "Magenta",
        "Yellow",
        "Amber",
        "White",
        "Warm White",
        "Cold White",
        "UV",
        "Lime",
        "Indigo",
    ];
    assert_eq!(
        FixtureCapabilityColor::ALL
            .iter()
            .map(|color| color.json_name())
            .collect::<Vec<_>>(),
        colors
    );
    assert_eq!(FixtureCapabilityColor::from_u8(0), None);
    assert_eq!(FixtureCapabilityColor::Red.as_u8(), 1);
    assert_eq!(FixtureCapabilityColor::WarmWhite.as_u8(), 9);
    assert_eq!(FixtureCapabilityColor::Indigo.as_u8(), 13);
    assert_eq!(
        FixtureCapabilityColor::from_u8(13),
        Some(FixtureCapabilityColor::Indigo)
    );
    assert_eq!(FixtureCapabilityColor::from_u8(14), None);
    for (index, color) in FixtureCapabilityColor::ALL.iter().enumerate() {
        assert_eq!(color.as_u8() as usize, index + 1);
        assert_eq!(FixtureCapabilityColor::from_u8(index as u8 + 1), Some(*color));
    }

    assert_eq!(FixtureWheelSlotType::ALL.len(), 9);
    assert_eq!(FixtureWheelSlotType::AnimationGoboEnd.as_u8(), 8);
    assert_eq!(FixtureCategory::ALL.len(), 17);
    assert_eq!(FixtureCategory::MovingHead.json_name(), "Moving Head");
    assert_eq!(
        EffectChannel::ALL
            .iter()
            .map(|channel| channel.json_name())
            .collect::<Vec<_>>(),
        ["colorRed", "colorGreen", "colorBlue", "dimmer", "pan", "tilt"]
    );
}

#[test]
fn enums_are_read_by_name_or_by_place() {
    let color = |text: &str| {
        FixtureCapability::from_json(&json(&format!(r#"{{"color": {text}}}"#))).map(|capability| capability.color)
    };
    assert_eq!(color(r#""Red""#), Ok(Some(FixtureCapabilityColor::Red)));
    assert_eq!(color(r#""Warm White""#), Ok(Some(FixtureCapabilityColor::WarmWhite)));
    assert_eq!(color(r#"" Red""#), Ok(Some(FixtureCapabilityColor::Red)));
    // a name nobody knows is read as missing
    assert_eq!(color(r#""WarmWhite""#), Ok(None));
    assert_eq!(color(r#""red""#), Ok(None));
    assert_eq!(color(r#""""#), Ok(None));
    assert_eq!(color(r#""Pink""#), Ok(None));
    // a whole number is the place in the enum, counted from 0
    assert_eq!(color("8"), Ok(Some(FixtureCapabilityColor::WarmWhite)));
    assert_eq!(color(r#""8""#), Ok(Some(FixtureCapabilityColor::WarmWhite)));
    assert_eq!(color(r#"" 8 ""#), Ok(Some(FixtureCapabilityColor::WarmWhite)));
    assert_eq!(color(r#""08""#), Ok(None));
    assert_eq!(color(r#""8a""#), Ok(None));
    assert_eq!(color(r#""+8""#), Ok(None));
    assert_eq!(color(r#""99999999999""#), Ok(None));
    assert_eq!(color("13"), Ok(None));
    assert_eq!(color("-1"), Ok(None));
    assert!(color("99999999999").is_err());
    assert!(color("true").is_err());
    assert!(color("1.0").is_err());
    assert!(color("[]").is_err());

    // a name nobody knows keeps the value it had before
    let capability = FixtureCapability::from_json(&json(r#"{"type": "Pan", "type": "PanOfTomorrow"}"#)).unwrap();
    assert_eq!(capability.type_, Some(FixtureCapabilityType::Pan));

    let base = EffectBase::from_json(&json(r#"{"effectChannels": ["dimmer", "hue", null, "pan"]}"#)).unwrap();
    assert_eq!(
        base.effect_channels,
        Some(vec![Some(EffectChannel::Dimmer), None, None, Some(EffectChannel::Pan)])
    );
    let profile = profile(r#"{"categories": ["Moving Head", "Color Changer", "Hologram"]}"#);
    assert_eq!(
        profile.categories,
        Some(vec![
            Some(FixtureCategory::MovingHead),
            Some(FixtureCategory::ColorChanger),
            None
        ])
    );
}

// ---- the effects ----

#[test]
fn an_effect_is_what_its_type_says() {
    let effects = preset(
        r#"{"effects": [
            {"type": "curve", "uuid": "a", "lengthMillis": 2},
            {"type": "pan-tilt", "shape": "figure8", "uuid": "b", "phasingMode": "spread"},
            {"type": "pixel", "shape": "square", "uuid": "c", "visible": false},
            {"type": "laser", "uuid": "d", "lengthMillis": 3},
            {"uuid": "e"},
            {"uuid": "late", "lengthMillis": 9, "type": "curve"},
            {"type": null, "type": "curve", "uuid": "f"},
            {"type": "curve", "type": "new", "uuid": "g"},
            {"type": "new", "type": "curve", "uuid": "h"},
            {"type": 5, "uuid": "i"},
            {"type": " curve", "uuid": "j"},
            {"type": "new", "type": null}
        ]}"#,
    )
    .effects;
    let kinds: Vec<String> = effects
        .iter()
        .map(|effect| match effect {
            Effect::Curve(_) => "curve".to_string(),
            Effect::PanTilt(_) => "pan-tilt".to_string(),
            Effect::Pixel(_) => "pixel".to_string(),
            Effect::Unknown(unknown) => format!("unknown {:?}", unknown.type_),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "curve",
            "pan-tilt",
            "pixel",
            "unknown Some(\"laser\")",
            "unknown None",
            "curve",
            "curve",
            "curve",
            "unknown Some(\"curve\")",
            "unknown Some(\"5\")",
            "unknown Some(\" curve\")",
            "unknown Some(\"new\")"
        ]
    );
    assert_eq!(
        effects
            .iter()
            .map(|effect| effect.uuid().unwrap_or("-"))
            .collect::<Vec<_>>(),
        ["a", "b", "c", "d", "e", "late", "f", "g", "h", "i", "j", "-"]
    );
    assert_eq!(effects[0].curve().map(|curve| curve.length_millis), Some(2.0));
    assert_eq!(effects[5].curve().map(|curve| curve.length_millis), Some(9.0));
    assert!(effects[3].curve().is_none());
    assert!(!effects[2].base().visible);
    let Effect::PanTilt(pan_tilt) = &effects[1] else {
        panic!("a movement");
    };
    assert_eq!(
        (pan_tilt.shape.as_str(), pan_tilt.curve.phasing_mode.as_str()),
        ("figure8", "spread")
    );

    assert!(Effect::from_json(&json(r#"{"type": ["curve"]}"#)).is_err());
    assert!(Effect::from_json(&json(r#""curve""#)).is_err());
    assert!(Effect::from_json(&json("null")).is_err());
    assert!(Preset::from_json(&json(r#"{"effects": [null]}"#)).is_err());
}

// ---- the profiles ----

#[test]
fn the_channels_and_the_wheels_keep_their_order() {
    let profile = profile(
        r#"{"uuid": "p", "availableChannels": {"Zoom": {}, "Dimmer": {"defaultValue": 128}, "Alpha": {"defaultValue": "50%"}, "Zoom": {"defaultValue": 1.50}},
            "templateChannels": {"Red $pixelKey": {}, "Blue $pixelKey": {"fineChannelAliases": ["Blue $pixelKey fine"]}},
            "wheels": {"Gobo Wheel": {"slots": [{"type": "Open"}, {"type": "Gobo", "name": "Dots"}, {"type": "Hologram"}]}, "Color Wheel": {}}}"#,
    );
    assert_eq!(
        profile.available_channels.keys().collect::<Vec<_>>(),
        ["Zoom", "Dimmer", "Alpha"]
    );
    // a number is kept as it is written
    assert_eq!(
        profile
            .available_channels
            .get("Zoom")
            .and_then(|channel| channel.default_value.as_deref()),
        Some("1.50")
    );
    assert_eq!(
        profile
            .available_channels
            .get("Dimmer")
            .and_then(|channel| channel.default_value.as_deref()),
        Some("128")
    );
    assert_eq!(
        profile.template_channels.keys().collect::<Vec<_>>(),
        ["Red $pixelKey", "Blue $pixelKey"]
    );
    assert_eq!(profile.wheels.keys().collect::<Vec<_>>(), ["Gobo Wheel", "Color Wheel"]);
    let slots = &profile.wheels.get("Gobo Wheel").unwrap().slots;
    assert_eq!(
        slots.iter().map(|slot| slot.type_).collect::<Vec<_>>(),
        [Some(FixtureWheelSlotType::Open), Some(FixtureWheelSlotType::Gobo), None]
    );
    assert!(profile.wheels.get("Color Wheel").unwrap().slots.is_empty());
    assert!(profile.wheels.get("Prism").is_none());

    // null keeps the empty map, anything but an object is an error
    assert!(self::profile(r#"{"availableChannels": null}"#)
        .available_channels
        .is_empty());
    assert!(parse_profile(r#"{"availableChannels": []}"#).is_err());
    assert!(parse_profile(r#"{"wheels": {"a": null}}"#).is_err());
}

#[test]
fn the_channels_of_a_mode_are_names_objects_or_nothing() {
    let profile = profile(
        r#"{"modes": [{"name": "Mode", "shortName": "m", "channels": [
            "Dimmer", null, {"insert": "matrixChannels", "repeatFor": "eachPixelABC", "channelOrder": "perPixel", "templateChannels": ["Red $pixelKey"]},
            {"repeatFor": ["1", 2, null, 2.50], "name": null}, {"repeatFor": 5}, 7, {"repeatFor": null}
        ]}, {"channels": null}, {"channels": "Dimmer"}, {"channels": {"a": "Dimmer", "b": null}}]}"#,
    );
    let modes = profile.modes.unwrap();
    let channels = &modes[0].channels;
    assert_eq!(channels.len(), 7);
    assert_eq!(
        channels[0],
        FixtureModeChannel {
            name: "Dimmer".to_string(),
            ..Default::default()
        }
    );
    assert_eq!(channels[1], FixtureModeChannel::default());
    assert_eq!(
        channels[2],
        FixtureModeChannel {
            name: String::new(),
            insert: Some("matrixChannels".to_string()),
            repeat_for: vec!["eachPixelABC".to_string()],
            channel_order: Some("perPixel".to_string()),
            template_channels: vec!["Red $pixelKey".to_string()],
        }
    );
    assert_eq!(channels[3].repeat_for, ["1", "2", "", "2.5"]);
    assert_eq!(channels[3].name, "");
    assert!(channels[4].repeat_for.is_empty());
    assert_eq!(channels[5], FixtureModeChannel::default());
    assert_eq!(channels[6], FixtureModeChannel::default());
    assert!(modes[1].channels.is_empty());
    assert!(modes[2].channels.is_empty());
    assert_eq!(
        modes[3]
            .channels
            .iter()
            .map(|channel| channel.name.as_str())
            .collect::<Vec<_>>(),
        ["Dimmer", ""]
    );
    assert!(parse_profile(r#"{"modes": [{"channels": [{"repeatFor": [["a"]]}]}]}"#).is_err());
}

#[test]
fn the_wheel_of_a_capability_is_a_name_a_list_or_anything() {
    let wheel = |text: &str| {
        FixtureCapability::from_json(&json(&format!(r#"{{"wheel": {text}}}"#)))
            .unwrap()
            .wheel
    };
    assert_eq!(wheel(r#""Gobo Wheel""#), Some(Wheel::Name("Gobo Wheel".to_string())));
    assert_eq!(wheel(r#""""#), Some(Wheel::Name(String::new())));
    assert_eq!(
        wheel(r#"["Gobo Wheel 1", "Gobo Wheel 2"]"#),
        Some(Wheel::Names(vec![
            "Gobo Wheel 1".to_string(),
            "Gobo Wheel 2".to_string()
        ]))
    );
    assert_eq!(wheel("[]"), Some(Wheel::Names(Vec::new())));
    // a name which is not written as a text is taken as its text
    assert_eq!(
        wheel(r#"["a", null, 1, 1.5, true, ["x", 2], {"k": "v"}, 1e2]"#)
            .and_then(|wheel| wheel.names().map(|names| names.join("|"))),
        Some("a|null|1|1.5|true|[x, 2]|{k=v}|100.0".to_string())
    );
    assert_eq!(wheel("1"), Some(Wheel::Other));
    assert_eq!(wheel(r#"{"a": 1}"#), Some(Wheel::Other));
    assert_eq!(wheel("null"), None);
    assert_eq!(Wheel::Other.names(), None);
    // the wheel of a value of a preset is a text
    assert_eq!(
        FixtureCapabilityValue::from_json(&json(r#"{"wheel": "Gobo Wheel"}"#))
            .unwrap()
            .wheel
            .as_deref(),
        Some("Gobo Wheel")
    );
}

#[test]
fn the_pixel_groups_come_with_the_whole_numbers_first() {
    let profile = profile(
        r#"{"matrix": {"pixelCount": [4, 1, 1], "pixelGroups": {
            "Master": "all", "10": ["4"], "2": ["2", 3], "01": {"x": ["<=2"], "name": []}, "1": {"isAll": false, "constraints": ["1"]},
            "-1": {"isAll": true}, "4294967295": "", "4294967294": 0, "nothing": {"isAll": false, "constraints": null}, "2": {"keys": ["2"], "y": ["odd"]}
        }}}"#,
    );
    let groups = profile.matrix.unwrap().pixel_groups.unwrap();
    assert_eq!(
        groups.iter().map(|group| group.name.as_str()).collect::<Vec<_>>(),
        [
            "1",
            "2",
            "10",
            "4294967294",
            "Master",
            "01",
            "-1",
            "4294967295",
            "nothing"
        ]
    );
    let group = |name: &str| groups.iter().find(|group| group.name == name).unwrap();
    let keys = |keys: &[&str]| {
        Some(FixtureMatrixPixelGroupConstraints {
            keys: Some(keys.iter().map(|key| key.to_string()).collect()),
            ..Default::default()
        })
    };
    assert_eq!(
        (group("Master").is_all, group("Master").constraints.clone()),
        (true, None)
    );
    assert_eq!((group("-1").is_all, group("-1").constraints.clone()), (true, None));
    assert_eq!(group("10").constraints, keys(&["4"]));
    assert_eq!(group("1").constraints, keys(&["1"]));
    // the last one written, where the first one is written
    assert_eq!(
        group("2").constraints,
        Some(FixtureMatrixPixelGroupConstraints {
            keys: Some(vec!["2".to_string()]),
            x: Some(Vec::new()),
            y: Some(vec!["odd".to_string()]),
            z: Some(Vec::new()),
            name: Some(Vec::new()),
        })
    );
    assert_eq!(
        group("01").constraints,
        Some(FixtureMatrixPixelGroupConstraints {
            keys: None,
            x: Some(vec!["<=2".to_string()]),
            y: Some(Vec::new()),
            z: Some(Vec::new()),
            name: Some(Vec::new())
        })
    );
    for nothing in ["4294967295", "4294967294", "nothing"] {
        assert_eq!(
            (group(nothing).is_all, group(nothing).constraints.clone()),
            (false, None),
            "{nothing}"
        );
    }

    assert_eq!(read_pixel_groups(&json("[1, 2]")), Ok(Vec::new()));
    assert_eq!(
        self::profile(r#"{"matrix": {"pixelGroups": null}}"#)
            .matrix
            .unwrap()
            .pixel_groups,
        None
    );
    assert_eq!(
        self::profile(r#"{"matrix": {"pixelGroups": "all"}}"#)
            .matrix
            .unwrap()
            .pixel_groups,
        Some(Vec::new())
    );
    assert_eq!(read_pixel_group("g", None).unwrap().constraints, None);
    assert_eq!(
        read_pixel_group("g", Some(&json("7"))).unwrap().constraints,
        Some(FixtureMatrixPixelGroupConstraints::default())
    );
    assert!(read_pixel_group("g", Some(&json(r#"[["a"]]"#))).is_err());
}

#[test]
fn the_pixel_keys_may_have_holes() {
    let matrix = profile(r#"{"matrix": {"pixelKeys": [[["A", null, 3]], [[]]], "pixelCount": [2, "3", 1.9]}}"#)
        .matrix
        .unwrap();
    assert_eq!(
        matrix.pixel_keys,
        Some(vec![
            vec![vec![Some("A".to_string()), None, Some("3".to_string())]],
            vec![vec![]]
        ])
    );
    assert_eq!(matrix.pixel_count, Some(vec![2, 3, 1]));
    assert!(parse_profile(r#"{"matrix": {"pixelKeys": [null]}}"#).is_err());
    assert!(parse_profile(r#"{"matrix": {"pixelCount": [1, null, 1]}}"#).is_err());
}

#[test]
fn the_ranges_are_scaled_to_the_resolution_of_the_channel() {
    let channel = |text: &str| FixtureChannel::from_json(&json(text)).unwrap();
    assert_eq!(FixtureChannel::get_resolution_multiplier(None), 1.0);
    assert_eq!(channel("{}").resolution_multiplier(), 1.0);
    assert_eq!(
        channel(r#"{"dmxValueResolution": "8bit"}"#).resolution_multiplier(),
        1.0
    );
    assert_eq!(
        channel(r#"{"dmxValueResolution": "8bit", "fineChannelAliases": ["fine"]}"#).resolution_multiplier(),
        256.0
    );
    assert_eq!(
        channel(r#"{"dmxValueResolution": "8bit", "fineChannelAliases": ["fine", "finer"]}"#).resolution_multiplier(),
        65536.0
    );
    assert_eq!(
        channel(r#"{"dmxValueResolution": "16bit", "fineChannelAliases": ["fine", "finer"]}"#).resolution_multiplier(),
        256.0
    );
    assert_eq!(
        channel(r#"{"dmxValueResolution": "16bit", "fineChannelAliases": ["fine"]}"#).resolution_multiplier(),
        1.0
    );
    assert_eq!(
        channel(r#"{"dmxValueResolution": "24bit", "fineChannelAliases": ["fine"]}"#).resolution_multiplier(),
        1.0
    );
    for unknown in ["32bit", "8 bit", "8bit ", "8Bit", ""] {
        assert_eq!(
            channel(&format!(
                r#"{{"dmxValueResolution": "{unknown}", "fineChannelAliases": ["fine"]}}"#
            ))
            .resolution_multiplier(),
            1.0,
            "{unknown}"
        );
    }

    let capability = FixtureCapability::from_json(&json(
        r#"{"type": "Intensity", "dmxRange": [0, 127], "comment": "half"}"#,
    ))
    .unwrap();
    let same = FixtureChannel::get_capability_in_full_resolution(&capability, 1.0);
    assert!(matches!(same, std::borrow::Cow::Borrowed(_)));
    let scaled = FixtureChannel::get_capability_in_full_resolution(&capability, 256.0);
    assert_eq!(scaled.dmx_range, Some(vec![0, 32767]));
    assert_eq!(
        (scaled.type_, scaled.comment.as_deref()),
        (Some(FixtureCapabilityType::Intensity), Some("half"))
    );
    let upper = FixtureCapability {
        dmx_range: Some(vec![128, 255]),
        ..Default::default()
    };
    assert_eq!(
        FixtureChannel::get_capability_in_full_resolution(&upper, 256.0).dmx_range,
        Some(vec![32768, 65535])
    );
    assert_eq!(
        FixtureChannel::get_capability_in_full_resolution(&upper, 65536.0).dmx_range,
        Some(vec![8388608, 16777215])
    );
    for range in [None, Some(vec![]), Some(vec![1]), Some(vec![1, 2, 3])] {
        let other = FixtureCapability {
            dmx_range: range.clone(),
            ..Default::default()
        };
        assert_eq!(
            FixtureChannel::get_capability_in_full_resolution(&other, 256.0).dmx_range,
            range
        );
    }
}

// ---- the helpers of the presets ----

#[test]
fn a_preset_mirrors_the_pan_and_the_tilt() {
    let preset = preset(r#"{"mirrorPan": true}"#);
    assert!(preset.mirrors_capability(Some(FixtureCapabilityType::Pan)));
    assert!(!preset.mirrors_capability(Some(FixtureCapabilityType::Tilt)));
    assert!(!preset.mirrors_capability(Some(FixtureCapabilityType::PanContinuous)));
    assert!(!preset.mirrors_capability(None));
    assert_eq!(
        preset.get_mirrored_value_percentage(Some(FixtureCapabilityType::Pan), 0.25),
        0.75
    );
    assert_eq!(
        preset.get_mirrored_value_percentage(Some(FixtureCapabilityType::Tilt), 0.25),
        0.25
    );
    let preset = self::preset(r#"{"mirrorTilt": true}"#);
    assert!(preset.mirrors_capability(Some(FixtureCapabilityType::Tilt)));
    assert!(!preset.mirrors_capability(Some(FixtureCapabilityType::Pan)));
}

#[test]
fn a_step_lets_through_the_effects_it_says_nothing_about() {
    let step = PresetStep::from_json(&json(
        r#"{"effectAmounts": [{"amount": 0.5}, {"effectUuid": "a", "amount": 0.25}, {"effectUuid": "a", "amount": 0.75}, {"effectUuid": "b"}, {"effectUuid": "c", "amount": 0}]}"#,
    ))
    .unwrap();
    assert_eq!(step.get_effect_amount(Some("a")), 0.25);
    assert_eq!(step.get_effect_amount(Some("b")), 1.0);
    assert_eq!(step.get_effect_amount(Some("c")), 0.0);
    assert_eq!(step.get_effect_amount(Some("d")), 1.0);
    assert_eq!(step.get_effect_amount(None), 1.0);
}

#[test]
fn the_values_turned_off_are_not_played() {
    let step = PresetStep::from_json(&json(
        r#"{"fixtureCapabilityValues": [{"type": "Pan"}, {"type": "Tilt", "deactivated": true}, {"type": "Zoom", "deactivated": false}],
            "fixtureChannelValues": [{"channelName": "a", "deactivated": true}, {"channelName": "b", "value": 3}]}"#,
    ))
    .unwrap();
    let played = played_capability_values(&step.fixture_capability_values);
    assert_eq!(
        played.iter().map(|value| value.type_).collect::<Vec<_>>(),
        [Some(FixtureCapabilityType::Pan), Some(FixtureCapabilityType::Zoom)]
    );
    let played = played_channel_values(&step.fixture_channel_values);
    assert_eq!(
        played
            .iter()
            .map(|value| value.channel_name.as_deref())
            .collect::<Vec<_>>(),
        [Some("b")]
    );
}

// ---- the profiles of the fixture library named by a project ----

#[test]
fn the_profiles_named_like_the_library_are_resolved() {
    let load = |name: &str| match name {
        "test/par" => Some(
            r#"{"name": "Par", "uuid": "old", "availableChannels": {"Red": {}, "Blue": {}, "Amber": {}}}"#.to_string(),
        ),
        "test/list" => Some("[]".to_string()),
        "test/broken" => Some("{".to_string()),
        "" => Some(r#"{"name": "None"}"#.to_string()),
        _ => None,
    };
    let text = r#"{"version": 11, "fixtureProfiles": [{"uuid": "own", "name": "Own"}, {"$profile": "test/par"}, 5, {"$profile": null}], "name": "p"}"#;

    let mut project = json(text);
    resolve_profiles_json(&mut project, &load).unwrap();
    let profiles = project.get("fixtureProfiles").and_then(Json::as_array).unwrap();
    assert_eq!(profiles[0], json(r#"{"uuid": "own", "name": "Own"}"#));
    assert_eq!(
        profiles[1],
        json(r#"{"name": "Par", "uuid": "test/par", "availableChannels": {"Red": {}, "Blue": {}, "Amber": {}}}"#)
    );
    assert_eq!(profiles[2], json("5"));
    assert_eq!(profiles[3], json(r#"{"name": "None", "uuid": ""}"#));
    assert_eq!(
        project
            .properties()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["version", "fixtureProfiles", "name"]
    );
    // a number among the profiles is none
    assert!(Project::from_json(&project).is_err());

    let mut value: serde_json::Value = serde_json::from_str(text).unwrap();
    resolve_profiles(&mut value, &load).unwrap();
    assert_eq!(value["fixtureProfiles"][1]["uuid"], "test/par");
    assert_eq!(value["fixtureProfiles"][1]["name"], "Par");
    assert_eq!(value["fixtureProfiles"][3]["uuid"], "");

    for wrong in [
        r#"{"fixtureProfiles": [{"$profile": "test/missing"}]}"#,
        r#"{"fixtureProfiles": [{"$profile": "test/list"}]}"#,
        r#"{"fixtureProfiles": [{"$profile": "test/broken"}]}"#,
        r#"{"fixtureProfiles": [{"$profile": ["test/par"]}]}"#,
        "[]",
    ] {
        assert!(resolve_profiles_json(&mut json(wrong), &load).is_err(), "{wrong}");
        assert!(
            resolve_profiles(&mut serde_json::from_str(wrong).unwrap(), &load).is_err(),
            "{wrong}"
        );
    }

    // a project without profiles gets an empty list of them
    let mut project = json(r#"{"name": "p"}"#);
    resolve_profiles_json(&mut project, &load).unwrap();
    assert_eq!(project, json(r#"{"name": "p", "fixtureProfiles": []}"#));
    assert_eq!(Project::from_json(&project).unwrap().fixture_profiles, Some(Vec::new()));
}

// ---- serde ----

#[test]
fn serde_reads_the_same_model() {
    let text = r#"{"version": 11, "name": "Show", "masterDimmerValue": 0.5,
        "fixtureProfiles": [{"uuid": "p", "availableChannels": {"Zoom": {}, "Dimmer": {"capability": {"type": "Intensity"}}},
            "matrix": {"pixelCount": [2, 1, 1], "pixelGroups": {"b": "all", "a": ["1"], "2": ["2"]}}}],
        "presets": [{"uuid": "x", "fanOrder": "", "effects": [{"type": "pixel", "pattern": "flow"}, {"type": "new"}]}]}"#;
    let own = parse_project(text).unwrap();
    let through_serde: Project = serde_json::from_str(text).unwrap();
    assert_eq!(own, through_serde);
    assert_eq!(
        own.fixture_profiles.as_ref().unwrap()[0]
            .available_channels
            .keys()
            .collect::<Vec<_>>(),
        ["Zoom", "Dimmer"]
    );
    let groups = own.fixture_profiles.as_ref().unwrap()[0]
        .matrix
        .as_ref()
        .unwrap()
        .pixel_groups
        .as_ref()
        .unwrap();
    assert_eq!(
        groups.iter().map(|group| group.name.as_str()).collect::<Vec<_>>(),
        ["2", "b", "a"]
    );
    let effect: Effect = serde_json::from_str(r#"{"type": "pan-tilt"}"#).unwrap();
    assert_eq!(effect, Effect::PanTilt(EffectPanTilt::default()));
    assert!(serde_json::from_str::<Project>(r#"{"version": "eleven"}"#).is_err());
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(Project::from_json(&Json::from(&value)).unwrap().presets, own.presets);
}
