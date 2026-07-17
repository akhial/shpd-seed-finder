# Seed Seeker for macOS

This directory contains the native SwiftUI app for Apple Silicon. The Swift package in
`SeedSeeker/` links the Rust engine statically through a small C shim.

## Requirements

- macOS 14 or newer
- Xcode with Swift 6
- Rust with the Apple Silicon target:

```sh
rustup target add aarch64-apple-darwin
```

## Build

From the repository root:

```sh
bash scripts/build-macos-app.sh
```

The app is written to `dist/Seed Seeker.app`. Local builds are ad-hoc signed. Set
`MACOS_SIGN_IDENTITY` to a Developer ID Application identity for distribution signing.

## Test

Build the Rust static library before running the Swift tests:

```sh
bash scripts/build-macos-native.sh
cd macos/SeedSeeker
swift test
```

See [`SeedSeeker/LINKING.md`](SeedSeeker/LINKING.md) for SwiftPM linking details.
