#!/usr/bin/env bash
# Execute the browser-target module, including optimized cleanup code.
set -euo pipefail
if (( $# != 0 )); then
    printf '%s\n' 'usage: test-wasm-erasure.sh' >&2
    exit 2
fi
repo_root="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
erasure_target="${CARGO_TARGET_DIR:-$repo_root/target}/wasm-erasure"
for profile in debug release; do
    build_args=(--locked --package wasm-erasure-tests --features runtime-tests
        --target wasm32-unknown-unknown --target-dir "$erasure_target")
    if [[ "$profile" == release ]]; then
        build_args+=(--release)
    fi
    cargo build "${build_args[@]}"
    WASM_ERASURE_MODULE="$erasure_target/wasm32-unknown-unknown/$profile/wasm_erasure_tests.wasm" \
        node --test "$repo_root/scripts/wasm-erasure.test.mjs"
done
