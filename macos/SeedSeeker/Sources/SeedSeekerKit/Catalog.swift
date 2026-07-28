import Foundation

public enum ItemCatalog {
    private static func weapon(_ id: String, _ name: String, _ tier: Int, _ sprite: Int) -> CatalogItem { .init(id: id, name: name, kind: .weapon, spriteIndex: sprite, tier: tier) }
    /// A weapon the scout reports but the item picker never offers.
    private static func scoutOnlyWeapon(_ id: String, _ name: String, _ tier: Int, _ sprite: Int) -> CatalogItem { .init(id: id, name: name, kind: .weapon, spriteIndex: sprite, tier: tier, requestable: false) }
    private static func armorItem(_ id: String, _ name: String, _ tier: Int, _ sprite: Int) -> CatalogItem { .init(id: id, name: name, kind: .armor, spriteIndex: sprite, tier: tier) }
    private static func wand(_ id: String, _ name: String, _ sprite: Int) -> CatalogItem { .init(id: id, name: name, kind: .wand, spriteIndex: sprite) }
    private static func ring(_ id: String, _ name: String, _ sprite: Int) -> CatalogItem { .init(id: id, name: name, kind: .ring, spriteIndex: sprite) }

    public static let weapons = [
        weapon("worn_shortsword", "Worn Shortsword", 1, 96), weapon("cudgel", "Cudgel", 1, 97), weapon("gloves", "Studded Gloves", 1, 98), weapon("rapier", "Rapier", 1, 99), weapon("dagger", "Dagger", 1, 100),
        weapon("shortsword", "Shortsword", 2, 104), weapon("hand_axe", "Hand Axe", 2, 105), weapon("spear", "Spear", 2, 106), weapon("quarterstaff", "Quarterstaff", 2, 107), weapon("dirk", "Dirk", 2, 108), weapon("sickle", "Sickle", 2, 109),
        weapon("sword", "Sword", 3, 112), weapon("mace", "Mace", 3, 113), weapon("scimitar", "Scimitar", 3, 114), weapon("round_shield", "Round Shield", 3, 115), weapon("sai", "Sai", 3, 116), weapon("whip", "Whip", 3, 117),
        weapon("longsword", "Longsword", 4, 120), weapon("battle_axe", "Battle Axe", 4, 121), weapon("flail", "Flail", 4, 122), weapon("runic_blade", "Runic Blade", 4, 123), weapon("assassins_blade", "Assassin's Blade", 4, 124), weapon("crossbow", "Crossbow", 4, 125), weapon("katana", "Katana", 4, 126),
        weapon("greatsword", "Greatsword", 5, 128), weapon("war_hammer", "War Hammer", 5, 129), weapon("glaive", "Glaive", 5, 130), weapon("greataxe", "Greataxe", 5, 131), weapon("greatshield", "Greatshield", 5, 132), weapon("gauntlet", "Stone Gauntlet", 5, 133), weapon("war_scythe", "War Scythe", 5, 134),
        weapon("throwing_stone", "Throwing Stone", 1, 147), weapon("throwing_knife", "Throwing Knife", 1, 146), weapon("throwing_spike", "Throwing Spike", 1, 145), weapon("fishing_spear", "Fishing Spear", 2, 148), weapon("throwing_club", "Throwing Club", 2, 150), weapon("shuriken", "Shuriken", 2, 149), weapon("throwing_spear", "Throwing Spear", 3, 151), weapon("kunai", "Kunai", 3, 153), weapon("bolas", "Bolas", 3, 152), weapon("javelin", "Javelin", 4, 154), weapon("tomahawk", "Tomahawk", 4, 155), weapon("heavy_boomerang", "Heavy Boomerang", 4, 156), weapon("trident", "Trident", 5, 157), weapon("throwing_hammer", "Throwing Hammer", 5, 158), weapon("force_cube", "Force Cube", 5, 159),
        scoutOnlyWeapon("rot_dart", "Rot Dart", 2, 161), scoutOnlyWeapon("incendiary_dart", "Incendiary Dart", 2, 162), scoutOnlyWeapon("adrenaline_dart", "Adrenaline Dart", 2, 163), scoutOnlyWeapon("healing_dart", "Healing Dart", 2, 164), scoutOnlyWeapon("chilling_dart", "Chilling Dart", 2, 165), scoutOnlyWeapon("shocking_dart", "Shocking Dart", 2, 166), scoutOnlyWeapon("poison_dart", "Poison Dart", 2, 167), scoutOnlyWeapon("cleansing_dart", "Cleansing Dart", 2, 168), scoutOnlyWeapon("paralytic_dart", "Paralytic Dart", 2, 169), scoutOnlyWeapon("holy_dart", "Holy Dart", 2, 170), scoutOnlyWeapon("displacing_dart", "Displacing Dart", 2, 171), scoutOnlyWeapon("blinding_dart", "Blinding Dart", 2, 172),
    ]
    public static let armor = [armorItem("cloth_armor", "Cloth Armor", 1, 176), armorItem("leather_armor", "Leather Armor", 2, 177), armorItem("mail_armor", "Mail Armor", 3, 178), armorItem("scale_armor", "Scale Armor", 4, 179), armorItem("plate_armor", "Plate Armor", 5, 180)]
    public static let wands = [wand("wand_magic_missile", "Wand of Magic Missile", 208), wand("wand_fireblast", "Wand of Fireblast", 209), wand("wand_frost", "Wand of Frost", 210), wand("wand_lightning", "Wand of Lightning", 211), wand("wand_disintegration", "Wand of Disintegration", 212), wand("wand_prismatic_light", "Wand of Prismatic Light", 213), wand("wand_corrosion", "Wand of Corrosion", 214), wand("wand_living_earth", "Wand of Living Earth", 215), wand("wand_blast_wave", "Wand of Blast Wave", 216), wand("wand_corruption", "Wand of Corruption", 217), wand("wand_warding", "Wand of Warding", 218), wand("wand_regrowth", "Wand of Regrowth", 219), wand("wand_transfusion", "Wand of Transfusion", 220)]
    public static let rings = [ring("ring_accuracy", "Ring of Accuracy", 224), ring("ring_arcana", "Ring of Arcana", 225), ring("ring_elements", "Ring of Elements", 226), ring("ring_energy", "Ring of Energy", 227), ring("ring_evasion", "Ring of Evasion", 228), ring("ring_force", "Ring of Force", 229), ring("ring_furor", "Ring of Furor", 230), ring("ring_haste", "Ring of Haste", 231), ring("ring_might", "Ring of Might", 232), ring("ring_sharpshooting", "Ring of Sharpshooting", 233), ring("ring_tenacity", "Ring of Tenacity", 234), ring("ring_wealth", "Ring of Wealth", 235)]
    public static let all = weapons + armor + wands + rings
    public static let enchantments = ["Blazing", "Blocking", "Blooming", "Chilling", "Corrupting", "Elastic", "Grim", "Kinetic", "Lucky", "Projecting", "Shocking", "Unstable", "Vampiric"]
    public static let weaponCurses = ["Annoying", "Dazzling", "Displacing", "Explosive", "Friendly", "Polarized", "Sacrificial", "Wayward"]
    public static let glyphs = ["Affection", "Anti-Magic", "Brimstone", "Camouflage", "Entanglement", "Flow", "Obfuscation", "Potential", "Repulsion", "Stone", "Swiftness", "Thorns", "Viscosity"]
    public static let armorCurses = ["Anti-Entropy", "Bulk", "Corrosion", "Displacement", "Metabolism", "Multiplicity", "Overgrowth", "Stench"]
    public static func forKind(_ kind: ItemKind) -> [CatalogItem] { switch kind { case .weapon: weapons; case .armor: armor; case .wand: wands; case .ring: rings } }
    /// The items a requirement may name, for the item picker.
    public static func requestableForKind(_ kind: ItemKind) -> [CatalogItem] { forKind(kind).filter(\.requestable) }
    public static func findById(_ id: String) -> CatalogItem? { all.first { $0.id == id } }
    public static func modifiersFor(_ kind: ItemKind) -> [String] { switch kind { case .weapon: enchantments + weaponCurses; case .armor: glyphs + armorCurses; default: [] } }
    public static func cursesFor(_ kind: ItemKind) -> [String] { switch kind { case .weapon: weaponCurses; case .armor: armorCurses; default: [] } }
}
