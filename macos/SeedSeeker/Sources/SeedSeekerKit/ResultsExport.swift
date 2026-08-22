import CSeedFinder
import Foundation

/// User-facing failure while reading a results file.
public struct ResultsExportError: Error, LocalizedError, Equatable {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

/// The cross-platform results-export document: search results plus the query
/// that found them.
///
/// The format is the Rust core's (`crates/seedfinder-core/src/results_export.rs`,
/// documented in `docs/results-export-format.md`) and so is every rule about
/// it: the envelope, the compatibility contract, the strict query validation,
/// the canonical seed codes, the import size cap and the dedupe-and-cap step
/// all live behind `seedfinder_results_encode`/`_decode`. What remains here is
/// the mapping between that canonical query document and the Swift models,
/// which the engine has already validated by the time it is read.
public enum ResultsExport {
    public static let suggestedFileName = "seed-seeker-results"

    public struct Imported: Sendable {
        public let query: SavedQuery
        public let seeds: [String]
        /// Exported entries the engine's dedupe-and-cap step removed.
        public let dropped: Int
        /// The upstream game version the file declares, if any.
        public let shpdVersion: String?
        public init(query: SavedQuery, seeds: [String], dropped: Int, shpdVersion: String?) {
            self.query = query; self.seeds = seeds
            self.dropped = dropped; self.shpdVersion = shpdVersion
        }
    }

    /// Stable document names, indexed by the matching enum raw value.
    private static let kindNames = ["weapon", "armor", "wand", "ring", "melee_weapon", "thrown_weapon"]
    private static let sourceNames = [
        "heap", "chest", "locked_chest", "crystal_chest", "tomb", "skeleton",
        "sacrificial_fire", "mimic", "golden_mimic", "crystal_mimic", "statue",
        "armored_statue", "shop", "ghost_reward", "wandmaker_reward",
        "blacksmith_reward", "imp_reward",
    ]
    private static let challengeNames: [(name: String, challenge: Challenge)] = [
        ("on_diet", .noFood), ("faith_is_my_armor", .noArmor),
        ("pharmacophobia", .noHealing), ("barren_land", .noHerbalism),
        ("swarm_intelligence", .swarmIntelligence), ("into_darkness", .darkness),
        ("forbidden_runes", .noScrolls), ("hostile_champions", .championEnemies),
        ("badder_bosses", .strongerBosses),
    ]

    /// Encodes the query and its seeds as results-file text, or "" when the
    /// engine refuses them (an unusable query, a non-canonical seed code).
    public static func encode(_ query: SavedQuery, seeds: [String], appVersion: String) -> String {
        let request: [String: Any] = [
            "query": encodeQuery(query),
            "seeds": seeds,
            "app_version": appVersion,
        ]
        guard let document = try? JSONSerialization.data(withJSONObject: request),
              let packet = try? enginePacket({ out, length in
                  document.withUnsafeBytes { bytes in
                      seedfinder_results_encode(
                          bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, out, length)
                  }
              }),
              let text = String(data: packet, encoding: .utf8) else { return "" }
        return text
    }

    public static func decode(_ text: String) throws -> Imported {
        let packet: Data
        do {
            packet = try enginePacket { out, length in
                Data(text.utf8).withUnsafeBytes { bytes in
                    seedfinder_results_decode(
                        bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, out, length)
                }
            }
        } catch SeedFinderEngineError.invalidArgument {
            throw ResultsExportError("This is not a Seed Seeker results file this version can import.")
        } catch {
            throw ResultsExportError("The native engine failed while reading the results file.")
        }
        guard let document = (try? JSONSerialization.jsonObject(with: packet)) as? [String: Any],
              let queryValue = document["query"] as? [String: Any],
              let seeds = document["seeds"] as? [String] else {
            throw ResultsExportError("The native engine returned an invalid results document.")
        }
        return Imported(query: try decodeQuery(queryValue), seeds: seeds,
                        dropped: intField(document, "dropped") ?? 0,
                        shpdVersion: document["shpd_version"] as? String)
    }

    // MARK: Document mapping

    // The document the engine hands back has already been validated by it, so
    // these readers only translate: a value of an unexpected shape simply is
    // not there.
    private static func intField(_ entry: [String: Any], _ key: String) -> Int? {
        guard let number = entry[key] as? NSNumber else { return nil }
        return Int(exactly: number)
    }

    private static func boolField(_ entry: [String: Any], _ key: String) -> Bool {
        (entry[key] as? NSNumber)?.boolValue ?? false
    }

    // Internal, not private: the share-link codec (`DeepLink`) and the engine
    // transport (`QueryDocument`) exchange the same canonical query document
    // with the Rust core.
    static func encodeQuery(_ query: SavedQuery) -> [String: Any] {
        // An alternative group is one `any_of` entry at its first member's
        // position, members in requirement order; a lone requirement is written plain.
        let entries: [Any] = query.requirements.slots.map { slot in
            slot.count == 1 ? encodeRequirement(slot[0]) : ["any_of": slot.map(encodeRequirement)]
        }
        var output: [String: Any] = ["requirements": entries]
        if query.maximumDepth != 24 { output["max_depth"] = query.maximumDepth }
        if query.requireBlacksmith { output["require_blacksmith"] = true }
        if query.excludeBlacksmithRewards { output["exclude_blacksmith_rewards"] = true }
        if let quest = query.wandmakerQuest { output["wandmaker_quest"] = quest.documentName }
        if query.fastMode { output["fast_mode"] = true }
        let challenges = challengeNames
            .filter { query.challenges & $0.challenge.rawValue != 0 }
            .map(\.name)
        if !challenges.isEmpty { output["challenges"] = challenges }
        return output
    }

    private static func encodeRequirement(_ requirement: ItemRequirement) -> [String: Any] {
        var output: [String: Any] = ["kind": kindNames[requirement.kind.rawValue]]
        if let item = requirement.item { output["item"] = item.id }
        switch requirement.tierMatch {
        case .any: break
        case .exactly: output["tier"] = ["exact": requirement.tier]
        case .atLeast: output["tier"] = ["at_least": requirement.tier]
        case .atMost: output["tier"] = ["at_most": requirement.tier]
        }
        switch requirement.upgradeMatch {
        case .any: break
        case .exactly: output["upgrade"] = requirement.upgrade
        case .atLeast: output["upgrade"] = ["at_least": requirement.upgrade]
        }
        switch requirement.effect {
        case .any: break
        case .anyEnchantment: output["effect"] = anyEnchantmentName
        case .oneOf(let names): output["effect"] = names.count == 1 ? names[0] : names
        }
        if requirement.requireUncursed { output["uncursed"] = true }
        if let source = requirement.source { output["source"] = sourceNames[source.rawValue] }
        if let group = requirement.identityGroup { output["identity_group"] = group }
        if let depth = requirement.maximumDepth { output["max_depth"] = depth }
        if let sum = requirement.upgradeSum { output["upgrade_sum"] = ["group": sum.group, "at_least": sum.atLeast] }
        return output
    }

    /// The document's shorthand for the family's whole non-curse effect set.
    private static let anyEnchantmentName = "any_enchantment"

    /// Maps a canonical query document onto the Swift models. Only a name this
    /// build has no model for fails — the document itself is the engine's, so
    /// its shape and bounds are already guaranteed.
    static func decodeQuery(_ value: [String: Any]) throws -> SavedQuery {
        var requirements: [ItemRequirement] = []
        // Alternative groups get fresh sequential ids in document order.
        var nextGroup = 1
        for (index, entry) in (value["requirements"] as? [Any] ?? []).enumerated() {
            guard let entry = entry as? [String: Any] else {
                throw ResultsExportError("Requirement \(index + 1) is not a JSON object.")
            }
            do {
                if let members = entry["any_of"] as? [Any] {
                    let group = nextGroup
                    nextGroup += 1
                    for member in members {
                        guard let member = member as? [String: Any] else {
                            throw ResultsExportError("an alternative is not a JSON object")
                        }
                        requirements.append(try decodeRequirement(
                            member, key: Int64(requirements.count + 1), alternativeGroup: group))
                    }
                } else {
                    requirements.append(try decodeRequirement(entry, key: Int64(requirements.count + 1)))
                }
            } catch let failure as ResultsExportError {
                throw ResultsExportError("Requirement \(index + 1): \(failure.message)")
            } catch {
                let reason = (error as? LocalizedError)?.errorDescription ?? "\(error)"
                throw ResultsExportError("Requirement \(index + 1): \(reason)")
            }
        }
        var challenges = 0
        for name in value["challenges"] as? [String] ?? [] {
            guard let match = challengeNames.first(where: { $0.name == name }) else {
                throw ResultsExportError("Unknown challenge \"\(name)\".")
            }
            challenges |= match.challenge.rawValue
        }
        var wandmakerQuest: WandmakerQuest?
        if let name = value["wandmaker_quest"] as? String {
            guard let quest = WandmakerQuest.named(name) else {
                throw ResultsExportError("Unknown Wandmaker quest \"\(name)\".")
            }
            wandmakerQuest = quest
        }
        return SavedQuery(
            requirements: requirements,
            maximumDepth: intField(value, "max_depth") ?? 24,
            requireBlacksmith: boolField(value, "require_blacksmith"),
            excludeBlacksmithRewards: boolField(value, "exclude_blacksmith_rewards"),
            wandmakerQuest: wandmakerQuest,
            fastMode: boolField(value, "fast_mode"),
            challenges: challenges)
    }

    private static func decodeRequirement(_ entry: [String: Any], key: Int64,
                                          alternativeGroup: Int? = nil) throws -> ItemRequirement {
        var item: CatalogItem?
        if let id = entry["item"] as? String {
            guard let found = ItemCatalog.findById(id) else {
                throw ResultsExportError("unknown item \"\(id)\"")
            }
            item = found
        }
        let kind: ItemKind
        if let name = entry["kind"] as? String {
            guard let index = kindNames.firstIndex(of: name),
                  let value = ItemKind(rawValue: index) else {
                throw ResultsExportError("unknown category \"\(name)\"")
            }
            kind = value
        } else if let item {
            kind = item.kind
        } else {
            throw ResultsExportError("a category is required when no item is set")
        }
        var tier = 0
        var tierMatch = TierMatch.any
        if let object = entry["tier"] as? [String: Any] {
            if let exact = intField(object, "exact") { tier = exact; tierMatch = .exactly }
            else if let atLeast = intField(object, "at_least") { tier = atLeast; tierMatch = .atLeast }
            else if let atMost = intField(object, "at_most") { tier = atMost; tierMatch = .atMost }
        }
        var upgrade = 0
        var upgradeMatch = UpgradeMatch.any
        if let object = entry["upgrade"] as? [String: Any] {
            if let exact = intField(object, "exact") { upgrade = exact; upgradeMatch = .exactly }
            else if let atLeast = intField(object, "at_least") { upgrade = atLeast; upgradeMatch = .atLeast }
        } else if let exact = intField(entry, "upgrade") {
            upgrade = exact
            upgradeMatch = .exactly
        }
        // Effect names match case-insensitively and canonicalize to the
        // catalog's own spelling.
        func effectName(_ name: String) throws -> String {
            guard let match = ItemCatalog.modifiersFor(kind)
                .first(where: { $0.caseInsensitiveCompare(name) == .orderedSame }) else {
                throw ResultsExportError("unknown effect \"\(name)\"")
            }
            return match
        }
        var effect = EffectFilter.any
        if let name = entry["effect"] as? String {
            effect = name.caseInsensitiveCompare(anyEnchantmentName) == .orderedSame
                ? .anyEnchantment : .oneOf([try effectName(name)])
        } else if let names = entry["effect"] as? [String] {
            effect = .oneOf(try names.map(effectName))
        }
        var upgradeSum: UpgradeSum?
        if let object = entry["upgrade_sum"] as? [String: Any],
           let group = intField(object, "group"), let atLeast = intField(object, "at_least") {
            upgradeSum = UpgradeSum(group: group, atLeast: atLeast)
        }
        var source: ScoutItemSource?
        if let name = entry["source"] as? String {
            guard let index = sourceNames.firstIndex(of: name),
                  let value = ScoutItemSource(rawValue: index) else {
                throw ResultsExportError("unknown source \"\(name)\"")
            }
            source = value
        }
        return try ItemRequirement(
            key: key,
            item: item,
            upgrade: upgrade,
            effect: effect,
            kind: kind,
            tier: tier,
            tierMatch: tierMatch,
            upgradeMatch: upgradeMatch,
            source: source,
            identityGroup: intField(entry, "identity_group"),
            maximumDepth: intField(entry, "max_depth"),
            requireUncursed: boolField(entry, "uncursed"),
            alternativeGroup: alternativeGroup,
            upgradeSum: upgradeSum)
    }
}

/// The canonical JSON query document as the bytes every query-taking engine
/// entry point accepts: the search, the resumed search, the seed filter, the
/// continuation and start decisions and the scout marks all read the same
/// document the share links and results files carry.
public enum QueryDocument {
    /// The document as a JSON object, before serialization.
    public static func object(_ request: SearchRequest) -> [String: Any] {
        ResultsExport.encodeQuery(SavedQuery(
            requirements: request.requirements, maximumDepth: request.maximumDepth,
            requireBlacksmith: request.requireBlacksmith,
            excludeBlacksmithRewards: request.excludeBlacksmithRewards,
            wandmakerQuest: request.wandmakerQuest, fastMode: request.fastMode,
            challenges: request.challenges))
    }

    /// UTF-8 JSON bytes of the document, keys sorted so equal queries encode
    /// to equal bytes.
    public static func encode(_ request: SearchRequest) throws -> Data {
        try JSONSerialization.data(withJSONObject: object(request), options: [.sortedKeys])
    }
}
