import Foundation

/// Shattered Pixel Dungeon's generated-equipment catalog, parsed from the
/// shared upstream asset rather than hand-copied into Swift.
///
/// The asset is the one every front-end reads
/// (`android/app/src/main/assets/third_party/shattered-pixel-dungeon/
/// catalog-v3.3.8.json`), reached here through the `Resources` symlink so no
/// platform keeps a second copy of the item list, its tiers and sprites, or
/// the enchantment, glyph and curse names.
public enum ItemCatalog {
    private struct Document: Decodable {
        struct Entry: Decodable {
            let id: String
            let name: String
            let type: String
            let weaponClass: String?
            let tier: Int?
            let sprite: Int

            private enum CodingKeys: String, CodingKey {
                case id, name, type, tier, sprite
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

    private static let document: Document = {
        guard let url = Bundle.module.url(forResource: "catalog-v3.3.8", withExtension: "json"),
              let data = try? Data(contentsOf: url),
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
        default: preconditionFailure("the item catalog names an unknown type \"\(type)\"")
        }
        return document.entries
            .filter { $0.type == type && (weaponClass == nil || $0.weaponClass == weaponClass) }
            .map { CatalogItem(id: $0.id, name: $0.name, kind: kind, spriteIndex: $0.sprite, tier: $0.tier) }
    }

    public static let meleeWeapons = items("weapon", weaponClass: "melee")
    public static let thrownWeapons = items("weapon", weaponClass: "thrown")
    public static let armor = items("armor")
    public static let wands = items("wand")
    public static let rings = items("ring")
    public static let weapons = meleeWeapons + thrownWeapons
    public static let all = weapons + armor + wands + rings
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
    public static func forKind(_ kind: ItemKind) -> [CatalogItem] { switch kind { case .weapon: weapons; case .meleeWeapon: meleeWeapons; case .thrownWeapon: thrownWeapons; case .armor: armor; case .wand: wands; case .ring: rings } }
    public static func findById(_ id: String) -> CatalogItem? { all.first { $0.id == id } }
    public static func modifiersFor(_ kind: ItemKind) -> [String] { switch kind.family { case .weapon: enchantments + weaponCurses; case .armor: glyphs + armorCurses; default: [] } }
    public static func cursesFor(_ kind: ItemKind) -> [String] { switch kind.family { case .weapon: weaponCurses; case .armor: armorCurses; default: [] } }
}
