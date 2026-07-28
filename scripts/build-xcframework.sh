#!/usr/bin/env bash
# Build the iOS XCFramework: device + simulator staticlibs, Swift bindings,
# module headers. Output: ios/Frameworks/NeuralComposeCoreFFI.xcframework and
# the Swift wrapper source in ios/Generated/.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --features uniffi -p neuralcompose-mobile-core --target aarch64-apple-ios
cargo build --release --features uniffi -p neuralcompose-mobile-core --target aarch64-apple-ios-sim

./scripts/gen-bindings.sh

# The generated modulemap declares module NeuralComposeCoreFFI; the .swift file
# expects to `import NeuralComposeCoreFFI` when built as a separate module.
HEADERS=ios/Generated/Headers
rm -rf "$HEADERS" ios/Frameworks
mkdir -p "$HEADERS" ios/Frameworks
cp ios/Generated/neuralcompose_mobile_coreFFI.h "$HEADERS/"
cp ios/Generated/neuralcompose_mobile_coreFFI.modulemap "$HEADERS/module.modulemap"

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libneuralcompose_mobile_core.a -headers "$HEADERS" \
  -library target/aarch64-apple-ios-sim/release/libneuralcompose_mobile_core.a -headers "$HEADERS" \
  -output ios/Frameworks/NeuralComposeCoreFFI.xcframework

echo "built ios/Frameworks/NeuralComposeCoreFFI.xcframework"
