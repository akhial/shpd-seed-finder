/*
 * Shattered Pixel Dungeon Java baseline seed finder
 * Copyright (C) 2026
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 */

package com.shatteredpixel.shatteredpixeldungeon;

import com.badlogic.gdx.Preferences;
import com.shatteredpixel.shatteredpixeldungeon.actors.blobs.SacrificialFire;
import com.shatteredpixel.shatteredpixeldungeon.actors.hero.HeroClass;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.ArmoredStatue;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.Mimic;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.Mob;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.Statue;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Blacksmith;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Ghost;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Imp;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.Wandmaker;
import com.shatteredpixel.shatteredpixeldungeon.items.Heap;
import com.shatteredpixel.shatteredpixeldungeon.items.Item;
import com.shatteredpixel.shatteredpixeldungeon.items.armor.Armor;
import com.shatteredpixel.shatteredpixeldungeon.items.rings.Ring;
import com.shatteredpixel.shatteredpixeldungeon.items.wands.Wand;
import com.shatteredpixel.shatteredpixeldungeon.items.weapon.Weapon;
import com.shatteredpixel.shatteredpixeldungeon.levels.CityBossLevel;
import com.shatteredpixel.shatteredpixeldungeon.levels.Level;
import com.shatteredpixel.shatteredpixeldungeon.levels.rooms.standard.ImpShopRoom;
import com.shatteredpixel.shatteredpixeldungeon.utils.DungeonSeed;
import com.watabou.noosa.Game;
import com.watabou.utils.GameSettings;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * A seed finder for Shattered Pixel Dungeon v4.0.0-BETA-3, driven headlessly
 * against the unmodified official desktop JAR.
 *
 * <p>It exists so that Seed Seeker's throughput can be compared with what the
 * game's own generator achieves on the JVM at the version Seed Seeker targets.
 * Elektrochecker's shpd-seed-finder — the tool this replaces as a baseline —
 * patches the game's <em>source</em> and so is pinned to v3.3.8; no v4.0.0
 * source has been published, only the release JAR. The startup technique is
 * the one {@code tooling/oracle-4.0} documents: nothing from the game is
 * recompiled and the only shadow class is a geometry-only
 * {@code com.watabou.noosa.TextureFilm} stand-in.
 *
 * <p>Per seed the finder runs the game's own {@code Dungeon.init()} and
 * {@code Dungeon.newLevel()} over floors 1..N, plus the Imp's Vault when the
 * Imp has spawned, and scans everything generated — heaps and their containers,
 * mimics, statues, the sacrificial-fire prize, the Imp's shop cache and the
 * Ghost/Wandmaker/Blacksmith/Imp reward options — for an item of a named class
 * at a named upgrade. The game's generator state is global, so one JVM searches
 * one seed at a time; use several processes over disjoint ranges for a
 * multi-core figure.
 */
public final class JarSeedFinder {

	private static final String GAME_VERSION = "4.0.0-BETA-3";
	private static final int GAME_VERSION_CODE = 904;
	/** {@code DeviceCompat.isDebug()} is {@code Game.version.contains("INDEV")}; see the oracle's README. */
	private static final String EFFECTIVE_GAME_VERSION = GAME_VERSION + "-INDEV";

	/** The Imp is guaranteed to have spawned by this depth. */
	private static final int LAST_IMP_DEPTH = 19;

	private JarSeedFinder() {
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
			System.err.println("java-finder: " + error.getMessage());
			System.err.println("Try --help for usage.");
			System.exit(2);
		} catch (Throwable error) {
			error.printStackTrace(System.err);
			System.exit(1);
		}
	}

	private static void run(Options options) throws Exception {
		Game.version = EFFECTIVE_GAME_VERSION;
		Game.versionCode = GAME_VERSION_CODE;
		GameSettings.set(new MemoryPreferences());
		SPDSettings.intro(false);
		setStaticField(Badges.class, "global", new HashSet<Badges.Badge>());
		setStaticField(Bones.class, "depth", 0);
		setStaticField(Bones.class, "branch", -1);
		setStaticField(Bones.class, "item", null);
		setStaticField(Bones.class, "heroClass", null);
		Dungeon.daily = false;
		Dungeon.dailyReplay = false;
		SPDSettings.challenges(options.challenges);

		List<String> matches = new ArrayList<String>();
		// The JIT needs a few hundred runs before the generator settles; the
		// warmup seeds are searched exactly like the rest but not timed, which
		// is the friendlier of the two readings for the JVM.
		for (long index = 0; index < options.warmup; index++) {
			search(options.start + index, options);
		}

		long tested = 0;
		long startedAt = System.nanoTime();
		for (long index = 0; index < options.seeds; index++) {
			long seed = options.start + options.warmup + index;
			if (search(seed, options)) matches.add(DungeonSeed.convertToCode(seed));
			tested++;
		}
		double elapsed = (System.nanoTime() - startedAt) / 1_000_000_000.0;

		if (options.printMatches) {
			for (String code : matches) System.out.println(code);
		}
		System.out.printf(Locale.ROOT,
				"BENCH item=%s+%d floors=%d start=%d warmup=%d seeds=%d matches=%d "
						+ "elapsed=%.3f seeds_per_s=%.1f%n",
				options.item, options.upgrade, options.floors, options.start + options.warmup,
				options.warmup, tested, matches.size(), elapsed, tested / elapsed);
	}

	/** Generates one seed's world and reports whether it holds the wanted item. */
	private static boolean search(long seed, Options options) throws Exception {
		SPDSettings.customSeed(DungeonSeed.convertToCode(seed));
		Dungeon.initSeed();
		GamesInProgress.selectedClass = HeroClass.WARRIOR;
		Dungeon.init();
		resetLeftoverQuestState();

		int impDepth = -1;
		boolean ghost = false;
		boolean wandmaker = false;
		boolean blacksmith = false;
		boolean found = false;
		for (int depth = 1; depth <= options.floors && !found; depth++) {
			// Depths 5, 10, 15 and 25 leave no run-persistent state behind — the
			// oracle's boss-skip fixtures pin that — so a search can step over
			// them. Depth 20 is not neutral (it caches the Imp's shop) and is
			// never skipped.
			if (options.skipBossFloors && isSkippableBossDepth(depth)) {
				Dungeon.depth++;
				continue;
			}
			Level level = Dungeon.newLevel();
			found = matches(level, options);

			if (!ghost && Ghost.Quest.weapon != null && Ghost.Quest.armor != null) {
				ghost = true;
				found |= matches(Ghost.Quest.weapon, options) || matches(Ghost.Quest.armor, options);
			}
			if (!wandmaker && Wandmaker.Quest.wand1 != null && Wandmaker.Quest.wand2 != null) {
				wandmaker = true;
				found |= matches(Wandmaker.Quest.wand1, options) || matches(Wandmaker.Quest.wand2, options);
			}
			if (!blacksmith && Blacksmith.Quest.smithRewards != null) {
				blacksmith = true;
				found |= matches(Blacksmith.Quest.smithRewards, options);
			}
			// Imp.Quest.rewardOptions is rolled on the Imp's City floor and cleared
			// again by VaultFinalRoom.paint(), so it has to be read here.
			if (impDepth < 0 && !Imp.Quest.rewardOptions.isEmpty()) {
				impDepth = depth;
				found |= matches(Imp.Quest.rewardOptions, options);
			}

			Dungeon.depth++;
		}

		if (!found && options.vault && impDepth > 0) found = searchVault(impDepth, options);
		return found;
	}

	/** Boss depths whose generation is run-state neutral; depth 20 is not. */
	private static boolean isSkippableBossDepth(int depth) {
		return depth == 5 || depth == 10 || depth == 15 || depth == 25;
	}

	/**
	 * Restores the run statics that {@code Dungeon.init()} leaves alone because
	 * the game never searches two seeds in one process.
	 *
	 * <p>{@code Imp.Quest.reset()} does not touch {@code rewardOptions},
	 * {@code oldQuest} or {@code alternative}: the reward options are rolled on
	 * the Imp's floor and cleared again by {@code VaultFinalRoom.paint()}, and
	 * the two flags are assigned by {@code Imp.Quest.spawn()}. A run whose vault
	 * is never built therefore hands its reward options to the next seed in the
	 * loop, where they would be read as that seed's own; the values a fresh JVM
	 * would hold are restored here instead.
	 */
	private static void resetLeftoverQuestState() throws Exception {
		Imp.Quest.rewardOptions.clear();
		setStaticField(Imp.Quest.class, "oldQuest", Boolean.FALSE);
		setStaticField(Imp.Quest.class, "alternative", Boolean.FALSE);
	}

	/**
	 * Builds the Imp's Vault (branch 1 of the Imp's floor) and scans it. The
	 * sub-level is seeded independently by {@code Dungeon.seedForDepth(depth, 1)}
	 * and leaves no run-persistent state behind, which is why it can be built
	 * after the main floors rather than between them; {@code tooling/oracle-4.0}
	 * pins that neutrality.
	 */
	private static boolean searchVault(int impDepth, Options options) throws Exception {
		int savedDepth = Dungeon.depth;
		int savedBranch = Dungeon.branch;
		Dungeon.depth = impDepth;
		Dungeon.branch = 1;
		try {
			return matches(Dungeon.newLevel(), options);
		} finally {
			Dungeon.depth = savedDepth;
			Dungeon.branch = savedBranch;
		}
	}

	/** Scans everything a generated floor carries: heaps, mimics, statues, prizes. */
	private static boolean matches(Level level, Options options) throws Exception {
		if (level.heaps != null) {
			for (int cell : level.heaps.keyArray()) {
				Heap heap = level.heaps.get(cell);
				if (heap != null && matches(heap.items, options)) return true;
			}
		}

		if (level.mobs != null) {
			for (Mob mob : level.mobs) {
				if (mob instanceof Mimic) {
					if (matches(((Mimic) mob).items, options)) return true;
				} else if (mob instanceof Statue) {
					if (matches(((Statue) mob).weapon(), options)) return true;
					if (mob instanceof ArmoredStatue
							&& matches(((ArmoredStatue) mob).armor(), options)) {
						return true;
					}
				}
			}
		}

		SacrificialFire fire = level.blobs == null ? null
				: (SacrificialFire) level.blobs.get(SacrificialFire.class);
		if (fire != null && matches(getField(fire, "prize"), options)) return true;

		return level instanceof CityBossLevel && matchesImpShopCache(level, options);
	}

	/** The depth-20 Imp shop cache, generated with the boss floor. */
	private static boolean matchesImpShopCache(Level level, Options options) throws Exception {
		Object shop = getField(level, "impShop");
		if (!(shop instanceof ImpShopRoom)) return false;
		Object items = getField(shop, "itemsToSpawn");
		return items instanceof Collection && matches((Collection<?>) items, options);
	}

	private static boolean matches(Collection<?> items, Options options) {
		if (items == null) return false;
		for (Object item : items) {
			if (matches(item, options)) return true;
		}
		return false;
	}

	/**
	 * The match itself: an item of the wanted class at the wanted upgrade.
	 * {@code trueLevel()} is the upgrade the item really carries — a cursed
	 * item's displayed level is lower — and reading it, unlike {@code name()}
	 * or {@code identify()}, does not mutate the item.
	 */
	private static boolean matches(Object candidate, Options options) {
		if (!(candidate instanceof Item)) return false;
		Item item = (Item) candidate;
		if (!isSearchable(item)) return false;
		if (!item.getClass().getSimpleName().equalsIgnoreCase(options.item)) return false;
		return options.upgrade < 0 || item.trueLevel() == options.upgrade;
	}

	/** The kinds Seed Seeker searches, so that both tools answer the same question. */
	private static boolean isSearchable(Item item) {
		return item instanceof Weapon || item instanceof Armor || item instanceof Wand
				|| item instanceof Ring;
	}

	private static Object getField(Object owner, String name) throws Exception {
		return findField(owner.getClass(), name).get(owner);
	}

	private static void setStaticField(Class<?> type, String name, Object value) throws Exception {
		findField(type, name).set(null, value);
	}

	private static Field findField(Class<?> type, String name) throws NoSuchFieldException {
		for (Class<?> current = type; current != null; current = current.getSuperclass()) {
			try {
				Field field = current.getDeclaredField(name);
				field.setAccessible(true);
				return field;
			} catch (NoSuchFieldException ignored) {
				// keep walking up
			}
		}
		throw new NoSuchFieldException(type.getName() + "." + name);
	}

	private static void printUsage() {
		System.out.println("Usage: java-finder [--item CLASS] [--upgrade N] [--floors N] "
				+ "[--seeds N] [--start N] [--warmup N] [--challenges N] [--no-vault]"
				+ " [--skip-boss-floors] [--print-matches]");
		System.out.println("  --item CLASS       Item class simple name (default: RunicBlade)");
		System.out.println("  --upgrade N        Required true upgrade, -1 for any (default: 5)");
		System.out.println("  --floors N         Deepest floor to generate (default: 19)");
		System.out.println("  --seeds N          Timed seeds (default: 2000)");
		System.out.println("  --start N          First numeric seed (default: 0)");
		System.out.println("  --warmup N         Untimed seeds searched first (default: 200)");
		System.out.println("  --challenges N     Challenge bit mask (default: 0)");
		System.out.println("  --no-vault         Do not build the Imp's Vault");
		System.out.println("  --skip-boss-floors Step over the state-neutral boss depths 5, 10, 15, 25");
		System.out.println("  --print-matches    Print each matching seed code before the BENCH line");
	}

	private static final class Options {
		String item = "RunicBlade";
		int upgrade = 5;
		int floors = LAST_IMP_DEPTH;
		long seeds = 2000;
		long start;
		long warmup = 200;
		int challenges;
		boolean vault = true;
		boolean skipBossFloors;
		boolean printMatches;
		boolean help;

		static Options parse(String[] args) {
			Options result = new Options();
			for (int i = 0; i < args.length; i++) {
				String arg = args[i];
				if ("--help".equals(arg) || "-h".equals(arg)) {
					result.help = true;
				} else if ("--item".equals(arg)) {
					result.item = requireValue(args, ++i, arg);
				} else if ("--upgrade".equals(arg)) {
					result.upgrade = Integer.parseInt(requireValue(args, ++i, arg));
				} else if ("--floors".equals(arg)) {
					result.floors = Integer.parseInt(requireValue(args, ++i, arg));
				} else if ("--seeds".equals(arg)) {
					result.seeds = Long.parseLong(requireValue(args, ++i, arg));
				} else if ("--start".equals(arg)) {
					result.start = Long.parseLong(requireValue(args, ++i, arg));
				} else if ("--warmup".equals(arg)) {
					result.warmup = Long.parseLong(requireValue(args, ++i, arg));
				} else if ("--challenges".equals(arg)) {
					result.challenges = Integer.parseInt(requireValue(args, ++i, arg));
				} else if ("--no-vault".equals(arg)) {
					result.vault = false;
				} else if ("--skip-boss-floors".equals(arg)) {
					result.skipBossFloors = true;
				} else if ("--print-matches".equals(arg)) {
					result.printMatches = true;
				} else {
					throw new IllegalArgumentException("unknown option '" + arg + "'");
				}
			}
			if (result.floors < 1 || result.floors > 26) {
				throw new IllegalArgumentException("--floors must be between 1 and 26");
			}
			if (result.seeds < 1) throw new IllegalArgumentException("--seeds must be positive");
			if (result.warmup < 0) throw new IllegalArgumentException("--warmup cannot be negative");
			return result;
		}

		private static String requireValue(String[] args, int index, String option) {
			if (index >= args.length) {
				throw new IllegalArgumentException(option + " requires a value");
			}
			return args[index];
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
