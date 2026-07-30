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
/// in `docs/results-export-format.md`. Keep this codec schema-compatible with
/// it: unknown envelope and per-result fields are ignored, files declaring a
/// newer `format_version` are rejected with an "update the app" message, and
/// unknown or wrong-typed query content fails the import instead of silently
/// changing the query's meaning.
public enum ResultsExport {
    public static let fileFormat = "seed-seeker-results"
    public static let formatVersion = 1
    public static let suggestedFileName = "seed-seeker-results"
    /// Mirrors the Rust core's `SHPD_VERSION`, the source of truth.
    public static let shpdVersion = "3.3.8"
    /// Import size cap; a maximal legal results file is far below this.
    public static let maxFileBytes = 2 * 1024 * 1024

    public struct Imported: Sendable {
        public let query: SavedQuery
        public let seeds: [String]
        /// The upstream game version the file declares, if any.
        public let shpdVersion: String?
        public init(query: SavedQuery, seeds: [String], shpdVersion: String?) {
            self.query = query; self.seeds = seeds; self.shpdVersion = shpdVersion
        }
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
        guard let versionValue = document["format_version"] else {
            throw ResultsExportError("This results file is missing its format version.")
        }
        // Strictly a positive integer: NSNumber bridging would otherwise let
        // `true` or `1.5` slip through `as? Int`.
        guard let version = strictInt(versionValue), version >= 1 else {
            throw ResultsExportError(
                "This results file does not declare a valid format version (a positive whole number).")
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
                throw ResultsExportError(
                    "Result \(index + 1) does not have a valid seed code (canonical XXX-XXX-XXX form).")
            }
            return seed
        }
        return Imported(query: query, seeds: seeds,
                        shpdVersion: document["shpd_version"] as? String)
    }

    // MARK: Strict typed readers

    /// A present-but-wrong-type value is an error, never coerced or treated
    /// as absent. JSON booleans and integers both bridge to `NSNumber`, so
    /// the two are told apart via `CFBoolean`, and fractions are rejected.
    private static func strictInt(_ value: Any) -> Int? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID(),
              let exact = Int(exactly: number) else { return nil }
        return exact
    }

    private static func strictBool(_ value: Any) -> Bool? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) == CFBooleanGetTypeID() else { return nil }
        return number.boolValue
    }

    private static func intField(_ entry: [String: Any], _ key: String) throws -> Int? {
        guard let value = entry[key], !(value is NSNull) else { return nil }
        guard let number = strictInt(value) else {
            throw ResultsExportError("\"\(key)\" must be a whole number")
        }
        return number
    }

    private static func stringField(_ entry: [String: Any], _ key: String) throws -> String? {
        guard let value = entry[key], !(value is NSNull) else { return nil }
        guard let text = value as? String, !(value is NSNumber) else {
            throw ResultsExportError("\"\(key)\" must be a string")
        }
        return text
    }

    private static func boolField(_ entry: [String: Any], _ key: String) throws -> Bool {
        guard let value = entry[key] else { return false }
        guard let flag = strictBool(value) else {
            throw ResultsExportError("\"\(key)\" must be true or false")
        }
        return flag
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
        let maximumDepth = try intField(value, "max_depth") ?? 24
        guard (1...24).contains(maximumDepth) else {
            throw ResultsExportError("Maximum floor must be 1..24.")
        }
        var challenges = 0
        if let challengesValue = value["challenges"], !(challengesValue is NSNull) {
            guard let names = challengesValue as? [Any] else {
                throw ResultsExportError("\"challenges\" must be a list of challenge names")
            }
            for nameValue in names {
                // Challenge names match the core decoder exactly.
                guard let name = nameValue as? String,
                      let match = challengeNames.first(where: { $0.name == name }) else {
                    throw ResultsExportError(
                        "The query in this results file uses an unknown challenge \"\(nameValue)\".")
                }
                challenges |= match.challenge.rawValue
            }
        }
        return try SavedQuery(
            requirements: requirements,
            maximumDepth: maximumDepth,
            requireBlacksmith: boolField(value, "require_blacksmith"),
            excludeBlacksmithRewards: boolField(value, "exclude_blacksmith_rewards"),
            fastMode: boolField(value, "fast_mode"),
            challenges: challenges)
    }

    private static func decodeRequirement(_ entry: [String: Any], key: Int64) throws -> ItemRequirement {
        for field in entry.keys where !requirementKeys.contains(field) {
            throw ResultsExportError("unknown field \"\(field)\" — update Seed Seeker to import it")
        }
        var item: CatalogItem?
        if let id = try stringField(entry, "item") {
            guard let found = ItemCatalog.findById(id) else {
                throw ResultsExportError("unknown item \"\(id)\"")
            }
            item = found
        }
        // Enum names match the core decoder exactly (lowercase snake_case);
        // only effect names and the "any" keyword match case-insensitively.
        let kind: ItemKind
        if let name = try stringField(entry, "kind") {
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
        if let tierValue = entry["tier"] {
            if let name = tierValue as? String, !(tierValue is NSNumber) {
                guard name.lowercased() == "any" else {
                    throw ResultsExportError("unknown tier mode \"\(name)\"")
                }
            } else if let object = tierValue as? [String: Any], object.count == 1 {
                if object["exact"] != nil { tier = try intField(object, "exact") ?? 0; tierMatch = .exactly }
                else if object["at_least"] != nil { tier = try intField(object, "at_least") ?? 0; tierMatch = .atLeast }
                else if object["at_most"] != nil { tier = try intField(object, "at_most") ?? 0; tierMatch = .atMost }
                else { throw ResultsExportError("unrecognized tier filter") }
            } else {
                throw ResultsExportError("unrecognized tier filter")
            }
        }
        var upgrade = 0
        var upgradeMatch = UpgradeMatch.any
        if let upgradeValue = entry["upgrade"] {
            if let name = upgradeValue as? String, !(upgradeValue is NSNumber) {
                guard name.lowercased() == "any" else {
                    throw ResultsExportError("unknown upgrade mode \"\(name)\"")
                }
            } else if let object = upgradeValue as? [String: Any], object.count == 1 {
                if object["exact"] != nil { upgrade = try intField(object, "exact") ?? 0; upgradeMatch = .exactly }
                else if object["at_least"] != nil { upgrade = try intField(object, "at_least") ?? 0; upgradeMatch = .atLeast }
                else { throw ResultsExportError("unrecognized upgrade filter") }
            } else if let number = strictInt(upgradeValue) {
                upgrade = number
                upgradeMatch = .exactly
            } else {
                throw ResultsExportError("unrecognized upgrade filter")
            }
        }
        var modifier: String?
        if let name = try stringField(entry, "effect") {
            guard let match = ItemCatalog.modifiersFor(kind)
                .first(where: { $0.caseInsensitiveCompare(name) == .orderedSame }) else {
                throw ResultsExportError("unknown effect \"\(name)\"")
            }
            modifier = match
        }
        var source: ScoutItemSource?
        if let name = try stringField(entry, "source") {
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
            modifier: modifier,
            kind: kind,
            tier: tier,
            tierMatch: tierMatch,
            upgradeMatch: upgradeMatch,
            source: source,
            identityGroup: try intField(entry, "identity_group"),
            maximumDepth: try intField(entry, "max_depth"),
            requireUncursed: try boolField(entry, "uncursed"))
    }
}
