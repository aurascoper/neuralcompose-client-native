#!/usr/bin/env bash
# Generate Swift + Kotlin bindings from the core's own bundled uniffi-bindgen.
# NEVER use a globally installed bindgen — the runtime/bindgen contract
# checksum must match, or apps abort at startup.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --features uniffi -p neuralcompose-mobile-core

LIB=target/debug/libneuralcompose_mobile_core.dylib

cargo run --quiet --features uniffi -p neuralcompose-mobile-core --bin uniffi-bindgen -- \
  generate --library "$LIB" --language swift --out-dir ios/Generated

cargo run --quiet --features uniffi -p neuralcompose-mobile-core --bin uniffi-bindgen -- \
  generate --library "$LIB" --language kotlin --out-dir android/core/src/main/kotlin

echo "bindings: ios/Generated + android/core/src/main/kotlin"
