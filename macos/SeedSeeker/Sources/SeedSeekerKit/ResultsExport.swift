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
/// The canonical implementation and compatibility rules live in the Rust core
/// (`crates/seedfinder-core/src/results_export.rs`); the schema is documented
/// in `docs/results-export-format.md`. Keep this codec byte-compatible with
/// it: unknown envelope and per-result fields are ignored, files declaring a
/// newer `format_version` are rejected with an "update the app" message, and
/// unknown query content fails the import instead of silently changing the
/// query's meaning.
public enum ResultsExport {
    public static let fileFormat = "seed-seeker-results"
    public static let formatVersion = 1
    public static let suggestedFileName = "seed-seeker-results"
    public static let shpdVersion = "3.3.8"

    public struct Imported: Sendable {
        public let query: SavedQuery
        public let seeds: [String]
        public init(query: SavedQuery, seeds: [String]) { self.query = query; self.seeds = seeds }
    }

    /// Stable document names, indexed by the matching enum raw value.
    private static let kindNames = ["weapon", "armor", "wand", "ring"]
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
    private static let queryKeys: Set<String> = [
        "requirements", "max_depth", "require_blacksmith",
        "exclude_blacksmith_rewards", "fast_mode", "challenges",
    ]
    private static let requirementKeys: Set<String> = [
        "kind", "item", "tier", "upgrade", "effect", "uncursed", "source",
        "identity_group", "max_depth",
    ]

    public static func encode(_ query: SavedQuery, seeds: [String], appVersion: String) -> String {
        let document: [String: Any] = [
            "format": fileFormat,
            "format_version": formatVersion,
            "app_version": appVersion,
            "shpd_version": shpdVersion,
            "query": encodeQuery(query),
            "results": seeds.map { ["seed": $0] },
        ]
        guard let data = try? JSONSerialization.data(
            withJSONObject: document, options: [.prettyPrinted, .sortedKeys]) else { return "" }
        return String(data: data, encoding: .utf8) ?? ""
    }

    public static func decode(_ text: String) throws -> Imported {
        guard let data = text.data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data),
              let document = parsed as? [String: Any],
              document["format"] as? String == fileFormat else {
            throw ResultsExportError("This is not a Seed Seeker results file.")
        }
        guard let version = document["format_version"] as? Int, version >= 1 else {
            throw ResultsExportError("This results file is missing its format version.")
        }
        guard version <= formatVersion else {
            throw ResultsExportError(
                "This results file uses format version \(version), but this app understands " +
                "up to version \(formatVersion). Update Seed Seeker to import it.")
        }
        guard let queryValue = document["query"] as? [String: Any] else {
            throw ResultsExportError("This results file is missing its query.")
        }
        let query = try decodeQuery(queryValue)
        guard let resultsValue = document["results"] as? [Any] else {
            throw ResultsExportError("This results file is missing its results list.")
        }
        let seeds = try resultsValue.enumerated().map { index, entry -> String in
            guard let entry = entry as? [String: Any],
                  let seed = entry["seed"] as? String, isSeedCode(seed) else {
                throw ResultsExportError("Result \(index + 1) does not have a valid seed code.")
            }
            return seed
        }
        return Imported(query: query, seeds: seeds)
    }

    private static func isSeedCode(_ text: String) -> Bool {
        let characters = Array(text)
        guard characters.count == 11 else { return false }
        for (index, character) in characters.enumerated() {
            if index == 3 || index == 7 {
                guard character == "-" else { return false }
            } else {
                guard character.isASCII, character.isUppercase, character.isLetter else { return false }
            }
        }
        return true
    }

    private static func encodeQuery(_ query: SavedQuery) -> [String: Any] {
        var output: [String: Any] = ["requirements": query.requirements.map(encodeRequirement)]
        if query.maximumDepth != 24 { output["max_depth"] = query.maximumDepth }
        if query.requireBlacksmith { output["require_blacksmith"] = true }
        if query.excludeBlacksmithRewards { output["exclude_blacksmith_rewards"] = true }
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
        if let modifier = requirement.modifier { output["effect"] = modifier }
        if requirement.requireUncursed { output["uncursed"] = true }
        if let source = requirement.source { output["source"] = sourceNames[source.rawValue] }
        if let group = requirement.identityGroup { output["identity_group"] = group }
        if let depth = requirement.maximumDepth { output["max_depth"] = depth }
        return output
    }

    private static func decodeQuery(_ value: [String: Any]) throws -> SavedQuery {
        for key in value.keys where !queryKeys.contains(key) {
            throw ResultsExportError(
                "The query in this results file uses an unknown field \"\(key)\". " +
                "Update Seed Seeker to import it.")
        }
        guard let requirementsValue = value["requirements"] as? [Any], !requirementsValue.isEmpty else {
            throw ResultsExportError("The query in this results file has no requirements.")
        }
        let requirements = try requirementsValue.enumerated().map { index, entry -> ItemRequirement in
            guard let entry = entry as? [String: Any] else {
                throw ResultsExportError("Requirement \(index + 1) is not a JSON object.")
            }
            do {
                return try decodeRequirement(entry, key: Int64(index + 1))
            } catch let failure as ResultsExportError {
                throw ResultsExportError("Requirement \(index + 1): \(failure.message)")
            } catch {
                let reason = (error as? LocalizedError)?.errorDescription ?? "\(error)"
                throw ResultsExportError("Requirement \(index + 1): \(reason)")
            }
        }
        let maximumDepth = value["max_depth"] as? Int ?? 24
        guard (1...24).contains(maximumDepth) else {
            throw ResultsExportError("Maximum floor must be 1..24.")
        }
        var challenges = 0
        if let names = value["challenges"] as? [Any] {
            for nameValue in names {
                guard let name = nameValue as? String,
                      let match = challengeNames.first(where: { $0.name == name.lowercased() }) else {
                    throw ResultsExportError(
                        "The query in this results file uses an unknown challenge \"\(nameValue)\".")
                }
                challenges |= match.challenge.rawValue
            }
        }
        return SavedQuery(
            requirements: requirements,
            maximumDepth: maximumDepth,
            requireBlacksmith: value["require_blacksmith"] as? Bool ?? false,
            excludeBlacksmithRewards: value["exclude_blacksmith_rewards"] as? Bool ?? false,
            fastMode: value["fast_mode"] as? Bool ?? false,
            challenges: challenges)
    }

    private static func decodeRequirement(_ entry: [String: Any], key: Int64) throws -> ItemRequirement {
        for field in entry.keys where !requirementKeys.contains(field) {
            throw ResultsExportError("unknown field \"\(field)\" — update Seed Seeker to import it")
        }
        var item: CatalogItem?
        if let id = entry["item"] as? String {
            guard let found = ItemCatalog.findById(id) else {
                throw ResultsExportError("unknown item \"\(id)\"")
            }
            item = found
        }
        let kind: ItemKind
        if let name = entry["kind"] as? String {
            guard let index = kindNames.firstIndex(of: name.lowercased()),
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
        if let tierValue = entry["tier"] {
            if let name = tierValue as? String {
                guard name.lowercased() == "any" else {
                    throw ResultsExportError("unknown tier mode \"\(name)\"")
                }
            } else if let object = tierValue as? [String: Any], object.count == 1 {
                if let exact = object["exact"] as? Int { tier = exact; tierMatch = .exactly }
                else if let atLeast = object["at_least"] as? Int { tier = atLeast; tierMatch = .atLeast }
                else if let atMost = object["at_most"] as? Int { tier = atMost; tierMatch = .atMost }
                else { throw ResultsExportError("unrecognized tier filter") }
            } else {
                throw ResultsExportError("unrecognized tier filter")
            }
        }
        var upgrade = 0
        var upgradeMatch = UpgradeMatch.any
        if let upgradeValue = entry["upgrade"] {
            if let name = upgradeValue as? String {
                guard name.lowercased() == "any" else {
                    throw ResultsExportError("unknown upgrade mode \"\(name)\"")
                }
            } else if let object = upgradeValue as? [String: Any], object.count == 1 {
                if let exact = object["exact"] as? Int { upgrade = exact; upgradeMatch = .exactly }
                else if let atLeast = object["at_least"] as? Int { upgrade = atLeast; upgradeMatch = .atLeast }
                else { throw ResultsExportError("unrecognized upgrade filter") }
            } else if let number = upgradeValue as? Int {
                upgrade = number
                upgradeMatch = .exactly
            } else {
                throw ResultsExportError("unrecognized upgrade filter")
            }
        }
        var modifier: String?
        if let name = entry["effect"] as? String {
            guard let match = ItemCatalog.modifiersFor(kind)
                .first(where: { $0.caseInsensitiveCompare(name) == .orderedSame }) else {
                throw ResultsExportError("unknown effect \"\(name)\"")
            }
            modifier = match
        }
        var source: ScoutItemSource?
        if let name = entry["source"] as? String {
            guard let index = sourceNames.firstIndex(of: name.lowercased()),
                  let value = ScoutItemSource(rawValue: index) else {
                throw ResultsExportError("unknown source \"\(name)\"")
            }
            source = value
        }
        return try ItemRequirement(
            key: key,
            item: item,
            upgrade: upgrade,
            modifier: modifier,
            kind: kind,
            tier: tier,
            tierMatch: tierMatch,
            upgradeMatch: upgradeMatch,
            source: source,
            identityGroup: entry["identity_group"] as? Int,
            maximumDepth: entry["max_depth"] as? Int,
            requireUncursed: entry["uncursed"] as? Bool ?? false)
    }
}
