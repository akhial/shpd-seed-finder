//! Held-out calibration checks for selected-trinket probability estimates.
use shpd_seedfinder_core::catalog::item_by_stable_id;
use shpd_seedfinder_core::challenges::Challenges;
use shpd_seedfinder_core::json_query;
use shpd_seedfinder_core::main_world::generate_main_world_with_trinket;
use shpd_seedfinder_core::probability::estimate_match_probability;
use shpd_seedfinder_core::seed::{DungeonSeed, TOTAL_SEEDS};
use shpd_seedfinder_core::trinkets::trinket_order;

#[test]
#[ignore = "held-out calibration; run with --release --ignored --nocapture"]
#[allow(clippy::cast_precision_loss)]
fn selected_estimates_track_generated_matches() {
    const WORLDS: u64 = 8192;
    let cases = [
        (
            "mimic_tooth",
            r#"{"kind":"weapon","source":"mimic","upgrade":1}"#,
        ),
        (
            "parchment_scrap",
            r#"{"kind":"weapon","effect":"any_enchantment","source":"ghost_reward"}"#,
        ),
        ("cracked_spyglass", r#"{"kind":"ring","source":"heap"}"#),
        ("rat_skull", r#"{"kind":"armor","upgrade":2}"#),
        ("exotic_crystals", r#"{"kind":"wand","upgrade":2}"#),
        ("mossy_clump", r#"{"kind":"ring","upgrade":2}"#),
        ("trap_mechanism", r#"{"kind":"armor","upgrade":2}"#),
    ];
    std::thread::scope(|scope| {
        for (name, equipment) in cases {
            scope.spawn(move || {
                let id = item_by_stable_id(name).unwrap().id;
                let query = json_query::decode(&format!(
                    r#"{{"requirements":[{{"item":"{name}","select_trinket":true}},{equipment}],"max_depth":24}}"#
                )).unwrap();
                let estimate = estimate_match_probability(&query);
                let mut plain = query.clone();
                plain.requirements[0].select_trinket = false;
                let old = estimate_match_probability(&plain);
                let mut hits = 0u64;
                // Offset from the evenly spaced training grid; no training seeds.
                for index in 0..WORLDS {
                    let seed = DungeonSeed::new((index * (TOTAL_SEEDS / WORLDS) + 123_457) % TOTAL_SEEDS).unwrap();
                    if !trinket_order(seed)[..4].contains(&id) { continue; }
                    let world = generate_main_world_with_trinket(seed, 24, Challenges::NONE, Some(id)).unwrap();
                    hits += u64::from(query.matches(&world));
                }
                let observed = hits as f64 / WORLDS as f64;
                eprintln!("{name}: selected={estimate:.5}, unselected={old:.5}, observed={observed:.5}");
                let noise = 3.0 * (observed * (1.0 - observed) / WORLDS as f64).sqrt();
                assert!((estimate - observed).abs() <= 0.35 * observed + noise,
                    "{name}: estimate {estimate}, observed {observed}");
            });
        }
    });
}
