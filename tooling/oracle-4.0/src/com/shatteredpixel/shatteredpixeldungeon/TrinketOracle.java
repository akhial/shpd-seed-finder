package com.shatteredpixel.shatteredpixeldungeon;

import com.shatteredpixel.shatteredpixeldungeon.items.Generator;
import com.shatteredpixel.shatteredpixeldungeon.items.Item;

/** Prints the private trinket deck after the normal oracle initializes a run.
 * Floor generation must not consume this deck in the canonical profile.
 * Usage: TrinketOracle AAA-AAA-AAA [maximum-floor]
 */
public final class TrinketOracle {
    public static void main(String[] args) {
        ParityOracle.main(new String[] {args[0], args.length > 1 ? args[1] : "1"});
        for (int i = 0; i < 17; i++) {
            Item item = Generator.random(Generator.Category.TRINKET);
            System.out.println("trinket_order " + (i + 1) + " "
                    + item.getClass().getSimpleName() + " " + item.image);
        }
    }
}
