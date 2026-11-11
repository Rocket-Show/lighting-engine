//! The conformance cases of the parts of the engine: the shape of a transition, an effect curve, the steps of a
//! preset and how much of a preset reaches the stage (see `conformance/README.md`). The recorded values come from
//! double precision, the core works in single precision and on whole nanoseconds.

use std::path::PathBuf;

use engine_compiler::compile::curve_of;
use engine_compiler::{compile_json, Compiled, Options};
use engine_core::engine::caps::{effect_amount, for_each_channel_value, materialize, CapList};
use engine_core::engine::steps::{state_at, transition_curve};
use engine_core::engine::{curve, preset_intensity};
use engine_core::format::{capv_flag, preset_flag};
use engine_core::Show;
use serde_json::{json, Value};

fn cases(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases")
        .join(name);
    let file: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("the cases")).expect("JSON");
    file["cases"].as_array().expect("cases").clone()
}

fn compiled(project: Value) -> Compiled {
    compile_json(&project.to_string(), &Options::default()).expect("the project compiles")
}

fn ns(millis: f64) -> i64 {
    (millis * 1e6).round() as i64
}

fn millis(ns: i64) -> f64 {
    ns as f64 / 1e6
}

#[test]
fn transition_curves() {
    let mut largest = 0f64;

    for case in cases("transition-curve.json") {
        let input = &case["input"];
        let position = input["position"]
            .as_f64()
            .map(|position| position as f32)
            .unwrap_or(f32::NAN);
        let shaped = transition_curve(curve_of(input["curve"].as_str().unwrap_or("")), position) as f64;
        let difference = (shaped - case["expected"].as_f64().expect("a number")).abs();
        largest = largest.max(difference);
        assert!(
            difference < 1e-6,
            "{}: {shaped} instead of {}",
            case["name"],
            case["expected"]
        );
    }

    println!("transition curves: largest difference {largest:e}");
}

/// The value of a curve may differ where a time is within a hair of an edge of the curve (the jump of a square or a
/// sawtooth, the start or the end of a run): the recorded value is decided by the rounding of a double there.
fn effect_curves(file: &str) {
    let mut largest_time = 0f64;
    let mut largest_value = 0f64;
    let mut samples = 0;
    let mut edges = Vec::new();

    for case in cases(file) {
        let input = &case["input"];
        let expected = &case["expected"];
        let compiled =
            compiled(json!({ "version": 11, "presets": [{ "uuid": "preset", "effects": [input["curve"]] }] }));
        let show = Show::check(&compiled.show).expect("a show");
        let preset = show.get(show.root.presets, 0).expect("the preset");
        let effect = show
            .get(preset.effects, 0)
            .unwrap_or_else(|| panic!("{}: the curve is not compiled", case["name"]));
        let bpm = input["beatsPerMinute"]
            .as_f64()
            .map(|bpm| (bpm * 4_294_967_296.0).round() as i64);
        let count = input["fixtureCount"].as_u64().unwrap_or(0) as u32;

        let mut time = |actual: Option<i64>, expected: &Value, what: &str| match (actual, expected.as_f64()) {
            (Some(actual), Some(expected)) => {
                let difference = (millis(actual) - expected).abs();
                largest_time = largest_time.max(difference);
                assert!(
                    difference <= 2e-6 * expected.abs().max(1.0),
                    "{} {what}: {} ms instead of {expected}",
                    case["name"],
                    millis(actual)
                );
            }
            (None, None) => {}
            (actual, expected) => panic!("{} {what}: {actual:?} instead of {expected:?}", case["name"]),
        };

        time(Some(curve::length(&effect, bpm)), &expected["lengthMillis"], "length");
        time(Some(curve::phase(&effect, bpm)), &expected["phaseMillis"], "phase");
        time(curve::run_end(&effect, bpm), &expected["runEndMillis"], "run end");
        time(
            curve::run_loop(&effect, count, bpm),
            &expected["runLoopMillis"],
            "run loop",
        );

        for (sample, recorded) in input["samples"]
            .as_array()
            .expect("samples")
            .iter()
            .zip(expected["samples"].as_array().expect("samples"))
        {
            let index = sample["fixtureIndex"].as_u64().unwrap_or(0) as u32;
            samples += 1;
            time(
                Some(curve::phasing(&effect, index, count, bpm)),
                &recorded[0],
                "phasing",
            );

            let value = curve::value_at(
                &effect,
                ns(sample["timeMillis"].as_f64().expect("a time")),
                index,
                count,
                bpm,
            );

            match (value, recorded[1].as_f64()) {
                (Some(value), Some(recorded_value)) if (value as f64 - recorded_value).abs() <= 2e-3 => {
                    largest_value = largest_value.max((value as f64 - recorded_value).abs())
                }
                (None, None) => {}
                (value, recorded_value) => edges.push(format!(
                    "{} at {} ms on fixture {index}: {value:?} instead of {recorded_value:?}",
                    case["name"], sample["timeMillis"]
                )),
            }
        }
    }

    println!("{file}: {samples} samples, largest difference of a time {largest_time:e} ms, of a value {largest_value:e}, {} on an edge", edges.len());

    for edge in &edges {
        println!("    {edge}");
    }

    // (the edges are looked at one by one in conformance/DIFFERENCES.md)
    assert!(
        edges.len() <= expected_edges(file),
        "{file}: {} samples differ, more than the {} explained",
        edges.len(),
        expected_edges(file)
    );
}

fn expected_edges(file: &str) -> usize {
    match file {
        "effect-curve.json" => EFFECT_CURVE_EDGES,
        _ => EFFECT_CURVE_EDGE_CASE_EDGES,
    }
}

const EFFECT_CURVE_EDGES: usize = 0;
const EFFECT_CURVE_EDGE_CASE_EDGES: usize = 0;

#[test]
fn effect_curve() {
    effect_curves("effect-curve.json");
}

#[test]
fn effect_curve_edges() {
    effect_curves("effect-curve-edges.json");
}

fn preset_steps(file: &str) -> (usize, Vec<String>) {
    let mut samples = 0;
    let mut differences = Vec::new();

    for case in cases(file) {
        let input = &case["input"];
        let expected = &case["expected"];
        let compiled = compiled(json!({ "version": 11, "presets": [input["preset"]] }));
        let show = Show::check(&compiled.show).expect("a show");
        let preset = show.get(show.root.presets, 0).expect("the preset");
        let name = case["name"].as_str().unwrap_or("?");

        let ordered = matches!(
            input["preset"]["stepsOrder"].as_str(),
            Some("backward" | "pingpong" | "random")
        );
        let recorded_loop = expected["loopMillis"].as_f64().expect("loopMillis");

        // (the length of a pass of the steps as they are written down)
        if !ordered || input["preset"]["steps"].as_array().map(Vec::len).unwrap_or(0) < 2 {
            assert!(
                (millis(preset.loop_length) - recorded_loop).abs() < 2e-6,
                "{name}: a pass of {} ms instead of {recorded_loop}",
                millis(preset.loop_length)
            );
        }

        assert_eq!(
            preset.flags & preset_flag::PHASED != 0,
            expected["phased"].as_bool().expect("phased"),
            "{name}: phased"
        );

        let effects: Vec<Option<&str>> = input["preset"]["effects"]
            .as_array()
            .map(|effects| effects.iter().map(|effect| effect["uuid"].as_str()).collect())
            .unwrap_or_default();

        for (sample, recorded) in input["samples"]
            .as_array()
            .expect("samples")
            .iter()
            .zip(expected["samples"].as_array().expect("samples"))
        {
            samples += 1;
            let time = sample["timeMillis"].as_f64().expect("a time")
                - recorded["phasingMillis"].as_f64().expect("phasingMillis");
            let sel = state_at(&show, &preset, ns(time));
            let state = &recorded["state"];
            let place = format!("{name} at {time} ms");

            // the step the preset is on
            let step = compiled.index.steps[0]
                .get(sel.to as usize)
                .map(String::as_str)
                .unwrap_or("");

            if input["preset"]["steps"].as_array().map(Vec::len).unwrap_or(0) > 0
                && state["step"].as_str() != Some(step)
            {
                differences.push(format!("{place}: on step {step:?} instead of {}", state["step"]));
                continue;
            }

            // the values of the capabilities, in their order
            let mut caps = CapList::new();
            materialize(&show, &preset, &sel, &mut caps);
            let recorded_caps = state["capabilities"].as_array().expect("capabilities");

            if caps.len() != recorded_caps.len() {
                differences.push(format!(
                    "{place}: {} capability values instead of {}",
                    caps.len(),
                    recorded_caps.len()
                ));
                continue;
            }

            for (value, recorded_value) in caps.iter().zip(recorded_caps) {
                let percentage = if value.v.flags & capv_flag::PERCENTAGE != 0 {
                    Some(value.v.percentage as f64)
                } else {
                    None
                };
                let close = |a: Option<f64>, b: Option<f64>| match (a, b) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-5,
                    (None, None) => true,
                    _ => false,
                };

                if !close(percentage, recorded_value["valuePercentage"].as_f64()) {
                    differences.push(format!("{place}: {:?} instead of {}", percentage, recorded_value));
                }

                let travelling = recorded_value.get("transitionFrom").is_some();

                if travelling != !value.from.is_empty()
                    || (travelling
                        && !close(
                            Some(value.position as f64),
                            recorded_value["transitionPosition"].as_f64(),
                        ))
                {
                    differences.push(format!(
                        "{place}: travelling {} at {} instead of {}",
                        !value.from.is_empty(),
                        value.position,
                        recorded_value
                    ));
                }
            }

            // the values of the channels, in their order
            let mut channel_values = Vec::new();
            for_each_channel_value(&show, &preset, &sel, |_, _, value| channel_values.push(value as f64));
            let recorded_channels = state["channels"].as_array().expect("channels");
            let recorded_values: Vec<f64> = recorded_channels
                .iter()
                .filter_map(|channel| channel[2].as_f64())
                .collect();

            if channel_values.len() != recorded_values.len()
                || channel_values
                    .iter()
                    .zip(&recorded_values)
                    .any(|(a, b)| (a - b).abs() > 1e-2)
            {
                differences.push(format!(
                    "{place}: channel values {channel_values:?} instead of {recorded_values:?}"
                ));
            }

            // how far the effects are open
            for (uuid, amount) in state["effects"].as_object().expect("effects") {
                for (index, effect) in effects.iter().enumerate() {
                    if *effect == Some(uuid.as_str()) {
                        let actual = effect_amount(&show, &preset, &sel, index as u32) as f64;

                        if (actual - amount.as_f64().unwrap_or(1.0)).abs() > 1e-5 {
                            differences.push(format!("{place}: effect {uuid} open {actual} instead of {amount}"));
                        }
                    }
                }
            }
        }
    }

    (samples, differences)
}

fn check_preset_steps(file: &str) {
    let (samples, differences) = preset_steps(file);
    println!("{file}: {samples} samples, {} differences", differences.len());

    for difference in differences.iter().take(20) {
        println!("    {difference}");
    }

    assert!(differences.is_empty(), "{file}: {} samples differ", differences.len());
}

#[test]
fn preset_steps_plain() {
    check_preset_steps("preset-steps.json");
}

#[test]
fn preset_steps_orders() {
    check_preset_steps("preset-steps-orders.json");
}

#[test]
fn preset_steps_edges() {
    check_preset_steps("preset-steps-edges.json");
}

fn preset_intensities(file: &str) {
    let mut largest = 0f64;
    let mut times = 0;

    for case in cases(file) {
        let input = &case["input"];
        let scenes: Vec<Value> = input.get("scene").cloned().into_iter().collect();
        let compiled = compiled(json!({ "version": 11, "presets": [input["preset"]], "scenes": scenes }));
        let show = Show::check(&compiled.show).expect("a show");
        let preset = show.get(show.root.presets, 0).expect("the preset");
        let scene = show.get(show.root.scenes, 0);
        let region = input.get("region").map(|region| {
            (
                ns(region["startMillis"].as_f64().unwrap_or(0.0)),
                ns(region["endMillis"].as_f64().unwrap_or(0.0)),
            )
        });

        for (time, recorded) in input["times"]
            .as_array()
            .expect("times")
            .iter()
            .zip(case["expected"].as_array().expect("intensities"))
        {
            times += 1;
            let intensity =
                preset_intensity(&preset, scene.as_ref(), region, ns(time.as_f64().expect("a time"))) as f64;
            let difference = (intensity - recorded.as_f64().expect("an intensity")).abs();
            largest = largest.max(difference);
            assert!(
                difference < 1e-5,
                "{} at {time} ms: {intensity} instead of {recorded}",
                case["name"]
            );
        }
    }

    println!("{file}: {times} times, largest difference {largest:e}");
}

#[test]
fn preset_intensity_plain() {
    preset_intensities("preset-intensity.json");
}

#[test]
fn preset_intensity_edges() {
    preset_intensities("preset-intensity-edges.json");
}
