#!/usr/bin/env bash
# Generated-binding drift guard: regenerate the UniFFI bindings and fail if
# the committed copies differ. A stale binding silently breaks the shells
# (the M5 UnsatisfiedLinkError lesson).
#
# ios/Generated/ is untracked build output; the tracked artifacts are the
# Kotlin binding and the SwiftPM wrapper copy, so git is the comparison.
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/gen-bindings.sh >/dev/null

# The wrapper copy is part of the generation contract, not an edit.
cp ios/Generated/neuralcompose_mobile_core.swift \
   ios/NeuralComposeCore/Sources/NeuralComposeCore/neuralcompose_mobile_core.swift

git --no-pager diff --exit-code -- \
  android/core/src/main/kotlin/uniffi/neuralcompose_mobile_core/neuralcompose_mobile_core.kt \
  ios/NeuralComposeCore/Sources/NeuralComposeCore/neuralcompose_mobile_core.swift

echo "bindings: no drift"
