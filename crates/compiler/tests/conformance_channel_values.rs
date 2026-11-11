//! The `channelValues` conformance cases: a project and how it is played in, the value of every channel of every
//! fixture and pixel at each time out, compared with what the designer recorded (see `conformance/README.md`).
//!
//! The recorded values come from double precision, the engine computes in single precision: a value may differ by
//! up to 1 (of 255, or of 65535 for a channel with a fine channel), which channel has a value at all must match.

use std::collections::BTreeMap;
use std::path::PathBuf;

use engine_compiler::model;
use engine_compiler::{compile, Compiled, Options};
use engine_core::{Engine, Show};
use serde_json::Value;

/// how far a value may be from the recorded one
const TOLERANCE: f64 = 1.0;

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

fn micros(millis: f64) -> i64 {
    (millis * 1000.0).round() as i64
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The differences from the recorded values which are explained (see `conformance/DIFFERENCES.md`): the case, the
/// fixtures or pixels and why. Each of them sits exactly on an edge where the recorded value itself is decided by the
/// rounding of a double.
const EXPLAINED: &[(&str, &[&str], &str)] = &[
    (
        "white and amber from the colors, and the color temperature as a color",
        &["rgbwa"],
        "the green left over once the amber LED took its part is 0: a hair below 0 in doubles, which Rocket Show takes as no value",
    ),
    ("comet-2", &["matrix-groups/Top", "matrix-groups/Corners"], "the place in the palette is exactly on the border of two stepped stops"),
    ("image-1", &["matrix/4", "matrix/8", "matrix/12"], "the scrolled image is exactly on the border of two of its pixels"),
    (
        "meter-1",
        &["bar-1/1", "bar-1/3", "bar-2/1", "bar-2/3", "band-thirds/1/3", "band-thirds/3/3"],
        "the place in the palette is exactly on the border of two stepped stops",
    ),
    (
        "scope-1",
        &["bar-1/1", "bar-1/3", "bar-2/1", "bar-2/3", "band-thirds/1/3", "band-thirds/3/3"],
        "the pixel is exactly on the border of two frames of the wave",
    ),
];

fn explained(case: &str, key: &str) -> bool {
    EXPLAINED
        .iter()
        .any(|(explained_case, keys, _)| *explained_case == case && keys.contains(&key))
}

struct Difference {
    place: String,
    expected: Option<f64>,
    actual: Option<f64>,
}

/// A digest of all values of all frames, to compare the builds of the core bit for bit: two 32-bit FNV-1a hashes
/// with different starts over the bits of each value (0 for a channel without one) and whether it has one.
#[derive(Clone, Copy)]
struct Digest(u32, u32);

impl Digest {
    fn new() -> Self {
        Digest(0x811c_9dc5, 0x0100_0193)
    }

    fn byte(&mut self, byte: u8) {
        self.0 = (self.0 ^ byte as u32).wrapping_mul(16_777_619);
        self.1 = (self.1 ^ byte as u32).wrapping_mul(16_777_619);
    }

    fn frame(&mut self, engine: &Engine) {
        let (values, flags) = engine.values();

        for (value, flag) in values.iter().zip(flags) {
            let set = flag & 1;

            for byte in (if set != 0 { *value } else { 0 }).to_le_bytes() {
                self.byte(byte);
            }

            self.byte(set as u8);
        }
    }

    fn text(&self) -> String {
        format!("{:08x}{:08x}", self.0, self.1)
    }
}

#[derive(Default)]
struct Report {
    frames: usize,
    values: usize,
    differences: Vec<Difference>,
    /// the differences which are explained
    explained: usize,
    largest: f64,
}

/// Play a run of a case and compare each frame.
fn play(
    compiled: &Compiled,
    case: &str,
    run_index: usize,
    run: &Value,
    audio: Option<&Value>,
    expected: &Value,
    report: &mut Report,
    digest: &mut Digest,
) {
    let show = Show::check(&compiled.show)
        .unwrap_or_else(|error| panic!("{case}: the compiled show doesn't pass its check: {error:?}"));
    let mut memory = vec![0u32; show.header.memory_words as usize];
    let mut engine = Engine::new(show, &mut memory, 0).expect("the engine starts");
    let index = &compiled.index;
    let preview = &run["preview"];

    match preview["mode"].as_str().unwrap_or("") {
        "timeline" => {
            let composition = index
                .composition_named(preview["composition"].as_str().unwrap_or(""))
                .expect("the composition of the case");
            engine.play_timeline(composition, 0);
        }
        "preset" => {
            engine.play_preview(true);
            let preset = preview["preset"].as_str().and_then(|uuid| index.preset(uuid));
            let step = match (preset, preview["step"].as_str()) {
                (Some(preset), Some(uuid)) => index.step(preset, uuid),
                _ => None,
            };

            engine.select_preset(
                preset.map(|preset| preset as i32).unwrap_or(-1),
                step.map(|step| step as i32).unwrap_or(-1),
            );

            if let Some(start) = preview.get("stepPreviewStartMillis").and_then(Value::as_f64) {
                engine.set_step_preview(true, micros(start));
            }
        }
        "scenes" => {
            engine.play_preview(false);

            for uuid in strings(preview.get("scenes")) {
                if let Some(scene) = index.scene(&uuid) {
                    engine.select_scene(scene, true);
                }
            }
        }
        mode => panic!("{case}: unknown preview mode {mode}"),
    }

    // the holds the timeline went through, if any
    if let Some(holds) = run.get("holds").and_then(Value::as_array) {
        for hold in holds {
            engine.add_hold(
                micros(hold["positionMillis"].as_f64().unwrap_or(0.0)),
                micros(hold["heldMillis"].as_f64().unwrap_or(0.0)),
            );
        }
    }

    for (list, kind) in [("identify", 0), ("reset", 1), ("lampOn", 2)] {
        for uuid in strings(preview.get(list)) {
            // (every fixture with that uuid)
            for (fixture, candidate) in index.fixtures.iter().enumerate() {
                if *candidate == uuid {
                    match kind {
                        0 => engine.identify(fixture as u32, true),
                        1 => engine.reset_fixture(fixture as u32, true),
                        _ => engine.lamp_on(fixture as u32, true),
                    }
                }
            }
        }
    }

    let frames: Vec<Vec<f32>> = audio
        .and_then(|audio| audio.get("frames"))
        .and_then(Value::as_array)
        .map(|frames| {
            frames
                .iter()
                .map(|frame| {
                    frame
                        .as_array()
                        .map(|values| {
                            values
                                .iter()
                                .map(|value| value.as_f64().unwrap_or(-100.0) as f32)
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();

    let mut universes = vec![0u8; 512 * index.universes.len().max(1)];
    let times = run["times"].as_array().expect("the times of the run");

    for (frame_index, time) in times.iter().enumerate() {
        let position = micros(time.as_f64().unwrap_or(0.0));

        if !frames.is_empty() {
            // the frames of the audio up to the moment, the ones the engine keeps
            engine.clear_audio();
            let current = position.max(0) / 20_000;

            for frame in (current - 300).max(0)..=current {
                if let Some(values) = frames.get(frame as usize) {
                    engine.audio_frame(frame, values);
                }
            }
        }

        engine.set_position(position.max(0) as u64, position, false);
        engine.render(position.max(0) as u64, &mut universes);
        digest.frame(&engine);
        report.frames += 1;

        let expected_frame = expected[run_index][frame_index]
            .as_object()
            .unwrap_or_else(|| panic!("{case}: no recorded frame {run_index}/{frame_index}"));
        let mut seen = 0;

        for (part_index, part) in index.parts.iter().enumerate() {
            let fixture = &index.fixtures[part.fixture as usize];
            let key = match &part.pixel_key {
                Some(pixel) => format!("{fixture}/{pixel}"),
                None => fixture.clone(),
            };

            let expected_part = match expected_frame.get(&key).and_then(Value::as_object) {
                Some(values) => values,
                None => {
                    report.differences.push(Difference {
                        place: format!("run {run_index} frame {frame_index} {key}: not recorded"),
                        expected: None,
                        actual: None,
                    });
                    continue;
                }
            };

            seen += 1;
            let mut actual_part: BTreeMap<&str, f64> = BTreeMap::new();

            for (channel_index, name) in part.channels.iter().enumerate() {
                if let Some(value) = engine.value(part_index as u32, channel_index as u32) {
                    // (the first channel of a name is the one found by the name)
                    actual_part.entry(name.as_str()).or_insert(value as f64);
                }
            }

            for (name, expected_value) in expected_part {
                report.values += 1;
                let expected_value = expected_value.as_f64();
                let actual_value = actual_part.remove(name.as_str());

                let difference = match (expected_value, actual_value) {
                    (Some(expected), Some(actual)) => (expected - actual).abs(),
                    _ => f64::INFINITY,
                };

                if !(difference <= TOLERANCE) && explained(case, &key) {
                    report.explained += 1;
                    continue;
                }

                if difference.is_finite() {
                    report.largest = report.largest.max(difference);
                }

                if !(difference <= TOLERANCE) {
                    report.differences.push(Difference {
                        place: format!("run {run_index} frame {frame_index} (t={time}) {key} {name}"),
                        expected: expected_value,
                        actual: actual_value,
                    });
                }
            }

            for (name, actual_value) in actual_part {
                if explained(case, &key) {
                    report.explained += 1;
                    continue;
                }

                report.differences.push(Difference {
                    place: format!("run {run_index} frame {frame_index} (t={time}) {key} {name}"),
                    expected: None,
                    actual: Some(actual_value),
                });
            }
        }

        // (parts sharing a key are recorded once)
        let _ = seen;
    }
}

fn run_file(name: &str) -> Digest {
    let path = conformance().join("cases").join(name);
    let file: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .expect("the cases are JSON");
    assert_eq!(file["kind"], "channelValues");

    let only = std::env::var("CASE").ok();
    let mut failed = 0;
    let mut total = Report::default();
    let mut digest = Digest::new();

    for case in file["cases"].as_array().expect("cases") {
        let case_name = case["name"].as_str().unwrap_or("?");

        if only.as_deref().map(|only| !case_name.contains(only)).unwrap_or(false) {
            continue;
        }

        let input = &case["input"];
        let mut project = input["project"].clone();

        model::resolve_profiles(&mut project, &|profile| {
            std::fs::read_to_string(conformance().join("profiles").join(format!("{profile}.json"))).ok()
        })
        .expect("the profiles of the case");

        let project = model::parse_project(&project.to_string()).unwrap_or_else(|error| panic!("{case_name}: {error}"));
        let compiled = compile(&project, &Options::default()).unwrap_or_else(|error| panic!("{case_name}: {error}"));
        let mut report = Report::default();

        for (run_index, run) in input["runs"].as_array().expect("runs").iter().enumerate() {
            play(
                &compiled,
                case_name,
                run_index,
                run,
                input.get("audio"),
                &case["expected"],
                &mut report,
                &mut digest,
            );
        }

        total.frames += report.frames;
        total.values += report.values;
        total.explained += report.explained;

        // (an explanation nothing needs anymore has to go)
        if EXPLAINED
            .iter()
            .any(|(explained_case, _, _)| *explained_case == case_name)
        {
            assert!(
                report.explained > 0,
                "{case_name}: nothing differs anymore, remove it from the explained differences"
            );
        }

        total.largest = total.largest.max(report.largest);

        if !report.differences.is_empty() {
            failed += 1;
            println!(
                "{name} / {case_name}: {} of {} values differ (warnings: {:?})",
                report.differences.len(),
                report.values,
                compiled.warnings
            );

            for difference in report.differences.iter().take(
                std::env::var("SHOW")
                    .ok()
                    .and_then(|show| show.parse().ok())
                    .unwrap_or(6),
            ) {
                println!(
                    "    {}: expected {:?}, was {:?}",
                    difference.place, difference.expected, difference.actual
                );
            }
        }
    }

    println!(
        "{name}: {} frames, {} values, {} explained differences, largest other difference {:.6}",
        total.frames, total.values, total.explained, total.largest
    );
    assert_eq!(failed, 0, "{name}: {failed} cases differ from the recorded values");
    digest
}

macro_rules! files {
    ($($test:ident => $file:literal),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                run_file($file);
            }
        )*

        const FILES: &[&str] = &[$($file),*];
    };
}

/// Every build of the core has to come to the very same bits: the digests of all values of all frames are recorded
/// in `conformance/core-digests.json` (from the native build, with `RECORD=1`) and compared by each build, here and
/// through `engine.wasm` (see `tools/conformance-wasm.mjs`).
#[test]
fn the_bits_of_all_frames_are_the_recorded_ones() {
    if std::env::var("CASE").is_ok() {
        return;
    }

    let path = conformance().join("core-digests.json");
    let mut text = String::from("{\n");

    for (index, file) in FILES.iter().enumerate() {
        text.push_str(&format!(
            "  \"{file}\": \"{}\"{}\n",
            run_file(file).text(),
            if index + 1 < FILES.len() { "," } else { "" }
        ));
    }

    text.push_str("}\n");

    if std::env::var("RECORD").is_ok() {
        std::fs::write(&path, &text).expect("the digests are written");
        return;
    }

    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(recorded, text, "the core comes to other bits than recorded: if that is meant, bump BEHAVIOUR_VERSION and record them anew with RECORD=1");
}

files! {
    attributes => "channel-values-attributes.json",
    capabilities => "channel-values-capabilities.json",
    color_dimmers => "channel-values-color-dimmers.json",
    edges => "channel-values-edges.json",
    fans => "channel-values-fans.json",
    library => "channel-values-library.json",
    mixing_edges => "channel-values-mixing-edges.json",
    movements => "channel-values-movements.json",
    pixel_audio => "channel-values-pixel-audio.json",
    pixel_effects => "channel-values-pixel-effects.json",
    pixel_stage => "channel-values-pixel-stage.json",
    pixel_white => "channel-values-pixel-white.json",
    scenarios => "channel-values-scenarios.json",
    show => "channel-values-show.json",
    timing => "channel-values-timing.json",
}
