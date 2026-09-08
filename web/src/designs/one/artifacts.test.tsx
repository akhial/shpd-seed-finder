import { readFile } from "node:fs/promises";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeAll, describe, expect, it } from "vite-plus/test";
import { itemsForKind } from "../../lib/catalog";
import { fromQueryJson, maxUpgradeOf, toQueryDocument, validateRequirement } from "../../lib/query";
import init, { analyze_query, filter_seeds, scout } from "../../lib/wasm/pkg/seedfinder.js";
import type { ScoutResult } from "../../lib/wasm/types";
import { RequirementEditor, namedItemEditorRequirement } from "./RequirementEditor";
import { ScoutPanel } from "./ScoutPanel";
import { boardItems, canStack, joinAlternatives } from "./relations";

beforeAll(async () => {
  await init({
    module_or_path: await readFile(
      new URL("../../lib/wasm/pkg/seedfinder_bg.wasm", import.meta.url),
    ),
  });
});

describe("artifact search and scout", () => {
  it("requires a named artifact and exposes floor limits", () => {
    const wildcard = fromQueryJson('{"requirements":[{"kind":"artifact"}]}').requirements[0];
    expect(validateRequirement(wildcard)).toContain("Select an artifact.");
    expect(JSON.parse(analyze_query('{"requirements":[{"kind":"artifact"}]}')).valid).toBe(false);
    expect(itemsForKind("artifact")).toHaveLength(11);
    const requirement = fromQueryJson(
      '{"requirements":[{"item":"sandals_of_nature","upgrade":5,"max_depth":19}]}',
    ).requirements[0];
    expect(maxUpgradeOf(requirement)).toBe(5);
    const html = renderToStaticMarkup(
      <RequirementEditor
        requirement={requirement}
        isNew={false}
        stack={{ count: 1, inCluster: false }}
        onSave={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(html).not.toContain("Any artifact");
    expect(html).not.toContain("Total item count");
    expect(html).not.toContain("Upgrade level");
    expect(namedItemEditorRequirement(requirement).upgrade).toEqual({ mode: "any", value: 0 });
    expect(html).toContain("Limit this item");
    expect(html).toContain('aria-valuetext="19"');
    const repeats = fromQueryJson(
      '{"requirements":[{"item":"ethereal_chains"},{"item":"ethereal_chains","max_depth":14}]}',
    ).requirements;
    expect(boardItems(repeats)).toHaveLength(2);
    const alternatives = joinAlternatives(repeats, 0, 1);
    expect(boardItems(alternatives)).toHaveLength(1);
    expect(
      alternatives.every((r) => r.item === "ethereal_chains" && r.identityGroup === undefined),
    ).toBe(true);
  });

  it("searches artifact OR groups and preserves individual floor limits", () => {
    const state = fromQueryJson(
      '{"requirements":[{"item":"unstable_spellbook","max_depth":14},{"item":"ethereal_chains","max_depth":4}]}',
    );
    state.requirements = joinAlternatives(state.requirements, 1, 0);
    const document = toQueryDocument(state);
    expect(canStack(state.requirements, boardItems(state.requirements)[0])).toBe(false);
    expect(
      fromQueryJson(JSON.stringify(document))
        .requirements.map((r) => r.maxDepth)
        .sort((a, b) => (a ?? 0) - (b ?? 0)),
    ).toEqual([4, 14]);
    expect(JSON.parse(filter_seeds(JSON.stringify(document), new Float64Array([0])))).toHaveLength(
      1,
    );
    expect(
      JSON.parse(
        filter_seeds(
          '{"requirements":[{"item":"unstable_spellbook","max_depth":13}]}',
          new Float64Array([0]),
        ),
      ),
    ).toHaveLength(0);
  });

  it("renders deterministic artifact records with +5 and vault choice metadata", () => {
    const query = { requirements: [{ item: "sandals_of_nature", upgrade: 5, max_depth: 19 }] };
    const result = JSON.parse(scout(JSON.stringify({ seed: "AAA-AAA-AAA", query }))) as ScoutResult;
    expect(result.matchedRequirements).toBe(1);
    const artifact = result.items.find((entry) => entry.id === "sandals_of_nature");
    expect(artifact).toMatchObject({
      category: "artifact",
      upgrade: 5,
      depth: 19,
      source: "imp_reward",
      matched: true,
      cursed: false,
    });
    expect(artifact?.accessibility.type).toBe("choice");
    for (const entry of result.items.filter((entry) => entry.category === "artifact")) {
      expect(itemsForKind("artifact").find((item) => item.id === entry.id)).toMatchObject({
        name: entry.name,
        sprite: entry.spriteIndex,
      });
    }
    const html = renderToStaticMarkup(
      <ScoutPanel
        input="AAA-AAA-AAA"
        onInput={() => {}}
        onScout={() => {}}
        loading={false}
        result={result}
      />,
    );
    expect(html).toContain("Sandals of Nature");
    expect(html).toContain(">+5</b>");
    expect(html).toContain("One reward of choice group");
    const analysis = JSON.parse(analyze_query(JSON.stringify(query)));
    expect(analysis).toMatchObject({ valid: true, impossible: false });
    expect(analysis.probability).toBeGreaterThan(0);
    expect(analysis.probability).toBeLessThan(1);
  });
});
