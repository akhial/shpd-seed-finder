import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vite-plus/test";
import { itemsForKind } from "../../lib/catalog";
import { fromQueryJson, toQueryDocument, validateQuery } from "../../lib/query";
import type { ScoutItem } from "../../lib/wasm/types";
import { CatalystEntry } from "./ScoutPanel";
import { boardItems, canStack, joinAlternatives } from "./relations";

describe("offered trinket pilot", () => {
  it("preserves all 17 identities, draws four choices in deck order, and highlights the match", () => {
    const order = itemsForKind("trinket")
      .map((item) => ({
        id: item.id,
        name: item.name,
        spriteIndex: item.sprite,
      }))
      .reverse();
    const offers: ScoutItem[] = order
      .slice(0, 4)
      .reverse()
      .map((entry) => ({
        ...entry,
        category: "trinket",
        depth: 2,
        source: "locked_chest",
        upgrade: 0,
        cursed: false,
        secret: false,
        effect: null,
        accessibility: { type: "independent" },
        matched: entry.id === order[1].id,
      }));
    const html = renderToStaticMarkup(<CatalystEntry offers={offers} order={order} />);
    expect(order).toHaveLength(17);
    expect(html).toContain("Magical catalyst");
    expect(html.match(/class="d1-trinket-choice(?: d1-trinket-match)?"/g)).toHaveLength(4);
    expect(html.match(/class="d1-trinket-choice d1-trinket-match"/g)).toHaveLength(1);
    for (let index = 1; index < 4; index++) {
      expect(html.indexOf(order[index - 1].name)).toBeLessThan(html.indexOf(order[index].name));
    }
    const tail = html.slice(html.indexOf('class="d1-trinket-tail"'));
    expect(tail.match(/<li /g)).toHaveLength(13);
    for (let index = 5; index < 17; index++) {
      expect(tail.indexOf(order[index - 1].name)).toBeLessThan(tail.indexOf(order[index].name));
    }
    expect(html).toContain("width:48px");
    expect(tail).toContain("width:24px");
    expect(tail).toContain("image-rendering:pixelated");
  });

  it("joins named trinkets into an OR group and persists them as offered predicates", () => {
    const state = fromQueryJson('{"requirements":[{"item":"mimic_tooth"},{"item":"rat_skull"}]}');
    state.requirements = joinAlternatives(state.requirements, 1, 0);
    expect(validateQuery(state).valid).toBe(true);
    const doc = toQueryDocument(state);
    expect(doc.requirements).toHaveLength(1);
    expect(doc.requirements[0]).toHaveProperty("any_of");
    expect(canStack(state.requirements, boardItems(state.requirements)[0])).toBe(false);
    expect(
      fromQueryJson(JSON.stringify(doc))
        .requirements.map((r) => r.item)
        .sort((left, right) => (left ?? "").localeCompare(right ?? "")),
    ).toEqual(["mimic_tooth", "rat_skull"]);
  });
});
