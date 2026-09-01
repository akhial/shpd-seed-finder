import { describe, expect, it } from "vite-plus/test";
import { effectGlows, itemGlow } from "./glow";

const enchanted = (name: string) => ({
  cursed: false,
  effect: { name, kind: "enchantment" as const },
});

describe("itemGlow", () => {
  it("pulses the upstream colour of the enchantments v4.0.0 added", () => {
    expect(itemGlow(enchanted("Venomous"))?.color).toBe("#4400aa");
    expect(itemGlow(enchanted("Eldritch"))?.color).toBe("#222222");
    expect(itemGlow(enchanted("Vorpal"))?.color).toBe("#aa6666");
    expect(itemGlow(enchanted("Crystal"))?.color).toBe("#0088ff");
    // The anchor from the block these follow, unchanged by v4.0.0.
    expect(itemGlow(enchanted("Blazing"))?.color).toBe("#ff4400");
  });

  it("glows black for the curses v4.0.0 added, and not at all for plain items", () => {
    expect(itemGlow({ cursed: true, effect: { name: "Pressurized", kind: "curse" } })?.color).toBe(
      "#000000",
    );
    expect(itemGlow({ cursed: true, effect: { name: "Wondrous", kind: "curse" } })?.color).toBe(
      "#000000",
    );
    expect(itemGlow({ cursed: false, effect: null })).toBeNull();
  });
});

describe("effectGlows", () => {
  it("gives one glow per named effect, in the order the filter names them", () => {
    expect(effectGlows(["Crystal", "Venomous"]).map((glow) => glow.color)).toEqual([
      "#0088ff",
      "#4400aa",
    ]);
    // An unknown name is a curse, which always glows black.
    expect(effectGlows(["Wondrous"]).map((glow) => glow.color)).toEqual(["#000000"]);
  });
});
