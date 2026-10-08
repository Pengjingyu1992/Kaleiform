#!/bin/sh
# Builds the Desaturate example plug-in and refreshes the test fixture built from it.
# Needs the wasm32-unknown-unknown target: `rustup target add wasm32-unknown-unknown`.
set -eu
cd "$(dirname "$0")"
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$HOME=/build/home"
cargo build --release --locked --target wasm32-unknown-unknown
out="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/vectorcraft_plugin_desaturate.wasm"
cp "$out" ../tests/fixtures/desaturate.wasm
echo "built $out ($(wc -c < "$out") bytes) -> crates/plugins/tests/fixtures/desaturate.wasm"
