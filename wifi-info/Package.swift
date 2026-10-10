// swift-tools-version:6.2

import PackageDescription

let package = Package(
    name: "wifi-info",
    platforms: [
        // CoreWLAN and CoreLocation, so this is macOS-only.
        .macOS(.v14)
    ],
    products: [
        .executable(name: "wifi-info", targets: ["wifi-info"])
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-argument-parser", from: "1.5.0")
    ],
    targets: [
        .executableTarget(
            name: "wifi-info",
            dependencies: [
                .product(name: "ArgumentParser", package: "swift-argument-parser")
            ]
        )
    ],
    swiftLanguageModes: [.v6]
)
