#!/usr/bin/env bash
# Generate Swift + Kotlin bindings from the core's own bundled uniffi-bindgen.
# NEVER use a globally installed bindgen — the runtime/bindgen contract
# checksum must match, or apps abort at startup.
#
# --no-format IS LOAD-BEARING, not a speed optimisation. uniffi-bindgen
# post-formats its output with whatever ktlint / swift-format happens to be on
# PATH, so the bytes it emits depend on which tools a machine has and at which
# version. check-binding-drift.sh compares those bytes against git, so a
# developer with ktlint installed would regenerate formatted output and fail a
# gate that passes for everyone without it — a drift failure caused by the
# checker's environment rather than by any change to the bindings.
#
# Disabling formatting makes generation depend only on the pinned bindgen,
# which is the property the drift gate needs. The alternative — pinning a
# formatter version — requires installing that formatter on every machine that
# may run the gate, including CI runners that otherwise need no JVM.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --features uniffi -p neuralcompose-mobile-core

# The shared-library extension is platform-specific; CI runs this on Linux.
case "$(uname -s)" in
  Darwin) EXT=dylib ;;
  MINGW*|MSYS*|CYGWIN*) EXT=dll ;;
  *) EXT=so ;;
esac
LIB="target/debug/libneuralcompose_mobile_core.$EXT"
[ -f "$LIB" ] || LIB="target/debug/neuralcompose_mobile_core.$EXT"

cargo run --quiet --features uniffi -p neuralcompose-mobile-core --bin uniffi-bindgen -- \
  generate --no-format --library "$LIB" --language swift --out-dir ios/Generated

cargo run --quiet --features uniffi -p neuralcompose-mobile-core --bin uniffi-bindgen -- \
  generate --no-format --library "$LIB" --language kotlin --out-dir android/core/src/main/kotlin

echo "bindings: ios/Generated + android/core/src/main/kotlin"
