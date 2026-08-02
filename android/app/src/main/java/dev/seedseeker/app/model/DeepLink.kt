// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.model

import dev.seedseeker.app.catalog.ItemCatalog

/**
 * Compact shareable-link codec for search queries.
 *
 * A deep link carries a whole query as a short base64url code, e.g.
 * `https://shpd-seed-seeker.web.app/#q=EAGWhMA`. The canonical implementation
 * lives in the Rust core (`crates/seedfinder-core/src/deep_link.rs`); this
 * port must stay bit-for-bit compatible with it. The payload is a versioned
 * bit stream, so codes shared today must keep decoding in every future
 * release: the numeric code tables below are frozen by tests and may only
 * ever grow at the end.
 */
object DeepLink {
    /** Canonical prefix for shared links; the code follows the `#q=` fragment. */
    const val WEB_LINK_PREFIX = "https://shpd-seed-seeker.web.app/#q="

    /** Custom URI scheme registered by the desktop apps. */
    private const val URI_SCHEME_PREFIX = "seedseeker://"

    private const val VERSION = 1

    /** Requirement-count field width; far above anything the UI produces. */
    private const val MAX_REQUIREMENTS = 63

    private const val BASE64URL =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"

    /** Item sources in frozen code order (indices are part of the link format). */
    internal val SOURCE_CODES = listOf(
        ScoutItemSource.HEAP,
        ScoutItemSource.CHEST,
        ScoutItemSource.LOCKED_CHEST,
        ScoutItemSource.CRYSTAL_CHEST,
        ScoutItemSource.TOMB,
        ScoutItemSource.SKELETON,
        ScoutItemSource.SACRIFICIAL_FIRE,
        ScoutItemSource.MIMIC,
        ScoutItemSource.GOLDEN_MIMIC,
        ScoutItemSource.CRYSTAL_MIMIC,
        ScoutItemSource.STATUE,
        ScoutItemSource.ARMORED_STATUE,
        ScoutItemSource.SHOP,
        ScoutItemSource.GHOST_REWARD,
        ScoutItemSource.WANDMAKER_REWARD,
        ScoutItemSource.BLACKSMITH_REWARD,
        ScoutItemSource.IMP_REWARD,
    )

    /** Weapon effects in frozen code order (indices are part of the link format). */
    internal val WEAPON_EFFECT_CODES = listOf(
        "Blazing",
        "Chilling",
        "Kinetic",
        "Shocking",
        "Blocking",
        "Blooming",
        "Elastic",
        "Lucky",
        "Projecting",
        "Unstable",
        "Corrupting",
        "Grim",
        "Vampiric",
        "Annoying",
        "Displacing",
        "Dazzling",
        "Explosive",
        "Sacrificial",
        "Wayward",
        "Polarized",
        "Friendly",
    )

    /** Armor effects in frozen code order (indices are part of the link format). */
    internal val ARMOR_EFFECT_CODES = listOf(
        "Obfuscation",
        "Swiftness",
        "Viscosity",
        "Potential",
        "Brimstone",
        "Stone",
        "Entanglement",
        "Repulsion",
        "Camouflage",
        "Flow",
        "Affection",
        "Anti-Magic",
        "Thorns",
        "Anti-Entropy",
        "Corrosion",
        "Displacement",
        "Metabolism",
        "Multiplicity",
        "Stench",
        "Overgrowth",
        "Bulk",
    )

    /**
     * Encodes a query as a bare share code (no URL prefix). Requirements are
     * valid by construction; only whole-query shape can fail here.
     *
     * @throws IllegalArgumentException with a user-facing message.
     */
    fun encode(query: PresetQuery): String {
        require(query.requirements.isNotEmpty()) { "Add at least one requirement to share a search." }
        require(query.requirements.size <= MAX_REQUIREMENTS) {
            "A share link can carry at most $MAX_REQUIREMENTS requirements."
        }
        require(query.maximumDepth in 1..24) { "Maximum floor must be 1..24." }
        require(query.challenges in 0..Challenge.ALL_MASK) {
            "Challenge mask must be 0..${Challenge.ALL_MASK}."
        }
        val bits = BitWriter()
        bits.push(VERSION, 4)
        bits.push(if (query.requireBlacksmith) 1 else 0, 1)
        bits.push(if (query.excludeBlacksmithRewards) 1 else 0, 1)
        bits.push(if (query.fastMode) 1 else 0, 1)
        bits.pushFlagged(query.maximumDepth.takeIf { it != 24 }?.minus(1), 5)
        bits.pushFlagged(query.challenges.takeIf { it != 0 }, 9)
        bits.pushFlagged(query.wandmakerQuest?.ordinal, 2)
        bits.push(query.requirements.size, 6)
        query.requirements.forEach { encodeRequirement(bits, it) }
        return base64urlEncode(bits.finish())
    }

    /** Encodes a query as a full shareable web link. */
    fun encodeLink(query: PresetQuery): String = WEB_LINK_PREFIX + encode(query)

    /**
     * Decodes a bare share code produced by [encode] (any version).
     *
     * @throws IllegalArgumentException with a user-facing message for
     * malformed, truncated, or unsupported codes and for payloads that fail
     * the model types' own validation.
     */
    fun decode(code: String): PresetQuery {
        val bits = BitReader(base64urlDecode(code.trim()))
        val version = bits.pull(4)
        require(version == VERSION) {
            "This link uses share format version $version; this app only understands " +
                "version $VERSION — it may have been created by a newer Seed Seeker release."
        }
        val requireBlacksmith = bits.pullFlag()
        val excludeBlacksmithRewards = bits.pullFlag()
        val fastMode = bits.pullFlag()
        val maximumDepth = if (bits.pullFlag()) depthFrom(bits.pull(5)) else 24
        val challenges = if (bits.pullFlag()) bits.pull(9) else 0
        val wandmakerQuest = if (bits.pullFlag()) wandmakerQuestFrom(bits.pull(2)) else null
        val count = bits.pull(6)
        val requirements = List(count) { index ->
            runCatching { decodeRequirement(bits, index) }.getOrElse { failure ->
                throw IllegalArgumentException("Requirement ${index + 1}: ${failure.message}")
            }
        }
        bits.expectExhausted()
        require(requirements.isNotEmpty()) { "This share link contains no requirements." }
        return PresetQuery(
            requirements = requirements,
            maximumDepth = maximumDepth,
            requireBlacksmith = requireBlacksmith,
            excludeBlacksmithRewards = excludeBlacksmithRewards,
            wandmakerQuest = wandmakerQuest,
            fastMode = fastMode,
            challenges = challenges,
        )
    }

    /**
     * Pulls the share code out of user-facing link text.
     *
     * Accepts full web links (`…#q=CODE` or `…?q=CODE`), custom-scheme links
     * (`seedseeker://q/CODE`), and bare codes. Returns null for text without
     * any plausible code.
     */
    fun extractCode(text: String): String? {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return null
        // A `q=` parameter introduced by `#`, `?`, or `&` wins wherever it
        // appears, so links keep working if the host page ever gains other
        // parameters.
        var search = trimmed
        while (true) {
            val position = search.indexOf("q=")
            if (position < 0) break
            val preceded = position > 0 && search[position - 1] in "#?&"
            val value = search.substring(position + 2)
            if (preceded) {
                val end = value.indexOfFirst { it == '&' || it == '#' }
                return if (end < 0) value else value.substring(0, end)
            }
            search = value
        }
        if (trimmed.startsWith(URI_SCHEME_PREFIX)) {
            return trimmed.removePrefix(URI_SCHEME_PREFIX)
                .substringAfterLast('/')
                .takeIf { it.isNotEmpty() }
        }
        if ("://" in trimmed || '/' in trimmed) return null
        return trimmed
    }

    /**
     * Decodes any accepted link form — see [extractCode] and [decode].
     *
     * @throws IllegalArgumentException when no code is present or the code
     * fails to decode.
     */
    fun decodeText(text: String): PresetQuery {
        val code = requireNotNull(extractCode(text)) { "The text contains no share code." }
        return decode(code)
    }

    private fun encodeRequirement(bits: BitWriter, requirement: ItemRequirement) {
        bits.push(kindCode(requirement.kind), 3)
        bits.pushFlagged(requirement.item?.let(::itemCode), 7)
        when (requirement.tierMatch) {
            TierMatch.ANY -> bits.push(0, 2)
            TierMatch.EXACT -> bits.pushFilter(1, requirement.tier)
            TierMatch.AT_LEAST -> bits.pushFilter(2, requirement.tier)
            TierMatch.AT_MOST -> bits.pushFilter(3, requirement.tier)
        }
        when (requirement.upgradeMatch) {
            UpgradeMatch.ANY -> bits.push(0, 2)
            UpgradeMatch.EXACT -> bits.pushFilter(1, requirement.upgrade)
            UpgradeMatch.AT_LEAST -> bits.pushFilter(2, requirement.upgrade)
        }
        bits.pushFlagged(requirement.modifier?.let { effectCode(requirement.kind, it) }, 5)
        bits.push(if (requirement.requireUncursed) 1 else 0, 1)
        bits.pushFlagged(requirement.source?.let(SOURCE_CODES::indexOf), 5)
        bits.pushFlagged(requirement.identityGroup, 8)
        bits.pushFlagged(requirement.maximumDepth?.minus(1), 5)
    }

    private fun decodeRequirement(bits: BitReader, index: Int): ItemRequirement {
        val kind = kindFrom(bits.pull(3))
        val item = if (bits.pullFlag()) itemFrom(bits.pull(7)) else null
        var tier = 0
        var tierMatch = TierMatch.ANY
        when (bits.pull(2)) {
            0 -> {}
            1 -> {
                tierMatch = TierMatch.EXACT
                tier = bits.pull(3)
            }
            2 -> {
                tierMatch = TierMatch.AT_LEAST
                tier = bits.pull(3)
            }
            else -> {
                tierMatch = TierMatch.AT_MOST
                tier = bits.pull(3)
            }
        }
        var upgrade = 0
        var upgradeMatch = UpgradeMatch.ANY
        when (val mode = bits.pull(2)) {
            0 -> {}
            1 -> {
                upgradeMatch = UpgradeMatch.EXACT
                upgrade = bits.pull(3)
            }
            2 -> {
                upgradeMatch = UpgradeMatch.AT_LEAST
                upgrade = bits.pull(3)
            }
            else -> throw IllegalArgumentException("unknown upgrade mode $mode")
        }
        val modifier = if (bits.pullFlag()) effectFrom(kind, bits.pull(5)) else null
        val requireUncursed = bits.pullFlag()
        val source = if (bits.pullFlag()) sourceFrom(bits.pull(5)) else null
        val identityGroup = if (bits.pullFlag()) {
            bits.pull(8).also { require(it != 0) { "identity group 0 is reserved" } }
        } else {
            null
        }
        val maximumDepth = if (bits.pullFlag()) depthFrom(bits.pull(5)) else null
        // The validating constructor enforces everything the bit stream
        // cannot: item/category agreement, tier and upgrade ranges, effect
        // legality, and the app's identity-group range.
        return ItemRequirement(
            key = index + 1L,
            item = item,
            upgrade = upgrade,
            modifier = modifier,
            kind = kind,
            tier = tier,
            tierMatch = tierMatch,
            upgradeMatch = upgradeMatch,
            source = source,
            identityGroup = identityGroup,
            maximumDepth = maximumDepth,
            requireUncursed = requireUncursed,
        )
    }

    private fun kindCode(kind: ItemKind): Int = when (kind) {
        ItemKind.WEAPON -> 0
        ItemKind.MELEE_WEAPON -> 1
        ItemKind.THROWN_WEAPON -> 2
        ItemKind.ARMOR -> 3
        ItemKind.WAND -> 4
        ItemKind.RING -> 5
    }

    private fun kindFrom(code: Int): ItemKind = when (code) {
        0 -> ItemKind.WEAPON
        1 -> ItemKind.MELEE_WEAPON
        2 -> ItemKind.THROWN_WEAPON
        3 -> ItemKind.ARMOR
        4 -> ItemKind.WAND
        5 -> ItemKind.RING
        else -> throw IllegalArgumentException("unknown category code $code")
    }

    private fun itemCode(item: CatalogItem): Int {
        val code = ITEM_CODES.indexOf(item.id)
        require(code >= 0) { "\"${item.id}\" cannot appear in a share link yet." }
        return code
    }

    private fun itemFrom(code: Int): CatalogItem {
        val id = ITEM_CODES.getOrNull(code)
            ?: throw IllegalArgumentException("unknown item code $code")
        return requireNotNull(ItemCatalog.findById(id)) { "unknown item \"$id\"" }
    }

    private fun effectTable(kind: ItemKind): List<String> = when (kind.family) {
        ItemKind.WEAPON -> WEAPON_EFFECT_CODES
        ItemKind.ARMOR -> ARMOR_EFFECT_CODES
        else -> emptyList()
    }

    private fun effectCode(kind: ItemKind, modifier: String): Int {
        val code = effectTable(kind).indexOf(modifier)
        require(code >= 0) { "\"$modifier\" cannot appear in a share link yet." }
        return code
    }

    private fun effectFrom(kind: ItemKind, code: Int): String =
        effectTable(kind).getOrNull(code)
            ?: throw IllegalArgumentException("effect code $code is not valid for this category")

    private fun sourceFrom(code: Int): ScoutItemSource =
        SOURCE_CODES.getOrNull(code)
            ?: throw IllegalArgumentException("unknown source code $code")

    /**
     * Wandmaker variants ride the wire order used everywhere else (corpse
     * dust, elemental embers, rotberry), biased down by one so the three of
     * them fit in two bits.
     */
    private fun wandmakerQuestFrom(code: Int): WandmakerQuest =
        WandmakerQuest.entries.getOrNull(code)
            ?: throw IllegalArgumentException("unknown Wandmaker quest code $code")

    private fun depthFrom(raw: Int): Int {
        val depth = raw + 1
        require(depth in 1..24) { "floor $depth is outside the dungeon" }
        return depth
    }

    private class BitWriter {
        private val bytes = mutableListOf<Byte>()
        private var used = 0

        /** Appends the low `width` bits of `value`, most significant first. */
        fun push(value: Int, width: Int) {
            for (offset in width - 1 downTo 0) {
                if (used % 8 == 0) bytes.add(0)
                val bit = (value ushr offset) and 1
                bytes[bytes.size - 1] = (bytes.last().toInt() or (bit shl (7 - used % 8))).toByte()
                used++
            }
        }

        /** Writes a presence flag, then `value` in `width` bits when present. */
        fun pushFlagged(value: Int?, width: Int) {
            push(if (value != null) 1 else 0, 1)
            if (value != null) push(value, width)
        }

        fun pushFilter(mode: Int, value: Int) {
            push(mode, 2)
            push(value, 3)
        }

        fun finish(): ByteArray = bytes.toByteArray()
    }

    private class BitReader(private val bytes: ByteArray) {
        private var cursor = 0

        /** Reads `width` bits, most significant first. */
        fun pull(width: Int): Int {
            var value = 0
            repeat(width) {
                require(cursor / 8 < bytes.size) { "This share link is incomplete." }
                val bit = (bytes[cursor / 8].toInt() ushr (7 - cursor % 8)) and 1
                value = (value shl 1) or bit
                cursor++
            }
            return value
        }

        fun pullFlag(): Boolean = pull(1) == 1

        /** Requires every remaining bit to be final-byte zero padding. */
        fun expectExhausted() {
            val remaining = bytes.size * 8 - cursor
            require(remaining < 8 && pull(remaining) == 0) { "This share link has trailing data." }
        }
    }

    private fun base64urlEncode(bytes: ByteArray): String = buildString((bytes.size + 2) / 3 * 4) {
        var index = 0
        while (index < bytes.size) {
            val chunk = minOf(3, bytes.size - index)
            var word = 0
            for (offset in 0 until chunk) {
                word = word or ((bytes[index + offset].toInt() and 0xff) shl (16 - 8 * offset))
            }
            for (position in 0..chunk) {
                append(BASE64URL[(word ushr (18 - 6 * position)) and 0x3f])
            }
            index += chunk
        }
    }

    private fun base64urlDecode(text: String): ByteArray {
        val digits = IntArray(text.length) { position ->
            val digit = BASE64URL.indexOf(text[position])
            require(digit >= 0) { "This link contains characters that are not part of a share code." }
            digit
        }
        require(digits.size % 4 != 1) { "This share link is incomplete." }
        val bytes = ArrayList<Byte>(digits.size * 3 / 4)
        var index = 0
        while (index < digits.size) {
            val chunk = minOf(4, digits.size - index)
            var word = 0
            for (offset in 0 until chunk) {
                word = word or (digits[index + offset] shl (18 - 6 * offset))
            }
            for (position in 0 until chunk - 1) {
                bytes.add(((word ushr (16 - 8 * position)) and 0xff).toByte())
            }
            index += chunk
        }
        return bytes.toByteArray()
    }

    /** Every item in frozen code order (indices are part of the link format). */
    internal val ITEM_CODES = listOf(
        "worn_shortsword",
        "cudgel",
        "gloves",
        "rapier",
        "dagger",
        "shortsword",
        "hand_axe",
        "spear",
        "quarterstaff",
        "dirk",
        "sickle",
        "sword",
        "mace",
        "scimitar",
        "round_shield",
        "sai",
        "whip",
        "longsword",
        "battle_axe",
        "flail",
        "runic_blade",
        "assassins_blade",
        "crossbow",
        "katana",
        "greatsword",
        "war_hammer",
        "glaive",
        "greataxe",
        "greatshield",
        "gauntlet",
        "war_scythe",
        "throwing_stone",
        "throwing_knife",
        "throwing_spike",
        "fishing_spear",
        "throwing_club",
        "shuriken",
        "throwing_spear",
        "kunai",
        "bolas",
        "javelin",
        "tomahawk",
        "heavy_boomerang",
        "trident",
        "throwing_hammer",
        "force_cube",
        "cloth_armor",
        "leather_armor",
        "mail_armor",
        "scale_armor",
        "plate_armor",
        "wand_magic_missile",
        "wand_fireblast",
        "wand_frost",
        "wand_lightning",
        "wand_disintegration",
        "wand_prismatic_light",
        "wand_corrosion",
        "wand_living_earth",
        "wand_blast_wave",
        "wand_corruption",
        "wand_warding",
        "wand_regrowth",
        "wand_transfusion",
        "rot_dart",
        "incendiary_dart",
        "adrenaline_dart",
        "healing_dart",
        "chilling_dart",
        "shocking_dart",
        "poison_dart",
        "cleansing_dart",
        "paralytic_dart",
        "holy_dart",
        "displacing_dart",
        "blinding_dart",
        "ring_accuracy",
        "ring_arcana",
        "ring_elements",
        "ring_energy",
        "ring_evasion",
        "ring_force",
        "ring_furor",
        "ring_haste",
        "ring_might",
        "ring_sharpshooting",
        "ring_tenacity",
        "ring_wealth",
    )
}
