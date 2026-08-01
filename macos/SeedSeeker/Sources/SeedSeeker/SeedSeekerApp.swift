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
            .defaultSize(width: 1_180, height: 720)
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
            NavigationSplitView {
                QueryView(requirements: $requirements, maximumDepth: $maximumDepth,
                          requireBlacksmith: $requireBlacksmith,
                          excludeBlacksmithRewards: $excludeBlacksmithRewards, fastMode: $fastMode,
                          challenges: $challenges,
                          userPresets: userPresets,
                          onApplyPreset: apply,
                          onSavePreset: savePreset,
                          onDeletePreset: deletePreset,
                          controller: controller)
                    .navigationSplitViewColumnWidth(min: 300, ideal: 330, max: 380)
            } content: {
                ResultsView(controller: controller) { seed in scout.scout(seed, challenges: challenges) }
                    .navigationSplitViewColumnWidth(min: 340, ideal: 420)
                    .toolbar {
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
                        }
                    }
            } detail: {
                SeedDetailView(model: scout, requirements: requirements, maximumDepth: maximumDepth,
                               excludeBlacksmithRewards: excludeBlacksmithRewards, challenges: challenges,
                               resultPosition: resultPosition, onNavigateResult: { _ = navigateResult($0) })
                    .navigationSplitViewColumnWidth(min: 360, ideal: 450)
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
                    Text("Shattered Pixel Dungeon v3.3.8 · Artwork & licenses")
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
            .padding(.horizontal, 10)
        }
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
        .alert("Results file", isPresented: Binding(
            get: { transferError != nil },
            set: { if !$0 { transferError = nil } })
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(transferError ?? "")
        }
        .frame(minWidth: 1_020, minHeight: 640)
        .background(WindowAccessor(window: $hostWindow))
        .onAppear {
            installResultKeyNavigation()
            guard !restored else { return }; restored = true
            let saved = QueryPersistence.decode(savedQueryJSON)
            requirements = saved.requirements; maximumDepth = saved.maximumDepth
            requireBlacksmith = saved.requireBlacksmith
            excludeBlacksmithRewards = saved.excludeBlacksmithRewards
            fastMode = saved.fastMode
            userPresets = PresetPersistence.decode(savedPresetsJSON)
        }
        .onDisappear {
            if let monitor = resultKeyMonitor { NSEvent.removeMonitor(monitor); resultKeyMonitor = nil }
        }
        .onChange(of: requirements) { save() }
        .onChange(of: maximumDepth) { save() }
        .onChange(of: requireBlacksmith) { save() }
        .onChange(of: excludeBlacksmithRewards) { save() }
        .onChange(of: fastMode) { save() }
        .onChange(of: challenges) { save() }
        .onChange(of: controller.selectedSeed) { _, seed in
            // J/K navigation scouts before moving the selection; only scout
            // here for direct table selections.
            if let seed, seed != scout.requestedSeed { scout.scout(seed, challenges: challenges) }
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
            excludeBlacksmithRewards: excludeBlacksmithRewards, fastMode: fastMode,
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
                    guard data.count <= ResultsExport.maxFileBytes else {
                        throw ResultsExportError(
                            "This file is too large to be a Seed Seeker results file (2 MiB limit).")
                    }
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
                controller.loadImported(seeds: imported.seeds, query: imported.query)
                if let fileVersion = imported.shpdVersion, fileVersion != ResultsExport.shpdVersion {
                    transferError = "Imported \(controller.results.count) seeds. Note: this file was " +
                        "made for Shattered Pixel Dungeon v\(fileVersion); this app targets " +
                        "v\(ResultsExport.shpdVersion), so the seeds may generate differently."
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

// MARK: - Item kind presentation

extension ItemKind {
    var icon: String {
        switch self {
        case .weapon, .meleeWeapon: "hammer.fill"
        case .thrownWeapon: "scope"
        case .armor: "shield.fill"
        case .wand: "wand.and.stars"
        case .ring: "circle.circle.fill"
        }
    }
    var tint: Color {
        switch self {
        case .weapon, .meleeWeapon, .thrownWeapon: .orange
        case .armor: .blue
        case .wand: .purple
        case .ring: .yellow
        }
    }
}

// MARK: - Query sidebar

private struct EditorSession: Identifiable {
    let requirement: ItemRequirement
    let isNew: Bool
    var id: Int64 { requirement.key }
}

private struct QueryView: View {
    @Binding var requirements: [ItemRequirement]
    @Binding var maximumDepth: Int
    @Binding var requireBlacksmith: Bool
    @Binding var excludeBlacksmithRewards: Bool
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
            List {
                Section("Presets") {
                    HStack {
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
                        Button {
                            presetName = ""
                            showingSavePreset = true
                        } label: {
                            HStack(spacing: 6) {
                                Image(systemName: "bookmark.badge.plus")
                                Text("Save Current Query")
                            }
                            .fixedSize()
                        }
                        .buttonStyle(.bordered)
                        .fixedSize()
                        .layoutPriority(1)
                        Spacer(minLength: 0)
                    }
                    if !userPresets.isEmpty {
                        Menu("Delete Saved Preset", systemImage: "trash") {
                            ForEach(userPresets) { preset in
                                Button(preset.name, role: .destructive) { onDeletePreset(preset) }
                            }
                        }
                    }
                }
                requirementSections
                Section {
                    Button("Add Requirement", systemImage: "plus") { addRequirement() }
                        .keyboardShortcut("n", modifiers: .command)
                }
                Section("Search scope") {
                    VStack(alignment: .leading, spacing: 2) {
                        LabeledContent("Floor limit") {
                            Text("first \(maximumDepth) floor\(maximumDepth == 1 ? "" : "s")")
                                .monospacedDigit().foregroundStyle(.secondary)
                        }
                        Slider(value: intBinding($maximumDepth), in: 1...24, step: 1)
                    }
                }
                Section("Blacksmith") {
                    Toggle("Require accessible blacksmith", isOn: $requireBlacksmith)
                        .disabled(maximumDepth >= 14)
                    VStack(alignment: .leading, spacing: 2) {
                        Toggle("Exclude Smith rewards", isOn: $excludeBlacksmithRewards)
                        Text("Required items cannot come from the 2,000-favor Smith choice, leaving favor available for reforging.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                Section("Performance") {
                    VStack(alignment: .leading, spacing: 2) {
                        Toggle("Fast search", isOn: $fastMode)
                        Text("Treats +3 weapons and armor as quest rewards only, skipping the rare Crypt and Sacrificial-fire prizes. Found seeds are always genuine.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            Divider()
            if challenges.nonzeroBitCount > 0 {
                Label("Challenges: \(challenges.nonzeroBitCount) enabled", systemImage: "flag.fill")
                    .font(.caption).foregroundStyle(.secondary)
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
                .disabled(requirements.isEmpty).keyboardShortcut(.return, modifiers: .command)
                .padding()
        }
        .navigationTitle("Query")
        .sheet(item: $editor) { session in
            RequirementEditor(requirement: session.requirement, isNew: session.isNew) { result in
                if let result {
                    if session.isNew {
                        requirements.append(result)
                    } else if let index = requirements.firstIndex(where: { $0.key == result.key }) {
                        requirements[index] = result
                    }
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

    private var builtRequest: SearchRequest? {
        try? SearchRequest(requirements: requirements, maximumDepth: maximumDepth,
                           requireBlacksmith: requireBlacksmith,
                           excludeBlacksmithRewards: excludeBlacksmithRewards,
                           fastMode: fastMode, challenges: challenges)
    }

    @ViewBuilder private var requirementSections: some View {
        if requirements.isEmpty {
            Section("Requirements") {
                Text("No requirements yet. Add one to describe the item you're hunting for.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        } else {
            // Group by the broad family so a narrowed "Any thrown weapon"
            // requirement sits with the other weapons.
            ForEach([ItemKind.weapon, .armor, .wand, .ring], id: \.self) { kind in
                let group = requirements.filter { $0.kind.family == kind }
                if !group.isEmpty {
                    Section {
                        ForEach(group) { requirement in
                            RequirementRow(requirement: requirement) {
                                editor = EditorSession(requirement: requirement, isNew: false)
                            } onRemove: {
                                requirements.removeAll { $0.key == requirement.key }
                            }
                        }
                    } header: {
                        Label(kind.label, systemImage: kind.icon)
                    }
                }
            }
        }
    }

    private func addRequirement() {
        if let value = try? ItemRequirement(key: Int64.random(in: 1...Int64.max), item: nil,
            upgrade: 0, kind: .weapon, upgradeMatch: .any) {
            editor = EditorSession(requirement: value, isNew: true)
        }
    }
}

private struct RequirementRow: View {
    let requirement: ItemRequirement
    let onEdit: () -> Void
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Button(action: onEdit) {
                HStack(spacing: 8) {
                    // A pinned item shows its own sprite; a wildcard keeps the
                    // category symbol. Either way an enchantment or curse
                    // requirement pulses in the game's glow colour.
                    ItemSpriteView(spriteIndex: requirement.item?.spriteIndex,
                                   kind: requirement.kind,
                                   glow: effectGlow(requirement.modifier),
                                   pointSize: 24, label: requirement.title)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(requirement.title).foregroundStyle(.primary)
                        Text(requirement.description).font(.caption).foregroundStyle(.secondary)
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }
                .contentShape(Rectangle())
            }.buttonStyle(.plain)
            Button(action: onRemove) {
                Image(systemName: "xmark.circle.fill").foregroundStyle(.tertiary)
            }.buttonStyle(.plain).help("Remove requirement")
        }
        .contextMenu {
            Button("Edit…") { onEdit() }
            Button("Remove", role: .destructive) { onRemove() }
        }
    }
}

// MARK: - Requirement editor

private struct RequirementEditor: View {
    let original: ItemRequirement
    let isNew: Bool
    let onFinish: (ItemRequirement?) -> Void
    @State private var kind: ItemKind
    @State private var itemID: String
    @State private var tierMatch: TierMatch
    @State private var tier: Int
    @State private var match: UpgradeMatch
    @State private var upgrade: Int
    @State private var modifier: String
    @State private var sourceRaw: Int
    @State private var group: Int
    @State private var maximumDepth: Int
    @State private var requireUncursed: Bool

    init(requirement: ItemRequirement, isNew: Bool, onFinish: @escaping (ItemRequirement?) -> Void) {
        original = requirement; self.isNew = isNew; self.onFinish = onFinish
        _kind = State(initialValue: requirement.kind); _itemID = State(initialValue: requirement.item?.id ?? "")
        _tierMatch = State(initialValue: requirement.tierMatch)
        _tier = State(initialValue: requirement.tier < 2 ? 2 : requirement.tier)
        _match = State(initialValue: requirement.upgradeMatch)
        let maximumUpgrade = requirement.kind.maximumSearchUpgrade
        let initialUpgrade = switch requirement.upgradeMatch {
        case .any: 0
        case .exactly: max(1, min(requirement.upgrade, maximumUpgrade))
        case .atLeast: max(1, min(requirement.upgrade, maximumUpgrade - 1))
        }
        _upgrade = State(initialValue: initialUpgrade)
        _modifier = State(initialValue: requirement.modifier ?? "")
        _sourceRaw = State(initialValue: requirement.source.map { $0.rawValue + 1 } ?? 0)
        _group = State(initialValue: requirement.identityGroup ?? 0)
        _maximumDepth = State(initialValue: requirement.maximumDepth ?? 0)
        _requireUncursed = State(initialValue: requirement.requireUncursed)
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
                            itemID = ""; tierMatch = .any; tier = 2; modifier = ""; normalizeUpgrade()
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
                            ForEach(2...5, id: \.self) { tier in
                                Section("Tier \(tier)") {
                                    ForEach(ItemCatalog.forKind(kind).filter { $0.tier == tier }) { item in
                                        Label { Text(item.name) } icon: {
                                            ItemSpriteIcon(spriteIndex: item.spriteIndex)
                                        }.tag(item.id)
                                    }
                                }
                            }
                        } else {
                            ForEach(ItemCatalog.forKind(kind).filter { $0.tier != 1 }) { item in
                                Label { Text(item.name) } icon: {
                                    ItemSpriteIcon(spriteIndex: item.spriteIndex)
                                }.tag(item.id)
                            }
                        }
                    }
                    .onChange(of: itemID) { _, value in if !value.isEmpty { tierMatch = .any } }
                    if itemID.isEmpty && (kind.family == .weapon || kind.family == .armor) {
                        Picker("Tier", selection: $tierMatch) {
                            ForEach(TierMatch.allCases, id: \.self) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        .onChange(of: tierMatch) { _, value in
                            if value == .atLeast || value == .atMost {
                                tier = max(3, min(tier, 4))
                            }
                        }
                        if tierMatch == .exactly {
                            VStack(alignment: .leading, spacing: 2) {
                                LabeledContent("Exact tier") {
                                    Text("Tier \(tier)")
                                        .monospacedDigit().foregroundStyle(.secondary)
                                }
                                Slider(value: intBinding($tier), in: 2...5, step: 1)
                            }
                        } else if tierMatch == .atLeast || tierMatch == .atMost {
                            Picker(tierMatch == .atLeast ? "Minimum tier" : "Maximum tier",
                                   selection: $tier) {
                                ForEach(3...4, id: \.self) { option in
                                    Text(tierMatch == .atLeast ? "Tier \(option) or higher" :
                                        "Tier \(option) or lower").tag(option)
                                }
                            }
                            .pickerStyle(.menu)
                        }
                    }
                }
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
                                   in: 1...Double(kind.maximumSearchUpgrade), step: 1)
                        }
                    } else if match == .atLeast {
                        if kind == .ring {
                            VStack(alignment: .leading, spacing: 2) {
                                LabeledContent("At least") {
                                    Text("+\(upgrade)").monospacedDigit().foregroundStyle(.secondary)
                                }
                                Slider(value: intBinding($upgrade),
                                       in: 1...Double(kind.maximumSearchUpgrade - 1), step: 1)
                            }
                        } else {
                            Picker("Minimum upgrade", selection: $upgrade) {
                                ForEach(1..<kind.maximumSearchUpgrade, id: \.self) { option in
                                    Text("+\(option) or higher").tag(option)
                                }
                            }
                            .pickerStyle(.menu)
                        }
                    }
                }
                Section {
                    if kind.modifierLabel != nil {
                        Picker(kind.modifierLabel!, selection: $modifier) {
                            Section { Text("None").tag("") }
                            Section(kind.family == .weapon ? "Enchantments" : "Glyphs") {
                                ForEach(kind.family == .weapon ? ItemCatalog.enchantments : ItemCatalog.glyphs, id: \.self) { Text($0).tag($0) }
                            }
                            if !requireUncursed {
                                Section("Curses") { ForEach(ItemCatalog.cursesFor(kind), id: \.self) { Text($0).tag($0) } }
                            }
                        }
                    }
                    Toggle("Require uncursed", isOn: $requireUncursed)
                        .toggleStyle(.checkbox)
                        .onChange(of: requireUncursed) { _, value in
                            if value && ItemCatalog.cursesFor(kind).contains(modifier) {
                                modifier = ""
                            }
                        }
                    Picker("Source", selection: $sourceRaw) {
                        Text("Any").tag(0)
                        ForEach(ScoutItemSource.allCases, id: \.rawValue) { Text($0.label).tag($0.rawValue + 1) }
                    }
                    Picker("Same-item group", selection: $group) {
                        Text("None").tag(0); Text("A").tag(1); Text("B").tag(2); Text("C").tag(3); Text("D").tag(4)
                    }.pickerStyle(.segmented)
                    Toggle("Limit this item to a floor", isOn: Binding(
                        get: { maximumDepth != 0 },
                        set: { maximumDepth = $0 ? 5 : 0 }
                    ))
                    if maximumDepth != 0 {
                        LabeledContent("Within first") {
                            Text("\(maximumDepth) floors").monospacedDigit().foregroundStyle(.secondary)
                        }
                        Slider(value: intBinding($maximumDepth), in: 1...24, step: 1)
                    }
                }
            }
            .formStyle(.grouped)
            Divider()
            HStack {
                Button("Cancel") { onFinish(nil) }.keyboardShortcut(.cancelAction)
                Spacer()
                Button(isNew ? "Add" : "Save") { save() }
                    .buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
            }.padding(12)
        }
        .frame(width: 460, height: kind.modifierLabel == nil ? 470 : 500)
    }

    private func normalizeUpgrade() {
        switch match {
        case .any:
            upgrade = 0
        case .exactly:
            upgrade = max(1, min(upgrade, kind.maximumSearchUpgrade))
        case .atLeast:
            upgrade = max(1, min(upgrade, kind.maximumSearchUpgrade - 1))
        }
    }
    private func save() {
        let item = itemID.isEmpty ? nil : ItemCatalog.findById(itemID)
        guard let value = try? ItemRequirement(key: original.key, item: item, upgrade: upgrade,
            modifier: modifier.isEmpty ? nil : modifier, kind: kind,
            tier: tierMatch == .any ? 0 : tier, tierMatch: tierMatch, upgradeMatch: match,
            source: sourceRaw == 0 ? nil : ScoutItemSource(rawValue: sourceRaw - 1),
            identityGroup: group == 0 ? nil : group,
            maximumDepth: maximumDepth == 0 ? nil : maximumDepth,
            requireUncursed: requireUncursed) else { return }
        onFinish(value)
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
                Text("Time elapsed: \(NumberFormat.duration(controller.elapsed))")
                    .font(.caption2).foregroundStyle(.tertiary)
            }
        } else if controller.isImpossibleQuery {
            VStack(alignment: .leading, spacing: 4) {
                Text("Impossible query").font(.caption.bold())
                    .padding(.horizontal, 10).padding(.vertical, 4)
                    .foregroundStyle(.orange).background(.quaternary, in: Capsule())
                Text("No seed can satisfy these requirements within the current floor limit. " +
                     "Quest-reward-only items need their quest floors in range: +3 wands floor 9, " +
                     "+3/+4 rings floor 19.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        } else if let state = controller.state {
            Text(state == .failed ? "Failed (error \(controller.errorCode))" : state == .completed ? "Completed" : "Cancelled")
                .font(.caption.bold()).padding(.horizontal, 10).padding(.vertical, 4).background(.quaternary, in: Capsule())
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
        }.navigationTitle("Seed Detail")
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
        }.padding([.horizontal, .top]).padding(.bottom, 8)
    }

    private func manifest(_ world: ScoutWorld) -> some View {
        let byDepth = Dictionary(grouping: world.items, by: \.depth)
        let depths = byDepth.keys.sorted()
        let matches = scoutMatchIndices(items: world.items, requirements: requirements,
                                        maximumDepth: maximumDepth,
                                        excludeBlacksmithRewards: excludeBlacksmithRewards)
        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 4) {
                Text("\(world.items.count) items across \(depths.count) floors")
                if !requirements.isEmpty {
                    Text("·")
                    Label("\(matches.count) requirement match\(matches.count == 1 ? "" : "es")", systemImage: "checkmark.circle")
                        .foregroundStyle(matches.isEmpty ? Color.secondary : Color.green)
                }
            }
            .font(.caption).foregroundStyle(.secondary)
            .padding(.horizontal).padding(.vertical, 6)
            List {
                ForEach(depths, id: \.self) { depth in
                    Section {
                        ForEach(Array(world.items.enumerated()).filter { $0.element.depth == depth }, id: \.offset) { entry in
                            ScoutItemRow(item: entry.element, matches: matches.contains(entry.offset))
                        }
                    } header: {
                        HStack {
                            Text("Floor \(depth)")
                            Text(Self.region(depth)).foregroundStyle(.tertiary)
                        }
                    }
                }
            }
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

private struct ScoutItemRow: View {
    let item: ScoutItem
    let matches: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            ItemSpriteView(spriteIndex: item.item.spriteIndex, kind: item.item.kind,
                           glow: itemGlow(item), pointSize: 32, label: item.item.name)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(item.item.name).fontWeight(matches ? .semibold : .regular)
                    if item.upgrade > 0 {
                        Text("+\(item.upgrade)").font(.caption.bold()).foregroundStyle(.green)
                    }
                    if item.cursed {
                        Text("cursed").font(.caption2.bold()).foregroundStyle(.red)
                            .padding(.horizontal, 5).padding(.vertical, 1)
                            .background(.red.opacity(0.12), in: Capsule())
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
                    .font(.caption.bold()).foregroundStyle(.green)
                    .padding(.horizontal, 7).padding(.vertical, 2)
                    .background(.green.opacity(0.12), in: Capsule())
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
