#!/usr/bin/env bash
# Generated-binding drift guard: regenerate the UniFFI bindings and fail if
# the committed copies differ. A stale binding silently breaks the shells.
set -euo pipefail
cd "$(dirname "$0")/.."

before="$(mktemp -d)"
trap 'rm -rf "$before"' EXIT
cp ios/Generated/neuralcompose_mobile_core.swift "$before/" 2>/dev/null || true
cp android/core/src/main/kotlin/uniffi/neuralcompose_mobile_core/neuralcompose_mobile_core.kt \
   "$before/" 2>/dev/null || true

./scripts/gen-bindings.sh >/dev/null

diff -u "$before/neuralcompose_mobile_core.swift" ios/Generated/neuralcompose_mobile_core.swift
diff -u "$before/neuralcompose_mobile_core.kt" \
        android/core/src/main/kotlin/uniffi/neuralcompose_mobile_core/neuralcompose_mobile_core.kt
# The SwiftPM wrapper must carry the same generated file as ios/Generated.
diff -u ios/Generated/neuralcompose_mobile_core.swift \
        ios/NeuralComposeCore/Sources/NeuralComposeCore/neuralcompose_mobile_core.swift
echo "bindings: no drift"
