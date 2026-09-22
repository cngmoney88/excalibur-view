#!/bin/sh
# Builds the plugin for WebAssembly. Signing happens on the publisher's PC:
#   python publish.py sign-plugin --key hyperview-signing.key \
#       --id mesafab-estimating --version <version> --name "Mesa Fab Estimating" \
#       mesafab_estimating.wasm
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/mesafab_estimating.wasm target/mesafab_estimating.wasm
ls -l target/mesafab_estimating.wasm
