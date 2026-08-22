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
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.9.4"),
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
        .target(
            name: "SeedSeekerKit",
            dependencies: ["CSeedFinder"],
            // The upstream item catalog every front-end reads. It is not
            // copied here: `Resources/shattered-pixel-dungeon` is a symlink to
            // the canonical third-party asset directory the atlases also come
            // from, so there is still one copy in the repository.
            // SwiftPM emits it as `SeedSeeker_SeedSeekerKit.bundle` beside the
            // built executable, which the app bundle must carry for
            // `Bundle.module` to find it.
            resources: [.copy("Resources/shattered-pixel-dungeon/catalog-v3.3.8.json")]
        ),
        .executableTarget(
            name: "SeedSeeker",
            dependencies: [
                "SeedSeekerKit",
                .product(name: "Sparkle", package: "Sparkle"),
            ],
            // Sparkle.framework is embedded in Contents/Frameworks by
            // scripts/build-macos-app.sh; the executable finds it via rpath.
            linkerSettings: [.unsafeFlags(
                ["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])]
        ),
        .testTarget(name: "SeedSeekerKitTests", dependencies: ["SeedSeekerKit"]),
    ]
)
