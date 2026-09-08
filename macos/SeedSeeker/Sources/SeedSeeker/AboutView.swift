import SeedSeekerKit
import SwiftUI

/// Artwork attribution and license notice.
///
/// The app now ships Shattered Pixel Dungeon's item atlases, which are
/// GPL-3.0-or-later artwork, so it must carry the attribution and the full
/// license text with it. `scripts/build-macos-app.sh` installs the upstream
/// `LICENSE.txt` and `ATTRIBUTION.md` next to the PNGs in
/// `Contents/Resources`; this reads the license from there, mirroring what
/// Android's `AboutScreen.kt` shows.
struct AboutView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var showingLicense = false

    var body: some View {
        VStack(spacing: 0) {
            Text("Artwork & Licenses")
                .font(.headline).padding(.top, 14).padding(.bottom, 4)
            Form {
                Section("Artwork attribution") {
                    Text("The item sprites, ring type glyphs, and floor feeling icons are unchanged copies of "
                         + "Shattered Pixel Dungeon's artwork. Floor feeling icons are from v3.3.8.")
                        .foregroundStyle(.secondary)
                    attribution("Upstream", "Shattered Pixel Dungeon v\(EngineInfo.shared.shpdVersion)")
                    attribution("Release JAR SHA-256", EngineInfo.shared.shpdCommit)
                    attribution("Pixel Dungeon", "© 2012–2015 Oleg Dolya")
                    attribution("Shattered Pixel Dungeon", "© 2014–2026 Evan Debenham")
                    attribution("Project", "github.com/00-Evan/shattered-pixel-dungeon")
                    attribution("Bundled notices", "ATTRIBUTION.md, DUNGEON-ICONS-ATTRIBUTION.md and LICENSE.txt in Resources")
                }
                Section("GNU GPL v3 or later") {
                    Text("This program is free software. You may redistribute and modify it under "
                         + "GPL-3.0-or-later. It comes with no warranty. Source distributions must "
                         + "retain the license and copyright notices.")
                        .foregroundStyle(.secondary)
                    Button(showingLicense ? "Hide Full License" : "Read Full License") {
                        showingLicense.toggle()
                    }
                    if showingLicense {
                        ScrollView {
                            Text(licenseText)
                                .font(.system(.caption2, design: .monospaced))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .frame(height: 220)
                    }
                }
                Section("Not the game") {
                    Text("Seed Seeker is an independent utility and is not affiliated with or "
                         + "endorsed by Shattered Pixel Dungeon or its authors. Its interface is "
                         + "original; no game UI components are used.")
                        .foregroundStyle(.secondary)
                }
            }
            .formStyle(.grouped)
            Divider()
            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
            }.padding(12)
        }
        .frame(width: 520, height: 600)
    }

    private func attribution(_ label: String, _ value: String) -> some View {
        LabeledContent(label) {
            Text(value).font(.caption).foregroundStyle(.secondary)
                .textSelection(.enabled).multilineTextAlignment(.trailing)
        }
    }

    private var licenseText: String {
        guard let url = Bundle.main.url(forResource: "LICENSE", withExtension: "txt"),
              let text = try? String(contentsOf: url, encoding: .utf8) else {
            return "The bundled LICENSE.txt could not be read. The full GNU General Public "
                 + "License v3 is available at https://www.gnu.org/licenses/gpl-3.0.txt"
        }
        return text
    }
}
