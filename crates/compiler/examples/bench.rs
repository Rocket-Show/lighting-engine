//! How long the compiler takes for a project and the core for a frame, on three rigs: 30 and 60 moving heads (17
//! channels each) playing two scenes with fans, a chase, a movement and steps, and 16 matrices of 16 x 8 RGB pixels
//! lit by a pixel effect.
//!
//!   cargo run --release -p lighting-engine-compiler --example bench

use std::path::PathBuf;
use std::time::Instant;

use engine_compiler::{compile, model, Options};
use engine_core::{Engine, Show};
use serde_json::{json, Value};

fn heads(count: usize) -> Value {
    let fixtures: Vec<Value> = (0..count)
        .map(|index| json!({ "uuid": format!("head-{index}"), "profileUuid": "dts/xr4-spot", "modeShortName": "17ch", "dmxUniverseUuid": format!("universe-{}", index / 30), "dmxFirstChannel": (index % 30) * 17 }))
        .collect();
    let all: Vec<Value> = (0..count)
        .map(|index| json!({ "fixtureUuid": format!("head-{index}") }))
        .collect();
    let half: Vec<Value> = (0..count)
        .step_by(2)
        .map(|index| json!({ "fixtureUuid": format!("head-{index}") }))
        .collect();

    json!({
        "version": 11,
        "fixtureProfiles": [{ "$profile": "dts/xr4-spot" }],
        "fixtures": fixtures,
        "presetFixtures": all,
        "presets": [
            {
                "uuid": "look",
                "fixtures": all,
                "steps": [{ "uuid": "look-step", "startMillis": 0, "fixtureCapabilityValues": [
                    { "type": "Intensity", "valuePercentage": 0.5, "fan": [{ "position": 0, "valuePercentage": 0.2 }, { "position": 1, "valuePercentage": 1 }] },
                    { "type": "ColorIntensity", "color": "Red", "valuePercentage": 1 },
                    { "type": "ColorIntensity", "color": "Green", "valuePercentage": 0.3 },
                    { "type": "ColorIntensity", "color": "Blue", "valuePercentage": 0.1 },
                    { "type": "Pan", "valueDegrees": 0, "spreadDegrees": 120 },
                    { "type": "Tilt", "valueDegrees": 30 },
                    { "type": "Zoom", "valuePercentage": 0.4 },
                    { "type": "ShutterStrobe", "valuePercentage": 0 }
                ] }],
                "effects": [
                    { "type": "curve", "uuid": "chase", "curveType": "sine", "lengthMillis": 2000, "phasingMode": "spread", "capabilities": [{ "type": "Intensity" }] },
                    { "type": "pan-tilt", "uuid": "circle", "shape": "circle", "lengthMillis": 4000, "phasingMode": "spread", "fanMode": "linear" }
                ]
            },
            {
                "uuid": "steps",
                "fixtures": half,
                "stepsLoop": true,
                "stepsPhasingMode": "spread",
                "steps": [
                    { "uuid": "a", "startMillis": 0, "transitionMillis": 500, "fixtureCapabilityValues": [{ "type": "ColorIntensity", "color": "Red", "valuePercentage": 0 }, { "type": "ColorIntensity", "color": "Blue", "valuePercentage": 1 }, { "type": "Iris", "valuePercentage": 0.2 }] },
                    { "uuid": "b", "startMillis": 1000, "transitionMillis": 500, "fixtureCapabilityValues": [{ "type": "ColorIntensity", "color": "Red", "valuePercentage": 1 }, { "type": "ColorIntensity", "color": "Blue", "valuePercentage": 0 }, { "type": "Iris", "valuePercentage": 1 }] },
                    { "uuid": "c", "startMillis": 2000, "transitionMillis": 500, "fixtureCapabilityValues": [{ "type": "ColorIntensity", "color": "Green", "valuePercentage": 1 }, { "type": "Iris", "valuePercentage": 0.6 }] }
                ],
                "effects": [{ "type": "curve", "uuid": "zoom", "curveType": "triangle", "lengthMode": "beats", "lengthBeats": 2, "capabilities": [{ "type": "Zoom" }] }]
            }
        ],
        "scenes": [{ "uuid": "base", "presetUuids": ["look"] }, { "uuid": "top", "presetUuids": ["steps"], "dimmer": 0.8 }]
    })
}

fn matrices(count: usize) -> Value {
    let fixtures: Vec<Value> =
        (0..count).map(|index| json!({ "uuid": format!("matrix-{index}"), "profileUuid": "matrix", "modeShortName": "384ch", "dmxUniverseUuid": format!("universe-{index}"), "dmxFirstChannel": 0 })).collect();
    // (the pixels of a plane are named by their column and their row)
    let pixels: Vec<Value> = (0..count)
        .flat_map(|index| {
            (1..=8).flat_map(move |y| {
                (1..=16).map(
                    move |x| json!({ "fixtureUuid": format!("matrix-{index}"), "pixelKey": format!("({x}, {y})") }),
                )
            })
        })
        .collect();
    let channel =
        |color: &str| json!({ "fineChannelAliases": [], "capability": { "type": "ColorIntensity", "color": color } });

    json!({
        "version": 11,
        "fixtureProfiles": [{
            "uuid": "matrix",
            "matrix": { "pixelCount": [16, 8, 1] },
            "availableChannels": {},
            "templateChannels": { "Red $pixelKey": channel("Red"), "Green $pixelKey": channel("Green"), "Blue $pixelKey": channel("Blue") },
            "modes": [{ "name": "384-channel", "shortName": "384ch", "channels": [{ "name": "", "insert": "matrixChannels", "repeatFor": ["eachPixelXYZ"], "channelOrder": "perPixel", "templateChannels": ["Red $pixelKey", "Green $pixelKey", "Blue $pixelKey"] }] }]
        }],
        "fixtures": fixtures,
        "presetFixtures": pixels,
        "presets": [{
            "uuid": "pixels",
            "fixtures": pixels,
            "steps": [{ "uuid": "step", "fixtureCapabilityValues": [{ "type": "ColorIntensity", "color": "Red", "valuePercentage": 0.5 }] }],
            "effects": [{ "type": "pixel", "uuid": "plasma", "pattern": "plasma", "lengthMillis": 4000, "palette": { "ref": "rainbow", "stops": [{ "position": 0, "color": "#ff0000" }, { "position": 0.5, "color": "#00ff00" }, { "position": 1, "color": "#0000ff" }] } }]
        }],
        "scenes": [{ "uuid": "scene", "presetUuids": ["pixels"] }]
    })
}

fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times[times.len() / 2]
}

fn measure(name: &str, mut project: Value) {
    let profiles = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/profiles");
    model::resolve_profiles(&mut project, &|profile| {
        std::fs::read_to_string(profiles.join(format!("{profile}.json"))).ok()
    })
    .expect("the profiles");
    let json = project.to_string();

    // reading the JSON and compiling it, as an edit of a project costs it
    let mut parse_times = Vec::new();
    let mut compile_times = Vec::new();
    let mut compiled = None;

    for _ in 0..15 {
        let started = Instant::now();
        let parsed = model::parse_project(&json).expect("the project");
        parse_times.push(started.elapsed().as_secs_f64() * 1000.0);

        let started = Instant::now();
        compiled = Some(compile(&parsed, &Options::default()).expect("the show"));
        compile_times.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let compiled = compiled.expect("a show");
    let show = Show::check(&compiled.show).expect("the show passes its check");
    let mut memory = vec![0u32; show.header.memory_words as usize];
    let mut universes = vec![0u8; 512 * show.root.universes as usize];
    let mut engine = Engine::new(show, &mut memory, 0).expect("the engine");
    engine.play_preview(false);

    for scene in 0..show.root.scenes.count {
        engine.select_scene(scene, true);
    }

    engine.set_step_preview(true, 0);
    engine.set_position(0, 0, true);

    // warm up, then measure frames 25 ms apart
    for frame in 0..200u64 {
        engine.render(frame * 25_000, &mut universes);
    }

    let frames = 2000u64;
    let started = Instant::now();
    let mut sum = 0u64;

    for frame in 0..frames {
        engine.render(frame * 25_000, &mut universes);
        sum += universes.iter().map(|value| *value as u64).sum::<u64>();
    }

    let per_frame = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;

    println!(
        "{name}: {} parts, {} channels, {} universes | project {} KB, show {} KB, working memory {} KB | read {:.2} ms, compile {:.2} ms | frame {:.4} ms ({sum})",
        show.root.parts.count,
        show.root.total_channels,
        show.root.universes,
        json.len() / 1024,
        compiled.show.len() / 1024,
        show.header.memory_words as usize * 4 / 1024,
        median(parse_times),
        median(compile_times),
        per_frame,
    );
}

fn main() {
    measure("30 moving heads", heads(30));
    measure("60 moving heads", heads(60));
    measure("2048 pixels", matrices(16));
}
