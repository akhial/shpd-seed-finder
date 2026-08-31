import AppKit
import Combine
import SeedSeekerKit
import Sparkle
import SwiftUI
import UniformTypeIdentifiers

@main
struct SeedSeekerApp: App {
    // Updates are handled by Sparkle: it schedules background checks (asking
    // the user for permission first) and drives the whole download/install
    // flow. Dev runs via `swift run` execute outside a bundle and see no
    // Info.plist, hence no feed — the updater then stays stopped and the
    // menu item below stays disabled.
    private let updaterController = SPUStandardUpdaterController(
        startingUpdater: Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") != nil,
        updaterDelegate: nil, userDriverDelegate: nil)

    var body: some Scene {
        WindowGroup("Seed Seeker") { ContentView() }
            .defaultSize(width: 1_360, height: 760)
            .commands {
                CommandGroup(after: .appInfo) {
                    CheckForUpdatesView(updater: updaterController.updater)
                }
            }
        Settings { ChallengesSettingsView() }
    }
}

/// The "Check for Updates…" menu item, enabled whenever Sparkle can check
/// (updater started and no check already in flight).
private struct CheckForUpdatesView: View {
    @ObservedObject private var model: CheckForUpdatesViewModel
    private let updater: SPUUpdater

    init(updater: SPUUpdater) {
        self.updater = updater
        model = CheckForUpdatesViewModel(updater: updater)
    }

    var body: some View {
        Button("Check for Updates…") { updater.checkForUpdates() }
            .disabled(!model.canCheckForUpdates)
    }
}

@MainActor
private final class CheckForUpdatesViewModel: ObservableObject {
    @Published var canCheckForUpdates = false

    init(updater: SPUUpdater) {
        updater.publisher(for: \.canCheckForUpdates).assign(to: &$canCheckForUpdates)
    }
}

private struct ContentView: View {
    @AppStorage("savedQuery") private var savedQueryJSON = ""
    @AppStorage("savedPresets") private var savedPresetsJSON = ""
    @AppStorage("challenges") private var challenges = 0
    @State private var requirements: [ItemRequirement] = []
    @State private var maximumDepth = 24
    @State private var requireBlacksmith = false
    @State private var wandmakerQuest: WandmakerQuest?
    @State private var excludeBlacksmithRewards = false
    @State private var fastMode = false
    @State private var restored = false
    @State private var userPresets: [QueryPreset] = []
    @State private var controller = SearchController()
    @State private var scout = ScoutViewModel()
    @State private var showingAbout = false
    @State private var resultKeyMonitor: Any?
    @State private var hostWindow: NSWindow?
    @State private var exportDocument: ResultsFileDocument?
    @State private var showingImporter = false
    @State private var transferError: String?
    @State private var pendingLink: URL?
    @State private var linkCopied = false
    @State private var linkCopiedReset: Task<Void, Never>?

    /// Transient search notes shown in the window-bottom status bar rather
    /// than inside the results list.
    private var statusBarText: String? {
        var parts: [String] = []
        if let kept = controller.refinedKept, let of = controller.refinedOf {
            parts.append("Refined: kept \(kept) of \(of) previous seed\(of == 1 ? "" : "s")")
        }
        // A fresh detached scan is the one moment the display and the kept
        // Target Set diverge, so say what happened to the earlier results. A
        // continued detached scan tells its own story through the refined
        // caption above.
        if controller.runKind == .detached && controller.refinedKept == nil && controller.target != nil {
            parts.append("Unrelated query — detached search from previous results.")
        }
        // Only a concluded run announces the cap: while an accumulating scan
        // runs, a full display is the expected state, not news.
        if controller.reachedResultCap && !controller.isRunning { parts.append("Result limit reached (1,024 seeds).") }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    var body: some View {
        VStack(spacing: 0) {
            // The query pane is where the work happens now that requirements
            // are a board of chips, so it is a sidebar in look only: it starts
            // at two fifths of the default window and may grow to most of it,
            // rather than being dealt the drawer's usual share.
            NavigationSplitView {
                QueryView(requirements: $requirements, maximumDepth: $maximumDepth,
                          requireBlacksmith: $requireBlacksmith,
                          excludeBlacksmithRewards: $excludeBlacksmithRewards,
                          wandmakerQuest: $wandmakerQuest, fastMode: $fastMode,
                          challenges: $challenges,
                          userPresets: userPresets,
                          onApplyPreset: apply,
                          onSavePreset: savePreset,
                          onDeletePreset: deletePreset,
                          controller: controller)
                    .navigationSplitViewColumnWidth(min: 440, ideal: 540, max: 900)
                    // A sidebar column is hosted in Tahoe's concentric glass,
                    // which paints a hard specular streak across the pane —
                    // a highlight Finder and System Settings do not show. The
                    // results column escapes it because its List brings an
                    // opaque background of its own; the query pane is a plain
                    // ScrollView, so it has to supply one.
                    .background(Color(nsColor: .windowBackgroundColor))
            } content: {
                ResultsView(controller: controller) { seed in scout.scout(seed, challenges: challenges) }
                    .navigationSplitViewColumnWidth(min: 300, ideal: 380)
            } detail: {
                SeedDetailView(model: scout, requirements: requirements, maximumDepth: maximumDepth,
                               excludeBlacksmithRewards: excludeBlacksmithRewards, challenges: challenges,
                               resultPosition: resultPosition, onNavigateResult: { _ = navigateResult($0) })
                    .navigationSplitViewColumnWidth(min: 380, ideal: 440)
            }
            Divider()
            // One permanent bottom bar: attribution on the left, transient
            // search status on the right. A bar that only existed once there
            // was status text resized the split view when it appeared,
            // clipping the sidebar's pinned Start Search button.
            HStack(spacing: 8) {
                // The bundled item artwork is GPL-3.0-or-later, so its
                // attribution and the full license text have to be reachable
                // from the app.
                Button { showingAbout = true } label: {
                    Text("Shattered Pixel Dungeon v\(EngineInfo.shared.shpdVersion) · Artwork & licenses")
                        .font(.caption).foregroundStyle(.secondary)
                        .padding(.vertical, 5)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .layoutPriority(1)
                .help("Item artwork attribution and license")
                Spacer(minLength: 8)
                if let text = statusBarText {
                    Text(text)
                        .font(.caption).foregroundStyle(.secondary)
                        .lineLimit(1).truncationMode(.tail)
                }
            }
            .padding(.horizontal, 16)
        }
        .toolbar { toolbarItems }
        .sheet(isPresented: $showingAbout) { AboutView() }
        .fileExporter(
            isPresented: Binding(
                get: { exportDocument != nil },
                set: { if !$0 { exportDocument = nil } }),
            document: exportDocument,
            contentType: .json,
            defaultFilename: ResultsExport.suggestedFileName
        ) { result in
            if case .failure(let error) = result {
                transferError = "Export failed: \(error.localizedDescription)"
            }
        }
        .fileImporter(isPresented: $showingImporter, allowedContentTypes: [.json]) { result in
            if case .success(let url) = result { importResults(from: url) }
        }
        .onOpenURL { url in
            // On a cold launch the URL can arrive before .onAppear has
            // restored the saved query; applying it then would be clobbered
            // by the restore, so it waits its turn.
            if restored { openQueryLink(url) } else { pendingLink = url }
        }
        .alert("Seed Seeker", isPresented: Binding(
            get: { transferError != nil },
            set: { if !$0 { transferError = nil } })
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(transferError ?? "")
        }
        .frame(minWidth: 1_140, minHeight: 640)
        .background(WindowAccessor(window: $hostWindow))
        .onAppear {
            installResultKeyNavigation()
            guard !restored else { return }; restored = true
            let saved = QueryPersistence.decode(savedQueryJSON)
            requirements = saved.requirements; maximumDepth = saved.maximumDepth
            requireBlacksmith = saved.requireBlacksmith
            excludeBlacksmithRewards = saved.excludeBlacksmithRewards
            wandmakerQuest = saved.wandmakerQuest
            fastMode = saved.fastMode
            userPresets = PresetPersistence.decode(savedPresetsJSON)
            if let link = pendingLink { pendingLink = nil; openQueryLink(link) }
        }
        .onDisappear {
            if let monitor = resultKeyMonitor { NSEvent.removeMonitor(monitor); resultKeyMonitor = nil }
        }
        .onChange(of: requirements) { save() }
        .onChange(of: maximumDepth) { save() }
        .onChange(of: requireBlacksmith) { save() }
        .onChange(of: excludeBlacksmithRewards) { save() }
        .onChange(of: wandmakerQuest) { save() }
        .onChange(of: fastMode) { save() }
        .onChange(of: challenges) { save() }
        .onChange(of: controller.selectedSeed) { _, seed in
            // J/K navigation scouts before moving the selection; only scout
            // here for direct table selections.
            if let seed, seed != scout.requestedSeed { scout.scout(seed, challenges: challenges) }
        }
    }

    /// The results' file actions, in the window toolbar: import, export,
    /// clear, and a link to the query that made them.
    @ToolbarContentBuilder private var toolbarItems: some ToolbarContent {
        ToolbarItemGroup {
            Button {
                showingImporter = true
            } label: {
                Label("Import…", systemImage: "square.and.arrow.down")
            }
            // Toolbar labels default to icon-only, which left the
            // glyphs looking uncentred inside their glass capsules.
            .labelStyle(ToolbarActionLabelStyle())
            .help("Import results and their query from a file")
            .disabled(controller.isRunning)
            Button {
                beginExport()
            } label: {
                Label("Export…", systemImage: "square.and.arrow.up")
            }
            .labelStyle(ToolbarActionLabelStyle())
            .help("Export the results and the query that produced them to a file")
            .disabled(controller.isRunning || controller.results.isEmpty
                || controller.exportQuery == nil)
            Button {
                controller.clearResults()
            } label: {
                Label("Clear", systemImage: "trash")
            }
            .labelStyle(ToolbarActionLabelStyle(trailingEllipsis: false))
            .help("Clear the results, so the next search starts from scratch")
            .disabled(!controller.canClearResults)
            Button {
                copyQueryLink()
            } label: {
                Label("Copy Link",
                      systemImage: linkCopied ? "checkmark" : "link")
            }
            .labelStyle(ToolbarActionLabelStyle())
            .help("Copy a shareable link to the current query")
            .disabled(controller.isRunning)
        }
    }

    /// Where the scouted seed sits in the search results, or nil when it did
    /// not come from one (hand-entered seed, or no search yet).
    private var resultPosition: ResultPosition? {
        let seeds = controller.results.map(\.seed)
        guard let index = ResultNavigation.position(of: scout.requestedSeed, in: seeds) else { return nil }
        return ResultPosition(index: index, total: seeds.count)
    }

    /// Scouts the search result `offset` steps from the last requested scout
    /// seed. Scouting first (which records the new anchor synchronously) and
    /// then moving the table selection lets rapid steps chain while a scout
    /// is still in flight. Returns whether navigation moved.
    private func navigateResult(_ offset: Int) -> Bool {
        guard let next = ResultNavigation.seed(from: scout.requestedSeed,
                                               in: controller.results.map(\.seed),
                                               offset: offset) else { return false }
        scout.scout(next, challenges: challenges)
        controller.selectedSeed = next
        return true
    }

    /// J (next) and K (previous) walk the search results while scouting, and
    /// holding either key keeps walking at the system key-repeat rate.
    /// A plain-key `.keyboardShortcut` would steal the letters from text
    /// fields, so a local monitor is used instead. It only acts for its own
    /// window (each window of the group installs one), and passes the event
    /// through while a sheet is presented, while a text view is typing, or
    /// when navigation has nowhere to go.
    private func installResultKeyNavigation() {
        guard resultKeyMonitor == nil else { return }
        resultKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard let window = hostWindow, event.window === window, window.attachedSheet == nil,
                  event.modifierFlags.intersection([.command, .option, .control]).isEmpty,
                  !(window.firstResponder is NSText)
            else { return event }
            // Match the letter (mnemonic) or the physical key (keycodes 38/40),
            // so the shortcut works on layouts without Latin letters.
            let key = event.charactersIgnoringModifiers?.lowercased()
            let offset: Int
            if key == "j" || event.keyCode == 38 {
                offset = 1
            } else if key == "k" || event.keyCode == 40 {
                offset = -1
            } else {
                return event
            }
            // A repeat arriving while a scout is still in flight is dropped
            // rather than queued, so a held key paces itself to the engine
            // instead of running the list away from the manifest. It stays
            // swallowed: the key is mid-navigation, not unhandled.
            if event.isARepeat && scout.loading { return nil }
            return navigateResult(offset) ? nil : event
        }
    }

    private func save() {
        guard restored else { return }
        savedQueryJSON = QueryPersistence.encode(.init(requirements: requirements,
            maximumDepth: maximumDepth, requireBlacksmith: requireBlacksmith,
            excludeBlacksmithRewards: excludeBlacksmithRewards,
            wandmakerQuest: wandmakerQuest, fastMode: fastMode,
            challenges: challenges)) ?? ""
    }

    private func apply(_ preset: QueryPreset) { apply(preset.query) }

    private func apply(_ saved: SavedQuery) {
        requirements = saved.requirements.map { requirement in
            var copy = requirement
            copy.key = Int64.random(in: 1...Int64.max)
            return copy
        }
        maximumDepth = saved.maximumDepth
        requireBlacksmith = saved.requireBlacksmith
        excludeBlacksmithRewards = saved.excludeBlacksmithRewards
        wandmakerQuest = saved.wandmakerQuest
        fastMode = saved.fastMode
        challenges = saved.challenges
    }

    private func beginExport() {
        // Export the query snapshot captured when the results were produced
        // (at search start or import), never the live editor state.
        guard !controller.isRunning, let query = controller.exportQuery,
              !controller.results.isEmpty else { return }
        let appVersion = Bundle.main
            .object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
        exportDocument = ResultsFileDocument(
            text: ResultsExport.encode(query, seeds: controller.results.map(\.seed),
                                       appVersion: appVersion))
    }

    /// Applies the query carried by a `seedseeker://` link (URL-scheme
    /// registration only works from the built app bundle, not `swift run`).
    private func openQueryLink(_ url: URL) {
        guard !controller.isRunning else {
            transferError = "Stop the search before opening a query link."
            return
        }
        do {
            apply(try DeepLink.decode(url.absoluteString))
        } catch {
            transferError = (error as? LocalizedError)?.errorDescription
                ?? "This link does not contain a valid Seed Seeker query."
        }
    }

    /// Encodes the query as currently edited (unlike export, which snapshots
    /// the query behind the results) and puts the web link on the pasteboard.
    private func copyQueryLink() {
        do {
            let link = try DeepLink.encodeLink(for: SavedQuery(
                requirements: requirements, maximumDepth: maximumDepth,
                requireBlacksmith: requireBlacksmith,
                excludeBlacksmithRewards: excludeBlacksmithRewards,
                wandmakerQuest: wandmakerQuest,
                fastMode: fastMode, challenges: challenges))
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(link, forType: .string)
            // Brief checkmark in the toolbar icon as the "copied" feedback.
            linkCopied = true
            linkCopiedReset?.cancel()
            linkCopiedReset = Task {
                try? await Task.sleep(for: .seconds(1.5))
                guard !Task.isCancelled else { return }
                linkCopied = false
            }
        } catch {
            transferError = (error as? LocalizedError)?.errorDescription
                ?? "The current query could not be turned into a link."
        }
    }

    private func importResults(from url: URL) {
        guard !controller.isRunning else {
            transferError = "Stop the search before importing results."
            return
        }
        Task {
            // Read and parse the untrusted file off the main actor.
            let outcome: Result<ResultsExport.Imported, any Error> = await Task.detached {
                do {
                    let accessing = url.startAccessingSecurityScopedResource()
                    defer { if accessing { url.stopAccessingSecurityScopedResource() } }
                    let data = try Data(contentsOf: url)
                    guard let text = String(data: data, encoding: .utf8) else {
                        throw ResultsExportError("This is not a Seed Seeker results file (not UTF-8 text).")
                    }
                    return .success(try ResultsExport.decode(text))
                } catch {
                    return .failure(error)
                }
            }.value
            switch outcome {
            case .success(let imported):
                // A search may have started while the file was being read.
                guard !controller.isRunning else {
                    transferError = "Stop the search before importing results."
                    return
                }
                apply(imported.query)
                controller.loadImported(seeds: imported.seeds, dropped: imported.dropped,
                                       query: imported.query)
                let engineVersion = EngineInfo.shared.shpdVersion
                if let fileVersion = imported.shpdVersion, fileVersion != engineVersion {
                    transferError = "Imported \(controller.results.count) seeds. Note: this file was " +
                        "made for Shattered Pixel Dungeon v\(fileVersion); this app targets " +
                        "v\(engineVersion), so the seeds may generate differently."
                }
            case .failure(let error):
                transferError = (error as? LocalizedError)?.errorDescription
                    ?? "The results file could not be imported."
            }
        }
    }

    private func savePreset(name: String) {
        let cleanName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanName.isEmpty else { return }
        let query = SavedQuery(requirements: requirements, maximumDepth: maximumDepth,
                               requireBlacksmith: requireBlacksmith,
                               excludeBlacksmithRewards: excludeBlacksmithRewards,
                               wandmakerQuest: wandmakerQuest,
                               fastMode: fastMode, challenges: challenges)
        if let index = userPresets.firstIndex(where: { $0.name.localizedCaseInsensitiveCompare(cleanName) == .orderedSame }) {
            userPresets[index].query = query
        } else {
            userPresets.append(QueryPreset(name: cleanName, query: query))
        }
        savedPresetsJSON = PresetPersistence.encode(userPresets) ?? ""
    }

    private func deletePreset(_ preset: QueryPreset) {
        userPresets.removeAll { $0.id == preset.id }
        savedPresetsJSON = PresetPersistence.encode(userPresets) ?? ""
    }
}

private struct ChallengesSettingsView: View {
    @AppStorage("challenges") private var challenges = 0

    var body: some View {
        Form {
            Section {
                Text("Searches simulate runs with the selected challenges enabled.")
                    .foregroundStyle(.secondary)
            }
            Section("Challenges") {
                ForEach(Challenge.allCases, id: \.rawValue) { challenge in
                    VStack(alignment: .leading, spacing: 2) {
                        Toggle(challenge.label, isOn: binding(for: challenge))
                        Text(challenge.changesLevelGeneration
                             ? "changes level generation" : "no effect on seed content")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 460, height: 570)
    }

    private func binding(for challenge: Challenge) -> Binding<Bool> {
        Binding {
            challenges & challenge.rawValue != 0
        } set: { enabled in
            if enabled { challenges |= challenge.rawValue }
            else { challenges &= ~challenge.rawValue }
        }
    }
}

// MARK: - Palette

/// The three colours the chip board is built from, taken from the web app's
/// palette so both front ends read as the same product. System `.green`/`.orange`
/// are close enough to look deliberate and far enough to look wrong beside the
/// game's own art, which is why none of them are used on the board.
extension Color {
    /// Shattered Pixel Dungeon's upgrade green, the web app's `--d1-upgrade`.
    static let shatteredGreen = Color(.sRGB, red: 131 / 255, green: 252 / 255, blue: 100 / 255)
    /// The softer green the web app spends on stack and match badges, `--d1-green`.
    static let shatteredMint = Color(.sRGB, red: 110 / 255, green: 201 / 255, blue: 143 / 255)
    /// The game's highlight yellow, the web app's `--d1-amber`.
    static let shatteredYellow = Color(.sRGB, red: 1, green: 1, blue: 85 / 255)
}

// MARK: - Query sidebar

private struct EditorSession: Identifiable {
    let requirement: ItemRequirement
    let isNew: Bool
    /// Where the edited chip sits in the requirement list, or nil for a new one.
    let index: Int?
    /// The chip's stack as the board holds it; the editor may reshape it.
    let stack: StackShape
    var id: Int64 { requirement.key }
}

/// What the editor is told about the chip's stack, and what it hands back.
private struct StackShape {
    var count = 1
    var total: Int?
    /// The floor limit the extra copies share, when they carry one.
    var copyDepth: Int?
    /// A cluster member's stack belongs to the cluster, not to the editor.
    var inCluster = false
}

/// The editor's result: the chip's own fields, plus its stack's shape.
private struct EditorResult {
    let requirement: ItemRequirement
    let count: Int
    let total: Int?
    let copyDepth: Int?
}

private struct QueryView: View {
    @Binding var requirements: [ItemRequirement]
    @Binding var maximumDepth: Int
    @Binding var requireBlacksmith: Bool
    @Binding var excludeBlacksmithRewards: Bool
    @Binding var wandmakerQuest: WandmakerQuest?
    @Binding var fastMode: Bool
    @Binding var challenges: Int
    let userPresets: [QueryPreset]
    let onApplyPreset: (QueryPreset) -> Void
    let onSavePreset: (String) -> Void
    let onDeletePreset: (QueryPreset) -> Void
    let controller: SearchController
    @State private var editor: EditorSession?
    @State private var showingSavePreset = false
    @State private var presetName = ""

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    presets
                    requirementBoard
                    settings
                }
                .padding(.horizontal, 16).padding(.top, 10).padding(.bottom, 16)
            }
            Divider()
            if challenges.nonzeroBitCount > 0 {
                Label("Challenges: \(challenges.nonzeroBitCount) enabled", systemImage: "flag.fill")
                    .font(.caption).foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal).padding(.top, 8)
            }
            if let requestError {
                Label(requestError, systemImage: "exclamationmark.triangle")
                    .font(.caption).foregroundStyle(.orange)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal).padding(.top, 8)
            }
            // Starting a search that narrows — or just repeats — the last
            // finished run refines it automatically; the controller decides,
            // so there is no second button here.
            Button {
                if controller.isRunning { controller.cancel() }
                else if let request = builtRequest { controller.start(request) }
            } label: {
                Label(controller.isRunning ? "Cancel Search" : "Start Search",
                      systemImage: controller.isRunning ? "stop.fill" : "play.fill")
                    .frame(maxWidth: .infinity).padding(.vertical, 5)
            }.buttonStyle(.borderedProminent).tint(controller.isRunning ? .red : .accentColor)
                .disabled(builtRequest == nil && !controller.isRunning).keyboardShortcut(.return, modifiers: .command)
                .padding()
        }
        .navigationTitle("Query")
        .sheet(item: $editor) { session in
            RequirementEditor(requirement: session.requirement, isNew: session.isNew,
                              stack: session.stack) { result in
                if let result {
                    requirements = requirements.applyEdit(
                        index: session.index, requirement: result.requirement,
                        count: result.count, total: result.total, copyDepth: result.copyDepth)
                }
                editor = nil
            }
        }
        .alert("Save Preset", isPresented: $showingSavePreset) {
            TextField("Preset name", text: $presetName)
            Button("Cancel", role: .cancel) {}
            Button("Save") { onSavePreset(presetName) }
                .disabled(presetName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        } message: {
            Text("Save the current requirements and search settings.")
        }
    }

    private var builtRequest: SearchRequest? { try? buildRequest() }

    /// Why the query cannot be searched as it stands (a combined-level
    /// group that no longer adds up, say), or nil when it can.
    private var requestError: String? {
        guard !requirements.isEmpty else { return nil }
        do { _ = try buildRequest(); return nil } catch {
            return (error as? LocalizedError)?.errorDescription ?? "The query cannot be searched"
        }
    }

    private func buildRequest() throws -> SearchRequest {
        try SearchRequest(requirements: requirements, maximumDepth: maximumDepth,
                          requireBlacksmith: requireBlacksmith,
                          excludeBlacksmithRewards: excludeBlacksmithRewards,
                          wandmakerQuest: wandmakerQuest,
                          fastMode: fastMode, challenges: challenges)
    }

    private var presets: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel("Presets")
            HStack(spacing: 8) {
                Menu("Load Preset", systemImage: "bookmark") {
                    Section("Included") {
                        ForEach(BuiltInPresets.all) { preset in
                            Button(preset.name) { onApplyPreset(preset) }
                        }
                    }
                    if !userPresets.isEmpty {
                        Section("Saved") {
                            ForEach(userPresets) { preset in
                                Button(preset.name) { onApplyPreset(preset) }
                            }
                        }
                    }
                }
                .fixedSize()
                Button {
                    presetName = ""
                    showingSavePreset = true
                } label: {
                    Label("Save Current Query", systemImage: "bookmark.badge.plus")
                }
                .fixedSize()
                if !userPresets.isEmpty {
                    Menu("Delete Saved Preset", systemImage: "trash") {
                        ForEach(userPresets) { preset in
                            Button(preset.name, role: .destructive) { onDeletePreset(preset) }
                        }
                    }
                    .fixedSize()
                }
                Spacer(minLength: 0)
            }
        }
    }

    /// The requirement board: one flat wrapping row of chips, whose count is
    /// what the pane calls its requirements — a stack of three is one chip to
    /// look at, even though the engine has three items to find.
    private var requirementBoard: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 7) {
                SectionLabel("Requirements")
                if !requirements.isEmpty { CountBadge(requirements.boardCount) }
            }
            RequirementBoardView(requirements: $requirements, onEdit: openEditor,
                                 onAdd: addRequirement)
            if requirements.isEmpty {
                Text("No requirements yet. Add one to describe the item you're hunting for.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        }
    }

    /// The search settings, in two columns: each group is a control or two
    /// deep, and a pane this wide would otherwise stretch a slider or a
    /// checkbox across far more room than it has any use for.
    private var settings: some View {
        HStack(alignment: .top, spacing: 14) {
            VStack(alignment: .leading, spacing: 14) {
                SettingsGroup("Search scope") {
                    LabeledContent("Floor limit") {
                        Text("first \(maximumDepth) floor\(maximumDepth == 1 ? "" : "s")")
                            .monospacedDigit().foregroundStyle(.secondary)
                    }
                    Slider(value: floorLimitBinding($maximumDepth),
                           in: 0...Double(FloorLimits.options.count - 1), step: 1)
                        .accessibilityValue(Text("first \(maximumDepth) floor\(maximumDepth == 1 ? "" : "s")"))
                }
                SettingsGroup("Performance") {
                    Toggle("Fast search", isOn: $fastMode)
                    SettingsCaption("Treats +3 weapons and armor as quest rewards only — the Ghost's, the Blacksmith's and the Imp's vault prizes, so the search ends at floor 19 — skipping the rare Crypt, Sacrificial-fire and special-room chest prizes. Found seeds are always genuine.")
                }
            }
            VStack(alignment: .leading, spacing: 14) {
                SettingsGroup("Wandmaker") {
                    Picker("Quest", selection: $wandmakerQuest) {
                        Text("Any").tag(WandmakerQuest?.none)
                        ForEach(WandmakerQuest.allCases, id: \.self) { quest in
                            Text(quest.label).tag(WandmakerQuest?.some(quest))
                        }
                    }
                }
                SettingsGroup("Blacksmith") {
                    // A run whose floor limit reaches his last floor always meets him.
                    Toggle("Require accessible blacksmith", isOn: $requireBlacksmith)
                        .disabled(maximumDepth >= ScoutQuestKind.blacksmith.depthRange.upperBound)
                    Toggle("Exclude Smith rewards", isOn: $excludeBlacksmithRewards)
                    SettingsCaption("Required items cannot come from the 2,000-favor Smith choice, leaving favor available for reforging.")
                }
            }
        }
    }

    /// Opens the editor on the chip at `index`, telling it the stack the chip
    /// stands for so the "Total item count" section starts where the board is.
    private func openEditor(_ index: Int) {
        guard requirements.indices.contains(index) else { return }
        let item = requirements.boardItem(holding: index)
        editor = EditorSession(
            requirement: requirements[index], isNew: false, index: index,
            stack: StackShape(count: item?.stackCount ?? 1, total: item?.total,
                              copyDepth: item.flatMap { requirements.copyDepth(of: $0) },
                              inCluster: item?.cluster != nil))
    }

    private func addRequirement() {
        if let value = try? ItemRequirement(key: Int64.random(in: 1...Int64.max), item: nil,
            upgrade: 0, kind: .weapon, upgradeMatch: .any) {
            editor = EditorSession(requirement: value, isNew: true, index: nil, stack: StackShape())
        }
    }
}

// MARK: - Requirement board

/**
 The requirement board: every requirement is a chip; drop one chip onto
 another for an either/or cluster, drag a chip out of its cluster to make it
 standalone again. Everything else is a property of the chip itself — a stack
 badge (×N / ≤N) for "more of the same kind", and a Σ badge for a stack whose
 items count their levels towards one total.

 The board is the *collapsed* view of the flat requirement list that
 ``Swift/Array/boardItems()`` derives; every gesture here goes through those
 pure edits, so what the board writes is always a query the engine will take.
 */
private struct RequirementBoardView: View {
    @Binding var requirements: [ItemRequirement]
    let onEdit: (Int) -> Void
    let onAdd: () -> Void
    /// The key of the chip in flight — also what says the bin should show.
    @State private var dragging: Int64?
    @State private var overBin = false

    var body: some View {
        let items = requirements.boardItems()
        let errors = boardErrors(requirements)
        VStack(alignment: .leading, spacing: 6) {
            FlowLayout(spacing: 6, lineSpacing: 8) {
                ForEach(items) { item in
                    if item.cluster == nil {
                        ChipView(requirements: $requirements, requirement: requirements[item.anchor],
                                 index: item.anchor, item: item, inCluster: false,
                                 error: errors[item.anchor], dragging: $dragging, onEdit: onEdit)
                    } else {
                        ClusterView(requirements: $requirements, item: item, errors: errors,
                                    dragging: $dragging, onEdit: onEdit)
                    }
                }
                AddChipView(action: onAdd)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if dragging != nil { bin }
        }
        .padding(.vertical, 2)
        // Behind the chips, so it can never take a click meant for one: the
        // bin only exists while a drag does, and a drag that ends off the
        // board leaves nothing to tell us so.
        .background(Color.clear.contentShape(Rectangle()).onTapGesture { dragging = nil })
        // Dropped on the board rather than on a chip: how a cluster member goes
        // back to standing on its own. It is also the catch-all that puts the
        // bin away when a drag ends without landing anywhere.
        .dropDestination(for: String.self) { payload, _ in
            dragging = nil
            guard let source = draggedIndex(payload, in: requirements),
                  requirements[source].alternativeGroup != nil else { return false }
            requirements = requirements.detach(source)
            return true
        }
    }

    /// The bin: only there while a chip is in flight, and the pointer's only
    /// way to delete one.
    private var bin: some View {
        HStack(spacing: 6) {
            Image(systemName: "xmark.circle")
            Text("drop to remove")
        }
        .font(.caption.weight(.semibold))
        .foregroundStyle(overBin ? Color.white : Color.red)
        .frame(maxWidth: .infinity)
        .padding(.vertical, 7)
        .background(overBin ? Color.red : Color.clear, in: RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(
            Color.red.opacity(overBin ? 0 : 0.5),
            style: StrokeStyle(lineWidth: 1, dash: [4, 3])))
        .dropDestination(for: String.self) { payload, _ in
            dragging = nil; overBin = false
            guard let source = draggedIndex(payload, in: requirements),
                  let item = requirements.boardItem(holding: source) else { return false }
            requirements = item.cluster != nil
                ? requirements.removeMember(source)
                : requirements.removeItem(item)
            return true
        } isTargeted: { overBin = $0 }
    }
}

/// An either/or cluster: its chips share one dashed capsule, with the stack
/// badges at the capsule's trailing edge, since the stack is the cluster's.
private struct ClusterView: View {
    @Binding var requirements: [ItemRequirement]
    let item: BoardItem
    let errors: [Int: String]
    @Binding var dragging: Int64?
    let onEdit: (Int) -> Void
    @State private var isTargeted = false

    var body: some View {
        // A cluster keeps its identity across board passes by group number, so
        // when a preset replaces the list wholesale SwiftUI can re-run this
        // body with the previous pass's `item` against the new, shorter list.
        // Members that no longer exist are skipped for that one frame; the
        // parent's next pass hands down a fresh item.
        let members = item.members.filter { requirements.indices.contains($0) }
        HStack(spacing: 2) {
            ForEach(Array(members.enumerated()), id: \.element) { entry in
                if entry.offset > 0 {
                    Text("or")
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundStyle(Color.shatteredYellow.opacity(0.9))
                        .padding(.horizontal, 2)
                }
                ChipView(requirements: $requirements, requirement: requirements[entry.element],
                         index: entry.element, item: item, inCluster: true,
                         error: errors[entry.element], dragging: $dragging, onEdit: onEdit)
            }
            if (item.stackCount > 1 || item.total != nil) && requirements.indices.contains(item.anchor) {
                StackBadgesView(requirements: $requirements,
                                anchorKey: requirements[item.anchor].key)
                    .padding(.leading, 1).padding(.trailing, 3)
            }
        }
        .padding(3)
        .background(Color.shatteredYellow.opacity(0.05), in: Capsule())
        .overlay(Capsule().strokeBorder(
            isTargeted ? Color.shatteredYellow : Color.shatteredYellow.opacity(0.45),
            style: StrokeStyle(lineWidth: 1, dash: isTargeted ? [] : [4, 3])))
        .dropDestination(for: String.self) { payload, _ in
            dragging = nil
            guard let source = draggedIndex(payload, in: requirements),
                  requirements[source].alternativeGroup != item.cluster else { return false }
            requirements = requirements.joinAlternatives(source: source, target: item.anchor)
            return true
        } isTargeted: { isTargeted = $0 }
    }
}

/// One chip: the item's sprite, its short name, the qualifiers that fit in a
/// capsule, and — for a chip standing on its own — its stack badges.
private struct ChipView: View {
    @Binding var requirements: [ItemRequirement]
    /// The requirement as this pass of the board saw it.
    let requirement: ItemRequirement
    /// Its place in the list at that moment. Every action looks the row up
    /// again by key, since an edit renumbers the list under it.
    let index: Int
    /// The board entry the chip belongs to: its own, or its cluster's.
    let item: BoardItem
    let inCluster: Bool
    /// What the query's cross-requirement validation blames this chip for.
    let error: String?
    @Binding var dragging: Int64?
    let onEdit: (Int) -> Void
    @State private var isTargeted = false
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 5) {
            HStack(spacing: 5) {
                // No seed here: a chip names an item class the search is to
                // look for, so its ring keeps the catalog's own cell.
                ItemSpriteView(item: requirement.item,
                               glow: effectGlow(requirement.effect.glowName), pointSize: 16)
                Text(chipName(requirement))
                    .font(.system(size: 12, weight: .semibold))
                    .lineLimit(1).truncationMode(.tail)
                    .frame(maxWidth: 150, alignment: .leading)
                ForEach(chipTags(requirement), id: \.self) { tag in
                    Text(tag.text)
                        .font(.system(size: 11, weight: .semibold, design: .monospaced))
                        .foregroundStyle(tag.upgrade ? Color.shatteredGreen : Color.shatteredYellow)
                        .padding(.horizontal, 4)
                        .background((tag.upgrade ? Color.shatteredGreen : Color.shatteredYellow).opacity(0.13),
                                    in: RoundedRectangle(cornerRadius: 4))
                }
                effectBadge
                if requirement.requireUncursed {
                    Text("✓")
                        .font(.system(size: 11, weight: .semibold, design: .monospaced))
                        .foregroundStyle(Color.shatteredMint)
                        .padding(.horizontal, 4)
                        .background(Color.shatteredMint.opacity(0.14), in: RoundedRectangle(cornerRadius: 4))
                }
            }
            .contentShape(Rectangle())
            .onTapGesture { if let live = liveIndex { onEdit(live) } }
            if !inCluster, item.stackCount > 1 || item.total != nil {
                StackBadgesView(requirements: $requirements, anchorKey: requirement.key)
            }
        }
        .padding(.leading, 7).padding(.trailing, 7)
        .frame(height: 30)
        .background(Color(nsColor: .controlBackgroundColor), in: Capsule())
        .overlay(Capsule().strokeBorder(borderColour, lineWidth: focused ? 2 : 1))
        .opacity(dragging == requirement.key ? 0.35 : 1)
        .contentShape(Capsule())
        .help(helpText)
        .focusable()
        .focused($focused)
        .onKeyPress(.delete) { removeSelf(); return .handled }
        .onDrag {
            dragging = requirement.key
            return NSItemProvider(object: NSString(string: "\(requirement.key)"))
        }
        .dropDestination(for: String.self) { payload, _ in
            dragging = nil
            guard let source = draggedIndex(payload, in: requirements),
                  let target = liveIndex, source != target else { return false }
            requirements = requirements.joinAlternatives(source: source, target: target)
            return true
        } isTargeted: { isTargeted = $0 }
        .contextMenu { menu }
        .accessibilityLabel(requirement.title)
    }

    private var borderColour: Color {
        if isTargeted { return .shatteredYellow }
        if error != nil { return .red }
        if focused { return .accentColor }
        return .secondary.opacity(0.35)
    }

    /// Where the chip's requirement is now, since an edit renumbers the list.
    private var liveIndex: Int? { requirements.firstIndex { $0.key == requirement.key } }
    /// The board entry it belongs to now.
    private var liveItem: BoardItem? { liveIndex.flatMap { requirements.boardItem(holding: $0) } }

    // MARK: The effect badge

    /// A single effect wants no badge of its own: the sprite is already
    /// pulsing that very colour — black, for a curse — and the tooltip names
    /// it. What is left for a badge is what one pulse cannot say: several
    /// effects at once, or "any enchantment", which settles on no colour.
    @ViewBuilder private var effectBadge: some View {
        let names = requirement.effect.names
        if names.count > 1 {
            Text("\(names.count)")
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .padding(.horizontal, 4).frame(minWidth: 16, minHeight: 16)
                .overlay(Capsule().strokeBorder(
                    AngularGradient(colors: effectColours(names), center: .center), lineWidth: 2.5))
        } else if requirement.effect == .anyEnchantment {
            Circle()
                .fill(AngularGradient(colors: Self.spectrum, center: .center))
                .frame(width: 11, height: 11)
        }
    }

    /// Every effect's glow colour around the ring, the first closing it.
    private func effectColours(_ names: [String]) -> [Color] {
        let colours = names.map { name -> Color in
            let glow = effectGlow(name) ?? curseGlow
            let (red, green, blue) = glow.components
            return Color(.sRGB, red: red, green: green, blue: blue)
        }
        return colours + [colours[0]]
    }

    /// "Any enchantment" settles on no colour, so its dot holds them all.
    private static let spectrum: [Color] = [
        Color(red: 1, green: 1 / 3, blue: 1 / 3), Color(red: 1, green: 1, blue: 1 / 3),
        Color(red: 1 / 3, green: 1, blue: 1 / 3), Color(red: 1 / 3, green: 1, blue: 1),
        Color(red: 1 / 3, green: 1 / 3, blue: 1), Color(red: 1, green: 1 / 3, blue: 1),
        Color(red: 1, green: 1 / 3, blue: 1 / 3),
    ]

    // MARK: The tooltip

    /// What the web design shows in its hover card, as a native tooltip: the
    /// chip's own qualities, then the relationships the badges only hint at.
    private var helpText: String {
        var lines = [requirement.title]
        var parts: [String] = []
        switch requirement.upgradeMatch {
        case .exactly: parts.append("exactly +\(requirement.upgrade)")
        case .atLeast: parts.append("+\(requirement.upgrade) or higher")
        case .any: if item.total == nil { parts.append("any upgrade") }
        }
        if let effect = requirement.effect.label(for: requirement.kind) { parts.append(effect) }
        if requirement.requireUncursed { parts.append("uncursed") }
        if let source = requirement.source { parts.append(source.label) }
        if let depth = requirement.maximumDepth { parts.append("floors 1–\(depth)") }
        if !parts.isEmpty { lines.append(parts.joined(separator: " · ")) }
        if let group = requirement.alternativeGroup {
            let peers = requirements
                .filter { $0.key != requirement.key && $0.alternativeGroup == group }
                .map(chipName)
            if !peers.isEmpty { lines.append("or \(peers.joined(separator: ", "))") }
        }
        if let total = item.total {
            lines.append("Σ up to \(item.stackCount) — levels add to ≥ \(total)")
        } else if item.stackCount > 1 {
            // The chip's own bounds (+3, F≤4) describe one copy, not the extras.
            let depths = Set(item.extras.map { requirements.indices.contains($0)
                ? requirements[$0].maximumDepth : nil })
            let floors = depths.count > 1 ? "own floor limits"
                : (depths.first ?? nil).map { "floors 1–\($0)" } ?? "any floor"
            lines.append("× \(item.stackCount) of the same kind — "
                         + "the extra copies: any upgrade, \(floors)")
        }
        if let error { lines.append(error) }
        return lines.joined(separator: "\n")
    }

    // MARK: The context menu — the gestures as words

    @ViewBuilder private var menu: some View {
        Button("Edit…") { if let live = liveIndex { onEdit(live) } }
        let others = otherChips
        if !others.isEmpty {
            Menu("Either/or with…") {
                ForEach(others) { other in
                    Button(other.label) {
                        guard let live = liveIndex else { return }
                        requirements = requirements.joinAlternatives(source: live, target: other.id)
                    }
                }
            }
        }
        // A cluster spanning two categories cannot anchor a stack, so it is
        // not offered one.
        if requirements.canStack(liveItem ?? item) {
            Divider()
            Menu("How many") {
                ForEach(1...SearchLimits.stackMax, id: \.self) { count in
                    Toggle("\(count)", isOn: Binding(
                        get: { (liveItem?.stackCount ?? 1) == count },
                        set: { on in
                            guard on, let fresh = liveItem else { return }
                            requirements = requirements.setStackCount(fresh, count)
                        }))
                }
            }
        }
        // Only a lone concrete chip can count levels: "up to N rings reaching
        // 5 levels" needs an item to be N of, and a cluster is one slot.
        if item.cluster == nil, requirement.item != nil, item.stackCount > 1 {
            Button(item.total == nil ? "Count levels together" : "Stop counting levels") {
                guard let fresh = liveItem else { return }
                requirements = requirements.setStackTotal(
                    fresh, fresh.total == nil ? max(1, fresh.stackCount) : nil)
            }
        }
        if inCluster {
            Divider()
            Button("On its own") {
                if let live = liveIndex { requirements = requirements.detach(live) }
            }
        }
        Divider()
        Button("Remove", role: .destructive) { removeSelf() }
    }

    /// The other board entries, named as the menu lists them.
    private var otherChips: [ChipTarget] {
        requirements.boardItems().compactMap { entry in
            guard !entry.members.contains(index) else { return nil }
            return ChipTarget(id: entry.anchor,
                              label: entry.members.map { chipName(requirements[$0]) }
                                  .joined(separator: " or "))
        }
    }

    private func removeSelf() {
        guard let live = liveIndex, let fresh = requirements.boardItem(holding: live) else { return }
        requirements = fresh.cluster != nil
            ? requirements.removeMember(live)
            : requirements.removeItem(fresh)
    }
}

/// One entry of the "Either/or with…" menu.
private struct ChipTarget: Identifiable {
    let id: Int
    let label: String
}

/// The stack badges: how many of the chip (×N, or ≤N once the levels are being
/// counted) and the combined level (Σ ≥ T). Clicking one adjusts it in place.
private struct StackBadgesView: View {
    @Binding var requirements: [ItemRequirement]
    /// The anchor's key: the board entry is looked up again on every change,
    /// so a badge keeps working while its own stepper reshapes the list.
    let anchorKey: Int64
    @State private var editingCount = false
    @State private var editingTotal = false

    private var item: BoardItem? {
        guard let index = requirements.firstIndex(where: { $0.key == anchorKey }) else { return nil }
        return requirements.boardItem(holding: index)
    }
    private var count: Int { item?.stackCount ?? 1 }
    private var total: Int? { item?.total }
    private var canGrow: Bool { item.map { requirements.canStack($0) } ?? false }

    var body: some View {
        HStack(spacing: 3) {
            if count > 1 {
                Button { editingCount = true } label: {
                    badge(total == nil ? "×\(count)" : "≤\(count)", tint: .shatteredMint)
                }
                .buttonStyle(.plain)
                .help(total == nil ? "\(count) of the same kind" : "Up to \(count) items")
                .popover(isPresented: $editingCount, arrowEdge: .bottom) {
                    // A hand-written document can hand a mixed cluster a
                    // stack; it may then only be shrunk, never grown.
                    Stepper(value: countBinding, in: 1...(canGrow ? SearchLimits.stackMax : count)) {
                        Text("How many: \(count)").monospacedDigit()
                    }
                    .padding(14).frame(width: 200)
                }
            }
            if let total {
                Button { editingTotal = true } label: { badge("Σ ≥ \(total)", tint: .shatteredYellow) }
                    .buttonStyle(.plain)
                    .help("Levels add to at least \(total) (a +0 item counts 1)")
                    .popover(isPresented: $editingTotal, arrowEdge: .bottom) {
                        Stepper(value: totalBinding, in: 1...max(1, capacity)) {
                            Text("Combined level: ≥ \(total)").monospacedDigit()
                        }
                        .padding(14).frame(width: 210)
                    }
            }
        }
    }

    private func badge(_ text: String, tint: Color) -> some View {
        Text(text)
            .font(.system(size: 11, weight: .bold, design: .monospaced))
            .foregroundStyle(Color.black)
            .padding(.horizontal, 6).frame(height: 18)
            .background(tint, in: Capsule())
    }

    private var countBinding: Binding<Int> {
        Binding(get: { count }, set: { value in
            guard let item else { return }
            requirements = requirements.setStackCount(item, value)
        })
    }

    private var totalBinding: Binding<Int> {
        Binding(get: { total ?? 1 }, set: { value in
            guard let item else { return }
            requirements = requirements.setStackTotal(item, value)
        })
    }

    /// The highest combined level this stack could reach: each member counts
    /// its upgrade plus one.
    private var capacity: Int {
        guard let item else { return 1 }
        return ([item.anchor] + item.extras)
            .filter(requirements.indices.contains)
            .reduce(0) { $0 + requirements[$1].maximumLevel }
    }
}

/// The dashed "+ Add" chip that closes the board, and ⌘N with it.
private struct AddChipView: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: "plus").font(.system(size: 10, weight: .bold))
                Text("Add").font(.system(size: 12, weight: .semibold))
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal, 11).frame(height: 30)
            .overlay(Capsule().strokeBorder(Color.secondary.opacity(0.45),
                                            style: StrokeStyle(lineWidth: 1, dash: [4, 3])))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .keyboardShortcut("n", modifiers: .command)
        .help("Add a requirement")
    }
}

// MARK: - Chip vocabulary

/// A qualifier beside a chip's name; the upgrade is tinted apart from the rest.
private struct ChipTag: Hashable {
    let text: String
    var upgrade = false
}

/// The short name a chip shows: the item, or its wildcard family.
private func chipName(_ requirement: ItemRequirement) -> String {
    if let item = requirement.item { return item.name }
    return switch requirement.kind {
    case .weapon: "Any weapon"
    case .meleeWeapon: "Any melee"
    case .thrownWeapon: "Any thrown"
    case .armor: "Any armor"
    case .wand: "Any wand"
    case .ring: "Any ring"
    }
}

/// The tiny qualifiers beside a chip's name: tier, upgrade, floor. A tier only
/// ever narrows a wildcard, so a named item never carries one.
private func chipTags(_ requirement: ItemRequirement) -> [ChipTag] {
    var tags: [ChipTag] = []
    if requirement.item == nil {
        switch requirement.tierMatch {
        case .any: break
        case .exactly: tags.append(ChipTag(text: "T\(requirement.tier)"))
        case .atLeast: tags.append(ChipTag(text: "T\(requirement.tier)+"))
        case .atMost: tags.append(ChipTag(text: "T≤\(requirement.tier)"))
        }
    }
    switch requirement.upgradeMatch {
    case .any: break
    case .exactly: tags.append(ChipTag(text: "+\(requirement.upgrade)", upgrade: true))
    case .atLeast: tags.append(ChipTag(text: "+\(requirement.upgrade)↑", upgrade: true))
    }
    if let depth = requirement.maximumDepth { tags.append(ChipTag(text: "F≤\(depth)")) }
    return tags
}

/// The requirement a chip drag carries: its key, written on the pasteboard.
/// Text from anywhere else parses to no key the query holds and is refused.
private func draggedIndex(_ payload: [String], in requirements: [ItemRequirement]) -> Int? {
    guard let key = payload.first.flatMap({ Int64($0) }) else { return nil }
    return requirements.firstIndex { $0.key == key }
}

/// Which requirements the query's cross-requirement validation blames, and
/// what it says of them. The rules within one requirement are enforced by the
/// model's own initialiser, so a chip can only ever be wrong about the company
/// it keeps — a stack of mixed categories, a total nothing can reach.
private func boardErrors(_ requirements: [ItemRequirement]) -> [Int: String] {
    do {
        try requirements.validateGroups()
        return [:]
    } catch {
        guard let failure = error as? ModelValidationError,
              let message = failure.errorDescription else { return [:] }
        let blames: (ItemRequirement) -> Bool
        switch failure {
        case .identityGroupMixedKinds(let group), .identityGroupOverconstrained(let group):
            blames = { $0.identityGroup == group }
        case .levelSumMismatch(let group), .levelSumUnattainable(let group, _, _):
            blames = { $0.levelSum?.group == group }
        default:
            return [:]
        }
        return requirements.enumerated().reduce(into: [:]) { found, entry in
            if blames(entry.element) { found[entry.offset] = message }
        }
    }
}

// MARK: - Requirement editor

/// The editor's effect choice: any item, any enchanted item, or a chosen set.
private enum EffectMode: Hashable {
    case any, anyEnchantment, specific
}

private struct RequirementEditor: View {
    let original: ItemRequirement
    let isNew: Bool
    /// The chip's stack as the board holds it. Its count and combined level
    /// belong to the whole chip, so a cluster member never sees them.
    let stack: StackShape
    let onFinish: (EditorResult?) -> Void
    @State private var kind: ItemKind
    @State private var itemID: String
    @State private var tierMatch: TierMatch
    @State private var tier: Int
    @State private var match: UpgradeMatch
    @State private var upgrade: Int
    @State private var effectMode: EffectMode
    @State private var selectedEffects: Set<String>
    @State private var sourceRaw: Int
    @State private var maximumDepth: Int
    @State private var requireUncursed: Bool
    /// How many items the chip asks for, and what its stack's copies carry.
    @State private var count: Int
    @State private var total: Int?
    @State private var copyDepth: Int?
    @State private var validationMessage: String?

    init(requirement: ItemRequirement, isNew: Bool, stack: StackShape,
         onFinish: @escaping (EditorResult?) -> Void) {
        original = requirement; self.isNew = isNew; self.stack = stack; self.onFinish = onFinish
        _kind = State(initialValue: requirement.kind); _itemID = State(initialValue: requirement.item?.id ?? "")
        _tierMatch = State(initialValue: requirement.tierMatch)
        _tier = State(initialValue: max(SearchLimits.exactTiers.lowerBound, requirement.tier))
        _match = State(initialValue: requirement.upgradeMatch)
        let maximumUpgrade = requirement.maximumUpgrade
        let initialUpgrade = switch requirement.upgradeMatch {
        case .any: 0
        case .exactly: max(1, min(requirement.upgrade, maximumUpgrade))
        case .atLeast: max(1, min(requirement.upgrade, maximumUpgrade - 1))
        }
        _upgrade = State(initialValue: initialUpgrade)
        let mode: EffectMode = switch requirement.effect {
        case .any: .any
        case .anyEnchantment: .anyEnchantment
        case .oneOf: .specific
        }
        _effectMode = State(initialValue: mode)
        _selectedEffects = State(initialValue: Set(requirement.effect.names))
        _sourceRaw = State(initialValue: requirement.source.map { $0.rawValue + 1 } ?? 0)
        _maximumDepth = State(initialValue: requirement.maximumDepth ?? 0)
        _requireUncursed = State(initialValue: requirement.requireUncursed)
        _count = State(initialValue: stack.count)
        _total = State(initialValue: stack.total)
        _copyDepth = State(initialValue: stack.copyDepth)
    }

    var body: some View {
        VStack(spacing: 0) {
            Text(isNew ? "New Requirement" : "Edit Requirement")
                .font(.headline).padding(.top, 14).padding(.bottom, 4)
            Form {
                Section("Item") {
                    Picker("Category", selection: Binding(get: { kind.family }, set: { kind = $0 })) {
                        ForEach([ItemKind.weapon, .armor, .wand, .ring], id: \.self) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .onChange(of: kind) { previous, value in
                        if previous.family != value.family {
                            itemID = ""; tierMatch = .any; tier = 2
                            effectMode = .any; selectedEffects = []; normalizeUpgrade()
                        } else if let item = ItemCatalog.findById(itemID), !value.accepts(item) {
                            itemID = ""
                        }
                    }
                    if kind.family == .weapon {
                        Picker("Weapon type", selection: $kind) {
                            Text("Any").tag(ItemKind.weapon)
                            Text("Melee").tag(ItemKind.meleeWeapon)
                            Text("Thrown").tag(ItemKind.thrownWeapon)
                        }
                        .pickerStyle(.segmented)
                    }
                    Picker("Item", selection: $itemID) {
                        Text("Any \(kind.singularLabel)").tag("")
                        if kind.family == .weapon {
                            // Tier-1 weapons are starting gear and never spawn in the dungeon.
                            ForEach(SearchLimits.exactTiers, id: \.self) { tier in
                                Section("Tier \(tier)") {
                                    ForEach(ItemCatalog.forKind(kind).filter { $0.tier == tier }) { item in
                                        Label { Text(item.name) } icon: {
                                            ItemSpriteIcon(item: item)
                                        }.tag(item.id)
                                    }
                                }
                            }
                        } else {
                            ForEach(ItemCatalog.forKind(kind).filter { $0.tier != 1 }) { item in
                                Label { Text(item.name) } icon: {
                                    ItemSpriteIcon(item: item)
                                }.tag(item.id)
                            }
                        }
                    }
                    .onChange(of: itemID) { _, value in
                        if value.isEmpty { total = nil } else { tierMatch = .any }
                        normalizeUpgrade()
                    }
                    if itemID.isEmpty && (kind.family == .weapon || kind.family == .armor) {
                        Picker("Tier", selection: $tierMatch) {
                            ForEach(TierMatch.allCases, id: \.self) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        .onChange(of: tierMatch) { _, value in
                            if value == .atLeast || value == .atMost {
                                tier = max(SearchLimits.boundedTiers.lowerBound, min(tier, SearchLimits.boundedTiers.upperBound))
                            }
                            normalizeUpgrade()
                        }
                        .onChange(of: tier) { normalizeUpgrade() }
                        if tierMatch == .exactly {
                            VStack(alignment: .leading, spacing: 2) {
                                LabeledContent("Exact tier") {
                                    Text("Tier \(tier)")
                                        .monospacedDigit().foregroundStyle(.secondary)
                                }
                                Slider(value: intBinding($tier),
                                       in: Double(SearchLimits.exactTiers.lowerBound)...Double(SearchLimits.exactTiers.upperBound),
                                       step: 1)
                            }
                        } else if tierMatch == .atLeast || tierMatch == .atMost {
                            Picker(tierMatch == .atLeast ? "Minimum tier" : "Maximum tier",
                                   selection: $tier) {
                                ForEach(SearchLimits.boundedTiers, id: \.self) { option in
                                    Text(tierMatch == .atLeast ? "Tier \(option) or higher" :
                                        "Tier \(option) or lower").tag(option)
                                }
                            }
                            .pickerStyle(.menu)
                        }
                    }
                }
                // A combined level speaks for the whole stack, so its members
                // take any upgrade and the per-item choice has nothing to say.
                if effectiveTotal == nil {
                    Section("Upgrade level") {
                        Picker("Predicate", selection: $match) {
                            ForEach(UpgradeMatch.allCases, id: \.self) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        .onChange(of: match) { normalizeUpgrade() }
                        if match == .exactly {
                            VStack(alignment: .leading, spacing: 2) {
                                LabeledContent("Exactly") {
                                    Text("+\(upgrade)").monospacedDigit().foregroundStyle(.secondary)
                                }
                                Slider(value: intBinding($upgrade),
                                       in: 1...Double(maximumUpgrade), step: 1)
                            }
                        } else if match == .atLeast {
                            if kind == .ring {
                                VStack(alignment: .leading, spacing: 2) {
                                    LabeledContent("At least") {
                                        Text("+\(upgrade)").monospacedDigit().foregroundStyle(.secondary)
                                    }
                                    Slider(value: intBinding($upgrade),
                                           in: 1...Double(maximumUpgrade - 1), step: 1)
                                }
                            } else {
                                Picker("Minimum upgrade", selection: $upgrade) {
                                    ForEach(1..<maximumUpgrade, id: \.self) { option in
                                        Text("+\(option) or higher").tag(option)
                                    }
                                }
                                .pickerStyle(.menu)
                            }
                        }
                    }
                }
                if !stack.inCluster {
                    Section("Total item count") {
                        Stepper(value: $count, in: 1...SearchLimits.stackMax) {
                            LabeledContent("How many") {
                                Text("×\(count)").monospacedDigit().foregroundStyle(.secondary)
                            }
                        }
                        .onChange(of: count) { _, value in
                            if value < 2 { total = nil }
                            else if let current = total { total = min(current, totalCapacity) }
                        }
                        if count > 1 && effectiveTotal == nil {
                            // The chip's own floor limit describes one copy; the
                            // extras are placed by a bound of their own.
                            Toggle("Limit the extra copies to a floor", isOn: Binding(
                                get: { copyDepth != nil },
                                set: { copyDepth = $0 ? 4 : nil }
                            ))
                            if let depth = copyDepth {
                                LabeledContent("Copies within first") {
                                    Text("\(depth) floors").monospacedDigit().foregroundStyle(.secondary)
                                }
                                Slider(value: floorLimitBinding(Binding(
                                    get: { copyDepth ?? 4 }, set: { copyDepth = $0 })),
                                       in: 0...Double(FloorLimits.options.count - 1), step: 1)
                                    .accessibilityValue(Text("\(depth) floors"))
                            }
                        }
                        if totalable {
                            Toggle("Count levels together", isOn: Binding(
                                get: { total != nil },
                                set: { total = $0 ? min(max(count, 1), totalCapacity) : nil }
                            ))
                            if let value = total {
                                LabeledContent("Levels reach") {
                                    Text("≥ \(value) across up to \(count)")
                                        .monospacedDigit().foregroundStyle(.secondary)
                                }
                                Slider(value: intBinding(Binding(
                                    get: { min(value, totalCapacity) }, set: { total = $0 })),
                                       in: 1...Double(max(1, totalCapacity)), step: 1)
                                Text("Up to \(count) of the item, each counting its upgrade plus "
                                     + "one; any subset reaching the total satisfies it.")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                if let label = kind.modifierLabel {
                    Section(label) {
                        // Labelled by hand rather than by the Picker: a grouped
                        // Form pins a labelled control to its trailing column,
                        // which leaves the segments short of the leading edge
                        // the effect grids below them start at.
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Effect").font(.caption).foregroundStyle(.secondary)
                            Picker("Effect", selection: $effectMode) {
                                Text("Any").tag(EffectMode.any)
                                Text("Any \(label.lowercased())").tag(EffectMode.anyEnchantment)
                                Text("Specific…").tag(EffectMode.specific)
                            }
                            .pickerStyle(.segmented)
                            .labelsHidden()
                        }
                        if effectMode == .specific {
                            effectGrid(kind.family == .weapon ? "Enchantments" : "Glyphs",
                                       names: kind.family == .weapon ? ItemCatalog.enchantments : ItemCatalog.glyphs)
                            // Curses cannot be on an uncursed item, so they hide with it.
                            if !requireUncursed {
                                effectGrid("Curses", names: ItemCatalog.cursesFor(kind))
                            }
                        }
                    }
                }
                Section {
                    Toggle("Require uncursed", isOn: $requireUncursed)
                        .toggleStyle(.checkbox)
                        .onChange(of: requireUncursed) { _, value in
                            if value { selectedEffects.subtract(ItemCatalog.cursesFor(kind)) }
                        }
                    Picker("Source", selection: $sourceRaw) {
                        Text("Any").tag(0)
                        ForEach(ScoutItemSource.allCases, id: \.rawValue) { Text($0.label).tag($0.rawValue + 1) }
                    }
                    Toggle("Limit this item to a floor", isOn: Binding(
                        get: { maximumDepth != 0 },
                        set: { maximumDepth = $0 ? 4 : 0 }
                    ))
                    if maximumDepth != 0 {
                        LabeledContent("Within first") {
                            Text("\(maximumDepth) floors").monospacedDigit().foregroundStyle(.secondary)
                        }
                        Slider(value: floorLimitBinding($maximumDepth),
                               in: 0...Double(FloorLimits.options.count - 1), step: 1)
                            .accessibilityValue(Text("\(maximumDepth) floors"))
                    }
                }
            }
            .formStyle(.grouped)
            Divider()
            HStack {
                Button("Cancel") { onFinish(nil) }.keyboardShortcut(.cancelAction)
                if let validationMessage {
                    Text(validationMessage).font(.caption).foregroundStyle(.orange)
                        .lineLimit(2).padding(.leading, 8)
                }
                Spacer()
                Button(isNew ? "Add" : "Save") { save() }
                    .buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
            }.padding(12)
        }
        .frame(width: 480, height: kind.modifierLabel == nil ? 580 : 660)
    }

    /// A combined level is a property of a concrete stack of two or more: it
    /// needs an item to be N of, and a cluster is one slot, not a stack.
    private var totalable: Bool { !stack.inCluster && !itemID.isEmpty && count > 1 }
    private var effectiveTotal: Int? { totalable ? total : nil }
    /// The most levels the stack could add up to, its members taking any
    /// upgrade: each counts the family's cap plus one.
    private var totalCapacity: Int { count * (maximumUpgrade + 1) }

    /// The highest upgrade the draft can name: only a tier-4 weapon is
    /// levelled past `SearchLimits.maxUpgradeAnyTier`, so naming an item of
    /// another tier or filtering tier 4 away lowers the ceiling.
    private var maximumUpgrade: Int {
        SearchLimits.maximumUpgrade(kind: kind, item: itemID.isEmpty ? nil : ItemCatalog.findById(itemID),
                                    tier: tier, tierMatch: tierMatch)
    }

    private func effectGrid(_ title: String, names: [String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), alignment: .leading), count: 3),
                      alignment: .leading, spacing: 4) {
                ForEach(names, id: \.self) { name in
                    Toggle(name, isOn: Binding(
                        get: { selectedEffects.contains(name) },
                        set: { if $0 { selectedEffects.insert(name) } else { selectedEffects.remove(name) } }
                    )).toggleStyle(.checkbox)
                }
            }
        }
    }

    private func normalizeUpgrade() {
        switch match {
        case .any:
            upgrade = 0
        case .exactly:
            upgrade = max(1, min(upgrade, maximumUpgrade))
        case .atLeast:
            upgrade = max(1, min(upgrade, maximumUpgrade - 1))
        }
    }
    private func save() {
        let item = itemID.isEmpty ? nil : ItemCatalog.findById(itemID)
        let effect: EffectFilter = switch effectMode {
        case .any: .any
        case .anyEnchantment: .anyEnchantment
        case .specific: .oneOf(Array(selectedEffects))
        }
        if effectMode == .specific && selectedEffects.isEmpty {
            validationMessage = "Choose at least one \((kind.modifierLabel ?? "effect").lowercased())"
            return
        }
        do {
            // The relationships are the board's to write: `applyEdit` turns the
            // count and total below into the stack's own encoding, so the row
            // saved here carries no group of its own.
            let value = try ItemRequirement(key: original.key, item: item, upgrade: upgrade,
                effect: effect, kind: kind,
                tier: tierMatch == .any ? 0 : tier, tierMatch: tierMatch, upgradeMatch: match,
                source: sourceRaw == 0 ? nil : ScoutItemSource(rawValue: sourceRaw - 1),
                maximumDepth: maximumDepth == 0 ? nil : maximumDepth,
                requireUncursed: requireUncursed,
                alternativeGroup: original.alternativeGroup)
            onFinish(EditorResult(
                requirement: value,
                count: stack.inCluster ? 1 : count,
                total: effectiveTotal,
                copyDepth: stack.inCluster || count < 2 || effectiveTotal != nil ? nil : copyDepth))
        } catch {
            validationMessage = (error as? LocalizedError)?.errorDescription ?? "The requirement is invalid"
        }
    }
}

// MARK: - Results

/// `square.and.arrow.up`/`down` carry more empty space above the glyph than
/// below it, so a toolbar label leaves them looking low against their capsule.
/// Lifting only the icon optically centres it without moving the title.
/// Both sides need an inset: hover highlights each button separately, so
/// padding one side alone draws the highlight hard against the other end of
/// the label. It also keeps the group's shared Liquid Glass container off the
/// outer labels.
///
/// The 6pt was tuned against "Import…"/"Export…", whose trailing dots sit on
/// the baseline and read as extra room on the right — enough to balance the
/// icon's own side bearing on the left. A title without an ellipsis ("Clear")
/// ends hard against the inset, so the same value leaves it visibly
/// left-heavy; `trailingEllipsis: false` trims the leading side by the
/// ellipsis's optical width to even the two gaps back out.
private struct ToolbarActionLabelStyle: LabelStyle {
    /// Room a trailing ellipsis contributes on the right, which a title
    /// without one has to reclaim from the leading inset instead.
    private static let ellipsisAllowance: CGFloat = 2
    /// Whether this label's title ends in an ellipsis.
    var trailingEllipsis = true

    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 5) {
            configuration.icon.offset(y: -1)
            configuration.title
        }
        .padding(.leading, trailingEllipsis ? 6 : 6 - Self.ellipsisAllowance)
        .padding(.trailing, 6)
    }
}

/// Plain-text JSON payload handed to `fileExporter`.
private struct ResultsFileDocument: FileDocument {
    static let readableContentTypes: [UTType] = [.json]
    var text: String

    init(text: String) { self.text = text }
    init(configuration: ReadConfiguration) throws {
        text = String(data: configuration.file.regularFileContents ?? Data(), encoding: .utf8) ?? ""
    }
    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: Data(text.utf8))
    }
}

/// A displayed result with its 1-based row number precomputed: numbering via
/// `firstIndex(of:)` in the cell is quadratic over the table, which a
/// cap-sized list turns into visible main-thread stalls.
private struct NumberedResult: Identifiable {
    let number: Int
    let result: SeedResult
    var id: String { result.id }
}

private struct ResultsView: View {
    let controller: SearchController
    let scout: (String) -> Void
    var body: some View {
        let rows = controller.results.enumerated().map { NumberedResult(number: $0.offset + 1, result: $0.element) }
        VStack(alignment: .leading, spacing: 10) {
            statusBody.padding([.horizontal, .top])
            Table(rows, selection: Bindable(controller).selectedSeed) {
                TableColumn("#") { row in Text("\(row.number)").foregroundStyle(.secondary) }.width(45)
                TableColumn("Seed") { row in
                    Text(row.result.seed).font(.system(.body, design: .monospaced))
                        .contextMenu { Button("Copy Seed") { copy(row.result.seed) }; Button("Scout Seed") { scout(row.result.seed) } }
                }
            }
            Button("Copy Selected") { if let seed = controller.selectedSeed { copy(seed) } }
                .keyboardShortcut("c", modifiers: .command).hidden()
        }.navigationTitle("Results")
    }
    @ViewBuilder private var statusBody: some View {
        if controller.isImported {
            HStack(spacing: 8) {
                Text("Imported").font(.caption.bold())
                    .padding(.horizontal, 10).padding(.vertical, 4)
                    .background(.quaternary, in: Capsule())
                Text(importedCaption).font(.caption).foregroundStyle(.secondary)
            }
        }
        else if controller.state == nil { Text("Add requirements, then press Start Search.").foregroundStyle(.secondary) }
        else if controller.isRunning {
            VStack(alignment: .leading, spacing: 2) {
                Text("Seed match probability: \(NumberFormat.probabilityPercent(controller.matchProbability)) " +
                     "TTS @ \(NumberFormat.seedRate(controller.seedsPerSecond)) seeds/s: " +
                     NumberFormat.estimateDuration(controller.timeToSeed))
                    .font(.caption).foregroundStyle(.secondary)
                Text("Time elapsed: \(NumberFormat.duration(controller.elapsed)) · " +
                     "Seeds searched: \(NumberFormat.si(Double(controller.scannedSeeds)))")
                    .font(.caption2).foregroundStyle(.tertiary)
            }
        } else if controller.isImpossibleQuery {
            VStack(alignment: .leading, spacing: 4) {
                Text("Impossible query").font(.caption.bold())
                    .padding(.horizontal, 10).padding(.vertical, 4)
                    .foregroundStyle(.orange).background(.quaternary, in: Capsule())
                Text("No seed can satisfy these requirements within the current floor limit. " +
                     "Quest-reward-only items need their quest floors in range: +3 wands floor 9, " +
                     "and, from the Imp's vault, +3/+4 rings, +4 wands and armor " +
                     "and +4/+5 weapons floor 19.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        } else if let state = controller.state {
            HStack(spacing: 8) {
                Text(state == .failed ? "Failed (error \(controller.errorCode))" : state == .completed ? "Completed" : "Cancelled")
                    .font(.caption.bold()).padding(.horizontal, 10).padding(.vertical, 4).background(.quaternary, in: Capsule())
                // A concluded run keeps its counter, except where nothing was
                // scanned: a filter-only refine never scans, and "0 seeds
                // searched" would read as a malfunction rather than as the
                // phase it is. (The impossible query, the other way to end at
                // zero, is handled above.) A failed run's count is unknown.
                if state != .failed && controller.scannedSeeds > 0 {
                    Text("\(NumberFormat.si(Double(controller.scannedSeeds))) seeds searched")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }
    private var importedCaption: String {
        let count = controller.results.count
        var caption = count == 0
            ? "the imported file contained no seeds"
            : "\(count) seed\(count == 1 ? "" : "s") loaded from file"
        if controller.importedDropped > 0 {
            caption += " · \(controller.importedDropped) duplicate or over-limit "
                + "entr\(controller.importedDropped == 1 ? "y" : "ies") dropped"
        }
        return caption
    }
    private func copy(_ seed: String) { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(seed, forType: .string) }
}

// MARK: - Scout / seed detail

@MainActor @Observable private final class ScoutViewModel {
    var input = ""
    var world: ScoutWorld?
    var error: String?
    var loading = false
    /// Anchor for result navigation: the seed of the most recent scout
    /// request, set synchronously so rapid steps chain even while a scout is
    /// in flight. A failed request falls back to the rendered manifest's seed.
    private(set) var requestedSeed: String?
    private var generation = 0
    private let engine = ProductionSeedFinderEngine()
    func scout(_ seed: String? = nil, challenges: Int) {
        if let seed { input = SeedCode.formatInput(seed) }
        guard SeedCode.isCanonical(input) else { error = "Seed must use XXX-XXX-XXX format"; return }
        let requested = input; requestedSeed = requested; loading = true; error = nil
        // Only the latest request may publish: unsequenced completions would
        // let an older manifest land under a newer position indicator.
        generation += 1
        let current = generation
        Task {
            do {
                let scouted = try await engine.scoutSeed(requested, challenges: challenges)
                guard current == generation else { return }
                world = scouted
            } catch {
                guard current == generation else { return }
                self.error = error.localizedDescription
                requestedSeed = world?.seed
            }
            loading = false
        }
    }
}

/// Position of the scouted seed within the ordered search results.
private struct ResultPosition {
    let index: Int
    let total: Int
}

private struct SeedDetailView: View {
    @Bindable var model: ScoutViewModel
    let requirements: [ItemRequirement]
    let maximumDepth: Int
    let excludeBlacksmithRewards: Bool
    let challenges: Int
    let resultPosition: ResultPosition?
    let onNavigateResult: (Int) -> Void
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            if let world = model.world {
                manifest(world)
            } else {
                ContentUnavailableView("No seed scouted", systemImage: "map",
                    description: Text("Enter a canonical seed, or select a search result, to inspect its item manifest."))
            }
            Button("") { focused = true }.keyboardShortcut("l", modifiers: .command).hidden()
        }
        .navigationTitle("Seed Detail")
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                TextField("AAA-AAA-AAA", text: $model.input).font(.system(size: 20, design: .monospaced)).focused($focused)
                    .onChange(of: model.input) { _, value in let formatted = SeedCode.formatInput(value); if formatted != value { model.input = formatted } }
                    .onSubmit { model.scout(challenges: challenges) }
                Button("Scout") { model.scout(challenges: challenges) }.disabled(!SeedCode.isCanonical(model.input))
                if let seed = model.world?.seed { Button("Copy") { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(seed, forType: .string) } }
                if model.loading { ProgressView().controlSize(.small) }
            }
            if let error = model.error { Text(error).foregroundStyle(.red).font(.caption) }
            if let position = resultPosition {
                HStack(spacing: 6) {
                    Button { onNavigateResult(-1) } label: { Image(systemName: "chevron.left") }
                        .buttonStyle(.borderless).disabled(position.index == 0)
                        .accessibilityLabel("Previous result")
                        .help("Scout the previous search result (K)")
                    Text("Result \(position.index + 1) of \(position.total)")
                        .font(.caption).monospacedDigit().foregroundStyle(.secondary)
                    Button { onNavigateResult(1) } label: { Image(systemName: "chevron.right") }
                        .buttonStyle(.borderless).disabled(position.index + 1 >= position.total)
                        .accessibilityLabel("Next result")
                        .help("Scout the next search result (J)")
                    Text("J / K").font(.caption2).foregroundStyle(.tertiary)
                }
            }
        }.padding(.horizontal).padding(.top, 10).padding(.bottom, 8)
    }

    /// The engine's own marks for the scouted world, taken from the same
    /// request the scout used. Without requirements (or with a query the
    /// engine refuses) there is nothing to mark.
    private func engineMatches(in world: ScoutWorld) -> ScoutMatches? {
        guard let query = try? SearchRequest(
                  requirements: requirements, maximumDepth: maximumDepth,
                  excludeBlacksmithRewards: excludeBlacksmithRewards, challenges: challenges)
        else { return nil }
        return try? ScoutMatches.mark(seed: world.seed, challenges: challenges, query: query)
    }

    private func manifest(_ world: ScoutWorld) -> some View {
        let byDepth = Dictionary(grouping: world.items, by: \.depth)
        let depths = byDepth.keys.sorted()
        let marks = engineMatches(in: world)
        let matches = marks?.matched ?? []
        // Slots, not rows: an "any of these" group counts once.
        let matched = marks?.matchedRequirements ?? 0
        let total = marks?.totalRequirements ?? requirements.slotCount
        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 4) {
                Text("\(world.items.count) items across \(depths.count) floors")
                if !requirements.isEmpty {
                    Text("·")
                    Label("\(matched) of \(total) requirement\(total == 1 ? "" : "s")", systemImage: "checkmark.circle")
                        .foregroundStyle(matched == 0 ? Color.secondary : Color.shatteredMint)
                }
            }
            .font(.caption).foregroundStyle(.secondary)
            .padding(.horizontal).padding(.vertical, 6)
            if !world.quests.isEmpty {
                FlowLayout(spacing: 6, lineSpacing: 6) {
                    ForEach(world.quests) { quest in
                        HStack(spacing: 4) {
                            Text(quest.variant.label).font(.caption.bold())
                            Text("\(quest.kind.giverLabel) · F\(quest.depth)")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        .padding(.horizontal, 7).padding(.vertical, 2)
                        .background(Self.questTint(quest.kind).opacity(0.12), in: Capsule())
                    }
                }.padding(.horizontal).padding(.bottom, 6)
            }
            List {
                ForEach(depths, id: \.self) { depth in
                    Section {
                        ForEach(Array(world.items.enumerated()).filter { $0.element.depth == depth }, id: \.offset) { entry in
                            ScoutItemRow(item: entry.element, ringGems: world.ringGems,
                                         matches: matches.contains(entry.offset))
                        }
                    } header: {
                        HStack {
                            Text("Floor \(depth)")
                            Text(Self.region(depth)).foregroundStyle(.tertiary)
                            if let quest = world.quests.first(where: { $0.depth == depth }) {
                                Text("· \(quest.variant.label)").foregroundStyle(.tertiary)
                            }
                        }
                    }
                }
            }
        }
    }

    private static func questTint(_ kind: ScoutQuestKind) -> Color {
        switch kind {
        case .ghost: .teal
        case .wandmaker: .purple
        case .blacksmith: .orange
        case .imp: .yellow
        }
    }

    private static func region(_ depth: Int) -> String {
        switch depth {
        case ..<6: "Sewers"
        case ..<11: "Prison"
        case ..<16: "Caves"
        case ..<21: "Dwarven City"
        default: "Demon Halls"
        }
    }

}

// MARK: - Pane furniture

/// A section's name within the query pane, in the sidebar's old voice.
private struct SectionLabel: View {
    let title: String
    init(_ title: String) { self.title = title }

    var body: some View {
        Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.secondary)
    }
}

/// A small count beside a section's name, for how many of a thing it holds.
private struct CountBadge: View {
    let count: Int
    init(_ count: Int) { self.count = count }

    var body: some View {
        Text("\(count)")
            .font(.system(size: 11, weight: .bold, design: .monospaced))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 5).frame(minWidth: 18, minHeight: 18)
            .background(.quaternary, in: Capsule())
            .accessibilityLabel("\(count) requirement\(count == 1 ? "" : "s")")
    }
}

/// One titled group of the query pane's settings: a section label over a
/// group box whose contents fill its column, left-aligned.
private struct SettingsGroup<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content
    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel(title)
            GroupBox {
                VStack(alignment: .leading, spacing: 8) { content }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(4)
            }
        }
    }
}

/// The small print under a setting; wraps rather than truncates, since a
/// column is narrower than the sentence.
private struct SettingsCaption: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).font(.caption).foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// Lays its subviews out left to right at their natural size, starting a new
/// row whenever the next one would overflow. SwiftUI ships no wrapping stack.
private struct FlowLayout: Layout {
    var spacing: CGFloat
    var lineSpacing: CGFloat

    /// Every subview's origin relative to the layout's top-left, plus the size
    /// the resulting rows occupy.
    ///
    /// Subviews sit on their row's centre line rather than its top edge. A
    /// cluster is a chip plus the inset its dashed capsule needs, so it stands
    /// taller than the chips beside it; centred, its members line up with them
    /// instead of hanging that inset lower.
    private func flow(_ subviews: Subviews, width: CGFloat) -> (origins: [CGPoint], size: CGSize) {
        var origins: [CGPoint] = []
        var heights: [CGFloat] = []
        var size = CGSize.zero
        var cursor = CGPoint.zero
        var rowHeight: CGFloat = 0
        var rowStart = 0
        // Only once a row is closed is its height — and so its centre — known.
        func centreRow() {
            for index in rowStart..<origins.count {
                origins[index].y += (rowHeight - heights[index]) / 2
            }
        }
        for subview in subviews {
            let item = subview.sizeThatFits(.unspecified)
            // A row always keeps its first subview, however wide it is.
            if cursor.x > 0, cursor.x + item.width > width {
                centreRow()
                rowStart = origins.count
                cursor = CGPoint(x: 0, y: cursor.y + rowHeight + lineSpacing)
                rowHeight = 0
            }
            origins.append(cursor)
            heights.append(item.height)
            cursor.x += item.width + spacing
            rowHeight = max(rowHeight, item.height)
            size.width = max(size.width, cursor.x - spacing)
        }
        centreRow()
        size.height = cursor.y + rowHeight
        return (origins, size)
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        flow(subviews, width: proposal.width ?? .infinity).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for (subview, origin) in zip(subviews, flow(subviews, width: bounds.width).origins) {
            subview.place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y),
                          proposal: .unspecified)
        }
    }
}

private struct ScoutItemRow: View {
    let item: ScoutItem
    /// The scouted run's gems: this row shows an item that seed actually
    /// holds, so a ring must be drawn in the gem that run gave its class.
    let ringGems: RingGems
    let matches: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            ItemSpriteView(item: item.item, ringGems: ringGems,
                           glow: itemGlow(item), pointSize: 32, label: item.item.name)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(item.item.name).fontWeight(matches ? .semibold : .regular)
                    if item.upgrade > 0 {
                        Text("+\(item.upgrade)").font(.caption.bold())
                            .foregroundStyle(Color.shatteredGreen)
                    }
                    if item.cursed {
                        Text("cursed").font(.caption2.bold()).foregroundStyle(.red)
                            .padding(.horizontal, 5).padding(.vertical, 1)
                            .background(.red.opacity(0.12), in: Capsule())
                    }
                    if item.secret {
                        Text("secret").font(.caption2.bold()).foregroundStyle(.purple)
                            .padding(.horizontal, 5).padding(.vertical, 1)
                            .background(.purple.opacity(0.12), in: Capsule())
                            .help("Hidden in a secret room — search to reveal it")
                    }
                }
                HStack(spacing: 6) {
                    if let effect = item.effect {
                        Text(effect).foregroundStyle(ItemCatalog.cursesFor(item.item.kind).contains(effect) ? .red : .teal)
                        Text("·").foregroundStyle(.tertiary)
                    }
                    Text(item.source.label).foregroundStyle(.secondary)
                }.font(.caption)
                accessibilityNote
            }
            Spacer(minLength: 0)
            if matches {
                Label("Match", systemImage: "checkmark")
                    .font(.caption.bold()).foregroundStyle(Color.shatteredMint)
                    .padding(.horizontal, 7).padding(.vertical, 2)
                    .background(Color.shatteredMint.opacity(0.12), in: Capsule())
                    .help("Selected as part of a jointly obtainable requirement match")
            }
        }
        .padding(.vertical, 1)
    }

    @ViewBuilder private var accessibilityNote: some View {
        switch item.accessibility {
        case .independent:
            EmptyView()
        case .choice(let group, let option):
            Label("One reward of choice group \(group) (option \(option + 1))", systemImage: "arrow.triangle.branch")
                .font(.caption2).foregroundStyle(.secondary)
        case .scenarios(let group, _):
            Label("Only in some outcomes of scenario group \(group)", systemImage: "arrow.triangle.branch")
                .font(.caption2).foregroundStyle(.secondary)
        }
    }
}

// MARK: - Helpers

/// Captures the NSWindow hosting a SwiftUI view, so the result-navigation key
/// monitor can scope itself to its own window in a multi-window WindowGroup.
private struct WindowAccessor: NSViewRepresentable {
    @Binding var window: NSWindow?

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { window = view.window }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async { window = view.window }
    }
}

private func intBinding(_ value: Binding<Int>) -> Binding<Double> {
    Binding(get: { Double(value.wrappedValue) }, set: { value.wrappedValue = Int($0.rounded()) })
}

/// Maps a floor-limit binding onto an index into `FloorLimits.options`, so
/// sliders skip the empty boss floors (5, 10, 15). Off-list values snap to
/// the nearest option below.
private func floorLimitBinding(_ value: Binding<Int>) -> Binding<Double> {
    Binding(
        get: { Double(FloorLimits.index(of: value.wrappedValue)) },
        set: {
            let index = min(max(Int($0.rounded()), 0), FloorLimits.options.count - 1)
            value.wrappedValue = FloorLimits.options[index]
        }
    )
}
