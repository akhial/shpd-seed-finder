//! Writes anonymous deterministic artifact placements for the probability model.
//! `cargo run --release --example calibrate_artifacts > artifact_worlds.bin`
use shpd_seedfinder_core::{
    catalog::{ItemKind, item},
    challenges::Challenges,
    main_world::CanonicalMainWorldGenerator,
    probability_tables::source_index,
    search::WorldGenerator,
    seed::{DungeonSeed, TOTAL_SEEDS},
};
use std::collections::BTreeMap;
use std::io::{self, Write};

fn main() {
    const WORLDS: u64 = 4096;
    let generator = CanonicalMainWorldGenerator::with_challenges(Challenges::NONE);
    let mut output = Vec::new();
    for index in 0..WORLDS {
        let seed = DungeonSeed::new(index * (TOTAL_SEEDS / WORLDS)).unwrap();
        let world = generator.generate(seed, 24);
        let artifacts: Vec<_> = world
            .items
            .iter()
            .filter(|entry| item(entry.item).kind == ItemKind::Artifact)
            .collect();
        output.push(u8::try_from(artifacts.len()).unwrap());
        let mut groups = BTreeMap::new();
        for entry in artifacts {
            let (group, mask) =
                entry
                    .accessibility
                    .scenario_constraint()
                    .map_or((0, 0), |(group, mask)| {
                        let next = u8::try_from(groups.len() + 1).unwrap();
                        (*groups.entry(group).or_insert(next), mask)
                    });
            output.push(entry.depth);
            output.push(
                u8::try_from(source_index(entry.source)).unwrap() | (u8::from(entry.cursed) << 5),
            );
            output.push(group);
            output.extend_from_slice(&mask.to_le_bytes());
        }
    }
    io::stdout().lock().write_all(&output).unwrap();
}
