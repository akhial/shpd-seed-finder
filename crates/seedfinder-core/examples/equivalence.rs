//! Streams exact RC1 items, equipment coordinates, and terrain comparisons.
//! See tooling/parity/BatchEquipmentOracle.java and tooling/parity/EQUIVALENCE.md.
use shpd_seedfinder_core::{
    catalog::item, main_world::CanonicalMainWorldGenerator, search::WorldGenerator,
    seed::DungeonSeed,
};
use std::io::{self, BufRead};
#[path = "support/parity_floors.rs"]
mod parity_floors;
// Keep stream validation, comparisons, and accounting together for auditing.
#[allow(clippy::too_many_lines, clippy::stable_sort_primitive)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let start: u64 = args[1].parse().unwrap();
    let count: u64 = args[2].parse().unwrap();
    let (mut tested, mut deviations, mut errors, mut entries) = (0_u64, 0_u64, 0_u64, 0_u64);
    let (mut floors_compared, mut terrain_cells, mut item_locations) = (0_u64, 0_u64, 0_u64);
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
        let expected_maps = fields.next().expect("cell comparison requires oracle maps");
        let expected_locations = fields
            .next()
            .expect("item comparison requires oracle locations");
        assert!(fields.next().is_none());
        let result = std::panic::catch_unwind(|| {
            CanonicalMainWorldGenerator.generate(DungeonSeed::new(seed).unwrap(), 24)
        });
        if let Ok(world) = result {
            let floors = parity_floors::dump(
                world.seed,
                24,
                shpd_seedfinder_core::challenges::Challenges::NONE,
            )
            .expect("floor comparison generation failed");
            let actual_maps = floors
                .lines()
                .filter(|line| line.starts_with("map "))
                .collect::<Vec<_>>()
                .join(";");
            let mut locations = floors
                .lines()
                .filter_map(|line| line.strip_prefix("loc "))
                .collect::<Vec<_>>();
            locations.sort();
            let expected_locations = expected_locations
                .split(';')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            item_locations += expected_locations.len() as u64;
            for floor in expected_maps.split(';') {
                floors_compared += 1;
                terrain_cells += floor
                    .split_once(':')
                    .expect("terrain record")
                    .1
                    .split(',')
                    .count() as u64;
            }
            if locations != expected_locations {
                deviations += 1;
                let mut extra = locations;
                let mut missing = Vec::new();
                for expected in expected_locations {
                    if let Some(index) = extra.iter().position(|a| *a == expected) {
                        extra.remove(index);
                    } else {
                        missing.push(expected);
                    }
                }
                println!(
                    "{}",
                    serde_json::json!({"seed": seed, "locations_missing": missing, "locations_extra": extra})
                );
            }
            if actual_maps != expected_maps {
                deviations += 1;
                for (actual, expected) in actual_maps.split(';').zip(expected_maps.split(';')) {
                    if actual != expected {
                        println!(
                            "{}",
                            serde_json::json!({"seed": seed, "map_expected": expected, "map_actual": actual})
                        );
                    }
                }
                assert_eq!(
                    actual_maps.split(';').count(),
                    expected_maps.split(';').count(),
                    "different floor coverage"
                );
            }
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
        serde_json::json!({"summary":true,"start":start,"requested":count,"tested":tested,"deviations":deviations,"errors":errors,"entries":entries,
            "floors":floors_compared,"terrain_cells":terrain_cells,"item_locations":item_locations})
    );
    assert_eq!(tested, count, "incomplete oracle stream");
    if deviations != 0 || errors != 0 {
        std::process::exit(1);
    }
}
