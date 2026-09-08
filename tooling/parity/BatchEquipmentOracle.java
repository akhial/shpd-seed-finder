package com.shatteredpixel.shatteredpixeldungeon;

import com.shatteredpixel.shatteredpixeldungeon.actors.blobs.SacrificialFire;
import com.shatteredpixel.shatteredpixeldungeon.actors.hero.HeroClass;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.*;
import com.shatteredpixel.shatteredpixeldungeon.actors.mobs.npcs.*;
import com.shatteredpixel.shatteredpixeldungeon.items.*;
import com.shatteredpixel.shatteredpixeldungeon.items.armor.Armor;
import com.shatteredpixel.shatteredpixeldungeon.items.rings.Ring;
import com.shatteredpixel.shatteredpixeldungeon.items.wands.Wand;
import com.shatteredpixel.shatteredpixeldungeon.items.weapon.Weapon;
import com.shatteredpixel.shatteredpixeldungeon.levels.*;
import com.shatteredpixel.shatteredpixeldungeon.levels.rooms.Room;
import com.shatteredpixel.shatteredpixeldungeon.utils.DungeonSeed;
import java.io.*;
import java.lang.reflect.Field;
import java.util.*;

/** Exact equipment multisets, one seed per line; no search early exits or hashes. */
public final class BatchEquipmentOracle {
    static final Map<String, Field> FIELDS = new HashMap<>();
    static final Map<Class<?>, String> IDS = new HashMap<>();
    static final ArrayList<String> ITEMS = new ArrayList<>();
    static final ArrayList<String> MAPS = new ArrayList<>();
    static final ArrayList<String> LOCATIONS = new ArrayList<>();
    static final ArrayList<String> CATALYSTS = new ArrayList<>();
    static int cell = -1;
    static int branch;
    static int depth;
    static Object get(Object object, String name) throws Exception {
        Class<?> type = object instanceof Class ? (Class<?>) object : object.getClass();
        String key = type.getName() + ":" + name;
        Field f = FIELDS.get(key);
        if (f == null) {
            Class<?> current = type;
            while (current != null) {
                try { f = current.getDeclaredField(name); f.setAccessible(true); break; }
                catch (NoSuchFieldException e) { current = current.getSuperclass(); }
            }
            if (f == null) throw new NoSuchFieldException(key);
            FIELDS.put(key, f);
        }
        return f.get(object instanceof Class ? null : object);
    }
    static void set(Class<?> type, String name, Object value) throws Exception {
        get(type, name); FIELDS.get(type.getName()+":"+name).set(null,value);
    }
    static void add(Object object, String source) {
        if (!(object instanceof Item)) return;
        Item item = (Item) object;
        if (item instanceof com.shatteredpixel.shatteredpixeldungeon.items.trinkets.TrinketCatalyst) {
            CATALYSTS.add(depth+","+source);
        }
        if (!(item instanceof Weapon || item instanceof Armor || item instanceof Wand || item instanceof Ring || item instanceof com.shatteredpixel.shatteredpixeldungeon.items.trinkets.Trinket)) return;
        String id = IDS.computeIfAbsent(item.getClass(), c -> c.getSimpleName()
            .replaceAll("(?<!^)([A-Z])", "_$1").toLowerCase(Locale.ROOT)
            .replace("wand_of_", "wand_").replace("ring_of_", "ring_"));
        if (id.equals("dart")) return; // plain Dart is not in Seed Seeker's catalog
        Object effect = item instanceof Weapon ? ((Weapon)item).enchantment : item instanceof Armor ? ((Armor)item).glyph : null;
        if (source.equals("GhostReward")) effect = item instanceof Weapon ? Ghost.Quest.enchant : Ghost.Quest.glyph;
        if (source.equals("BlacksmithReward")) effect = item instanceof Weapon ? Blacksmith.Quest.smithEnchant : item instanceof Armor ? Blacksmith.Quest.smithGlyph : null;
        String e = effect == null ? "-" : effect.getClass().getSimpleName();
        if (e.equals("AntiMagic")) e="Anti-Magic";
        if (e.equals("AntiEntropy")) e="Anti-Entropy";
        ITEMS.add(depth+","+source+","+id+","+item.trueLevel()+","+(item.cursed?1:0)+","+e);
        if (cell >= 0) LOCATIONS.add(depth+","+branch+","+cell+","+id+","+item.trueLevel()+","+(item.cursed?1:0)+","+e);
    }
    static void addAll(Object values, String source) {
        if (values instanceof Collection<?>) for (Object i : (Collection<?>)values) add(i,source);
    }
    static void scan(Level level, boolean vault) throws Exception {
        branch = vault ? 1 : 0;
        if (!(level instanceof CityBossLevel)) {
            StringJoiner cells = new StringJoiner(",");
            for (int tile : level.map) cells.add(Integer.toString(tile));
            MAPS.add("map "+(depth+(vault?100:0))+" "+level.width()+","+level.height()+":"+cells);
        }
        for (Heap h : level.heaps.valueList()) {
            Room r = level instanceof RegularLevel ? ((RegularLevel)level).room(h.pos) : null;
            if (vault && r != null && r.getClass().getSimpleName().equals("VaultFinalRoom")) continue;
            String source = vault ? "VaultTreasure" : switch(h.type) {
                case HEAP -> "Heap"; case CHEST -> "Chest"; case LOCKED_CHEST -> "LockedChest";
                case CRYSTAL_CHEST -> "CrystalChest"; case TOMB -> "Tomb";
                case SKELETON -> "Skeleton"; case FOR_SALE -> "Shop";
                default -> throw new IllegalStateException("unsupported heap "+h.type);
            };
            cell = h.pos; addAll(h.items,source); cell = -1;
        }
        for (Mob m : level.mobs) {
            cell = m.pos;
            if (m instanceof Mimic) addAll(((Mimic)m).items,vault?"VaultTreasure":m instanceof GoldenMimic?"GoldenMimic":m instanceof CrystalMimic?"CrystalMimic":"Mimic");
            else if (m instanceof Statue) {
                String source = vault?"VaultTreasure":m instanceof ArmoredStatue?"ArmoredStatue":"Statue";
                add(((Statue)m).weapon(),source);
                if (m instanceof ArmoredStatue) add(((ArmoredStatue)m).armor(),source);
            }
        }
        cell = -1;
        SacrificialFire fire=(SacrificialFire)level.blobs.get(SacrificialFire.class);
        if (fire != null) add(get(fire,"prize"),"SacrificialFire");
        if (level instanceof CityBossLevel) addAll(get(get(level,"impShop"),"itemsToSpawn"),"Shop");
    }
    static void generate(long seed) throws Exception {
        ITEMS.clear();
        MAPS.clear();
        LOCATIONS.clear(); CATALYSTS.clear(); cell = -1;
        SPDSettings.customSeed(DungeonSeed.convertToCode(seed)); Dungeon.initSeed();
        GamesInProgress.selectedClass=HeroClass.WARRIOR; Dungeon.init();
        Imp.Quest.rewardOptions.clear(); set(Imp.Quest.class,"oldQuest",false); set(Imp.Quest.class,"alternative",false);
        boolean ghost=false,wandmaker=false,smith=false; int imp=-1;
        for (depth=1;depth<=24;depth++) {
            if (depth==5 || depth==10 || depth==15) { Dungeon.depth++; continue; }
            scan(Dungeon.newLevel(),false);
            if (!ghost && Ghost.Quest.weapon!=null && Ghost.Quest.armor!=null) { ghost=true;add(Ghost.Quest.weapon,"GhostReward");add(Ghost.Quest.armor,"GhostReward"); }
            if (!wandmaker && Wandmaker.Quest.wand1!=null && Wandmaker.Quest.wand2!=null) { wandmaker=true;add(Wandmaker.Quest.wand1,"WandmakerReward");add(Wandmaker.Quest.wand2,"WandmakerReward"); }
            if (!smith && Blacksmith.Quest.smithRewards!=null) { smith=true;addAll(Blacksmith.Quest.smithRewards,"BlacksmithReward"); }
            if (imp<0 && !Imp.Quest.rewardOptions.isEmpty()) { imp=depth;addAll(Imp.Quest.rewardOptions,"ImpReward"); }
            Dungeon.depth++;
        }
        if (imp>0) { depth=imp;Dungeon.depth=imp;Dungeon.branch=1;scan(Dungeon.newLevel(),true); }
        for (String catalyst : CATALYSTS) {
            String[] parts = catalyst.split(","); depth = Integer.parseInt(parts[0]);
            for (int i=0; i<4; i++) add(Generator.random(Generator.Category.TRINKET), parts[1]);
        }
        Collections.sort(ITEMS);
        Collections.sort(LOCATIONS);
    }
    public static void main(String[] args) throws Exception {
        long start=Long.parseLong(args[0]),count=Long.parseLong(args[1]);
        JarSeedFinder.main(new String[]{"--seeds","1","--warmup","0","--floors","1","--no-vault"});
        PrintWriter out=new PrintWriter(new BufferedWriter(new OutputStreamWriter(System.out),65536));
        long began=System.nanoTime();
        for(long seed=start;seed<start+count;seed++) {
            try { generate(seed); out.println(seed+"|"+String.join(";",ITEMS)+"|"+String.join(";",MAPS)+"|"+String.join(";",LOCATIONS)); }
            catch(Throwable e) { out.println("ERROR|"+seed+"|"+e.toString().replace('\n',' ')); e.printStackTrace(System.err); }
            if ((seed-start+1)%1000==0) { out.flush();System.err.printf(Locale.ROOT,"PROGRESS tested=%d elapsed=%.3f%n",seed-start+1,(System.nanoTime()-began)/1e9); }
        }
        out.flush();
        System.err.printf(Locale.ROOT,"DONE tested=%d elapsed=%.3f%n",count,(System.nanoTime()-began)/1e9);
    }
}

