//! Deterministic seed-search primitives for Shattered Pixel Dungeon v3.3.x.
//!
//! The compatibility boundary is intentionally explicit: all random generation
//! must flow through [`rng`] so Java parity can be tested independently from
//! higher-level dungeon generation.

pub mod batch;
pub mod builder;
pub mod catalog;
pub mod caves_floor;
pub mod caves_mobs;
pub mod caves_rooms;
pub mod challenges;
pub mod city_boss_shop;
pub mod city_floor;
pub mod city_mobs;
pub mod city_rooms;
pub mod equipment;
pub mod feasibility;
pub mod generator;
pub mod geometry;
pub mod halls_floor;
pub mod halls_mobs;
pub mod halls_rooms;
pub mod java_math;
pub mod level;
pub mod level_flags;
pub mod level_prelude;
pub mod main_world;
mod maze;
pub mod mobs;
pub mod model;
pub mod painter;
pub mod prison_floor;
pub mod prison_mobs;
pub mod prison_rooms;
pub mod probability;
pub mod query;
pub mod quest_rooms;
pub mod quests;
pub mod regular_items;
pub mod regular_level;
pub mod regular_placement;
pub mod rng;
pub mod room;
pub mod room_decks;
pub mod run;
pub mod search;
pub mod secret_rooms;
pub mod seed;
pub mod sewer_floor;
pub mod sewer_mob_placement;
pub mod sewer_rooms;
pub mod shop;
pub mod special_consumable;
pub mod special_equipment;
pub mod special_forced;
pub mod wire;

/// Upstream generation line this engine targets.
pub const SHPD_VERSION: &str = "3.3.8";

/// Exact upstream revision used while implementing and validating parity.
pub const SHPD_COMMIT: &str = "7b8b845a76fe76c6b7c031ae9e570852411f56db";
