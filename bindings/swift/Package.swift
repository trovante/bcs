// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "Bcs",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .library(name: "Bcs", targets: ["Bcs"]),
        .executable(name: "BcsSelfTest", targets: ["BcsSelfTest"]),
    ],
    targets: [
        .target(
            name: "Bcs",
            path: "Sources/Bcs"
        ),
        .executableTarget(
            name: "BcsSelfTest",
            dependencies: ["Bcs"],
            path: "Sources/BcsSelfTest"
        ),
    ]
)
