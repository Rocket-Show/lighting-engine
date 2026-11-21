//! What the conformance cases don't play: the universes as they are sent, the master dimmers, the channels held by
//! the host, the scenes picked by hand, the tempo from outside, a show running for a month and a damaged show.

use std::path::PathBuf;

use engine_compiler::{compile_json, model, Compiled, Options};
use engine_core::format::checksum;
use engine_core::{Engine, Show, ShowError};
use serde_json::{json, Value};

fn compiled(mut project: Value) -> Compiled {
    let profiles = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/profiles");
    model::resolve_profiles(&mut project, &|profile| {
        std::fs::read_to_string(profiles.join(format!("{profile}.json"))).ok()
    })
    .expect("the profiles");
    compile_json(&project.to_string(), &Options::default()).expect("the project compiles")
}

fn par_project(steps: Value, effects: Value, scene: Value) -> Value {
    let mut scene = scene;
    scene["uuid"] = json!("scene");
    scene["presetUuids"] = json!(["preset"]);

    json!({
        "version": 11,
        "fixtureProfiles": [{ "$profile": "test/rgb-par" }, { "$profile": "test/rgb" }, { "$profile": "test/fine-dimmer" }],
        "fixtures": [
            { "uuid": "par", "profileUuid": "test/rgb-par", "modeShortName": "6ch", "dmxUniverseUuid": "a", "dmxFirstChannel": 10 },
            { "uuid": "rgb", "profileUuid": "test/rgb", "modeShortName": "4ch", "dmxUniverseUuid": "b", "dmxFirstChannel": 0 },
            { "uuid": "fine", "profileUuid": "test/fine-dimmer", "modeShortName": "4ch", "dmxUniverseUuid": "b", "dmxFirstChannel": 100 }
        ],
        "presetFixtures": [{ "fixtureUuid": "par" }, { "fixtureUuid": "rgb" }, { "fixtureUuid": "fine" }],
        "presets": [{
            "uuid": "preset",
            "fixtures": [{ "fixtureUuid": "par" }, { "fixtureUuid": "rgb" }, { "fixtureUuid": "fine" }],
            "steps": steps,
            "effects": effects
        }],
        "scenes": [scene]
    })
}

fn look(values: Value) -> Value {
    json!([{ "uuid": "step", "startMillis": 0, "fixtureCapabilityValues": values }])
}

struct Player<'a> {
    engine: Engine<'a>,
    universes: Vec<u8>,
}

impl<'a> Player<'a> {
    fn new(compiled: &'a Compiled, memory: &'a mut Vec<u32>) -> Player<'a> {
        let show = Show::check(&compiled.show).expect("the show passes its check");
        memory.resize(show.header.memory_words as usize, 0);
        let universes = vec![0; 512 * show.root.universes as usize];
        Player {
            engine: Engine::new(show, memory, 0).expect("the engine starts"),
            universes,
        }
    }

    /// the universes at a position of the preview, in ms
    fn at(&mut self, millis: i64) -> &[u8] {
        self.engine.set_position(0, millis * 1000, false);
        self.engine.render(0, &mut self.universes);
        &self.universes
    }

    /// the universes at a real time, in ms
    fn now(&mut self, millis: u64) -> &[u8] {
        self.engine.render(millis * 1000, &mut self.universes);
        &self.universes
    }
}

#[test]
fn the_universes_carry_each_channel_at_its_address() {
    let compiled = compiled(par_project(
        look(json!([
            { "type": "Intensity", "valuePercentage": 1 },
            { "type": "ColorIntensity", "color": "Red", "valuePercentage": 1 },
            { "type": "ColorIntensity", "color": "Green", "valuePercentage": 0.5 },
            { "type": "Zoom", "valuePercentage": 0.5 }
        ])),
        json!([]),
        json!({}),
    ));

    assert_eq!(compiled.index.universes, ["a", "b"]);

    let mut memory = Vec::new();
    let mut player = Player::new(&compiled, &mut memory);
    player.engine.play_preview(false);
    player.engine.select_scene(0, true);

    let universes = player.at(0).to_vec();

    // the par from its first channel on: dimmer, red, green, blue, an unused channel and the strobe at its default
    assert_eq!(universes[10..16], [255, 255, 127, 0, 0, 5]);
    assert!(universes[..10].iter().all(|value| *value == 0));

    // the fixture without a dimmer lights its colors through its virtual dimmer, which is not sent
    assert_eq!(universes[512..516], [255, 127, 0, 0]);

    // a channel with a fine channel is sent as its two bytes: half of 65535 is 127 and 255
    assert_eq!(universes[512 + 102..512 + 104], [127, 255]);

    // the master dimmer dims the dimmers, and the colors of a fixture with a virtual dimmer
    player.engine.set_master_dimmer(0.5);
    let universes = player.at(0).to_vec();
    assert_eq!(universes[10..14], [127, 255, 127, 0]);
    assert_eq!(universes[512..514], [127, 63]);

    player.engine.set_blackout(true);
    let universes = player.at(0).to_vec();
    assert_eq!(universes[10], 0);
    assert_eq!(universes[512..514], [0, 0]);
    player.engine.set_blackout(false);
    player.engine.set_master_dimmer(1.0);

    // a channel held by the host sends the value it is held at, until it is let go
    player.engine.set_override(0, 11, 7);
    player.engine.set_override(1, 300, 200);
    let universes = player.at(0).to_vec();
    assert_eq!((universes[11], universes[512 + 300]), (7, 200));
    player.engine.set_override(0, 11, -1);
    player.engine.clear_overrides();
    let universes = player.at(0).to_vec();
    assert_eq!((universes[11], universes[512 + 300]), (255, 0));

    // nothing plays: the fixtures stand at what their channels start out with (the dimmer of a fixture lighting in
    // colors only at full, its colors off)
    player.engine.stop();
    assert_eq!(player.at(0)[10..16], [255, 0, 0, 0, 0, 5]);
}

#[test]
fn a_host_taking_one_universe_gets_it_as_with_all_of_them() {
    // a chase over the fixtures of both universes, a curve on a color and a fan: where a fixture is among all of them
    // counts for what it does
    let compiled = compiled(par_project(
        look(json!([
            { "type": "Intensity", "valuePercentage": 0.6, "fan": [{ "position": 0, "valuePercentage": 0.2 }, { "position": 1, "valuePercentage": 1 }] },
            { "type": "ColorIntensity", "color": "Red", "valuePercentage": 1 },
            { "type": "ColorIntensity", "color": "Green", "valuePercentage": 0.5 }
        ])),
        json!([
            { "type": "curve", "uuid": "chase", "curveType": "sine", "lengthMillis": 700, "phasingMode": "spread", "capabilities": [{ "type": "Intensity" }] },
            { "type": "curve", "uuid": "green", "curveType": "triangle", "lengthMillis": 1300, "phasingMode": "spread", "capabilities": [{ "type": "ColorIntensity", "color": "Green" }] }
        ]),
        json!({}),
    ));

    assert_eq!(compiled.index.universes, ["a", "b"]);

    let (mut memory, mut memory_of_one) = (Vec::new(), Vec::new());
    let mut all = Player::new(&compiled, &mut memory);
    let mut one = Player::new(&compiled, &mut memory_of_one);

    // the host of the second one only has room for the first universe
    one.universes.truncate(512);

    for player in [&mut all, &mut one] {
        player.engine.play_preview(false);
        player.engine.select_scene(0, true);
    }

    let mut lit = false;

    for frame in 0..400 {
        let first = all.at(frame * 25)[..512].to_vec();

        assert_eq!(one.at(frame * 25), first, "frame {frame}");
        lit |= first.iter().any(|value| *value > 0);
    }

    assert!(lit);

    // the fixtures on the second universe were not worked out: their values are what they were
    let rgb = compiled
        .index
        .fixtures
        .iter()
        .position(|uuid| uuid == "rgb")
        .expect("the fixture") as u32;

    assert!((0..4).any(|channel| all.engine.value(rgb, channel).is_some_and(|value| value > 0.0)));
    assert!((0..4).all(|channel| one.engine.value(rgb, channel).map_or(true, |value| value == 0.0)));
}

#[test]
fn a_frame_is_the_same_a_month_later() {
    // a sine of 2.5 s on the dimmer and three steps of a second each, looping: both start over every 15 s
    let mut project = par_project(
        json!([
            { "uuid": "a", "startMillis": 0, "fixtureCapabilityValues": [{ "type": "ColorIntensity", "color": "Red", "valuePercentage": 0.1 }], "transitionMillis": 700, "transitionCurve": "ease-in-out" },
            { "uuid": "b", "startMillis": 1000, "fixtureCapabilityValues": [{ "type": "ColorIntensity", "color": "Red", "valuePercentage": 0.9 }], "transitionMillis": 700 },
            { "uuid": "c", "startMillis": 2000, "fixtureCapabilityValues": [{ "type": "ColorIntensity", "color": "Red", "valuePercentage": 0.4 }], "transitionMillis": 1000, "transitionCurve": "ease-in" }
        ]),
        json!([{ "type": "curve", "uuid": "curve", "curveType": "sine", "lengthMillis": 2500, "phasingMode": "spread", "capabilities": [{ "type": "Intensity" }, { "type": "ColorIntensity", "color": "Green" }, { "type": "Zoom" }] }]),
        json!({}),
    );
    project["presets"][0]["stepsLoop"] = json!(true);
    let compiled = compiled(project);

    let mut memory = Vec::new();
    let mut player = Player::new(&compiled, &mut memory);
    player.engine.play_preview(false);
    player.engine.select_scene(0, true);
    player.engine.set_step_preview(true, 0);

    let month = 30 * 24 * 3600 * 1000i64;
    assert_eq!(month % 15_000, 0);

    for millis in [0i64, 1, 333, 617, 999, 1000, 1001, 1777, 2499, 2500, 4096, 7123, 14_999] {
        player.at(millis);
        let (values, flags) = player.engine.values();
        let now = (values.to_vec(), flags.to_vec());

        player.at(millis + month);
        let (values, flags) = player.engine.values();
        assert_eq!(
            (values.to_vec(), flags.to_vec()),
            now,
            "the frame at {millis} ms and the one a month later"
        );

        // ... and a year of months later
        player.at(millis + 12 * month);
        assert_eq!(
            player.engine.values().0,
            now.0.as_slice(),
            "the frame at {millis} ms and the one a year later"
        );
    }
}

#[test]
fn scenes_picked_by_hand_fade_in_real_time() {
    // (the red of the par, on the channel after its dimmer)
    let red = json!([{ "type": "ColorIntensity", "color": "Red", "valuePercentage": 1 }]);
    let compiled = compiled(par_project(
        look(red),
        json!([]),
        json!({ "fadeInMillis": 1000, "fadeOutMillis": 1000 }),
    ));
    let mut memory = Vec::new();
    let mut player = Player::new(&compiled, &mut memory);

    player.engine.play_live(0);
    assert!(player.engine.live_idle(0));

    // picked: it fades in over a second
    player.engine.live_scene(0, 0, true);
    assert_eq!(player.now(0)[11], 0);
    assert_eq!(player.now(500)[11], 127);
    assert_eq!(player.now(1000)[11], 255);
    assert!(!player.engine.live_idle(1000));

    // the dimmer it is given by hand
    player.engine.live_dimmer(0, 0.5);
    assert_eq!(player.now(1500)[11], 127);
    player.engine.live_dimmer(0, 1.0);

    // let go: it fades out over a second, then nothing plays anymore
    player.engine.live_scene(2_000_000, 0, false);
    assert_eq!(player.now(2250)[11], 191);
    assert_eq!(player.now(2500)[11], 127);

    // picked again while it fades out: it fades in from where it is
    player.engine.live_scene(2_500_000, 0, true);
    assert_eq!(player.now(2500)[11], 127);
    assert_eq!(player.now(2750)[11], 191);
    assert_eq!(player.now(3000)[11], 255);

    player.engine.live_scene(4_000_000, 0, false);
    assert_eq!(player.now(5000)[11], 0);
    assert!(player.engine.live_idle(5_000_000));

    // flashed: on at once and off at once
    player.engine.live_flash(6_000_000, 0, true);
    assert_eq!(player.now(6000)[11], 255);
    player.engine.live_flash(6_100_000, 0, false);
    assert_eq!(player.now(6100)[11], 0);
}

#[test]
fn what_follows_the_beat_runs_on_the_tempo_from_outside() {
    // a sine of one beat on the dimmer: at its top a quarter of a beat in
    let curve = json!([{ "type": "curve", "uuid": "curve", "curveType": "sine", "lengthMode": "beats", "lengthBeats": 1, "capabilities": [{ "type": "ColorIntensity", "color": "Red" }] }]);
    let compiled = compiled(par_project(look(json!([])), curve, json!({})));

    // at the default tempo of 120 a beat lasts half a second
    let mut memory = Vec::new();
    let mut player = Player::new(&compiled, &mut memory);
    player.engine.play_live(0);
    player.engine.live_scene(0, 0, true);
    assert_eq!(player.now(125)[11], 255);
    assert_eq!(player.now(375)[11], 0);

    // at a tempo of 60 set from outside it lasts a second
    let mut memory = Vec::new();
    let mut player = Player::new(&compiled, &mut memory);
    player.engine.set_tempo(0, 60.0);
    player.engine.play_live(0);
    player.engine.live_scene(0, 0, true);
    assert_eq!(player.now(250)[11], 255);
    assert_eq!(player.now(750)[11], 0);

    // the tempo changes while it plays: the curve goes on from where it is, twice as fast
    player.engine.set_tempo(1_000_000, 120.0);
    assert_eq!(player.now(1125)[11], 255);
    assert_eq!(player.engine.beats(1_500_000), 2 << 32);

    // half as fast on the speed master
    player.engine.live_speed(1_500_000, 0.5);
    assert_eq!(player.now(1750)[11], 255);
}

#[test]
fn the_timeline_holds_and_goes_on() {
    let mut project = par_project(
        look(json!([])),
        json!([{ "type": "curve", "uuid": "curve", "curveType": "sawtooth", "lengthMillis": 4000, "capabilities": [{ "type": "ColorIntensity", "color": "Red" }] }]),
        json!({}),
    );
    project["compositions"] = json!([{
        "uuid": "composition",
        "name": "Song",
        "tracks": [{ "uuid": "track", "name": "Track" }],
        "scenePlaybackRegions": [{ "sceneUuid": "scene", "trackUuid": "track", "startMillis": 0, "endMillis": 10000 }],
        "markers": [{ "type": "hold", "positionMillis": 1000 }]
    }]);

    let compiled = compiled(project);
    let mut memory = Vec::new();
    let mut player = Player::new(&compiled, &mut memory);
    player
        .engine
        .play_timeline(compiled.index.composition_named("Song").expect("the composition"), 1);

    // a ramp over four seconds: a quarter up after a second
    player.engine.set_position(0, 0, true);
    assert_eq!(player.now(1000)[11], 63);

    // the timeline holds there: the position stands still, the clip runs on
    player.engine.set_position(1_000_000, 1_000_000, false);
    player.engine.hold(1_000_000, 1_000_000);
    assert!(player.engine.is_holding());
    assert_eq!(player.now(2000)[11], 127);

    // GO: it goes on from the hold point, the time held counting for the clip
    player.engine.go(2_000_000);
    player.engine.set_position(2_000_000, 1_000_000, true);
    assert_eq!(player.now(3000)[11], 191);

    // the position jumps back before the hold: it wasn't gone through anymore
    player.engine.forget_holds(0);
    player.engine.set_position(3_000_000, 1_000_000, false);
    assert_eq!(player.now(3000)[11], 63);
}

#[test]
fn a_show_carries_its_project_and_says_what_it_needs() {
    let project = par_project(
        look(json!([{ "type": "Intensity", "valuePercentage": 1 }])),
        json!([{ "type": "curve", "uuid": "curve", "capabilities": [{ "type": "Zoom" }] }]),
        json!({}),
    );
    let mut resolved = project.clone();
    let profiles = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/profiles");
    model::resolve_profiles(&mut resolved, &|profile| {
        std::fs::read_to_string(profiles.join(format!("{profile}.json"))).ok()
    })
    .expect("the profiles");
    let json = resolved.to_string();

    let with_project = compile_json(
        &json,
        &Options {
            project_json: Some(json.as_bytes()),
            ..Options::default()
        },
    )
    .expect("the project compiles");
    let without = compile_json(&json, &Options::default()).expect("the project compiles");
    assert_eq!(with_project.features, engine_core::format::feature::CURVES);
    assert!(with_project.show.len() >= without.show.len() + json.len());
    assert!(with_project
        .show
        .windows(json.len())
        .any(|window| window == json.as_bytes()));
    Show::check(&with_project.show).expect("a show with its project passes its check");

    // an engine which can't play curves is not given the show
    match compile_json(
        &json,
        &Options {
            features: 0,
            project_json: None,
        },
    ) {
        Err(engine_compiler::CompileError::Unsupported(missing)) => {
            assert_eq!(missing, engine_core::format::feature::CURVES)
        }
        other => panic!(
            "compiled for an engine without curves: {:?}",
            other.map(|compiled| compiled.features)
        ),
    }

    // too little working memory is refused before anything plays
    let show = Show::check(&without.show).expect("a show");
    let mut memory = vec![0u32; show.header.memory_words as usize - 1];
    assert!(
        matches!(Engine::new(show, &mut memory, 0), Err(ShowError::Memory(needed)) if needed == show.header.memory_words)
    );
}

/// a small generator of numbers which look random, the same on every run
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn a_damaged_show_is_refused_or_played_but_never_crashes() {
    let cases = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases/channel-values-scenarios.json");
    let file: Value = serde_json::from_str(&std::fs::read_to_string(cases).expect("the cases")).expect("JSON");
    let mut random = Random(0x2545_F491_4F6C_DD1D);
    let mut refused = 0;
    let mut played = 0;

    for case in file["cases"].as_array().expect("cases").iter().take(6) {
        let compiled = compiled(case["input"]["project"].clone());

        // cut off anywhere, the show is never taken
        for length in [
            0,
            3,
            4,
            20,
            47,
            48,
            60,
            compiled.show.len() / 2,
            compiled.show.len() - 1,
        ] {
            assert!(Show::check(&compiled.show[..length.min(compiled.show.len() - 1)]).is_err());
        }

        // a byte changed without the checksum is always noticed
        let mut damaged = compiled.show.clone();
        let at = 48 + random.next() as usize % (damaged.len() - 48);
        damaged[at] ^= 0x40;
        assert_eq!(Show::check(&damaged).err(), Some(ShowError::Checksum));

        // bytes changed by someone who fixes the checksum: refused by the check, or played without a crash
        for _ in 0..400 {
            let mut hostile = compiled.show.clone();

            for _ in 0..1 + random.next() % 3 {
                let at = 24 + random.next() as usize % (hostile.len() - 24);
                hostile[at] = match random.next() % 4 {
                    0 => 0xFF,
                    1 => 0,
                    2 => 0x80,
                    _ => random.next() as u8,
                };
            }

            let crc = checksum(&hostile);
            hostile[20..24].copy_from_slice(&crc.to_le_bytes());

            let show = match Show::check(&hostile) {
                Ok(show) => show,
                Err(_) => {
                    refused += 1;
                    continue;
                }
            };

            played += 1;

            // (numbers wrapping around are caught by a debug build, a release build plays on with whatever comes of
            // them: only a release build plays a show which was tampered with)
            if cfg!(debug_assertions) {
                continue;
            }

            let mut memory = vec![0u32; show.header.memory_words as usize];
            let mut universes = vec![0u8; 512 * show.root.universes as usize];
            let mut engine = Engine::new(show, &mut memory, 0).expect("the engine starts");

            for mode in 0..3 {
                match mode {
                    0 => {
                        engine.play_preview(false);

                        for scene in 0..show.root.scenes.count {
                            engine.select_scene(scene, true);
                        }
                    }
                    1 => engine.play_timeline(0, 1),
                    _ => {
                        engine.play_live(0);

                        for scene in 0..show.root.scenes.count {
                            engine.live_scene(0, scene, true);
                        }
                    }
                }

                for millis in [0u64, 250, 1000, 5000, 60_000] {
                    engine.set_position(millis * 1000, millis as i64 * 1000, false);
                    engine.render(millis * 1000, &mut universes);
                }
            }
        }
    }

    println!("{refused} damaged shows refused, {played} passed the check");
    assert!(refused > 0 && played > 0);
}
