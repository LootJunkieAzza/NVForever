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

mod actor;
mod items;
mod misc;
mod player;
mod quest;
mod refs;

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
        // Items.
        (0x1002, "AddItem", Impl(items::add_item)),
        (0x1052, "RemoveItem", Impl(items::remove_item)),
        (0x102F, "GetItemCount", Impl(items::get_item_count)),
        (0x10AD, "RemoveAllItems", Impl(items::remove_all_items)),
        (0x10EE, "EquipItem", Impl(items::equip_item)),
        (0x10EF, "UnequipItem", Impl(items::unequip_item)),
        (0x10B6, "GetEquipped", Impl(items::get_equipped)),
        // Actors.
        (0x100E, "GetActorValue", Impl(actor::get_actor_value)),
        (0x100F, "SetActorValue", Impl(actor::set_actor_value)),
        (0x1010, "ModActorValue", Impl(actor::mod_actor_value)),
        (0x110E, "ForceActorValue", Impl(actor::set_actor_value)),
        (0x1181, "DamageActorValue", Impl(actor::damage_actor_value)),
        (0x1182, "RestoreActorValue", Impl(actor::mod_actor_value)),
        (0x1115, "GetBaseActorValue", Impl(actor::get_base_actor_value)),
        (0x11EF, "GetPermanentActorValue", Impl(actor::get_base_actor_value)),
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
        (0x1121, "IsInCombat", Impl(misc::zero)),
        (0x108D, "IsTalking", Impl(misc::zero)),
        // Layer 6 systems: recorded, no effect.
        (0x105E, "EvaluatePackage", Stub("AI packages")),
        (0x1097, "AddScriptPackage", Impl(actor::add_script_package)),
        (0x1098, "RemoveScriptPackage", Impl(actor::remove_script_package)),
        (0x11FA, "ResetAI", Stub("AI packages")),
        (0x1034, "SayTo", Stub("dialogue")),
        (0x1033, "Say", Stub("dialogue")),
        (0x1056, "StartConversation", Stub("dialogue")),
        (0x1016, "StartCombat", Stub("combat")),
        (0x1017, "StopCombat", Stub("combat")),
        (0x1078, "SetEnemy", Stub("combat (faction relations)")),
        (0x1079, "SetAlly", Stub("combat (faction relations)")),
        (0x1184, "SetDisposition", Stub("dialogue (disposition)")),
        (0x115C, "SendTrespassAlarm", Stub("crime")),
        (0x10AB, "IgnoreCrime", Stub("crime")),
        (0x1013, "PlayGroup", Stub("animation")),
        (0x1190, "PlayIdle", Stub("animation")),
        (0x105C, "Look", Stub("animation (head tracking)")),
        (0x105D, "StopLook", Stub("animation (head tracking)")),
        (0x11E4, "AgeRace", Stub("character appearance")),
        (0x1026, "PlaySound", Stub("sound")),
        (0x11FF, "PlayMusic", Stub("sound")),
        (0x1114, "PlayBink", Stub("video")),
        (0x1191, "ApplyImageSpaceModifier", Stub("image space")),
        (0x1192, "RemoveImageSpaceModifier", Stub("image space")),
        (0x1232, "IsImageSpaceActive", Stub("image space")),
        (0x1240, "TriggerScreenSplatter", Stub("image space")),
        (0x1243, "SetScreenSplatterFade", Stub("image space")),
        (0x1241, "ClearScreenSplatter", Stub("image space")),
        (0x112D, "ForceWeather", Stub("weather")),
        (0x1158, "ReleaseWeatherOverride", Stub("weather")),
        (0x1186, "SetGlobalTimeMultiplier", Stub("time scale")),
        (0x113C, "SetScale", Stub("rendering")),
        (0x1269, "SwapTexture", Stub("rendering")),
        (0x1225, "AutoDisplayObjectives", Stub("HUD")),
        (0x11DD, "ForceActiveQuest", Stub("Pip-Boy")),
        (0x115E, "Autosave", Stub("saving")),
        (0x11E1, "GetPlayerName", Stub("menus (name entry)")),
        (0x10DA, "ShowRaceMenu", Stub("menus (race menu)")),
        (0x121A, "SetTagSkills", Stub("menus (tag skills)")),
        (0x1268, "ShowTraitMenu", Stub("menus (traits)")),
        (0x10A0, "GetFurnitureMarkerID", Stub("AI (furniture)")),
        (0x1187, "GetHitLocation", Stub("combat")),
        (0x106E, "GetCurrentAIPackage", Stub("AI packages")),
        (0x108F, "GetCurrentAIProcedure", Stub("AI packages")),
        (0x10A1, "GetIsCurrentPackage", Stub("AI packages")),
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
pub(crate) fn base_of(g: &Game, r: FormId) -> Option<FormId> {
    match g.world.peek(r) {
        Some(s) => Some(s.base),
        None if r == FormId::PLAYER_REF => Some(FormId::PLAYER_BASE),
        None => g.world.placed.get(r).map(|p| p.base),
    }
}
