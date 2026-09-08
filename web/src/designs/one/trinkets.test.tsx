import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vite-plus/test";
import { itemsForKind } from "../../lib/catalog";
import {
  fromQueryJson,
  toQueryDocument,
  validateQuery,
  validateRequirement,
} from "../../lib/query";
import type { ScoutItem } from "../../lib/wasm/types";
import { CatalystEntry } from "./ScoutPanel";
import { boardItems, canStack, joinAlternatives } from "./relations";
import { RequirementEditor, trinketEditorRequirement } from "./RequirementEditor";
import { requirementTitle } from "./summary";

describe("offered trinket pilot", () => {
  it("persists choosing a trinket through grouped queries and shows only four scout overrides", () => {
    const query = fromQueryJson(
      '{"requirements":[{"any_of":[{"item":"mimic_tooth","select_trinket":true},{"item":"rat_skull"}]}]}',
    );
    expect(query.requirements[0].selectTrinket).toBe(true);
    expect(fromQueryJson(JSON.stringify(toQueryDocument(query)))).toEqual(query);
    const order = itemsForKind("trinket").map((i) => ({
      id: i.id,
      name: i.name,
      spriteIndex: i.sprite,
    }));
    const offers: ScoutItem[] = order.slice(0, 4).map((i) => ({
      ...i,
      category: "trinket",
      depth: 2,
      source: "heap",
      upgrade: 0,
      cursed: false,
      secret: false,
      effect: null,
      accessibility: { type: "independent" },
      matched: false,
    }));
    const html = renderToStaticMarkup(
      <CatalystEntry
        offers={offers}
        order={order}
        selectedTrinket={order[1].id}
        onSelect={() => {}}
      />,
    );
    expect(html).not.toContain("<select");
    expect(html.match(/<button /g)).toHaveLength(4);
    expect(html).not.toContain("No Trinket");
    expect(html).not.toContain("Click a trinket");
    expect(html).toContain(`aria-label="Apply ${order[1].name} at +3" aria-pressed="true"`);
    expect(html).toContain("Applied +3");
    expect(html).not.toContain(`aria-label="Apply ${order[4].name}`);
    const cleared = renderToStaticMarkup(
      <CatalystEntry offers={offers} order={order} onSelect={() => {}} disabled />,
    );
    expect(cleared).not.toContain('aria-pressed="true"');
    expect(cleared.match(/disabled=""/g)).toHaveLength(4);
    expect(cleared).not.toContain("Applied +3");
  });

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
    expect(html).not.toContain("Initial choices when");
    expect(html).not.toContain("not initial offers");
    expect(html).not.toContain("d1-badge-match");
    expect(html).not.toContain("d1-trinket-number");
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
    expect(tail).not.toMatch(/>\d+<\//);
    for (const trinket of itemsForKind("trinket")) {
      expect(trinket.name).toBe(trinket.name.replace(/\b\w/g, (letter) => letter.toUpperCase()));
    }
  });

  it("requires a named trinket and shows no details or wildcard controls", () => {
    const legacy = fromQueryJson(
      '{"requirements":[{"kind":"trinket","source":"locked_chest","max_depth":2}]}',
    ).requirements[0];
    expect(validateRequirement(legacy)).toContain("Select a trinket.");
    expect(requirementTitle(legacy)).toBe("Trinket");
    const draft = trinketEditorRequirement(legacy);
    expect(draft.item).toBe("rat_skull");
    expect(draft.source).toBeUndefined();
    expect(draft.maxDepth).toBeUndefined();
    expect(requirementTitle(draft)).toBe("Rat Skull");
    const html = renderToStaticMarkup(
      <RequirementEditor
        requirement={legacy}
        isNew={false}
        stack={{ count: 1, inCluster: false }}
        onSave={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(html).toContain("Choose matching trinket at +3");
    expect(html).not.toContain("Any trinket");
    expect(html).not.toContain('value=""');
    expect(html).not.toContain("Details");
    expect(html).not.toContain("Source");
    expect(html).not.toContain("Limit this item");
    expect(html.match(/<option /g)).toHaveLength(17);
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
