# Seed Seeker for Windows

The Windows client is a native WinUI 3 desktop app using Fluent Design 2 and the same Rust engine and wire formats as the macOS and Android clients.

## Requirements

- Windows 10 1809 or newer (Windows 11 recommended)
- Visual Studio with **WinUI application development**, **.NET desktop development**, and ARM64 tools
- Rust MSVC ARM64 target: `rustup target add aarch64-pc-windows-msvc`

Open `SeedSeeker.slnx` in Visual Studio and select ARM64, or run `.\scripts\build-windows-app.ps1` from the repository root.

The app is unpackaged and framework-dependent. The Windows App SDK 1.8 runtime must be installed; Visual Studio installs it on development machines.

On first launch the app registers the `seedseeker://` link scheme for the current user (per-user `HKCU` entries, refreshed on every launch), so shared query links open in the app — in the already-running instance when there is one.

## Item artwork

Scouted items and requirements render the real Shattered Pixel Dungeon sprites, pulsing with the game's enchantment and curse glow colours. The atlas geometry and the glow table mirror `web/src/lib/sprites.ts` and `web/src/lib/glow.ts`; the Fluent palette itself is unchanged, and the only new colours are the per-enchantment glows, which are item data from the game rather than app chrome.

`items.png`, `item_icons.png`, `LICENSE.txt` and `ATTRIBUTION.md` are linked into `Assets\` from `android/app/src/main/assets/third_party/shattered-pixel-dungeon/` rather than duplicated. The artwork is GPL-3.0-or-later, so the app ships those notices and surfaces them: the sidebar footer's upstream-version link — whose text is the engine's own `shpdVersion` — opens an About dialog with the attribution and the full license text.

## Trinkets

The Trinket category selects a named trinket and supports the board’s existing OR groups. Its editor omits equipment details. Scout displays the Magical Catalyst at its source and floor, with four square choice cards and the remaining deck in one row. Matches use the Fluent success fill and border. SpriteView renders both sizes with nearest-neighbor scaling, and long card names shrink on one line. The SSC4 decoder reads all 17 ordered identities and retains SSC3 compatibility for older engines.

## Artifacts

The Artifact category requires a named artifact and supports floor limits, source and curse filters, upgrades through +5, and OR groups. Artifacts are single items rather than stacks. Scout uses the shared catalog and engine upgrades, including +5 Imp vault rewards, and keeps artifact matches aligned with the full manifest. Artifact queries show probability and estimated search time as unavailable.
