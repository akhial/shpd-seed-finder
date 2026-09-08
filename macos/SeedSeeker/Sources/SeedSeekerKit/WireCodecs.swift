import CSeedFinder
import Foundation

public enum WireCodecError: Error, Equatable, LocalizedError {
    case truncated, badMagic, invalidValue(String), malformedText, trailingBytes
    public var errorDescription: String? {
        switch self {
        case .truncated: "Truncated native packet"
        case .badMagic: "Unexpected native packet"
        case .invalidValue(let message): message
        case .malformedText: "Malformed text in native packet"
        case .trailingBytes: "Trailing bytes in native packet"
        }
    }
}

/// Builds versioned scout requests; queries go to the engine as the canonical
/// JSON document (`QueryDocument`), not as a hand-built packet.
private struct Writer {
    var data = Data()
    mutating func bytes(_ value: some Sequence<UInt8>) { data.append(contentsOf: value) }
    mutating func u16LittleEndian(_ value: Int) { bytes([UInt8(value & 0xff), UInt8((value >> 8) & 0xff)]) }
}

private struct Reader {
    let data: Data
    var offset = 0
    var remaining: Int { data.count - offset }
    mutating func bytes(_ count: Int) throws -> Data {
        guard count >= 0, remaining >= count else { throw WireCodecError.truncated }
        defer { offset += count }
        return data.subdata(in: offset..<(offset + count))
    }
    mutating func u8() throws -> UInt8 { try bytes(1)[0] }
    mutating func u16() throws -> Int { Int(try u8()) << 8 | Int(try u8()) }
    mutating func u64() throws -> UInt64 {
        var value: UInt64 = 0
        for _ in 0..<8 { value = value << 8 | UInt64(try u8()) }
        return value
    }
    mutating func utf8(_ count: Int) throws -> String {
        guard let value = String(data: try bytes(count), encoding: .utf8) else { throw WireCodecError.malformedText }
        return value
    }
    mutating func ascii(_ count: Int) throws -> String {
        let value = try bytes(count)
        guard value.allSatisfy({ $0 <= 0x7f }), let string = String(data: value, encoding: .ascii) else { throw WireCodecError.malformedText }
        return string
    }
}

/// Seed codes are the game's own base-26 text, so reading and masking them is
/// the engine's job: `seedfinder_seed_format` is the as-you-type masker and
/// `seedfinder_seed_parse` the parser, and neither rule is restated here.
public enum SeedCode {
    /// One parsed seed code: the canonical text to display and the numeric
    /// value the engine scans.
    public struct Parsed: Hashable, Sendable {
        public let code: String
        public let value: Int64
    }

    /// Masks partial, as-you-type input into uppercase groups of three.
    public static func formatInput(_ input: String) -> String {
        guard let packet = try? enginePacket({ out, length in
                  Data(input.utf8).withUnsafeBytes { bytes in
                      seedfinder_seed_format(
                          bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, out, length)
                  }
              }), let text = String(data: packet, encoding: .utf8) else { return "" }
        return text
    }

    /// Reads seed-code text with the game's own rules, or nil when it is not a
    /// seed code at all.
    public static func parse(_ input: String) -> Parsed? {
        guard let packet = try? enginePacket({ out, length in
                  Data(input.utf8).withUnsafeBytes { bytes in
                      seedfinder_seed_parse(
                          bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, out, length)
                  }
              }),
              let document = (try? JSONSerialization.jsonObject(with: packet)) as? [String: Any],
              let code = document["code"] as? String,
              let value = (document["value"] as? NSNumber).flatMap({ Int64(exactly: $0) })
        else { return nil }
        return Parsed(code: code, value: value)
    }

    /// Whether `seed` is already written the way the engine spells it: the
    /// canonical `XXX-XXX-XXX` form the app displays and files carry.
    public static func isCanonical(_ seed: String) -> Bool { parse(seed)?.code == seed }
}

public enum ResultCodec {
    public static func decode(_ packet: Data, requirementCount: Int) throws -> [SeedResult] {
        var input = Reader(data: packet)
        guard try input.bytes(4) == Data("SSR1".utf8) else { throw WireCodecError.badMagic }
        let results = try (0..<input.u16()).map { _ in
            let seed = try input.ascii(Int(input.u8()))
            guard SeedCode.isCanonical(seed) else { throw WireCodecError.invalidValue("Malformed seed from native engine") }
            return SeedResult(seed: seed, matchedRequirements: requirementCount)
        }
        guard input.remaining == 0 else { throw WireCodecError.trailingBytes }
        return results
    }
}

public enum ScoutCodec {
    public static func encodeRequest(seed: String, challenges: Int, query: SearchRequest? = nil, trinket: String? = nil) throws -> Data {
        guard SeedCode.isCanonical(seed) else { throw WireCodecError.invalidValue("Seed must use XXX-XXX-XXX format") }
        guard (0...SearchLimits.challengeMask).contains(challenges) else { throw WireCodecError.invalidValue("Challenge mask must be 0..\(SearchLimits.challengeMask)") }
        var output = Writer()
        if query == nil && trinket == nil {
            output.bytes("SSQ2".utf8)
            output.u16LittleEndian(challenges)
            output.bytes(seed.utf8)
        } else {
            let override = trinket ?? ""
            guard override.isEmpty || override == "none" || ItemCatalog.findById(override)?.kind == .trinket else {
                throw WireCodecError.invalidValue("Unknown trinket")
            }
            output.bytes("SSQ3".utf8)
            output.u16LittleEndian(challenges)
            output.u16LittleEndian(seed.utf8.count)
            output.bytes(seed.utf8)
            output.u16LittleEndian(override.utf8.count)
            output.bytes(override.utf8)
            if let query { output.bytes(try QueryDocument.encode(query)) }
        }
        return output.data
    }

    public static func decode(_ packet: Data) throws -> ScoutWorld {
        var input = Reader(data: packet)
        let magic = try input.bytes(4)
        guard magic == Data("SSC3".utf8) || magic == Data("SSC4".utf8) || magic == Data("SSC5".utf8) else { throw WireCodecError.badMagic }
        let seed = try input.ascii(Int(input.u8()))
        guard SeedCode.isCanonical(seed) else { throw WireCodecError.invalidValue("Malformed seed from native scout") }
        // Twelve gem ordinals, one per ring class in the order the catalog
        // lists rings, sitting between the seed and the quests: like them it is
        // a property of this one run rather than of anything an item carries.
        let gemOrdinals = try input.bytes(RingGems.count).map { Int($0) }
        guard let ringGems = RingGems(ordinals: gemOrdinals) else {
            throw WireCodecError.invalidValue("Malformed ring gem table from native scout")
        }
        let questCount = Int(try input.u8())
        guard questCount <= 4 else { throw WireCodecError.invalidValue("Scout quest count must be 0..4") }
        var previousQuestID = 0
        let quests: [ScoutQuest] = try (0..<questCount).map { _ in
            let questID = Int(try input.u8())
            guard let kind = ScoutQuestKind(rawValue: questID) else { throw WireCodecError.invalidValue("Unknown scout quest \(questID)") }
            guard questID > previousQuestID else { throw WireCodecError.invalidValue("Scout quest ids must be strictly ascending") }
            previousQuestID = questID
            let variantID = Int(try input.u8())
            guard (1...kind.variants.count).contains(variantID) else {
                throw WireCodecError.invalidValue("Unknown \(kind.giverLabel) quest variant \(variantID)")
            }
            let depth = Int(try input.u8())
            guard kind.depthRange.contains(depth) else {
                throw WireCodecError.invalidValue("\(kind.giverLabel) quest depth must be \(kind.depthRange.lowerBound)..\(kind.depthRange.upperBound)")
            }
            return ScoutQuest(variant: kind.variants[variantID - 1], depth: depth)
        }
        let items: [ScoutItem] = try (0..<input.u16()).map { _ in
            let stableID = try input.utf8(input.u16())
            guard let item = ItemCatalog.findById(stableID) else { throw WireCodecError.invalidValue("Unknown catalog item '\(stableID)' in native scout packet") }
            let depth = Int(try input.u8())
            guard (1...SearchLimits.maxDepth).contains(depth) else { throw WireCodecError.invalidValue("Scout item depth must be 1..\(SearchLimits.maxDepth)") }
            let upgrade = Int(try input.u8())
            guard (0...item.kind.maximumSearchUpgrade).contains(upgrade) else { throw WireCodecError.invalidValue("Invalid scout item upgrade") }
            let flags = try input.u8()
            guard flags & 0xfc == 0 else { throw WireCodecError.invalidValue("Unknown scout item flags \(flags)") }
            let effectText = try input.utf8(input.u16())
            let effect = effectText.isEmpty ? nil : effectText
            if let effect, !ItemCatalog.modifiersFor(item.kind).contains(effect) { throw WireCodecError.invalidValue("Unknown modifier '\(effect)' for \(item.id)") }
            guard let source = ScoutItemSource(rawValue: Int(try input.u8())) else { throw WireCodecError.invalidValue("Unknown scout item source") }
            let accessibility: ScoutAccessibility
            switch try input.u8() {
            case 0: accessibility = .independent
            case 1:
                let group = try input.u16(), option = Int(try input.u8())
                guard option < 64 else { throw WireCodecError.invalidValue("Scout choice option must be 0..63") }
                accessibility = .choice(group: group, option: option)
            case 2:
                let group = try input.u16(), mask = try input.u64()
                guard mask != 0 else { throw WireCodecError.invalidValue("Scout scenario mask must be non-zero") }
                accessibility = .scenarios(group: group, mask: mask)
            case let tag: throw WireCodecError.invalidValue("Unknown scout accessibility tag \(tag)")
            }
            return ScoutItem(item: item, depth: depth, upgrade: upgrade, effect: effect,
                             cursed: flags & 1 != 0, source: source, accessibility: accessibility,
                             secret: flags & 2 != 0)
        }
        var trinketOrder: [CatalogItem] = []
        if magic != Data("SSC3".utf8) {
            guard try input.u8() == 17 else { throw WireCodecError.invalidValue("Trinket deck must contain 17 entries") }
            for _ in 0..<17 {
                let id = try input.utf8(input.u16())
                guard let item = ItemCatalog.findById(id), item.kind == .trinket,
                      !trinketOrder.contains(item) else {
                    throw WireCodecError.invalidValue("Malformed trinket deck")
                }
                trinketOrder.append(item)
            }
        }
        var selectedTrinket: String?
        if magic == Data("SSC5".utf8) {
            let id = try input.utf8(input.u16())
            guard id.isEmpty || trinketOrder.prefix(4).contains(where: { $0.id == id }) else {
                throw WireCodecError.invalidValue("Selected trinket is not initially offered")
            }
            selectedTrinket = id.isEmpty ? nil : id
        }
        guard input.remaining == 0 else { throw WireCodecError.trailingBytes }
        return ScoutWorld(seed: seed, quests: quests, items: items, ringGems: ringGems, trinketOrder: trinketOrder, selectedTrinket: selectedTrinket)
    }
}
