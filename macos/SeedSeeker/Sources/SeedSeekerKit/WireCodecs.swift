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

private struct Writer {
    var data = Data()
    mutating func bytes(_ value: some Sequence<UInt8>) { data.append(contentsOf: value) }
    mutating func u8(_ value: Int) { data.append(UInt8(value)) }
    mutating func u16(_ value: Int) { bytes([UInt8((value >> 8) & 0xff), UInt8(value & 0xff)]) }
    mutating func u16LittleEndian(_ value: Int) { bytes([UInt8(value & 0xff), UInt8((value >> 8) & 0xff)]) }
    mutating func text(_ value: String) throws {
        let encoded = Array(value.utf8)
        guard encoded.count <= 65_535 else { throw WireCodecError.invalidValue("Wire string is too long") }
        u16(encoded.count); bytes(encoded)
    }
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

public enum SeedCode {
    public static func formatInput(_ input: String) -> String {
        let letters = input.uppercased(with: Locale(identifier: "en_US_POSIX"))
            .unicodeScalars.filter { (65...90).contains($0.value) }.prefix(9).map(String.init).joined()
        return stride(from: 0, to: letters.count, by: 3).map { start in
            let lower = letters.index(letters.startIndex, offsetBy: start)
            let upper = letters.index(lower, offsetBy: min(3, letters.count - start))
            return String(letters[lower..<upper])
        }.joined(separator: "-")
    }
    public static func isCanonical(_ seed: String) -> Bool {
        seed.wholeMatch(of: /^[A-Z]{3}-[A-Z]{3}-[A-Z]{3}$/) != nil
    }
}

public enum QueryCodec {
    public static func encode(_ request: SearchRequest) throws -> Data {
        var output = Writer(); output.bytes("SSF7".utf8); output.u8(request.maximumDepth)
        output.u8((request.requireBlacksmith ? 1 : 0)
            | (request.fastMode ? 2 : 0)
            | (request.excludeBlacksmithRewards ? 4 : 0))
        output.u16LittleEndian(request.challenges)
        output.u16(request.requirements.count)
        for requirement in request.requirements {
            output.u8(requirement.kind.rawValue); try output.text(requirement.item?.id ?? "")
            output.u8(requirement.tierMatch.rawValue); output.u8(requirement.tier)
            output.u8(requirement.upgradeMatch.rawValue); output.u8(requirement.upgrade)
            try output.text(requirement.modifier ?? "")
            output.u8(requirement.source.map { $0.rawValue + 1 } ?? 0)
            output.u8(requirement.identityGroup ?? 0)
            output.u8(requirement.maximumDepth ?? 0)
            output.u8(requirement.requireUncursed ? 1 : 0)
        }
        return output.data
    }
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
    public static func encodeRequest(seed: String, challenges: Int) throws -> Data {
        guard SeedCode.isCanonical(seed) else { throw WireCodecError.invalidValue("Seed must use XXX-XXX-XXX format") }
        guard (0...511).contains(challenges) else { throw WireCodecError.invalidValue("Challenge mask must be 0..511") }
        var output = Writer()
        output.bytes("SSQ2".utf8)
        output.u16LittleEndian(challenges)
        output.bytes(seed.utf8)
        return output.data
    }

    public static func decode(_ packet: Data) throws -> ScoutWorld {
        var input = Reader(data: packet)
        guard try input.bytes(4) == Data("SSC1".utf8) else { throw WireCodecError.badMagic }
        let seed = try input.ascii(Int(input.u8()))
        guard SeedCode.isCanonical(seed) else { throw WireCodecError.invalidValue("Malformed seed from native scout") }
        let items: [ScoutItem] = try (0..<input.u16()).map { _ in
            let stableID = try input.utf8(input.u16())
            guard let item = ItemCatalog.findById(stableID) else { throw WireCodecError.invalidValue("Unknown catalog item '\(stableID)' in native scout packet") }
            let depth = Int(try input.u8())
            guard (1...24).contains(depth) else { throw WireCodecError.invalidValue("Scout item depth must be 1..24") }
            let upgrade = Int(try input.u8())
            guard (0...item.kind.maximumSearchUpgrade).contains(upgrade) else { throw WireCodecError.invalidValue("Invalid scout item upgrade") }
            let flags = try input.u8()
            guard flags & 0xfe == 0 else { throw WireCodecError.invalidValue("Unknown scout item flags \(flags)") }
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
                             cursed: flags & 1 != 0, source: source, accessibility: accessibility)
        }
        guard input.remaining == 0 else { throw WireCodecError.trailingBytes }
        return ScoutWorld(seed: seed, items: items)
    }
}
