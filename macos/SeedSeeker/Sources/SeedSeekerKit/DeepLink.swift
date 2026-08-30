import CSeedFinder
import Foundation

/// User-facing failure while encoding or decoding a query share link.
public struct DeepLinkError: Error, LocalizedError, Equatable {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

/// Shareable-link codec for search queries, backed by the Rust core.
///
/// A deep link carries a whole query as a compact code in a web link like
/// `https://shpd-seed-seeker.web.app/#q=EAGWhMA`. Decoding accepts any link
/// form the core recognizes: the full web link, a `seedseeker://` link, or
/// the bare code.
///
/// The code's format version is the core's business: it writes the oldest
/// version a query fits — version 3 only when an effect set names one of the
/// weapon enchantments v4.0.0 appended — and reads versions 1 through 3, so
/// links shared by older releases keep opening here.
public enum DeepLink {
    /// Custom URL scheme registered by the app bundle's Info.plist.
    public static let scheme = "seedseeker"

    /// Encodes a query as a full shareable web link.
    public static func encodeLink(for query: SavedQuery) throws -> String {
        let document = try JSONSerialization.data(
            withJSONObject: ResultsExport.encodeQuery(query))
        let packet = try sharePacket(
            document,
            invalid: "The current query cannot be shared as a link. "
                + "It needs at least one valid requirement."
        ) { pointer, count, outPacket, outLength in
            seedfinder_share_encode(pointer, count, outPacket, outLength)
        }
        guard let link = String(data: packet, encoding: .utf8), !link.isEmpty else {
            throw DeepLinkError("The native engine returned an invalid link.")
        }
        return link
    }

    /// Decodes any accepted link text back into the query it carries.
    public static func decode(_ text: String) throws -> SavedQuery {
        let packet = try sharePacket(
            Data(text.utf8),
            invalid: "This is not a Seed Seeker query link."
        ) { pointer, count, outPacket, outLength in
            seedfinder_share_decode(pointer, count, outPacket, outLength)
        }
        guard let object = (try? JSONSerialization.jsonObject(with: packet)) as? [String: Any]
        else {
            throw DeepLinkError("The native engine returned an invalid query document.")
        }
        return try ResultsExport.decodeQuery(object)
    }

    /// Runs one share FFI call and copies out its packet; both calls are
    /// pure, fast bit twiddling, so they stay synchronous.
    private static func sharePacket(
        _ input: Data, invalid: String,
        _ call: (UnsafePointer<UInt8>?, Int,
                 UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
                 UnsafeMutablePointer<Int>?) -> Int32
    ) throws -> Data {
        var pointer: UnsafeMutablePointer<UInt8>?
        var length = 0
        let code = input.withUnsafeBytes { bytes in
            call(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, &pointer, &length)
        }
        guard code == 0 else {
            throw code == -1 ? DeepLinkError(invalid)
                : DeepLinkError("The native engine failed while processing the link.")
        }
        guard let pointer else {
            throw DeepLinkError("The native engine returned an invalid response.")
        }
        defer { seedfinder_buffer_free(pointer, length) }
        return Data(bytes: pointer, count: length)
    }
}
