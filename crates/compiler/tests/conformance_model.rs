//! The conformance cases of the designer which only need the model and the profile helpers:
//! `profileNumber`, `defaultValue`, `hexColor`, `mixColors`, `pixels` and `pixelGroup`, run like
//! `DesignerConformanceTest` of Rocket Show runs them and compared exactly. The projects of the
//! `channelValues` cases and the profiles of the cases are read as well.

use std::fs;
use std::path::{Path, PathBuf};

use engine_compiler::model::{
    read_pixel_group, resolve_profiles, resolve_profiles_json, FixtureChannel, FixtureProfile, Json, JsonNumber,
    Project,
};
use engine_compiler::profile::{
    get_all_pixel_keys, get_default_value_by_channel, get_pixels_for_key_or_group, get_pixels_in_order, hex_to_rgb,
    mix_hex_colors, parse_number, resolve_group_pixels, Color, Pixel,
};

fn conformance() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn read_json(path: &Path) -> Json {
    Json::parse(&read(path)).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

// the files of the cases, sorted
fn case_files() -> Vec<PathBuf> {
    let directory = conformance().join("cases");
    let mut files: Vec<PathBuf> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.expect("an entry of the cases").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    files.sort();
    files
}

// the first difference between what the designer recorded and what the engine came to, None for
// none (`difference` of the conformance test of Java)
fn difference(expected: &Json, actual: &Json, path: &str) -> Option<String> {
    match (expected, actual) {
        // exactly the same number, the sign of a zero aside
        (Json::Number(expected_number), Json::Number(actual_number)) => (expected_number.as_f64()
            != actual_number.as_f64())
        .then(|| format!("{path}: expected {expected:?} but was {actual:?}")),
        (Json::Array(expected_items), Json::Array(actual_items)) => {
            if expected_items.len() != actual_items.len() {
                return Some(format!("{path}: expected {expected:?} but was {actual:?}"));
            }
            expected_items
                .iter()
                .zip(actual_items)
                .enumerate()
                .find_map(|(index, (expected_item, actual_item))| {
                    difference(expected_item, actual_item, &format!("{path}[{index}]"))
                })
        }
        (Json::Object(_), Json::Object(_)) => {
            let mut names: Vec<&str> = expected
                .properties()
                .into_iter()
                .chain(actual.properties())
                .map(|(name, _)| name)
                .collect();
            names.sort();
            names.dedup();
            names
                .into_iter()
                .find_map(|name| match (expected.get(name), actual.get(name)) {
                    (Some(expected_property), Some(actual_property)) => {
                        difference(expected_property, actual_property, &format!("{path}.{name}"))
                    }
                    (expected_property, actual_property) => Some(format!(
                        "{path}.{name}: expected {expected_property:?} but was {actual_property:?}"
                    )),
                })
        }
        _ => (expected != actual).then(|| format!("{path}: expected {expected:?} but was {actual:?}")),
    }
}

// runs all cases of a kind and tells how many there were
fn run_kind(kind: &str, run: impl Fn(&Json) -> Json) -> usize {
    let mut count = 0;
    let mut failures = Vec::new();
    for file in case_files() {
        let case_file = read_json(&file);
        if case_file.get("kind").and_then(Json::as_str) != Some(kind) {
            continue;
        }
        for case in case_file.get("cases").and_then(Json::as_array).expect("the cases") {
            let name = case.get("name").and_then(Json::as_str).expect("the name of a case");
            let expected = case
                .get("expected")
                .unwrap_or_else(|| panic!("{name}: not recorded yet"));
            let actual = run(case.get("input").expect("the input of a case"));
            count += 1;
            if let Some(difference) = difference(expected, &actual, "") {
                failures.push(format!(
                    "{} / {name}{difference}",
                    file.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {count} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(count > 0, "no cases of the kind {kind}");
    count
}

fn text(json: Option<&Json>) -> String {
    json.expect("a text").as_text().expect("a text")
}

fn strings(json: Option<&Json>) -> Vec<String> {
    json.and_then(Json::as_array)
        .expect("a list")
        .iter()
        .map(|item| item.as_text().expect("a text"))
        .collect()
}

fn number(value: Option<f64>) -> Json {
    value.map_or(Json::Null, Json::number)
}

fn color(color: Option<Color>) -> Json {
    color.map_or(Json::Null, |color| {
        Json::Array(vec![
            Json::number(color.red),
            Json::number(color.green),
            Json::number(color.blue),
        ])
    })
}

fn whole(value: i32) -> Json {
    Json::Number(JsonNumber::from_i64(value as i64))
}

fn keys(pixels: &[Pixel]) -> Json {
    Json::Array(pixels.iter().map(|pixel| Json::string(pixel.key.clone())).collect())
}

#[test]
fn profile_number() {
    run_kind("profileNumber", |input| number(parse_number(&text(input.get("text")))));
}

#[test]
fn default_value() {
    run_kind("defaultValue", |input| {
        let channel = FixtureChannel::from_json(input.get("channel").expect("the channel")).expect("a channel");
        number(get_default_value_by_channel(&channel))
    });
}

#[test]
fn hex_color() {
    run_kind("hexColor", |input| color(hex_to_rgb(&text(input.get("text")))));
}

#[test]
fn mix_colors() {
    run_kind("mixColors", |input| {
        color(mix_hex_colors(&strings(input.get("colors"))))
    });
}

#[test]
fn pixels() {
    run_kind("pixels", |input| {
        let profile_json = Json::Object(vec![(
            "matrix".to_string(),
            input.get("matrix").expect("the matrix").clone(),
        )]);
        let profile = FixtureProfile::from_json(&profile_json).expect("a profile");
        let matrix = profile.matrix.as_ref().expect("the matrix");
        let all_pixels = get_all_pixel_keys(&profile);

        let pixels = all_pixels
            .iter()
            .map(|pixel| {
                Json::Array(vec![
                    Json::string(pixel.key.clone()),
                    whole(pixel.x),
                    whole(pixel.y),
                    whole(pixel.z),
                ])
            })
            .collect();

        let groups = matrix
            .pixel_groups
            .iter()
            .flatten()
            .map(|group| {
                let group_pixels =
                    get_pixels_for_key_or_group(Some(matrix), &all_pixels, &group.name).expect("the pixels of a group");
                (group.name.clone(), keys(&group_pixels))
            })
            .collect();

        let repeat_for = input
            .get("repeatFor")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .map(|entries| keys(&get_pixels_in_order(&profile, &strings(Some(entries)))))
            .collect();

        Json::Object(vec![
            ("pixels".to_string(), Json::Array(pixels)),
            ("groups".to_string(), Json::Object(groups)),
            ("repeatFor".to_string(), Json::Array(repeat_for)),
        ])
    });
}

#[test]
fn pixel_group() {
    run_kind("pixelGroup", |input| {
        let group = read_pixel_group("group", input.get("group")).expect("a group");
        let whole_of = |json: &Json| json.as_f64().expect("a position") as i32;
        let pixels: Vec<Pixel> = input
            .get("pixels")
            .and_then(Json::as_array)
            .expect("the pixels")
            .iter()
            .map(|pixel| {
                let pixel = pixel.as_array().expect("a pixel");
                Pixel::new(
                    text(pixel.first()),
                    whole_of(&pixel[1]),
                    whole_of(&pixel[2]),
                    whole_of(&pixel[3]),
                )
            })
            .collect();
        match resolve_group_pixels(Some(&group), &pixels) {
            Ok(pixels) => keys(&pixels),
            // a rule which can't be read stops the whole group on both sides
            Err(_) => Json::string("error"),
        }
    });
}

fn load_profile(name: &str) -> Option<String> {
    fs::read_to_string(conformance().join("profiles").join(format!("{name}.json"))).ok()
}

// all files below a directory with the passed extension
fn files_below(directory: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap_or_else(|error| panic!("{}: {error}", directory.display())) {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            files_below(&path, extension, files);
        } else if path.extension().is_some_and(|found| found == extension) {
            files.push(path);
        }
    }
}

#[test]
fn reads_the_projects_of_the_channel_values() {
    let mut projects = 0;
    let mut profiles = 0;
    for file in case_files() {
        let file_name = file.file_name().unwrap().to_string_lossy().to_string();
        if !file_name.starts_with("channel-values-") {
            continue;
        }
        let case_file = read_json(&file);
        assert_eq!(
            case_file.get("kind").and_then(Json::as_str),
            Some("channelValues"),
            "{file_name}"
        );
        for case in case_file.get("cases").and_then(Json::as_array).expect("the cases") {
            let name = case.get("name").and_then(Json::as_str).expect("the name of a case");
            let mut project_json = case
                .get("input")
                .and_then(|input| input.get("project"))
                .expect("the project")
                .clone();
            resolve_profiles_json(&mut project_json, &load_profile)
                .unwrap_or_else(|error| panic!("{file_name} / {name}: {error}"));
            let project =
                Project::from_json(&project_json).unwrap_or_else(|error| panic!("{file_name} / {name}: {error}"));
            assert_eq!(project.version, 11, "{file_name} / {name}");
            for profile in project.fixture_profiles.iter().flatten() {
                assert!(profile.uuid.is_some(), "{file_name} / {name}: a profile without a uuid");
                profiles += 1;
            }
            projects += 1;
        }
    }
    assert!(projects > 100, "only {projects} projects");
    assert!(profiles > 100, "only {profiles} profiles");
}

#[test]
fn reads_the_projects_through_serde_json() {
    // the same projects through a value of serde_json: the same model, but for the order of the
    // names (which serde_json doesn't keep) and for the last digit of some numbers
    let file = conformance().join("cases").join("channel-values-library.json");
    let text = read(&file);
    let case_file: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let own_case_file = read_json(&file);
    let cases = case_file["cases"].as_array().expect("the cases");
    let own_cases = own_case_file.get("cases").and_then(Json::as_array).expect("the cases");
    assert_eq!(cases.len(), own_cases.len());
    for (case, own_case) in cases.iter().zip(own_cases) {
        let mut value = case["input"]["project"].clone();
        resolve_profiles(&mut value, &load_profile).expect("the profiles");
        let project = Project::from_json(&Json::from(&value)).expect("a project");

        let mut own_json = own_case
            .get("input")
            .and_then(|input| input.get("project"))
            .expect("the project")
            .clone();
        resolve_profiles_json(&mut own_json, &load_profile).expect("the profiles");
        let own_project = Project::from_json(&own_json).expect("a project");

        assert_eq!(project.fixtures, own_project.fixtures);
        assert_eq!(project.scenes, own_project.scenes);
        let profile_uuids = |project: &Project| {
            project
                .fixture_profiles
                .iter()
                .flatten()
                .map(|profile| profile.uuid.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(profile_uuids(&project), profile_uuids(&own_project));
    }
}

#[test]
fn reads_the_profiles() {
    let mut files = Vec::new();
    files_below(&conformance().join("profiles"), "json", &mut files);
    assert!(files.len() >= 20, "only {} profiles", files.len());
    for file in files {
        let text = read(&file);
        let profile =
            engine_compiler::model::parse_profile(&text).unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        let modes = profile
            .modes
            .as_ref()
            .unwrap_or_else(|| panic!("{}: no modes", file.display()));
        assert!(!modes.is_empty(), "{}: no modes", file.display());
        // the channels come in the order they are written in
        let json = Json::parse(&text).expect("JSON");
        for (property, channels) in [
            ("availableChannels", &profile.available_channels),
            ("templateChannels", &profile.template_channels),
        ] {
            let written: Vec<&str> = json
                .get(property)
                .map(|channels| channels.properties().into_iter().map(|(name, _)| name).collect())
                .unwrap_or_default();
            assert_eq!(
                channels.keys().collect::<Vec<_>>(),
                written,
                "{}: {property}",
                file.display()
            );
        }
    }
}
