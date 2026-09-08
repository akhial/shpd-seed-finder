/*
 * Shattered Pixel Dungeon parity oracle
 * Copyright (C) 2026
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 */

package com.shatteredpixel.shatteredpixeldungeon;

import com.badlogic.gdx.Preferences;
import com.shatteredpixel.shatteredpixeldungeon.actors.hero.HeroClass;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.ArmoredStatue;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.Mimic;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.Mob;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.Statue;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Blacksmith;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Ghost;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Imp;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Wandmaker;
import com.shatteredpixel.shatteredpixeldungeon.actors.blobs.SacrificialFire;
import com.shatteredpixel.shatteredpixeldungeon.items.Generator;
import com.shatteredpixel.shatteredpixeldungeon.items.Heap;
import com.shatteredpixel.shatteredpixeldungeon.items.Item;
import com.shatteredpixel.shatteredpixeldungeon.items.armor.Armor;
import com.shatteredpixel.shatteredpixeldungeon.items.artifacts.Artifact;
import com.shatteredpixel.shatteredpixeldungeon.items.artifacts.TimekeepersHourglass;
import com.shatteredpixel.shatteredpixeldungeon.items.potions.Potion;
import com.shatteredpixel.shatteredpixeldungeon.items.quest.CeremonialCandle;
import com.shatteredpixel.shatteredpixeldungeon.items.rings.Ring;
import com.shatteredpixel.shatteredpixeldungeon.items.scrolls.Scroll;
import com.shatteredpixel.shatteredpixeldungeon.items.wands.Wand;
import com.shatteredpixel.shatteredpixeldungeon.items.weapon.Weapon;
import com.shatteredpixel.shatteredpixeldungeon.levels.Level;
import com.shatteredpixel.shatteredpixeldungeon.levels.RegularLevel;
import com.shatteredpixel.shatteredpixeldungeon.levels.CityBossLevel;
import com.shatteredpixel.shatteredpixeldungeon.levels.VaultLevel;
import com.shatteredpixel.shatteredpixeldungeon.levels.rooms.Room;
import com.shatteredpixel.shatteredpixeldungeon.levels.rooms.secret.SecretRoom;
import com.shatteredpixel.shatteredpixeldungeon.levels.rooms.special.SpecialRoom;
import com.shatteredpixel.shatteredpixeldungeon.levels.rooms.standard.ImpShopRoom;
import com.shatteredpixel.shatteredpixeldungeon.utils.DungeonSeed;
import com.watabou.noosa.Game;
import com.watabou.utils.GameSettings;

import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;

/**
 * A non-interactive, machine-readable oracle over the official v4.0.0-BETA-4
 * generation path, driven headlessly against the unmodified desktop JAR.
 * This class deliberately avoids identifying or otherwise mutating generated
 * items while recording them.
 */
public final class ParityOracle {

	private static final String SCHEMA = "shpd-parity-oracle/v2";
	private static final String GAME_VERSION = "4.0.0-BETA-4";
	private static final int GAME_VERSION_CODE = 906;
	private static final String GAME_JAR_URL =
			"https://github.com/00-Evan/shattered-pixel-dungeon/releases/download/4.0.0-beta/ShatteredPD-v4.0.0-BETA-4-Java.jar";
	private static final String GAME_JAR_SHA256 =
			"76f6983e7b619267666621de9f1ecbbc3645d4925c2c446736987c3011b9dfd1";
	/**
	 * The JAR's own {@code DeviceCompat.isDebug()} is {@code Game.version.contains("INDEV")}.
	 * Debug mode marks every journal page as read, which is what keeps the unseeded
	 * early-Guidebook heap off the first floors.  Nothing else on the generation path
	 * reads {@code Game.version} (see README), so the suffix is the smallest possible
	 * headless intervention.
	 */
	private static final String EFFECTIVE_GAME_VERSION = GAME_VERSION + "-INDEV";

	private static final int MAX_DEPTH = 26;
	private static final int LAST_IMP_DEPTH = 19;

	private static Set<Integer> requestedDepths = Collections.emptySet();
	private static Output output;

	private ParityOracle() {
	}

	public static void main(String[] args) {
		try {
			Options options = Options.parse(args);
			if (options.help) {
				printUsage();
				return;
			}
			run(options);
		} catch (IllegalArgumentException error) {
			System.err.println("parity-oracle: " + error.getMessage());
			System.err.println("Try --help for usage.");
			System.exit(2);
		} catch (Throwable error) {
			error.printStackTrace(System.err);
			System.exit(1);
		}
	}

	private static void run(Options options) throws Exception {
		requestedDepths = options.depths;
		output = new Output(options.format);

		Game.version = EFFECTIVE_GAME_VERSION;
		Game.versionCode = GAME_VERSION_CODE;
		GameSettings.set(new MemoryPreferences());
		// Match a clean installation without asking libGDX's absent headless file
		// service for badges/remains files, and skip the tutorial intro flag.
		SPDSettings.intro(false);
		setStaticField(Badges.class, "global", new HashSet<Badges.Badge>());
		setStaticField(Bones.class, "depth", 0);
		setStaticField(Bones.class, "branch", -1);
		setStaticField(Bones.class, "item", null);
		setStaticField(Bones.class, "heroClass", null);

		Dungeon.daily = false;
		Dungeon.dailyReplay = false;
		SPDSettings.customSeed(options.seedCode);
		SPDSettings.challenges(options.challenges);
		Dungeon.initSeed();
		GamesInProgress.selectedClass = HeroClass.WARRIOR;
		Dungeon.init();
		// INDEV leaves the final Halls lore page missing. Match the engine's
		// canonical all-pages-read profile explicitly, including that page.
		for (com.shatteredpixel.shatteredpixeldungeon.journal.Document document
				: com.shatteredpixel.shatteredpixeldungeon.journal.Document.values()) {
			@SuppressWarnings("unchecked")
			Map<String, Integer> states = (Map<String, Integer>) getField(document, "pagesStates");
			states.replaceAll((page, state) -> 2);
		}

		output.emit(runInitRecord(options));

		QuestTracker quests = new QuestTracker();
		int lastDepth = options.depths.isEmpty() ? 0 : ((TreeSet<Integer>) options.depths).last();
		// The Imp is guaranteed to have spawned by depth 19; --vault keeps generating
		// (unselected) floors until its floor has been observed.
		int stopDepth = options.vault ? Math.max(lastDepth, LAST_IMP_DEPTH) : lastDepth;
		String selectedTrinket = System.getProperty("seedfinder.trinket");
		boolean brewedTrinket = false;
		boolean alchemyAvailable = false;
		for (int depth = 1; depth <= stopDepth; depth++) {
			PersistentSnapshot beforeBoss = options.bossSkipCheckpoints && isBossDepth(depth)
					? persistentSnapshot() : null;
			Level level = Dungeon.newLevel();
			boolean selected = requestedDepths.contains(depth);
			List<Map<String, Object>> questItems = quests.observe(depth, selected);
			if (selected) {
				output.emit(levelRecord(level));
				List<Map<String, Object>> items = itemRecords(level, null);
				items.addAll(questItems);
				items.removeIf(item -> !Boolean.TRUE.equals(item.get("searchable")));
				sortItemRecords(items);
				for (Map<String, Object> item : items) output.emit(item);
			}
			if (options.runCheckpoints) output.emit(generatorCheckpoint(level, selected));
			if (beforeBoss != null) {
				output.emit(bossTransition(level, beforeBoss, persistentSnapshot()));
			}
			for (int tile : level.map) {
				if (tile == com.shatteredpixel.shatteredpixeldungeon.levels.Terrain.ALCHEMY) alchemyAvailable = true;
			}
			if (selectedTrinket != null && !brewedTrinket && alchemyAvailable
					&& Dungeon.LimitedDrops.TRINKET_CATA.count > 0) {
				Item trinket = (Item) Class.forName("com.shatteredpixel.shatteredpixeldungeon.items.trinkets." + selectedTrinket)
					.getDeclaredConstructor().newInstance();
				trinket.level(3);
				Dungeon.hero.belongings.backpack.items.add(trinket);
				brewedTrinket = true;
			}
			Dungeon.depth++;
			if (options.vault && depth >= lastDepth && quests.impDepth > 0) break;
		}

		if (options.vault) generateVault(options, quests.impDepth);

		output.finish();
	}

	/**
	 * Builds the Imp's Vault (branch 1 of the Imp's floor) after the main run.  The
	 * branch is seeded independently by {@code Dungeon.seedForDepth(depth, 1)}; the
	 * emitted {@code vault_transition} record proves that every run-persistent
	 * component is unchanged afterwards.
	 */
	private static void generateVault(Options options, int impDepth) throws Exception {
		if (impDepth <= 0) {
			throw new IllegalStateException("the Imp did not spawn by depth " + LAST_IMP_DEPTH);
		}
		int savedDepth = Dungeon.depth;
		int savedBranch = Dungeon.branch;
		PersistentSnapshot before = persistentSnapshot();

		Dungeon.depth = impDepth;
		Dungeon.branch = 1;
		Level vault = Dungeon.newLevel();
		if (!(vault instanceof VaultLevel)) {
			throw new IllegalStateException("expected a VaultLevel at depth " + impDepth
					+ " branch 1, got " + className(vault));
		}

		output.emit(levelRecord(vault));
		List<Map<String, Object>> items = itemRecords(vault, "vault_heap");
		sortItemRecords(items);
		for (Map<String, Object> item : items) output.emit(item);
		if (options.runCheckpoints) output.emit(generatorCheckpoint(vault, true));

		Dungeon.depth = savedDepth;
		Dungeon.branch = savedBranch;
		PersistentSnapshot after = persistentSnapshot();

		Map<String, Object> record = record("vault_transition");
		record.put("depth", impDepth);
		record.put("branch", 1);
		record.put("level_class", className(vault));
		record.put("restored_depth", Dungeon.depth);
		record.put("restored_branch", Dungeon.branch);
		putTransition(record, before, after);
		output.emit(record);
	}

	private static Map<String, Object> runInitRecord(Options options) throws Exception {
		Map<String, Object> record = record("run_init");
		record.put("game_version", GAME_VERSION);
		record.put("game_version_code", GAME_VERSION_CODE);
		record.put("game_jar_url", GAME_JAR_URL);
		record.put("game_jar_sha256", GAME_JAR_SHA256);
		record.put("effective_game_version", Game.version);
		record.put("oracle_mode", "official-jar");
		Map<String, Object> runtime = new LinkedHashMap<String, Object>();
		runtime.put("java_version", System.getProperty("java.version"));
		runtime.put("java_vendor", System.getProperty("java.vendor"));
		runtime.put("java_vm", System.getProperty("java.vm.name"));
		record.put("runtime", runtime);
		record.put("seed_input", options.seedInput);
		record.put("seed_code", DungeonSeed.convertToCode(Dungeon.seed));
		record.put("seed", Dungeon.seed);
		record.put("requested_depths", new ArrayList<Integer>(options.depths));
		record.put("vault_requested", options.vault);
		record.put("challenges", Dungeon.challenges);
		record.put("hero_class", GamesInProgress.selectedClass.name());
		record.put("depth", Dungeon.depth);
		record.put("branch", Dungeon.branch);
		List<Map<String, Object>> depthSeeds = new ArrayList<Map<String, Object>>();
		for (int depth : options.depths) {
			Map<String, Object> value = new LinkedHashMap<String, Object>();
			value.put("depth", depth);
			value.put("branch", 0);
			value.put("seed", Dungeon.seedForDepth(depth, 0));
			depthSeeds.add(value);
		}
		record.put("depth_seeds", depthSeeds);
		record.put("limited_drops", limitedDrops());
		record.put("identification", identificationState());
		record.put("generator", generatorState());
		record.put("room_queues", roomQueueState());
		return record;
	}

	private static Map<String, Object> levelRecord(Level level) throws Exception {
		Map<String, Object> record = record("level");
		record.put("depth", Dungeon.depth);
		record.put("branch", Dungeon.branch);
		record.put("depth_seed", Dungeon.seedForDepth(Dungeon.depth, Dungeon.branch));
		record.put("level_class", className(level));
		record.put("feeling", level.feeling == null ? null : level.feeling.name());
		record.put("width", level.width());
		record.put("height", level.height());
		record.put("length", level.length());
		record.put("map_hash", arrayHash(level.map));
		record.put("map", integers(level.map));

		record.put("entrance", level.entrance());
		record.put("exit", level.exit());
		record.put("rooms", roomRecords(level));
		record.put("mobs", mobRecords(level));
		record.put("mob_count", level.mobs == null ? 0 : level.mobs.size());
		record.put("heap_count", level.heaps == null ? 0 : level.heaps.size);
		record.put("limited_drops", limitedDrops());
		record.put("generator_state_hash", generatorStateHash());
		record.put("generator", generatorState());
		record.put("room_queues", roomQueueState());
		return record;
	}

	private static Map<String, Object> generatorCheckpoint(Level level, boolean selected)
			throws Exception {
		Map<String, Object> record = record("generator_checkpoint");
		record.put("depth", Dungeon.depth);
		record.put("branch", Dungeon.branch);
		record.put("selected", selected);
		record.put("level_class", className(level));
		record.put("generator_state_hash", generatorStateHash());
		return record;
	}

	private static Map<String, Object> bossTransition(Level level, PersistentSnapshot before,
			PersistentSnapshot after) throws Exception {
		Map<String, Object> record = record("boss_transition");
		record.put("depth", Dungeon.depth);
		record.put("branch", Dungeon.branch);
		record.put("level_class", className(level));
		putTransition(record, before, after);
		List<Map<String, Object>> searchable = bossGeneratedSearchableItems(level);
		record.put("initial_searchable_count", searchable.size());
		record.put("initial_searchable_items", searchable);
		List<String> searchableClasses = new ArrayList<String>();
		for (Map<String, Object> item : searchable) {
			searchableClasses.add(String.valueOf(item.get("class")));
		}
		Collections.sort(searchableClasses);
		record.put("initial_searchable_classes", searchableClasses);
		return record;
	}

	private static void putTransition(Map<String, Object> record, PersistentSnapshot before,
			PersistentSnapshot after) {
		record.put("generator_hash_before", before.generatorHash);
		record.put("generator_hash_after", after.generatorHash);
		record.put("limited_drops_hash_before", before.limitedDropsHash);
		record.put("limited_drops_hash_after", after.limitedDropsHash);
		record.put("quests_hash_before", before.questsHash);
		record.put("quests_hash_after", after.questsHash);
		record.put("room_queues_hash_before", before.roomQueuesHash);
		record.put("room_queues_hash_after", after.roomQueuesHash);
		record.put("shop_state_hash_before", before.shopStateHash);
		record.put("shop_state_hash_after", after.shopStateHash);
		record.put("generator_unchanged", before.generator.equals(after.generator));
		record.put("limited_drops_unchanged", before.limitedDrops.equals(after.limitedDrops));
		record.put("quests_unchanged", before.quests.equals(after.quests));
		record.put("room_queues_unchanged", before.roomQueues.equals(after.roomQueues));
		record.put("shop_state_unchanged", before.shopState.equals(after.shopState));
	}

	private static boolean isBossDepth(int depth) {
		return depth == 5 || depth == 10 || depth == 15 || depth == 20 || depth == 25;
	}

	private static List<Map<String, Object>> roomRecords(Level level) throws Exception {
		if (!(level instanceof RegularLevel)) return new ArrayList<Map<String, Object>>();
		Object raw = getField(level, "rooms");
		if (!(raw instanceof Collection)) return new ArrayList<Map<String, Object>>();

		List<Room> rooms = new ArrayList<Room>();
		for (Object room : (Collection<?>) raw) rooms.add((Room) room);
		Collections.sort(rooms, new Comparator<Room>() {
			@Override public int compare(Room left, Room right) {
				return roomId(left).compareTo(roomId(right));
			}
		});

		List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
		for (Room room : rooms) {
			Map<String, Object> value = new LinkedHashMap<String, Object>();
			value.put("id", roomId(room));
			value.put("class", className(room));
			value.put("left", room.left);
			value.put("top", room.top);
			value.put("right", room.right);
			value.put("bottom", room.bottom);
			value.put("distance", room.distance);
			value.put("price", room.price);

			List<Map<String, Object>> connections = new ArrayList<Map<String, Object>>();
			for (Map.Entry<Room, Room.Door> connection : room.connected.entrySet()) {
				Map<String, Object> edge = new LinkedHashMap<String, Object>();
				edge.put("target", roomId(connection.getKey()));
				Room.Door door = connection.getValue();
				edge.put("door_x", door == null ? null : door.x);
				edge.put("door_y", door == null ? null : door.y);
				edge.put("door_type", door == null || door.type == null ? null : door.type.name());
				connections.add(edge);
			}
			Collections.sort(connections, new Comparator<Map<String, Object>>() {
				@Override public int compare(Map<String, Object> left, Map<String, Object> right) {
					return String.valueOf(left.get("target")).compareTo(String.valueOf(right.get("target")));
				}
			});
			value.put("connections", connections);
			result.add(value);
		}
		return result;
	}

	private static String roomId(Room room) {
		return className(room) + "@" + room.left + "," + room.top + "," + room.right + "," + room.bottom;
	}

	/** Simple class name of the room containing a cell, for regular (room-based) levels. */
	private static String roomAt(Level level, Integer cell) {
		if (cell == null || !(level instanceof RegularLevel)) return null;
		Room room = ((RegularLevel) level).room(cell);
		return room == null ? null : room.getClass().getSimpleName();
	}

	private static List<Map<String, Object>> mobRecords(Level level) {
		List<Mob> mobs = sortedMobs(level);
		List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
		for (Mob mob : mobs) {
			Map<String, Object> value = new LinkedHashMap<String, Object>();
			value.put("class", className(mob));
			value.put("cell", mob.pos);
			value.put("room", roomAt(level, mob.pos));
			value.put("hp", mob.HP);
			value.put("ht", mob.HT);
			value.put("alignment", mob.alignment == null ? null : mob.alignment.name());
			List<String> properties = new ArrayList<String>();
			for (Object property : mob.properties()) properties.add(String.valueOf(property));
			Collections.sort(properties);
			value.put("properties", properties);
			result.add(value);
		}
		return result;
	}

	private static List<Mob> sortedMobs(Level level) {
		List<Mob> mobs = level.mobs == null
				? new ArrayList<Mob>() : new ArrayList<Mob>(level.mobs);
		Collections.sort(mobs, new Comparator<Mob>() {
			@Override public int compare(Mob left, Mob right) {
				int cells = Integer.compare(left.pos, right.pos);
				if (cells != 0) return cells;
				return className(left).compareTo(className(right));
			}
		});
		return mobs;
	}

	/**
	 * Records the items of a generated floor.  When {@code heapSourceOverride} is
	 * null, ordinary heap sources are used; otherwise every heap item is attributed
	 * to that source (used for the Vault, whose full inventory is recorded).
	 */
	private static List<Map<String, Object>> itemRecords(Level level, String heapSourceOverride)
			throws Exception {
		List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();

		if (level.heaps != null) {
			int[] cells = level.heaps.keyArray();
			Arrays.sort(cells);
			int crystalVaultOption = 0;
			for (int cell : cells) {
				Heap heap = level.heaps.get(cell);
				String source = heapSourceOverride != null ? heapSourceOverride : heapSource(heap.type);
				Integer vaultOption = heapSourceOverride == null && heap.type == Heap.Type.CRYSTAL_CHEST
						? crystalVaultOption++ : null;
				int choice = 0;
				for (Item item : heap.items) {
					Map<String, Object> record = itemRecord(level, item, source, choice++, cell,
							heap.type.name(), null, null, null);
					if (vaultOption != null) {
						record.put("accessibility", choiceAccessibility(
								"crystal_vault@" + Dungeon.depth, vaultOption));
					}
					result.add(record);
				}
			}
		}

		for (Mob mob : sortedMobs(level)) {
			if (mob instanceof Mimic) {
				Mimic mimic = (Mimic) mob;
				if (mimic.items != null) {
					for (int choice = 0; choice < mimic.items.size(); choice++) {
						result.add(itemRecord(level, mimic.items.get(choice), "mimic", choice, mob.pos,
								null, className(mob), null, null));
					}
				}
			} else if (mob instanceof Statue) {
				Statue statue = (Statue) mob;
				if (statue.weapon() != null) {
					result.add(itemRecord(level, statue.weapon(), "statue", 0, mob.pos,
							null, className(mob), null, null));
				}
				if (mob instanceof ArmoredStatue && ((ArmoredStatue) mob).armor() != null) {
					result.add(itemRecord(level, ((ArmoredStatue) mob).armor(), "statue", 1, mob.pos,
							null, className(mob), null, null));
				}
			}
		}

		SacrificialFire fire = level.blobs == null ? null
				: (SacrificialFire) level.blobs.get(SacrificialFire.class);
		if (fire != null) {
			Object prize = getField(fire, "prize");
			if (prize instanceof Item) {
				result.add(itemRecord(level, (Item) prize, "sacrificial_prize", 0, null,
						null, className(fire), null, null));
			}
		}

		result.addAll(bossGeneratedSearchableItems(level));

		return result;
	}

	private static List<Map<String, Object>> bossGeneratedSearchableItems(Level level)
			throws Exception {
		List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
		if (!(level instanceof CityBossLevel)) return result;

		Object rawShop = getField(level, "impShop");
		if (!(rawShop instanceof ImpShopRoom)) return result;
		Object rawItems = getField(rawShop, "itemsToSpawn");
		if (!(rawItems instanceof Collection)) return result;

		int choice = 0;
		for (Object value : (Collection<?>) rawItems) {
			if (value instanceof Item && isSearchable((Item) value)) {
				result.add(itemRecord(level, (Item) value, "imp_shop_cache", choice, null,
						null, ImpShopRoom.class.getName(), null, null));
			}
			choice++;
		}
		return result;
	}

	private static String heapSource(Heap.Type type) {
		switch (type) {
			case FOR_SALE: return "shop";
			case CHEST: return "chest";
			case LOCKED_CHEST: return "locked_chest";
			case CRYSTAL_CHEST: return "crystal_chest";
			case TOMB: return "tomb";
			case SKELETON: return "skeleton";
			case REMAINS: return "remains";
			case HEAP:
			default: return "heap";
		}
	}

	private static Map<String, Object> itemRecord(Level level, Item item, String source, int choice,
			Integer cell, String container, String owner, Object enchantOverride, Object glyphOverride) {
		Map<String, Object> record = record("item");
		record.put("depth", Dungeon.depth);
		record.put("branch", Dungeon.branch);
		record.put("source", source);
		record.put("choice", choice);
		record.put("cell", cell);
		record.put("room", roomAt(level, cell));
		record.put("container", container);
		record.put("owner", owner);
		record.put("class", className(item));
		record.put("simple_class", item.getClass().getSimpleName());
		record.put("kind", itemKind(item));
		record.put("searchable", isSearchable(item));
		record.put("true_level", item.trueLevel());
		record.put("cursed", item.cursed);
		record.put("quantity", item.quantity());

		Object enchantment = enchantOverride;
		Object glyph = glyphOverride;
		if (enchantment == null && item instanceof Weapon) enchantment = ((Weapon) item).enchantment;
		if (glyph == null && item instanceof Armor) glyph = ((Armor) item).glyph;
		record.put("enchantment", enchantment == null ? null : className(enchantment));
		record.put("glyph", glyph == null ? null : className(glyph));
		record.put("accessibility", accessibilityRecord(source, choice));
		return record;
	}

	private static Map<String, Object> accessibilityRecord(String source, int choice) {
		if ("ghost_quest".equals(source) || "wandmaker_quest".equals(source)
				|| "blacksmith_quest".equals(source) || "imp_quest".equals(source)) {
			return choiceAccessibility(source + "@" + Dungeon.depth, choice);
		}
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		result.put("kind", "independent");
		return result;
	}

	private static Map<String, Object> choiceAccessibility(String group, int option) {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		result.put("kind", "choice");
		result.put("group", group);
		result.put("option", option);
		return result;
	}

	private static boolean isSearchable(Item item) {
		return item instanceof Weapon || item instanceof Armor || item instanceof Wand
				|| item instanceof Ring;
	}

	private static String itemKind(Item item) {
		if (item instanceof Wand) return "wand";
		if (item instanceof Armor) return "armor";
		if (item instanceof Weapon) return "weapon";
		if (item instanceof Ring) return "ring";
		if (item instanceof Artifact) return "artifact";
		return "other";
	}

	private static void sortItemRecords(List<Map<String, Object>> items) {
		Collections.sort(items, new Comparator<Map<String, Object>>() {
			@Override public int compare(Map<String, Object> left, Map<String, Object> right) {
				int source = String.valueOf(left.get("source")).compareTo(String.valueOf(right.get("source")));
				if (source != 0) return source;
				int cell = Integer.compare(nullableInt(left.get("cell")), nullableInt(right.get("cell")));
				if (cell != 0) return cell;
				int choice = Integer.compare((Integer) left.get("choice"), (Integer) right.get("choice"));
				if (choice != 0) return choice;
				return String.valueOf(left.get("class")).compareTo(String.valueOf(right.get("class")));
			}
		});
	}

	private static int nullableInt(Object value) {
		return value == null ? -1 : ((Number) value).intValue();
	}

	private static List<Map<String, Object>> limitedDrops() {
		List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
		for (Dungeon.LimitedDrops drop : Dungeon.LimitedDrops.values()) {
			Map<String, Object> value = new LinkedHashMap<String, Object>();
			value.put("drop", drop.name());
			value.put("count", drop.count);
			result.add(value);
		}
		return result;
	}

	private static Map<String, Object> questState() throws Exception {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		result.put("ghost", staticState(Ghost.Quest.class, new String[]{
				"spawned", "type", "given", "processed", "depth", "weapon", "armor",
				"enchant", "glyph"
		}));
		result.put("wandmaker", staticState(Wandmaker.Quest.class, new String[]{
				"type", "spawned", "given", "wand1", "wand2", "questRoomSpawned"
		}));
		result.put("wandmaker_ritual_pos", getStaticField(CeremonialCandle.class, "ritualPos"));
		result.put("blacksmith", staticState(Blacksmith.Quest.class, new String[]{
				"type", "spawned", "given", "started", "bossBeaten", "completed", "favor",
				"pickaxe", "freePickaxe", "reforges", "hardens", "upgrades", "smiths",
				"smithRewards", "smithEnchant", "smithGlyph"
		}));
		// Imp.Quest.rewardOptions is deliberately excluded: it is transient (rolled on
		// the Imp's floor and cleared when VaultFinalRoom paints it) and is recorded
		// as imp_quest / vault_heap item records instead.
		result.put("imp", staticState(Imp.Quest.class, new String[]{
				"oldQuest", "alternative", "spawned", "given", "completed", "reward",
				"hazardFreebies", "mirrorUsed", "score"
		}));
		return result;
	}

	private static Map<String, Object> staticState(Class<?> type, String[] fields) throws Exception {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		for (String field : fields) result.put(field, stableStateValue(getStaticField(type, field)));
		return result;
	}

	private static Object stableStateValue(Object value) {
		if (value == null || value instanceof String || value instanceof Boolean
				|| value instanceof Number) return value;
		if (value instanceof Enum) return String.valueOf(value);
		if (value instanceof Item) return itemState((Item) value);
		if (value instanceof Collection) {
			List<Object> result = new ArrayList<Object>();
			for (Object item : (Collection<?>) value) result.add(stableStateValue(item));
			return result;
		}
		return className(value);
	}

	private static Map<String, Object> itemState(Item item) {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		result.put("class", className(item));
		result.put("true_level", item.trueLevel());
		result.put("cursed", item.cursed);
		result.put("quantity", item.quantity());
		result.put("enchantment", item instanceof Weapon
				&& ((Weapon) item).enchantment != null ? className(((Weapon) item).enchantment) : null);
		result.put("glyph", item instanceof Armor
				&& ((Armor) item).glyph != null ? className(((Armor) item).glyph) : null);
		return result;
	}

	private static Map<String, Object> shopState() {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		List<Object> backpack = new ArrayList<Object>();
		for (Item item : Dungeon.hero.belongings.backpack.items) {
			backpack.add(itemState(item));
		}
		result.put("hero_backpack", backpack);
		TimekeepersHourglass hourglass = Dungeon.hero.belongings.getItem(TimekeepersHourglass.class);
		result.put("hourglass_sand_bags", hourglass == null ? null : hourglass.sandBags);
		Map<String, Object> bags = new LinkedHashMap<String, Object>();
		bags.put("VELVET_POUCH", Dungeon.LimitedDrops.VELVET_POUCH.count);
		bags.put("SCROLL_HOLDER", Dungeon.LimitedDrops.SCROLL_HOLDER.count);
		bags.put("POTION_BANDOLIER", Dungeon.LimitedDrops.POTION_BANDOLIER.count);
		bags.put("MAGICAL_HOLSTER", Dungeon.LimitedDrops.MAGICAL_HOLSTER.count);
		result.put("limited_bags", bags);
		return result;
	}

	private static PersistentSnapshot persistentSnapshot() throws Exception {
		Map<String, Object> generator = generatorState();
		List<Map<String, Object>> limitedDrops = limitedDrops();
		Map<String, Object> quests = questState();
		Map<String, Object> roomQueues = roomQueueState();
		Map<String, Object> shopState = shopState();
		return new PersistentSnapshot(generator, limitedDrops, quests, roomQueues, shopState);
	}

	private static Map<String, Object> roomQueueState() throws Exception {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		result.put("special_run", classNames(SpecialRoom.runSpecials));
		result.put("special_floor", classNames(SpecialRoom.floorSpecials));
		result.put("pit_needed_depth", getStaticField(SpecialRoom.class, "pitNeededDepth"));
		result.put("secret_run", classNames(SecretRoom.runSecrets));
		result.put("secret_region_remaining",
				integers((int[]) getStaticField(SecretRoom.class, "regionSecretsThisRun")));
		return result;
	}

	private static List<String> classNames(Collection<?> classes) {
		List<String> result = new ArrayList<String>();
		for (Object value : classes) result.add(((Class<?>) value).getName());
		return result;
	}

	private static Map<String, Object> identificationState() throws Exception {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		result.put("potions", labelsFor(Potion.class));
		result.put("scrolls", labelsFor(Scroll.class));
		result.put("rings", labelsFor(Ring.class));
		return result;
	}

	private static List<Map<String, Object>> labelsFor(Class<?> itemClass) throws Exception {
		Object handler = getStaticField(itemClass, "handler");
		Object raw = getField(handler, "itemLabels");
		List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
		for (Map.Entry<?, ?> entry : ((Map<?, ?>) raw).entrySet()) {
			Map<String, Object> value = new LinkedHashMap<String, Object>();
			value.put("class", ((Class<?>) entry.getKey()).getName());
			value.put("label", entry.getValue());
			result.add(value);
		}
		Collections.sort(result, new Comparator<Map<String, Object>>() {
			@Override public int compare(Map<String, Object> left, Map<String, Object> right) {
				return String.valueOf(left.get("class")).compareTo(String.valueOf(right.get("class")));
			}
		});
		return result;
	}

	private static Map<String, Object> generatorState() throws Exception {
		Map<String, Object> result = new LinkedHashMap<String, Object>();
		result.put("using_first_deck", getStaticField(Generator.class, "usingFirstDeck"));

		Map<?, ?> categoryProbabilities = (Map<?, ?>) getStaticField(Generator.class, "categoryProbs");
		List<Map<String, Object>> probabilities = new ArrayList<Map<String, Object>>();
		for (Generator.Category category : Generator.Category.values()) {
			Map<String, Object> value = new LinkedHashMap<String, Object>();
			value.put("category", category.name());
			value.put("probability", categoryProbabilities.get(category));
			probabilities.add(value);
		}
		result.put("category_probabilities", probabilities);

		List<Map<String, Object>> categories = new ArrayList<Map<String, Object>>();
		for (Generator.Category category : Generator.Category.values()) {
			Map<String, Object> value = new LinkedHashMap<String, Object>();
			value.put("category", category.name());
			value.put("using_second_deck", category.using2ndProbs);
			value.put("seed", category.seed);
			value.put("dropped", category.dropped);
			value.put("probabilities", floats(category.probs));
			value.put("default_probabilities", floats(category.defaultProbs));
			value.put("default_probabilities_2", floats(category.defaultProbs2));
			value.put("default_probabilities_total", floats(category.defaultProbsTotal));
			List<String> classes = new ArrayList<String>();
			if (category.classes != null) {
				for (Class<?> itemClass : category.classes) classes.add(itemClass.getName());
			}
			value.put("classes", classes);
			categories.add(value);
		}
		result.put("categories", categories);
		return result;
	}

	private static List<Float> floats(float[] values) {
		if (values == null) return null;
		List<Float> result = new ArrayList<Float>(values.length);
		for (float value : values) result.add(value);
		return result;
	}

	private static List<Integer> integers(int[] values) {
		List<Integer> result = new ArrayList<Integer>(values == null ? 0 : values.length);
		if (values != null) for (int value : values) result.add(value);
		return result;
	}

	private static int generatorStateHash() throws Exception {
		return Json.encode(generatorState()).hashCode();
	}

	private static int arrayHash(int[] values) {
		return values == null ? 0 : Arrays.hashCode(values);
	}

	private static Map<String, Object> record(String kind) {
		Map<String, Object> value = new LinkedHashMap<String, Object>();
		value.put("schema", SCHEMA);
		value.put("record", kind);
		return value;
	}

	private static String className(Object value) {
		return value == null ? null : value.getClass().getName();
	}

	private static Field findField(Class<?> type, String name) throws NoSuchFieldException {
		Class<?> cursor = type;
		while (cursor != null) {
			try {
				Field field = cursor.getDeclaredField(name);
				field.setAccessible(true);
				return field;
			} catch (NoSuchFieldException ignored) {
				cursor = cursor.getSuperclass();
			}
		}
		throw new NoSuchFieldException(type.getName() + "." + name);
	}

	private static Object getField(Object target, String name) throws Exception {
		return findField(target.getClass(), name).get(target);
	}

	private static Object getStaticField(Class<?> type, String name) throws Exception {
		return findField(type, name).get(null);
	}

	private static void setStaticField(Class<?> type, String name, Object value) throws Exception {
		findField(type, name).set(null, value);
	}

	private static void printUsage() {
		System.out.println("Usage: parity-oracle --seed XXX-XXX-XXX [--floors LIST] [--format ndjson|json]");
		System.out.println("  --floors 1,3-5     Generate through the highest depth and emit the selected depths");
		System.out.println("  --challenges N     Challenge bit mask (default: 0)");
		System.out.println("  --run-checkpoints  Emit a Generator-state hash after every generated floor");
		System.out.println("  --boss-skip-checkpoints  Compare all persistent state around boss floors");
		System.out.println("  --vault            Also build the Imp's Vault (branch 1) after the main floors");
		System.out.println("  --no-phases        Accepted for v1 compatibility; v2 has no phase records");
		System.out.println("Positional form is also accepted: parity-oracle XXX-XXX-XXX 1,3-5");
	}

	private static final class PersistentSnapshot {
		final Map<String, Object> generator;
		final List<Map<String, Object>> limitedDrops;
		final Map<String, Object> quests;
		final Map<String, Object> roomQueues;
		final Map<String, Object> shopState;
		final int generatorHash;
		final int limitedDropsHash;
		final int questsHash;
		final int roomQueuesHash;
		final int shopStateHash;

		PersistentSnapshot(Map<String, Object> generator,
				List<Map<String, Object>> limitedDrops, Map<String, Object> quests,
				Map<String, Object> roomQueues, Map<String, Object> shopState) {
			this.generator = generator;
			this.limitedDrops = limitedDrops;
			this.quests = quests;
			this.roomQueues = roomQueues;
			this.shopState = shopState;
			this.generatorHash = Json.encode(generator).hashCode();
			this.limitedDropsHash = Json.encode(limitedDrops).hashCode();
			this.questsHash = Json.encode(quests).hashCode();
			this.roomQueuesHash = Json.encode(roomQueues).hashCode();
			this.shopStateHash = Json.encode(shopState).hashCode();
		}
	}

	private static final class QuestTracker {
		private boolean ghost;
		private boolean wandmaker;
		private boolean blacksmith;
		private boolean imp;
		/** Depth of the City floor whose AmbitiousImpRoom rolled the reward options, or -1. */
		int impDepth = -1;

		List<Map<String, Object>> observe(int depth, boolean emit) {
			List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
			if (!ghost && Ghost.Quest.weapon != null && Ghost.Quest.armor != null) {
				ghost = true;
				if (emit) {
					result.add(itemRecord(null, Ghost.Quest.weapon, "ghost_quest", 0, null,
							null, Ghost.class.getName(), Ghost.Quest.enchant, null));
					result.add(itemRecord(null, Ghost.Quest.armor, "ghost_quest", 1, null,
							null, Ghost.class.getName(), null, Ghost.Quest.glyph));
				}
			}
			if (!wandmaker && Wandmaker.Quest.wand1 != null && Wandmaker.Quest.wand2 != null) {
				wandmaker = true;
				if (emit) {
					result.add(itemRecord(null, Wandmaker.Quest.wand1, "wandmaker_quest", 0, null,
							null, Wandmaker.class.getName(), null, null));
					result.add(itemRecord(null, Wandmaker.Quest.wand2, "wandmaker_quest", 1, null,
							null, Wandmaker.class.getName(), null, null));
				}
			}
			if (!blacksmith && Blacksmith.Quest.smithRewards != null) {
				blacksmith = true;
				if (emit) {
					for (int choice = 0; choice < Blacksmith.Quest.smithRewards.size(); choice++) {
						Item item = Blacksmith.Quest.smithRewards.get(choice);
						Object enchant = item instanceof Weapon ? Blacksmith.Quest.smithEnchant : null;
						Object glyph = item instanceof Armor ? Blacksmith.Quest.smithGlyph : null;
						result.add(itemRecord(null, item, "blacksmith_quest", choice, null,
								null, Blacksmith.class.getName(), enchant, glyph));
					}
				}
			}
			// v4.0.0: Imp.Quest.spawn rolls six reward options (artifact-or-ring, ring,
			// weapon/missile pair, PlateArmor, wand) on the Imp's floor.  The list is
			// captured here, before VaultFinalRoom.paint() clears it.
			if (!imp && !Imp.Quest.rewardOptions.isEmpty()) {
				imp = true;
				impDepth = depth;
				if (emit) {
					for (int choice = 0; choice < Imp.Quest.rewardOptions.size(); choice++) {
						result.add(itemRecord(null, Imp.Quest.rewardOptions.get(choice), "imp_quest",
								choice, null, null, Imp.class.getName(), null, null));
					}
				}
			}
			return result;
		}
	}

	private static final class Options {
		String seedInput;
		String seedCode;
		TreeSet<Integer> depths = new TreeSet<Integer>();
		String format = "ndjson";
		int challenges;
		boolean runCheckpoints;
		boolean bossSkipCheckpoints;
		boolean vault;
		boolean help;

		static Options parse(String[] args) {
			Options result = new Options();
			List<String> positional = new ArrayList<String>();
			for (int i = 0; i < args.length; i++) {
				String arg = args[i];
				if ("--help".equals(arg) || "-h".equals(arg)) {
					result.help = true;
				} else if ("--no-phases".equals(arg)) {
					// v1 compatibility: schema v2 never emits level_phase records.
				} else if ("--run-checkpoints".equals(arg)) {
					result.runCheckpoints = true;
				} else if ("--boss-skip-checkpoints".equals(arg)) {
					result.bossSkipCheckpoints = true;
				} else if ("--vault".equals(arg)) {
					result.vault = true;
				} else if ("--seed".equals(arg)) {
					result.seedInput = requireValue(args, ++i, arg);
				} else if ("--floors".equals(arg) || "--depths".equals(arg)) {
					parseDepths(requireValue(args, ++i, arg), result.depths);
				} else if ("--format".equals(arg)) {
					result.format = requireValue(args, ++i, arg).toLowerCase(Locale.ROOT);
				} else if ("--challenges".equals(arg)) {
					result.challenges = Integer.parseInt(requireValue(args, ++i, arg));
				} else if (arg.startsWith("--")) {
					throw new IllegalArgumentException("unknown option: " + arg);
				} else {
					positional.add(arg);
				}
			}

			if (result.help) return result;
			if (result.seedInput == null && !positional.isEmpty()) result.seedInput = positional.remove(0);
			if (result.depths.isEmpty() && !positional.isEmpty()) parseDepths(positional.remove(0), result.depths);
			if (!positional.isEmpty()) throw new IllegalArgumentException("too many positional arguments");
			if (result.seedInput == null) throw new IllegalArgumentException("--seed is required");
			if (result.depths.isEmpty()) result.depths.add(1);
			if (!"ndjson".equals(result.format) && !"json".equals(result.format)) {
				throw new IllegalArgumentException("format must be ndjson or json");
			}

			long seed = DungeonSeed.convertFromCode(result.seedInput);
			result.seedCode = DungeonSeed.convertToCode(seed);
			return result;
		}

		private static String requireValue(String[] args, int index, String option) {
			if (index >= args.length) throw new IllegalArgumentException(option + " requires a value");
			return args[index];
		}

		private static void parseDepths(String spec, Set<Integer> output) {
			for (String part : spec.split(",")) {
				String token = part.trim();
				if (token.isEmpty()) throw new IllegalArgumentException("empty floor in: " + spec);
				int dash = token.indexOf('-');
				if (dash < 0) {
					addDepth(Integer.parseInt(token), output);
				} else {
					int first = Integer.parseInt(token.substring(0, dash));
					int last = Integer.parseInt(token.substring(dash + 1));
					if (first > last) throw new IllegalArgumentException("descending floor range: " + token);
					for (int value = first; value <= last; value++) addDepth(value, output);
				}
			}
		}

		private static void addDepth(int depth, Set<Integer> output) {
			if (depth < 1 || depth > MAX_DEPTH) {
				throw new IllegalArgumentException("floors must be in [1, " + MAX_DEPTH + "]");
			}
			output.add(depth);
		}
	}

	private static final class Output {
		private final String format;
		private final PrintWriter writer = new PrintWriter(System.out, true);
		private final List<Map<String, Object>> records = new ArrayList<Map<String, Object>>();

		Output(String format) {
			this.format = format;
		}

		void emit(Map<String, Object> record) {
			// Defer serialization for both formats. Serializing the large run-init
			// record allocates substantially; doing that before level generation
			// would make the two output formats exercise different JVM allocation
			// schedules even though they observe the same game path.
			records.add(record);
		}

		void finish() {
			if ("json".equals(format)) {
				Map<String, Object> document = new LinkedHashMap<String, Object>();
				document.put("schema", SCHEMA);
				document.put("records", records);
				writer.println(Json.encode(document));
			} else for (Map<String, Object> record : records) writer.println(Json.encode(record));
			writer.flush();
		}
	}

	private static final class Json {
		static String encode(Object value) {
			StringBuilder result = new StringBuilder();
			append(result, value);
			return result.toString();
		}

		private static void append(StringBuilder result, Object value) {
			if (value == null) {
				result.append("null");
			} else if (value instanceof String || value instanceof Character || value instanceof Enum) {
				string(result, String.valueOf(value));
			} else if (value instanceof Boolean || value instanceof Byte || value instanceof Short
					|| value instanceof Integer || value instanceof Long) {
				result.append(value);
			} else if (value instanceof Float || value instanceof Double) {
				double number = ((Number) value).doubleValue();
				if (Double.isNaN(number) || Double.isInfinite(number)) result.append("null");
				else result.append(value);
			} else if (value instanceof Map) {
				result.append('{');
				boolean first = true;
				for (Map.Entry<?, ?> entry : ((Map<?, ?>) value).entrySet()) {
					if (!first) result.append(',');
					first = false;
					string(result, String.valueOf(entry.getKey()));
					result.append(':');
					append(result, entry.getValue());
				}
				result.append('}');
			} else if (value instanceof Iterable) {
				result.append('[');
				boolean first = true;
				for (Object item : (Iterable<?>) value) {
					if (!first) result.append(',');
					first = false;
					append(result, item);
				}
				result.append(']');
			} else if (value.getClass().isArray()) {
				result.append('[');
				int length = java.lang.reflect.Array.getLength(value);
				for (int index = 0; index < length; index++) {
					if (index > 0) result.append(',');
					append(result, java.lang.reflect.Array.get(value, index));
				}
				result.append(']');
			} else {
				throw new IllegalArgumentException("unsupported JSON value: " + value.getClass().getName());
			}
		}

		private static void string(StringBuilder result, String value) {
			result.append('"');
			for (int index = 0; index < value.length(); index++) {
				char c = value.charAt(index);
				switch (c) {
					case '"': result.append("\\\""); break;
					case '\\': result.append("\\\\"); break;
					case '\b': result.append("\\b"); break;
					case '\f': result.append("\\f"); break;
					case '\n': result.append("\\n"); break;
					case '\r': result.append("\\r"); break;
					case '\t': result.append("\\t"); break;
					default:
						if (c < 0x20) result.append(String.format(Locale.ROOT, "\\u%04x", (int) c));
						else result.append(c);
				}
			}
			result.append('"');
		}
	}

	private static final class MemoryPreferences implements Preferences {
		private final Map<String, Object> values = new HashMap<String, Object>();

		@Override public Preferences putBoolean(String key, boolean val) { values.put(key, val); return this; }
		@Override public Preferences putInteger(String key, int val) { values.put(key, val); return this; }
		@Override public Preferences putLong(String key, long val) { values.put(key, val); return this; }
		@Override public Preferences putFloat(String key, float val) { values.put(key, val); return this; }
		@Override public Preferences putString(String key, String val) { values.put(key, val); return this; }
		@Override public Preferences put(Map<String, ?> vals) { values.putAll(vals); return this; }
		@Override public boolean getBoolean(String key) { return getBoolean(key, false); }
		@Override public int getInteger(String key) { return getInteger(key, 0); }
		@Override public long getLong(String key) { return getLong(key, 0L); }
		@Override public float getFloat(String key) { return getFloat(key, 0f); }
		@Override public String getString(String key) { return getString(key, ""); }
		@Override public boolean getBoolean(String key, boolean defValue) { Object v = values.get(key); return v instanceof Boolean ? (Boolean) v : defValue; }
		@Override public int getInteger(String key, int defValue) { Object v = values.get(key); return v instanceof Number ? ((Number) v).intValue() : defValue; }
		@Override public long getLong(String key, long defValue) { Object v = values.get(key); return v instanceof Number ? ((Number) v).longValue() : defValue; }
		@Override public float getFloat(String key, float defValue) { Object v = values.get(key); return v instanceof Number ? ((Number) v).floatValue() : defValue; }
		@Override public String getString(String key, String defValue) { Object v = values.get(key); return v instanceof String ? (String) v : defValue; }
		@Override public Map<String, ?> get() { return new HashMap<String, Object>(values); }
		@Override public boolean contains(String key) { return values.containsKey(key); }
		@Override public void clear() { values.clear(); }
		@Override public void remove(String key) { values.remove(key); }
		@Override public void flush() { }
	}
}
