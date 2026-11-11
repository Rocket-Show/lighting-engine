// Plays the channelValues conformance cases through engine.wasm and compares the bits of every value of every frame
// with the digests recorded from the native build of the core (conformance/core-digests.json): the WebAssembly build
// has to come to the very same bits.
//
//   cargo build -p lighting-engine-wasm --target wasm32-unknown-unknown --release
//   node tools/conformance-wasm.mjs [path/to/engine.wasm]

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const wasmPath = process.argv[2] ?? path.join(root, 'target/wasm32-unknown-unknown/release/engine.wasm');
const conformance = path.join(root, 'conformance');

// no imports: the module needs nothing from its host
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const le = instance.exports;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

const bytes = (pointer, length) => new Uint8Array(le.memory.buffer, pointer, length);

function withBytes(data, use) {
  const pointer = le.le_alloc(data.length);
  bytes(pointer, data.length).set(data);

  try {
    return use(pointer, data.length);
  } finally {
    le.le_free(pointer, data.length);
  }
}

const micros = (millis) => BigInt(Math.round(millis * 1000));

function resolveProfiles(project) {
  const profiles = (project.fixtureProfiles ?? []).map((profile) => {
    if (!('$profile' in profile)) {
      return profile;
    }

    const loaded = JSON.parse(fs.readFileSync(path.join(conformance, 'profiles', profile.$profile + '.json'), 'utf8'));
    loaded.uuid = profile.$profile;
    return loaded;
  });

  return { ...project, fixtureProfiles: profiles };
}

class Digest {
  a = 0x811c9dc5;
  b = 0x01000193;

  byte(value) {
    this.a = Math.imul(this.a ^ value, 16777619) >>> 0;
    this.b = Math.imul(this.b ^ value, 16777619) >>> 0;
  }

  frame(engine) {
    const count = le.le_values_len(engine);
    const values = new Uint8Array(le.memory.buffer, le.le_values_ptr(engine), count * 4);
    const flags = new Uint32Array(le.memory.buffer, le.le_flags_ptr(engine), count);

    for (let index = 0; index < count; index++) {
      const set = flags[index] & 1;

      for (let byte = 0; byte < 4; byte++) {
        this.byte(set ? values[index * 4 + byte] : 0);
      }

      this.byte(set);
    }
  }

  text() {
    return this.a.toString(16).padStart(8, '0') + this.b.toString(16).padStart(8, '0');
  }
}

function play(show, index, run, audio, digest) {
  const engine = withBytes(show, (pointer, length) => le.le_engine_new(pointer, length, 0n));

  if (!engine) {
    throw new Error('the compiled show does not pass its check');
  }

  const preview = run.preview;

  if (preview.mode === 'timeline') {
    le.le_play_timeline(engine, index.compositions.findIndex((composition) => composition.name === preview.composition), 0);
  } else if (preview.mode === 'preset') {
    le.le_play_preview(engine, 1);
    const preset = preview.preset == null ? -1 : index.presets.indexOf(preview.preset);
    const step = preset < 0 || preview.step == null ? -1 : index.steps[preset].indexOf(preview.step);
    le.le_select_preset(engine, preset, step);

    if ('stepPreviewStartMillis' in preview) {
      le.le_set_step_preview(engine, 1, micros(preview.stepPreviewStartMillis));
    }
  } else {
    le.le_play_preview(engine, 0);

    for (const uuid of preview.scenes ?? []) {
      const scene = index.scenes.indexOf(uuid);

      if (scene >= 0) {
        le.le_select_scene(engine, scene, 1);
      }
    }
  }

  for (const hold of run.holds ?? []) {
    le.le_add_hold(engine, micros(hold.positionMillis), micros(hold.heldMillis));
  }

  for (const [list, call] of [['identify', le.le_identify], ['reset', le.le_reset_fixture], ['lampOn', le.le_lamp_on]]) {
    for (const uuid of preview[list] ?? []) {
      index.fixtures.forEach((candidate, fixture) => {
        if (candidate === uuid) {
          call(engine, fixture, 1);
        }
      });
    }
  }

  const frames = audio?.frames ?? [];
  const framePointer = le.le_alloc(21 * 4);

  for (const time of run.times) {
    const position = micros(time);
    const now = position < 0n ? 0n : position;

    if (frames.length > 0) {
      le.le_clear_audio(engine);
      const current = Number(now / 20000n);

      for (let frame = Math.max(current - 300, 0); frame <= current; frame++) {
        if (frame < frames.length) {
          new Float32Array(le.memory.buffer, framePointer, 21).set(frames[frame]);
          le.le_audio_frame(engine, BigInt(frame), framePointer, frames[frame].length);
        }
      }
    }

    le.le_set_position(engine, now, position, 0);
    le.le_render(engine, now);
    digest.frame(engine);
  }

  le.le_free(framePointer, 21 * 4);
  le.le_engine_free(engine);
}

const recorded = JSON.parse(fs.readFileSync(path.join(conformance, 'core-digests.json'), 'utf8'));
let failed = 0;

for (const [file, expected] of Object.entries(recorded)) {
  const cases = JSON.parse(fs.readFileSync(path.join(conformance, 'cases', file), 'utf8')).cases;
  const digest = new Digest();
  const started = performance.now();

  for (const conformanceCase of cases) {
    const project = encoder.encode(JSON.stringify(resolveProfiles(conformanceCase.input.project)));
    const status = withBytes(project, (pointer, length) => le.le_compile(pointer, length, le.le_features(), 0));

    if (status !== 0) {
      throw new Error(`${file} / ${conformanceCase.name}: ${decoder.decode(bytes(le.le_result_ptr(2), le.le_result_len(2)))}`);
    }

    const show = bytes(le.le_result_ptr(0), le.le_result_len(0)).slice();
    const index = JSON.parse(decoder.decode(bytes(le.le_result_ptr(1), le.le_result_len(1))));

    for (const run of conformanceCase.input.runs) {
      play(show, index, run, conformanceCase.input.audio, digest);
    }
  }

  const same = digest.text() === expected;
  failed += same ? 0 : 1;
  console.log(`${same ? 'ok  ' : 'FAIL'} ${file}: ${digest.text()}${same ? '' : ` (recorded ${expected})`} in ${Math.round(performance.now() - started)} ms`);
}

if (failed > 0) {
  console.error(`${failed} files come to other bits than the native build of the core`);
  process.exit(1);
}

console.log('engine.wasm comes to the very same bits as the native build of the core');
