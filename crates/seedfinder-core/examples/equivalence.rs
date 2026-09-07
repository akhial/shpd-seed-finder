//! Streams exact BETA-4 equipment comparisons. See tooling/parity/BatchEquipmentOracle.java.
use shpd_seedfinder_core::{
    catalog::item, main_world::CanonicalMainWorldGenerator, search::WorldGenerator,
    seed::DungeonSeed,
};
use std::io::{self, BufRead};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let start: u64 = args[1].parse().unwrap();
    let count: u64 = args[2].parse().unwrap();
    let (mut tested, mut deviations, mut errors, mut entries) = (0_u64, 0_u64, 0_u64, 0_u64);
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        if line.starts_with("BENCH ") {
            continue;
        }
        let mut fields = line.split('|');
        let first = fields.next().unwrap();
        if first == "ERROR" {
            let seed: u64 = fields.next().unwrap().parse().unwrap();
            assert_eq!(seed, start + tested);
            tested += 1;
            errors += 1;
            println!(
                "{}",
                serde_json::json!({"seed":seed,"oracle_error":fields.collect::<Vec<_>>().join("|")})
            );
            continue;
        }
        let seed: u64 = first.parse().unwrap();
        assert_eq!(seed, start + tested);
        let expected: Vec<&str> = fields
            .next()
            .unwrap()
            .split(';')
            .filter(|s| !s.is_empty())
            .collect();
        assert!(fields.next().is_none());
        let result = std::panic::catch_unwind(|| {
            CanonicalMainWorldGenerator.generate(DungeonSeed::new(seed).unwrap(), 24)
        });
        if let Ok(world) = result {
            let mut actual: Vec<String> = world
                .items
                .iter()
                .map(|i| {
                    format!(
                        "{},{:?},{},{},{},{}",
                        i.depth,
                        i.source,
                        item(i.item).stable_id,
                        i.upgrade,
                        u8::from(i.cursed),
                        i.effect.map_or("-", |e| e.wire_name())
                    )
                })
                .collect();
            actual.sort();
            entries += expected.len() as u64;
            if actual
                .iter()
                .map(String::as_str)
                .ne(expected.iter().copied())
            {
                deviations += 1;
                // Multiset difference: remove one occurrence at a time, retaining duplicates.
                let mut extra = actual.clone();
                let mut missing = Vec::new();
                for e in &expected {
                    if let Some(n) = extra.iter().position(|a| a == e) {
                        extra.remove(n);
                    } else {
                        missing.push(*e);
                    }
                }
                println!(
                    "{}",
                    serde_json::json!({"seed":seed,"code":world.seed.to_code(),"missing":missing,"extra":extra})
                );
            }
        } else {
            errors += 1;
            println!(
                "{}",
                serde_json::json!({"seed":seed,"engine_error":"panic"})
            );
        }
        tested += 1;
        if tested % 1000 == 0 {
            eprintln!(
                "PROGRESS tested={tested} deviations={deviations} errors={errors} entries={entries}"
            );
        }
    }
    println!(
        "{}",
        serde_json::json!({"summary":true,"start":start,"requested":count,"tested":tested,"deviations":deviations,"errors":errors,"entries":entries})
    );
    assert_eq!(tested, count, "incomplete oracle stream");
    if deviations != 0 || errors != 0 {
        std::process::exit(1);
    }
}
