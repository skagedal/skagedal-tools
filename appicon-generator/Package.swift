// swift-tools-version:6.2

import PackageDescription

let package = Package(
    name: "appicon-generator",
    platforms: [
        // NSBitmapImageRep and Core Text do the drawing, so this is macOS-only.
        .macOS(.v14)
    ],
    products: [
        .executable(name: "appicon-generator", targets: ["appicon-generator"]),
        .library(name: "AppIconKit", targets: ["AppIconKit"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-argument-parser", from: "1.5.0")
    ],
    targets: [
        .target(name: "AppIconKit"),
        .executableTarget(
            name: "appicon-generator",
            dependencies: [
                "AppIconKit",
                .product(name: "ArgumentParser", package: "swift-argument-parser"),
            ]
        ),
        .testTarget(
            name: "AppIconKitTests",
            dependencies: ["AppIconKit"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
