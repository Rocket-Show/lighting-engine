//! The stage geometry against worked examples and recorded values.

use std::collections::BTreeSet;
use std::f64::consts::PI;

use engine_compiler::stage::*;

fn hanging() -> StagePlacement {
    StagePlacement {
        position: [0.0, 405.4, 0.0],
        rotation: [0.0, 0.0, 0.0],
    }
}

fn fixture<'a>(uuid: &'a str, positioning: &'a str) -> StageFixture<'a> {
    StageFixture {
        uuid,
        positioning,
        position: [0.0; 3],
        rotation: [0.0; 3],
    }
}

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} is not within {tolerance} of {expected}"
    );
}

fn assert_bits(actual: f64, expected: u64) {
    assert_eq!(
        actual.to_bits(),
        expected,
        "{actual} is not {}",
        f64::from_bits(expected)
    );
}

// to three decimals, rounded half up
fn round(values: &[f64]) -> Vec<f64> {
    values
        .iter()
        .map(|value| (value * 1000.0 + 0.5).floor() / 1000.0)
        .collect()
}

// ---- the sine over a cycle ----

#[test]
fn sine_stays_within_a_digit_of_the_sine_of_the_platform() {
    // Over a few cycles. The platform is given the angle within one cycle (taking whole cycles off
    // is exact), as 2π · position is rounded to more than a digit of the sine a few cycles out.
    // Where the sine crosses 0 the angle is still rounded, so the difference is measured against 1
    // there.
    for i in -40_000..=40_000 {
        let position = f64::from(i) / 10_000.0 + 1.0 / 3_000_000.0;
        let expected = (2.0 * PI * (position - position.round())).sin();
        let actual = sin_cycle(position);

        assert!(
            (actual - expected).abs() <= 1e-15 * expected.abs().max(1.0),
            "sin_cycle({position}) = {actual}, not {expected}"
        );
    }

    // in the first quarter the angle is the same on both sides: within 1e-15 of the value itself
    for i in 1..25_000 {
        let position = f64::from(i) / 100_000.0;
        let expected = (2.0 * PI * position).sin();
        let actual = sin_cycle(position);

        assert!(
            (actual - expected).abs() <= 1e-15 * expected.abs(),
            "sin_cycle({position}) = {actual}, not {expected}"
        );
    }
}

#[test]
fn sine_is_exact_on_the_quarters() {
    assert_bits(sin_cycle(0.0), 0.0f64.to_bits());
    assert_bits(sin_cycle(0.25), 1.0f64.to_bits());
    // the second half of the cycle is the first one negated: -0.0
    assert_bits(sin_cycle(0.5), (-0.0f64).to_bits());
    assert_bits(sin_cycle(0.75), (-1.0f64).to_bits());
    assert_bits(sin_cycle(1.0), 0.0f64.to_bits());
    assert_bits(sin_cycle(-0.25), (-1.0f64).to_bits());
    assert_bits(sin_cycle(-0.5), (-0.0f64).to_bits());
    assert_bits(sin_cycle(-0.75), 1.0f64.to_bits());
    assert_bits(sin_cycle(1234.25), 1.0f64.to_bits());
    assert!(sin_cycle(f64::NAN).is_nan());
    assert!(sin_cycle(f64::INFINITY).is_nan());
}

#[test]
fn sine_comes_out_as_recorded() {
    // the recorded sines of these, to the last digit
    assert_bits(sin_cycle(0.1), 0x3fe2cf2304755a5e);
    assert_bits(sin_cycle(0.125), 0x3fe6a09e667f3bcd);
    assert_bits(sin_cycle(1.0 / 3.0), 0x3febb67ae8584cab);
    assert_bits(sin_cycle(0.7), 0xbfee6f0e134454ff);
    assert_bits(sin_cycle(-0.2), 0xbfee6f0e13445500);
    assert_bits(sin_cycle(12345.678), 0xbfecc7ed85aa7580);
    assert_bits(sin_cycle(1e-10), 0x3e0596bf8ce7631e);
}

// ---- the arc tangent ----

#[test]
fn arc_tangent_comes_out_like_the_one_of_fdlibm() {
    assert_eq!(atan2(0.9267919362380059, 0.37557516814108993), 1.1857790291288564);
    assert_eq!(atan(1.0), PI / 4.0);
    assert_eq!(atan(-1.0), -PI / 4.0);

    // the recorded arc tangents of these, to the last digit
    assert_bits(atan(0.3), 0x3fd2a73a661eaf06);
    assert_bits(atan(0.5), 0x3fddac670561bb4f);
    assert_bits(atan(0.9), 0x3fe77338a80603be);
    assert_bits(atan(1.5), 0x3fef730bd281f69b);
    assert_bits(atan(3.0), 0x3ff3fc176b7a8560);
    assert_bits(atan(1e30), 0x3ff921fb54442d18);
    assert_bits(atan(1e-12), 0x3d719799812dea11);
    assert_bits(atan(-0.5), 0xbfddac670561bb4f);
    assert_bits(atan2(1.0, 2.0), 0x3fddac670561bb4f);
    assert_bits(atan2(-1.0, -1.0), 0xc002d97c7f3321d2);
    assert_bits(atan2(1.0, -1.0), 0x4002d97c7f3321d2);
    assert_bits(atan2(0.5, 1.0), 0x3fddac670561bb4f);
    assert_bits(atan2(3.0, -4.0), 0x4003fc176b7a8560);
    assert_bits(atan2(-3.0, 4.0), 0xbfe4978fa3269ee1);
}

#[test]
fn arc_tangent_stays_within_a_digit_of_the_one_of_the_platform() {
    // over many magnitudes
    for i in 0..500 {
        let y = (f64::from(i) * 1.7).sin() * 10f64.powi((i % 7) - 3);
        let x = (f64::from(i) * 2.3).cos() * 10f64.powi((i % 5) - 2);
        let expected = y.atan2(x);

        assert!(
            (atan2(y, x) - expected).abs() <= 1e-15 * expected.abs(),
            "atan2({y}, {x})"
        );
    }

    // all around the circle, close to the axes and far from them
    for i in 0..36_000 {
        let angle = f64::from(i) / 36_000.0 * 2.0 * PI + 1e-7;

        for radius in [1e-6, 0.37, 1.0, 12.5, 4e8] {
            let y = angle.sin() * radius;
            let x = angle.cos() * radius;
            let expected = y.atan2(x);

            assert!(
                (atan2(y, x) - expected).abs() <= 1e-15 * expected.abs(),
                "atan2({y}, {x})"
            );
        }
    }

    for i in -20_000..=20_000 {
        let value = f64::from(i) / 1_000.0;
        let expected = value.atan();

        assert!(
            (atan(value) - expected).abs() <= 1e-15 * expected.abs(),
            "atan({value})"
        );
    }
}

#[test]
fn arc_tangent_takes_every_range_of_numbers_and_the_axes() {
    for x in [1e-12, 0.1, 0.5, 0.9, 1.0, 1.5, 3.0, 1e30] {
        assert_close(atan(x), x.atan(), 1e-15);
        assert_close(atan(-x), -x.atan(), 1e-15);
        assert_eq!(atan(-x), -atan(x));
    }

    assert_bits(atan2(0.0, 1.0), 0.0f64.to_bits());
    assert_eq!(atan2(0.0, -1.0), PI);
    assert_eq!(atan2(-0.0, -1.0), -PI);
    assert_eq!(atan2(1.0, 0.0), PI / 2.0);
    assert_eq!(atan2(-1.0, 0.0), -PI / 2.0);
    assert_close(atan2(-1.0, -1.0), -3.0 * PI / 4.0, 1e-15);
    assert_close(atan2(1.0, -1.0), 3.0 * PI / 4.0, 1e-15);
    assert_close(atan2(0.5, 1.0), 0.5f64.atan2(1.0), 1e-15);
}

#[test]
fn arc_tangent_takes_the_special_cases() {
    // x == 1 is the arc tangent of y alone
    for y in [-7.5, -1.0, -0.3, -0.0, 0.0, 1e-12, 0.5, 1.0, 2.0, 1e30] {
        assert_bits(atan2(y, 1.0), atan(y).to_bits());
    }

    // y == 0: y itself to the right (with its sign), π or -π to the left by the sign of the zero
    assert_bits(atan2(0.0, 5.0), 0.0f64.to_bits());
    assert_bits(atan2(-0.0, 5.0), (-0.0f64).to_bits());
    assert_bits(atan2(0.0, 0.0), 0.0f64.to_bits());
    assert_bits(atan2(-0.0, 0.0), (-0.0f64).to_bits());
    assert_bits(atan2(0.0, -0.0), 0.0f64.to_bits());
    assert_bits(atan2(0.0, -3.0), PI.to_bits());
    assert_bits(atan2(-0.0, -3.0), (-PI).to_bits());

    // x == 0: a quarter up or down
    assert_bits(atan2(2.0, 0.0), (PI / 2.0).to_bits());
    assert_bits(atan2(2.0, -0.0), (PI / 2.0).to_bits());
    assert_bits(atan2(-2.0, 0.0), (-PI / 2.0).to_bits());

    // tiny numbers are their own arc tangent, huge ones a quarter
    assert_bits(atan(1e-9), 1e-9f64.to_bits());
    assert_bits(atan(-1e-9), (-1e-9f64).to_bits());
    assert_bits(atan(0.0), 0.0f64.to_bits());
    assert_bits(atan(-0.0), (-0.0f64).to_bits());
    assert_bits(atan(1e20), (PI / 2.0).to_bits());
    assert_bits(atan(-1e20), (-PI / 2.0).to_bits());
    assert_bits(atan(f64::INFINITY), (PI / 2.0).to_bits());
    assert!(atan(f64::NAN).is_nan());
}

#[test]
fn arc_tangent_takes_the_edges_of_its_ranges() {
    for edge in [0.4375f64, 0.6875, 1.1875, 2.4375] {
        for x in [edge, f64::from_bits(edge.to_bits() - 1)] {
            assert_close(atan(x), x.atan(), 1e-15);
            assert_close(atan(-x), (-x).atan(), 1e-15);
        }
    }
}

// ---- where the fixtures hang ----

#[test]
fn turns_by_the_quarters_exactly() {
    assert_eq!(
        get_rotation_matrix([0.0, 0.0, 0.0]),
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    );

    let upside_down = get_rotation_matrix([180.0, 0.0, 0.0]);
    assert_eq!(upside_down[0][0], 1.0);
    assert_eq!(upside_down[1][1], -1.0);
    assert_eq!(upside_down[2][2], -1.0);
    assert_eq!(upside_down[1][2].abs(), 0.0);
    assert_eq!(get_rotation_matrix([0.0, -90.0, 0.0])[0][2], -1.0);
    assert_close(get_rotation_matrix([0.0, 0.0, 30.0])[1][0], 0.5, 1e-15);

    for (degrees, sine, cosine) in [
        (0.0, 0.0, 1.0),
        (90.0, 1.0, 0.0),
        (180.0, 0.0, -1.0),
        (270.0, -1.0, 0.0),
        (360.0, 0.0, 1.0),
        (-90.0, -1.0, 0.0),
        (-180.0, 0.0, -1.0),
        (-270.0, 1.0, 0.0),
        (-0.0, 0.0, 1.0),
        (810.0, 1.0, 0.0),
    ] {
        assert_bits(sin_degrees(degrees), f64::to_bits(sine));
        assert_bits(cos_degrees(degrees), f64::to_bits(cosine));
    }

    // in between like the sine over a cycle
    assert_bits(sin_degrees(36.0), sin_cycle(36.0 / 360.0).to_bits());
    assert_bits(cos_degrees(36.0), sin_cycle(36.0 / 360.0 + 0.25).to_bits());
    assert_close(sin_degrees(30.0), 0.5, 1e-15);
    assert_close(cos_degrees(60.0), 0.5, 1e-15);
    assert!(sin_degrees(f64::NAN).is_nan());
    assert!(cos_degrees(f64::INFINITY).is_nan());
}

#[test]
fn rotation_matrix_comes_out_as_recorded() {
    // the recorded matrix of a turn by 10, 20 and 30 degrees
    let matrix = get_rotation_matrix([10.0, 20.0, 30.0]);
    let expected: [[u64; 3]; 3] = [
        [0x3fea0aa16f5e991a, 0xbfde11f642522d1a, 0x3fd5e3a8748a0bf5],
        [0x3fe1671f3f88ed60, 0x3fea576ec9868ad0, 0xbfc4e2f2c0fa463a],
        [0xbfca3950c149a308, 0x3fd4672667b87e92, 0x3fed9d033a6cb460],
    ];

    for row in 0..3 {
        for column in 0..3 {
            assert_bits(matrix[row][column], expected[row][column]);
        }
    }
}

#[test]
fn places_the_fixtures_in_their_rows_and_by_hand() {
    let fixtures = [
        fixture("a", "topFront"),
        fixture("c", "bottomBack"),
        StageFixture {
            uuid: "d",
            positioning: "manual",
            position: [10.0, 0.0, 0.0],
            rotation: [0.0, 90.0, 0.0],
        },
    ];
    let placements = stage_placements(&fixtures, &["a", "c"], &StageSize::default());

    assert_eq!(placements["a"].position, [0.0, 367.0, 230.0]);
    assert_eq!(placements["c"].rotation[0], 180.0);
    assert_eq!(placements["c"].position[1], 43.0);
    assert_eq!(placements["d"].position[0], 10.0);
    assert_eq!(placements["d"].rotation[1], 90.0);
}

#[test]
fn spreads_the_fixtures_of_a_row_over_the_width_of_the_stage() {
    let fixtures = [
        fixture("a", "topFront"),
        fixture("b", "topFront"),
        fixture("c", "bottomBack"),
        StageFixture {
            uuid: "d",
            positioning: "manual",
            position: [10.0, 20.0, 30.0],
            rotation: [0.0, 90.0, 0.0],
        },
    ];
    // the pixels of a fixture count once
    let placements = stage_placements(&fixtures, &["a", "a", "b", "c", "gone"], &StageSize::default());

    assert_eq!(
        placements["a"],
        StagePlacement {
            position: [-100.0, 367.0, 230.0],
            rotation: [0.0, 0.0, 0.0]
        }
    );
    assert_eq!(
        placements["b"],
        StagePlacement {
            position: [100.0, 367.0, 230.0],
            rotation: [0.0, 0.0, 0.0]
        }
    );
    assert_eq!(
        placements["c"],
        StagePlacement {
            position: [0.0, 43.0, -230.0],
            rotation: [180.0, 0.0, 0.0]
        }
    );
    assert_eq!(
        placements["d"],
        StagePlacement {
            position: [10.0, 20.0, 30.0],
            rotation: [0.0, 90.0, 0.0]
        }
    );
    assert!(!placements.contains_key("gone"));
    assert_eq!(placements.len(), 4);
}

#[test]
fn places_only_the_fixtures_of_a_row_or_placed_by_hand() {
    let stage = StageSize::default();
    assert_eq!(
        stage,
        StageSize {
            width_cm: 600.0,
            height_cm: 350.0,
            depth_cm: 600.0,
            floor_height_cm: 30.0
        }
    );

    // a fixture in no preset is not placed, unless it was placed by hand; neither is one of an unknown row
    let fixtures = [
        fixture("a", "topFront"),
        fixture("m", "manual"),
        fixture("x", "somewhere"),
        fixture("y", ""),
    ];
    let placements = stage_placements(&fixtures, &["x", "y"], &stage);
    assert_eq!(placements.len(), 1);
    assert_eq!(placements["m"], StagePlacement::ORIGIN);

    // nothing at all
    assert!(stage_placements(&[], &[], &stage).is_empty());
    assert!(stage_placements(&[], &["a"], &stage).is_empty());

    // the four rows: on the floor the fixtures stand upside down, 13 cm above it
    let fixtures = [
        fixture("tf", "topFront"),
        fixture("bf", "bottomFront"),
        fixture("tb", "topBack"),
        fixture("bb", "bottomBack"),
    ];
    let stage = StageSize {
        width_cm: 800.0,
        height_cm: 400.0,
        depth_cm: 500.0,
        floor_height_cm: 50.0,
    };
    let placements = stage_placements(&fixtures, &["bb", "tb", "bf", "tf"], &stage);
    assert_eq!(
        placements["tf"],
        StagePlacement {
            position: [0.0, 437.0, 180.0],
            rotation: [0.0, 0.0, 0.0]
        }
    );
    assert_eq!(
        placements["bf"],
        StagePlacement {
            position: [0.0, 63.0, 180.0],
            rotation: [180.0, 0.0, 0.0]
        }
    );
    assert_eq!(
        placements["tb"],
        StagePlacement {
            position: [0.0, 437.0, -180.0],
            rotation: [0.0, 0.0, 0.0]
        }
    );
    assert_eq!(
        placements["bb"],
        StagePlacement {
            position: [0.0, 63.0, -180.0],
            rotation: [180.0, 0.0, 0.0]
        }
    );
}

#[test]
fn takes_the_later_of_two_fixtures_with_the_same_uuid() {
    let stage = StageSize::default();
    let by_hand = StageFixture {
        uuid: "a",
        positioning: "manual",
        position: [1.0, 2.0, 3.0],
        rotation: [4.0, 5.0, 6.0],
    };

    // placed by hand first, in a row later: the row wins (the placement by hand is overwritten)
    let placements = stage_placements(&[by_hand, fixture("a", "topBack")], &["a"], &stage);
    assert_eq!(
        placements["a"],
        StagePlacement {
            position: [0.0, 367.0, -230.0],
            rotation: [0.0, 0.0, 0.0]
        }
    );

    // in a row first, placed by hand later: placed by hand
    let placements = stage_placements(&[fixture("a", "topBack"), by_hand], &["a"], &stage);
    assert_eq!(
        placements["a"],
        StagePlacement {
            position: [1.0, 2.0, 3.0],
            rotation: [4.0, 5.0, 6.0]
        }
    );

    // placed by hand first and not in any preset later: still where it was placed by hand
    let placements = stage_placements(&[by_hand, fixture("a", "topBack")], &[], &stage);
    assert_eq!(placements["a"].position, [1.0, 2.0, 3.0]);
}

#[test]
fn counts_a_fixture_coming_back_later_once_more() {
    // only the pixels following each other count once: a, b, a are three places, a ending up at the last one
    let fixtures = [fixture("a", "topFront"), fixture("b", "topFront")];
    let placements = stage_placements(&fixtures, &["a", "b", "a", "a"], &StageSize::default());

    assert_eq!(placements["a"].position[0], 150.0);
    assert_eq!(placements["b"].position[0], 0.0);

    // a fixture of another row in between starts a new run as well
    let fixtures = [fixture("a", "topFront"), fixture("c", "bottomBack")];
    let placements = stage_placements(&fixtures, &["a", "c", "a"], &StageSize::default());
    assert_eq!(placements["a"].position[0], 100.0);
    assert_eq!(placements["c"].position[0], 0.0);

    // a preset fixture without a fixture in between too
    let placements = stage_placements(&fixtures, &["a", "gone", "a"], &StageSize::default());
    assert_eq!(placements["a"].position[0], 100.0);
}

#[test]
fn aims_a_hanging_head_at_a_spot() {
    let straight_down = get_aim_degrees(&hanging(), [0.0, 0.0, 0.0]);
    assert_bits(straight_down.pan, 0);
    assert_bits(straight_down.tilt, 0);
    // the spot right where the head turns
    assert_eq!(
        get_aim_degrees(&hanging(), [0.0, 400.0, 0.0]),
        Aim { pan: 0.0, tilt: 0.0 }
    );

    // straight towards the front of the stage, 45° up from down
    let front = get_aim_degrees(&hanging(), [0.0, 0.0, 400.0]);
    assert_close(front.pan, 0.0, 1e-9);
    assert_close(front.tilt, -45.0, 1e-9);

    // to the side: turned either way just as far, the first way
    let side = get_aim_degrees(&hanging(), [400.0, 0.0, 0.0]);
    assert_close(side.pan, -90.0, 1e-9);
    assert_close(side.tilt, 45.0, 1e-9);

    // a bit behind it, the pan turning the shorter way
    let behind = get_aim_degrees(&hanging(), [400.0, 0.0, -100.0]);
    assert!(behind.pan > -90.0 && behind.pan < 0.0);
    assert_bits(behind.pan, 0xc052fdae2fe0a3fb);
    assert_bits(behind.tilt, 0x4046ef22d78186c5);

    // standing on the floor, upside down: up in the middle
    let standing = StagePlacement {
        position: [0.0, 0.0, 0.0],
        rotation: [180.0, 0.0, 0.0],
    };
    assert_close(get_aim_degrees(&standing, [0.0, 405.4, 0.0]).tilt, 0.0, 1e-9);

    // a head turned on every axis, as recorded
    let turned = StagePlacement {
        position: [100.0, 367.0, 230.0],
        rotation: [10.0, 20.0, 30.0],
    };
    let aim = get_aim_degrees(&turned, [-50.0, 30.0, 120.0]);
    assert_bits(aim.pan, 0x4051b78a7d94dbb3);
    assert_bits(aim.tilt, 0x4049d6aa4ecf1725);
}

// ---- the fan over the stage ----

#[test]
fn places_the_parts_of_a_preset_along_the_stage() {
    // from the left to the right, from the front (towards z) to the back, from the bottom to the top
    let positions = [[-100.0, 0.0, 200.0], [100.0, 300.0, -200.0], [0.0, 100.0, 0.0]];
    assert_eq!(stage_fan_places(&positions, "left-right"), [0.0, 1.0, 0.5]);
    assert_eq!(stage_fan_places(&positions, "front-back"), [0.0, 1.0, 0.5]);
    assert_close(stage_fan_places(&positions, "bottom-top")[2], 1.0 / 3.0, 1e-12);

    // all at the same place
    assert!(stage_fan_places(&[[0.0; 3], [0.0; 3]], "left-right")[0].is_nan());

    // two of them at the same height
    let positions = [[-100.0, 400.0, 200.0], [300.0, 400.0, -100.0], [100.0, 0.0, 0.0]];
    assert_eq!(stage_fan_places(&positions, "left-right"), [0.0, 1.0, 0.5]);
    assert_eq!(stage_fan_places(&positions, "front-back"), [0.0, 1.0, 2.0 / 3.0]);
    assert_eq!(stage_fan_places(&positions, "bottom-top"), [1.0, 1.0, 0.0]);
    assert!(stage_fan_places(&positions[..2], "bottom-top")
        .iter()
        .all(|place| place.is_nan()));

    assert!(is_stage_fan_order("left-right"));
    assert!(is_stage_fan_order("front-back"));
    assert!(is_stage_fan_order("bottom-top"));
    assert!(!is_stage_fan_order("preset"));
    assert!(!is_stage_fan_order(""));
}

#[test]
fn fans_the_edge_cases() {
    // none and a single one
    assert!(stage_fan_places(&[], "left-right").is_empty());
    let alone = stage_fan_places(&[[5.0, 6.0, 7.0]], "bottom-top");
    assert_eq!(alone.len(), 1);
    assert!(alone[0].is_nan());

    // any other order is from the left to the right
    let positions = [[1.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    assert_eq!(stage_fan_places(&positions, "preset"), [0.0, 1.0, 0.5]);

    // a place which is not a number leaves none of them told apart (the lowest and the highest are none either)
    let positions = [[1.0, 0.0, 0.0], [f64::NAN, 0.0, 0.0], [2.0, 0.0, 0.0]];
    assert!(stage_fan_places(&positions, "left-right")
        .iter()
        .all(|place| place.is_nan()));

    // -0.0 and 0.0 are the same place
    assert!(stage_fan_places(&[[0.0; 3], [-0.0; 3]], "front-back")
        .iter()
        .all(|place| place.is_nan()));
}

// ---- where a part is in its fixture ----

#[test]
fn a_fresh_place_is_the_one_pixel_of_a_fixture() {
    assert_eq!(
        PixelPlace::default(),
        PixelPlace {
            x: 0.5,
            y: 0.5,
            column: 0,
            row: 0,
            columns: 1,
            rows: 1,
            aspect_x: 1.0,
            aspect_y: 1.0
        }
    );
}

#[test]
fn counts_the_pixels_of_a_matrix() {
    // the keys win, the longest row and the layer with the most rows counting
    let keys = vec![
        vec![vec!["1", "2", "3"], vec!["4", "5"]],
        vec![vec!["6"], vec!["7"], vec!["8"]],
    ];
    assert_eq!(matrix_size(Some(&keys), Some(&[Some(9), Some(9)])), (3, 3));
    assert_eq!(
        matrix_size(Some(&Vec::<Vec<Vec<String>>>::new()), Some(&[Some(9), Some(9)])),
        (0, 0)
    );

    let none: Option<&[Vec<Vec<String>>]> = None;
    assert_eq!(matrix_size(none, Some(&[Some(12), Some(1), Some(1)])), (12, 1));
    assert_eq!(matrix_size(none, Some(&[Some(12)])), (12, 0));
    assert_eq!(matrix_size(none, Some(&[None, Some(4)])), (0, 4));
    assert_eq!(matrix_size(none, Some(&[])), (0, 0));
    assert_eq!(matrix_size(none, None), (0, 0));
}

#[test]
fn places_a_part_in_its_fixture() {
    let fresh = PixelPlace::default();

    // without a pixel or without a matrix: the one pixel of the fixture
    assert_eq!(pixel_place(Some((12, 1)), None), fresh);
    assert_eq!(pixel_place(None, Some(PixelPosition::Own { x: 3, y: 1 })), fresh);
    assert_eq!(pixel_place(None, None), fresh);

    // a pixel of a bar of 12
    assert_eq!(
        pixel_place(Some((12, 1)), Some(PixelPosition::Own { x: 3, y: 1 })),
        PixelPlace {
            x: 2.5 / 12.0,
            y: 0.5,
            column: 2,
            row: 0,
            columns: 12,
            rows: 1,
            aspect_x: 1.0,
            aspect_y: 1.0 / 12.0
        }
    );

    // a pixel of a matrix higher than wide
    assert_eq!(
        pixel_place(Some((2, 4)), Some(PixelPosition::Own { x: 2, y: 4 })),
        PixelPlace {
            x: 0.75,
            y: 0.875,
            column: 1,
            row: 3,
            columns: 2,
            rows: 4,
            aspect_x: 0.5,
            aspect_y: 1.0
        }
    );

    // a group in the middle of its pixels, in the column and row the middle is in
    assert_eq!(
        pixel_place(
            Some((4, 2)),
            Some(PixelPosition::Group(&[(1, 1), (2, 1), (1, 2), (2, 2)]))
        ),
        PixelPlace {
            x: 0.25,
            y: 0.5,
            column: 0,
            row: 0,
            columns: 4,
            rows: 2,
            aspect_x: 1.0,
            aspect_y: 0.5
        }
    );
    assert_eq!(
        pixel_place(Some((4, 2)), Some(PixelPosition::Group(&[(3, 2), (4, 2)]))),
        PixelPlace {
            x: 0.75,
            y: 0.75,
            column: 2,
            row: 1,
            columns: 4,
            rows: 2,
            aspect_x: 1.0,
            aspect_y: 0.5
        }
    );

    // a group without pixels in the middle of the matrix
    assert_eq!(
        pixel_place(Some((4, 2)), Some(PixelPosition::Group(&[]))),
        PixelPlace {
            x: 0.5,
            y: 0.5,
            column: 1,
            row: 0,
            columns: 4,
            rows: 2,
            aspect_x: 1.0,
            aspect_y: 0.5
        }
    );

    // a matrix which doesn't say how large it is has one column and one row
    assert_eq!(
        pixel_place(Some((0, 0)), Some(PixelPosition::Own { x: 3, y: 2 })),
        PixelPlace {
            x: 2.5,
            y: 1.5,
            column: 2,
            row: 1,
            columns: 1,
            rows: 1,
            aspect_x: 1.0,
            aspect_y: 1.0
        }
    );
    assert_eq!(pixel_place(Some((-3, 0)), Some(PixelPosition::Group(&[]))), fresh);
}

#[test]
fn tells_a_pixel_from_a_group() {
    let group = [(1, 1), (2, 1)];

    assert_eq!(PixelPosition::of(3, 2, &group), PixelPosition::Own { x: 3, y: 2 });
    assert_eq!(PixelPosition::of(1, 0, &group), PixelPosition::Own { x: 1, y: 0 });
    assert_eq!(PixelPosition::of(0, 2, &group), PixelPosition::Group(&group));
    assert_eq!(PixelPosition::of(-1, 2, &[]), PixelPosition::Group(&[]));
}

// ---- where the pixels are on the stage ----

// where a pixel is in its fixture: its column of a bar of the passed number of pixels
fn place_in(column: i32, columns: i32) -> PixelPlace {
    PixelPlace {
        x: (f64::from(column) + 0.5) / f64::from(columns),
        y: 0.5,
        column,
        columns,
        ..PixelPlace::default()
    }
}

#[test]
fn makes_the_housing_as_large_as_the_profile_says_or_as_large_as_its_pixels_need() {
    assert_eq!(housing_size(Some(&[1057.0, 154.0, 64.0]), 12, 1), [105.7, 15.4, 6.4]);
    assert_eq!(housing_size(None, 12, 2), [96.0, 16.0, 7.0]);
    // a size of 0 is none
    assert_eq!(housing_size(Some(&[0.0, 50.0, 0.0]), 3, 1), [24.0, 5.0, 7.0]);

    // no dimensions, fewer than three, more than three, a NaN
    assert_eq!(housing_size(Some(&[]), 3, 2), [24.0, 16.0, 7.0]);
    assert_eq!(housing_size(Some(&[300.0]), 3, 2), [30.0, 16.0, 7.0]);
    assert_eq!(housing_size(Some(&[300.0, 20.0]), 3, 2), [30.0, 2.0, 7.0]);
    assert_eq!(housing_size(Some(&[300.0, 20.0, 45.0, 99.0]), 3, 2), [30.0, 2.0, 4.5]);
    assert_eq!(housing_size(Some(&[f64::NAN, -0.0, 12.5]), 3, 2), [24.0, 16.0, 1.25]);
    // a negative size is taken as it is
    assert_eq!(housing_size(Some(&[-50.0, 20.0, 45.0]), 3, 2), [-5.0, 2.0, 4.5]);
}

#[test]
fn hangs_the_housing_below_the_truss() {
    assert_eq!(housing_offset("manual", 8.0), 0.0);
    assert_eq!(housing_offset("topFront", 8.0), 4.0);
    assert_eq!(housing_offset("bottomBack", 30.0), -7.0);
    assert_eq!(housing_offset("", 10.0), 3.0);

    assert!(is_on_the_floor("bottomFront"));
    assert!(is_on_the_floor("bottomBack"));
    assert!(!is_on_the_floor("topFront"));
    assert!(!is_on_the_floor("topBack"));
    assert!(!is_on_the_floor("manual"));
    assert!(!is_on_the_floor(""));
}

#[test]
fn places_the_leds_on_the_front_of_the_housing() {
    let size = [24.0, 8.0, 6.0];
    let hanging = StagePlacement {
        position: [100.0, 367.0, 230.0],
        rotation: [0.0, 0.0, 0.0],
    };
    let standing = StagePlacement {
        position: [100.0, 43.0, 230.0],
        rotation: [180.0, 0.0, 0.0],
    };

    // the first of three pixels a third of the housing left of its middle, the top of the housing 8 cm above where its
    // fixture is placed, the LEDs facing the audience
    assert_eq!(
        round(&pixel_position(&place_in(0, 3), size, &hanging, "topFront")),
        [92.0, 371.0, 233.0]
    );
    // on the floor the housing stands above it, upright and facing the audience as well
    assert_eq!(
        round(&pixel_position(&place_in(0, 3), size, &standing, "bottomFront")),
        [92.0, 39.0, 233.0]
    );
    // placed by hand, the housing is turned around its middle
    let by_hand = StagePlacement {
        position: [0.0, 200.0, 0.0],
        rotation: [0.0, 0.0, 90.0],
    };
    assert_eq!(
        round(&pixel_position(&place_in(0, 3), size, &by_hand, "manual")),
        [0.0, 192.0, 3.0]
    );

    // a housing turned on every axis, as recorded
    let turned = StagePlacement {
        position: [100.0, 367.0, 230.0],
        rotation: [10.0, 20.0, 30.0],
    };
    let position = pixel_position(
        &PixelPlace {
            x: 1.0 / 6.0,
            ..PixelPlace::default()
        },
        size,
        &turned,
        "topFront",
    );
    assert_bits(position[0], 0x405728b9095e6bde);
    assert_bits(position[1], 0x4076d73d458c8b86);
    assert_bits(position[2], 0x406d7617f8014a60);
}

#[test]
fn places_a_part_on_the_stage() {
    let hanging = StagePlacement {
        position: [100.0, 367.0, 230.0],
        rotation: [0.0, 0.0, 0.0],
    };

    // the housing as large as the pixels of the part need: three columns of 8 cm, one row, 7 cm deep
    assert_eq!(
        part_stage_position(&place_in(0, 3), None, Some(&hanging), "topFront"),
        [92.0, 371.0, 233.5]
    );
    // as large as the profile says
    assert_eq!(
        part_stage_position(&place_in(0, 3), Some(&[240.0, 80.0, 60.0]), Some(&hanging), "topFront"),
        pixel_position(&place_in(0, 3), [24.0, 8.0, 6.0], &hanging, "topFront")
    );

    // a fixture nobody placed is at the origin, not turned
    assert_eq!(
        part_stage_position(&PixelPlace::default(), None, None, "topFront"),
        [0.0, 4.0, 3.5]
    );
    assert_eq!(
        part_stage_position(&PixelPlace::default(), None, None, "bottomBack"),
        [0.0, 4.0, -3.5]
    );
    assert_eq!(
        part_stage_position(&PixelPlace::default(), None, None, "manual"),
        [0.0, 0.0, 3.5]
    );
    assert_eq!(
        part_stage_position(&place_in(2, 3), None, None, "topBack"),
        part_stage_position(&place_in(2, 3), None, Some(&StagePlacement::ORIGIN), "topBack")
    );
}

#[test]
fn rounds_to_millimeters() {
    assert_eq!(millimeters(1.24), 12);
    assert_eq!(millimeters(1.25), 13);
    assert_eq!(millimeters(-1.25), -12);
    assert_eq!(millimeters(-1.26), -13);
    assert_eq!(millimeters(0.0), 0);
    assert_eq!(millimeters(f64::NAN), 0);
    assert_eq!(millimeters(f64::INFINITY), i64::MAX);
    assert_eq!(millimeters(f64::NEG_INFINITY), i64::MIN);

    let places = indices(&BTreeSet::from([30, -5, 12]));
    assert_eq!(places.len(), 3);
    assert_eq!((places[&-5], places[&12], places[&30]), (0, 1, 2));
    assert!(indices(&BTreeSet::new()).is_empty());
}

#[test]
fn puts_the_leds_into_the_columns_and_rows_of_one_picture() {
    // two bars of three pixels, a meter apart, seen from the front
    let positions: Vec<[f64; 3]> = [-208.0, -200.0, -192.0, -108.0, -100.0, -92.0]
        .iter()
        .map(|&x| [x, 371.0, 233.0])
        .collect();
    let places = pixel_places(&positions, "front");

    assert_eq!(
        places.iter().map(|place| place.column).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );
    assert!(places
        .iter()
        .all(|place| place.columns == 6 && place.rows == 1 && place.y == 0.5 && place.row == 0));
    // as far apart as they are on the stage, the first and the last in the middle of a pixel of their own
    let xs: Vec<f64> = places.iter().map(|place| place.x).collect();
    assert_eq!(round(&xs), [0.083, 0.141, 0.198, 0.802, 0.859, 0.917]);
    assert_eq!((places[0].aspect_x, places[0].aspect_y), (1.0, 0.0));
}

#[test]
fn sees_the_leds_from_above_and_finds_a_single_one_in_the_middle() {
    let positions = [[0.0, 371.0, -230.0], [0.0, 39.0, 230.0], [100.0, 39.0, 230.0]];
    let places = pixel_places(&positions, "top");

    assert_eq!(
        places.iter().map(|place| (place.column, place.row)).collect::<Vec<_>>(),
        [(0, 0), (0, 1), (1, 1)]
    );
    assert_eq!(places[0].y, 0.25);

    let alone = pixel_places(&[[5.0, 6.0, 7.0]], "front")[0];
    assert_eq!((alone.x, alone.y, alone.aspect_x, alone.aspect_y), (0.5, 0.5, 1.0, 1.0));
    assert_eq!(alone, PixelPlace::default());

    // any other view is the one from the front: two rows, the upper one first
    let places = pixel_places(&positions, "");
    assert_eq!(
        places.iter().map(|place| (place.column, place.row)).collect::<Vec<_>>(),
        [(0, 0), (0, 1), (1, 1)]
    );
    assert_eq!(places, pixel_places(&positions, "front"));
    assert_eq!((places[0].columns, places[0].rows), (2, 2));

    // none
    assert!(pixel_places(&[], "top").is_empty());

    // less than a millimeter apart is the same column, the picture still framed around both
    let places = pixel_places(&[[10.0, 0.0, 0.0], [10.04, 0.0, 0.0], [20.0, 0.0, 0.0]], "front");
    assert_eq!(places.iter().map(|place| place.column).collect::<Vec<_>>(), [0, 0, 1]);
    assert_eq!(places[0].columns, 2);
    assert_eq!(places[0].x, 0.25);
    assert_eq!(places[2].x, 0.75);
    assert!(places[1].x > 0.25 && places[1].x < 0.26);
}

// ---- the same doubles over a sweep ----
//
// Each sweep below runs on inputs from a generator with a fixed seed. The numbers are the recorded
// checksums over the bits of everything that came out, so a single differing digit shows.

struct Sweep {
    state: u64,
    hash: u64,
}

const MAGNITUDES: [f64; 7] = [1e-3, 1e-2, 1e-1, 1.0, 1e1, 1e2, 1e3];
const POSITIONINGS: [&str; 6] = ["topFront", "bottomFront", "topBack", "bottomBack", "manual", "other"];

impl Sweep {
    fn new(seed: u64) -> Sweep {
        Sweep {
            state: seed,
            hash: 0xcbf29ce484222325,
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    fn pick(&mut self, count: u64) -> i32 {
        (self.next() % count) as i32
    }

    fn mix_long(&mut self, value: i64) {
        self.hash = (self.hash ^ value as u64).wrapping_mul(0x100000001B3);
    }

    fn mix(&mut self, value: f64) {
        // every NaN is the same
        let bits = if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        };
        self.hash = (self.hash ^ bits).wrapping_mul(0x100000001B3);
    }

    fn mix_place(&mut self, place: &PixelPlace) {
        self.mix(place.x);
        self.mix(place.y);
        self.mix_long(i64::from(place.column));
        self.mix_long(i64::from(place.row));
        self.mix_long(i64::from(place.columns));
        self.mix_long(i64::from(place.rows));
        self.mix(place.aspect_x);
        self.mix(place.aspect_y);
    }

    fn coordinate(&mut self) -> f64 {
        (self.unit() - 0.5) * 1000.0
    }

    fn angle(&mut self) -> f64 {
        match self.pick(4) {
            0 => 90.0 * f64::from(self.pick(17) - 8),
            1 => 15.0 * f64::from(self.pick(97) - 48),
            2 => (self.unit() - 0.5) * 1440.0,
            _ => (self.unit() - 0.5) * 360.0,
        }
    }

    fn coordinates(&mut self) -> [f64; 3] {
        [self.coordinate(), self.coordinate(), self.coordinate()]
    }

    fn angles(&mut self) -> [f64; 3] {
        [self.angle(), self.angle(), self.angle()]
    }
}

#[test]
fn the_sine_is_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(1);

    for _ in 0..200_000 {
        let scale = [1.0, 4.0, 100.0, 1e6][sweep.pick(4) as usize];
        let value = sin_cycle((sweep.unit() - 0.5) * scale);
        sweep.mix(value);
    }

    for k in -64..=64 {
        sweep.mix(sin_cycle(f64::from(k) / 16.0));
    }

    assert_eq!(sweep.hash, 0x0ddef4d00a484b48);
}

#[test]
fn the_arc_tangent_is_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(2);

    for _ in 0..200_000 {
        let magnitude = MAGNITUDES[sweep.pick(7) as usize];
        let value = atan((sweep.unit() - 0.5) * magnitude);
        sweep.mix(value);
    }

    assert_eq!(sweep.hash, 0xa4170aafccdd34ac);

    let mut sweep = Sweep::new(3);

    for _ in 0..200_000 {
        let y = (sweep.unit() - 0.5) * MAGNITUDES[sweep.pick(7) as usize];
        let x = (sweep.unit() - 0.5) * MAGNITUDES[sweep.pick(7) as usize];
        sweep.mix(atan2(y, x));
    }

    assert_eq!(sweep.hash, 0x6e0a0616dc37b174);
}

#[test]
fn the_rotations_and_aims_are_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(4);

    for _ in 0..50_000 {
        let matrix = get_rotation_matrix(sweep.angles());

        for row in matrix {
            for value in row {
                sweep.mix(value);
            }
        }
    }

    assert_eq!(sweep.hash, 0xf9a0c31997344e1b);

    let mut sweep = Sweep::new(5);

    for _ in 0..50_000 {
        let position = sweep.coordinates();
        let rotation = sweep.angles();
        let target = sweep.coordinates();
        let aim = get_aim_degrees(&StagePlacement { position, rotation }, target);
        sweep.mix(aim.pan);
        sweep.mix(aim.tilt);
    }

    assert_eq!(sweep.hash, 0x7dab2d6ac2b21060);
}

#[test]
fn the_placements_are_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(6);

    for _ in 0..200 {
        let stage = StageSize {
            width_cm: 100.0 + sweep.unit() * 900.0,
            height_cm: 100.0 + sweep.unit() * 500.0,
            depth_cm: 200.0 + sweep.unit() * 800.0,
            floor_height_cm: sweep.unit() * 100.0,
        };

        // fixtures sharing a uuid, in every row, by hand and nowhere
        let count = 1 + sweep.pick(30);
        let mut drawn: Vec<(String, &str, [f64; 3], [f64; 3])> = Vec::new();

        for _ in 0..count {
            let uuid = format!("f{}", sweep.pick(25));
            let positioning = POSITIONINGS[sweep.pick(6) as usize];
            let position = sweep.coordinates();
            let rotation = sweep.angles();
            drawn.push((uuid, positioning, position, rotation));
        }

        // preset fixtures following each other (the pixels of a fixture), coming back later and without a fixture
        let count = sweep.pick(60);
        let mut preset_fixture_uuids: Vec<String> = Vec::new();

        for index in 0..count {
            if index > 0 && sweep.pick(3) == 0 {
                preset_fixture_uuids.push(preset_fixture_uuids[index as usize - 1].clone());
            } else {
                preset_fixture_uuids.push(format!("f{}", sweep.pick(28)));
            }
        }

        let fixtures: Vec<StageFixture> = drawn
            .iter()
            .map(|(uuid, positioning, position, rotation)| StageFixture {
                uuid,
                positioning,
                position: *position,
                rotation: *rotation,
            })
            .collect();
        let uuids: Vec<&str> = preset_fixture_uuids.iter().map(String::as_str).collect();
        let placements = stage_placements(&fixtures, &uuids, &stage);

        for k in 0..28 {
            match placements.get(&format!("f{k}")) {
                None => sweep.mix_long(-1),
                Some(placement) => {
                    for value in placement.position {
                        sweep.mix(value);
                    }

                    for value in placement.rotation {
                        sweep.mix(value);
                    }
                }
            }
        }

        sweep.mix_long(placements.len() as i64);
    }

    assert_eq!(sweep.hash, 0xd7eeeb664854f7f6);
}

#[test]
fn the_parts_are_on_the_stage_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(7);

    for _ in 0..50_000 {
        // no physical entry, fewer dimensions than three, sizes of 0
        let kind = sweep.pick(4);
        let dimensions: Option<Vec<f64>> = if kind != 0 {
            let length = if kind == 1 { sweep.pick(4) } else { 3 };
            let mut dimensions = Vec::new();

            for _ in 0..length {
                dimensions.push(if sweep.pick(4) == 0 { 0.0 } else { sweep.unit() * 2000.0 });
            }

            Some(dimensions)
        } else {
            None
        };

        let columns = 1 + sweep.pick(16);
        let rows = 1 + sweep.pick(8);
        let x = sweep.unit();
        let y = sweep.unit();
        let place = PixelPlace {
            x,
            y,
            columns,
            rows,
            ..PixelPlace::default()
        };

        let placement = if sweep.pick(5) == 0 {
            None
        } else {
            let position = sweep.coordinates();
            let rotation = sweep.angles();
            Some(StagePlacement { position, rotation })
        };
        let positioning = POSITIONINGS[sweep.pick(6) as usize];

        let size = housing_size(dimensions.as_deref(), columns, rows);
        let position = part_stage_position(&place, dimensions.as_deref(), placement.as_ref(), positioning);

        for value in size {
            sweep.mix(value);
        }

        for value in position {
            sweep.mix(value);
        }
    }

    assert_eq!(sweep.hash, 0xddc0ca6e57213746);
}

#[test]
fn the_pictures_are_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(8);

    for _ in 0..2_000 {
        let count = sweep.pick(40);
        let view = if sweep.pick(2) == 0 { "top" } else { "front" };
        // on a grid (LEDs sharing columns and rows) or anywhere
        let quantized = sweep.pick(2) == 0;
        let mut positions: Vec<[f64; 3]> = Vec::new();

        for _ in 0..count {
            let mut position = [0.0; 3];

            for value in &mut position {
                *value = if quantized {
                    f64::from(sweep.pick(9) - 4) * 12.5
                } else {
                    sweep.coordinate()
                };
            }

            positions.push(position);
        }

        for place in pixel_places(&positions, view) {
            sweep.mix_place(&place);
        }
    }

    assert_eq!(sweep.hash, 0x324a9dadd0ab2267);
}

#[test]
fn the_fans_are_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(9);
    let orders = ["left-right", "front-back", "bottom-top", "preset"];

    for _ in 0..2_000 {
        let count = sweep.pick(20);
        let order = orders[sweep.pick(4) as usize];
        // at a few places only (all at the same one now and then) or anywhere
        let quantized = sweep.pick(3) == 0;
        let mut positions: Vec<[f64; 3]> = Vec::new();

        for _ in 0..count {
            let mut position = [0.0; 3];

            for value in &mut position {
                *value = if quantized {
                    f64::from(sweep.pick(3) - 1) * 50.0
                } else {
                    sweep.coordinate()
                };
            }

            positions.push(position);
        }

        for place in stage_fan_places(&positions, order) {
            sweep.mix(place);
        }
    }

    assert_eq!(sweep.hash, 0xa6315d0eb5f28564);
}

#[test]
fn the_parts_are_in_their_fixtures_as_recorded_over_a_sweep() {
    let mut sweep = Sweep::new(10);

    for _ in 0..50_000 {
        let present = sweep.pick(8) != 0;
        let size = (sweep.pick(10), sweep.pick(6));
        let kind = sweep.pick(4);
        let mut group: Vec<(i32, i32)> = Vec::new();

        let pixel = if kind == 1 {
            let x = 1 + sweep.pick(12);
            let y = 1 + sweep.pick(8);
            Some(PixelPosition::of(x, y, &[]))
        } else if kind >= 2 {
            for _ in 0..sweep.pick(6) {
                let x = 1 + sweep.pick(12);
                let y = 1 + sweep.pick(8);
                group.push((x, y));
            }

            Some(PixelPosition::of(0, 0, &group))
        } else {
            None
        };

        let place = pixel_place(if present { Some(size) } else { None }, pixel);
        sweep.mix_place(&place);
    }

    assert_eq!(sweep.hash, 0x341cc6f4e69757cb);
}
