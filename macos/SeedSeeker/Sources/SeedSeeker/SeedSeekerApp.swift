import AppKit
import SeedSeekerKit
import SwiftUI

@main
struct SeedSeekerApp: App {
    var body: some Scene {
        WindowGroup("Seed Seeker") { ContentView() }
            .defaultSize(width: 1_180, height: 720)
        Settings { ChallengesSettingsView() }
    }
}

private struct ContentView: View {
    @AppStorage("savedQuery") private var savedQueryJSON = ""
    @AppStorage("challenges") private var challenges = 0
    @State private var requirements: [ItemRequirement] = []
    @State private var maximumDepth = 24
    @State private var requireBlacksmith = false
    @State private var excludeBlacksmithRewards = false
    @State private var fastMode = false
    @State private var restored = false
    @State private var controller = SearchController()
    @State private var scout = ScoutViewModel()

    var body: some View {
        VStack(spacing: 0) {
            NavigationSplitView {
                QueryView(requirements: $requirements, maximumDepth: $maximumDepth,
                          requireBlacksmith: $requireBlacksmith,
                          excludeBlacksmithRewards: $excludeBlacksmithRewards, fastMode: $fastMode,
                          challenges: $challenges,
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
            Text("Shattered Pixel Dungeon v3.3.8")
                .font(.caption).foregroundStyle(.secondary).frame(maxWidth: .infinity).padding(5)
        }
        .frame(minWidth: 1_020, minHeight: 640)
        .onAppear {
            guard !restored else { return }; restored = true
            let saved = QueryPersistence.decode(savedQueryJSON)
            requirements = saved.requirements; maximumDepth = saved.maximumDepth
            requireBlacksmith = saved.requireBlacksmith
            excludeBlacksmithRewards = saved.excludeBlacksmithRewards
            fastMode = saved.fastMode
        }
        .onChange(of: requirements) { save() }
        .onChange(of: maximumDepth) { save() }
        .onChange(of: requireBlacksmith) { save() }
        .onChange(of: excludeBlacksmithRewards) { save() }
        .onChange(of: fastMode) { save() }
        .onChange(of: controller.selectedSeed) { _, seed in
            if let seed { scout.scout(seed, challenges: challenges) }
        }
    }

    private func save() {
        guard restored else { return }
        savedQueryJSON = QueryPersistence.encode(.init(requirements: requirements,
            maximumDepth: maximumDepth, requireBlacksmith: requireBlacksmith,
            excludeBlacksmithRewards: excludeBlacksmithRewards, fastMode: fastMode)) ?? ""
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
    var id: Int64 { requirement.key }
}

private struct QueryView: View {
    @Binding var requirements: [ItemRequirement]
    @Binding var maximumDepth: Int
    @Binding var requireBlacksmith: Bool
    @Binding var excludeBlacksmithRewards: Bool
    @Binding var fastMode: Bool
    @Binding var challenges: Int
    let controller: SearchController
    @State private var editor: EditorSession?

    var body: some View {
        VStack(spacing: 0) {
            List {
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
                }
                editor = nil
            }
        }
    }

    @ViewBuilder private var requirementSections: some View {
        if requirements.isEmpty {
            Section("Requirements") {
                Text("No requirements yet. Add one to describe the item you're hunting for.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        } else {
            ForEach(ItemKind.allCases, id: \.self) { kind in
                let group = requirements.filter { $0.kind == kind }
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
                VStack(alignment: .leading, spacing: 3) {
                    Text(requirement.title).foregroundStyle(.primary)
                    Text(requirement.description).font(.caption).foregroundStyle(.secondary)
                }.frame(maxWidth: .infinity, alignment: .leading)
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
        _match = State(initialValue: requirement.upgradeMatch); _upgrade = State(initialValue: requirement.upgrade)
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
                    Picker("Category", selection: $kind) {
                        ForEach(ItemKind.allCases, id: \.self) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .onChange(of: kind) { _, _ in
                        itemID = ""; tierMatch = .any; tier = 2; modifier = ""; normalizeUpgrade()
                    }
                    Picker("Item", selection: $itemID) {
                        Text("Any \(kind.singularLabel)").tag("")
                        if kind == .weapon {
                            // Tier-1 melee weapons are starting gear and never spawn in the dungeon.
                            ForEach(2...5, id: \.self) { tier in
                                Section("Tier \(tier)") {
                                    ForEach(ItemCatalog.weapons.filter { $0.tier == tier }) { Text($0.name).tag($0.id) }
                                }
                            }
                        } else {
                            ForEach(ItemCatalog.forKind(kind).filter { $0.tier != 1 }) {
                                Text($0.name).tag($0.id)
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
                    if match != .any {
                        VStack(alignment: .leading, spacing: 2) {
                            LabeledContent(match == .exactly ? "Exactly" : "At least") {
                                Text("+\(upgrade)").monospacedDigit().foregroundStyle(.secondary)
                            }
                            Slider(value: intBinding($upgrade),
                                   in: Double(minimumUpgrade)...Double(kind.maximumSearchUpgrade), step: 1)
                        }
                    }
                }
                Section {
                    if kind.modifierLabel != nil {
                        Picker(kind.modifierLabel!, selection: $modifier) {
                            Section { Text("None").tag("") }
                            Section(kind == .weapon ? "Enchantments" : "Glyphs") {
                                ForEach(kind == .weapon ? ItemCatalog.enchantments : ItemCatalog.glyphs, id: \.self) { Text($0).tag($0) }
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

    private var minimumUpgrade: Int { match == .exactly ? 1 : 0 }
    private func normalizeUpgrade() {
        upgrade = match == .any ? 0 : max(minimumUpgrade, min(upgrade, kind.maximumSearchUpgrade))
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
            Image(systemName: item.item.kind.icon)
                .foregroundStyle(item.item.kind.tint)
                .frame(width: 20, alignment: .center)
                .padding(.top, 2)
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
