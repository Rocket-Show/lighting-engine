//! `engine.wasm`: the core and the compiler behind one boundary, the same bytes for the browser and for the JVM.
//!
//! Only integers, single floats and byte ranges cross the boundary: no strings, no callbacks, no imports. A host
//! writes bytes into memory it got from `le_alloc` and reads results where `le_result_ptr` and the pointers of an
//! engine say. Times are whole microseconds (64-bit), an engine is named by the handle `le_engine_new` returns. One
//! call at a time per engine; several engines may live side by side.
//!
//! The same functions are there in a native build, where the tests call them.

use std::ffi::c_void;

use engine_compiler::{compile_json, CompileError, Index, Options};
use engine_core::format::{feature, BEHAVIOUR_VERSION, FORMAT_MAJOR, FORMAT_MINOR};
use engine_core::{Engine, Show, ShowError};

// ---- memory of the host ----

/// Memory for the host to write into (a project, a show, a frame of audio): `len` bytes, aligned for 64-bit numbers.
#[no_mangle]
pub extern "C" fn le_alloc(len: usize) -> *mut u8 {
    let words = len.div_ceil(8).max(1);
    let mut memory: Vec<u64> = vec![0; words];
    let pointer = memory.as_mut_ptr();
    std::mem::forget(memory);
    pointer as *mut u8
}

/// Give memory from `le_alloc` back.
///
/// # Safety
/// `pointer` has to come from `le_alloc(len)` and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn le_free(pointer: *mut u8, len: usize) {
    if !pointer.is_null() {
        let words = len.div_ceil(8).max(1);
        drop(Vec::from_raw_parts(pointer as *mut u64, words, words));
    }
}

unsafe fn bytes<'a>(pointer: *const u8, len: usize) -> &'a [u8] {
    if pointer.is_null() || len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(pointer, len)
    }
}

// ---- what this build is ----

/// The version of the compiled show format this engine reads: the major version times 65536 plus the minor one.
#[no_mangle]
pub extern "C" fn le_format_version() -> u32 {
    (FORMAT_MAJOR as u32) << 16 | FORMAT_MINOR as u32
}

/// Counts up with every change of the engine which changes the values it comes to.
#[no_mangle]
pub extern "C" fn le_behaviour_version() -> u32 {
    BEHAVIOUR_VERSION
}

/// The feature bits this engine plays.
#[no_mangle]
pub extern "C" fn le_features() -> u32 {
    feature::SUPPORTED
}

// ---- the compiler ----

/// What the last call of `le_compile` left: the show, the index (which uuid has which number, as JSON) and a message.
struct Results {
    show: Vec<u8>,
    index: Vec<u8>,
    message: Vec<u8>,
    features: u32,
}

static mut RESULTS: Results = Results {
    show: Vec::new(),
    index: Vec::new(),
    message: Vec::new(),
    features: 0,
};

#[allow(static_mut_refs)]
fn results() -> &'static mut Results {
    // (one call at a time: the boundary has no threads)
    unsafe { &mut RESULTS }
}

fn json_string(out: &mut String, text: &str) {
    out.push('"');

    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if (character as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", character as u32)),
            character => out.push(character),
        }
    }

    out.push('"');
}

fn json_strings(out: &mut String, items: &[String]) {
    out.push('[');

    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }

        json_string(out, item);
    }

    out.push(']');
}

/// The index of a compiled show as JSON: the uuids of the fixtures, the presets (with the ones of their steps), the
/// scenes, the compositions (with their names) and the universes, each at its number, and the parts of the fixtures
/// with the names of their channels.
pub fn index_json(index: &Index) -> String {
    let mut out = String::from("{\"fixtures\":");
    json_strings(&mut out, &index.fixtures);
    out.push_str(",\"parts\":[");

    for (number, part) in index.parts.iter().enumerate() {
        if number > 0 {
            out.push(',');
        }

        out.push_str(&format!("{{\"fixture\":{},\"pixelKey\":", part.fixture));

        match &part.pixel_key {
            Some(key) => json_string(&mut out, key),
            None => out.push_str("null"),
        }

        out.push_str(",\"channels\":");
        json_strings(&mut out, &part.channels);
        out.push('}');
    }

    out.push_str("],\"presets\":");
    json_strings(&mut out, &index.presets);
    out.push_str(",\"steps\":[");

    for (number, steps) in index.steps.iter().enumerate() {
        if number > 0 {
            out.push(',');
        }

        json_strings(&mut out, steps);
    }

    out.push_str("],\"scenes\":");
    json_strings(&mut out, &index.scenes);
    out.push_str(",\"compositions\":[");

    for (number, (uuid, name)) in index.compositions.iter().enumerate() {
        if number > 0 {
            out.push(',');
        }

        out.push_str("{\"uuid\":");
        json_string(&mut out, uuid);
        out.push_str(",\"name\":");
        json_string(&mut out, name);
        out.push('}');
    }

    out.push_str("],\"universes\":");
    json_strings(&mut out, &index.universes);
    out.push('}');
    out
}

/// Compile a project (its JSON, of the current version) for an engine with the passed feature bits. `keep_project`
/// carries the JSON in the show. Returns 0 for a show (see `le_result_ptr`), 1 for a project which can't be read, 2
/// for a show needing features the engine lacks (`le_result_features` says which) and 3 for one too large; the
/// message says more.
///
/// # Safety
/// `json` has to point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn le_compile(json: *const u8, len: usize, features: u32, keep_project: u32) -> u32 {
    let results = results();
    results.show.clear();
    results.index.clear();
    results.message.clear();
    results.features = 0;

    let source = bytes(json, len);

    let text = match std::str::from_utf8(source) {
        Ok(text) => text,
        Err(error) => {
            results.message = format!("the project is not UTF-8: {error}").into_bytes();
            return 1;
        }
    };

    let options = Options {
        features,
        project_json: if keep_project != 0 { Some(source) } else { None },
    };

    match compile_json(text, &options) {
        Ok(compiled) => {
            results.show = compiled.show;
            results.index = index_json(&compiled.index).into_bytes();
            results.message = compiled.warnings.join("\n").into_bytes();
            results.features = compiled.features;
            0
        }
        Err(error) => {
            results.message = error.to_string().into_bytes();

            match error {
                CompileError::Project(_) => 1,
                CompileError::Unsupported(missing) => {
                    results.features = missing;
                    2
                }
                CompileError::Limit(_) => 3,
            }
        }
    }
}

fn result(kind: u32) -> &'static [u8] {
    match kind {
        0 => &results().show,
        1 => &results().index,
        _ => &results().message,
    }
}

/// Where a result of the last `le_compile` is: 0 the show, 1 the index (JSON), 2 the message (text: the warnings of
/// a show, or why there is none). Valid until the next `le_compile`.
#[no_mangle]
pub extern "C" fn le_result_ptr(kind: u32) -> *const u8 {
    result(kind).as_ptr()
}

#[no_mangle]
pub extern "C" fn le_result_len(kind: u32) -> usize {
    result(kind).len()
}

/// The feature bits the last show compiled needs, or the ones the engine it was to be compiled for lacks.
#[no_mangle]
pub extern "C" fn le_result_features() -> u32 {
    results().features
}

// ---- an engine ----

fn error_code(error: ShowError) -> i32 {
    match error {
        ShowError::Magic => -1,
        ShowError::Version { .. } => -2,
        ShowError::Features(_) => -3,
        ShowError::Length => -4,
        ShowError::Checksum => -5,
        ShowError::Chunk => -6,
        ShowError::Corrupt => -7,
        ShowError::Memory(_) => -8,
    }
}

/// Validate a compiled show. Returns the working memory it needs in 32-bit words, or below 0 why it can't be played:
/// -1 not a show, -2 another major version, -3 features this engine lacks, -4 a wrong length, -5 a wrong checksum,
/// -6 a chunk missing, -7 damaged.
///
/// # Safety
/// `show` has to point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn le_check(show: *const u8, len: usize) -> i64 {
    match Show::check(bytes(show, len)) {
        Ok(show) => show.header.memory_words as i64,
        Err(error) => error_code(error) as i64,
    }
}

struct Instance {
    engine: Engine<'static>,
    universes: Vec<u8>,
    show: *mut [u8],
    memory: *mut [u32],
}

/// Make an engine for a compiled show (which is copied). Returns its handle, or null for a show which doesn't pass
/// `le_check`.
///
/// # Safety
/// `show` has to point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn le_engine_new(show: *const u8, len: usize, now_us: u64) -> *mut c_void {
    let show: &'static mut [u8] = Box::leak(bytes(show, len).to_vec().into_boxed_slice());
    let show_pointer: *mut [u8] = show;

    let checked = match Show::check(&*show_pointer) {
        Ok(checked) => checked,
        Err(_) => {
            drop(Box::from_raw(show_pointer));
            return std::ptr::null_mut();
        }
    };

    let memory: &'static mut [u32] = Box::leak(vec![0u32; checked.header.memory_words as usize].into_boxed_slice());
    let memory_pointer: *mut [u32] = memory;
    let universes = vec![0u8; 512 * checked.root.universes as usize];

    match Engine::new(checked, &mut *memory_pointer, now_us) {
        Ok(engine) => Box::into_raw(Box::new(Instance {
            engine,
            universes,
            show: show_pointer,
            memory: memory_pointer,
        })) as *mut c_void,
        Err(_) => {
            drop(Box::from_raw(memory_pointer));
            drop(Box::from_raw(show_pointer));
            std::ptr::null_mut()
        }
    }
}

/// Free an engine.
///
/// # Safety
/// `engine` has to come from `le_engine_new` and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn le_engine_free(engine: *mut c_void) {
    if engine.is_null() {
        return;
    }

    let instance = Box::from_raw(engine as *mut Instance);
    let (show, memory) = (instance.show, instance.memory);
    drop(instance);
    drop(Box::from_raw(memory));
    drop(Box::from_raw(show));
}

unsafe fn instance<'a>(engine: *mut c_void) -> Option<&'a mut Instance> {
    (engine as *mut Instance).as_mut()
}

macro_rules! engine_calls {
    ($($(#[$meta:meta])* fn $name:ident($($argument:ident: $type:ty),*) => |$engine:ident| $body:expr;)*) => {
        $(
            $(#[$meta])*
            ///
            /// # Safety
            /// `engine` has to be a handle of `le_engine_new`.
            #[no_mangle]
            pub unsafe extern "C" fn $name(engine: *mut c_void $(, $argument: $type)*) {
                if let Some(instance) = instance(engine) {
                    let $engine = &mut instance.engine;
                    $body;
                }
            }
        )*
    };
}

engine_calls! {
    /// Play the timeline of a composition: bit 0 of the flags as the show (every track), bit 1 as the composition played in between.
    fn le_play_timeline(composition: u32, flags: u32) => |engine| engine.play_timeline(composition, flags);
    /// Preview what is selected in the designer: the selected preset alone (solo), or else the selected scenes.
    fn le_play_preview(solo: u32) => |engine| engine.play_preview(solo != 0);
    /// Play the scenes picked by hand, from now on.
    fn le_play_live(now_us: u64) => |engine| engine.play_live(now_us);
    /// Play nothing.
    fn le_stop() => |engine| engine.stop();
    /// The preset selected in the designer and the step being edited in it (-1: none).
    fn le_select_preset(preset: i32, step: i32) => |engine| engine.select_preset(preset, step);
    /// Whether the steps of the previewed presets run (from a position on) or hold at the step being edited.
    fn le_set_step_preview(running: u32, start_us: i64) => |engine| engine.set_step_preview(running != 0, start_us);
    fn le_select_scene(scene: u32, selected: u32) => |engine| engine.select_scene(scene, selected != 0);
    fn le_clear_scenes() => |engine| engine.clear_scenes();
    /// Where the timeline (or the clock of the preview) is at a moment, and whether it runs on from there.
    fn le_set_position(now_us: u64, position_us: i64, running: u32) => |engine| engine.set_position(now_us, position_us, running != 0);
    /// The timeline holds at a hold point until `le_go`.
    fn le_hold(now_us: u64, position_us: i64) => |engine| engine.hold(now_us, position_us);
    fn le_go(now_us: u64) => |engine| engine.go(now_us);
    /// The position jumped: the holds from there on weren't gone through anymore.
    fn le_forget_holds(from_position_us: i64) => |engine| engine.forget_holds(from_position_us);
    /// A hold the timeline went through: its position and the time held there.
    fn le_add_hold(position_us: i64, held_us: i64) => |engine| engine.add_hold(position_us, held_us);
    fn le_set_holding(now_us: u64, holding: u32) => |engine| engine.set_holding(now_us, holding != 0);
    /// The tempo from outside in beats per minute (0 or less: none).
    fn le_set_tempo(now_us: u64, bpm: f32) => |engine| engine.set_tempo(now_us, bpm);
    /// A beat falls on a moment (a beat tapped).
    fn le_tap(now_us: u64) => |engine| engine.tap(now_us);
    /// The first beat of a bar falls on a moment.
    fn le_beat_one(now_us: u64, jump: u32) => |engine| engine.beat_one(now_us, jump != 0);
    fn le_set_beats(now_us: u64, beats: f32) => |engine| engine.set_beats(now_us, beats);
    /// A scene is picked by hand or let go.
    fn le_live_scene(now_us: u64, scene: u32, on: u32) => |engine| engine.live_scene(now_us, scene, on != 0);
    /// A scene is flashed or let go.
    fn le_live_flash(now_us: u64, scene: u32, on: u32) => |engine| engine.live_flash(now_us, scene, on != 0);
    fn le_live_dimmer(scene: u32, dimmer: f32) => |engine| engine.live_dimmer(scene, dimmer);
    fn le_live_speed(now_us: u64, speed: f32) => |engine| engine.live_speed(now_us, speed);
    fn le_live_reset() => |engine| engine.live_reset();
    fn le_set_master_dimmer(dimmer: f32) => |engine| engine.set_master_dimmer(dimmer);
    fn le_set_blackout(blackout: u32) => |engine| engine.set_blackout(blackout != 0);
    fn le_identify(fixture: u32, on: u32) => |engine| engine.identify(fixture, on != 0);
    fn le_reset_fixture(fixture: u32, on: u32) => |engine| engine.reset_fixture(fixture, on != 0);
    fn le_lamp_on(fixture: u32, on: u32) => |engine| engine.lamp_on(fixture, on != 0);
    /// Hold a DMX channel (from 0) of a universe at a value (below 0: let go).
    fn le_set_override(universe: u32, channel: u32, value: i32) => |engine| engine.set_override(universe, channel, value);
    fn le_clear_overrides() => |engine| engine.clear_overrides();
    fn le_clear_audio() => |engine| engine.clear_audio();
}

/// A frame of the analysis of the audio playing: `count` single floats (21: the RMS, the peak, the bass, the mids,
/// the highs and 16 bands, in dB), for the 20 ms from `index * 20 ms` of the position played.
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`, `values` has to point to `count` floats.
#[no_mangle]
pub unsafe extern "C" fn le_audio_frame(engine: *mut c_void, index: i64, values: *const f32, count: usize) {
    if let Some(instance) = instance(engine) {
        let values = if values.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(values, count)
        };
        instance.engine.audio_frame(index, values);
    }
}

/// The beats of the tempo playing at a moment, as a 32.32 fixed-point number.
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_beats(engine: *mut c_void, now_us: u64) -> i64 {
    instance(engine)
        .map(|instance| instance.engine.beats(now_us))
        .unwrap_or(0)
}

/// Whether no scene picked by hand plays anymore (1) or one still does (0).
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_live_idle(engine: *mut c_void, now_us: u64) -> u32 {
    instance(engine)
        .map(|instance| instance.engine.live_idle(now_us) as u32)
        .unwrap_or(1)
}

/// Work out a frame: the values of every channel of every part (see `le_values_ptr`) and the universes (see
/// `le_universes_ptr`) for a moment.
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_render(engine: *mut c_void, now_us: u64) {
    if let Some(instance) = instance(engine) {
        instance.engine.render(now_us, &mut instance.universes);
    }
}

/// The universes of the last frame: 512 bytes for each universe of the show, one after the other.
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_universes_ptr(engine: *mut c_void) -> *const u8 {
    instance(engine)
        .map(|instance| instance.universes.as_ptr())
        .unwrap_or(std::ptr::null())
}

/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_universes_len(engine: *mut c_void) -> usize {
    instance(engine).map(|instance| instance.universes.len()).unwrap_or(0)
}

/// The values of the last frame: a single float for each channel of each part, the parts one after the other in the
/// order of the index, their channels in the order of their names there.
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_values_ptr(engine: *mut c_void) -> *const f32 {
    instance(engine)
        .map(|instance| instance.engine.values().0.as_ptr() as *const f32)
        .unwrap_or(std::ptr::null())
}

/// For each value of `le_values_ptr` a 32-bit word: bit 0 says whether the channel has a value at all.
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_flags_ptr(engine: *mut c_void) -> *const u32 {
    instance(engine)
        .map(|instance| instance.engine.values().1.as_ptr())
        .unwrap_or(std::ptr::null())
}

/// The number of values of `le_values_ptr` (and of flags of `le_flags_ptr`).
///
/// # Safety
/// `engine` has to be a handle of `le_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn le_values_len(engine: *mut c_void) -> usize {
    instance(engine)
        .map(|instance| instance.engine.values().0.len())
        .unwrap_or(0)
}
