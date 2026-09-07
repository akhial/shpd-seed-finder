import Foundation

/// Shattered Pixel Dungeon's generated-equipment catalog, parsed from the
/// shared upstream asset rather than hand-copied into Swift.
///
/// The asset is the one every front-end reads
/// (`android/app/src/main/assets/third_party/shattered-pixel-dungeon/
/// catalog-v4.0.0.json`), installed into the app bundle by
/// `scripts/build-macos-app.sh` beside the atlases it indexes, so no platform
/// keeps a second copy of the item list, its tiers and sprites, or the
/// enchantment, glyph and curse names.
public enum ItemCatalog {
    private struct Document: Decodable {
        struct Entry: Decodable {
            let id: String
            let name: String
            let type: String
            let weaponClass: String?
            let tier: Int?
            let sprite: Int
            /// Present only on rings: the class's glyph cell in
            /// `item_icons.png`, which the asset states rather than leaving
            /// each front-end to derive it from `sprite`.
            let typeIcon: Int?

            private enum CodingKeys: String, CodingKey {
                case id, name, type, tier, sprite, typeIcon
                case weaponClass = "class"
            }
        }
        struct Modifiers: Decodable {
            let weaponEnchantments: [String]
            let weaponCurses: [String]
            let armorGlyphs: [String]
            let armorCurses: [String]
        }
        let entries: [Entry]
        let modifiers: Modifiers
    }

    /// The catalog file, wherever this build reaches it: `Contents/Resources`
    /// of the `.app`, where `scripts/build-macos-app.sh` installs it beside the
    /// atlases, or the repository checkout itself under `swift test` and
    /// `swift run`, located relative to this source file.
    private static var catalogURL: URL {
        let installed = Bundle.main.resourceURL?.appendingPathComponent("catalog-v4.0.0.json")
        if let installed, FileManager.default.fileExists(atPath: installed.path) { return installed }
        return URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // Catalog.swift
            .deletingLastPathComponent() // SeedSeekerKit
            .deletingLastPathComponent() // Sources
            .deletingLastPathComponent() // SeedSeeker
            .deletingLastPathComponent() // macos
            .appendingPathComponent(
                "android/app/src/main/assets/third_party/shattered-pixel-dungeon/catalog-v4.0.0.json")
    }

    private static let document: Document = {
        guard let data = try? Data(contentsOf: catalogURL),
              let document = try? JSONDecoder().decode(Document.self, from: data) else {
            preconditionFailure("the bundled item catalog is missing or unreadable")
        }
        return document
    }()

    private static func items(_ type: String, weaponClass: String? = nil) -> [CatalogItem] {
        let kind: ItemKind = switch type {
        case "weapon": .weapon
        case "armor": .armor
        case "wand": .wand
        case "ring": .ring
        case "trinket": .trinket
        case "artifact": .artifact
        default: preconditionFailure("the item catalog names an unknown type \"\(type)\"")
        }
        return document.entries
            .filter { $0.type == type && (weaponClass == nil || $0.weaponClass == weaponClass) }
            .map { CatalogItem(id: $0.id, name: $0.name, kind: kind, spriteIndex: $0.sprite,
                               tier: $0.tier, typeIconIndex: $0.typeIcon) }
    }

    public static let meleeWeapons = items("weapon", weaponClass: "melee")
    public static let thrownWeapons = items("weapon", weaponClass: "thrown")
    public static let armor = items("armor")
    public static let wands = items("wand")
    public static let rings = items("ring")
    public static let trinkets = items("trinket")
    public static let artifacts = items("artifact")
    public static let weapons = meleeWeapons + thrownWeapons
    public static let all = weapons + armor + wands + rings + trinkets + artifacts
    private static let thrownIDs = Set(thrownWeapons.map(\.id))

    /// Melee/thrown classification of one catalog ID; nil for non-weapons.
    public static func weaponClass(of id: String) -> WeaponClass? {
        guard let item = findById(id), item.kind == .weapon else { return nil }
        return thrownIDs.contains(id) ? .thrown : .melee
    }
    public static let enchantments = document.modifiers.weaponEnchantments
    public static let weaponCurses = document.modifiers.weaponCurses
    public static let glyphs = document.modifiers.armorGlyphs
    public static let armorCurses = document.modifiers.armorCurses
    public static func forKind(_ kind: ItemKind) -> [CatalogItem] { switch kind { case .weapon: weapons; case .meleeWeapon: meleeWeapons; case .thrownWeapon: thrownWeapons; case .armor: armor; case .wand: wands; case .ring: rings; case .trinket: trinkets; case .artifact: artifacts } }
    public static func findById(_ id: String) -> CatalogItem? { all.first { $0.id == id } }
    /// Every effect the family can carry in the shared asset's order — the
    /// non-curse effects alphabetically, then the curses alphabetically — which
    /// is also the order effect lists take in the canonical query document.
    public static func modifiersFor(_ kind: ItemKind) -> [String] { switch kind.family { case .weapon: enchantments + weaponCurses; case .armor: glyphs + armorCurses; default: [] } }
    /// The family's non-curse effects in asset order.
    public static func enchantmentsFor(_ kind: ItemKind) -> [String] {
        let curses = cursesFor(kind)
        return modifiersFor(kind).filter { !curses.contains($0) }
    }
    public static func cursesFor(_ kind: ItemKind) -> [String] { switch kind.family { case .weapon: weaponCurses; case .armor: armorCurses; default: [] } }
}
