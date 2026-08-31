//! Prints one hash over every item and quest outcome a seed range generates.
//!
//! Optimising the generator is only ever allowed to make the same draws
//! faster, never different ones, and the unit tests pin a handful of seeds
//! rather than a whole range. Running this before and after a change and
//! comparing the two digests covers thousands of seeds at once:
//!
//! ```sh
//! cargo run --release -p shpd-seedfinder-core --example world_digest -- 3000 19
//! ```
//!
//! Depth 19 is the interesting default because it is the deepest floor that
//! can hold the Imp's Vault, the largest level in the game.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use shpd_seedfinder_core::main_world::generate_main_world;
use shpd_seedfinder_core::seed::DungeonSeed;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let seeds: u64 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3_000);
    let depth: u8 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(19);

    let mut hasher = DefaultHasher::new();
    let mut items = 0_u64;
    for value in 0..seeds {
        let seed = DungeonSeed::new(value).expect("a counting seed is in range");
        let world = generate_main_world(seed, depth).expect("every seed generates a world");
        // The debug forms carry every searchable field, so a change to any of
        // them shows up here without this example having to track the model.
        for item in &world.items {
            format!("{item:?}").hash(&mut hasher);
            items += 1;
        }
        format!("{:?}", world.quests).hash(&mut hasher);
    }

    println!(
        "seeds={seeds} depth={depth} items={items} digest={:016x}",
        hasher.finish()
    );
}
