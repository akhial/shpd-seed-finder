// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.catalog

import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.ItemKind

/**
 * Searchable, naturally generated equipment in Shattered Pixel Dungeon v3.3.8.
 *
 * Sprite values are ItemSpriteSheet constants, never Generator enum ordinals. Mages Staff,
 * Pickaxe, Spirit Bow, and hero/class armor are absent because they have no generated-world pool.
 */
object ItemCatalog {
    val weapons = listOf(
        weapon("worn_shortsword", "Worn Shortsword", 1, 96),
        weapon("cudgel", "Cudgel", 1, 97),
        weapon("gloves", "Studded Gloves", 1, 98),
        weapon("rapier", "Rapier", 1, 99),
        weapon("dagger", "Dagger", 1, 100),
        weapon("shortsword", "Shortsword", 2, 104),
        weapon("hand_axe", "Hand Axe", 2, 105),
        weapon("spear", "Spear", 2, 106),
        weapon("quarterstaff", "Quarterstaff", 2, 107),
        weapon("dirk", "Dirk", 2, 108),
        weapon("sickle", "Sickle", 2, 109),
        weapon("sword", "Sword", 3, 112),
        weapon("mace", "Mace", 3, 113),
        weapon("scimitar", "Scimitar", 3, 114),
        weapon("round_shield", "Round Shield", 3, 115),
        weapon("sai", "Sai", 3, 116),
        weapon("whip", "Whip", 3, 117),
        weapon("longsword", "Longsword", 4, 120),
        weapon("battle_axe", "Battle Axe", 4, 121),
        weapon("flail", "Flail", 4, 122),
        weapon("runic_blade", "Runic Blade", 4, 123),
        weapon("assassins_blade", "Assassin's Blade", 4, 124),
        weapon("crossbow", "Crossbow", 4, 125),
        weapon("katana", "Katana", 4, 126),
        weapon("greatsword", "Greatsword", 5, 128),
        weapon("war_hammer", "War Hammer", 5, 129),
        weapon("glaive", "Glaive", 5, 130),
        weapon("greataxe", "Greataxe", 5, 131),
        weapon("greatshield", "Greatshield", 5, 132),
        weapon("gauntlet", "Stone Gauntlet", 5, 133),
        weapon("war_scythe", "War Scythe", 5, 134),
        weapon("throwing_stone", "Throwing Stone", 1, 147),
        weapon("throwing_knife", "Throwing Knife", 1, 146),
        weapon("throwing_spike", "Throwing Spike", 1, 145),
        weapon("fishing_spear", "Fishing Spear", 2, 148),
        weapon("throwing_club", "Throwing Club", 2, 150),
        weapon("shuriken", "Shuriken", 2, 149),
        weapon("throwing_spear", "Throwing Spear", 3, 151),
        weapon("kunai", "Kunai", 3, 153),
        weapon("bolas", "Bolas", 3, 152),
        weapon("javelin", "Javelin", 4, 154),
        weapon("tomahawk", "Tomahawk", 4, 155),
        weapon("heavy_boomerang", "Heavy Boomerang", 4, 156),
        weapon("trident", "Trident", 5, 157),
        weapon("throwing_hammer", "Throwing Hammer", 5, 158),
        weapon("force_cube", "Force Cube", 5, 159),
        weapon("rot_dart", "Rot Dart", 2, 161),
        weapon("incendiary_dart", "Incendiary Dart", 2, 162),
        weapon("adrenaline_dart", "Adrenaline Dart", 2, 163),
        weapon("healing_dart", "Healing Dart", 2, 164),
        weapon("chilling_dart", "Chilling Dart", 2, 165),
        weapon("shocking_dart", "Shocking Dart", 2, 166),
        weapon("poison_dart", "Poison Dart", 2, 167),
        weapon("cleansing_dart", "Cleansing Dart", 2, 168),
        weapon("paralytic_dart", "Paralytic Dart", 2, 169),
        weapon("holy_dart", "Holy Dart", 2, 170),
        weapon("displacing_dart", "Displacing Dart", 2, 171),
        weapon("blinding_dart", "Blinding Dart", 2, 172),
    )

    val armor = listOf(
        armor("cloth_armor", "Cloth Armor", 1, 176),
        armor("leather_armor", "Leather Armor", 2, 177),
        armor("mail_armor", "Mail Armor", 3, 178),
        armor("scale_armor", "Scale Armor", 4, 179),
        armor("plate_armor", "Plate Armor", 5, 180),
    )

    val wands = listOf(
        wand("wand_magic_missile", "Wand of Magic Missile", 208),
        wand("wand_fireblast", "Wand of Fireblast", 209),
        wand("wand_frost", "Wand of Frost", 210),
        wand("wand_lightning", "Wand of Lightning", 211),
        wand("wand_disintegration", "Wand of Disintegration", 212),
        wand("wand_prismatic_light", "Wand of Prismatic Light", 213),
        wand("wand_corrosion", "Wand of Corrosion", 214),
        wand("wand_living_earth", "Wand of Living Earth", 215),
        wand("wand_blast_wave", "Wand of Blast Wave", 216),
        wand("wand_corruption", "Wand of Corruption", 217),
        wand("wand_warding", "Wand of Warding", 218),
        wand("wand_regrowth", "Wand of Regrowth", 219),
        wand("wand_transfusion", "Wand of Transfusion", 220),
    )

    val rings = listOf(
        ring("ring_accuracy", "Ring of Accuracy", 224, 0),
        ring("ring_arcana", "Ring of Arcana", 225, 1),
        ring("ring_elements", "Ring of Elements", 226, 2),
        ring("ring_energy", "Ring of Energy", 227, 3),
        ring("ring_evasion", "Ring of Evasion", 228, 4),
        ring("ring_force", "Ring of Force", 229, 5),
        ring("ring_furor", "Ring of Furor", 230, 6),
        ring("ring_haste", "Ring of Haste", 231, 7),
        ring("ring_might", "Ring of Might", 232, 8),
        ring("ring_sharpshooting", "Ring of Sharpshooting", 233, 9),
        ring("ring_tenacity", "Ring of Tenacity", 234, 10),
        ring("ring_wealth", "Ring of Wealth", 235, 11),
    )

    val all = weapons + armor + wands + rings
    private val byId = all.associateBy(CatalogItem::id)

    val enchantments = listOf(
        "Blazing",
        "Blocking",
        "Blooming",
        "Chilling",
        "Corrupting",
        "Elastic",
        "Grim",
        "Kinetic",
        "Lucky",
        "Projecting",
        "Shocking",
        "Unstable",
        "Vampiric",
    )

    val weaponCurses = listOf(
        "Annoying",
        "Dazzling",
        "Displacing",
        "Explosive",
        "Friendly",
        "Polarized",
        "Sacrificial",
        "Wayward",
    )

    val glyphs = listOf(
        "Affection",
        "Anti-Magic",
        "Brimstone",
        "Camouflage",
        "Entanglement",
        "Flow",
        "Obfuscation",
        "Potential",
        "Repulsion",
        "Stone",
        "Swiftness",
        "Thorns",
        "Viscosity",
    )

    val armorCurses = listOf(
        "Anti-Entropy",
        "Bulk",
        "Corrosion",
        "Displacement",
        "Metabolism",
        "Multiplicity",
        "Overgrowth",
        "Stench",
    )

    fun forKind(kind: ItemKind): List<CatalogItem> = when (kind) {
        ItemKind.WEAPON -> weapons
        ItemKind.ARMOR -> armor
        ItemKind.WAND -> wands
        ItemKind.RING -> rings
    }

    fun findById(id: String): CatalogItem? = byId[id]

    fun modifiersFor(kind: ItemKind): List<String> = when (kind) {
        ItemKind.WEAPON -> enchantments + weaponCurses
        ItemKind.ARMOR -> glyphs + armorCurses
        ItemKind.WAND, ItemKind.RING -> emptyList()
    }

    fun cursesFor(kind: ItemKind): List<String> = when (kind) {
        ItemKind.WEAPON -> weaponCurses
        ItemKind.ARMOR -> armorCurses
        ItemKind.WAND, ItemKind.RING -> emptyList()
    }

    private fun weapon(id: String, name: String, tier: Int, sprite: Int) =
        CatalogItem(id, name, ItemKind.WEAPON, sprite, tier)

    private fun armor(id: String, name: String, tier: Int, sprite: Int) =
        CatalogItem(id, name, ItemKind.ARMOR, sprite, tier)

    private fun wand(id: String, name: String, sprite: Int) =
        CatalogItem(id, name, ItemKind.WAND, sprite)

    private fun ring(id: String, name: String, sprite: Int, typeIcon: Int) =
        CatalogItem(id, name, ItemKind.RING, sprite, typeIconIndex = typeIcon)
}
