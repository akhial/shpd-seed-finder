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

The Trinket category selects one named trinket and supports the board's existing
Either/or grouping. Scout shows the Magical Catalyst on its generated floor,
with four square choices and the remaining deck in order below them. Matched
choices use the app's flat mint highlight. The native scout decoder accepts SSC4
(including the full 17-entry deck) and legacy SSC3 manifests.

The Artifacts category requires a named artifact and supports floor limits,
source and curse filters, upgrade predicates through +5, and Either/or groups.
Scouting shows artifacts alongside other items with their generated upgrade.
Artifact searches display unavailable probability estimates.
