import AppKit
import Combine
import SeedSeekerKit
import Sparkle
import SwiftUI

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
            } detail: {
                SeedDetailView(model: scout, requirements: requirements, maximumDepth: maximumDepth,
                               excludeBlacksmithRewards: excludeBlacksmithRewards, challenges: challenges)
                    .navigationSplitViewColumnWidth(min: 360, ideal: 450)
            }
            Divider()
            // The bundled item artwork is GPL-3.0-or-later, so its attribution
            // and the full license text have to be reachable from the app.
            Button { showingAbout = true } label: {
                Text("Shattered Pixel Dungeon v3.3.8 · Artwork & licenses")
                    .font(.caption).foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity).padding(5)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Item artwork attribution and license")
        }
        .sheet(isPresented: $showingAbout) { AboutView() }
        .frame(minWidth: 1_020, minHeight: 640)
        .onAppear {
            guard !restored else { return }; restored = true
            let saved = QueryPersistence.decode(savedQueryJSON)
            requirements = saved.requirements; maximumDepth = saved.maximumDepth
            requireBlacksmith = saved.requireBlacksmith
            excludeBlacksmithRewards = saved.excludeBlacksmithRewards
            fastMode = saved.fastMode
            userPresets = PresetPersistence.decode(savedPresetsJSON)
        }
        .onChange(of: requirements) { save() }
        .onChange(of: maximumDepth) { save() }
        .onChange(of: requireBlacksmith) { save() }
        .onChange(of: excludeBlacksmithRewards) { save() }
        .onChange(of: fastMode) { save() }
        .onChange(of: challenges) { save() }
        .onChange(of: controller.selectedSeed) { _, seed in
            if let seed { scout.scout(seed, challenges: challenges) }
        }
    }

    private func save() {
        guard restored else { return }
        savedQueryJSON = QueryPersistence.encode(.init(requirements: requirements,
            maximumDepth: maximumDepth, requireBlacksmith: requireBlacksmith,
            excludeBlacksmithRewards: excludeBlacksmithRewards, fastMode: fastMode,
            challenges: challenges)) ?? ""
    }

    private func apply(_ preset: QueryPreset) {
        let saved = preset.query
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
        case .weapon: "hammer.fill"
        case .armor: "shield.fill"
        case .wand: "wand.and.stars"
        case .ring: "circle.circle.fill"
        }
    }
    var tint: Color {
        switch self {
        case .weapon: .orange
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
    /// When adding an alternative to a previously ungrouped row, that row only
    /// joins the new group once the new requirement is actually saved.
    var joinSourceKey: Int64?
    var id: Int64 { requirement.key }

    init(requirement: ItemRequirement, isNew: Bool, joinSourceKey: Int64? = nil) {
        self.requirement = requirement; self.isNew = isNew; self.joinSourceKey = joinSourceKey
    }
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
            Button {
                if controller.isRunning { controller.cancel() }
                else if let request = try? SearchRequest(requirements: requirements,
                    maximumDepth: maximumDepth, requireBlacksmith: requireBlacksmith,
                    excludeBlacksmithRewards: excludeBlacksmithRewards,
                    fastMode: fastMode, challenges: challenges) { controller.start(request) }
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
                    // A previously ungrouped source row joins the new
                    // alternative group only once the alternative is saved.
                    if let sourceKey = session.joinSourceKey, let group = result.alternativeGroup,
                       let index = requirements.firstIndex(where: { $0.key == sourceKey }) {
                        requirements[index].alternativeGroup = group
                        // The engine rejects combined-upgrade members inside
                        // alternative groups.
                        requirements[index].upgradeSumGroup = nil
                        requirements[index].upgradeSumTotal = nil
                    }
                    // Members of a combined-upgrade group must agree on the
                    // total; the freshly saved requirement's total wins.
                    if let group = result.upgradeSumGroup, let total = result.upgradeSumTotal {
                        for index in requirements.indices
                        where requirements[index].upgradeSumGroup == group {
                            requirements[index].upgradeSumTotal = total
                        }
                    }
                    dissolveSingletonAlternativeGroups()
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

    @ViewBuilder private var requirementSections: some View {
        if requirements.isEmpty {
            Section("Requirements") {
                Text("No requirements yet. Add one to describe the item you're hunting for.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        } else {
            // Ungrouped rows keep their per-category sections; each
            // alternative group renders as one "Any of" card in its own
            // section, since a group may span categories.
            ForEach(ItemKind.allCases, id: \.self) { kind in
                let group = requirements.filter { $0.kind == kind && $0.alternativeGroup == nil }
                if !group.isEmpty {
                    Section {
                        ForEach(group) { requirement in row(requirement) }
                    } header: {
                        Label(kind.label, systemImage: kind.icon)
                    }
                }
            }
            ForEach(alternativeGroupIDs, id: \.self) { groupID in
                Section {
                    alternativeGroupCard(groupID)
                } header: {
                    Label("Any of", systemImage: "arrow.triangle.branch")
                }
            }
        }
    }

    /// Alternative group labels in order of first appearance.
    private var alternativeGroupIDs: [Int] {
        var seen: [Int] = []
        for requirement in requirements {
            if let group = requirement.alternativeGroup, !seen.contains(group) { seen.append(group) }
        }
        return seen
    }

    private func alternativeGroupCard(_ groupID: Int) -> some View {
        let members = requirements.filter { $0.alternativeGroup == groupID }
        return VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(members.enumerated()), id: \.element.key) { position, requirement in
                if position > 0 {
                    Text("or").font(.caption.smallCaps().bold()).foregroundStyle(.tertiary)
                        .padding(.leading, 32)
                }
                row(requirement)
            }
            Button {
                addAlternative(to: members[0])
            } label: {
                Label("Add alternative", systemImage: "plus.circle")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .padding(.top, 2)
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 8).strokeBorder(.quaternary))
    }

    private func row(_ requirement: ItemRequirement) -> RequirementRow {
        RequirementRow(requirement: requirement) {
            editor = EditorSession(requirement: requirement, isNew: false)
        } onRemove: {
            requirements.removeAll { $0.key == requirement.key }
            dissolveSingletonAlternativeGroups()
        } onAddAlternative: {
            addAlternative(to: requirement)
        }
    }

    private func addRequirement() {
        if let value = try? ItemRequirement(key: Int64.random(in: 1...Int64.max), item: nil,
            upgrade: 0, kind: .weapon, upgradeMatch: .any) {
            editor = EditorSession(requirement: value, isNew: true)
        }
    }

    /// Opens the editor for a new requirement joining `source`'s alternative
    /// group, allocating a fresh group when the source row has none yet.
    private func addAlternative(to source: ItemRequirement) {
        let group = source.alternativeGroup ?? nextFreeAlternativeGroup()
        if let value = try? ItemRequirement(key: Int64.random(in: 1...Int64.max), item: nil,
            upgrade: 0, kind: source.kind, upgradeMatch: .any, alternativeGroup: group) {
            editor = EditorSession(requirement: value, isNew: true,
                                   joinSourceKey: source.alternativeGroup == nil ? source.key : nil)
        }
    }

    private func nextFreeAlternativeGroup() -> Int {
        let taken = Set(requirements.compactMap(\.alternativeGroup))
        return (1...255).first { !taken.contains($0) } ?? 255
    }

    /// A group that shrinks to one member stops being an alternative.
    private func dissolveSingletonAlternativeGroups() {
        let counts = Dictionary(grouping: requirements.compactMap(\.alternativeGroup)) { $0 }
            .mapValues(\.count)
        for index in requirements.indices {
            if let group = requirements[index].alternativeGroup, counts[group, default: 0] < 2 {
                requirements[index].alternativeGroup = nil
            }
        }
    }
}

private struct RequirementRow: View {
    let requirement: ItemRequirement
    let onEdit: () -> Void
    let onRemove: () -> Void
    let onAddAlternative: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Button(action: onEdit) {
                HStack(spacing: 8) {
                    // A pinned item shows its own sprite; a wildcard keeps the
                    // category symbol. A single wanted enchantment or curse
                    // pulses in the game's glow colour.
                    ItemSpriteView(spriteIndex: requirement.item?.spriteIndex,
                                   kind: requirement.kind,
                                   glow: effectGlow(requirement.primaryEffectName),
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
            Button("Add Alternative…") { onAddAlternative() }
            Button("Remove", role: .destructive) { onRemove() }
        }
    }
}

// MARK: - Requirement editor

private enum EffectMode: Hashable {
    case any, anyEnchantment, specific
}

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
    @State private var effectMode: EffectMode
    @State private var selectedEffects: Set<String>
    @State private var sourceRaw: Int
    @State private var group: Int
    @State private var sumGroup: Int
    @State private var sumTotal: Int
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
        switch requirement.effect {
        case .any:
            _effectMode = State(initialValue: .any)
            _selectedEffects = State(initialValue: [])
        case .anyEnchantment:
            _effectMode = State(initialValue: .anyEnchantment)
            _selectedEffects = State(initialValue: [])
        case .oneOf(let names):
            _effectMode = State(initialValue: .specific)
            _selectedEffects = State(initialValue: Set(names))
        }
        _sourceRaw = State(initialValue: requirement.source.map { $0.rawValue + 1 } ?? 0)
        _group = State(initialValue: requirement.identityGroup ?? 0)
        _sumGroup = State(initialValue: requirement.upgradeSumGroup ?? 0)
        _sumTotal = State(initialValue: requirement.upgradeSumTotal ?? 2)
        _maximumDepth = State(initialValue: requirement.maximumDepth ?? 0)
        _requireUncursed = State(initialValue: requirement.requireUncursed)
    }

    var body: some View {
        VStack(spacing: 0) {
            Text(isNew ? "New Requirement" : "Edit Requirement")
                .font(.headline).padding(.top, 14).padding(.bottom, 4)
            Form {
                Section("Item") {
                    Picker("Category", selection: $kind) {
                        ForEach(ItemKind.allCases, id: \.self) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .onChange(of: kind) { _, _ in
                        itemID = ""; tierMatch = .any; tier = 2
                        effectMode = .any; selectedEffects = []
                        normalizeUpgrade()
                    }
                    Picker("Item", selection: $itemID) {
                        Text("Any \(kind.singularLabel)").tag("")
                        if kind == .weapon {
                            // Tier-1 melee weapons are starting gear and never spawn in the dungeon.
                            ForEach(2...5, id: \.self) { tier in
                                Section("Tier \(tier)") {
                                    ForEach(ItemCatalog.weapons.filter { $0.tier == tier }) { item in
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
                    if itemID.isEmpty && (kind == .weapon || kind == .armor) {
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
                        Picker(kind.modifierLabel!, selection: $effectMode) {
                            Text("Any").tag(EffectMode.any)
                            Text(kind == .weapon ? "Any enchantment" : "Any glyph")
                                .tag(EffectMode.anyEnchantment)
                            Text("Specific…").tag(EffectMode.specific)
                        }
                        .pickerStyle(.segmented)
                        if effectMode == .specific {
                            effectChecklist
                        }
                    }
                    Toggle("Require uncursed", isOn: $requireUncursed)
                        .toggleStyle(.checkbox)
                        .onChange(of: requireUncursed) { _, value in
                            if value {
                                selectedEffects.subtract(ItemCatalog.cursesFor(kind))
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
                // An alternative-group member cannot join a combined-upgrade
                // group; the engine rejects the combination.
                if original.alternativeGroup == nil {
                    Section("Combined upgrade") {
                        Picker("Group", selection: $sumGroup) {
                            Text("None").tag(0); Text("A").tag(1); Text("B").tag(2); Text("C").tag(3); Text("D").tag(4)
                        }.pickerStyle(.segmented)
                        if sumGroup != 0 {
                            Stepper(value: $sumTotal, in: 1...8) {
                                LabeledContent("Total at least") {
                                    Text("+\(sumTotal)").monospacedDigit().foregroundStyle(.secondary)
                                }
                            }
                            Text("Distinct items matching this group's requirements must have upgrades totalling at least +\(sumTotal).")
                                .font(.caption).foregroundStyle(.secondary)
                        }
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
        .frame(width: 460, height: sheetHeight)
    }

    private var sheetHeight: CGFloat {
        var height: CGFloat = 470
        if kind.modifierLabel != nil { height += 40 }
        if kind.modifierLabel != nil && effectMode == .specific { height += 170 }
        if original.alternativeGroup == nil { height += sumGroup == 0 ? 70 : 130 }
        return height
    }

    /// Multi-select checklist for the "Specific…" effect mode: the family's
    /// enchantments/glyphs, plus its curses while "require uncursed" is off.
    private var effectChecklist: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 3) {
                ForEach(kind == .weapon ? ItemCatalog.enchantments : ItemCatalog.glyphs,
                        id: \.self) { name in
                    Toggle(name, isOn: effectBinding(name)).toggleStyle(.checkbox)
                }
                if !requireUncursed {
                    Divider().padding(.vertical, 2)
                    ForEach(ItemCatalog.cursesFor(kind), id: \.self) { name in
                        Toggle(name, isOn: effectBinding(name)).toggleStyle(.checkbox)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.vertical, 2)
        }
        .frame(height: 160)
    }

    private func effectBinding(_ name: String) -> Binding<Bool> {
        Binding {
            selectedEffects.contains(name)
        } set: { enabled in
            if enabled { selectedEffects.insert(name) } else { selectedEffects.remove(name) }
        }
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
        // Keep the specific set in catalog order; an empty selection falls
        // back to the wildcard.
        let orderedSelection = ItemCatalog.modifiersFor(kind).filter(selectedEffects.contains)
        let effect: EffectPredicate = switch effectMode {
        case .any: .any
        case .anyEnchantment: kind.modifierLabel == nil ? .any : .anyEnchantment
        case .specific: orderedSelection.isEmpty ? .any : .oneOf(orderedSelection)
        }
        let inAlternative = original.alternativeGroup != nil
        guard let value = try? ItemRequirement(key: original.key, item: item, upgrade: upgrade,
            effect: effect, kind: kind,
            tier: tierMatch == .any ? 0 : tier, tierMatch: tierMatch, upgradeMatch: match,
            source: sourceRaw == 0 ? nil : ScoutItemSource(rawValue: sourceRaw - 1),
            identityGroup: group == 0 ? nil : group,
            maximumDepth: maximumDepth == 0 ? nil : maximumDepth,
            requireUncursed: requireUncursed,
            alternativeGroup: original.alternativeGroup,
            upgradeSumGroup: inAlternative || sumGroup == 0 ? nil : sumGroup,
            upgradeSumTotal: inAlternative || sumGroup == 0 ? nil : sumTotal) else { return }
        onFinish(value)
    }
}

// MARK: - Results

private struct ResultsView: View {
    let controller: SearchController
    let scout: (String) -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            status.padding([.horizontal, .top])
            if controller.reachedResultCap { Text("Result limit reached (1,024 seeds).").font(.caption).foregroundStyle(.secondary).padding(.horizontal) }
            Table(controller.results, selection: Bindable(controller).selectedSeed) {
                TableColumn("#") { result in Text("\((controller.results.firstIndex(of: result) ?? 0) + 1)").foregroundStyle(.secondary) }.width(45)
                TableColumn("Seed") { result in
                    Text(result.seed).font(.system(.body, design: .monospaced))
                        .contextMenu { Button("Copy Seed") { copy(result.seed) }; Button("Scout Seed") { scout(result.seed) } }
                }
            }
            Button("Copy Selected") { if let seed = controller.selectedSeed { copy(seed) } }
                .keyboardShortcut("c", modifiers: .command).hidden()
        }.navigationTitle("Results")
    }
    @ViewBuilder private var status: some View {
        if controller.state == nil { Text("Add requirements, then press Start Search.").foregroundStyle(.secondary) }
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
    private func copy(_ seed: String) { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(seed, forType: .string) }
}

// MARK: - Scout / seed detail

@MainActor @Observable private final class ScoutViewModel {
    var input = ""
    var world: ScoutWorld?
    var error: String?
    var loading = false
    private let engine = ProductionSeedFinderEngine()
    func scout(_ seed: String? = nil, challenges: Int) {
        if let seed { input = SeedCode.formatInput(seed) }
        guard SeedCode.isCanonical(input) else { error = "Seed must use XXX-XXX-XXX format"; return }
        let requested = input; loading = true; error = nil
        Task { do { world = try await engine.scoutSeed(requested, challenges: challenges) } catch { self.error = error.localizedDescription }; loading = false }
    }
}

private struct SeedDetailView: View {
    @Bindable var model: ScoutViewModel
    let requirements: [ItemRequirement]
    let maximumDepth: Int
    let excludeBlacksmithRewards: Bool
    let challenges: Int
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

private func intBinding(_ value: Binding<Int>) -> Binding<Double> {
    Binding(get: { Double(value.wrappedValue) }, set: { value.wrappedValue = Int($0.rounded()) })
}
