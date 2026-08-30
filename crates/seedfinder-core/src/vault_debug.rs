//! Ignored diagnostic that dumps a generated vault in the same text layout
//! as the official `VaultProbe2` oracle, for line-by-line comparison.

use std::fmt::Write as _;

use crate::challenges::Challenges;
use crate::seed::DungeonSeed;
use crate::vault_floor::generate_vault;

#[test]
#[ignore = "diagnostic dump for oracle comparison; set VAULT_SEED/VAULT_DEPTH/VAULT_OUT"]
fn dump_vault_for_probe_diff() {
    let code = std::env::var("VAULT_SEED").unwrap_or_else(|_| "AAA-AAA-AAA".to_string());
    let depth: u8 = std::env::var("VAULT_DEPTH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(19);
    let challenges = std::env::var("VAULT_CHALLENGES")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .map_or(Challenges::NONE, |bits| Challenges::new(bits).unwrap());
    let output_path = std::env::var("VAULT_OUT").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join(format!("rust-vault-{code}.txt"))
            .to_string_lossy()
            .into_owned()
    });
    let seed = i64::try_from(DungeonSeed::from_code(&code).unwrap().value()).unwrap();
    let vault = generate_vault(seed, depth, challenges).unwrap();
    let mut out = String::new();
    writeln!(
        out,
        "vault {}x{} mapHash={} heaps={} mobs={}",
        vault.width(),
        vault.height(),
        vault.level.java_map_hash(),
        vault.heaps.len(),
        vault.mobs.len()
    )
    .unwrap();
    let mut heaps: Vec<_> = vault.heaps.iter().collect();
    heaps.sort_by_key(|heap| heap.cell);
    for heap in heaps {
        let room = vault.room_at(heap.cell).map_or_else(
            || "-".to_string(),
            |room| format!("{:?}", vault.rooms[room].kind),
        );
        for item in &heap.items {
            writeln!(
                out,
                "  heap {} {:?} room={} : {:?}",
                heap.cell, heap.kind, room, item
            )
            .unwrap();
        }
    }
    let mut mobs: Vec<_> = vault.mobs.iter().map(|mob| (mob.cell, mob.kind)).collect();
    mobs.sort();
    for (cell, kind) in mobs {
        writeln!(out, "  mob {cell} {kind:?}").unwrap();
    }
    writeln!(out, "rooms:").unwrap();
    for room in &vault.rooms {
        writeln!(
            out,
            "  room {:?} {},{}-{},{}",
            room.kind, room.bounds.left, room.bounds.top, room.bounds.right, room.bounds.bottom
        )
        .unwrap();
    }
    writeln!(
        out,
        "transitions:\n  transition {} BRANCH_ENTRANCE",
        vault.entrance_cell
    )
    .unwrap();
    for y in 0..vault.height() {
        let row: Vec<String> = (0..vault.width())
            .map(|x| vault.level.map.cells[vault.level.map.cell(x, y)].to_string())
            .collect();
        writeln!(out, "map {},", row.join(",")).unwrap();
    }
    std::fs::write(output_path, out).unwrap();
}
