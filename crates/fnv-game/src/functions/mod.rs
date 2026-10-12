//! Script function implementations, dispatched by opcode (docs/design/layer5.md §3.7).
//! Scripts and conditions share them (a condition's function index is opcode −
//! 0x1000). Each entry is registered with the catalogue name of its opcode, which a test
//! checks against `schemas/script/functions.json`.
//!
//! - [`Kind::Impl`]: implemented over the Layer 4 world and the Layer 5 state;
//! - [`Kind::Stub`]: the function belongs to a Layer 6 system (dialogue, AI packages,
//!   combat, sound, image space, menus, rendering): the call is recorded in the trace and
//!   has no effect;
//! - not listed: unhandled (traced once per opcode and unit, counted, returns 0).
//!
//! Behaviour notes per function are paraphrased from the catalogue (`summary`/`notes`,
//! our wording of the GECK wiki) unless tagged otherwise. docs/spec-notes/scripts.md §7
//! lists every function with its evidence.

pub(crate) mod actor;
pub(crate) mod furniture;
mod items;
mod magic;
pub(crate) mod misc;
mod objects;
mod player;
mod quest;
mod refs;
mod reputation;

pub use reputation::{store as reputation_store, tiers as reputation_tiers};
pub use actor::{actor_value, actor_value_capped, actor_value_parts, av_by_name, av_code, set_current as set_actor_value_current, weapon_anim_code, AV_NAMES};
pub use actor::{faction_rank, faction_relation, factions_of, factions_relation};
pub use misc::clock_now;
/// The level (0–3) of fame or infamy against a reputation's scale (the Pip-Boy's STATS page).
pub use reputation::level as reputation_level;

use crate::game::{Game, InstKey};
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The context of one call.
#[derive(Debug, Clone)]
pub struct Ctx {
    /// The running unit's key (for trace events), or a condition's owner label.
    pub unit: String,
    pub opcode: u16,
    /// The explicit calling reference (`Ref.Func`), resolved.
    pub on: Option<FormId>,
    /// The reference the script is attached to (object scripts, dialogue speakers,
    /// package owners); a condition's run-on reference.
    pub self_ref: Option<FormId>,
    /// The activating reference inside OnActivate, the holder in inventory events.
    pub action_ref: Option<FormId>,
    /// The quest whose script or stage result is running.
    pub quest: Option<FormId>,
    pub inst: Option<InstKey>,
    pub in_condition: bool,
}

impl Ctx {
    /// The calling reference: explicit, else the script's own reference
    /// [hypothesis: a bare call in an object script runs on its reference,
    /// script-functions.md §6].
    pub fn subject(&self) -> Result<FormId, String> {
        self.on.or(self.self_ref).ok_or_else(|| "no calling reference".to_string())
    }

    /// The calling reference, or the player when there is none (functions documented to
    /// act on the player without a reference).
    pub fn subject_or_player(&self) -> FormId {
        self.on.or(self.self_ref).unwrap_or(FormId::PLAYER_REF)
    }
}

/// Evaluated arguments with typed accessors.
pub struct Args<'a>(pub &'a [ArgValue]);

impl Args<'_> {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&ArgValue> {
        self.0.get(i)
    }

    pub fn form(&self, i: usize) -> Result<FormId, String> {
        self.opt_form(i).ok_or_else(|| format!("parameter {} is not a form", i + 1))
    }

    pub fn opt_form(&self, i: usize) -> Option<FormId> {
        match self.0.get(i)? {
            ArgValue::Value(v) => v.as_form().map(FormId),
            _ => None,
        }
    }

    pub fn num(&self, i: usize) -> Result<f64, String> {
        self.0.get(i).and_then(ArgValue::value).map(Value::num).ok_or_else(|| format!("parameter {} is missing", i + 1))
    }

    pub fn num_or(&self, i: usize, d: f64) -> f64 {
        self.num(i).unwrap_or(d)
    }

    pub fn int(&self, i: usize) -> Result<i32, String> {
        self.num(i).map(|x| x as i32)
    }

    /// An enumeration code (actor value, sex, …), also accepted as a number.
    pub fn code(&self, i: usize) -> Result<u16, String> {
        match self.0.get(i) {
            Some(ArgValue::Enum(c)) => Ok(*c),
            Some(ArgValue::Value(v)) => Ok(v.num() as u16),
            _ => Err(format!("parameter {} is not a code", i + 1)),
        }
    }

    pub fn axis(&self, i: usize) -> Result<usize, String> {
        let c = match self.0.get(i) {
            Some(ArgValue::Axis(c)) => *c,
            Some(ArgValue::Enum(c)) => char::from_u32(*c as u32).unwrap_or('?'),
            Some(ArgValue::Value(v)) => char::from_u32(v.num() as u32).unwrap_or('?'),
            _ => '?',
        };
        match c.to_ascii_uppercase() {
            'X' => Ok(0),
            'Y' => Ok(1),
            'Z' => Ok(2),
            _ => Err(format!("parameter {} is not an axis", i + 1)),
        }
    }

    pub fn str(&self, i: usize) -> Result<&str, String> {
        match self.0.get(i) {
            Some(ArgValue::Str(s)) => Ok(s),
            _ => Err(format!("parameter {} is not a string", i + 1)),
        }
    }
}

pub type FnImpl = fn(&mut Game, &Ctx, &Args) -> Result<Value, String>;

#[derive(Clone, Copy)]
pub enum Kind {
    Impl(FnImpl),
    /// A Layer 6 system; the text names it.
    Stub(&'static str),
}

/// Every registered function: opcode, catalogue name, kind.
pub fn registry() -> &'static [(u16, &'static str, Kind)] {
    use Kind::*;
    &[
        // Quests.
        (0x1039, "SetStage", Impl(quest::set_stage)),
        (0x103A, "GetStage", Impl(quest::get_stage)),
        (0x103B, "GetStageDone", Impl(quest::get_stage_done)),
        (0x1036, "StartQuest", Impl(quest::start_quest)),
        (0x1037, "StopQuest", Impl(quest::stop_quest)),
        (0x1038, "GetQuestRunning", Impl(quest::get_quest_running)),
        (0x1222, "GetQuestCompleted", Impl(quest::get_quest_completed)),
        (0x11A3, "SetObjectiveDisplayed", Impl(quest::set_objective_displayed)),
        (0x11A2, "SetObjectiveCompleted", Impl(quest::set_objective_completed)),
        (0x11A5, "GetObjectiveDisplayed", Impl(quest::get_objective_displayed)),
        (0x11A4, "GetObjectiveCompleted", Impl(quest::get_objective_completed)),
        (0x104F, "GetQuestVariable", Impl(quest::get_quest_variable)),
        (0x1035, "GetScriptVariable", Impl(quest::get_script_variable)),
        (0x11DC, "SetQuestDelay", Impl(quest::set_quest_delay)),
        (0x1071, "CompleteQuest", Impl(quest::complete_quest)),
        // References.
        (0x1021, "Enable", Impl(refs::enable)),
        (0x1022, "Disable", Impl(refs::disable)),
        (0x1023, "GetDisabled", Impl(refs::get_disabled)),
        (0x109E, "MoveTo", Impl(refs::move_to)),
        (0x124F, "MoveToMarkerWithFade", Impl(refs::move_to)),
        (0x10CE, "GetSelf", Impl(refs::get_self)),
        (0x1048, "GetIsID", Impl(refs::get_is_id)),
        (0x1088, "GetIsReference", Impl(refs::get_is_reference)),
        (0x1043, "GetInCell", Impl(refs::get_in_cell)),
        (0x1136, "GetInWorldspace", Impl(refs::get_in_worldspace)),
        (0x1020, "GetInSameCell", Impl(refs::get_in_same_cell)),
        (0x112C, "IsInInterior", Impl(refs::is_in_interior)),
        (0x1001, "GetDistance", Impl(refs::get_distance)),
        (0x1006, "GetPos", Impl(refs::get_pos)),
        (0x1007, "SetPos", Impl(refs::set_pos)),
        (0x1008, "GetAngle", Impl(refs::get_angle)),
        (0x1009, "SetAngle", Impl(refs::set_angle)),
        (0x116B, "GetLinkedRef", Impl(refs::get_linked_ref)),
        (0x1113, "GetParentRef", Impl(refs::get_parent_ref)),
        (0x11BB, "MarkForDelete", Impl(refs::mark_for_delete)),
        (0x10CC, "SetDestroyed", Impl(refs::set_destroyed)),
        (0x10CB, "GetDestroyed", Impl(refs::get_destroyed)),
        (0x1072, "Lock", Impl(refs::lock)),
        (0x1073, "Unlock", Impl(refs::unlock)),
        (0x1005, "GetLocked", Impl(refs::get_locked)),
        (0x100D, "Activate", Impl(refs::activate)),
        (0x10CD, "GetActionRef", Impl(refs::get_action_ref)),
        (0x1069, "IsActionRef", Impl(refs::is_action_ref)),
        (0x1161, "IsActor", Impl(refs::is_actor)),
        (0x1040, "GetIsCreature", Impl(refs::get_is_creature)),
        (0x1174, "IsInList", Impl(refs::is_in_list)),
        (0x1233, "AddFormToFormList", Impl(refs::add_form_to_form_list)),
        (0x10A5, "RemoveMe", Impl(items::remove_me)),
        // Items.
        (0x1002, "AddItem", Impl(items::add_item)),
        (0x1052, "RemoveItem", Impl(items::remove_item)),
        (0x102F, "GetItemCount", Impl(items::get_item_count)),
        (0x10AD, "RemoveAllItems", Impl(items::remove_all_items)),
        (0x10EE, "EquipItem", Impl(items::equip_item)),
        (0x10EF, "UnequipItem", Impl(items::unequip_item)),
        (0x10B6, "GetEquipped", Impl(items::get_equipped)),
        (0x11F4, "GetWeaponHealthPerc", Impl(items::get_weapon_health_perc)),
        // Actors.
        (0x100E, "GetActorValue", Impl(actor::get_actor_value)),
        (0x100F, "SetActorValue", Impl(actor::set_actor_value)),
        (0x1010, "ModActorValue", Impl(actor::mod_actor_value)),
        (0x110E, "ForceActorValue", Impl(actor::force_actor_value)),
        (0x1181, "DamageActorValue", Impl(actor::damage_actor_value)),
        (0x1182, "RestoreActorValue", Impl(actor::restore_actor_value)),
        (0x1115, "GetBaseActorValue", Impl(actor::get_base_actor_value)),
        (0x11EF, "GetPermanentActorValue", Impl(actor::get_permanent_actor_value)),
        (0x102E, "GetDead", Impl(actor::get_dead)),
        (0x108B, "KillActor", Impl(actor::kill_actor)),
        (0x108C, "ResurrectActor", Impl(actor::resurrect_actor)),
        (0x1054, "GetDeadCount", Impl(actor::get_dead_count)),
        (0x1047, "GetInFaction", Impl(actor::get_in_faction)),
        (0x117F, "AddToFaction", Impl(actor::add_to_faction)),
        (0x1180, "RemoveFromFaction", Impl(actor::remove_from_faction)),
        (0x1049, "GetFactionRank", Impl(actor::get_faction_rank)),
        (0x1089, "SetFactionRank", Impl(actor::add_to_faction)),
        (0x11AB, "GetIsVoiceType", Impl(actor::get_is_voice_type)),
        (0x1046, "GetIsSex", Impl(actor::get_is_sex)),
        (0x1083, "GetPCIsSex", Impl(actor::get_pc_is_sex)),
        (0x11E7, "SexChange", Impl(actor::sex_change)),
        (0x1045, "GetIsRace", Impl(actor::get_is_race)),
        (0x1044, "GetIsClass", Impl(actor::get_is_class)),
        (0x10EC, "SetGhost", Impl(actor::set_ghost)),
        (0x10F3, "SetRestrained", Impl(actor::set_restrained)),
        (0x10F4, "GetRestrained", Impl(actor::get_restrained)),
        (0x10F1, "SetUnconscious", Impl(actor::set_unconscious)),
        (0x10F2, "GetUnconscious", Impl(actor::get_unconscious)),
        (0x1150, "ResetHealth", Impl(actor::reset_health)),
        // The player.
        (0x1177, "RewardXP", Impl(player::reward_xp)),
        (0x11C1, "HasPerk", Impl(player::has_perk)),
        (0x1176, "AddPerk", Impl(player::add_perk)),
        (0x11A8, "RemovePerk", Impl(player::remove_perk)),
        (0x117E, "GetHasNote", Impl(player::get_has_note)),
        (0x117C, "AddNote", Impl(player::add_note)),
        (0x124A, "IsHardcore", Impl(player::is_hardcore)),
        (0x11CE, "IsPlayerTagSkill", Impl(player::is_player_tag_skill)),
        (0x11CD, "SetPlayerTagSkill", Impl(player::set_player_tag_skill)),
        (0x114A, "AddAchievement", Impl(player::add_achievement)),
        (0x1061, "DisablePlayerControls", Impl(player::disable_player_controls)),
        (0x1060, "EnablePlayerControls", Impl(player::enable_player_controls)),
        (0x1062, "GetPlayerControlsDisabled", Impl(player::get_player_controls_disabled)),
        (0x1050, "GetLevel", Impl(player::get_level)),
        (0x1138, "GetPCMiscStat", Impl(player::get_pc_misc_stat)),
        (0x1137, "ModPCMiscStat", Impl(player::mod_pc_misc_stat)),
        (0x11AF, "GetHealthPercentage", Impl(actor::get_health_percentage)),
        (0x117D, "RemoveNote", Impl(player::remove_note)),
        (0x126D, "ResetXP", Impl(player::reset_xp)),
        (0x123D, "GetReputation", Impl(player::get_reputation)),
        (0x1238, "SetReputation", Impl(player::set_reputation)),
        (0x1204, "RewardKarma", Impl(player::reward_karma)),
        (0x124B, "SetHardcore", Impl(player::set_hardcore)),
        (0x1055, "ShowMap", Impl(player::show_map)),
        (0x11EC, "GetMapMarkerVisible", Impl(player::get_map_marker_visible)),
        (0x1024, "MenuMode", Impl(misc::menu_mode)),
        (0x10DD, "SetOpenState", Impl(refs::set_open_state)),
        (0x109D, "GetOpenState", Impl(refs::get_open_state)),
        (0x11BA, "EnterTrigger", Impl(refs::enter_trigger)),
        (0x11C5, "SetPlayerTeammate", Impl(actor::set_player_teammate)),
        (0x11C6, "GetPlayerTeammate", Impl(actor::get_player_teammate)),
        // Messages, time, globals, numbers.
        (0x1059, "ShowMessage", Impl(misc::show_message)),
        (0x101F, "GetButtonPressed", Impl(misc::get_button_pressed)),
        (0x11B9, "ShowWarning", Impl(misc::show_warning)),
        (0x104D, "GetRandomPercent", Impl(misc::get_random_percent)),
        (0x100C, "GetSecondsPassed", Impl(misc::get_seconds_passed)),
        (0x104A, "GetGlobalValue", Impl(misc::get_global_value)),
        (0x1012, "GetCurrentTime", Impl(misc::get_current_time)),
        (0x1100, "GetGameSetting", Impl(misc::get_game_setting)),
        (0x1221, "Abs", Impl(misc::abs)),
        // State that only Layer 6 changes; with no Layer 6 it reads "no".
        (0x1121, "IsInCombat", Impl(crate::awareness::is_in_combat)),
        // The PC version [decision]: not a console.
        (0x1135, "IsXBox", Impl(misc::zero)),
        (0x120B, "IsPS3", Impl(misc::zero)),
        (0x108D, "IsTalking", Impl(misc::is_talking)),
        // Reputations (reputation.rs).
        (0x123F, "GetReputationThreshold", Impl(reputation::get_reputation_threshold)),
        (0x1239, "AddReputation", Impl(reputation::add_reputation)),
        (0x123A, "RemoveReputation", Impl(reputation::remove_reputation)),
        (0x123B, "AddReputationExact", Impl(reputation::add_reputation_exact)),
        (0x123E, "GetReputationPct", Impl(reputation::get_reputation_pct)),
        // Objects in the world (objects.rs).
        (0x1025, "PlaceAtMe", Impl(objects::place_at_me)),
        (0x11D1, "PlaceLeveledActorAtMe", Impl(objects::place_leveled_actor_at_me)),
        (0x10A6, "DropMe", Impl(objects::drop_me)),
        (0x1057, "Drop", Impl(objects::drop)),
        (0x1117, "SetOwnership", Impl(objects::set_ownership)),
        (0x118C, "ClearOwnership", Impl(objects::clear_ownership)),
        (0x1116, "IsOwner", Impl(objects::is_owner)),
        (0x1119, "SetCellOwnership", Impl(objects::set_cell_ownership)),
        (0x10BB, "SetCellPublicFlag", Impl(objects::set_cell_public_flag)),
        (0x116C, "DamageObject", Impl(objects::damage_object)),
        (0x11D7, "GetDestructionStage", Impl(objects::get_destruction_stage)),
        (0x11D8, "ClearDestruction", Impl(objects::clear_destruction)),
        (0x1260, "IsPlayerInRegion", Impl(objects::is_player_in_region)),
        (0x1264, "PlayerInRegion", Impl(objects::is_player_in_region)),
        (0x1063, "GetHeadingAngle", Impl(objects::get_heading_angle)),
        (0x122E, "HasLoaded3D", Impl(objects::has_loaded_3d)),
        (0x1018, "GetScale", Impl(objects::get_scale)),
        (0x113C, "SetScale", Impl(objects::set_scale)),
        (0x1041, "GetLockLevel", Impl(objects::get_lock_level)),
        (0x10E6, "GetInCellParam", Impl(objects::get_in_cell_param)),
        (0x10CF, "GetContainer", Impl(objects::get_container)),
        (0x1228, "SetMinimalUse", Impl(objects::set_minimal_use)),
        (0x1212, "SetCriticalStage", Impl(objects::set_critical_stage)),
        (0x1213, "IsInCriticalStage", Impl(objects::is_in_critical_stage)),
        (0x11D5, "SetBroadcastState", Impl(objects::set_broadcast_state)),
        (0x11D4, "GetBroadcastState", Impl(crate::radio::get_broadcast_state)),
        (0x1171, "SetTalkingActivatorActor", Impl(objects::set_talking_activator_actor)),
        // Inventories (items.rs).
        (0x1249, "RemoveAllTypedItems", Impl(items::remove_all_typed_items)),
        (0x11BC, "AddItemHealthPercent", Impl(items::add_item_health_percent)),
        (0x11F6, "ModWeaponHealthPerc", Impl(items::mod_weapon_health_perc)),
        (0x11F5, "SetWeaponHealthPerc", Impl(items::set_weapon_health_perc)),
        (0x118F, "IsWeaponInList", Impl(items::is_weapon_in_list)),
        (0x106D, "IsWeaponSkillType", Impl(items::is_weapon_skill_type)),
        (0x11ED, "ResetInventory", Impl(items::reset_inventory)),
        (0x127A, "GetContainerInventoryCount", Impl(items::get_container_inventory_count)),
        (0x1155, "SetItemValue", Impl(items::set_item_value)),
        (0x110D, "SetQuestObject", Impl(items::set_quest_object)),
        // Magic effects and effect scripts (magic.rs).
        (0x101C, "AddSpell", Impl(magic::add_spell)),
        (0x101D, "RemoveSpell", Impl(magic::remove_spell)),
        (0x11D9, "CastImmediateOnSelf", Impl(magic::cast_immediate_on_self)),
        (0x1142, "Dispel", Impl(magic::dispel)),
        (0x1148, "DispelAllSpells", Impl(magic::dispel_all_spells)),
        (0x10DF, "IsSpellTarget", Impl(magic::is_spell_target)),
        (0x10D6, "HasMagicEffect", Impl(magic::has_magic_effect)),
        (0x10BA, "ScriptEffectElapsedSeconds", Impl(magic::script_effect_elapsed_seconds)),
        // Actors and factions.
        (0x116D, "IsChild", Impl(actor::is_child)),
        (0x11B6, "GetIsCreatureType", Impl(actor::get_is_creature_type)),
        (0x1003, "SetEssential", Impl(actor::set_essential)),
        (0x1273, "SetActorRefEssential", Impl(actor::set_actor_ref_essential)),
        (0x1162, "IsEssential", Impl(actor::is_essential)),
        (0x1272, "IsActorRefEssential", Impl(actor::is_essential)),
        (0x111C, "SetActorFullName", Impl(actor::set_actor_full_name)),
        (0x1199, "IsKiller", Impl(actor::is_killer)),
        (0x11C2, "GetFactionRelation", Impl(actor::get_faction_relation)),
        (0x119B, "GetFactionCombatReaction", Impl(actor::get_faction_combat_reaction)),
        (0x1078, "SetEnemy", Impl(actor::set_enemy)),
        (0x1079, "SetAlly", Impl(actor::set_ally)),
        (0x1214, "RemoveFromAllFactions", Impl(actor::remove_from_all_factions)),
        (0x103C, "GetFactionRankDifference", Impl(actor::get_faction_rank_difference)),
        (0x10C6, "SetPCEnemyOfFaction", Impl(actor::set_pc_enemy_of_faction)),
        (0x10C5, "GetPCEnemyOfFaction", Impl(actor::get_pc_enemy_of_faction)),
        (0x11C9, "ClearFactionPlayerEnemyFlag", Impl(actor::clear_faction_player_enemy_flag)),
        (0x11CC, "GetActorFactionPlayerEnemy", Impl(actor::get_actor_faction_player_enemy)),
        (0x10AB, "IgnoreCrime", Impl(actor::ignore_crime)),
        (0x10C0, "GetIgnoreCrime", Impl(actor::get_ignore_crime)),
        (0x1151, "SetIgnoreFriendlyHits", Impl(actor::set_ignore_friendly_hits)),
        (0x1152, "GetIgnoreFriendlyHits", Impl(actor::get_ignore_friendly_hits)),
        (0x10ED, "GetIsGhost", Impl(actor::get_ghost)),
        (0x1133, "SetLevel", Impl(actor::set_level)),
        (0x11C7, "GetPlayerTeammateCount", Impl(actor::get_player_teammate_count)),
        // The player (player.rs).
        (0x1111, "EnableFastTravel", Impl(player::enable_fast_travel)),
        (0x125E, "UnlockChallenge", Impl(player::unlock_challenge)),
        (0x1266, "GetChallengeCompleted", Impl(player::get_challenge_completed)),
        (0x126C, "RemoveRecurringFromChallenge", Impl(player::remove_recurring_from_challenge)),
        (0x10E7, "SetInCharGen", Impl(player::set_in_chargen)),
        (0x127F, "GetInCharGen", Impl(player::get_in_chargen)),
        (0x125C, "GetCasinoWinningsLevel", Impl(player::get_casino_winnings_level)),
        (0x1262, "GetCasinoWinningStage", Impl(player::get_casino_winnings_level)),
        (0x1229, "SetPCCanUsePowerArmor", Impl(player::set_pc_can_use_power_armor)),
        (0x1058, "AddTopic", Impl(player::add_topic)),
        (0x126B, "IsAlwaysHardcore", Impl(player::is_always_hardcore)),
        // Quests.
        (0x11AD, "CompleteAllObjectives", Impl(quest::complete_all_objectives)),
        (0x11DB, "ResetQuest", Impl(quest::reset_quest)),
        // Time and numbers.
        (0x10AA, "GetDayOfWeek", Impl(misc::get_day_of_week)),
        (0x121F, "Sqrt", Impl(misc::sqrt)),
        (0x120C, "IsWin32", Impl(misc::one)),
        // Gore is on, the game's default [decision].
        (0x1226, "IsGoreDisabled", Impl(misc::zero)),
        // Layer 6 systems: recorded, no effect.
        (0x105E, "EvaluatePackage", Impl(actor::evaluate_package)),
        (0x1097, "AddScriptPackage", Impl(actor::add_script_package)),
        (0x1098, "RemoveScriptPackage", Impl(actor::remove_script_package)),
        (0x11FA, "ResetAI", Stub("AI packages")),
        (0x1034, "SayTo", Impl(misc::say_to)),
        (0x1033, "Say", Impl(misc::say)),
        (0x1056, "StartConversation", Impl(misc::start_conversation)),
        (0x1016, "StartCombat", Impl(crate::awareness::start_combat)),
        (0x1017, "StopCombat", Impl(crate::awareness::stop_combat)),
        (0x1184, "SetDisposition", Stub("dialogue (disposition)")),
        (0x115C, "SendTrespassAlarm", Stub("crime")),
        (0x1013, "PlayGroup", Impl(crate::anims::play_group)),
        (0x1190, "PlayIdle", Impl(crate::idles::play_idle)),
        (0x105C, "Look", Stub("animation (head tracking)")),
        (0x105D, "StopLook", Stub("animation (head tracking)")),
        (0x11E4, "AgeRace", Stub("character appearance")),
        (0x1026, "PlaySound", Impl(crate::sound::play_sound)),
        (0x11FF, "PlayMusic", Impl(crate::sound::play_music)),
        (0x1114, "PlayBink", Stub("video")),
        (0x1191, "ApplyImageSpaceModifier", Impl(misc::apply_image_space_modifier)),
        (0x1192, "RemoveImageSpaceModifier", Impl(misc::remove_image_space_modifier)),
        (0x1232, "IsImageSpaceActive", Impl(misc::is_image_space_active)),
        (0x1240, "TriggerScreenSplatter", Stub("image space")),
        (0x1243, "SetScreenSplatterFade", Stub("image space")),
        (0x1241, "ClearScreenSplatter", Stub("image space")),
        (0x112D, "ForceWeather", Stub("weather")),
        (0x1158, "ReleaseWeatherOverride", Stub("weather")),
        (0x1186, "SetGlobalTimeMultiplier", Stub("time scale")),
        (0x1269, "SwapTexture", Stub("rendering")),
        (0x1225, "AutoDisplayObjectives", Stub("HUD")),
        (0x11DD, "ForceActiveQuest", Stub("Pip-Boy")),
        (0x115E, "Autosave", Stub("saving")),
        (0x11E1, "GetPlayerName", Impl(misc::get_player_name)),
        (0x10DA, "ShowRaceMenu", Impl(misc::show_race_menu)),
        (0x121A, "SetTagSkills", Impl(misc::set_tag_skills)),
        (0x1268, "ShowTraitMenu", Impl(misc::show_trait_menu)),
        (0x10A0, "GetFurnitureMarkerID", Impl(furniture::get_furniture_marker_id)),
        (0x1187, "GetHitLocation", Impl(actor::get_hit_location)),
        (0x106E, "GetCurrentAIPackage", Stub("AI packages")),
        (0x108F, "GetCurrentAIProcedure", Stub("AI packages")),
        (0x10A1, "GetIsCurrentPackage", Impl(actor::get_is_current_package)),
        (0x10A2, "IsCurrentFurnitureRef", Impl(furniture::is_current_furniture_ref)),
        (0x1065, "IsWeaponOut", Impl(actor::is_weapon_out)),
        (0x106C, "GetWeaponAnimType", Impl(actor::get_weapon_anim_type)),
        (0x111E, "IsSneaking", Stub("movement (sneaking)")),
        (0x1032, "GetTalkedToPC", Impl(misc::get_talked_to_pc)),
        (0x1245, "ShowLoveTesterMenuParams", Impl(misc::show_love_tester_menu)),
        (0x1173, "ShowBarterMenu", Impl(misc::show_barter_menu)),
        (0x1123, "PlayMagicShaderVisuals", Stub("rendering (effect shaders)")),
        (0x1129, "SetActorAlpha", Stub("rendering")),
        (0x10B2, "PlaySound3D", Impl(crate::sound::play_sound_3d)),
        (0x1198, "GetVATSValue", Stub("combat (VATS)")),
        (0x105A, "SetAlert", Impl(crate::awareness::set_alert)),
        (0x1042, "GetShouldAttack", Stub("combat")),
        (0x1275, "IncrementScriptedChallenge", Stub("challenges")),
        (0x1201, "ResetPipboyManager", Stub("Pip-Boy")),
        (0x122C, "ForceRadioStationUpdate", Impl(crate::radio::force_radio_station_update)),
        // Layer 6 systems added 2026-10-09 (function coverage round): recorded, no effect.
        (0x1128, "IsAnimPlaying", Impl(crate::anims::is_anim_playing)),
        (0x10DB, "GetAnimAction", Stub("animation")),
        (0x1070, "IsIdlePlaying", Impl(crate::idles::is_idle_playing)),
        (0x1064, "PickIdle", Stub("animation")),
        (0x111D, "IsLeftUp", Stub("animation")),
        (0x106A, "IsFacingUp", Stub("animation (ragdoll)")),
        (0x1004, "Rotate", Stub("movement (scripted rotation)")),
        (0x109F, "GetSitting", Impl(furniture::get_sitting)),
        (0x10A3, "IsCurrentFurnitureObj", Impl(furniture::is_current_furniture_obj)),
        (0x1031, "GetSleeping", Impl(furniture::get_sleeping)),
        (0x105B, "GetIsAlerted", Impl(crate::awareness::get_is_alerted)),
        (0x10F5, "ForceFlee", Stub("AI")),
        (0x115B, "SetNoAvoidance", Stub("AI")),
        (0x118B, "SetActorsAI", Stub("AI")),
        (0x1122, "SetPackDuration", Stub("AI packages")),
        (0x10F6, "GetIsUsedItem", Stub("AI (used items)")),
        (0x10F7, "GetIsUsedItemType", Stub("AI (used items)")),
        (0x11E0, "GetIsUsedItemEquipType", Stub("AI (used items)")),
        (0x1103, "GetUsedItemActivate", Stub("AI (used items)")),
        (0x107E, "StartCannibal", Stub("AI (Cannibal perk)")),
        (0x10E3, "GetCannibal", Stub("AI (Cannibal perk)")),
        (0x107F, "HasBeenEaten", Stub("AI (Cannibal perk)")),
        (0x107C, "StartMisterSandman", Stub("AI (Mister Sandman perk)")),
        (0x10E2, "GetSandman", Stub("AI (Mister Sandman perk)")),
        (0x1101, "StopCombatAlarmOnActor", Impl(crate::awareness::stop_combat_alarm_on_actor)),
        (0x10B1, "SetCombatStyle", Stub("combat")),
        (0x11A0, "GetGroupMemberCount", Stub("combat (groups)")),
        (0x11A1, "GetGroupTargetCount", Stub("combat (groups)")),
        (0x10E8, "GetCombatTarget", Impl(crate::awareness::get_combat_target)),
        (0x1203, "IsCombatTarget", Stub("combat")),
        (0x11E2, "FireWeapon", Stub("combat")),
        (0x119C, "UseWeapon", Stub("combat")),
        (0x103F, "GetAttacked", Impl(crate::provocation::get_attacked)),
        (0x1120, "GetFriendHit", Impl(crate::provocation::get_friend_hit)),
        (0x106B, "GetKnockedState", Stub("combat (knockdown)")),
        (0x118E, "IsLimbGone", Stub("combat (dismemberment)")),
        (0x118D, "GetCauseofDeath", Stub("combat")),
        (0x1259, "GetForceHitReaction", Stub("combat")),
        (0x1051, "GetArmorRating", Stub("combat (armor rating)")),
        (0x11AC, "GetPlantedExplosive", Stub("combat (explosives)")),
        (0x124D, "DetonatePlacedExplosives", Impl(actor::detonate_placed_explosives)),
        (0x11EB, "SetVatsTarget", Stub("combat (VATS)")),
        (0x10EB, "GetVATSTargetHeight", Stub("combat (VATS)")),
        (0x10E0, "GetVATSMode", Stub("combat (VATS)")),
        (0x102D, "GetDetected", Impl(crate::detection::get_detected)),
        (0x10B4, "GetDetectionLevel", Impl(crate::detection::get_detection_level)),
        (0x101B, "GetLineOfSight", Stub("detection (line of sight)")),
        (0x115A, "SetAllVisible", Stub("detection")),
        (0x1068, "CreateDetectionEvent", Impl(crate::awareness::create_detection_event)),
        (0x105F, "SendAssaultAlarm", Stub("crime")),
        (0x1090, "GetTrespassWarningLevel", Stub("crime")),
        (0x1091, "IsTrespassing", Stub("crime")),
        (0x103D, "GetAlarmed", Stub("crime")),
        (0x1075, "GetMajorCrimeCount", Stub("crime")),
        (0x124E, "GetOwnerLastTarget", Stub("crime")),
        (0x11AE, "IsActorTalkingThroughActivator", Stub("dialogue (talking activators)")),
        (0x11B3, "GetDialogueEmotion", Impl(misc::get_dialogue_emotion)),
        (0x104C, "GetDisposition", Stub("dialogue (disposition)")),
        (0x1277, "AlwaysShowActorSubtitles", Stub("HUD (subtitles)")),
        (0x1263, "PlaceAtReticle", Stub("HUD (reticle)")),
        (0x1019, "IsMoving", Stub("movement")),
        (0x101A, "IsTurning", Stub("movement")),
        (0x111F, "IsRunning", Stub("movement")),
        (0x10B9, "IsSwimming", Stub("movement (swimming)")),
        (0x10D3, "SetForceSneak", Stub("movement (sneaking)")),
        (0x1166, "IsPlayerMovingIntoNewSpace", Stub("movement")),
        (0x1170, "IsPlayerActionActive", Stub("player input")),
        (0x1188, "IsPC1stPerson", Stub("camera")),
        (0x118A, "PushActorAway", Stub("physics (ragdoll)")),
        (0x11D0, "IsPlayerGrabbedRef", Stub("physics (grab)")),
        (0x110C, "trapUpdate", Stub("physics (traps)")),
        (0x11C0, "EnableNavMesh", Stub("navigation")),
        (0x11BF, "DisableNavmesh", Stub("navigation")),
        (0x1159, "setAllReachable", Stub("navigation")),
        (0x1125, "StopMagicShaderVisuals", Stub("rendering (effect shaders)")),
        (0x1248, "SetSecuritronExpression", Stub("rendering")),
        (0x1146, "Reset3DState", Stub("rendering")),
        (0x1211, "AttachAshPile", Stub("rendering (death effects)")),
        (0x115D, "SetSceneIsComplex", Stub("rendering")),
        (0x1196, "SetCellImageSpace", Stub("image space")),
        (0x1104, "SetWeather", Stub("weather")),
        (0x103E, "IsRaining", Stub("weather")),
        (0x1095, "GetIsCurrentWeather", Stub("weather")),
        (0x11F7, "GetRadiationLevel", Stub("radiation (environment)")),
        (0x11D6, "StartRadioConversation", Impl(crate::radio::start_radio_conversation)),
        (0x1224, "PipBoyRadioOff", Impl(crate::radio::pipboy_radio_off)),
        (0x11A7, "PipBoyRadio", Impl(crate::radio::pipboy_radio)),
        (0x11C4, "SetNPCRadio", Impl(crate::radio::set_npc_radio)),
        (0x11FB, "SetRumble", Stub("controller rumble")),
        (0x1189, "PurgeCellBuffers", Stub("memory (cell buffers)")),
        (0x1163, "PreloadMagicEffect", Stub("loading")),
        (0x1223, "ForceTerminalBack", Stub("menus (terminal)")),
        (0x125A, "ShowCaravanMenu", Stub("menus (Caravan)")),
        (0x125B, "AddCardToPlayer", Stub("Caravan")),
        (0x11C8, "OpenTeammateContainer", Stub("menus (companion inventory)")),
        (0x1219, "AddSPECIALPoints", Stub("menus (character creation)")),
        (0x1218, "SetSPECIALPoints", Stub("menus (character creation)")),
        (0x117A, "ShowRepairMenu", Stub("menus (repair)")),
        (0x1250, "ShowRecipeMenu", Stub("menus (crafting)")),
        (0x1254, "ShowBlackJackMenuParams", Stub("menus (casino)")),
        (0x1256, "ShowRouletteMenuParams", Stub("menus (casino)")),
        (0x1252, "ShowSlotMachineMenuParams", Stub("menus (casino)")),
        (0x11F1, "ShowBarberMenu", Stub("menus (appearance)")),
        (0x11F2, "ShowPlasticSurgeonMenu", Stub("menus (appearance)")),
        (0x11E3, "ShowTutorialMenu", Stub("menus (tutorial)")),
        (0x1279, "ShowSleepWaitMenu", Impl(crate::rest::show_sleep_wait_menu)),
        (0x1270, "EnableLoadingMenu", Stub("menus (loading)")),
        (0x11F8, "ShowAllMapMarkers", Stub("Pip-Boy (map markers)")),
        (0x10AE, "WakeUpPC", Impl(crate::rest::wake_up_pc)),
        (0x10BD, "SetPCSleepHours", Impl(crate::rest::set_pc_sleep_hours)),
        (0x10AF, "IsPCSleeping", Impl(crate::rest::is_pc_sleeping)),
        (0x1109, "IsTimePassing", Impl(crate::rest::is_time_passing)),
        (0x120A, "GetIsLockBroken", Stub("lockpicking")),
        (0x127B, "SystemSave", Stub("saving")),
        (0x1217, "ForceSave", Stub("saving")),
        (0x11B5, "ExitGame", Stub("application")),
        (0x121B, "AddTagSkills", Stub("menus (character creation)")),
        (0x119F, "Exists", Stub("dialogue (combat dialogue)")),
        (0x1130, "IsWaterObject", Stub("AI (drinking)")),
    ]
}

fn index() -> &'static HashMap<u16, usize> {
    static IX: OnceLock<HashMap<u16, usize>> = OnceLock::new();
    IX.get_or_init(|| registry().iter().enumerate().map(|(i, r)| (r.0, i)).collect())
}

/// The implementation registered for an opcode.
pub fn lookup(opcode: u16) -> Option<&'static Kind> {
    index().get(&opcode).map(|&i| &registry()[i].2)
}

/// Whether an opcode is implemented (stubs count as handled, not implemented).
pub fn implemented(opcode: u16) -> bool {
    matches!(lookup(opcode), Some(Kind::Impl(_)))
}

pub fn is_stub(opcode: u16) -> bool {
    matches!(lookup(opcode), Some(Kind::Stub(_)))
}

pub(crate) fn yes(b: bool) -> Result<Value, String> {
    Ok(Value::bool(b))
}

pub(crate) fn none() -> Result<Value, String> {
    Ok(Value::ZERO)
}

/// The base form of a reference.
/// The destruction stage of a reference (−1 without data or damage).
pub(crate) fn objects_destruction_stage(g: &Game, r: FormId) -> i32 {
    objects::destruction_stage(g, r)
}

/// Disable a reference as `Disable` does.
pub(crate) fn objects_disable(g: &mut Game, r: FormId) {
    objects::disable_ref(g, r)
}

/// An actor's current health (Health's permanent part minus its damage).
pub fn actor_health(g: &mut Game, actor: FormId) -> Result<f64, String> {
    actor::actor_value(g, actor, crate::combat::HEALTH)
}

pub(crate) fn base_of(g: &Game, r: FormId) -> Option<FormId> {
    match g.world.peek(r) {
        Some(s) => Some(s.base),
        None if r == FormId::PLAYER_REF => Some(FormId::PLAYER_BASE),
        None => g.world.placed.get(r).map(|p| p.base),
    }
}
