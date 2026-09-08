//! Searchable loot captured from the official v4.0.0-BETA-4 Java oracle.
//! Seven generation-changing trinkets, each at +3 after the first brewing
//! opportunity, across three seeds through floor 24 (including vault rewards).
use serde::Deserialize;
use shpd_seedfinder_core::catalog::{ItemKind, item, item_by_stable_id};
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::main_world::generate_main_world_with_trinket;

#[derive(Deserialize)]
struct Case {
    seed: String,
    trinket: String,
    items: Vec<(u8, String, u8, bool, String)>,
}

#[test]
fn selected_trinket_loot_matches_beta4_java() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/selected_trinkets_beta4.json")).unwrap();
    assert_eq!(cases.len(), 21);
    for mut case in cases {
        let world = generate_main_world_with_trinket(
            shpd_seedfinder_core::seed::DungeonSeed::from_code(&case.seed).unwrap(),
            24,
            Challenges::NONE,
            Some(item_by_stable_id(&case.trinket).unwrap().id),
        )
        .unwrap();
        let mut actual: Vec<_> = world
            .items
            .iter()
            .filter(|entry| item(entry.item).kind != ItemKind::Trinket)
            .map(|entry| {
                (
                    entry.depth,
                    item(entry.item).stable_id.to_owned(),
                    entry.upgrade,
                    entry.cursed,
                    entry
                        .effect
                        .map_or("-", |effect| effect.wire_name())
                        .to_owned(),
                )
            })
            .collect();
        actual.sort();
        case.items.sort();
        assert_eq!(actual, case.items, "{} {}", case.seed, case.trinket);
    }
}
