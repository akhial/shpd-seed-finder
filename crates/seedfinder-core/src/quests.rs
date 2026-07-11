//! Canonical v3.3.8 main-path quest scheduling and generated rewards.
//!
//! Quest room scheduling runs inside each region level's `initRooms()` hook,
//! while the Ghost and Wandmaker rewards are generated later in `createMobs()`.
//! Those phases are deliberately separate here: room placement and painting
//! consume RNG between them, and both NPC placement loops consume RNG before
//! their reward constructors run.

use std::fmt;

use crate::catalog::{ArmorEffect, Effect, ItemId, WeaponEffect};
use crate::equipment::{EquipmentRoll, random_armor_glyph, random_weapon_enchantment};
use crate::generator::{
    GeneratedEquipment, GeneratedItem, GeneratedItemFamily, GeneratedMissile, GeneratedRing,
    GeneratorError, random_armor, random_category, random_category_target, random_missile,
    random_wand, random_weapon, undo_drop,
};
use crate::model::{Accessibility, ItemSource, WorldItem};
use crate::rng::RandomStack;
use crate::run::{GeneratorCategory, GeneratorState};

/// A quest-generation invariant or a malformed level-pipeline call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestError {
    Generator(GeneratorError),
    GhostSpawnWasNotBegun,
    WandmakerSpawnWasNotBegun,
    InvalidGhostDepth(u8),
    ExpectedRing(GeneratedItemFamily),
}

impl fmt::Display for QuestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generator(error) => error.fmt(formatter),
            Self::GhostSpawnWasNotBegun => {
                formatter.write_str("Ghost reward generation requires a successful placement first")
            }
            Self::WandmakerSpawnWasNotBegun => formatter
                .write_str("Wandmaker reward generation requires a successful placement first"),
            Self::InvalidGhostDepth(depth) => {
                write!(formatter, "Ghost cannot spawn on canonical depth {depth}")
            }
            Self::ExpectedRing(actual) => {
                write!(
                    formatter,
                    "Imp reward expected a ring but generated {actual:?}"
                )
            }
        }
    }
}

impl std::error::Error for QuestError {}

impl From<GeneratorError> for QuestError {
    fn from(value: GeneratorError) -> Self {
        Self::Generator(value)
    }
}

/// Sewer quest selected solely by the Ghost's spawn depth.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum GhostQuestType {
    FetidRat = 1,
    GnollTrickster = 2,
    GreatCrab = 3,
}

impl GhostQuestType {
    fn from_depth(depth: u8) -> Result<Self, QuestError> {
        match depth {
            2 => Ok(Self::FetidRat),
            3 => Ok(Self::GnollTrickster),
            4 => Ok(Self::GreatCrab),
            _ => Err(QuestError::InvalidGhostDepth(depth)),
        }
    }
}

/// State owned by `Ghost.Quest` for one run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GhostQuest {
    pub spawned: bool,
    pub quest_type: Option<GhostQuestType>,
    pub given: bool,
    pub processed: bool,
    pub depth: Option<u8>,
    pub weapon: Option<GeneratedEquipment>,
    pub armor: Option<GeneratedEquipment>,
    pub enchantment: Option<WeaponEffect>,
    pub glyph: Option<ArmorEffect>,
    pending_spawn_depth: Option<u8>,
}

impl GhostQuest {
    /// Mirrors the condition at the start of `Ghost.Quest.spawn`.
    ///
    /// When this returns `true`, the level pipeline must run the Ghost's
    /// `roomExit.random()` placement loop and then call [`Self::finish_spawn`]
    /// before ordinary sewer mobs are generated.
    pub fn try_begin_spawn(&mut self, random: &mut RandomStack, depth: u8) -> bool {
        let should_spawn = !self.spawned
            && depth > 1
            && random.int_bound(5_i32.wrapping_sub(i32::from(depth))) == 0;
        self.pending_spawn_depth = should_spawn.then_some(depth);
        should_spawn
    }

    /// Generates both Ghost choices after the caller has consumed all NPC
    /// placement draws.
    ///
    /// # Errors
    ///
    /// Returns an error when no matching placement phase was begun, the depth
    /// is outside the canonical sewer range, or generator state is corrupted.
    pub fn finish_spawn(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
    ) -> Result<(), QuestError> {
        let depth = self
            .pending_spawn_depth
            .take()
            .ok_or(QuestError::GhostSpawnWasNotBegun)?;
        let quest_type = GhostQuestType::from_depth(depth)?;

        let armor_index = random
            .chances(&[0.0, 0.0, 10.0, 6.0, 3.0, 1.0])
            .unwrap_or(2);
        let armor_item = match armor_index {
            2 => ItemId::LeatherArmor,
            3 => ItemId::MailArmor,
            4 => ItemId::ScaleArmor,
            _ => ItemId::PlateArmor,
        };

        let weapon_index = random
            .chances(&[0.0, 0.0, 10.0, 6.0, 3.0, 1.0])
            .unwrap_or(2);
        let weapon_category = match weapon_index {
            2 => GeneratorCategory::WeaponTier2,
            3 => GeneratorCategory::WeaponTier3,
            4 => GeneratorCategory::WeaponTier4,
            _ => GeneratorCategory::WeaponTier5,
        };
        let mut weapon = random_category_target(random, generator, weapon_category, 0)
            .map_err(QuestError::from)?;

        // Generator.random() has already consumed Weapon.random(), but Ghost
        // clears every resulting property before assigning the shared level.
        weapon.roll = clean_roll(0);
        let mut armor = GeneratedEquipment {
            item: armor_item,
            roll: clean_roll(0),
        };

        let level_roll = random.float();
        let level = if level_roll < 0.5 {
            0
        } else if level_roll < 0.8 {
            1
        } else if level_roll < 0.95 {
            2
        } else {
            3
        };
        weapon.roll.upgrade = level;
        armor.roll.upgrade = level;

        // Both modifiers are generated even when the final float discards
        // them, keeping the number of RNG calls independent of the outcome.
        let enchantment = random_weapon_enchantment(random);
        let glyph = random_armor_glyph(random);
        let keep_modifier = random.float() <= 0.2;

        self.spawned = true;
        self.quest_type = Some(quest_type);
        self.given = false;
        self.processed = false;
        self.depth = Some(depth);
        self.weapon = Some(weapon);
        self.armor = Some(armor);
        self.enchantment = keep_modifier.then_some(enchantment);
        self.glyph = keep_modifier.then_some(glyph);
        Ok(())
    }

    /// Marks the quest as accepted after the boss placement succeeds.
    pub const fn give(&mut self) {
        self.given = true;
    }

    /// Mirrors `Ghost.Quest.process()` on the quest's own floor.
    pub const fn process(&mut self, current_depth: u8) {
        if self.spawned
            && self.given
            && !self.processed
            && matches!(self.depth, Some(depth) if depth == current_depth)
        {
            self.processed = true;
        }
    }

    /// A Ghost quest is active only between acceptance and boss defeat.
    #[must_use]
    pub const fn active(&self, current_depth: u8) -> bool {
        self.spawned
            && self.given
            && !self.processed
            && matches!(self.depth, Some(depth) if depth == current_depth)
    }

    /// Mirrors selecting either reward in `WndSadGhost`.
    pub fn complete(&mut self) {
        self.weapon = None;
        self.armor = None;
    }

    /// Appends the two mutually exclusive searchable reward choices on the
    /// floor where the Ghost spawned.
    pub fn append_world_items(
        &self,
        current_depth: u8,
        group: u16,
        output: &mut Vec<WorldItem>,
    ) -> usize {
        let (Some(depth), Some(weapon), Some(armor)) = (self.depth, self.weapon, self.armor) else {
            return 0;
        };
        if depth != current_depth {
            return 0;
        }

        let mut weapon_roll = weapon.roll;
        weapon_roll.effect = self.enchantment.map(Effect::Weapon);
        output.push(WorldItem::from_equipment_roll(
            weapon.item,
            weapon_roll,
            depth,
            ItemSource::GhostReward,
            Accessibility::Choice { group, option: 0 },
        ));

        let mut armor_roll = armor.roll;
        armor_roll.effect = self.glyph.map(Effect::Armor);
        output.push(WorldItem::from_equipment_roll(
            armor.item,
            armor_roll,
            depth,
            ItemSource::GhostReward,
            Accessibility::Choice { group, option: 1 },
        ));
        2
    }
}

/// Prison quest-room variant chosen when Wandmaker scheduling succeeds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum WandmakerQuestType {
    CorpseDust = 1,
    ElementalEmbers = 2,
    Rotberry = 3,
}

impl WandmakerQuestType {
    fn from_java_value(value: i32) -> Self {
        match value {
            1 => Self::CorpseDust,
            2 => Self::ElementalEmbers,
            _ => Self::Rotberry,
        }
    }
}

/// State owned by `Wandmaker.Quest` for one run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WandmakerQuest {
    pub spawned: bool,
    pub quest_type: Option<WandmakerQuestType>,
    pub given: bool,
    pub depth: Option<u8>,
    pub wand1: Option<GeneratedEquipment>,
    pub wand2: Option<GeneratedEquipment>,
    pub discarded_duplicates: usize,
    quest_room_spawned: bool,
    pending_spawn_depth: Option<u8>,
}

impl WandmakerQuest {
    /// Mirrors `Wandmaker.Quest.spawnRoom` at the end of `PrisonLevel.initRooms`.
    ///
    /// The stored type deliberately survives a failed painter attempt. On a
    /// retry Java short-circuits before the spawn-chance draw and adds the same
    /// quest room again.
    pub fn schedule_room(
        &mut self,
        random: &mut RandomStack,
        depth: u8,
    ) -> Option<WandmakerQuestType> {
        self.quest_room_spawned = false;
        let should_add = !self.spawned
            && (self.quest_type.is_some()
                || (depth > 6 && random.int_bound(10_i32.wrapping_sub(i32::from(depth))) == 0));
        if !should_add {
            return None;
        }

        let quest_type = *self.quest_type.get_or_insert_with(|| {
            WandmakerQuestType::from_java_value(random.int_bound(3).wrapping_add(1))
        });
        self.quest_room_spawned = true;
        Some(quest_type)
    }

    /// Starts the `createMobs()` phase and clears Java's transient
    /// `questRoomSpawned` flag before the caller runs the Wandmaker placement
    /// loop.
    pub fn begin_mob_spawn(&mut self, depth: u8) -> bool {
        if !self.quest_room_spawned {
            return false;
        }
        self.quest_room_spawned = false;
        self.pending_spawn_depth = Some(depth);
        true
    }

    /// Generates the two Wandmaker choices after all NPC placement draws.
    ///
    /// # Errors
    ///
    /// Returns an error when the mob phase was not begun or generator state is
    /// corrupted.
    pub fn finish_spawn(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
    ) -> Result<(), QuestError> {
        let depth = self
            .pending_spawn_depth
            .take()
            .ok_or(QuestError::WandmakerSpawnWasNotBegun)?;

        let mut first = random_wand(random, generator, false)?;
        first.roll.cursed = false;
        first.roll.upgrade = first.roll.upgrade.wrapping_add(1);
        // Wand.upgrade() always performs its curse-clearing Int(3), even
        // though the curse was cleared immediately before the call.
        random.int_bound(3);

        let mut discarded = Vec::new();
        let mut second = random_wand(random, generator, false)?;
        while second.item == first.item {
            discarded.push(second.item);
            second = random_wand(random, generator, false)?;
        }
        for item in &discarded {
            // Upstream's concrete-class assignability direction makes this a
            // no-op; retaining the call documents and tests that quirk.
            undo_drop(generator, *item);
        }
        second.roll.cursed = false;
        second.roll.upgrade = second.roll.upgrade.wrapping_add(1);
        random.int_bound(3);

        self.spawned = true;
        self.given = false;
        self.depth = Some(depth);
        self.wand1 = Some(first);
        self.wand2 = Some(second);
        self.discarded_duplicates = discarded.len();
        Ok(())
    }

    pub const fn give(&mut self) {
        self.given = true;
    }

    /// Mirrors selecting either reward in `WndWandmaker`.
    pub fn complete(&mut self) {
        self.wand1 = None;
        self.wand2 = None;
    }

    /// Appends the two mutually exclusive searchable wand choices.
    pub fn append_world_items(&self, group: u16, output: &mut Vec<WorldItem>) -> usize {
        let (Some(depth), Some(first), Some(second)) = (self.depth, self.wand1, self.wand2) else {
            return 0;
        };
        for (option, reward) in [(0, first), (1, second)] {
            output.push(WorldItem::from_equipment_roll(
                reward.item,
                reward.roll,
                depth,
                ItemSource::WandmakerReward,
                Accessibility::Choice { group, option },
            ));
        }
        2
    }
}

/// Implemented Caves quest variant; Fungi exists upstream but is not rolled.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum BlacksmithQuestType {
    Crystal = 1,
    Gnoll = 2,
}

/// The four entries in `Blacksmith.Quest.smithRewards`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlacksmithRewards {
    pub weapons: [GeneratedEquipment; 2],
    pub missile: GeneratedMissile,
    pub armor: GeneratedEquipment,
}

/// State owned by `Blacksmith.Quest` for one run.
#[allow(clippy::struct_excessive_bools)] // These are the upstream persisted state flags.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlacksmithQuest {
    pub spawned: bool,
    pub quest_type: Option<BlacksmithQuestType>,
    pub given: bool,
    pub started: bool,
    pub boss_beaten: bool,
    pub completed: bool,
    pub depth: Option<u8>,
    pub room_accessible: bool,
    pub rewards: Option<BlacksmithRewards>,
    pub enchantment: Option<WeaponEffect>,
    pub glyph: Option<ArmorEffect>,
    pub discarded_duplicates: usize,
    pub favor: i32,
    pub free_pickaxe: bool,
    pub pickaxe_available: bool,
    pub smiths: i32,
    room_in_current_build: bool,
}

impl Default for BlacksmithQuest {
    fn default() -> Self {
        Self {
            spawned: false,
            quest_type: None,
            given: false,
            started: false,
            boss_beaten: false,
            completed: false,
            depth: None,
            room_accessible: false,
            rewards: None,
            enchantment: None,
            glyph: None,
            discarded_duplicates: 0,
            favor: 0,
            free_pickaxe: false,
            pickaxe_available: true,
            smiths: 0,
            room_in_current_build: false,
        }
    }
}

impl BlacksmithQuest {
    /// Mirrors `Blacksmith.Quest.spawn` after `CavesLevel.initRooms` has made
    /// all ordinary rooms.
    ///
    /// # Errors
    ///
    /// Returns an error only for corrupted generator state.
    pub fn schedule_room(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
        depth: u8,
    ) -> Result<bool, QuestError> {
        if !self.begin_room_schedule(random, depth) {
            return Ok(false);
        }
        self.finish_room_schedule(random, generator)?;
        Ok(true)
    }

    /// Runs the spawn predicate and persistent-state writes which precede
    /// `new BlacksmithRoom()` in Java. The room's inherited `StandardRoom`
    /// constructor draw must occur before [`Self::finish_room_schedule`].
    pub(crate) fn begin_room_schedule(&mut self, random: &mut RandomStack, depth: u8) -> bool {
        self.room_in_current_build = false;
        if self.spawned
            || depth <= 11
            || random.int_bound(15_i32.wrapping_sub(i32::from(depth))) != 0
        {
            return false;
        }

        self.spawned = true;
        self.room_in_current_build = true;
        self.depth = Some(depth);
        self.given = false;
        true
    }

    /// Completes `Blacksmith.Quest.spawn()` after the Blacksmith room object
    /// has been constructed and appended to the room list.
    pub(crate) fn finish_room_schedule(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
    ) -> Result<(), QuestError> {
        debug_assert!(self.room_in_current_build && self.spawned);
        self.quest_type = Some(if random.int_range(1, 2) == 1 {
            BlacksmithQuestType::Crystal
        } else {
            BlacksmithQuestType::Gnoll
        });
        self.regenerate_rewards(random, generator, true)?;
        Ok(())
    }

    /// Mirrors `Blacksmith.Quest.generateRewards(useDecks)`.
    ///
    /// The upstream parameter name is misleading: it is forwarded directly to
    /// Generator's `useDefaults` parameter. The scheduled quest passes `true`,
    /// so its weapons and missile do not decrement their identity decks.
    ///
    /// # Errors
    ///
    /// Returns an error only for corrupted generator state.
    pub fn regenerate_rewards(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
        java_use_decks_flag: bool,
    ) -> Result<(), QuestError> {
        let first = random_weapon(random, generator, 3, java_use_decks_flag)?;
        let mut second = random_weapon(random, generator, 3, java_use_decks_flag)?;
        let mut discarded = Vec::new();
        while first.item == second.item {
            if java_use_decks_flag {
                discarded.push(second.item);
            }
            second = random_weapon(random, generator, 3, java_use_decks_flag)?;
        }
        for item in &discarded {
            undo_drop(generator, *item);
        }

        let mut missile = random_missile(random, generator, 3, java_use_decks_flag)?;
        let mut armor = random_armor(random, 3)?;
        let mut weapons = [first, second];

        let level_roll = random.float();
        let reward_level = if level_roll < 0.3 {
            0
        } else if level_roll < 0.75 {
            1
        } else if level_roll < 0.95 {
            2
        } else {
            3
        };
        for weapon in &mut weapons {
            weapon.roll = clean_roll(reward_level);
        }
        missile.roll = clean_roll(reward_level);
        armor.roll = clean_roll(reward_level);

        let enchantment = random_weapon_enchantment(random);
        let glyph = random_armor_glyph(random);
        let keep_modifier = random.float() <= 0.3;

        self.rewards = Some(BlacksmithRewards {
            weapons,
            missile,
            armor,
        });
        self.enchantment = keep_modifier.then_some(enchantment);
        self.glyph = keep_modifier.then_some(glyph);
        self.discarded_duplicates = discarded.len();
        Ok(())
    }

    /// Must be called after each painter attempt. Only the room present in the
    /// successful attempt makes the reward obtainable. Because upstream marks
    /// `spawned` before painting, a failed first attempt can permanently lose
    /// the room on the retry.
    pub const fn finish_build_attempt(&mut self, succeeded: bool) {
        if succeeded && self.room_in_current_build {
            self.room_accessible = true;
        }
        self.room_in_current_build = false;
    }

    pub const fn give(&mut self) {
        self.given = true;
        self.completed = false;
    }

    pub const fn start(&mut self) {
        self.started = true;
    }

    pub const fn beat_boss(&mut self) {
        self.boss_beaten = true;
    }

    /// Mirrors quest completion's favor calculation for collected dark gold.
    pub fn complete(&mut self, dark_gold: i32) {
        self.completed = true;
        self.favor = dark_gold.saturating_mul(50).clamp(0, 2_000);
        if self.boss_beaten {
            self.favor = self.favor.saturating_add(1_000);
        }
        if self.favor >= 2_500 {
            self.free_pickaxe = true;
        }
    }

    /// Whether at least one Blacksmith service remains in the modeled state.
    #[must_use]
    pub const fn rewards_available(&self) -> bool {
        self.favor > 0
            || (self.rewards.is_some() && self.smiths > 0)
            || (self.pickaxe_available && self.free_pickaxe)
    }

    /// Appends all four searchable entries from the smith choice, including
    /// the generated thrown weapon at option 2.
    pub fn append_world_items(&self, group: u16, output: &mut Vec<WorldItem>) -> usize {
        let (Some(depth), true, Some(rewards)) = (self.depth, self.room_accessible, self.rewards)
        else {
            return 0;
        };

        for (option, reward) in [(0, rewards.weapons[0]), (1, rewards.weapons[1])] {
            let mut roll = reward.roll;
            roll.effect = self.enchantment.map(Effect::Weapon);
            output.push(WorldItem::from_equipment_roll(
                reward.item,
                roll,
                depth,
                ItemSource::BlacksmithReward,
                Accessibility::Choice { group, option },
            ));
        }
        let mut count = 2;
        if let Some(missile_item) = rewards.missile.kind.item_id() {
            let mut missile_roll = rewards.missile.roll;
            missile_roll.effect = self.enchantment.map(Effect::Weapon);
            output.push(WorldItem::from_equipment_roll(
                missile_item,
                missile_roll,
                depth,
                ItemSource::BlacksmithReward,
                Accessibility::Choice { group, option: 2 },
            ));
            count += 1;
        }
        let mut armor_roll = rewards.armor.roll;
        armor_roll.effect = self.glyph.map(Effect::Armor);
        output.push(WorldItem::from_equipment_roll(
            rewards.armor.item,
            armor_roll,
            depth,
            ItemSource::BlacksmithReward,
            Accessibility::Choice { group, option: 3 },
        ));
        count + 1
    }
}

/// Mob class targeted by the old Imp quest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ImpTarget {
    Monk,
    Golem,
}

/// State owned by `Imp.Quest` for one run.
#[allow(clippy::struct_excessive_bools)] // These are the upstream persisted state flags.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImpQuest {
    pub spawned: bool,
    pub target: Option<ImpTarget>,
    pub given: bool,
    pub completed: bool,
    pub depth: Option<u8>,
    pub room_accessible: bool,
    pub reward: Option<GeneratedRing>,
    pub rejected_cursed_rings: usize,
    room_in_current_build: bool,
}

impl ImpQuest {
    /// Mirrors `Imp.Quest.spawn` after `CityLevel.initRooms` has made ordinary
    /// rooms.
    ///
    /// # Errors
    ///
    /// Returns an error only for corrupted generator state.
    pub fn schedule_room(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
        depth: u8,
    ) -> Result<bool, QuestError> {
        if !self.begin_room_schedule(random, depth) {
            return Ok(false);
        }
        self.finish_room_schedule(random, generator)?;
        Ok(true)
    }

    /// Runs the spawn predicate and state writes which precede construction
    /// of `AmbitiousImpRoom` in Java. Call [`Self::finish_room_schedule`] only
    /// after the room object has been appended to the initialized room list.
    pub(crate) fn begin_room_schedule(&mut self, random: &mut RandomStack, depth: u8) -> bool {
        self.room_in_current_build = false;
        if self.spawned
            || depth <= 16
            || random.int_bound(20_i32.wrapping_sub(i32::from(depth))) != 0
        {
            return false;
        }

        self.spawned = true;
        self.room_in_current_build = true;
        self.depth = Some(depth);
        true
    }

    /// Completes `Imp.Quest.spawn()` after `new AmbitiousImpRoom()` has run.
    /// This boundary is explicit even though v3.3.8's room constructor itself
    /// is draw-free, preventing later room changes from silently reordering
    /// the target and reward stream.
    pub(crate) fn finish_room_schedule(
        &mut self,
        random: &mut RandomStack,
        generator: &mut GeneratorState,
    ) -> Result<(), QuestError> {
        debug_assert!(self.room_in_current_build && self.spawned);
        let depth = self.depth.expect("scheduled Imp quest records its depth");
        self.target = Some(match depth {
            18 if random.int_bound(2) != 0 => ImpTarget::Golem,
            19 => ImpTarget::Golem,
            _ => ImpTarget::Monk,
        });
        self.given = false;
        let (reward, rejected) = generate_imp_reward(random, generator, depth)?;
        self.reward = Some(reward);
        self.rejected_cursed_rings = rejected;
        Ok(())
    }

    /// Must be called after each City painter attempt; see the equivalent
    /// Blacksmith hook for the failed-attempt edge case.
    pub const fn finish_build_attempt(&mut self, succeeded: bool) {
        if succeeded && self.room_in_current_build {
            self.room_accessible = true;
        }
        self.room_in_current_build = false;
    }

    pub const fn give(&mut self) {
        self.given = true;
        self.completed = false;
    }

    /// Whether killing this mob drops a token for the active quest.
    #[must_use]
    pub const fn should_drop_token(&self, current_depth: u8, mob: ImpTarget) -> bool {
        self.spawned
            && self.given
            && !self.completed
            && current_depth != 20
            && matches!(self.target, Some(target) if target as u8 == mob as u8)
    }

    /// Mirrors handing the required tokens to the Imp.
    pub fn complete(&mut self) {
        self.reward = None;
        self.completed = true;
    }

    /// Appends the obtainable deterministic ring reward for the accessible Imp room.
    pub fn append_world_item(&self, output: &mut Vec<WorldItem>) -> bool {
        let (Some(depth), true, Some(reward)) = (self.depth, self.room_accessible, self.reward)
        else {
            return false;
        };
        output.push(WorldItem::from_equipment_roll(
            reward.kind.item_id(),
            reward.roll,
            depth,
            ItemSource::ImpReward,
            Accessibility::Independent,
        ));
        true
    }
}

/// All quest state reset by `Dungeon.init()`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuestState {
    pub ghost: GhostQuest,
    pub wandmaker: WandmakerQuest,
    pub blacksmith: BlacksmithQuest,
    pub imp: ImpQuest,
}

impl QuestState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

const fn clean_roll(upgrade: u8) -> EquipmentRoll {
    EquipmentRoll {
        upgrade,
        effect: None,
        cursed: false,
    }
}

fn generate_imp_reward(
    random: &mut RandomStack,
    generator: &mut GeneratorState,
    depth: u8,
) -> Result<(GeneratedRing, usize), QuestError> {
    let mut rejected = 0;
    let mut reward = loop {
        let generated =
            random_category(random, generator, GeneratorCategory::Ring, i32::from(depth))?;
        let GeneratedItem::Ring(ring) = generated else {
            return Err(QuestError::ExpectedRing(generated.family()));
        };
        if ring.roll.cursed {
            rejected += 1;
        } else {
            break ring;
        }
    };

    // Ring.upgrade(2) dispatches to Ring.upgrade twice. Each call consumes an
    // Int(3) curse-clearing roll; Imp then unconditionally sets cursed=true.
    reward.roll.upgrade = reward.roll.upgrade.wrapping_add(2);
    random.int_bound(3);
    random.int_bound(3);
    reward.roll.cursed = true;
    Ok((reward, rejected))
}

#[cfg(test)]
mod tests {
    use crate::catalog::{ArmorEffect, Effect, ItemId, WeaponEffect};
    use crate::generator::MissileKind;
    use crate::model::{Accessibility, ItemSource};
    use crate::rng::RandomStack;
    use crate::run::{GeneratorCategory, RingKind, RunState};

    use super::{
        BlacksmithQuest, GhostQuest, ImpQuest, ImpTarget, WandmakerQuest, generate_imp_reward,
    };

    fn fixture(dungeon_seed: i64, outer_seed: i64) -> (RandomStack, crate::run::GeneratorState) {
        let mut random = RandomStack::with_base_seed(0);
        random.push(outer_seed);
        (random, RunState::new(dungeon_seed).generator)
    }

    #[test]
    fn ghost_rewards_match_actual_java_fixture() {
        let (mut random, mut generator) = fixture(0, 0);
        let mut quest = GhostQuest {
            pending_spawn_depth: Some(2),
            ..GhostQuest::default()
        };
        quest.finish_spawn(&mut random, &mut generator).unwrap();

        assert_eq!(quest.weapon.unwrap().item, ItemId::Crossbow);
        assert_eq!(quest.armor.unwrap().item, ItemId::MailArmor);
        assert_eq!(quest.weapon.unwrap().roll.upgrade, 1);
        assert_eq!(quest.enchantment, None);
        assert_eq!(quest.glyph, None);
        assert_eq!(
            generator.category(GeneratorCategory::WeaponTier4).dropped,
            1
        );
        assert_eq!(random.long(), 7_105_486_291_024_734_541);
    }

    #[test]
    fn wandmaker_rewards_include_upgrade_rng_and_match_java_fixture() {
        let (mut random, mut generator) = fixture(0, 0);
        let mut quest = WandmakerQuest {
            quest_room_spawned: true,
            ..WandmakerQuest::default()
        };
        assert!(quest.begin_mob_spawn(7));
        quest.finish_spawn(&mut random, &mut generator).unwrap();

        assert_eq!(quest.wand1.unwrap().item, ItemId::WandFrost);
        assert_eq!(quest.wand1.unwrap().roll.upgrade, 2);
        assert_eq!(quest.wand2.unwrap().item, ItemId::WandBlastWave);
        assert_eq!(quest.wand2.unwrap().roll.upgrade, 1);
        assert_eq!(quest.discarded_duplicates, 0);
        assert_eq!(generator.category(GeneratorCategory::Wand).dropped, 2);
        assert_eq!(random.long(), 2_158_390_814_503_909_950);
    }

    #[test]
    fn blacksmith_defaults_rewards_match_actual_java_fixture() {
        let (mut random, mut generator) = fixture(0, 0);
        let mut quest = BlacksmithQuest::default();
        quest
            .regenerate_rewards(&mut random, &mut generator, true)
            .unwrap();
        let rewards = quest.rewards.unwrap();

        assert_eq!(rewards.weapons[0].item, ItemId::StoneGauntlet);
        assert_eq!(rewards.weapons[1].item, ItemId::Longsword);
        assert_eq!(rewards.missile.kind, MissileKind::Tomahawk);
        assert_eq!(rewards.armor.item, ItemId::PlateArmor);
        assert_eq!(rewards.armor.roll.upgrade, 1);
        assert_eq!(quest.enchantment, Some(WeaponEffect::Chilling));
        assert_eq!(quest.glyph, Some(ArmorEffect::Viscosity));
        assert_eq!(quest.discarded_duplicates, 0);
        assert_eq!(random.long(), 25_579_809_655_956_232);
        assert_eq!(
            generator.category(GeneratorCategory::WeaponTier5).dropped,
            0,
            "scheduled Blacksmith rewards use default identities, not decks"
        );
    }

    #[test]
    fn blacksmith_duplicate_fixture_preserves_all_consumed_draws() {
        let (mut random, mut generator) = fixture(8_687_205_886, 42);
        let mut quest = BlacksmithQuest::default();
        quest
            .regenerate_rewards(&mut random, &mut generator, true)
            .unwrap();
        let rewards = quest.rewards.unwrap();

        assert_eq!(rewards.weapons[0].item, ItemId::Greatshield);
        assert_eq!(rewards.weapons[1].item, ItemId::StoneGauntlet);
        assert_eq!(rewards.missile.kind, MissileKind::ThrowingSpear);
        assert_eq!(rewards.armor.item, ItemId::ScaleArmor);
        assert_eq!(rewards.armor.roll.upgrade, 2);
        assert_eq!(quest.enchantment, Some(WeaponEffect::Shocking));
        assert_eq!(quest.glyph, Some(ArmorEffect::Repulsion));
        assert_eq!(quest.discarded_duplicates, 1);
        assert_eq!(random.long(), 4_690_832_018_155_665_766);
    }

    #[test]
    fn imp_ring_loop_and_virtual_upgrade_draws_match_java_fixture() {
        let (mut random, mut generator) = fixture(0, 0);
        let (reward, rejected) = generate_imp_reward(&mut random, &mut generator, 17).unwrap();

        assert_eq!(reward.kind, RingKind::Tenacity);
        assert_eq!(reward.roll.upgrade, 2);
        assert!(reward.roll.cursed);
        assert_eq!(rejected, 1);
        assert_eq!(generator.category(GeneratorCategory::Ring).dropped, 2);
        assert_eq!(random.long(), 2_158_390_814_503_909_950);
    }

    #[test]
    fn wandmaker_retry_short_circuits_without_an_rng_draw() {
        let (mut random, _) = fixture(0, 123);
        let mut reference = random.clone();
        let mut quest = WandmakerQuest {
            quest_type: Some(super::WandmakerQuestType::Rotberry),
            ..WandmakerQuest::default()
        };
        assert_eq!(
            quest.schedule_room(&mut random, 8),
            Some(super::WandmakerQuestType::Rotberry)
        );
        assert_eq!(random.long(), reference.long());
    }

    #[test]
    fn failed_blacksmith_build_loses_room_but_retains_generated_state() {
        let (mut random, mut generator) = fixture(0, 0);
        let mut quest = BlacksmithQuest::default();
        assert!(
            quest
                .schedule_room(&mut random, &mut generator, 14)
                .unwrap()
        );
        quest.finish_build_attempt(false);
        assert!(
            !quest
                .schedule_room(&mut random, &mut generator, 14)
                .unwrap()
        );
        quest.finish_build_attempt(true);

        assert!(quest.spawned);
        assert!(quest.rewards.is_some());
        assert!(!quest.room_accessible);
    }

    #[test]
    fn world_records_retain_mutually_exclusive_options_and_modifiers() {
        let (mut random, mut generator) = fixture(0, 0);
        let mut ghost = GhostQuest {
            pending_spawn_depth: Some(2),
            ..GhostQuest::default()
        };
        ghost.finish_spawn(&mut random, &mut generator).unwrap();
        let mut output = Vec::new();
        assert_eq!(ghost.append_world_items(2, 7, &mut output), 2);
        assert_eq!(ghost.append_world_items(3, 8, &mut output), 0);
        assert_eq!(output[0].source, ItemSource::GhostReward);
        assert_eq!(
            output[0].accessibility,
            Accessibility::Choice {
                group: 7,
                option: 0
            }
        );
        assert_eq!(
            output[1].accessibility,
            Accessibility::Choice {
                group: 7,
                option: 1
            }
        );

        let (mut random, mut generator) = fixture(0, 0);
        let mut smith = BlacksmithQuest {
            depth: Some(12),
            room_accessible: true,
            ..BlacksmithQuest::default()
        };
        smith
            .regenerate_rewards(&mut random, &mut generator, true)
            .unwrap();
        output.clear();
        assert_eq!(smith.append_world_items(9, &mut output), 4);
        assert_eq!(
            output[0].effect,
            Some(Effect::Weapon(WeaponEffect::Chilling))
        );
        assert_eq!(
            output[2].effect,
            Some(Effect::Weapon(WeaponEffect::Chilling))
        );
        assert_eq!(output[2].item, ItemId::Tomahawk);
        assert_eq!(
            output[3].effect,
            Some(Effect::Armor(ArmorEffect::Viscosity))
        );
        assert_eq!(
            output[3].accessibility,
            Accessibility::Choice {
                group: 9,
                option: 3
            }
        );
    }

    #[test]
    fn imp_target_and_token_conditions_match_depth_rules() {
        let mut quest = ImpQuest {
            spawned: true,
            target: Some(ImpTarget::Monk),
            given: true,
            ..ImpQuest::default()
        };
        assert!(quest.should_drop_token(17, ImpTarget::Monk));
        assert!(!quest.should_drop_token(17, ImpTarget::Golem));
        assert!(!quest.should_drop_token(20, ImpTarget::Monk));
        quest.complete();
        assert!(!quest.should_drop_token(17, ImpTarget::Monk));
    }
}
