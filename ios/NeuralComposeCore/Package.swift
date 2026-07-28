// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "NeuralComposeCore",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "NeuralComposeCore", targets: ["NeuralComposeCore"])
    ],
    targets: [
        // Built by scripts/build-xcframework.sh (device + simulator slices).
        .binaryTarget(
            name: "NeuralComposeCoreFFI",
            path: "../Frameworks/NeuralComposeCoreFFI.xcframework"
        ),
        // UniFFI-generated Swift wrapper (scripts/gen-bindings.sh output,
        // copied from ios/Generated/).
        .target(
            name: "NeuralComposeCore",
            dependencies: ["NeuralComposeCoreFFI"],
            path: "Sources/NeuralComposeCore"
        ),
        .testTarget(
            name: "NeuralComposeCoreTests",
            dependencies: ["NeuralComposeCore"],
            path: "Tests"
        ),
    ]
)
