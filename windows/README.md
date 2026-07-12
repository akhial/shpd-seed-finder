# Seed Seeker for Windows

The Windows client is a native WinUI 3 desktop app using Fluent Design 2 and the same Rust engine and wire formats as the macOS and Android clients.

## Requirements

- Windows 10 1809 or newer (Windows 11 recommended)
- Visual Studio with **WinUI application development**, **.NET desktop development**, and ARM64 tools
- Rust MSVC ARM64 target: `rustup target add aarch64-pc-windows-msvc`

Open `SeedSeeker.slnx` in Visual Studio and select ARM64, or run `.\scripts\build-windows-app.ps1` from the repository root.

The app is unpackaged and framework-dependent. The Windows App SDK 1.8 runtime must be installed; Visual Studio installs it on development machines.
