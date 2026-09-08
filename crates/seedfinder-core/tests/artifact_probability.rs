//! Artifact estimates are checked against independently sampled full worlds.
use shpd_seedfinder_core::{
    challenges::Challenges,
    json_query,
    main_world::CanonicalMainWorldGenerator,
    probability::estimate_match_probability,
    search::WorldGenerator,
    seed::{DungeonSeed, TOTAL_SEEDS},
};

fn probability(document: &str) -> f64 {
    estimate_match_probability(&json_query::decode(document).unwrap())
}

#[test]
fn artifact_constraints_and_shared_rewards_have_finite_estimates() {
    let any = probability(r#"{"requirements":[{"item":"ethereal_chains"}]}"#);
    let early = probability(r#"{"requirements":[{"item":"ethereal_chains","max_depth":4}]}"#);
    let clean = probability(r#"{"requirements":[{"item":"ethereal_chains","uncursed":true}]}"#);
    let vault = probability(r#"{"requirements":[{"item":"ethereal_chains","upgrade":5}]}"#);
    let shop = probability(r#"{"requirements":[{"item":"ethereal_chains","source":"shop"}]}"#);
    for estimate in [any, early, clean, vault, shop] {
        assert!(
            estimate.is_finite() && estimate > 0.0 && estimate < 1.0,
            "{estimate}"
        );
    }
    assert!(early < any && clean < any && vault < any && shop < any);
    for impossible in [
        r#"{"requirements":[{"item":"ethereal_chains"},{"item":"ethereal_chains"}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains","upgrade":{"exact":1}}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains","upgrade":5,"max_depth":16}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains","upgrade":5},{"item":"horn_of_plenty","upgrade":5}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains","upgrade":5},{"kind":"ring","upgrade":4}]}"#,
    ] {
        assert!(probability(impossible) <= 0.0, "{impossible}");
    }
    let combined =
        probability(r#"{"requirements":[{"item":"ethereal_chains"},{"item":"horn_of_plenty"}]}"#);
    assert!(combined > 0.0 && combined < any);
    let trinket =
        probability(r#"{"requirements":[{"item":"ethereal_chains"},{"item":"rat_skull"}]}"#);
    assert!((trinket / any - 4.0 / 17.0).abs() < 1e-10);
    let alternative = probability(
        r#"{"requirements":[{"any_of":[{"item":"ethereal_chains"},{"item":"horn_of_plenty"}]}]}"#,
    );
    assert!(alternative >= any);
}

#[test]
fn measured_estimates_track_an_independent_seed_sample() {
    const WORLDS: u32 = 768;
    let documents = [
        r#"{"requirements":[{"item":"ethereal_chains"}],"max_depth":4}"#,
        r#"{"requirements":[{"item":"ethereal_chains"}],"max_depth":9}"#,
        r#"{"requirements":[{"item":"ethereal_chains"}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains","uncursed":true}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains","upgrade":5}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains"},{"item":"horn_of_plenty"}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains"},{"item":"horn_of_plenty"},{"item":"dried_rose"}]}"#,
        r#"{"requirements":[{"item":"ethereal_chains"},{"kind":"wand","upgrade":2}]}"#,
    ];
    let queries: Vec<_> = documents
        .iter()
        .map(|document| json_query::decode(document).unwrap())
        .collect();
    let generator = CanonicalMainWorldGenerator::with_challenges(Challenges::NONE);
    let counts = std::thread::scope(|scope| {
        let workers = 8;
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let queries = &queries;
                let generator = &generator;
                scope.spawn(move || {
                    let mut counts = vec![0_u32; queries.len()];
                    for index in (worker..WORLDS).step_by(workers as usize) {
                        // Offset and stride differ from calibration's evenly spaced seeds.
                        let seed = DungeonSeed::new(
                            (u64::from(index) * 7_919_333_117 + 1_234_567) % TOTAL_SEEDS,
                        )
                        .unwrap();
                        let world = generator.generate(seed, 24);
                        for (query, count) in queries.iter().zip(&mut counts) {
                            *count += u32::from(query.matches(&world));
                        }
                    }
                    counts
                })
            })
            .collect();
        let mut counts = vec![0_u32; queries.len()];
        for handle in handles {
            for (total, count) in counts.iter_mut().zip(handle.join().unwrap()) {
                *total += count;
            }
        }
        counts
    });
    for ((document, query), count) in documents.iter().zip(&queries).zip(counts) {
        let observed = f64::from(count) / f64::from(WORLDS);
        let estimated = estimate_match_probability(query);
        // Allow the existing matching approximation plus independent sampling noise.
        let tolerance =
            0.2 * observed + 3.0 * (observed * (1.0 - observed) / f64::from(WORLDS)).sqrt();
        assert!(
            (estimated - observed).abs() <= tolerance,
            "{document}: observed {observed:.4}, estimated {estimated:.4}, tolerance {tolerance:.4}"
        );
    }
}
