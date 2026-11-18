#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const modulePath = process.env.WASM_ERASURE_MODULE;
assert.equal(typeof modulePath, "string", "WASM_ERASURE_MODULE is required");
assert.notEqual(modulePath.length, 0, "WASM_ERASURE_MODULE must not be empty");
const module = await WebAssembly.compile(await readFile(modulePath));
assert.deepEqual(WebAssembly.Module.imports(module), [
  { module: "erasure_test", name: "observe_release", kind: "function" },
]);

const cases = Object.freeze([
  { name: "control: Vec cleanup without final slot wipe", releases: 1, dirty: 1 },
  { name: "control: ordinary String drop", releases: 1, dirty: 1 },
  { name: "Vec cleanup wipes post-drop slots and spare capacity", releases: 1, dirty: 0 },
  { name: "empty Vec cleanup wipes retained capacity", releases: 1, dirty: 0 },
  { name: "guarded list wipes its outgrown allocation", releases: 1, dirty: 0 },
  { name: "guarded list drop wipes values and spare slots", releases: 1, dirty: 0 },
  { name: "partial double-ended iteration wipes released list storage", releases: 1, dirty: 0 },
  { name: "guarded string drop wipes live bytes and spare capacity", releases: 1, dirty: 0 },
  { name: "nested guarded WKTs wipe key and value buffers", releases: 3, dirty: 0 },
  { name: "guarded WKT replacement wipes duplicate keys and old values", releases: 4, dirty: 0 },
  { name: "failed map value decode wipes the decoded guarded key", releases: 1, dirty: 0 },
]);

function instantiate() {
  let memory;
  const observations = [];
  const instance = new WebAssembly.Instance(module, {
    erasure_test: {
      observe_release(pointer, length, watch) {
        // Wasm i32 arguments arrive as signed JavaScript numbers. Normalize
        // their bits before checking the complete byte range against memory.
        assert.ok(Number.isInteger(pointer));
        assert.ok(Number.isInteger(length));
        assert.ok(Number.isInteger(watch));
        const offset = pointer >>> 0;
        const size = length >>> 0;
        const index = watch >>> 0;
        assert.ok(memory instanceof WebAssembly.Memory);
        assert.ok(size > 0);
        const end = offset + size;
        assert.ok(Number.isSafeInteger(end) && end <= memory.buffer.byteLength);
        assert.ok(index < cases.length);
        assert.ok(!observations.some((observation) => observation.watch === index));

        // Recreate the view on every callback: Wasm allocation may have grown
        // memory since the preceding observation. No freed-memory scan or
        // persistent typed-array alias is used, and no bytes are logged.
        let nonzero = 0;
        for (const byte of new Uint8Array(memory.buffer, offset, size)) {
          if (byte !== 0) nonzero += 1;
        }
        observations.push({ watch: index, length: size, nonzero });
        return nonzero;
      },
    },
  });
  memory = instance.exports.memory;
  assert.ok(memory instanceof WebAssembly.Memory);
  const version = instance.exports.erasure_abi_version;
  const count = instance.exports.erasure_case_count;
  const run = instance.exports.erasure_run_case;
  assert.equal(typeof version, "function");
  assert.equal(typeof count, "function");
  assert.equal(typeof run, "function");
  assert.equal(version(), 1);
  assert.equal(count(), cases.length, "every exported case must run");
  return { run, observations, memory };
}

for (const [index, scenario] of cases.entries()) {
  test(scenario.name, () => {
    // A fresh instance isolates both allocator watches and linear memory.
    const { run, observations } = instantiate();
    assert.equal(run(index), 0, "Rust fixture status must indicate success");
    assert.equal(observations.length, scenario.releases);
    assert.equal(
      observations.filter((observation) => observation.nonzero !== 0).length,
      scenario.dirty,
      "allocation residue must match the cleanup contract",
    );
  });
}

test("unknown case is rejected without observing memory", () => {
  const { run, observations } = instantiate();
  assert.equal(run(0xffff_ffff), 1);
  assert.deepEqual(observations, []);
  assert.equal(run(2), 0, "a rejected request must not poison the next case");
  assert.equal(observations.length, 1);
  assert.equal(observations[0].nonzero, 0);
});

test("observer reads the current linear memory buffer after growth", () => {
  const { run, observations, memory } = instantiate();
  const previous = memory.buffer;
  memory.grow(1);
  assert.notEqual(memory.buffer, previous);
  // Use the dirty control: a stale/detached view must never look like a wipe.
  assert.equal(run(1), 0);
  assert.equal(observations.length, 1);
  assert.ok(observations[0].nonzero > 0);
});
