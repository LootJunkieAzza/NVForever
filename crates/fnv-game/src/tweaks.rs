//! The player's tweaks (docs/design/tweaks.md): optional changes to how the game plays
//! or what it shows, off unless the player turns them on in the settings page's Tweaks
//! tab. The list follows the features of a popular script-extender plugin for the
//! original (Stewie's Tweaks), each built here from what its readme says the feature
//! does, in NVForever's own words and code; none of the plugin's code or files are used.
//! The engine keeps the chosen values and hands them to the running game each frame
//! (`nvf_play::tweaks`), as it does the difficulty; they are settings, not saved with a
//! game.
//!
//! A field's name is the plugin's INI key it stands for, in snake case, so the catalogue
//! in docs/design/tweaks.md can be followed both ways.

/// Every tweak's value. `Default` is the original's behaviour: every switch off, every
/// number at the value that changes nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Tweaks {
    // ---- experience ----
    /// `bReduceXP`/`fXPMultiplier`: every experience gain is multiplied by this (after
    /// the perks), 1 changes nothing.
    pub xp_multiplier: f64,
    // ---- items ----
    /// `bPowerArmorNeedsNoTraining`: power armour can be worn without the training perk.
    pub power_armor_needs_no_training: bool,
    // ---- rest ----
    /// `bSleepInOwnedBeds`: a bed that belongs to someone else does not refuse sleep.
    pub sleep_in_owned_beds: bool,
    // ---- screen ----
    /// `bRemoveChemWarnOffIMOD`: a chem wearing off plays no screen effect.
    pub no_chem_wore_off_effect: bool,
    // ---- menus (read through the game by the engine's menus) ----
    /// `bNoSkillMessageIfIncreaseIsZero`: reading a skill book that gives no points shows
    /// no message.
    pub no_book_message_without_gain: bool,
    /// `bCapitalizeRecipeCategories`: the crafting menu's categories in capitals.
    pub capitalize_recipe_categories: bool,
    /// `bShowWeaponAmmoUseInMenus`: a weapon that uses more than one round a shot says
    /// how many beside its ammo in the Pip-Boy.
    pub show_weapon_ammo_use: bool,
    /// `bShowBookEffects`: a skill book says what it teaches in the Pip-Boy.
    pub show_book_effects: bool,
    /// `bSkipSkillMenuIfNoPointsToAssign`: a level-up with no skill points to spend but
    /// a perk to choose opens on the perks.
    pub skip_skill_page_without_points: bool,
    /// `bFasterHackingTransition`: a successful hack opens the terminal sooner.
    pub faster_hacking_transition: bool,
    /// `bMapRemembersPosition`: the Pip-Boy map opens where it was left.
    pub remember_map_position: bool,
    // ---- character ----
    /// `iPerksPerLevel`: perks chosen at a level-up's perk page; 1 changes nothing.
    pub perks_per_level: usize,
    /// `bCustomSpecialPoints`/`iNumPointsToAllocate`: the S.P.E.C.I.A.L. menu's total
    /// when no script set one; 40 changes nothing.
    pub special_points: i32,
    // ---- locks, terminals, casinos ----
    /// `bPickLocksWithoutSkill`: a lock above the player's skill can be picked.
    pub pick_locks_without_skill: bool,
    /// `bHackTerminalsWithoutSkill`: a terminal above the player's skill can be hacked.
    pub hack_terminals_without_skill: bool,
    /// `bNoLockFailedTerminals`: a failed hack does not lock the terminal.
    pub no_terminal_lockout: bool,
    /// `bNoCasinoBans`: winning big never bans the player from a casino.
    pub no_casino_bans: bool,
    // ---- actors ----
    /// `bLootUnconsciousVictims`: an unconscious actor can be searched.
    pub loot_unconscious: bool,
    /// `bPickpocketWornItems`: what an actor wears can be pickpocketed.
    pub pickpocket_worn_items: bool,
    // ---- combat ----
    /// `bNoSneakAttacksCriticals`: the player's sneak attacks are not criticals.
    pub no_sneak_criticals: bool,
    /// `bDisableReloadingNonEmptyClip`: the player reloads only an empty magazine.
    pub reload_only_when_empty: bool,
    // ---- weight ----
    /// `bWeightlessItems`: item kinds that weigh nothing (`fnv_sim::inventory`'s
    /// `WEIGHTLESS_*` bits).
    pub weightless_kinds: u8,
    /// `bWeightlessWornArmor`: worn armour weighs nothing.
    pub weightless_worn_armor: bool,
    /// `bWeightlessWornPowerArmor`: worn power armour weighs nothing.
    pub weightless_worn_power_armor: bool,
    // ---- Action Points ----
    /// `bKillsRewardAP`/`iKillRewardAmount`: Action Points a kill by the player gives
    /// back; 0 changes nothing.
    pub kill_reward_ap: f64,
    // ---- rest ----
    /// `bOnlyAllowWaitWhileSitting`: waiting is refused unless the player sits (or is in
    /// god mode).
    pub wait_only_seated: bool,
    /// `bModifySkillPointsEarned`: a level-up's skill points are `base` + Intelligence ×
    /// `per_int` (rounded up) instead of the game's rule; `None` changes nothing.
    pub skill_points: Option<(i32, f64)>,
    // ---- batch 9 ----
    /// `bNoUnconciousMessage`: no "<name> is unconscious." line.
    pub no_unconscious_message: bool,
    /// `bNoAudioDistortion`: no helmet or speaker filter on voices.
    pub no_voice_filter: bool,
    /// `bNoSkipDialog`: a click does not cut a spoken line short.
    pub no_skip_dialogue: bool,
    /// `bLockpickAllowMouse`: the left mouse button turns the screwdriver too.
    pub lockpick_mouse_turn: bool,
    /// `bNoMaxCasinoBet`: the casino games take any bet the player can pay.
    pub no_max_casino_bet: bool,
    /// `bEnteringVATSCostsAP`/`iEnterVATSAPCost`: Action Points entering V.A.T.S.
    /// costs; 0 changes nothing.
    pub vats_entry_ap: f64,
    // ---- batch 11 ----
    /// `bDontShowContainerAfterLockpick`: a picked container is not opened at once.
    pub no_container_after_pick: bool,
    /// `bDontOpenDoorsAfterLockpick`: a picked door is not used at once.
    pub no_door_after_pick: bool,
    /// `bPatchUnseenCellName`: a door to a place never visited ends its prompt with ` *`.
    pub mark_unvisited_doors: bool,
    /// `bNoExitHacking`: the hacking game cannot be left once a guess is made.
    pub no_exit_hacking: bool,
    /// `bNoGrabOwnedItems`: someone else's things cannot be grabbed.
    pub no_grab_owned: bool,
    // ---- batch 12 ----
    /// `bAgilityScalesMovementSpeed`/`fAgilityMovementSpeedMult`: every actor's speed
    /// gains this share per point of Agility above 5 (loses it below); 0 changes nothing.
    pub agility_speed_per_point: f64,
    /// `bAutoUnlockTerminals`: a terminal opens to its menu without the hacking game when
    /// the player's Science is at least its lock plus `auto_unlock_margin`.
    pub auto_unlock_terminals: bool,
    pub auto_unlock_margin: f32,
    /// `bHittingWeaponsDoesntDamageThem`: a shot on a weapon in hand does not wear it.
    pub weapons_unhurt_by_hits: bool,
    /// `bGodModePreventsJamAnims`: the player's weapon never jams in god mode.
    pub god_mode_no_jams: bool,
    /// `bExplosionKnockdownAvoidanceUsesStrength`: Strength, not Agility, resists an
    /// explosion's knockdown.
    pub knockdown_by_strength: bool,
    /// `bDrowningDrainsAP`: drowning takes Action Points before health.
    pub drowning_drains_ap: bool,
    // ---- batch 13 ----
    /// `bAllowAidAtMaxHealth`: the STATS page's Stimpak and Doctor's Bag are used at full
    /// health and with no limb hurt.
    pub aid_at_full_health: bool,
    /// `bGrabbingItemsIsCrime`: grabbing someone else's thing counts as stealing it.
    pub grabbing_is_theft: bool,
    /// `bLockNeedsKeyShowName`: "requires a key" names the key.
    pub lock_needs_key_name: bool,
    /// `bNoSkillTags`: dialogue topics show no [Speech 50]-style tag.
    pub no_skill_tags: bool,
    /// `bPreventNPCAddiction`: only the player becomes addicted.
    pub no_npc_addiction: bool,
    /// `bPickpocketFleeingActors`: a fleeing actor can be pickpocketed.
    pub pickpocket_fleeing: bool,
    /// `bReloadingWithFullClipSwitchesAmmoType`: reloading a full magazine swaps ammo.
    pub full_reload_swaps_ammo: bool,
    /// `bScalePlayerMeleeReach`/`fReachMult`: the player's melee reach multiplier.
    pub melee_reach_mult: f32,
    /// `bRepairsRewardXP`/`iRepairRewardXP`: experience a Pip-Boy repair gives.
    pub repair_xp: f64,
    /// `bPreventStealingCapsAfterRepair`: a repairer's caps go into its merchant container.
    pub repair_caps_to_container: bool,
    /// `bNoDamageMeleeWeaponIfTargetDead`: hitting only the dead does not wear the weapon.
    pub no_wear_on_dead: bool,
    /// `bNoAPRegenWhileOverencumbered`/`fOverencumberedAPRegenScale`: Action Points come
    /// back this much as fast while over-encumbered; 1 changes nothing.
    pub encumbered_ap_regen: f64,
    /// `bLightStepAffectsCompanions`: teammates are spared mines when the player is.
    pub light_step_companions: bool,
    /// `bReloadJamsAffectedByAgility`: a jam clears at the reload's own pace.
    pub jam_at_reload_pace: bool,
    /// `bSleepOnChairs`: waiting on a chair is sleeping.
    pub sleep_on_chairs: bool,
    /// `bStealingSendsNoAlarm`: taking someone's things raises no alarm (pickpocketing
    /// still does).
    pub stealing_no_alarm: bool,
    /// `bNoCompanionKillXP`: a teammate's kills give the player no experience.
    pub no_companion_kill_xp: bool,
    /// `bNoDisarmCompanions`: a crippled arm does not disarm a teammate.
    pub no_disarm_companions: bool,
    /// `bNoSelfExplosionDamage`: the player's own blasts do not hurt the player.
    pub no_self_explosion_damage: bool,
    /// `bSneakCriticalsOnlyMeleeWeapons`: sneak criticals with melee only.
    pub sneak_criticals_melee_only: bool,
    /// `bAllowTeammatesUseMeltdown`: teammates' energy kills use the player's Meltdown.
    pub teammates_meltdown: bool,
    /// `bSpeedMultScalesJumpHeight`: the player's Speed Mult scales the jump.
    pub speed_scales_jump: bool,
}

impl Default for Tweaks {
    fn default() -> Tweaks {
        Tweaks {
            xp_multiplier: 1.0,
            power_armor_needs_no_training: false,
            sleep_in_owned_beds: false,
            no_chem_wore_off_effect: false,
            no_book_message_without_gain: false,
            capitalize_recipe_categories: false,
            show_weapon_ammo_use: false,
            show_book_effects: false,
            skip_skill_page_without_points: false,
            faster_hacking_transition: false,
            remember_map_position: false,
            perks_per_level: crate::levelup::PERKS_PER_LEVEL,
            special_points: crate::chargen::SPECIAL_POINTS,
            pick_locks_without_skill: false,
            hack_terminals_without_skill: false,
            no_terminal_lockout: false,
            no_casino_bans: false,
            loot_unconscious: false,
            pickpocket_worn_items: false,
            no_sneak_criticals: false,
            reload_only_when_empty: false,
            weightless_kinds: 0,
            weightless_worn_armor: false,
            weightless_worn_power_armor: false,
            kill_reward_ap: 0.0,
            wait_only_seated: false,
            skill_points: None,
            no_unconscious_message: false,
            no_voice_filter: false,
            no_skip_dialogue: false,
            lockpick_mouse_turn: false,
            no_max_casino_bet: false,
            vats_entry_ap: 0.0,
            no_container_after_pick: false,
            no_door_after_pick: false,
            mark_unvisited_doors: false,
            no_exit_hacking: false,
            no_grab_owned: false,
            agility_speed_per_point: 0.0,
            auto_unlock_terminals: false,
            auto_unlock_margin: 0.0,
            weapons_unhurt_by_hits: false,
            god_mode_no_jams: false,
            knockdown_by_strength: false,
            drowning_drains_ap: false,
            aid_at_full_health: false,
            grabbing_is_theft: false,
            lock_needs_key_name: false,
            no_skill_tags: false,
            no_npc_addiction: false,
            pickpocket_fleeing: false,
            full_reload_swaps_ammo: false,
            melee_reach_mult: 1.0,
            repair_xp: 0.0,
            repair_caps_to_container: false,
            no_wear_on_dead: false,
            encumbered_ap_regen: 1.0,
            light_step_companions: false,
            jam_at_reload_pace: false,
            sleep_on_chairs: false,
            stealing_no_alarm: false,
            no_companion_kill_xp: false,
            no_disarm_companions: false,
            no_self_explosion_damage: false,
            sneak_criticals_melee_only: false,
            teammates_meltdown: false,
            speed_scales_jump: false,
        }
    }
}
