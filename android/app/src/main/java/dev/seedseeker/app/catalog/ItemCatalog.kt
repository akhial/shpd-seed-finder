// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.catalog

import dev.seedseeker.app.model.CatalogItem
import dev.seedseeker.app.model.ItemKind
import dev.seedseeker.app.model.WeaponClass
import org.json.JSONObject
import java.io.InputStream

/**
 * Searchable, naturally generated equipment, read from the catalog asset this
 * app ships beside the sprite atlas it indexes.
 *
 * The asset is the same third-party extraction every other frontend loads —
 * ids, display names, tiers, `ItemSpriteSheet` sprite constants and the four
 * modifier lists — so a hand-maintained Kotlin copy could only drift from it.
 * Mages Staff, Pickaxe, Spirit Bow, and hero/class armor are absent because
 * they have no generated-world pool.
 */
object ItemCatalog {
    private const val ASSET_PATH = "third_party/shattered-pixel-dungeon/catalog-v4.0.0.json"

    /** Opens a packaged asset by its assets-relative path. */
    fun interface Assets {
        fun open(path: String): InputStream
    }

    @Volatile
    private var assets: Assets = Assets { _ ->
        error("ItemCatalog.install was not called; SeedSeekerApplication does this at process start")
    }

    /**
     * Points the catalog at the packaged asset. [dev.seedseeker.app.SeedSeekerApplication]
     * calls this once at process start, before any entry point can look an
     * item up; the catalog is parsed lazily on first use after that.
     */
    fun install(assets: Assets) {
        this.assets = assets
    }

    private val loaded: Loaded by lazy { load() }

    val meleeWeapons: List<CatalogItem> get() = loaded.meleeWeapons
    val thrownWeapons: List<CatalogItem> get() = loaded.thrownWeapons
    val weapons: List<CatalogItem> get() = loaded.weapons
    val armor: List<CatalogItem> get() = loaded.armor
    val wands: List<CatalogItem> get() = loaded.wands
    val rings: List<CatalogItem> get() = loaded.rings
    val trinkets: List<CatalogItem> get() = loaded.trinkets
    val artifacts: List<CatalogItem> get() = loaded.artifacts
    val all: List<CatalogItem> get() = loaded.all

    val enchantments: List<String> get() = loaded.enchantments
    val weaponCurses: List<String> get() = loaded.weaponCurses
    val glyphs: List<String> get() = loaded.glyphs
    val armorCurses: List<String> get() = loaded.armorCurses

    fun forKind(kind: ItemKind): List<CatalogItem> = when (kind) {
        ItemKind.WEAPON -> weapons
        ItemKind.MELEE_WEAPON -> meleeWeapons
        ItemKind.THROWN_WEAPON -> thrownWeapons
        ItemKind.ARMOR -> armor
        ItemKind.WAND -> wands
        ItemKind.RING -> rings
        ItemKind.TRINKET -> trinkets
        ItemKind.ARTIFACT -> artifacts
    }

    fun findById(id: String): CatalogItem? = loaded.byId[id]

    fun modifiersFor(kind: ItemKind): List<String> = when (kind.family) {
        ItemKind.WEAPON -> enchantments + weaponCurses
        ItemKind.ARMOR -> glyphs + armorCurses
        else -> emptyList()
    }

    /** The non-curse effects of [kind]'s family, in catalog order; what "any enchantment" stands for. */
    fun enchantmentsFor(kind: ItemKind): List<String> = when (kind.family) {
        ItemKind.WEAPON -> enchantments
        ItemKind.ARMOR -> glyphs
        else -> emptyList()
    }

    fun cursesFor(kind: ItemKind): List<String> = when (kind.family) {
        ItemKind.WEAPON -> weaponCurses
        ItemKind.ARMOR -> armorCurses
        else -> emptyList()
    }

    private class Loaded(entries: List<CatalogItem>, modifiers: JSONObject) {
        val meleeWeapons = entries.filter { it.weaponClass == WeaponClass.MELEE }
        val thrownWeapons = entries.filter { it.weaponClass == WeaponClass.THROWN }
        val weapons = meleeWeapons + thrownWeapons
        val armor = entries.filter { it.kind == ItemKind.ARMOR }
        val wands = entries.filter { it.kind == ItemKind.WAND }
        val rings = entries.filter { it.kind == ItemKind.RING }
        val trinkets = entries.filter { it.kind == ItemKind.TRINKET }
        val artifacts = entries.filter { it.kind == ItemKind.ARTIFACT }
        val all = weapons + armor + wands + rings + trinkets + artifacts
        val byId = all.associateBy(CatalogItem::id)

        val enchantments = names(modifiers, "weaponEnchantments")
        val weaponCurses = names(modifiers, "weaponCurses")
        val glyphs = names(modifiers, "armorGlyphs")
        val armorCurses = names(modifiers, "armorCurses")

        private companion object {
            fun names(modifiers: JSONObject, key: String): List<String> {
                val list = modifiers.getJSONArray(key)
                return List(list.length()) { list.getString(it) }
            }
        }
    }

    private fun load(): Loaded {
        val text = assets.open(ASSET_PATH).use { it.readBytes().toString(Charsets.UTF_8) }
        val document = JSONObject(text)
        val entries = document.getJSONArray("entries")
        return Loaded(
            entries = List(entries.length()) { itemFor(entries.getJSONObject(it)) },
            modifiers = document.getJSONObject("modifiers"),
        )
    }

    private fun itemFor(entry: JSONObject): CatalogItem {
        val id = entry.getString("id")
        val kind = when (val type = entry.getString("type")) {
            "weapon" -> ItemKind.WEAPON
            "armor" -> ItemKind.ARMOR
            "wand" -> ItemKind.WAND
            "ring" -> ItemKind.RING
            "trinket" -> ItemKind.TRINKET
            "artifact" -> ItemKind.ARTIFACT
            else -> error("Unknown catalog item type '$type' for '$id'")
        }
        return CatalogItem(
            id = id,
            name = entry.getString("name"),
            kind = kind,
            spriteIndex = entry.getInt("sprite"),
            tier = if (entry.has("tier")) entry.getInt("tier") else null,
            typeIconIndex = if (entry.has("typeIcon")) entry.getInt("typeIcon") else null,
            weaponClass = when (val weaponClass = entry.optString("class")) {
                "" -> null
                "melee" -> WeaponClass.MELEE
                "thrown" -> WeaponClass.THROWN
                else -> error("Unknown weapon class '$weaponClass' for '$id'")
            },
        )
    }
}
