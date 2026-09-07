//! Artifact records from deterministic level generation, never runtime loot.
use shpd_seedfinder_core::catalog::{ItemId, ItemKind, item};
use shpd_seedfinder_core::feasibility::QueryPlan;
use shpd_seedfinder_core::generator::{ARTIFACT_ITEMS, GeneratedArtifact, GeneratedItem};
use shpd_seedfinder_core::json_query;
use shpd_seedfinder_core::main_world::CanonicalMainWorldGenerator;
use shpd_seedfinder_core::model::ItemSource;
use shpd_seedfinder_core::query::scout_matches;
use shpd_seedfinder_core::search::WorldGenerator;
use shpd_seedfinder_core::seed::DungeonSeed;

#[test]
fn all_spawn_artifacts_match_eight_official_beta4_worlds() {
    // ParityOracle 1-24; pinned official JAR SHA-256:
    // 76f6983e7b619267666621de9f1ecbbc3645d4925c2c446736987c3011b9dfd1
    let mut identities = std::collections::HashSet::new();
    for line in include_str!("fixtures/beta4-artifacts.txt").lines() {
        let (seed, expected) = line.split_once('|').unwrap();
        let seed = DungeonSeed::from_code(seed).unwrap();
        let world = CanonicalMainWorldGenerator.generate(seed, 24);
        let mut actual: Vec<_> = world
            .items
            .iter()
            .filter(|entry| item(entry.item).kind == ItemKind::Artifact)
            .map(|entry| {
                identities.insert(entry.item);
                // Stable JSON source spelling, shared with the oracle fixture.
                let source = match entry.source {
                    ItemSource::ImpReward => "imp_reward",
                    ItemSource::Shop => "shop",
                    ItemSource::Tomb => "tomb",
                    ItemSource::CrystalChest => "crystal_chest",
                    ItemSource::LockedChest => "locked_chest",
                    ItemSource::Chest => "chest",
                    ItemSource::Heap => "heap",
                    ItemSource::Skeleton => "skeleton",
                    ItemSource::Mimic => "mimic",
                    ItemSource::GoldenMimic => "golden_mimic",
                    ItemSource::CrystalMimic => "crystal_mimic",
                    other => panic!("unexpected artifact source {other:?}"),
                };
                format!(
                    "{},{},{},{},{}",
                    entry.depth,
                    source,
                    item(entry.item).stable_id,
                    entry.upgrade,
                    u8::from(entry.cursed)
                )
            })
            .collect();
        actual.sort();
        assert_eq!(actual.join(";"), expected, "seed {}", seed.to_code());
    }
    assert_eq!(identities.len(), 11);
}

#[test]
fn aaa_artifacts_match_beta4_oracle_and_round_trip_native_wire() {
    let world = CanonicalMainWorldGenerator.generate(DungeonSeed::MIN, 24);
    let actual: Vec<_> = world
        .items
        .iter()
        .filter(|entry| item(entry.item).kind == ItemKind::Artifact)
        .map(|entry| {
            (
                entry.item,
                entry.depth,
                entry.source,
                entry.upgrade,
                entry.cursed,
            )
        })
        .collect();
    // Official BETA-4 ParityOracle AAA-AAA-AAA 1-24. Sandals have
    // internal trueLevel 2; the search displays the transferUpgrade(5) amount.
    assert_eq!(
        actual,
        vec![
            (ItemId::UnstableSpellbook, 14, ItemSource::Chest, 0, true),
            (ItemId::SandalsOfNature, 19, ItemSource::ImpReward, 5, false),
            (
                ItemId::AlchemistsToolkit,
                21,
                ItemSource::CrystalChest,
                0,
                true
            ),
            (ItemId::SkeletonKey, 22, ItemSource::Chest, 0, true),
        ]
    );
    let packet = shpd_seedfinder_core::wire::encode_scout_world_with_trinkets(&world).unwrap();
    assert_eq!(
        shpd_seedfinder_core::wire::decode_scout_world(&packet).unwrap(),
        world
    );
}

#[test]
fn artifact_predicates_codecs_and_vault_exclusivity() {
    assert!(json_query::decode(r#"{"requirements":[{"kind":"artifact"}]}"#).is_err());
    let query = json_query::decode(r#"{"requirements":[{"item":"sandals_of_nature","upgrade":5,"uncursed":true,"source":"imp_reward","max_depth":19}]}"#).unwrap();
    let world = CanonicalMainWorldGenerator.generate(DungeonSeed::MIN, 24);
    assert_eq!(scout_matches(&world, &query).matched_requirements, 1);
    assert_eq!(QueryPlan::analyze(&query).generation_depth(), 19);
    assert_eq!(
        shpd_seedfinder_core::deep_link::decode(
            &shpd_seedfinder_core::deep_link::encode(&query).unwrap()
        )
        .unwrap(),
        query
    );
    let exported =
        shpd_seedfinder_core::results_export::encode(&query, &[DungeonSeed::MIN], "test");
    assert_eq!(
        shpd_seedfinder_core::results_export::decode(&exported)
            .unwrap()
            .query,
        query
    );
    let too_early = json_query::decode(
        r#"{"requirements":[{"item":"sandals_of_nature","upgrade":5,"max_depth":16}]}"#,
    )
    .unwrap();
    assert!(QueryPlan::analyze(&too_early).is_unsatisfiable());
    let exclusive = json_query::decode(r#"{"requirements":[{"item":"sandals_of_nature","source":"imp_reward"},{"item":"ring_haste","source":"imp_reward"}]}"#).unwrap();
    assert_eq!(scout_matches(&world, &exclusive).matched_requirements, 1);
    let alternatives = json_query::decode(r#"{"requirements":[{"any_of":[{"item":"sandals_of_nature"},{"item":"ethereal_chains"}]}]}"#).unwrap();
    assert_eq!(scout_matches(&world, &alternatives).matched_requirements, 1);
    assert!(shpd_seedfinder_core::probability::estimate_match_probability(&query).is_nan());
}

#[test]
fn artifacts_respect_floor_limits_and_scalar_gated_search_agree() {
    for value in 0..32 {
        let seed = DungeonSeed::new(value).unwrap();
        let world = CanonicalMainWorldGenerator.generate(seed, 24);
        let artifacts: Vec<_> = world
            .items
            .iter()
            .filter(|entry| item(entry.item).kind == ItemKind::Artifact)
            .collect();
        let mut identities = std::collections::HashSet::new();
        for artifact in artifacts {
            assert!(
                identities.insert(artifact.item),
                "unique artifact deck: seed {value}"
            );
            assert_eq!(
                artifact.upgrade,
                if artifact.source == ItemSource::ImpReward {
                    5
                } else {
                    0
                }
            );
            let query = json_query::decode(&format!(
                r#"{{"requirements":[{{"item":"{}","max_depth":{}}}]}}"#,
                item(artifact.item).stable_id,
                artifact.depth
            ))
            .unwrap();
            let plan = QueryPlan::analyze(&query);
            assert!(!plan.is_unsatisfiable());
            let generated = CanonicalMainWorldGenerator.generate_batch_gated(
                &[seed],
                plan.generation_depth(),
                &plan,
            );
            assert_eq!(
                scout_matches(generated[0].as_ref().unwrap(), &query).matched_requirements,
                1
            );
            if artifact.depth > 1 {
                let mut early = query.clone();
                early.requirements[0].max_depth = Some(artifact.depth - 1);
                assert_eq!(scout_matches(&world, &early).matched_requirements, 0);
            }
        }
    }
}

#[test]
fn artifact_projection_is_read_only_and_covers_spawn_deck() {
    let mut count = 0;
    for kind in ARTIFACT_ITEMS {
        let generated = GeneratedItem::Artifact(GeneratedArtifact {
            kind,
            cursed: true,
            spellbook_scrolls: None,
        });
        let before = generated;
        if let Some(projected) = generated.searchable_equipment() {
            count += 1;
            assert_eq!(item(projected.item).kind, ItemKind::Artifact);
            assert_eq!(projected.roll.upgrade, 0);
            assert!(projected.roll.cursed);
        }
        assert_eq!(before, generated);
    }
    assert_eq!(count, 11); // Cloak and Holy Tome have zero spawn weight.
}
