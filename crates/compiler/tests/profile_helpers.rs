//! The helpers of the profiles against recorded cases: the regular expressions matched by hand,
//! the numbers and the properties of the fixture library, the order of the pixel keys and the
//! rules of the pixel groups (see the tables at the end), and the wheels, the modes and the
//! brightness of the profiles.

use std::cmp::Ordering;

use engine_compiler::model::{
    parse_profile, Fixture, FixtureCapability, FixtureMatrixPixelGroup, FixtureMatrixPixelGroupConstraints,
    FixtureModeChannel, FixtureProfile,
};
use engine_compiler::profile::*;

fn profile(text: &str) -> FixtureProfile {
    parse_profile(text).expect("a profile")
}

fn fixture(mode: Option<&str>) -> Fixture {
    Fixture {
        mode_short_name: mode.map(str::to_string),
        ..Fixture::default()
    }
}

fn keys(pixels: &[Pixel]) -> Vec<&str> {
    pixels.iter().map(|pixel| pixel.key.as_str()).collect()
}

// ---- against the recorded cases ----

#[test]
fn the_comments_of_maintenance_match_like_their_regular_expressions() {
    for (comment, expected) in MAINTENANCE {
        let actual = [
            maintenance_matches_exactly(MaintenanceAction::Reset, Some(comment)),
            maintenance_matches_exactly(MaintenanceAction::LampOn, Some(comment)),
            maintenance_matches_exactly(MaintenanceAction::LampOff, Some(comment)),
            maintenance_matches_among(MaintenanceAction::Reset, Some(comment)),
            maintenance_matches_among(MaintenanceAction::LampOn, Some(comment)),
            maintenance_matches_among(MaintenanceAction::LampOff, Some(comment)),
        ];
        assert_eq!(actual, *expected, "{comment:?}");
    }
}

#[test]
fn maintenance_names_its_actions() {
    assert_eq!(MaintenanceAction::of(Some("reset")), Some(MaintenanceAction::Reset));
    assert_eq!(MaintenanceAction::of(Some("lamp-on")), Some(MaintenanceAction::LampOn));
    assert_eq!(
        MaintenanceAction::of(Some("lamp-off")),
        Some(MaintenanceAction::LampOff)
    );
    assert_eq!(MaintenanceAction::of(Some("Reset")), None);
    assert_eq!(MaintenanceAction::of(None), None);
    assert_eq!(maintenance_comment(Some("  Lamp ON\t")), "lamp on");
    assert_eq!(maintenance_comment(None), "");
    for action in MaintenanceAction::ALL {
        assert_eq!(maintenance_match(action, None), MaintenanceMatch::None);
        assert!(!maintenance_matches_exactly(action, None) && !maintenance_matches_among(action, None));
    }
    assert_eq!(
        maintenance_match(MaintenanceAction::Reset, Some("Full Reset")),
        MaintenanceMatch::Exactly
    );
    assert_eq!(
        maintenance_match(MaintenanceAction::Reset, Some("Reset pan/tilt")),
        MaintenanceMatch::Among
    );
    assert_eq!(
        maintenance_match(MaintenanceAction::Reset, Some("Presets")),
        MaintenanceMatch::None
    );
    assert_eq!(
        maintenance_match(MaintenanceAction::LampOn, Some("Lamp on")),
        MaintenanceMatch::Exactly
    );
    assert_eq!(
        maintenance_match(MaintenanceAction::LampOn, Some("LampOn (hold 3s)")),
        MaintenanceMatch::Among
    );
    assert_eq!(
        maintenance_match(MaintenanceAction::LampOff, Some("Lamp on")),
        MaintenanceMatch::None
    );
    assert_eq!(
        maintenance_match(MaintenanceAction::LampOff, Some("lamp   off, reset")),
        MaintenanceMatch::Among
    );
}

// regular expressions which are valid but not supported: they have to be told apart from the
// ones which can't be read at all
const UNSUPPORTED_REGEXES: [&str; 8] = [
    "[a-c&&[b-d]]",
    "(?i)a",
    "a(?=b)",
    "\\p{L}",
    "\\1",
    "a++",
    "a?+",
    "(?<n>a)\\k<n>",
];

#[test]
fn the_regular_expressions_find_what_is_recorded() {
    for (pattern, expected) in REGEXES {
        let actual: Result<String, String> = REGEX_TEXTS
            .iter()
            .map(|text| regex_find(pattern, text).map(|found| if found { '1' } else { '0' }))
            .collect();
        match expected {
            None => assert!(actual.is_err(), "{pattern:?} can't be read, but came to {actual:?}"),
            Some(_) if UNSUPPORTED_REGEXES.contains(pattern) => {
                assert!(actual.unwrap_err().contains("not supported"), "{pattern:?}");
            }
            Some(expected) => assert_eq!(actual.as_deref(), Ok(*expected), "{pattern:?}"),
        }
    }
    for pattern in UNSUPPORTED_REGEXES {
        assert!(
            REGEXES
                .iter()
                .any(|(known, expected)| *known == pattern && expected.is_some()),
            "{pattern:?}"
        );
    }
    // an expression which takes forever is given up, and so is one which goes too deep
    assert!(regex_find("(a*)*(a*)*(a*)*(a*)*(a*)*b", &"a".repeat(40)).is_err());
    assert!(regex_find("a*b", &"a".repeat(100_000)).is_err());
    assert_eq!(regex_find("^a*b$", &format!("{}b", "a".repeat(300))), Ok(true));
    assert!(regex_find(&format!("{}a{}", "(".repeat(101), ")".repeat(101)), "a").is_err());
    assert_eq!(
        regex_find(&format!("{}a{}", "(".repeat(100), ")".repeat(100)), "a"),
        Ok(true)
    );
    assert_eq!(regex_find("^(a|b)*?c$", "ababc"), Ok(true));
    assert_eq!(regex_find("\\Bx\\B", "axa x"), Ok(true));
    assert_eq!(regex_find("\\Bx\\B", "a x"), Ok(false));
    assert_eq!(
        regex_find("a{1001}", "a"),
        Err("The regular expression \"a{1001}\" is not supported: that many repetitions (at 6)".to_string())
    );
}

#[test]
fn the_properties_of_the_fixture_library_lie_where_they_are_recorded() {
    for (text, rank, rotation_rank, nothing) in RANKS {
        assert_eq!(
            get_property_rank(Some(text)).map(f64::to_bits),
            *rank,
            "the rank of {text:?}"
        );
        assert_eq!(
            get_rotation_rank(Some(text)).map(f64::to_bits),
            *rotation_rank,
            "the rotation rank of {text:?}"
        );
        assert_eq!(is_nothing(Some(text)), *nothing, "{text:?} is nothing");
    }
    assert_eq!(get_property_rank(None), None);
    assert_eq!(get_rotation_rank(None), None);
    assert!(!is_nothing(None));
    assert_eq!(get_property_rank(Some("120bpm")), Some(2.0));
    assert_eq!(get_property_rank(Some("500ms")), Some(0.5));
    assert_eq!(get_rotation_rank(Some("fast CCW")), Some(-1e9));
}

#[test]
fn the_numbers_of_the_fixture_library_are_read_as_recorded() {
    for (text, number) in NUMBERS {
        assert_eq!(parse_number(text).map(f64::to_bits), *number, "{text:?}");
    }
}

#[test]
fn the_rules_of_the_pixel_groups_are_read_as_recorded() {
    for (rule, expected) in RULES {
        let actual: String = RULE_VALUES
            .iter()
            .map(|value| match match_number_rule(*value, rule) {
                Ok(true) => 'T',
                Ok(false) => 'F',
                Err(_) => 'E',
            })
            .collect();
        assert_eq!(actual, *expected, "{rule:?}");
    }
}

#[test]
fn the_pixel_keys_are_sorted_as_recorded() {
    for (key1, row) in KEYS.iter().zip(KEY_ORDER) {
        for (key2, expected) in KEYS.iter().zip(row.chars()) {
            let expected = match expected {
                '<' => Ordering::Less,
                '>' => Ordering::Greater,
                _ => Ordering::Equal,
            };
            assert_eq!(compare_keys(key1, key2), expected, "{key1:?} to {key2:?}");
        }
    }
}

#[test]
fn the_brightness_is_read_as_recorded() {
    for (brightness, without_lumens, with_lumens) in BRIGHTNESS {
        assert_eq!(
            get_brightness(Some(brightness), 0.0).map(f64::to_bits),
            *without_lumens,
            "{brightness:?}"
        );
        assert_eq!(
            get_brightness(Some(brightness), 2000.0).map(f64::to_bits),
            *with_lumens,
            "{brightness:?} of 2000 lm"
        );
    }
    assert_eq!(get_brightness(None, 2000.0), None);
    assert_eq!(parse_brightness_number("12.5%", "%"), Some(12.5));
    assert_eq!(parse_brightness_number("700lm", "lm"), Some(700.0));

    let capability = |brightness: Option<&str>, start: Option<&str>, end: Option<&str>| FixtureCapability {
        brightness: brightness.map(str::to_string),
        brightness_start: start.map(str::to_string),
        brightness_end: end.map(str::to_string),
        ..FixtureCapability::default()
    };
    let capabilities = [
        capability(Some("500lm"), None, None),
        capability(None, Some("off"), Some("2000lm")),
        capability(None, Some("3000lm"), Some("50%")),
        capability(Some("bright"), Some("9000lm"), None),
        capability(Some("xlm"), None, Some("lm")),
    ];
    assert_eq!(get_max_lumens(&capabilities), 9000.0);
    assert_eq!(get_max_lumens(&capabilities[..2]), 2000.0);
    assert_eq!(get_max_lumens(&capabilities[4..]), 0.0);
    assert_eq!(get_max_lumens(std::iter::empty()), 0.0);

    // from off to bright, if not given
    assert_eq!(get_brightness_range(&capability(None, None, None), 0.0), [0.0, 1.0]);
    assert_eq!(
        get_brightness_range(&capability(None, Some("bright"), Some("off")), 0.0),
        [1.0, 0.0]
    );
    assert_eq!(
        get_brightness_range(&capability(None, Some("20%"), None), 0.0),
        [0.2, 1.0]
    );
    assert_eq!(
        get_brightness_range(&capability(None, None, Some("dark")), 0.0),
        [0.0, 0.01]
    );
    // one brightness for all of the capability, whatever its ends say
    assert_eq!(
        get_brightness_range(&capability(Some("50%"), Some("off"), Some("bright")), 0.0),
        [0.5, 0.5]
    );
    assert_eq!(
        get_brightness_range(&capability(Some("500lm"), None, None), 2000.0),
        [0.25, 0.25]
    );
    // one which can't be read is off at the start and bright at the end
    assert_eq!(
        get_brightness_range(&capability(Some("500lm"), None, None), 0.0),
        [0.0, 1.0]
    );
    assert_eq!(
        get_brightness_range(&capability(Some("glowing"), None, None), 0.0),
        [0.0, 1.0]
    );
}

// ---- the pixel groups ----

#[test]
fn a_rule_is_only_read_once_a_pixel_gets_to_it() {
    let group = |x: &[&str], name: &[&str]| FixtureMatrixPixelGroup {
        name: "group".to_string(),
        is_all: false,
        constraints: Some(FixtureMatrixPixelGroupConstraints {
            x: Some(x.iter().map(|rule| rule.to_string()).collect()),
            name: Some(name.iter().map(|rule| rule.to_string()).collect()),
            ..Default::default()
        }),
    };
    let pixels = [Pixel::new("b", 2, 1, 1), Pixel::new("a", 3, 1, 1)];
    // no pixel is at 1, so nobody gets to the rule which can't be read
    assert_eq!(
        resolve_group_pixels(Some(&group(&["=1", "most"], &[])), &pixels),
        Ok(Vec::new())
    );
    assert!(resolve_group_pixels(Some(&group(&["=2", "most"], &[])), &pixels).is_err());
    assert_eq!(resolve_group_pixels(Some(&group(&["most"], &[])), &[]), Ok(Vec::new()));
    assert_eq!(
        resolve_group_pixels(Some(&group(&["=1"], &["("])), &pixels),
        Ok(Vec::new())
    );
    assert!(resolve_group_pixels(Some(&group(&["=2"], &["("])), &pixels).is_err());
    // constrained by their name, the pixels come sorted by their key
    assert_eq!(
        resolve_group_pixels(Some(&group(&[], &["[ab]"])), &pixels)
            .map(|pixels| pixels.iter().map(|pixel| pixel.x).collect::<Vec<_>>()),
        Ok(vec![3, 2])
    );
    assert_eq!(
        resolve_group_pixels(Some(&group(&[">=2"], &[])), &pixels)
            .map(|pixels| pixels.iter().map(|pixel| pixel.x).collect::<Vec<_>>()),
        Ok(vec![2, 3])
    );
    assert_eq!(resolve_group_pixels(None, &pixels), Ok(Vec::new()));

    let all = FixtureMatrixPixelGroup {
        name: "all".to_string(),
        is_all: true,
        constraints: None,
    };
    assert_eq!(
        resolve_group_pixels(Some(&all), &pixels).map(|pixels| pixels.iter().map(|pixel| pixel.x).collect::<Vec<_>>()),
        Ok(vec![3, 2])
    );
    let nothing = FixtureMatrixPixelGroup {
        name: "nothing".to_string(),
        is_all: false,
        constraints: None,
    };
    assert_eq!(resolve_group_pixels(Some(&nothing), &pixels), Ok(Vec::new()));
}

#[test]
fn a_key_comes_before_a_group_of_the_same_name() {
    let profile = profile(
        r#"{"matrix": {"pixelCount": [3, 1, 1], "pixelGroups": {"2": ["1", "3"], "Ends": ["3", "1", "9"], "Broken": {"x": ["x"]}}}}"#,
    );
    let all_pixels = get_all_pixel_keys(&profile);
    let matrix = profile.matrix.as_ref();
    assert_eq!(
        get_pixels_for_key_or_group(matrix, &all_pixels, "2"),
        Ok(vec![Pixel::new("2", 2, 1, 1)])
    );
    assert_eq!(
        get_pixels_for_key_or_group(matrix, &all_pixels, "Ends")
            .as_deref()
            .map(keys),
        Ok(vec!["3", "1"])
    );
    assert_eq!(
        get_pixels_for_key_or_group(matrix, &all_pixels, "Middle"),
        Ok(Vec::new())
    );
    assert_eq!(get_pixels_for_key_or_group(None, &all_pixels, "2"), Ok(Vec::new()));
    assert!(get_pixels_for_key_or_group(matrix, &all_pixels, "Broken").is_err());
}

// ---- the pixels ----

#[test]
fn the_pixels_of_a_matrix_are_named_like_the_fixture_library_names_them() {
    assert_eq!(get_default_pixel_key(&[12, 1, 1], 3, 1, 1), "3");
    assert_eq!(get_default_pixel_key(&[1, 4, 1], 1, 3, 1), "3");
    assert_eq!(get_default_pixel_key(&[1, 1, 4], 1, 1, 3), "3");
    assert_eq!(get_default_pixel_key(&[1, 1, 1], 1, 1, 1), "1");
    assert_eq!(get_default_pixel_key(&[3, 3, 1], 2, 3, 1), "(2, 3)");
    assert_eq!(get_default_pixel_key(&[3, 1, 3], 2, 1, 3), "(2, 3)");
    // on a plane of Y and Z, a pixel is named by its Y twice
    assert_eq!(get_default_pixel_key(&[1, 3, 3], 1, 2, 3), "(2, 2)");
    assert_eq!(get_default_pixel_key(&[2, 2, 2], 1, 2, 3), "(1, 2, 3)");

    assert!(get_all_pixel_keys(&profile("{}")).is_empty());
    assert!(get_all_pixel_keys(&profile(r#"{"matrix": {}}"#)).is_empty());
    assert!(get_all_pixel_keys(&profile(r#"{"matrix": {"pixelCount": [2, 2]}}"#)).is_empty());
    assert!(get_all_pixel_keys(&profile(r#"{"matrix": {"pixelCount": [2, 0, 1]}}"#)).is_empty());
    // the keys come before the count
    let both = profile(r#"{"matrix": {"pixelKeys": [[["L", null, "R"]]], "pixelCount": [5, 1, 1]}}"#);
    assert_eq!(
        get_all_pixel_keys(&both),
        [Pixel::new("L", 1, 1, 1), Pixel::new("R", 3, 1, 1)]
    );

    let size = |text: &str| get_matrix_size(profile(text).matrix.as_ref().unwrap());
    assert_eq!(
        size(
            r#"{"matrix": {"pixelKeys": [[["a", "b", null], ["c"]], [["d"], ["e"], ["f"]]], "pixelCount": [9, 9, 9]}}"#
        ),
        (3, 3)
    );
    assert_eq!(size(r#"{"matrix": {"pixelCount": [5, 2, 1]}}"#), (5, 2));
    assert_eq!(size(r#"{"matrix": {"pixelCount": [5]}}"#), (5, 0));
    assert_eq!(size(r#"{"matrix": {}}"#), (0, 0));

    let pixels = [
        Pixel::new("a", 1, 2, 1),
        Pixel::new("b", 2, 1, 1),
        Pixel::new("c", 1, 1, 2),
        Pixel::new("d", 1, 1, 1),
    ];
    assert_eq!(keys(&get_pixels_by_axes(&pixels, "XYZ")), ["d", "b", "a", "c"]);
    assert_eq!(keys(&get_pixels_by_axes(&pixels, "YXZ")), ["d", "a", "b", "c"]);
    assert_eq!(keys(&get_pixels_by_axes(&pixels, "ZYX")), ["d", "c", "a", "b"]);
}

#[test]
fn the_channels_are_repeated_for_the_pixels_in_their_order() {
    let profile = profile(
        r#"{"matrix": {"pixelKeys": [[["b", "a", "10", "9"]]], "pixelGroups": {"Left": ["b", "a"], "All": "all"}},
            "modes": [
                {"name": "Pixels", "shortName": "px", "channels": [
                    {"insert": "matrixChannels", "repeatFor": ["Left", "9"], "channelOrder": "perPixel", "templateChannels": ["Dimmer $pixelKey"]},
                    {"insert": "matrixChannels", "repeatFor": "eachPixelABC", "channelOrder": "perPixel", "templateChannels": ["Red $pixelKey"]},
                    null
                ]},
                {"name": "Groups only", "channels": [{"insert": "matrixChannels", "repeatFor": "eachPixelGroup", "channelOrder": "perPixel", "templateChannels": ["Red $pixelKey"]}]},
                {"name": "Plain", "shortName": "", "channels": ["Dimmer", {"repeatFor": ["missing"]}]},
                {"shortName": "none", "channels": []}
            ]}"#,
    );
    let repeat = |entries: &[&str]| {
        let entries: Vec<String> = entries.iter().map(|entry| entry.to_string()).collect();
        get_pixels_in_order(&profile, &entries)
    };
    assert_eq!(keys(&repeat(&["eachPixelABC"])), ["9", "10", "a", "b"]);
    assert_eq!(keys(&repeat(&["eachPixelXYZ"])), ["b", "a", "10", "9"]);
    assert_eq!(
        repeat(&["eachPixelGroup"]),
        [Pixel::new("Left", 0, 0, 0), Pixel::new("All", 0, 0, 0)]
    );
    assert_eq!(
        repeat(&["All", "9", "nothing", "b"]),
        [
            Pixel::new("All", 0, 0, 0),
            Pixel::new("9", 4, 1, 1),
            Pixel::new("b", 1, 1, 1)
        ]
    );
    // an order nobody knows, and an order among other entries
    assert!(repeat(&["eachPixelXXZ"]).is_empty());
    assert!(repeat(&["eachPixelxyz"]).is_empty());
    assert!(repeat(&["eachPixel"]).is_empty());
    assert!(repeat(&["eachPixelABC", "9"])
        .iter()
        .map(|pixel| pixel.key.as_str())
        .eq(["9"]));
    assert!(repeat(&[]).is_empty());
    assert!(get_pixels_in_order(&self::profile("{}"), &["eachPixelABC".to_string()]).is_empty());
    assert_eq!(get_pixel_group_names(&profile), ["Left", "All"]);
    assert!(get_pixel_group_names(&self::profile("{}")).is_empty());

    // the mode of a fixture: by its short name, or by its name if it has none
    let mode_name = |mode: Option<&str>| get_mode_by_fixture(&profile, &fixture(mode)).map(|mode| mode.name.clone());
    assert_eq!(mode_name(Some("px")), Some(Some("Pixels".to_string())));
    assert_eq!(mode_name(Some("Pixels")), None);
    assert_eq!(mode_name(Some("Groups only")), Some(Some("Groups only".to_string())));
    assert_eq!(mode_name(Some("Plain")), Some(Some("Plain".to_string())));
    assert_eq!(mode_name(Some("none")), Some(None));
    assert_eq!(mode_name(Some("")), None);
    assert_eq!(mode_name(None), None);
    assert!(get_mode_by_fixture(&self::profile("{}"), &fixture(Some("px"))).is_none());

    assert_eq!(
        fixture_get_unique_pixels(&profile, &fixture(Some("px")))
            .as_deref()
            .map(keys),
        Some(vec!["Left", "9", "10", "a", "b"])
    );
    assert_eq!(
        fixture_get_unique_pixels(&profile, &fixture(Some("Groups only")))
            .as_deref()
            .map(keys),
        Some(vec!["Left", "All"])
    );
    assert_eq!(
        fixture_get_unique_pixels(&profile, &fixture(Some("Plain"))),
        Some(Vec::new())
    );
    assert_eq!(fixture_get_unique_pixels(&profile, &fixture(Some("missing"))), None);
    // the channel nobody uses is one without pixels as well
    assert_eq!(fixture_has_general_channel(&profile, &fixture(Some("px"))), Some(true));
    assert_eq!(
        fixture_has_general_channel(&profile, &fixture(Some("Groups only"))),
        Some(false)
    );
    assert_eq!(
        fixture_has_general_channel(&profile, &fixture(Some("Plain"))),
        Some(true)
    );
    assert_eq!(
        fixture_has_general_channel(&profile, &fixture(Some("none"))),
        Some(false)
    );
    assert_eq!(fixture_has_general_channel(&profile, &fixture(Some("missing"))), None);
    let modes = profile.modes.as_ref().unwrap();
    assert_eq!(
        keys(&mode_get_unique_pixels(&profile, &modes[0])),
        ["Left", "9", "10", "a", "b"]
    );
    assert!(mode_has_general_channel(&profile, &modes[0]));

    assert!(is_unused_channel(&modes[0].channels[2]));
    assert!(!is_unused_channel(&modes[0].channels[0]));
    assert!(!is_unused_channel(&modes[2].channels[0]));
    assert!(!is_unused_channel(&modes[2].channels[1]));
    assert!(is_unused_channel(&FixtureModeChannel::default()));

    assert_eq!(get_channel_name_with_pixel_key("Red $pixelKey", Some("10")), "Red 10");
    assert_eq!(
        get_channel_name_with_pixel_key("$pixelKey/$pixelKey fine", Some("a")),
        "a/a fine"
    );
    assert_eq!(
        get_channel_name_with_pixel_key("Red $pixelKey", Some("")),
        "Red $pixelKey"
    );
    assert_eq!(get_channel_name_with_pixel_key("Red $pixelKey", None), "Red $pixelKey");
    assert_eq!(get_channel_name_with_pixel_key("Dimmer", Some("10")), "Dimmer");
}

// ---- the wheels and the colors ----

#[test]
fn the_slots_of_a_wheel_are_counted_around_it() {
    let profile = profile(
        r##"{"wheels": {"Color Wheel": {"slots": [
            {"type": "Open"}, {"type": "Color", "name": "Red", "colors": ["#ff0000"]}, {"type": "Color", "name": "Blue", "colors": ["#0000ff"]},
            {"type": "Color", "name": "Two", "colors": ["#00ff00", "white", "#000000"]}, {"type": "Color", "colors": []}
        ]}, "Empty": {}}}"##,
    );
    let wheel = profile.wheels.get("Color Wheel");
    let names = |slot_number: f64| {
        get_wheel_slots(wheel, slot_number)
            .iter()
            .map(|slot| slot.name.as_deref().unwrap_or("-"))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(1.0), ["-"]);
    assert_eq!(names(2.0), ["Red"]);
    assert_eq!(names(2.5), ["Red", "Blue"]);
    assert_eq!(names(5.5), ["-", "-"]);
    // around the wheel
    assert_eq!(names(6.0), ["-"]);
    assert_eq!(names(7.0), ["Red"]);
    assert_eq!(names(0.0), ["-"]);
    assert_eq!(names(-1.0), ["Two"]);
    assert_eq!(names(-0.5), ["Two", "-"]);
    assert_eq!(names(1e10), ["Red"]);
    assert_eq!(names(f64::NAN), ["-", "-"]);
    assert_eq!(
        get_wheel_slot(wheel.unwrap(), 3).and_then(|slot| slot.name.as_deref()),
        Some("Blue")
    );
    // the number before the lowest i32 is the highest one (it wraps around)
    assert_eq!(
        get_wheel_slot(wheel.unwrap(), i32::MIN).and_then(|slot| slot.name.as_deref()),
        Some("Blue")
    );
    assert!(get_wheel_slot(profile.wheels.get("Empty").unwrap(), 1).is_none());
    assert!(get_wheel_slots(profile.wheels.get("Empty"), 1.0).is_empty());
    assert!(get_wheel_slots(None, 1.0).is_empty());

    assert_eq!(get_wheel_slot_colors(wheel, 2.5), ["#ff0000", "#0000ff"]);
    assert_eq!(get_wheel_slot_colors(wheel, 4.0), ["#00ff00", "white", "#000000"]);
    assert!(get_wheel_slot_colors(wheel, 1.0).is_empty());
    assert!(get_wheel_slot_colors(None, 1.0).is_empty());
    assert_eq!(
        get_mixed_wheel_slot_color(wheel, 2.5),
        Some(Color {
            red: 127.5,
            green: 0.0,
            blue: 127.5
        })
    );
    assert_eq!(
        get_mixed_wheel_slot_color(wheel, 4.0),
        Some(Color {
            red: 0.0,
            green: 127.5,
            blue: 0.0
        })
    );
    assert_eq!(get_mixed_wheel_slot_color(wheel, 1.0), None);
    assert_eq!(get_mixed_wheel_slot_color(wheel, 5.0), None);

    assert_eq!(
        hex_to_rgb("#F80"),
        Some(Color {
            red: 255.0,
            green: 136.0,
            blue: 0.0
        })
    );
    assert_eq!(hex_to_rgb("##fff"), None);
    assert_eq!(hex_to_rgb("#"), None);
    assert_eq!(hex_to_rgb("\u{ff11}23"), None);
    assert_eq!(
        mix_colors(&[
            Color {
                red: 1.0,
                green: 2.0,
                blue: 3.0
            },
            Color {
                red: 2.0,
                green: 2.0,
                blue: 4.0
            }
        ]),
        Color {
            red: 1.5,
            green: 2.0,
            blue: 3.5
        }
    );
    assert!(mix_colors(&[]).red.is_nan());
}

#[test]
fn the_values_of_a_channel_follow_its_fine_channels() {
    let channel = |text: &str| {
        engine_compiler::model::FixtureChannel::from_json(&engine_compiler::model::Json::parse(text).unwrap()).unwrap()
    };
    assert_eq!(get_max_value_by_channel(&channel("{}")), 255.0);
    assert_eq!(
        get_max_value_by_channel(&channel(r#"{"fineChannelAliases": ["fine"]}"#)),
        65535.0
    );
    assert_eq!(
        get_max_value_by_channel(&channel(r#"{"fineChannelAliases": ["fine", "finer"]}"#)),
        16777215.0
    );
    // a DMX value written in 8 bits for a channel of 16 bits
    assert_eq!(
        get_default_value_by_channel(&channel(
            r#"{"defaultValue": 128, "fineChannelAliases": ["fine"], "dmxValueResolution": "8bit"}"#
        )),
        Some(32768.0)
    );
    assert_eq!(
        get_default_value_by_channel(&channel(r#"{"defaultValue": 128, "fineChannelAliases": ["fine"]}"#)),
        Some(128.0)
    );
    // a percentage is one of the whole channel, whatever the resolution of the values
    assert_eq!(
        get_default_value_by_channel(&channel(
            r#"{"defaultValue": "100%", "fineChannelAliases": ["fine"], "dmxValueResolution": "8bit"}"#
        )),
        Some(65535.0)
    );
    assert_eq!(
        get_default_value_by_channel(&channel(r#"{"defaultValue": " 7 "}"#)),
        Some(7.0)
    );
    assert_eq!(
        get_default_value_by_channel(&channel(r#"{"defaultValue": "7 %"}"#)),
        Some(255.0 / 100.0 * 7.0)
    );
    assert_eq!(
        get_default_value_by_channel(&channel(r#"{"defaultValue": true}"#)),
        None
    );
}

// The recorded cases.

// a comment of a capability, and whether it names an action exactly and among other things: reset,
// lamp on and lamp off of each
const MAINTENANCE: &[(&str, [bool; 6])] = &[
    ("reset", [true, false, false, true, false, false]),
    ("Reset", [true, false, false, true, false, false]),
    (" RESET ", [true, false, false, true, false, false]),
    ("fixture reset", [true, false, false, true, false, false]),
    ("total reset", [true, false, false, true, false, false]),
    ("full reset", [true, false, false, true, false, false]),
    ("reset all", [true, false, false, true, false, false]),
    ("full reset all", [true, false, false, true, false, false]),
    ("fixture  reset", [false, false, false, true, false, false]),
    ("resets", [false, false, false, false, false, false]),
    ("reset fixture", [false, false, false, true, false, false]),
    ("pan reset", [false, false, false, true, false, false]),
    ("pan/tilt reset", [false, false, false, true, false, false]),
    ("reset_all", [false, false, false, false, false, false]),
    ("reset-all", [false, false, false, true, false, false]),
    ("preset", [false, false, false, false, false, false]),
    ("reset1", [false, false, false, false, false, false]),
    ("reset.", [false, false, false, true, false, false]),
    ("reset\u{2028}", [true, false, false, true, false, false]),
    ("reset\u{85}", [true, false, false, true, false, false]),
    ("reset\u{301}", [false, false, false, false, false, false]),
    ("re\u{301}set", [false, false, false, false, false, false]),
    ("\u{e9}reset", [false, false, false, true, false, false]),
    ("reset\u{e9}", [false, false, false, true, false, false]),
    ("\u{416}reset", [false, false, false, true, false, false]),
    ("reset all\u{2029}", [true, false, false, true, false, false]),
    ("lamp on", [false, true, false, false, true, false]),
    ("Lamp On", [false, true, false, false, true, false]),
    ("lamp  on", [false, false, false, false, true, false]),
    ("lampon", [false, false, false, false, true, false]),
    ("lamp\u{9}on", [false, false, false, false, true, false]),
    ("lamp\u{a0}on", [false, false, false, false, false, false]),
    ("lamp on/off", [false, false, false, false, true, false]),
    ("lamp once", [false, false, false, false, false, false]),
    ("lamp one", [false, false, false, false, false, false]),
    ("clamp on", [false, false, false, false, false, false]),
    ("lamp on\u{301}", [false, false, false, false, false, false]),
    ("lamp off", [false, false, true, false, false, true]),
    ("lamp off (hold 5s)", [false, false, false, false, false, true]),
    ("lampoff", [false, false, false, false, false, true]),
    ("lamp of", [false, false, false, false, false, false]),
    ("the lamp on now", [false, false, false, false, true, false]),
    ("lamp\u{a}on", [false, false, false, false, true, false]),
    ("lamp on\u{2028}", [false, true, false, false, true, false]),
    ("lamp\u{b}on", [false, false, false, false, true, false]),
    ("lamp\u{c}on", [false, false, false, false, true, false]),
    ("lamp\u{2028}on", [false, false, false, false, false, false]),
    ("", [false, false, false, false, false, false]),
    ("all reset", [false, false, false, true, false, false]),
    ("full  reset", [false, false, false, true, false, false]),
    ("fullreset", [false, false, false, false, false, false]),
    ("reset all channels", [false, false, false, true, false, false]),
    ("do a reset now", [false, false, false, true, false, false]),
    ("reset\u{d}\u{a}", [true, false, false, true, false, false]),
    ("x\u{a}reset", [false, false, false, true, false, false]),
    ("reset\u{a}x", [false, false, false, true, false, false]),
    ("LAMP ON", [false, true, false, false, true, false]),
    ("\u{130} reset", [false, false, false, true, false, false]),
    ("re set", [false, false, false, false, false, false]),
    ("lamp-on", [false, false, false, false, false, false]),
    ("lamp_on", [false, false, false, false, false, false]),
    ("9reset", [false, false, false, false, false, false]),
    ("reset9", [false, false, false, false, false, false]),
    ("_reset", [false, false, false, false, false, false]),
    ("reset\u{300}\u{301}", [false, false, false, false, false, false]),
    ("a\u{301}reset", [false, false, false, false, false, false]),
    ("\u{301}reset", [false, false, false, true, false, false]),
    ("reset \u{301}", [false, false, false, true, false, false]),
    ("lamp off\u{85}", [false, false, true, false, false, true]),
    ("total reset all", [true, false, false, true, false, false]),
    ("fixture reset all ", [true, false, false, true, false, false]),
];

// the texts the regular expressions below are searched in
const REGEX_TEXTS: &[&str] = &[
    "1", "10", "b", "A", "B1", "Head 12", "Left 1", "aac", "abc", "a\u{a}c", "aaa", "", "x", "a x b", "ab", "a b", "]",
    "a-", "a.b", "(1, 2)", "(12, 1)", "a\u{a}", "abcd", "aab", "\u{e4}", "a", "A1", "aaaa", "1 2", "$", "\u{5c}", "^",
    "z", "-", "_", "q", "b\u{a}", "ba", "12", "xx",
];

// a regular expression, and for each of the texts whether it is found in it (None for an
// expression which can't be read)
const REGEXES: &[(&str, Option<&str>)] = &[
    ("^[0-9]$", Some("1000000000000000000000000000000000000000")),
    ("[a-z]", Some("0010011111101111011001110101000010011101")),
    ("^B", Some("0000100000000000000000000000000000000000")),
    ("^[0-9]+$", Some("1100000000000000000000000000000000000010")),
    ("0", Some("0100000000000000000000000000000000000000")),
    ("^Head \u{5c}d+$", Some("0000010000000000000000000000000000000000")),
    ("^(Left|Right)", Some("0000001000000000000000000000000000000000")),
    ("(?:a|b)+c", Some("0000000110000000000000100000000000000000")),
    ("a.c", Some("0000000110000000000000100000000000000000")),
    ("a{2,3}", Some("0000000100100000000000010001000000000000")),
    ("a{2}", Some("0000000100100000000000010001000000000000")),
    ("a{2,}", Some("0000000100100000000000010001000000000000")),
    ("^$", Some("0000000000010000000000000000000000000000")),
    ("\u{5c}bx\u{5c}b", Some("0000000000001100000000000000000000000000")),
    ("[^a-c]", Some("1101111001001101111111101010111111111011")),
    ("[\u{5c}d\u{5c}s]", Some("1100111001000101000111000010100000001010")),
    ("a*?b", Some("0010000010000111001000110000000000001100")),
    ("a+?", Some("0000010111100111011001110101000000000100")),
    ("x|", Some("1111111111111111111111111111111111111111")),
    ("(", None),
    ("[", None),
    ("a{", None),
    ("*a", None),
    ("\u{5c}w+", Some("1111111111101111011111110111100010111111")),
    ("\u{5c}W", Some("0000011001000101111111001000111101001000")),
    (
        "\u{5c}S+\u{5c}s\u{5c}S+",
        Some("0000011001000101000110000000100000000000"),
    ),
    ("[]a]", Some("0000010111100111111001110101000000000100")),
    ("[a-]", Some("0000010111100111011001110101000001000100")),
    ("[-a]", Some("0000010111100111011001110101000001000100")),
    ("a\u{5c}.b", Some("0000000000000000001000000000000000000000")),
    (
        "\u{5c}(1, \u{5c}d\u{5c})",
        Some("0000000000000000000100000000000000000000"),
    ),
    (
        "^\u{5c}(\u{5c}d+, 1\u{5c})$",
        Some("0000000000000000000010000000000000000000"),
    ),
    ("a$", Some("0000000000100000000001000101000000000100")),
    ("a\u{5c}z", Some("0000000000100000000000000101000000000100")),
    ("\u{5c}Aa", Some("0000000111100111011001110101000000000000")),
    ("(a|ab)(c|bcd)(d*)", Some("0000000110000000000000100000000000000000")),
    ("a??b", Some("0010000010000111001000110000000000001100")),
    ("(a*)*b", Some("0010000010000111001000110000000000001100")),
    ("a|b|", Some("1111111111111111111111111111111111111111")),
    ("[a-c&&[b-d]]", Some("0010000111000111001000110000000000001100")),
    ("(?i)a", Some("0001010111100111011001110111000000000100")),
    ("a(?=b)", Some("0000000010000010000000110000000000000000")),
    ("\u{5c}p{L}", Some("0011111111101111011001111111000010011101")),
    ("\u{5c}1", Some("0000000000000000000000000000000000000000")),
    ("a{,2}", None),
    ("a{1,2}{2}", Some("0000010111100111011001110101000000000100")),
    ("a**", None),
    ("a+*", None),
    ("a++", Some("0000010111100111011001110101000000000100")),
    ("a?+", Some("1111111111111111111111111111111111111111")),
    ("x{0}", Some("1111111111111111111111111111111111111111")),
    ("x{2,1}", None),
    ("\u{5c}", None),
    ("[z-a]", None),
    ("\u{5c}q", None),
    ("\u{5c}_", Some("0000000000000000000000000000000000100000")),
    ("\u{5c}-", Some("0000000000000000010000000000000001000000")),
    ("]", Some("0000000000000000100000000000000000000000")),
    ("}", Some("0000000000000000000000000000000000000000")),
    ("a{1", None),
    ("{1}", Some("1111111111111111111111111111111111111111")),
    ("a||b", Some("1111111111111111111111111111111111111111")),
    ("()", Some("1111111111111111111111111111111111111111")),
    ("(?<n>a)\u{5c}k<n>", Some("0000000100100000000000010001000000000000")),
    ("(?<n>a)b", Some("0000000010000010000000110000000000000000")),
    ("\u{5c}x41", Some("0001000000000000000000000010000000000000")),
    ("\u{5c}u0041", Some("0001000000000000000000000010000000000000")),
    ("\u{5c}t", Some("0000000000000000000000000000000000000000")),
    ("[\u{5c}]]", Some("0000000000000000100000000000000000000000")),
    ("[a\u{5c}-z]", Some("0000010111100111011001110101000011000100")),
    ("[\u{5c}^a]", Some("0000010111100111011001110101000100000100")),
    ("[a^]", Some("0000010111100111011001110101000100000100")),
    ("\u{5c}d{2}", Some("0100010000000000000010000000000000000010")),
    ("^.$", Some("1011000000001000100001001100011111111000")),
    ("..", Some("0100111110100111011110110011100000000111")),
    ("\u{5c}$", Some("0000000000000000000000000000010000000000")),
    ("\u{5c}\u{5c}", Some("0000000000000000000000000000001000000000")),
];

// a property of the fixture library, with the bits of its rank and of its rotation rank, and
// whether it is nothing
const RANKS: &[(&str, Option<u64>, Option<u64>, bool)] = &[
    ("10deg", Some(0x4024000000000000), Some(0x4024000000000000), false),
    (" 10deg ", Some(0x4024000000000000), Some(0x4024000000000000), false),
    ("wide", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("narrow", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("closed", Some(0xc1ddcd6500000000), Some(0xc1ddcd6500000000), false),
    ("open", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("near", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("far", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("off", Some(0xc1ddcd6500000000), Some(0xc1ddcd6500000000), true),
    ("low", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("high", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("warm", Some(0xc1ddcd6500000000), Some(0xc1ddcd6500000000), false),
    ("CTO", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("default", Some(0x40b9640000000000), Some(0x40b9640000000000), false),
    ("CTB", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("cold", Some(0x41ddcd6500000000), Some(0x41ddcd6500000000), false),
    ("stop", Some(0xc1ddcd6500000000), Some(0x0000000000000000), false),
    ("slow", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("fast", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("instant", Some(0xc1ddcd6500000000), Some(0xc1ddcd6500000000), false),
    ("short", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("long", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("weak", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("strong", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("out", Some(0xc1cdcd6500000000), Some(0xc1cdcd6500000000), false),
    ("in", Some(0x41cdcd6500000000), Some(0x41cdcd6500000000), false),
    ("Wide", None, None, false),
    ("120bpm", Some(0x4000000000000000), Some(0x4000000000000000), false),
    ("33rpm", Some(0x3fe199999999999a), Some(0x3fe199999999999a), false),
    ("500ms", Some(0x3fe0000000000000), Some(0x3fe0000000000000), false),
    ("2s", Some(0x4000000000000000), Some(0x4000000000000000), false),
    ("3200K", Some(0x40a9000000000000), Some(0x40a9000000000000), false),
    ("50%", Some(0x4049000000000000), Some(0x4049000000000000), false),
    ("1Hz", Some(0x3ff0000000000000), Some(0x3ff0000000000000), false),
    ("2.5m", Some(0x4004000000000000), Some(0x4004000000000000), false),
    ("3m^3/min", Some(0x4008000000000000), Some(0x4008000000000000), false),
    ("100lm", Some(0x4059000000000000), Some(0x4059000000000000), false),
    ("5", Some(0x4014000000000000), Some(0x4014000000000000), false),
    ("-5", Some(0xc014000000000000), Some(0xc014000000000000), false),
    ("+5", Some(0x4014000000000000), Some(0x4014000000000000), false),
    (".5", Some(0x3fe0000000000000), Some(0x3fe0000000000000), false),
    ("5.", Some(0x4014000000000000), Some(0x4014000000000000), false),
    ("1e3", Some(0x408f400000000000), Some(0x408f400000000000), false),
    ("1e3Hz", Some(0x408f400000000000), Some(0x408f400000000000), false),
    ("1E-2s", Some(0x3f847ae147ae147b), Some(0x3f847ae147ae147b), false),
    ("5 deg", None, None, false),
    ("deg", None, None, false),
    ("", None, None, false),
    (" ", None, None, false),
    ("5e", None, None, false),
    ("5em", None, None, false),
    ("1.5.5", None, None, false),
    ("0", Some(0x0000000000000000), Some(0x0000000000000000), true),
    ("0%", Some(0x0000000000000000), Some(0x0000000000000000), true),
    ("-0", Some(0x8000000000000000), Some(0x8000000000000000), true),
    ("0.0deg", Some(0x0000000000000000), Some(0x0000000000000000), true),
    ("off ", Some(0xc1ddcd6500000000), Some(0xc1ddcd6500000000), true),
    ("OFF", None, None, false),
    ("12x", None, None, false),
    ("5mm", None, None, false),
    ("5 ", Some(0x4014000000000000), Some(0x4014000000000000), false),
    ("ms", None, None, false),
    ("1e", None, None, false),
    ("1e+", None, None, false),
    ("1e+2ms", Some(0x3fb999999999999a), Some(0x3fb999999999999a), false),
    ("--5", None, None, false),
    ("5K ", Some(0x4014000000000000), Some(0x4014000000000000), false),
    ("fast CW", None, Some(0x41cdcd6500000000), false),
    ("slow CW", None, Some(0x3e112e0be826d695), false),
    ("slow CCW", None, Some(0xbe112e0be826d695), false),
    ("fast CCW", None, Some(0xc1cdcd6500000000), false),
    (" stop ", Some(0xc1ddcd6500000000), Some(0x0000000000000000), false),
    ("fast cw", None, None, false),
    ("-10rpm", Some(0xbfc5555555555555), Some(0xbfc5555555555555), false),
    ("0rpm", Some(0x0000000000000000), Some(0x0000000000000000), true),
    ("5m^3/min", Some(0x4014000000000000), Some(0x4014000000000000), false),
    ("5m^3", None, None, false),
    ("\u{665}", None, None, false),
    ("1_0", None, None, false),
    ("0x10", None, None, false),
    ("1d", None, None, false),
    ("NaN", None, None, false),
    ("Infinity", None, None, false),
    ("5.e2", Some(0x407f400000000000), Some(0x407f400000000000), false),
    ("5.e", None, None, false),
    (".e2", None, None, false),
    ("+.5e-1%", Some(0x3fa999999999999a), Some(0x3fa999999999999a), false),
    ("5%%", None, None, false),
    ("5Hzs", None, None, false),
    ("5bpm ", Some(0x3fb5555555555555), Some(0x3fb5555555555555), false),
    (
        "\u{9}7deg\u{a}",
        Some(0x401c000000000000),
        Some(0x401c000000000000),
        false,
    ),
    ("7rpm", Some(0x3fbdddddddddddde), Some(0x3fbdddddddddddde), false),
    ("0.1ms", Some(0x3f1a36e2eb1c432d), Some(0x3f1a36e2eb1c432d), false),
    ("1e400", Some(0x7ff0000000000000), Some(0x7ff0000000000000), false),
    ("1e-400s", Some(0x0000000000000000), Some(0x0000000000000000), true),
    ("-0ms", Some(0x8000000000000000), Some(0x8000000000000000), true),
    ("\u{a0}5deg", None, None, false),
    ("5deg\u{2028}", None, None, false),
];

// the same texts with the bits of the number they are read as
const NUMBERS: &[(&str, Option<u64>)] = &[
    ("10deg", None),
    (" 10deg ", None),
    ("wide", None),
    ("narrow", None),
    ("closed", None),
    ("open", None),
    ("near", None),
    ("far", None),
    ("off", None),
    ("low", None),
    ("high", None),
    ("warm", None),
    ("CTO", None),
    ("default", None),
    ("CTB", None),
    ("cold", None),
    ("stop", None),
    ("slow", None),
    ("fast", None),
    ("instant", None),
    ("short", None),
    ("long", None),
    ("weak", None),
    ("strong", None),
    ("out", None),
    ("in", None),
    ("Wide", None),
    ("120bpm", None),
    ("33rpm", None),
    ("500ms", None),
    ("2s", None),
    ("3200K", None),
    ("50%", None),
    ("1Hz", None),
    ("2.5m", None),
    ("3m^3/min", None),
    ("100lm", None),
    ("5", Some(0x4014000000000000)),
    ("-5", Some(0xc014000000000000)),
    ("+5", Some(0x4014000000000000)),
    (".5", Some(0x3fe0000000000000)),
    ("5.", Some(0x4014000000000000)),
    ("1e3", Some(0x408f400000000000)),
    ("1e3Hz", None),
    ("1E-2s", None),
    ("5 deg", None),
    ("deg", None),
    ("", None),
    (" ", None),
    ("5e", None),
    ("5em", None),
    ("1.5.5", None),
    ("0", Some(0x0000000000000000)),
    ("0%", None),
    ("-0", Some(0x8000000000000000)),
    ("0.0deg", None),
    ("off ", None),
    ("OFF", None),
    ("12x", None),
    ("5mm", None),
    ("5 ", Some(0x4014000000000000)),
    ("ms", None),
    ("1e", None),
    ("1e+", None),
    ("1e+2ms", None),
    ("--5", None),
    ("5K ", None),
    ("fast CW", None),
    ("slow CW", None),
    ("slow CCW", None),
    ("fast CCW", None),
    (" stop ", None),
    ("fast cw", None),
    ("-10rpm", None),
    ("0rpm", None),
    ("5m^3/min", None),
    ("5m^3", None),
    ("\u{665}", None),
    ("1_0", None),
    ("0x10", None),
    ("1d", None),
    ("NaN", None),
    ("Infinity", None),
    ("5.e2", Some(0x407f400000000000)),
    ("5.e", None),
    (".e2", None),
    ("+.5e-1%", None),
    ("5%%", None),
    ("5Hzs", None),
    ("5bpm ", None),
    ("\u{9}7deg\u{a}", None),
    ("7rpm", None),
    ("0.1ms", None),
    ("1e400", Some(0x7ff0000000000000)),
    ("1e-400s", None),
    ("-0ms", None),
    ("\u{a0}5deg", None),
    ("5deg\u{2028}", None),
];

// the positions the rules below are asked for
const RULE_VALUES: [i32; 13] = [-3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 10, 2147483647, -2147483648];

// a rule of a pixel group, and for each of the positions whether it meets it (T), doesn't (F) or
// the rule can't be read (E)
const RULES: &[(&str, &str)] = &[
    ("<=2", "TTTTTTFFFFFFT"),
    (">=2", "FFFFFTTTTTTTF"),
    ("=3", "FFFFFFTFFFFFF"),
    ("= 3", "FFFFFFTFFFFFF"),
    ("<= -1", "TTTFFFFFFFFFT"),
    ("<=-1", "TTTFFFFFFFFFT"),
    (">=  0", "FFFTTTTTTTTTF"),
    ("even", "FTFTFTFTFTTFT"),
    ("odd", "TFTFTFTFTFFTF"),
    ("EVEN", "FTFTFTFTFTTFT"),
    (" Odd ", "TFTFTFTFTFFTF"),
    ("3n", "TFFTFFTFFTFFF"),
    ("3n+1", "FTFFTFFTFFTTT"),
    ("2N+1", "TFTFTFTFTFFTF"),
    ("0n", "EEEEEEEEEEEEE"),
    ("1n", "TTTTTTTTTTTTT"),
    ("3n+4", "FTFFTFFTFFTTT"),
    ("10n+10", "FFFTFFFFFFTFF"),
    ("2", "FFFFFTFFFFFFF"),
    ("-2", "FTFFFFFFFFFFF"),
    ("+2", "EEEEEEEEEEEEE"),
    ("02", "FFFFFTFFFFFFF"),
    ("-0", "FFFTFFFFFFFFF"),
    ("", "EEEEEEEEEEEEE"),
    (" ", "EEEEEEEEEEEEE"),
    ("most", "EEEEEEEEEEEEE"),
    ("<2", "EEEEEEEEEEEEE"),
    (">2", "EEEEEEEEEEEEE"),
    ("==2", "EEEEEEEEEEEEE"),
    ("=<2", "EEEEEEEEEEEEE"),
    ("a<=2", "EEEEEEEEEEEEE"),
    ("<=2x", "EEEEEEEEEEEEE"),
    ("3n+", "EEEEEEEEEEEEE"),
    ("n", "EEEEEEEEEEEEE"),
    ("n+1", "EEEEEEEEEEEEE"),
    ("3n-1", "EEEEEEEEEEEEE"),
    ("3 n", "EEEEEEEEEEEEE"),
    ("3n +1", "EEEEEEEEEEEEE"),
    ("2147483647", "FFFFFFFFFFFTF"),
    ("2147483648", "EEEEEEEEEEEEE"),
    ("=2147483648", "EEEEEEEEEEEEE"),
    ("<=-2147483648", "FFFFFFFFFFFFT"),
    ("<=-2147483649", "EEEEEEEEEEEEE"),
    ("2147483647n", "FFFTFTFFFFFTF"),
    ("2147483648n", "EEEEEEEEEEEEE"),
    ("3n+2147483648", "EEEEEEEEEEEEE"),
    ("2147483647n+2147483647", "FFFTFTFFFFFTF"),
    ("<=\u{9}2", "TTTTTTFFFFFFT"),
    ("<=\u{b}2", "TTTTTTFFFFFFT"),
    ("<=\u{a0}2", "EEEEEEEEEEEEE"),
    ("=\u{663}", "EEEEEEEEEEEEE"),
    ("=2\u{a}", "FFFFFTFFFFFFF"),
    ("\u{2028}=2", "EEEEEEEEEEEEE"),
    ("<= 2 ", "TTTTTTFFFFFFT"),
    ("=-", "EEEEEEEEEEEEE"),
    ("<=", "EEEEEEEEEEEEE"),
    ("3n+1+1", "EEEEEEEEEEEEE"),
    ("-3n", "EEEEEEEEEEEEE"),
    ("3nn", "EEEEEEEEEEEEE"),
    ("=2\u{2028}", "EEEEEEEEEEEEE"),
    ("2\u{85}", "EEEEEEEEEEEEE"),
    ("even\u{2029}", "EEEEEEEEEEEEE"),
    ("0n+0", "EEEEEEEEEEEEE"),
    ("00n", "EEEEEEEEEEEEE"),
    ("007", "FFFFFFFFFFFFF"),
    ("=007", "FFFFFFFFFFFFF"),
    ("=-007", "FFFFFFFFFFFFF"),
];

// pixel keys, and how each of them compares to each of them
const KEYS: &[&str] = &[
    "",
    "1",
    "01",
    "001",
    "2",
    "10",
    "a",
    "A",
    "b",
    "B",
    "a1",
    "a01",
    "a2",
    "a10",
    "A10",
    "B2",
    "1a",
    "01b",
    " ",
    "_",
    "\u{e9}",
    "\u{c9}",
    "\u{130}",
    "i",
    "\u{df}",
    "9999999999999999999999",
    "10000000000000000000000",
    "a b",
    "a-b",
    "(1, 2)",
    "(1, 10)",
    "(2, 1)",
    "\u{1f600}",
    "\u{1f601}",
    "\u{663}",
    "Z",
    "[",
    "z",
    "{",
    "0",
    "00",
    "a0",
    "a00b",
    "a0b",
];
const KEY_ORDER: &[&str] = &[
    "=<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<",
    ">===<<<<<<<<<<<<<<><<<<<<<<<<>>><<<<<<<>><<<",
    ">===<<<<<<<<<<<<<<><<<<<<<<<<>>><<<<<<<>><<<",
    ">===<<<<<<<<<<<<<<><<<<<<<<<<>>><<<<<<<>><<<",
    ">>>>=<<<<<<<<<<<>>><<<<<<<<<<>>><<<<<<<>><<<",
    ">>>>>=<<<<<<<<<<>>><<<<<<<<<<>>><<<<<<<>><<<",
    ">>>>>>==<<<<<<<<>>>><<<<<>><<>>><<<<><<>><<<",
    ">>>>>>==<<<<<<<<>>>><<<<<>><<>>><<<<><<>><<<",
    ">>>>>>>>==>>>>><>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>>>>>>==>>>>><>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>>>>>><<==<<<<>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>>>>>><<==<<<<>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>>>>>><<>>=<<<>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>>>>>><<>>>==<>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>>>>>><<>>>==<>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>>>>>>>>>>>>>=>>>><<<<<>>>>>>><<<<><<>>>>>",
    ">>>><<<<<<<<<<<<=<><<<<<<<<<<>>><<<<<<<>><<<",
    ">>>><<<<<<<<<<<<>=><<<<<<<<<<>>><<<<<<<>><<<",
    "><<<<<<<<<<<<<<<<<=<<<<<<<<<<<<<<<<<<<<<<<<<",
    ">>>>>><<<<<<<<<<>>>=<<<<<>><<>>><<<<><<>><<<",
    ">>>>>>>>>>>>>>>>>>>>==>>>>>>>>>><<<>>>>>>>>>",
    ">>>>>>>>>>>>>>>>>>>>==>>>>>>>>>><<<>>>>>>>>>",
    ">>>>>>>>>>>>>>>>>>>><<==<>>>>>>><<<<><<>>>>>",
    ">>>>>>>>>>>>>>>>>>>><<==<>>>>>>><<<<><<>>>>>",
    ">>>>>>>>>>>>>>>>>>>><<>>=>>>>>>><<<>>>>>>>>>",
    ">>>>>><<<<<<<<<<>>><<<<<<=<<<>>><<<<<<<>><<<",
    ">>>>>><<<<<<<<<<>>><<<<<<>=<<>>><<<<<<<>><<<",
    ">>>>>>>><<<<<<<<>>>><<<<<>>=<>>><<<<><<>><<<",
    ">>>>>>>><<<<<<<<>>>><<<<<>>>=>>><<<<><<>><<<",
    "><<<<<<<<<<<<<<<<<><<<<<<<<<<=<<<<<<<<<<<<<<",
    "><<<<<<<<<<<<<<<<<><<<<<<<<<<>=<<<<<<<<<<<<<",
    "><<<<<<<<<<<<<<<<<><<<<<<<<<<>>=<<<<<<<<<<<<",
    ">>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>=<>>>>>>>>>>",
    ">>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>=>>>>>>>>>>",
    ">>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>><<=>>>>>>>>>",
    ">>>>>>>>>>>>>>>>>>>><<>><>>>>>>><<<=>=<>>>>>",
    ">>>>>><<<<<<<<<<>>><<<<<<>><<>>><<<<=<<>><<<",
    ">>>>>>>>>>>>>>>>>>>><<>><>>>>>>><<<=>=<>>>>>",
    ">>>>>>>>>>>>>>>>>>>><<>><>>>>>>><<<>>>=>>>>>",
    "><<<<<<<<<<<<<<<<<><<<<<<<<<<>>><<<<<<<==<<<",
    "><<<<<<<<<<<<<<<<<><<<<<<<<<<>>><<<<<<<==<<<",
    ">>>>>>>><<<<<<<<>>>><<<<<>>>>>>><<<<><<>>=<<",
    ">>>>>>>><<<<<<<<>>>><<<<<>>>>>>><<<<><<>>>==",
    ">>>>>>>><<<<<<<<>>>><<<<<>>>>>>><<<<><<>>>==",
];

// a brightness, with the bits of how bright it is without lumens to compare to and with 2000 of
// them
const BRIGHTNESS: &[(&str, Option<u64>, Option<u64>)] = &[
    ("off", Some(0x0000000000000000), Some(0x0000000000000000)),
    ("dark", Some(0x3f847ae147ae147b), Some(0x3f847ae147ae147b)),
    ("bright", Some(0x3ff0000000000000), Some(0x3ff0000000000000)),
    ("50%", Some(0x3fe0000000000000), Some(0x3fe0000000000000)),
    ("100%", Some(0x3ff0000000000000), Some(0x3ff0000000000000)),
    ("0%", Some(0x0000000000000000), Some(0x0000000000000000)),
    ("33.3%", Some(0x3fd54fdf3b645a1c), Some(0x3fd54fdf3b645a1c)),
    ("1000lm", None, Some(0x3fe0000000000000)),
    ("2000lm", None, Some(0x3ff0000000000000)),
    ("0lm", None, Some(0x0000000000000000)),
    ("abc", None, None),
    ("", None, None),
    ("%", None, None),
    ("lm", None, None),
    ("x%", None, None),
    ("xlm", None, None),
    (" 50%", Some(0x3fe0000000000000), Some(0x3fe0000000000000)),
    ("50 %", Some(0x3fe0000000000000), Some(0x3fe0000000000000)),
    ("Off", None, None),
    ("5", None, None),
    ("1e2%", Some(0x3ff0000000000000), Some(0x3ff0000000000000)),
    ("50%%", None, None),
    ("3000lm", None, Some(0x3ff8000000000000)),
    ("-5%", Some(0xbfa999999999999a), Some(0xbfa999999999999a)),
    (" 500 lm", None, Some(0x3fd0000000000000)),
    ("500lm ", None, None),
];
