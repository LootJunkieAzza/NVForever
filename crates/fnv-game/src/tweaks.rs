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
    /// `bDisableTutorialMessages`: no first-time help boxes for the minigames.
    pub no_tutorials: bool,
    /// `bNoKnockdownInGodmode`: the player is never knocked down in god mode.
    pub no_knockdown_in_god_mode: bool,
    /// `bLockpickHackingMessageShowsCurrentSkill`: "skill too low" says the player's.
    pub show_current_skill: bool,
    /// `bUnspokenNPCIndicator`: an actor not yet spoken to ends its prompt with ` *`.
    pub mark_unspoken: bool,
    /// `bHideCapsAddedMessages`: no "Caps added" line.
    pub no_caps_added_message: bool,
    /// `bSubtitlesShowActorNames`: general subtitles name the speaker.
    pub subtitle_names: bool,
    // ---- batch 17: menus ----
    /// `bDialogueKeepVoiceActingNotes`: spoken lines keep their {acting notes}.
    pub keep_acting_notes: bool,
    /// `bHideCompletedQuests`: the Pip-Boy lists only quests under way.
    pub hide_completed_quests: bool,
    /// `bHideMapMarkerFactionReputation`: no reputation line under map markers.
    pub hide_marker_reputation: bool,
    /// `bDisableShowQuestLocation`: no Show Location button on the quests page.
    pub no_show_quest_location: bool,
    /// `bHideRanksInTraitMenu`: no Ranks line in the trait menu.
    pub hide_trait_ranks: bool,
    /// `bHideQuestItemWeightAndValue`: quest items and keys show no weight or value.
    pub hide_quest_item_figures: bool,
    /// `bIndividialItemStats`: a stack shows one item's weight and value.
    pub single_item_figures: bool,
    /// `bShowAlternateAmmoTypesAvailableInMenus`: a `+` when other ammo is carried.
    pub mark_other_ammo: bool,
    /// `bReputationShowsFameInfamy`: reputations show their fame and infamy.
    pub show_fame_infamy: bool,
    // ---- batch 19 ----
    /// `bShowQuestAndNoDRDTItemsInRepairMenu`: a repairer lists armour with no DT too.
    pub repair_any_armor: bool,
    /// `bSortMiscStats`: the STATS page's misc statistics in name order.
    pub sort_misc_stats: bool,
    /// `bStatsMenuShowEffectTimeRemaining`: effects show the seconds left.
    pub effect_time_left: bool,
    /// `bShowModNameInPerkDescriptions`: a perk's description names its plugins.
    pub perk_plugin_names: bool,
    /// `bCustomAddToInventoryMessageTimer`/`fAddItem`: seconds an "added" line stays;
    /// 0 changes nothing.
    pub item_added_seconds: f32,
    /// `b12HourPipboyClock`: the Pip-Boy's clock in 12 hours.
    pub pipboy_12_hour: bool,
    /// `bShowActiveQuestNotesShowsAllStartedQuests`: Show Active Quest Notes takes in
    /// every quest under way.
    pub notes_of_all_quests: bool,
    /// `bAllowPickpocketIfAlreadyCaught`: someone who caught the player can be tried again.
    pub pickpocket_after_caught: bool,
    /// `bSleepWaitAnywhere`: only a script's ban refuses waiting or sleeping.
    pub wait_anywhere: bool,
    // ---- batch 21 ----
    /// `bDetachedBeams`: the player's beams do not stay on the barrel as it turns.
    pub detached_beams: bool,
    /// `bDisableGodmodeOnLoad`: loading a save turns god mode off.
    pub god_mode_off_on_load: bool,
    /// `bDisableStealthEffectInPipboy`: no stealth shimmer while the Pip-Boy is up.
    pub no_stealth_shimmer_in_pipboy: bool,
    /// `bPreventTeammateFootstepSounds`: teammates' feet are silent.
    pub quiet_teammate_steps: bool,
    // ---- batch 22 ----
    /// `bLevelDifferenceAffectsCombatXP`: kill experience gains this share for each
    /// level the target is above the player (loses it below); 0 changes nothing.
    pub kill_xp_per_level: f64,
    /// `bUsingKeysRewardsXP`: opening a lock with its key gives this share of picking
    /// it; 0 changes nothing.
    pub key_xp_share: f32,
    /// `bUsingNotesRewardsXP`: opening a terminal with its password gives this share of
    /// hacking it, once; 0 changes nothing.
    pub password_xp_share: f32,
    // ---- batch 24 ----
    /// `bNoEnvironmentRadiationInFullPowerArmor`: power armour on the body keeps the
    /// world's radiation off (water and chems still count).
    pub power_armor_blocks_radiation: bool,
    /// `bPowerArmorScalesLimbDamage`: limb damage to anyone in power armour is
    /// multiplied by this; 1 changes nothing.
    pub power_armor_limb_damage: f64,
    /// `bEnduranceScalesMinFallHeight`/`fEnduranceMult`: the player's safe fall height
    /// gains this share per point of Endurance above 5 (loses it below); 0 changes
    /// nothing.
    pub fall_height_per_endurance: f32,
    // ---- batch 27 ----
    /// `bSkipVeryEasyLocksAtMaxSkill`: a lock at least `skip_lock_margin` under the
    /// player's Lockpick opens without the game (its experience given).
    pub skip_easy_locks: bool,
    pub skip_lock_margin: f32,
    /// `bDisarmingMinesPicksThemUp`: a mine the player disarms goes into the inventory.
    pub pick_up_disarmed: bool,
    // ---- batch 28 ----
    /// `bNoScaleNpcDamageByCondition`: NPCs' weapons hit as if whole.
    pub npc_weapons_whole: bool,
    /// `bLethalHitsIncreaseLimbChallenges`: a killing knockdown still counts for the
    /// knockdown challenges.
    pub lethal_knockdowns_count: bool,
    /// `bCripplingDoesntDisarm`: a crippled arm never makes an actor drop its weapon.
    pub crippling_never_disarms: bool,
    /// `bAutoWeaponNoFiringDelay`: the player's automatic weapons fire without spinning up.
    pub no_spin_up: bool,
    /// `bGodModeAllowsCraftingWithoutRequirements`: in god mode recipes need nothing.
    pub god_mode_free_crafting: bool,
    /// `bCustomSubtitleDistance`/`fSubtitleDistance`: general subtitles show within this
    /// many units; 0 changes nothing (the voices' own reach).
    pub subtitle_distance: f32,
    /// `bSneakCriticalsHeadshotsOnly`: sneak criticals only on the head.
    pub sneak_criticals_head_only: bool,
    /// `bCompareWeaponStats`: a weapon's DAM and DPS in the Pip-Boy show the difference
    /// from the weapon in hand.
    pub compare_weapons: bool,
    /// `bMarkNearbyLocationsOnMap`/`fDistance`: map markers within this many units of
    /// the player are marked on the map (as a ShowMap would); 0 changes nothing.
    pub mark_nearby_places: f32,
    /// `bScaleAshpileSize`: an ash pile takes its actor's scale (0.5 to 2).
    pub scale_ash_piles: bool,
    /// `bReloadWhileFiring`: a reload may start during the last shot's recovery.
    pub reload_while_firing: bool,
    /// `bSemiAutoQueue`: a semi-automatic pull this many seconds before the weapon is
    /// ready again is kept and fired then; 0 changes nothing.
    pub semi_auto_queue: f32,
    /// `bRepairScalesCraftingCondition`: a weapon or armour made at a workbench comes out
    /// at [`crafted_condition`] of the player's Repair.
    pub repair_scales_crafting: bool,
    /// `bNoHealingInCombat`: aid that restores health or a limb is refused in a fight.
    pub no_healing_in_combat: bool,
    /// `bSlowBreathRegen` (`fBreathRegainRate`): seconds of breath a second at the
    /// surface gives the player back; 0 keeps the whole breath at once.
    pub breath_regain: f32,
    /// `bPickpocketCaughtCausesDetection`: the victim that catches the player detects them.
    pub pickpocket_caught_detects: bool,
    /// `bAgilityScalesJumpHeight` (`fAgilityMult`): the jump height is 1 + this x
    /// (Agility - 1) times its own; 0 leaves it.
    pub agility_jump_mult: f32,
    /// `bLockpickForceBonusNearSweetSpot`: percent added to Force Lock with the pin on the
    /// sweet spot's middle, falling to none 90 degrees away.
    pub lockpick_force_near_bonus: f32,
    /// `bContactMines` (`bPlayer`, `bNPCs`): a mine stood on goes off with no fuse.
    pub contact_mines_player: bool,
    pub contact_mines_npcs: bool,
    /// `bSleepHealingMinDuration` (`iMinDuration`): a sleep heals Health and limbs only
    /// from this hour slept on; 0 heals from the first.
    pub sleep_heal_min_hours: u32,
    /// `bBarterAffectsRepairCosts`: percent of a vendor's repair price the player's
    /// Barter 100 takes off (less in proportion below; [`barter_repair_factor`]).
    pub barter_repair_discount: f32,
    /// `bCustomArmorConditionPenalty` (`fScale`): the multiplier on how fast armour below
    /// half condition loses its DT and DR; 1 is the original's.
    pub armor_condition_penalty: f64,
    /// `bIgnoreCompanionsVATS`: companions cannot be marked in V.A.T.S.
    pub vats_ignores_companions: bool,
    /// `bFrenzyGrantsPlayerXP`: a frenzied actor's kills earn the player experience.
    pub frenzy_kill_xp: bool,
    /// `bFreeWildWasteland`: Wild Wasteland does not count against the trait limit.
    pub free_wild_wasteland: bool,
    /// `bStatsMenuPercentageXP`: the Stats page shows experience as the way through the
    /// level, in percent.
    pub stats_xp_percent: bool,
    /// `bSortPipboyRepairMenu` (`bMatchedItemFirst`): parts that are the item repaired
    /// itself come first in the Pip-Boy repair list.
    pub repair_same_first: bool,
    /// `bInvertPipboyRepairMenuSorting`: lower condition parts first, high at the bottom.
    pub repair_low_first: bool,
    /// `bNoSkillBooksAbove100`: a skill book whose skill is already at 100 is not read.
    pub no_skill_books_at_max: bool,
    /// `bLessFrequentPlayerPainSounds` (`iMinPlayerPainSoundIntervalMS`): seconds the
    /// player's pain line waits before it may come again; 0 as the original.
    pub player_pain_gap: f64,
    /// `bPauseOnSaveLoad`: a loaded save starts in the pause menu.
    pub pause_on_load: bool,
    /// `bSwapAmmoWithWeaponHolstered`: Ammo Swap works with the weapon put away.
    pub swap_ammo_holstered: bool,
    /// `bNoAutoContinueDialog`: each response of a conversation waits for a click.
    pub no_auto_continue_dialogue: bool,
    /// `bDontSetQuestWhenObjectivesAdded`, `bDontSetQuestWhenCompletingObjectives`: the
    /// Pip-Boy does not make a quest active by itself (only the player's pick or a
    /// script's ForceActiveQuest does).
    pub no_quest_set_on_objective: bool,
    pub no_quest_set_on_complete: bool,
    /// `bHideReadNotes`: the Pip-Boy's notes leave out those already read.
    pub hide_read_notes: bool,
    /// `bVATSStopBurstIfTargetDead`: a V.A.T.S. burst stops once its target is dead.
    pub vats_burst_stops_on_kill: bool,
    /// `bRepairRequiresWeaponSkill`: the player repairs a weapon only with Repair at
    /// least its skill requirement.
    pub repair_needs_weapon_skill: bool,
    /// `bHideUnknownNPCNames`: the prompt names no one who has not spoken to the player.
    pub hide_unknown_names: bool,
    /// `bShowUsePasswordOnTerminalsWithNote`: a locked terminal the player holds the
    /// password note of says so until first opened.
    pub terminal_password_prompt: bool,
    /// `bLockPickMenuKeyboardMovement`: in lockpicking the strafe keys move the pin and
    /// Forward, Back and Jump turn the screwdriver.
    pub lockpick_move_keys: bool,
    /// `bMapRecenterHotkey`: C centres the Pip-Boy's maps on the player again.
    pub map_recentre_key: bool,
    /// `bMapLocationDisplayDistance`: the World Map says how far the marker under the
    /// pointer is.
    pub map_marker_distance: bool,
    /// `bBarterQuantityMenuShowsPrice`, `bContainerQuantityMenuShowsWeight`: the "How
    /// many?" box says what the amount costs in a barter, or weighs in a container.
    pub quantity_shows_price: bool,
    pub quantity_shows_weight: bool,
    /// `bHideReputationTabIfEmpty`: with no reputations the Stats page's General shows
    /// the statistics only.
    pub hide_empty_reputation: bool,
    /// `bPreviewPerksOnLevelUp`: the level-up menu shows the perks page even with no perk
    /// to take.
    pub preview_perks: bool,
    /// `bMarkNotesUnread`: a right click on a note in the Pip-Boy marks it unread.
    pub mark_notes_unread: bool,
    /// `bShiftIgnoresFriendlyVATS`: with Shift held V.A.T.S. marks no one who would not
    /// fight the player.
    pub vats_shift_ignores_friends: bool,
    /// `bRadiationCausesRadioStatic`: radiation taken brings static into the Pip-Boy's
    /// radio.
    pub radiation_radio_static: bool,
    /// `bNoSelfMeltdownDamage`: Meltdown's blasts do not hurt the player or companions.
    pub no_self_meltdown: bool,
    /// `iMaxCharacterLevel`: the player's level cap, the add-ons' +5 left out; 0 as the
    /// game.
    pub max_level: u32,
    /// `bSneakAttackWithoutCrouching`: an attack on someone who has not detected the
    /// player is a sneak attack standing too.
    pub standing_sneak_attacks: bool,
    /// `bNoCompanionItemDamage`: the companions' weapons do not wear.
    pub no_companion_item_damage: bool,
    /// `bWeaponModdingSkillRequirement`: a weapon is modded only by a player who meets
    /// its skill requirement.
    pub modding_needs_skill: bool,
    /// `bPlantingLiveGrenadesRequiresWeaponSkill`: an explosive placed in a pocket goes
    /// live only for a player who meets its skill requirement.
    pub planting_needs_skill: bool,
    /// `bFastTravelCostsSpecialNukaBottles`: fast travel takes a Nuka-Cola Quantum,
    /// Quartz or Victory, and is refused without one.
    pub travel_costs_bottle: bool,
    /// `bArmorCausesSinking`: medium and heavy body armour pull a swimmer down.
    pub armor_sinks: bool,
    /// `bRememberBobbyPinHealth`: the bobby pin keeps its wear from lock to lock.
    pub remember_pin_health: bool,
    /// `bForceLockpickNoBreakLock`: a failed Force Lock breaks the pin, not the lock.
    pub force_spares_lock: bool,
    /// `bNoWeaponConditionDamagePenalty`: weapons deal full damage whatever their
    /// condition.
    pub no_condition_damage_penalty: bool,
    /// `bRemoveWeaponDamageBuffer`: damage falls with condition from full, not from 75%.
    pub no_condition_damage_buffer: bool,
    /// `bCritChanceIgnoresFireRate`: automatic weapons' critical chance is not divided by
    /// their fire rate.
    pub crit_ignores_fire_rate: bool,
    /// `bScaleCriticalDamage` (`fCriticalDamageMult`): a factor on critical damage.
    pub crit_damage_scale: f64,
    /// `bNoScaleNpcDamageResistanceByCondition`, `bNoScaleNpcDamageThresholdByCondition`:
    /// an NPC's armour keeps its whole DR, or DT, however worn.
    pub npc_dr_ignores_condition: bool,
    pub npc_dt_ignores_condition: bool,
    /// `bExplosionRadiusBuff`: an explosion's inner 30% deals full damage, its edge 20%.
    pub explosion_radius_buff: bool,
    /// `bNPCsCanSneakCritPlayer`: an NPC the player has not detected lands sneak attack
    /// criticals on the player.
    pub npcs_sneak_crit_player: bool,
    /// `bFatalNonSneakCritsAlwaysGib`: a killing critical that is no sneak attack always
    /// takes the struck part off.
    pub fatal_crits_gib: bool,
    /// `bFallDeathsCauseDismemberment`: an actor killed by a fall loses its legs.
    pub fall_deaths_dismember: bool,
    /// `bSplashDamageTorsoGibbing`: a blast kill by someone with Bloody Mess may blow the
    /// torso apart.
    pub splash_torso_gib: bool,
    /// `bNPCsDropWeaponHolsteredWeapon`: an NPC that dies with its weapon put away drops
    /// it as one with it drawn does.
    pub npcs_drop_holstered: bool,
    /// `bNPCsEarnAmmoCasings`: NPCs keep their spent rounds' items (cases, hulls, drained
    /// cells) as the player does.
    pub npcs_earn_casings: bool,
    /// `bNoAmmoCasings`: the player's spent rounds leave no items (cases, hulls, drained
    /// cells).
    pub no_ammo_casings: bool,
    /// `bCompareArmorStats`: a piece of armour's DT in the Pip-Boy shows the difference
    /// from the worn pieces it would replace.
    pub compare_armor: bool,
    /// `bMoreDetailedStatsMenu`: the Pip-Boy's (+) and (-) give the amount, and values
    /// show past their caps.
    pub detailed_stats: bool,
    /// `bTimescaleAffectsMinigames`: the lock's turning runs at the world clock's speed
    /// (SetGlobalTimeMultiplier).
    pub minigames_timescale: bool,
    /// `bNoPlayerNameLimit`: the player's name may be as long as
    /// `chargen::NAME_LONG_CHARS`.
    pub no_name_limit: bool,
    /// `bRecurringChallengeIndicator`: a challenge that starts again when done says so in
    /// the Pip-Boy.
    pub mark_recurring_challenges: bool,
    /// `bBetterMapZoom`: the wheel zooms the Pip-Boy's maps about the pointer.
    pub map_zoom_at_pointer: bool,
    /// `bModConsolePrintsIncludeName`: a script's console line starts with its plugin's
    /// name in brackets.
    pub print_plugin_names: bool,
    /// `bWeaponRequirementsMatter`: the player cannot equip a weapon without its Strength
    /// and skill requirements.
    pub weapon_requirements_matter: bool,
    /// `bImprovedHacking`'s options: a lone symbol does nothing; a word tried before
    /// does nothing; a bracket pair with full attempts removes a dud; removing a dud
    /// spares the words tried.
    pub hack_no_single_chars: bool,
    pub hack_no_repeat_words: bool,
    pub hack_dud_when_full: bool,
    pub hack_spare_guessed: bool,
}

/// The factor on a vendor's repair price under the Barter Affects Repair Costs tweak:
/// `discount` percent off at Barter 100, in proportion below [design:
/// docs/design/tweaks.md; the tweak's own constants are not used, this rule is ours].
pub fn barter_repair_factor(discount: f32, barter: f32) -> f32 {
    1.0 - discount.clamp(0.0, 100.0) / 100.0 * barter.clamp(0.0, 100.0) / 100.0
}

/// The condition (a fraction) a crafted weapon or armour comes out at under the Repair
/// Scales Crafting Condition tweak: half at Repair 0, whole at 100 [design:
/// docs/design/tweaks.md; the tweak names no formula, this one is ours].
pub fn crafted_condition(repair: f32) -> f32 {
    0.5 + repair.clamp(0.0, 100.0) / 200.0
}

impl crate::game::Game<'_> {
    /// `r` wears power armour on the upper body (for the tweaks).
    pub fn wears_power_armor(&self, r: fnv_sim::FormId) -> bool {
        use fnv_sim::forms::ItemKind;
        self.world.equipment(r).unwrap_or_default().into_iter().filter_map(|i| self.world.forms.item(i)).any(|it| it.kind == ItemKind::Armor && it.power_armor && it.slots.is_some_and(|s| s & crate::detection::UPPER_BODY != 0))
    }

    /// The player's Weapon Requirements Matter tweak: why the player may not equip
    /// `weapon` (its DNAM Strength or skill requirement above the player's value now), or
    /// `None` [design: docs/design/tweaks.md].
    pub fn weapon_requirement_unmet(&mut self, weapon: fnv_sim::FormId) -> Option<String> {
        use fnv_script::vm::XVal;
        use fnv_sim::forms::ItemKind;
        if !self.tweaks.weapon_requirements_matter || self.world.forms.item(weapon).map(|i| i.kind) != Some(ItemKind::Weapon) {
            return None;
        }
        let field = |g: &Self, path: &str| match crate::nvse::record_value(g, weapon, "DNAM", path) {
            Some(XVal::Num(x)) => x,
            _ => 0.0,
        };
        let (strength_req, skill_req, skill) = (field(self, "strength_req"), field(self, "skill_req"), field(self, "skill") as u16);
        let player = fnv_sim::FormId::PLAYER_REF;
        if strength_req > 0.0 && crate::functions::actor::actor_value(self, player, crate::actor_values::STRENGTH).unwrap_or(0.0) < strength_req {
            return Some(format!("Needs Strength {strength_req:.0}."));
        }
        if skill_req > 0.0 && skill != 0 && crate::functions::actor::actor_value(self, player, skill).unwrap_or(0.0) < skill_req {
            let name = crate::chargen::skill_name(self, skill);
            return Some(format!("Needs {name} {skill_req:.0}."));
        }
        None
    }
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
            no_tutorials: false,
            no_knockdown_in_god_mode: false,
            show_current_skill: false,
            mark_unspoken: false,
            no_caps_added_message: false,
            subtitle_names: false,
            keep_acting_notes: false,
            hide_completed_quests: false,
            hide_marker_reputation: false,
            no_show_quest_location: false,
            hide_trait_ranks: false,
            hide_quest_item_figures: false,
            single_item_figures: false,
            mark_other_ammo: false,
            show_fame_infamy: false,
            repair_any_armor: false,
            sort_misc_stats: false,
            effect_time_left: false,
            perk_plugin_names: false,
            item_added_seconds: 0.0,
            pipboy_12_hour: false,
            notes_of_all_quests: false,
            pickpocket_after_caught: false,
            wait_anywhere: false,
            detached_beams: false,
            god_mode_off_on_load: false,
            no_stealth_shimmer_in_pipboy: false,
            quiet_teammate_steps: false,
            kill_xp_per_level: 0.0,
            key_xp_share: 0.0,
            password_xp_share: 0.0,
            power_armor_blocks_radiation: false,
            power_armor_limb_damage: 1.0,
            fall_height_per_endurance: 0.0,
            skip_easy_locks: false,
            skip_lock_margin: 0.0,
            pick_up_disarmed: false,
            npc_weapons_whole: false,
            lethal_knockdowns_count: false,
            crippling_never_disarms: false,
            no_spin_up: false,
            god_mode_free_crafting: false,
            subtitle_distance: 0.0,
            sneak_criticals_head_only: false,
            compare_weapons: false,
            mark_nearby_places: 0.0,
            scale_ash_piles: false,
            reload_while_firing: false,
            semi_auto_queue: 0.0,
            repair_scales_crafting: false,
            no_healing_in_combat: false,
            breath_regain: 0.0,
            pickpocket_caught_detects: false,
            agility_jump_mult: 0.0,
            lockpick_force_near_bonus: 0.0,
            contact_mines_player: false,
            contact_mines_npcs: false,
            sleep_heal_min_hours: 0,
            barter_repair_discount: 0.0,
            armor_condition_penalty: 1.0,
            vats_ignores_companions: false,
            frenzy_kill_xp: false,
            free_wild_wasteland: false,
            stats_xp_percent: false,
            repair_same_first: false,
            repair_low_first: false,
            no_skill_books_at_max: false,
            player_pain_gap: 0.0,
            pause_on_load: false,
            swap_ammo_holstered: false,
            no_auto_continue_dialogue: false,
            no_quest_set_on_objective: false,
            no_quest_set_on_complete: false,
            hide_read_notes: false,
            vats_burst_stops_on_kill: false,
            repair_needs_weapon_skill: false,
            hide_unknown_names: false,
            terminal_password_prompt: false,
            lockpick_move_keys: false,
            map_recentre_key: false,
            map_marker_distance: false,
            quantity_shows_price: false,
            quantity_shows_weight: false,
            hide_empty_reputation: false,
            preview_perks: false,
            mark_notes_unread: false,
            vats_shift_ignores_friends: false,
            radiation_radio_static: false,
            no_self_meltdown: false,
            max_level: 0,
            standing_sneak_attacks: false,
            no_companion_item_damage: false,
            modding_needs_skill: false,
            planting_needs_skill: false,
            travel_costs_bottle: false,
            armor_sinks: false,
            remember_pin_health: false,
            force_spares_lock: false,
            no_condition_damage_penalty: false,
            no_condition_damage_buffer: false,
            crit_ignores_fire_rate: false,
            crit_damage_scale: 1.0,
            npc_dr_ignores_condition: false,
            npc_dt_ignores_condition: false,
            explosion_radius_buff: false,
            npcs_sneak_crit_player: false,
            fatal_crits_gib: false,
            fall_deaths_dismember: false,
            splash_torso_gib: false,
            npcs_drop_holstered: false,
            npcs_earn_casings: false,
            no_ammo_casings: false,
            compare_armor: false,
            detailed_stats: false,
            minigames_timescale: false,
            no_name_limit: false,
            mark_recurring_challenges: false,
            map_zoom_at_pointer: false,
            print_plugin_names: false,
            weapon_requirements_matter: false,
            hack_no_single_chars: false,
            hack_no_repeat_words: false,
            hack_dud_when_full: false,
            hack_spare_guessed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barter_takes_its_part_off_repairs() {
        assert_eq!(barter_repair_factor(50.0, 100.0), 0.5);
        assert_eq!(barter_repair_factor(50.0, 50.0), 0.75);
        assert_eq!(barter_repair_factor(0.0, 100.0), 1.0);
        assert_eq!(barter_repair_factor(50.0, 150.0), 0.5);
    }

    #[test]
    fn crafted_condition_runs_from_half_to_whole() {
        assert_eq!(crafted_condition(0.0), 0.5);
        assert_eq!(crafted_condition(50.0), 0.75);
        assert_eq!(crafted_condition(100.0), 1.0);
        assert_eq!(crafted_condition(130.0), 1.0);
        assert_eq!(crafted_condition(-5.0), 0.5);
    }
}
