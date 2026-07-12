// swift-tools-version: 6.0
import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let rustLibrary = packageRoot
    .appendingPathComponent("../../target/aarch64-apple-darwin/release")
    .standardizedFileURL.path

let package = Package(
    name: "SeedSeeker",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "SeedSeekerKit", targets: ["SeedSeekerKit"]),
        .executable(name: "SeedSeeker", targets: ["SeedSeeker"]),
    ],
    targets: [
        .target(
            name: "CSeedFinder",
            publicHeadersPath: "include",
            // Link the static archive by explicit path: with `-l`, ld prefers
            // the cdylib that the same cargo build emits for other platforms,
            // and the app then cannot launch off the build machine.
            linkerSettings: [.unsafeFlags([rustLibrary + "/libshpd_seedfinder_ffi.a"])]
        ),
        .target(name: "SeedSeekerKit", dependencies: ["CSeedFinder"]),
        .executableTarget(name: "SeedSeeker", dependencies: ["SeedSeekerKit"]),
        .testTarget(name: "SeedSeekerKitTests", dependencies: ["SeedSeekerKit"]),
    ]
)
