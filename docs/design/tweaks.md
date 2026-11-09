# Tweaks

The settings page's **Tweaks** tab: optional changes to how the game plays or what it
shows, each off (or at "no change") unless the player sets it. The owner asked for
the features of Stewie's Tweaks, a script-extender plugin for the original, as engine
settings (2026-11-08, #1891).

## Sources and clean room

- [documented: Stewie Tweaks v9.87, its readme and its menu list `AllTweaks.json`, a private
  copy in the owner's `C:\fnv-private\stewietweaks`] say what each tweak does. Only
  that is used: the plugin's DLL is not opened, and none of its files or text are copied.
  The labels and notes of the Tweaks tab are NVForever's own words. The catalogue below
  names each tweak by its INI key, so a player who knows the plugin can find it.
- [documented: Viva New Vegas's generated `nvse_stewie_tweaks.ini`, the owner's
  modlist] says which tweaks that modlist turns on (the "VNV on" column); those go
  first in later batches.

## What is left out

- **Fixes** (the plugin's Fixes category): workarounds for the original's bugs. The
  owner: "if there are any fixes for the engine skip them we dont need fixes". NVForever
  is its own engine and does not carry those bugs over.
- **n/a**: about the original program, not about play: its error log, its keyboard
  handling, its achievements, its main menu's Downloads button, mod-author debug prints,
  the plugin's own performance work (its "inline vanilla functions" option is not even in
  the menu list).
- **unsure, skipped**: needs the plugin's own menu files or another mod's files, or the
  readme does not say clearly enough what changes to build it without guessing. Each says
  why. These wait for an owner decision or an observation.

## How it is built

- `fnv_game::tweaks::Tweaks`: the values the game's rules read (`Game::tweaks`), e.g.
  the experience multiplier in `experience::award`, power armour training in the
  frame's `set_power_armor_training`. Not saved with a game: settings.
- `nvf_play::tweaks::TweakSettings`: every value, with the game's part inside; the engine's
  modules read the rest (the karma messages in `crime`, the fast-travel refusals in
  `pipboy::data::travel`, the quest lines in `quest_messages::Hidden`, the XP meter).
  `sync` hands the game's part to the running game each frame, as the difficulty is
  handed over, and pushes the quest-line choices into `QuestMessages`.
- Each tweak is a row of the Tweaks tab, kept in `NVForever.ini` `[Settings]` under a
  `tweak-…` key (`crate::settings`). A switch is `switch!`; a multiplier is a slider
  from x0 to x2 in tenths, x1 in the middle.
- A tweak's options (the plugin's sub-settings) are added as rows of their own only
  when they change what the tweak does in a way a player would choose; the first batch
  keeps each tweak to one row and says in the catalogue which options are not built.

## First batch (2026-11-08)

| Row | INI key | Where |
|---|---|---|
| Experience Earned (x0–x2) | `bReduceXP` | `fnv_game::experience::award`, after the perks |
| Quiet Experience | `bNoXPMessages` | `xp_meter::watch`: a gain that earns no level is not shown; LEVEL UP still is |
| Quiet Karma | `bNoKarmaMessages` | `crime::show_notes` |
| No Quest Failed Banner | `bNoQuestFailedPopup` | `QuestMessages::push` |
| No Location Banner | `bNoLocationPopup` | `QuestMessages::push` (the place is still discovered and its XP given) |
| No Objective List | `bRemoveQuestObjectiveAddedText` | `QuestMessages::push` |
| No Fast Travel | `bDisableFastTravel` | `travel::tweak_checks`: refused with the script block's text |
| Fast Travel in the Air | `bMidairFastTravel` | `travel::tweak_checks` |
| Fast Travel Near Enemies | `bFastTravelWithEnemiesNearby` | `travel::tweak_checks` (combat itself still refuses) |
| Fast Travel Encumbered | `bFastTravelOverencumbered` | `travel::tweak_checks` |
| Power Armor Without Training | `bPowerArmorNeedsNoTraining` | `Game` frame tick and `SetPCCanUsePowerArmor` |
| No Red Crosshair | `bNoRedCrosshairOnEnemies` | `hud_target::aim` (`AimHostile`, read by both HUDs) |
| No Enemies on the Compass | `bHideEnemyMarkers` | `compass::gather` |
| Quiet Reputation | `bNoReputationMessages` | `reputation::watch` |
| No Battle Music | `bDisableCombatMusic` | `music::play` |
| No Loading Tips | `bDisableLoadingScreenTips` | `loading::spawn_screen` (`Loading::hide_tips`) |
| No Explosion Blur | `bDisableExplosionInFaceIMOD` | nvforever `shot_fx::explosions` |
| No Chem Wore-Off Effect | `bRemoveChemWarnOffIMOD` | `Game::drug_wore_off` |
| Sleep in Owned Beds | `bSleepInOwnedBeds` | `wait::refusal` |

Already so, no row added: `bNoDialogueZoom` is the Gameplay tab's dialogue-zoom row;
NVForever never refuses a jump for being over-encumbered (`bJumpWhileOverencumbered`)
and its main menu's Quit asks nothing (`bNoExitConfirm`).

Options not built in this batch [design]: Quiet Karma's separate good/evil switches and
thresholds, Quiet Experience's per-source switches, the quest-failed "only if started"
option, Quiet Reputation's per-kind switches, the compass's "show enemies that fire"
option.

## Third batch (2026-11-08, Viva New Vegas's first)

| Row | INI key | Where |
|---|---|---|
| No Sneak Label | `bRemoveSneakLabel` | `hud::gather` and the tile HUD (`tiles`) |
| Skip Kill Camera | `bVatsExitKey` | `kill_cam::update`: a right click (Tab is the Pip-Boy here) |
| Number Keys on Terminals | `bNumberedComputerHotkeys` | `terminal::keys`: 1-9, 0 the tenth, outside the hacking page |
| Unassign Hotkeys | `bDeselectHotkeys` | `Hotkeys::clear_if_holding` from `hotkey_panel::assign_row` |
| Music While Talking (slider) | `bScaleMusicVolumeDuringDialogue` | `music::play`, in `PlayState::Dialogue` |
| Radio While Talking (slider) | `bScaleRadioSongVolumeDuringDialogue` | `radio::play`, in `PlayState::Dialogue` |
| Perks First When No Points | `bSkipSkillMenuIfNoPointsToAssign` | `levelup::sync` |
| No Message for Spent Books | `bNoSkillMessageIfIncreaseIsZero` | `script_events::effects` (`EffectCtx::tweaks`) |
| Recipe Categories in Capitals | `bCapitalizeRecipeCategories` | `crafting::view` |
| Rounds per Shot | `bShowWeaponAmmoUseInMenus` | `pipboy::items::info_of` (WEAP ammo use) |
| Skill Book Effects | `bShowBookEffects` | `pipboy::items::info_of` (the points with the perks applied) |
| Map Remembers Its Place | `bMapRemembersPosition` | `pipboy::data::opened` |
| Faster Hacking Exit | `bFasterHackingTransition` | `terminal::timers`: 0.5 s instead of 2 s |

## Fourth batch (2026-11-08)

| Row | INI key | Where |
|---|---|---|
| Console Clicks Skip the Player | `bNonSelectablePlayerInConsole` | `console::click` |
| Holster in Real Time | `bPipboyLightAndHolsteringIgnoreTimescale` | `control::read_controls` (the light already counts real time) |
| No Screen Blood in Power Armor | `bPowerArmorPreventsScreenBlood` | `screen_splatter::take_hurts` |
| Invisible Foes Stay Hidden | `bHideRedCrosshairOnDistantInvisibleTargets` | `hud_target::aim` (Invisibility or Chameleon ≥ 1) |
| Quiet Landings in God Mode | `bGodmodePreventsLegCrippleSound` | `falls::hurt` |

## Fifth batch (2026-11-08)

| Row | INI key | Where |
|---|---|---|
| Count Repeated Console Lines | `bCompressDuplicateConsoleMessages` | `Console::print` (`line -#n`) |
| Console Times | `bConsolePrintsIncludeTimestamp` | `Console::print` (`[mm:ss]` since the first line) |
| Living Anatomy Shows DR | `bLivingAnatomyShowDR` | `hud_target::aim`, `TargetHealth::anatomy_line` |
| Matching Categories | `bSynchronizeContainerCategories` | `MenuModel` `Category`/`SetCategory` (`sync_categories`) |

## Sixth batch (2026-11-08, gameplay)

| Row | INI key | Where |
|---|---|---|
| Perks per Level (1–5) | `iPerksPerLevel` | `PerkSheet::of_player` |
| S.P.E.C.I.A.L. Points (30–70) | `bCustomSpecialPoints` | `SpecialSheet::for_menu` when no script set a total |
| Pick Any Lock | `bPickLocksWithoutSkill` | `lockpick::opening` (the skill asked of `may_pick`) |
| Hack Any Terminal | `bHackTerminalsWithoutSkill` | `terminal::opening` |
| No Terminal Lockout | `bNoLockFailedTerminals` | `terminal::hacked` |
| No Casino Bans | `bNoCasinoBans` | `casino::opening` |
| Search the Unconscious | `bLootUnconsciousVictims` | `activate::actor_action_in` |
| Pickpocket Worn Items | `bPickpocketWornItems` | `Game::pickpocket_worn` |
| No Sneak Criticals | `bNoSneakAttacksCriticals` | `combat::sneak_attack` |
| Reload Only When Empty | `bDisableReloadingNonEmptyClip` | `combat::fire` |
| Fast Travel Takes No Time | `bNoFastTravelTimeChange` | `travel::go` |
| Falls in Power Armor (x0–x2) | `bPowerArmorScalesFallDamage` | `falls::hurt` (`tweaks::wears_power_armor`) |

## Seventh batch (2026-11-08, weight)

| Row | INI key | Where |
|---|---|---|
| Weightless Armor / Aid / Ammo / Weapons / Misc Items | `bWeightlessItems` | `fnv_sim::inventory::WeightRules::weightless` (bits), filled by `Game::sync_perk_weights` |
| Worn Armor Weighs Nothing | `bWeightlessWornArmor` | the same, for equipped armour |
| Worn Power Armor Weighs Nothing | `bWeightlessWornPowerArmor` | the same, for equipped power armour |

`WeightRules` is saved with the world; its new fields have serde defaults, so older
saves load, and the frame's sync sets them from the settings again.

## Eighth batch (2026-11-08, Action Points and refusals)

| Row | INI key | Where |
|---|---|---|
| Action Points per Kill (0–50) | `bKillsRewardAP` | `Game` hit outcome, beside the kill XP |
| Action Points per Jump (0–50) | `bJumpingCostsAP` | `tweaks::ap_costs` (after the input, before physics) |
| Action Points per Second Running (0–20) | `bRunningCostsAP` | `tweaks::ap_costs` |
| No Travel on a Crippled Leg | `bNoFastTravelIfLegsCrippled` | `travel::answer`, `Refusal::CrippledLeg` (own text) |
| Wait Only When Seated | `bOnlyAllowWaitWhileSitting` | `wait::refusal`, `Refusal::NotSeated` (own text) |

## Ninth batch (2026-11-08)

| Row | INI key | Where |
|---|---|---|
| No Selling | `bDisableSellingItems` | `MenuModel` barter offer (`no_selling`, set by `tweaks::sync`) |
| Own Skill Points Rule (+ two sliders) | `bModifySkillPointsEarned` | `levelup::points_for` (`custom_skill_points`) |
| No Unconscious Message | `bNoUnconciousMessage` | `script_events::effects` |
| No Helmet Voice Filter | `bNoAudioDistortion` | `voice::play` |
| No Skipping Lines | `bNoSkipDialog` | `dialogue::input` |
| Mouse Turns the Lock | `bLockpickAllowMouse` | `lockpick::step` |
| No Casino Bet Limit | `bNoMaxCasinoBet` | the Show…Menu script functions (`functions::misc`) |
| Full Pace While Aiming | `bAimingSpeed` | `player::aiming_walks` |
| Hold to Keep Jumping | `bRepeatJumping` | `player::read_input` |
| Hold to Sneak | `bHoldCrouchToSneak` | `control::read_controls` |

## Tenth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Action Points to Enter V.A.T.S. (0–50) | `bEnteringVATSCostsAP` | `vats_mode::try_enter` |
| Action Points per Power Attack (0–50) | `bChargedAttacksCostAP` | `combat::melee` power branch |
| Quest Markers Within (0–20000 units) | `bDistanceBasedQuestMarkerVisibilty` | `compass::gather` quest loop |
| Scope Zoom Time (part) | `bUseCustomSniperZoomRate` | `aim_zoom::zoom` while a scope is up |
| Q Drops Items | `bAddInventoryDropItemHotkey` | `pipboy::items::keys` |

## Eleventh batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Picked Containers Stay Shut | `bDontShowContainerAfterLockpick` | `lockpick::close` (no `ActivateNext` for a container) |
| Picked Doors Stay Shut | `bDontOpenDoorsAfterLockpick` | `lockpick::close` (no `ActivateNext` for a door) |
| Mark Unvisited Doors | `bPatchUnseenCellName` | `activate::find_target` (`visited_cells`) |
| No Leaving a Hack | `bNoExitHacking` | `terminal::apply` |
| No Grabbing Owned Items | `bNoGrabOwnedItems` | `grab::grab` (`activate::owned_by_other`) |

## Twelfth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Agility Moves Faster (0–10% a point) | `bAgilityScalesMovementSpeed` | `Game::move_speed_mult` |
| Skip Easy Hacks (+ margin) | `bAutoUnlockTerminals` | `terminal::opening` |
| Shots Spare Weapons | `bHittingWeaponsDoesntDamageThem` | `combat::strike_weapon` |
| No Jams in God Mode | `bGodModePreventsJamAnims` | `combat::fire` |
| Strength Resists Blasts | `bExplosionKnockdownAvoidanceUsesStrength` | `combat::fly` knockdown |
| Drowning Takes AP First | `bDrowningDrainsAP` | `drowning::breathe` |
| Aiming Draws the Weapon | `bAimingUnholstersWeapons` | `control::read_controls` |
| Steepest Walkable Slope (30–80°) | `bBillyGoatMode` | `player::step` (`max_slope_deg`) |
| Jumping on a Crippled Leg (part) | `bCrippledLegsScaleJumpHeight` | `player::step` (`jump_height`) |

## Thirteenth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Aid at Full Health | `bAllowAidAtMaxHealth` | `pipboy::stats::model::refusal_with` |
| Grabbing Is Stealing | `bGrabbingItemsIsCrime` | `grab::grab` (`Game::player_stole`) |
| Merchants Keep to Their Lists | `bBarterCheckActorBuySellFlags` | `MenuModel::refresh_trade` (`vendor_lists_both`) |
| Name the Key | `bLockNeedsKeyShowName` | `lockpick::open` refusal line |
| No Skill Tags | `bNoSkillTags` | `dialogue` topic tags |
| No NPC Addiction | `bPreventNPCAddiction` | `Game` aid addiction chance |
| Pickpocket the Fleeing | `bPickpocketFleeingActors` | `Game::pickpocket_refusal` |
| Full Reload Swaps Ammo | `bReloadingWithFullClipSwitchesAmmoType` | `combat::fire` |
| Melee Reach (x0–x2) | `bScalePlayerMeleeReach` | `combat::melee` reach |
| Experience per Repair (0–50) | `bRepairsRewardXP` | `pipboy::repair::keys` (`Source::Repair`) |
| Repair Caps Go to the Till | `bPreventStealingCapsAfterRepair` | `repair::repair_one` |
| Spare the Blade on the Dead | `bNoDamageMeleeWeaponIfTargetDead` | `combat::melee` wear |
| Action Points Back When Over-Encumbered (part) | `bNoAPRegenWhileOverencumbered` | `vats_mode::regenerate` |
| Light Step for Companions | `bLightStepAffectsCompanions` | `mines::tick` |
| No Player Stagger | `bNoPlayerStaggerAnims` | `combat::player_flinch` |
| Ready Does Not Draw | `bPreventReloadWeaponUnholstering` | `control::read_controls` |

## Fourteenth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Jams at Reload Pace | `bReloadJamsAffectedByAgility` | `combat::fire` jam time |
| No Hotkeys While Reloading | `bReloadingPreventsWeaponHotkeys` | `weapon_wheel::read` |
| Wading Pace (part) | `bRunSlowerInWater` | `player::step` pace, by depth over `swim_depth` |
| Sleep on Chairs | `bSleepOnChairs` | `wait::open` |
| Quiet Theft | `bStealingSendsNoAlarm` | `Game::player_stole` |
| No XP for Companion Kills | `bNoCompanionKillXP` | `Game::kill_earns_xp` |
| Companions Keep Their Weapons | `bNoDisarmCompanions` | `combat::hit_actor` |
| Own Blasts Spare the Player | `bNoSelfExplosionDamage` | `combat::fly` blast loop |
| Sneak Criticals Melee Only | `bSneakCriticalsOnlyMeleeWeapons` | `combat::fly` shots |
| Meltdown for Companions | `bAllowTeammatesUseMeltdown` | `combat::meltdown` |
| Speed Scales the Jump | `bSpeedMultScalesJumpHeight` | `player::step` jump height |

## Fifteenth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| No Minigame Help | `bDisableTutorialMessages` | each minigame's `first_help` |
| No Knockdowns in God Mode | `bNoKnockdownInGodmode` | `Game::can_knock_down` |
| Say My Skill | `bLockpickHackingMessageShowsCurrentSkill` | `lockpick::open`, `terminal` refusal lines |
| Blood Only on Big Hits (0–50) | `bMinBloodSplatterHealthDamage` | `screen_splatter::take_hurts` |
| XP Sound at the Level Cap | `bMinigamesPlayXPSoundAtMaxLevel` | `xp_meter::watch` |
| Hold to Wait | `bHoldWaitKeyToShowMenu` | `wait::rest_opens` |
| No Iron Sight Zoom | `bDisableWeaponFOV` | `aim_zoom::zoom` |
| Hide Quest Items in Misc | `bHideMiscQuestItems` | `MenuModel` list refresh (`hide_misc_quest`) |

## Sixteenth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Hotkey Draws and Holsters | `bHotkeyHolstersWeaponIfEquipped` | `weapon_wheel::act` on `Used::InHand` |
| No Interior Fog | `bDisableInteriorFog` | nvforever `sky::update_sky` |
| Mark Strangers | `bUnspokenNPCIndicator` | `activate` Talk prompt |
| No Caps Added Line | `bHideCapsAddedMessages` | `script_events::effects` |
| Subtitles Name the Speaker | `bSubtitlesShowActorNames` | `script_hud` |
| Compass Pips Only in Danger | `bNoCompassPipsIfNotInDanger` | `compass::gather` |
| Foes in the Usual Colour | `bHostilesUseNeutralColorOnCompass` | `compass::gather` |
| Sneak Label While Standing | `bShowSneakLabelWhileStanding` | `hud::gather` |
| No XP Meter in Combat | `bNoXPBarInCombat` | `xp_meter::watch` |
| No Crippled Limb Figure | `bDisableHUDCrippledLimbIndicator` | `hud_limbs::watch` |

## Seventeenth batch (2026-11-09, menus)

| Row | INI key | Where |
|---|---|---|
| Keep Acting Notes | `bDialogueKeepVoiceActingNotes` | `dialogue` spoken line text |
| Hide Finished Quests | `bHideCompletedQuests` | `pipboy::data::lists` |
| No Reputation on Map Markers | `bHideMapMarkerFactionReputation` | `pipboy::data::world_map` |
| No Show Location | `bDisableShowQuestLocation` | `pipboy::data::build` |
| No Ranks for Traits | `bHideRanksInTraitMenu` | `chargen::lists` trait info |
| No Figures for Quest Items | `bHideQuestItemWeightAndValue` | `pipboy::items::info_of` |
| One Item's Figures | `bIndividialItemStats` | `pipboy::items::info_of` |
| Mark Other Ammo | `bShowAlternateAmmoTypesAvailableInMenus` | `pipboy::items::info_of` |
| Show Fame and Infamy | `bReputationShowsFameInfamy` | `pipboy::stats::read` standings |
| Take All Stays Open | `bContainerTakeAllDoesntCloseMenu` | `MenuModel` Take All (`take_all_stays`) |
| Hide Worn Items in Barter | `bHideEquippedItemsInBarter` | `MenuModel::refresh_trade` |
| Hide Worn Items in Containers | `bHideEquippedItemsInContainers` | `MenuModel` list refresh |
| Quiet Category Titles | `bNoCapitaliseContainerCategories` | `menu::look::half_title` |
| No Right-Click Drop | `bDisableInventoryRightClickDrop` | `pipboy::items::on_row_click` |
| Quiet Undiscovered Markers | `bPreventNoFastTravelMessage` | `travel::answer` |

## Eighteenth batch (2026-11-09, keys)

The plugin's hotkeys are controls on the Controls page, not rows of the Tweaks tab:
eight new `Control`s after Lower Weapon (`bindings::TWEAK_CONTROLS` on), none bound
unless the player binds them, so nothing changes until then.

| Control | INI key | Where |
|---|---|---|
| Holster Weapon | `iHolsterWeaponKey` | `control::read_controls` |
| Pip-Boy Light | `iTogglePipboyLightKey` | `flashlight::menu_key` |
| Show/Hide Crosshair | `iToggleCrosshairKey` | `tweaks::keys`, read by `reticle::draw` and the tile HUD |
| Show/Hide Sneak Label | `iToggleSneakIndicatorKey` | `tweaks::keys` (`no_sneak_label`) |
| Show/Hide HUD | `iToggleMenusKey` | `tweaks::keys` (ToggleMenus' flag) |
| Main Menu | `iExitToMainMenuKey` | `tweaks::keys` (`MainMenuRequest`) |
| Quit Game | `iExitGameKey` | `tweaks::keys` (`AppExit`) |
| No Collision (held) | `iDisableCollisionKey` | `tweaks::keys` (`NoClip`) |

## Nineteenth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Repairers Take Any Armor | `bShowQuestAndNoDRDTItemsInRepairMenu` | `repair::view` (`vendor_lists`) |
| Sorted Statistics | `bSortMiscStats` | `pipboy::stats::read` |
| Effect Time Left | `bStatsMenuShowEffectTimeRemaining` | `pipboy::stats::read` effect lines |
| Perk Plugins | `bShowModNameInPerkDescriptions` | `levelup::model` (`GameData::form_plugins`) |
| Item Added Line Time (0–10 s) | `bCustomAddToInventoryMessageTimer` | `script_events::effects` |
| 12-Hour Pip-Boy Clock | `b12HourPipboyClock` | `pipboy::data::model::date_time_with` |

## Twentieth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Notes of Every Quest | `bShowActiveQuestNotesShowsAllStartedQuests` | `pipboy::data::lists` notes |
| Aiming Turn Speed: Across / Up and Down | `bTurnSlowerWhileAiming` | `player::mouse_look` |
| Pickpocket Again | `bAllowPickpocketIfAlreadyCaught` | `Game::pickpocket_refusal` |
| Wait Anywhere | `bSleepWaitAnywhere` | `wait::refusal` after the script ban |
| Radio Songs / Talk Volume | `bScaleRadioVolume` | `radio::play` line level |
| Wheel Zoom Needs Toggle View | `bOnlyChangeCameraHeightIfPOVKeyHeld` | `control::read_controls` |
| No Hotkey Sounds | `bNoHotkeyEquipSounds` | `weapon_wheel::act` |

## Twenty-first batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Detached Beams | `bDetachedBeams` | `combat::fly` beam `follow` |
| God Mode Off on Load | `bDisableGodmodeOnLoad` | `save::apply_engine` |
| No Stealth Shimmer in the Pip-Boy | `bDisableStealthEffectInPipboy` | nvforever `stealth_field::apply` |
| Quiet Companion Footsteps | `bPreventTeammateFootstepSounds` | nvforever `anim_sounds::actor_keys` |
| No Weapon Hotkeys | `bDisableWeaponHotkeys` | `weapon_wheel::read` taps |
| Pip-Boy Opens on Items | `bOpenPipboyToInventoryByDefault` | `pipboy::follow_menu` |
| No Crosshair While Reloading | `bHideCrosshairWhileReloading` | `reticle::draw` |

## Twenty-second batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Kill XP by Level (0–20% a level) | `bLevelDifferenceAffectsCombatXP` | `Game::kill_xp` |
| Experience for Keys (part) | `bUsingKeysRewardsXP` | `Game::activate` (`unlocked_with`) |
| Experience for Passwords (part) | `bUsingNotesRewardsXP` | `terminal::still_locked` |

## Twenty-third batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Invert Look Left/Right | `bInvertCameraX` | `player::mouse_look` |
| Nearest Unfound Place | `bShowNearestUndiscoveredLocation` | `compass::gather` after the marker loop |
| Discoveries in the Corner | `bLocationDiscoveredCornerMessage` | `compass::discover` |

## Twenty-fourth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Power Armor Blocks Radiation | `bNoEnvironmentRadiationInFullPowerArmor` | `Game::tick_radiation` (`Game::wears_power_armor`) |
| Limb Damage in Power Armor (x0–x2) | `bPowerArmorScalesLimbDamage` | `Game::hit_limb` |
| Endurance Softens Falls (0–20% a point) | `bEnduranceScalesMinFallHeight` | `falls::hurt` (`FallRules::min`) |

## Twenty-fifth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Reverse Category Wheel | `bInvertContainerTitleScrollwheelDirection` | `menu::menu_mouse` |
| Continue at Start | `bAutoContinueGame` | `main_menu::choose` (once per run; Shift held stays: quitting to the main menu restarts the program, so without it the menu could not be reached) |

## Twenty-sixth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| No Ammo Counter | `bHideAmmoLabel` | `hud::gather` |
| Price for Worthless Things (0–10) | `bNoFreeBarterItems` | `MenuModel::refresh_trade` (`free_item_price`) |

## Twenty-seventh batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Skip Easy Locks (+ margin) | `bSkipVeryEasyLocksAtMaxSkill` | `lockpick::opening` (`opened` with its XP) |
| Pick Up Disarmed Mines | `bDisarmingMinesPicksThemUp` | `Game::activate` after a disarm |

## Twenty-eighth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| NPC Weapons Hit Whole | `bNoScaleNpcDamageByCondition` | `combat` NPC blows and shots (`HitInput::condition`) |
| Killing Knockdowns Count | `bLethalHitsIncreaseLimbChallenges` | `Game::knock` |
| Shared Keys | `bAllowDuplicateControlBinds` | `Bindings::rebind`, `rebind_pad` (`allow_duplicates`) |

## Twenty-ninth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| Crippling Never Disarms | `bCripplingDoesntDisarm` | `combat::hit_actor` |
| No Spin-Up | `bAutoWeaponNoFiringDelay` | `combat::fire` (`Timing::spin_up`) |
| Free Crafting in God Mode | `bGodModeAllowsCraftingWithoutRequirements` | `crafting::lacks`, `make_recipe` |

## Thirtieth batch (2026-11-09)

| Row | INI key | Where |
|---|---|---|
| View Follows the Weapon | `bTogglePOVWhenHolsteringWeapon` | `control::read_controls` end |
| Kill Camera Chance (part) | `bChanceBasedKillcams` | `kill_cam::update` (the game's RNG) |
| Subtitle Distance (0–10000) | `bCustomSubtitleDistance` | `script_hud` (`voice::hearing`) |
| Sneak Criticals Head Only | `bSneakCriticalsHeadshotsOnly` | `combat::melee` and `combat::fly` after the body part |
| Compare Weapons | `bCompareWeaponStats` | `pipboy::items::info_of` |
| Weight Warning | `bContainerEncumbranceIndicator` | `menu::look` half figure (`warn_encumbrance`) |
| Mark Nearby Places (0–10000) | `bMarkNearbyLocationsOnMap` | `compass::discover` (ShowMap's set) |
| Ash Piles Fit the Body | `bScaleAshpileSize` | `Game` ash pile creation (`scale_of`) |
| Pip-Boy Buzz | `bHapticPipboy` | `rumble::rumble` on a page or tab change |
| Reload While Firing | `bReloadWhileFiring` | `fnv_combat::attack::Cycle` (`reload_in_recovery`, set by `combat::fire`) |
| Semi-Auto Shot Queue (0–0.5 s) | `bSemiAutoQueue` | `fnv_combat::attack::Cycle` (`semi_auto_queue`) |
| Crafting Uses Repair | `bRepairScalesCraftingCondition` | `crafting::make_recipe`; `fnv_game::tweaks::crafted_condition` (ours: 0.5 + Repair / 200) |
| No Healing In Combat | `bNoHealingInCombat` | `tweaks::healing_barred` in `menu::apply_input` and the Stats page's `use_hints`; a fight is `after_combat::AfterCombat::fighting`. The weapon hotkeys do not check yet |
| Power Attack From a Block | `bPowerAttackIfBlocking` | `fnv_combat::melee::MeleeInput::instant_power`, set by `combat::melee` while Block is held. `bOnlyIfBlocking` is taken as on: outside a block the hold stays |
| Slow Breath Regen (0–20 s/s) | `bSlowBreathRegen` | `Game::breath_rules` gives the player a finite `recover`; the breath meter already stays up until whole (`hud_breath::shown`), so `bShowBreathMeterOutOfWater` is already so |
| V.A.T.S. Hip Fire | `bVATSHipFire` | `vats_mode::read_keys`: `aimed_before` kept while the mode is off; entered not aiming, the firing phase keeps the sights down for guns |
| Caught Pickpocketing Means Detected | `bPickpocketCaughtCausesDetection` | `Game::caught_detects` from `pickpocket_caught`: the pair is recorded as detected (the sneak word's `[DETECTED]` until measured again) and fed to `observe_threat` |
| Agility Scales Jump Height (0–0.2) | `bAgilityScalesJumpHeight` | `combat::stat_speed` puts 1 + mult x (Agility − 1) in `player::StatSpeed`'s second number, a factor on the jump height (the tweak's own formula) |
| Force Lock Near the Sweet Spot (0–50 %) | `bLockpickForceBonusNearSweetSpot` | `lockpick::Picking::force_chance` adds `Settings::force_near_bonus` × (1 − distance / 90°); the falloff is ours (the tweak's formula is not all known) |
| Contact Mines (Player), (Others) | `bContactMines` | `mines::tick`: an actor that sets a mine off within `CONTACT_DISTANCE` (24 units, ours) of its axis gets no fuse |
| Sleep Healing Min Duration (0–12 h) | `bSleepHealingMinDuration` | `aid::hour_passed` calls `Game::pass_hours_healing` with healing held until the hour; every bed counts (who owns the bed is not known there); `bMiscStatRequiresSleepDuration` is not done |
| Barter Lowers Repair Costs (0–60 %) | `bBarterAffectsRepairCosts` | `repair::view` scales `RepairSettings::cost_mult` by `fnv_game::tweaks::barter_repair_factor` (ours; no perk modifiers) |
| Armour Condition Penalty (x0–x2) | `bCustomArmorConditionPenalty` | `combat::target_armor` uses `CombatForms::armor_values_worn_with` and `fnv_combat::condition::armor_mult_with`; the repair menus still show DT at the original's x1 |
| No Pip-Boy In Combat | `bNoPipBoyInCombat` | `menu::open_menus` keeps the inventory request shut while `after_combat::AfterCombat::fighting`, with a corner line (ours). Opening it for Action Points is not done |
| V.A.T.S. Ignores Companions | `bIgnoreCompanionsVATS` | `vats_mode::barred` gains `Bar::Companion` for teammates (marks and the preview); always, `bCombatOnly` is not done |
| Frenzied Kills Earn Experience | `bFrenzyGrantsPlayerXP` | `Game::kill_earns_xp` (now `&mut`) counts a killer at Aggression 3, Frenzied [documented: GECK wiki Aggression] |
| Free Wild Wasteland | `bFreeWildWasteland` | `chargen::TraitSheet::free`: the trait with editor ID `WildWasteland` [observed: FalloutNV.esm] does not count against `max` |
| Pip-Boy page and tab keys (seven) | `iPipboyStatsKey`, `iPipboyItemsKey`, `iPipboyDataKey`, `iOpenLocalMapKey`, `iOpenMapKey`, `iOpenQuestsKey`, `iOpenRadioKey` | new unbound Controls; `tweaks::keys` sets `PipBoy::open_at` and asks for the inventory, `pipboy::follow_menu` opens on that page and tab |
| True Iron Sights key | `iToggleTrueIronSightsKey` | a new Control; `nvforever` `player_body::toggle_iron_sights` flips `PlayerBody::true_iron_sights` |
| Experience as a Percentage | `bStatsMenuPercentageXP` | `pipboy::stats::read` fills `Snapshot::xp_percent`, the way from this level's experience to the next |
| Repair List: Same Item First / Low Condition First | `bSortPipboyRepairMenu`, `bInvertPipboyRepairMenuSorting` | `pipboy::repair::tweak_order` on the offer and after each repair; `iSortMode` is not done |

Every tweak has now been looked at against the code: each "to do" row says what it
needs, and the "already so" notes say where NVForever does it.

Menu tweaks that code holding the game reads (the Pip-Boy's item info, crafting, the
level-up menu, the terminal's timers) are kept in `fnv_game::tweaks` too, so no system
needs another parameter; the catalogue's notes say where NVForever already does what a
tweak asks ("already so").

## Next

The "to do" rows, the ones Viva New Vegas turns on first. Not yet: reading a player's
own `nvse_stewie_tweaks.ini` as the starting values (it would need the INI read from the
game's Data folder, read-only).

## Catalogue

<!-- generated: the table below lists every tweak of AllTweaks.json (v9.87) by its INI key -->
| Status | Count |
|---|---|
| done | 350 |
| to do | 253 |
| unsure, skipped | 17 |
| n/a | 15 |
| skipped: fix | 10 |

### Gameplay

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bActorsStickToSlopes` | done |  | 2 | already so: characters settle onto walkable ground going down (fnv-phys character) |
| `bAgilityScalesJumpHeight` | done |  | 1 | Agility Scales Jump Height (1 + mult x (Agility - 1)) |
| `bAgilityScalesMovementSpeed` | done |  | 3 | Agility Moves Faster (slider, every actor; the player-only and NPC-only options not built) |
| `bAimingSpeed` | done |  | 2 | Full Pace While Aiming (aiming and blocking together) |
| `bAimingUnholstersWeapons` | done |  |  | Aiming Draws the Weapon |
| `bAllowAidAtMaxHealth` | done |  | 3 | Aid at Full Health (the STATS page hints; per-item options not built) |
| `bAllowSleepWaitWithReducedMaxHealth` | done |  |  | already so: no wait refusal looks at health |
| `bAmmoSwapWindowMatchesReloadTime` | done |  |  | already so: an ammo swap is taken at any moment, mid-reload too |
| `bAutoUnlockTerminals` | done |  | 2 | Skip Easy Hacks (switch + margin slider; the XP option not built) |
| `bAutoWeaponJamWhileFiring` | done |  |  | already so: every shot rolls the jam, mid-burst too |
| `bBarterAffectsRepairCosts` | done |  | 3 | Barter Lowers Repair Costs (our rule: a percent off at Barter 100, in proportion below; no perk modifiers) |
| `bBarterCheckActorBuySellFlags` | done |  | 2 | Merchants Keep to Their Lists (the ammo and perk options not built) |
| `bBillyGoatMode` | done |  | 3 | Steepest Walkable Slope (30-80 degrees; one angle for walking, jumping and auto-walk) |
| `bCompanionsDontUseAmmo` | done |  |  | already so: NPCs carry no ammo and reload without spending any |
| `bContactMines` | done |  | 2 | Contact Mines (Player) and (Others): a mine stood on, within 24 units of the body's axis, goes off with no fuse |
| `bCrippledLegsScaleJumpHeight` | done |  | 2 | Jumping on a Crippled Leg (one part per crippled leg) |
| `bCripplingDoesntDisarm` | done |  |  | Crippling Never Disarms |
| `bCustomArmorConditionPenalty` | done |  | 1 | Armour Condition Penalty (x0 to x2 on the penalty below half condition; the repair menus still show DT at x1) |
| `bCustomSpecialPoints` | done |  | 1 | S.P.E.C.I.A.L. Points (30 to 70; a script's own total still wins) |
| `bDisableFastTravel` | done |  |  | No Fast Travel |
| `bDisableReloadingNonEmptyClip` | done |  | 2 | Reload Only When Empty (the energy-only and looping options not built) |
| `bDrowningDrainsAP` | done |  |  | Drowning Takes AP First |
| `bEnduranceScalesMinFallHeight` | done |  | 2 | Endurance Softens Falls (slider; the base bonus option not built) |
| `bExplosionKnockdownAvoidanceUsesStrength` | done |  |  | Strength Resists Blasts |
| `bExtendLockpickSpotRange` | done |  |  | already so: the sweet spot can sit anywhere on the half circle |
| `bFastTravelOverencumbered` | done |  |  | Fast Travel Encumbered |
| `bFreeWildWasteland` | done |  |  | Free Wild Wasteland (editor ID WildWasteland, a trait on offer) |
| `bFrenzyGrantsPlayerXP` | done |  |  | Frenzied Kills Earn Experience (Aggression 3) |
| `bGodModePreventsJamAnims` | done |  |  | No Jams in God Mode |
| `bGrabbingItemsIsCrime` | done |  | 1 | Grabbing Is Stealing (any value; the minimum-value option not built) |
| `bHittingWeaponsDoesntDamageThem` | done |  |  | Shots Spare Weapons |
| `bHoldToHolsterMeleeWeapons` | done |  |  | already so: a tap reloads, holstering needs the hold, and melee drops the reload |
| `bJumpingCostsAP` | done |  | 3 | Action Points per Jump (slider 0-50; the combat-only and jump-anyway options not built) |
| `bJumpWhileOverencumbered` | done |  |  | already so: NVForever never refuses the jump when over-encumbered |
| `bLevelDifferenceAffectsCombatXP` | done |  | 2 | Kill XP by Level (slider; scales the game's kill XP per level of difference instead of the plugin's base) |
| `bLightStepAffectsCompanions` | done |  |  | Light Step for Companions |
| `bLockNeedsKeyShowName` | done |  |  | Name the Key |
| `bLockpickForceBonusNearSweetSpot` | done |  | 2 | Force Lock Near the Sweet Spot (our falloff: full bonus on the middle, none 90 degrees away) |
| `bLockpickRememberBobbyPinAngle` | done |  |  | already so: a new pin keeps the angle |
| `bLootUnconsciousVictims` | done |  |  | Search the Unconscious |
| `bMarkNearbyLocationsOnMap` | done |  | 1 | Mark Nearby Places (slider, game units) |
| `bModifySkillPointsEarned` | done |  | 2 | Own Skill Points Rule (switch + base and per-Intelligence sliders) |
| `bNoAmmoFromTakingNPCWeapon` | done |  |  | already so: taking a weapon adds only its placed ammo |
| `bNoAPRegenWhileOverencumbered` | done |  | 2 | Action Points Back When Over-Encumbered (slider; the while-moving option not built) |
| `bNoDamageMeleeWeaponIfTargetDead` | done |  |  | Spare the Blade on the Dead |
| `bNoDialogueZoom` | done |  |  | already a row: Gameplay tab, dialogue-zoom |
| `bNoEnvironmentRadiationInFullPowerArmor` | done |  |  | Power Armor Blocks Radiation (body armour; the helmet is not asked for) |
| `bNoFastTravelIfLegsCrippled` | done |  | 1 | No Travel on a Crippled Leg (NVForever's own refusal text) |
| `bNoFreeBarterItems` | done |  | 1 | Price for Worthless Things (slider, 0-10 caps) |
| `bNoGrabOwnedItems` | done |  |  | No Grabbing Owned Items |
| `bNoLockFailedTerminals` | done |  |  | No Terminal Lockout |
| `bNoPlayerStaggerAnims` | done |  |  | No Player Stagger |
| `bNoSkillTags` | done |  | 6 | No Skill Tags (the outline, percent and XP options not built) |
| `bNPCsTakeLimbFallDamage` | done |  |  | already so: falls hit a leg for NPCs too |
| `bOnlyAllowWaitWhileSitting` | done |  | 3 | Wait Only When Seated (NVForever's own refusal text) |
| `bPerksAffectFistWeaponSpeed` | done |  |  | already so: the fists are the weapon the perks are asked about |
| `bPickLocksWithoutSkill` | done |  | 1 | Pick Any Lock (the lock plays as before; the difficulty option not built) |
| `bPickpocketCaughtCausesDetection` | done |  |  | Caught Pickpocketing Means Detected |
| `bPickpocketFleeingActors` | done |  |  | Pickpocket the Fleeing |
| `bPowerArmorNeedsNoTraining` | done |  |  | Power Armor Without Training |
| `bPowerArmorScalesFallDamage` | done |  | 2 | Falls in Power Armor (slider; applies to everyone, the player-only option not built) |
| `bPowerArmorScalesLimbDamage` | done |  | 3 | Limb Damage in Power Armor (slider, every limb; the head and mode options not built) |
| `bPowerAttackIfBlocking` | done |  | 1 | Power Attack From a Block (a press of attack while blocking is a power attack at once; bOnlyIfBlocking taken as on) |
| `bPreventHittingHolsteredWeapons` | done |  | 2 | already so: only a drawn weapon has a hit part |
| `bPreventNPCAddiction` | done |  |  | No NPC Addiction |
| `bPreventReloadWeaponUnholstering` | done |  |  | Ready Does Not Draw |
| `bPreventStealingCapsAfterRepair` | done |  | 1 | Repair Caps Go to the Till |
| `bReduceXP` | done |  | 2 | Experience Earned (slider, x0 to x2) |
| `bReloadingPreventsWeaponHotkeys` | done |  |  | No Hotkeys While Reloading (the weapon wheel and its taps) |
| `bReloadingWithFullClipSwitchesAmmoType` | done |  |  | Full Reload Swaps Ammo |
| `bReloadJamsAffectedByAgility` | done |  |  | Jams at Reload Pace |
| `bReloadWhileFiring` | done |  |  | Reload While Firing (during the last shot's recovery) |
| `bRepairScalesCraftingCondition` | done |  |  | Crafting Uses Repair (half condition at Repair 0, whole at 100) |
| `bRepairsRewardXP` | done |  | 1 | Experience per Repair (slider; Pip-Boy repairs) |
| `bRunningCostsAP` | done |  | 8 | Action Points per Second Running (slider 0-20, a flat rate; the Endurance formula not built) |
| `bRunSlowerInWater` | done |  | 1 | Wading Pace (slider; deeper water slows more) |
| `bScalePlayerMeleeReach` | done |  | 3 | Melee Reach (multiplier slider; the min and max options not built) |
| `bSellStolenItems` | done |  |  | already so: barter never checks stolen items |
| `bSemiAutoQueue` | done |  | 2 | Semi-Auto Shot Queue (slider, up to 0.5 s before the weapon is ready) |
| `bSeparateAlcoholEffects` | done |  |  | already so: effects are kept per item, so different drinks stack |
| `bSkipVeryEasyLocksAtMaxSkill` | done |  | 2 | Skip Easy Locks (+ margin; full XP; use again to go through) |
| `bSleepHealingMinDuration` | done |  | 2 | Sleep Healing Min Duration (hours; every bed, the bed's owner is not known; bMiscStatRequiresSleepDuration not done) |
| `bSleepOnChairs` | done |  |  | Sleep on Chairs |
| `bSlowBreathRegen` | done |  | 2 | Slow Breath Regen (seconds of breath a second at the surface; the meter already stays up until whole) |
| `bSpeedMultScalesJumpHeight` | done |  |  | Speed Scales the Jump (the player; NPCs do not jump) |
| `bStealingSendsNoAlarm` | done |  |  | Quiet Theft |
| `bUnequipWeaponMods` | done |  | 7 | already so: attached mods can be taken off and come back (item_mod detach) |
| `bUseCustomSniperZoomRate` | done |  | 2 | Scope Zoom Time (slider; the zoom-out option not built) |
| `bUsingKeysRewardsXP` | done |  | 3 | Experience for Keys (a share of the picking XP; the base and skill options not built) |
| `bUsingNotesRewardsXP` | done |  | 3 | Experience for Passwords (a share of the hacking XP, once; the base and skill options not built) |
| `bVATSHipFire` | done |  |  | V.A.T.S. Hip Fire (entered not aiming: every gun shot from the hip) |
| `bVATSIgnoreNonExplosiveProjectiles` | done |  |  | already so: no projectile can be targeted in V.A.T.S. |
| `bWeightlessWornArmor` | done |  | 1 | Worn Armor Weighs Nothing (weightless; the scale option not built) |
| `bWeightlessWornPowerArmor` | done |  | 3 | Worn Power Armor Weighs Nothing (weightless; the scale and requirement options not built) |
| `iPerksPerLevel` | done |  | 1 | Perks per Level (1 to 5) |
| `bHiddenFragsFix` | to do | yes |  | NPCs' own unplayable explosives are hidden from the pickpocket list |
| `bMoreFrequentNPCLightUpdates` | unsure, skipped | yes | 1 | NVForever lights actors its own way; unclear what to change |
| `bAdjustableScopeZoom` | to do |  | 14 | needs a zoom level in AimZoom and the wheel (now the third-person distance) |
| `bArmorCausesSinking` | to do |  | 4 | the physics step does not know the worn armour class |
| `bCapSkillsBySPECIAL` | to do |  | 5 | SkillSheet needs a per-row cap read at three places |
| `bCharacterRadiation` | to do |  | 1 | not built: only the player takes environment radiation |
| `bClearNearbyPlayerMarker` | to do |  | 1 | not built: no player-placed map marker |
| `bCrippledLimbsPlayPainSoundWhenFalling` | to do |  | 1 | falls::hurt with a crippled leg; NVForever has no pain sound, only the landing ones |
| `bCritChanceIgnoresFireRate` | to do |  |  | crit_chance in fnv-combat; Rules is serialized |
| `bDisarmRequiresSkill` | to do |  | 2 | disarming happens in fnv_sim World::activate without skill or game |
| `bEncumbranceIncludesGrabbedItems` | to do |  |  | encumbered_walks plus the grabbed reference weight |
| `bExplosionRadiusBuff` | to do |  | 3 | explosion_damage reads CombatTuning (serialized) |
| `bExplosionsDontPushTakeableItems` | to do |  |  | push_dynamic knows no references |
| `bFallDeathsCauseDismemberment` | to do |  |  | falls::hurt lacks the dismember queue |
| `bFastTravelCostsSpecialNukaBottles` | to do |  | 3 | a new refusal plus taking the bottle in travel::go |
| `bForceLockpickNoBreakLock` | to do |  | 3 | Picking::force; the engine apply would need the pin-broken path |
| `bHardcoreTweaks` | to do |  | 3 | unsure which way each option goes outside Hardcore |
| `bHipFireAnimsWhileScoped` | unsure, skipped |  |  | depends on alternate animations |
| `bHoldToActivate` | to do |  | 9 | a hold on E is taken by grab; needs a timer and per-kind choices |
| `bHostilesPrioritizePlayer` | to do |  | 3 | Awareness::combat_target has no game or rolls |
| `bImprovedHacking` | to do |  | 10 | each option is its own rule in hacking.rs |
| `bJumpSwimsUpwards` | to do |  | 2 | MoveIntent.jump is a one-shot latch; needs a held flag |
| `bKeepFallHeightOnLoad` | to do |  |  | the fall top is not saved |
| `bLuckDoesntAffectGambling` | to do |  | 1 | already so? no Luck in the gambling rules found; check the original first |
| `bMapSelectableCompanions` | to do |  | 2 | not built: companions are not on the map and cannot be summoned |
| `bMousewheelScrollsWeaponHotkeys` | to do |  | 1 | needs cycle state in weapon_wheel and the wheel off the camera |
| `bMovementPenalties` | to do |  | 11 | player::step pace chain; seven percentages to choose |
| `bNoCompanionItemDamage` | to do |  |  | wear_weapon and wear_armor; the armour path has no game |
| `bNoLockEncounterZoneLevels` | unsure, skipped |  |  | encounter-zone level locking not yet checked in NVForever |
| `bNPCsDetectMineExplosions` | to do |  |  | the mine trigger actor is not carried to the explosion |
| `bNPCsDropLiveGrenades` | to do |  |  | not built: NPC grenades have no in-hand phase |
| `bNPCsEarnAmmoCasings` | to do |  |  | actor_attacks; spent_ammo_item is player-only |
| `bOverencumberedTweak` | to do |  | 8 | encumbered_walks and ap_costs together, with a timer |
| `bPerBulletLoopingReloads` | to do |  | 2 | a press counter on the attack Cycle |
| `bPickpocketOverhaul` | to do |  | 15 | Steal::chance needs weight, Perception and detection |
| `bPlantingLiveGrenadesRequiresWeaponSkill` | to do |  |  | pickpocket::results; a dud when under the skill |
| `bPreventRepairIfNotAtWorkbench` | to do |  |  | no notion of a workbench in reach |
| `bQuickUse` | to do |  | 9 | two input paths that do not exist yet |
| `bRememberBobbyPinHealth` | to do |  |  | Picking::new starts a full pin; needs a stored health |
| `bReviveUnconsciousCompanions` | to do |  | 3 | not built: no revive action |
| `bRunAndGunAPInCombatOnly` | to do |  |  | not built: no run-and-gun AP rule |
| `bScaleWeaponVolume` | to do |  | 2 | PlaySound has no volume |
| `bScopeHoldBreath` | to do |  | 7 | scope_view has no input or AP |
| `bSneakAttackWithoutCrouching` | to do |  |  | sneaking comes from control.sneak (melee) and the projectile; both need the switch |
| `bSneakingDoesntForcePowerAttacks` | unsure, skipped |  |  | depends on separate animations |
| `bStrengthAffectsAllThrowables` | to do |  |  | not built: fThrowingStrengthPenalty is not read |
| `bTargetProjectilesInVATS` | to do |  | 1 | not built: projectiles in flight are not targetable |
| `bUseArmorRepairKitsInRepairMenu` | to do |  |  | the repair menu lists only parts |
| `bUseCustomBarterPriceMultipliers` | to do |  | 13 | refresh_trade price; ten multipliers |
| `bUseWeaponRepairKitsInRepairMenu` | to do |  | 2 | the repair menu lists only parts |
| `bVATSDequeueWeaponShotsOnDisarm` | to do |  |  | vats_mode::shoot needs ActorParts |
| `bVATSTargetPlayerProjectiles` | to do |  |  | not built: projectiles in flight are not targetable |
| `bVATSThrowablesUseVisibility` | to do |  |  | not built: V.A.T.S. chance has no visibility term |
| `bWeaponModdingSkillRequirement` | to do |  |  | item_mod keys needs the skill and two params |
| `iMaxCharacterLevel` | to do |  | 1 | experience::level_cap adds 5 per story add-on; choose how the cap is set |

### Combat

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAllowTeammatesUseMeltdown` | done |  |  | Meltdown for Companions |
| `bAutoWeaponNoFiringDelay` | done |  | 1 | No Spin-Up (the player; the heavy-weapons option not built) |
| `bBloodyMessGibTargetedLimbOnly` | done |  |  | already so: only the struck part is checked |
| `bCharacterMeleeDamageIgnoresScale` | done |  |  | already so: no damage formula reads actor scale |
| `bCharacterNonMeleeDamageIgnoresScale` | done |  |  | already so: no damage formula reads actor scale |
| `bChargedAttacksCostAP` | done |  | 4 | Action Points per Power Attack (slider, a flat cost for every power attack; the weight factor and the refusal not built) |
| `bClipSizeMatters` | done |  |  | already so: a shot needs a whole burst loaded (attack has_round) |
| `bDifficultyDoesntAffectNPCToNPCDamage` | done |  | 3 | already so: difficulty applies only when the player hits or is hit |
| `bDisableCombatMusic` | done |  |  | No Battle Music |
| `bEnteringVATSCostsAP` | done |  | 4 | Action Points to Enter V.A.T.S. (slider; refused without them; the fail-only and no-target options not built) |
| `bHideEnemyMarkers` | done |  | 1 | No Enemies on the Compass (the firing-enemies option is not built) |
| `bIgnoreCompanionsVATS` | done |  | 1 | V.A.T.S. Ignores Companions (always; bCombatOnly not done) |
| `bKillsRewardAP` | done |  | 1 | Action Points per Kill (slider 0-50; given in VATS too) |
| `bMaxVATSDistanceUsesWeaponRange` | done |  | 3 | already so: V.A.T.S. reach is the projectile range |
| `bNoCompanionKillXP` | done |  | 1 | No XP for Companion Kills (also leaves their damage out of the kill share) |
| `bNoDisarmCompanions` | done |  |  | Companions Keep Their Weapons |
| `bNoHealingInCombat` | done |  | 1 | No Healing In Combat (Pip-Boy inventory and Stats page; the weapon hotkeys still use aid) |
| `bNoPipBoyInCombat` | done |  | 5 | No Pip-Boy In Combat (a corner message; the AP option bAllowPipboyUsingActionPoints not done) |
| `bNoScaleNpcDamageByCondition` | done |  |  | NPC Weapons Hit Whole |
| `bNoSelfExplosionDamage` | done |  |  | Own Blasts Spare the Player |
| `bNoSneakAttacksCriticals` | done |  |  | No Sneak Criticals |
| `bNoVATSTargetInvisible` | done |  |  | already so: cloaked actors are barred without Enhanced Sensors |
| `bPlayerMeleeDamageIgnoresScale` | done |  |  | already so: no damage formula reads actor scale |
| `bPlayerNonMeleeDamageIgnoresScale` | done |  |  | already so: no damage formula reads actor scale |
| `bPowerAttackWhileOverencumbered` | done |  |  | already so: nothing gates power attacks on weight |
| `bSneakCriticalsHeadshotsOnly` | done |  |  | Sneak Criticals Head Only |
| `bSneakCriticalsOnlyMeleeWeapons` | done |  |  | Sneak Criticals Melee Only |
| `bFatalNonSneakCritsAlwaysGib` | to do |  | 1 | dismember checks carry no critical flag |
| `bFirstPersonVATS` | to do |  |  | VATS keeps the current view; forcing one goes in vats_mode |
| `bHoldAndReleaseThrowables` | to do |  | 1 | not built: no hold-to-release throw |
| `bLocationalMeleeDamage` | to do |  | 3 | NPC blows on the player use no part yet; NPC on NPC has no part lookup |
| `bNoScaleNpcDamageResistanceByCondition` | to do |  |  | target_armor computed twice for NPC targets |
| `bNoScaleNpcDamageThresholdByCondition` | to do |  |  | target_armor computed twice for NPC targets |
| `bNoSelfMeltdownDamage` | to do |  | 3 | the blast loop by the Meltdown explosion form |
| `bNoWeaponConditionDamagePenalty` | to do |  |  | condition_mult is a free function; needs a Rules field (serialized) |
| `bNPCsCanDisarmPlayer` | to do |  |  | the player weapon lives in Combat; dropping it is new |
| `bNPCsCanSneakCritPlayer` | to do |  |  | sneak_attack is player-only |
| `bRemoveWeaponDamageBuffer` | to do |  |  | condition_mult is a free function; needs a Rules field (serialized) |
| `bScaleCriticalDamage` | to do |  | 1 | needs a field on fnv-combat Rules; check what its serialized dumps feed first |
| `bVATSAutoTargetHead` | to do |  |  | V.A.T.S. picks no target on entry |
| `bVATSStopBurstIfTargetDead` | to do |  |  | vats_mode shoot with the target dead |
| `bWeaponRequirementsMatter` | to do |  | 7 | equip_at has no actor values |

### Quality-of-life

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bBetterAutoWalk` | done | yes | 1 | already so: strafing does not end auto-walk |
| `bConsoleHistoryNoStoreDuplicates` | done | yes |  | already so: the console keeps no repeated line |
| `bCrouchWhileEquippingWeapons` | done | yes |  | already so: crouching is never refused while (un)equipping |
| `bDecreasedDialogueClickDelay` | done | yes |  | already so: topics can be chosen at once |
| `bDelayPostCombatLevelUp` | done | yes |  | already so: the level-up menu waits for combat to end (after_combat) |
| `bDelayPostCombatReputationPopup` | done | yes |  | already so: the reputation box waits for combat to end |
| `bDeselectHotkeys` | done | yes |  | Unassign Hotkeys |
| `bFasterEnterLockpickMenu` | done | yes |  | already so: the lockpick menu opens at once |
| `bFasterHackingTransition` | done | yes |  | Faster Hacking Exit (0.5 s instead of 2 s) |
| `bFasterSaveMenuClose` | done | yes |  | already so: saving leaves the pause menu open, no wait |
| `bGodmodePreventsLegCrippleSound` | done | yes |  | Quiet Landings in God Mode (NVForever's pain sound on a fall is the hard-landing sound) |
| `bHideCrosshairInKillcams` | done | yes |  | already so: no crosshair in a kill camera |
| `bHideHealthbarInKillcams` | done | yes |  | already so: the HUD is hidden in a kill camera |
| `bLeftAndRightCancelEachOther` | done | yes |  | already so: left and right held together cancel |
| `bLessRestrictiveVATSMenu` | done | yes |  | already so: VATS mode has no zoom phase to wait for |
| `bNonSelectablePlayerInConsole` | done | yes |  | Console Clicks Skip the Player |
| `bNoSkillMessageIfIncreaseIsZero` | done | yes |  | No Message for Spent Books (the two-decimal option is moot: book points are whole) |
| `bPipboyLightAndHolsteringIgnoreTimescale` | done | yes |  | Holster in Real Time (the Pip-Boy light already counts real time) |
| `bQueueWeaponHolsteringWhileAnimsPlay` | done | yes |  | already so: a holster is never dropped (it waits for a draw, else starts at once) |
| `bScaleMusicVolumeDuringDialogue` | done | yes | 1 | Music While Talking (slider) |
| `bScaleRadioSongVolumeDuringDialogue` | done | yes | 1 | Radio While Talking (slider) |
| `bSkipIntroVideo` | done | yes |  | already so: the intro plays only after New Game and any key skips it |
| `bVatsExitKey` | done | yes | 1 | Skip Kill Camera (right click; Tab is the Pip-Boy key here) |
| `bAllowFiringWhileLanding` | done |  |  | already so: firing waits only for the weapon to be ready |
| `bAllowOpenPipboyDuringCameraShake` | done |  |  | already so: no camera shake blocks the Pip-Boy |
| `bAllowVATSWhileAnimsPlay` | done |  |  | already so: no animation refuses V.A.T.S. |
| `bAllowWeaponHotkeysAndPipBoyWhileReloading` | done |  |  | already so: nothing refuses them while reloading |
| `bAllowWeaponHotkeysWhileEquipping` | done |  |  | already so: the swap waits its turn |
| `bAllowWeaponHotkeysWhileFiring` | done |  |  | already so: nothing refuses hotkeys while firing |
| `bDisableCompanionKillcam` | done |  |  | already so: only the player's VATS kills start a kill camera |
| `bDisableExplosionInFaceIMOD` | done |  |  | No Explosion Blur |
| `bDisableHolsteredWeaponFOVZoom` | done |  |  | already so: a holstered weapon gives no zoom |
| `bDisableInteriorFog` | done |  | 1 | No Interior Fog (from the next interior; the threshold option not built) |
| `bDisableTutorialMessages` | done |  |  | No Minigame Help (lockpick, blackjack, roulette, slots; Caravan not yet) |
| `bDisableWeaponFOV` | done |  | 1 | No Iron Sight Zoom (the non-sighted option not built) |
| `bFireWhileAiming` | done |  |  | already so: the trigger does not wait for the zoom |
| `bHideMiscQuestItems` | done |  | 1 | Hide Quest Items in Misc (the star caps option not built) |
| `bHoldCrouchToSneak` | done |  |  | Hold to Sneak |
| `bHoldWaitKeyToShowMenu` | done |  | 1 | Hold to Wait (half a second; the hold time option not built) |
| `bHotkeyHolstersWeaponIfEquipped` | done |  | 1 | Hotkey Draws and Holsters (the never-holster option not built) |
| `bHUDRotatesWithVanityCam` | done |  |  | already so: the compass follows the orbiting camera |
| `bJumpingDoesntDropGrabbedItem` | done |  |  | already so: jumping does not let go |
| `bKeepHolotapePlayingWhenSelectingOtherNotes` | done |  |  | already so: only voice and sound notes start playing |
| `bKeepRouletteBetAmount` | done |  |  | already so: the bet stays between spins |
| `bKeepXPBarWhenClosingMenus` | done |  |  | already so: the XP meter holds while a menu is up |
| `bLockpickHackingMessageShowsCurrentSkill` | done |  | 2 | Say My Skill |
| `bMidairFastTravel` | done |  |  | Fast Travel in the Air |
| `bMinBloodSplatterHealthDamage` | done |  | 1 | Blood Only on Big Hits (slider) |
| `bMinigamesPlayXPSoundAtMaxLevel` | done |  |  | XP Sound at the Level Cap (any XP at the cap, not only minigames) |
| `bNoDropWeaponSoundOnPlayerDeath` | done |  |  | already so: the player weapon is not put away at death |
| `bNoFoodWornOffMessage` | done |  | 1 | already so: no worn-off message is shown |
| `bNoKnockdownInGodmode` | done |  |  | No Knockdowns in God Mode |
| `bNoMapMarkerAddedPopup` | done |  |  | already so: NVForever shows no marker-added popup |
| `bNoPipboyIdleAnims` | done |  |  | already so: only the raise clip plays |
| `bNoReputationMessages` | done |  | 5 | Quiet Reputation (box and sound together; per-kind options not built) |
| `bPopupMenusDontMoveCursor` | done |  |  | already so: nothing moves the pointer |
| `bPreventBushPassthroughSounds` | done |  |  | already so: walking through plants makes no sound |
| `bQuestTextVisibleWhileAiming` | done |  |  | already so: aiming hides only the reticle |
| `bRandomizeKillcamMode` | done |  |  | n/a: NVForever has one kill camera kind |
| `bRemoveChemWarnOffIMOD` | done |  |  | No Chem Wore-Off Effect |
| `bRepeatJumping` | done |  |  | Hold to Keep Jumping |
| `bTurnSlowerWhileAiming` | done |  | 2 | Aiming Turn Speed: Across / Up and Down (two sliders) |
| `bUnspokenNPCIndicator` | done |  |  | Mark Strangers |
| `bUseFirstPersonEmptyClipSound` | done |  |  | already so: one empty-clip sound, played at the listener |
| `bBetterPickupPrompt` | to do | yes |  | the prompt shows one unit; needs the stack count and the encumbrance |
| `bDoubleTapReloadToChangeAmmoType` | to do | yes | 2 | the owner moved Lower Weapon off a double tap because it held reloads back; ask first |
| `bFasterControllerPOVRotate` | to do | yes |  | player::mouse_look orbit speed for the controller |
| `bReloadingWithNoAmmoSwapsAmmoTypes` | to do | yes |  | combat::fire is at the system parameter limit |
| `bRestore2Hotkey` | unsure, skipped | yes | 2 | needs the plugin's own menu XML |
| `bSelectUnavailableRadios` | to do | yes |  | the Pip-Boy lists only stations in range |
| `bShiftIgnoresFriendlyVATS` | to do | yes | 1 | friendlies are targetable; a Shift filter belongs in vats_mode::barred |
| `bShiftScreenshotHidesMenus` | to do | yes |  | the capture is asynchronous; MenusHidden must be held a frame or two |
| `bSkipDeathcamHotkey` | to do | yes | 2 | the death camera (nvforever death_cam) has no end but a load yet |
| `bActivatingDoesntStandUp` | to do |  |  | activate and nvforever furniture drive_player |
| `bAddItemUsesItemIcon` | unsure, skipped |  |  | needs an icon replacer |
| `bAddNightVisionToggle` | to do |  | 4 | a night vision state plus flashlight and scope_view |
| `bAllowActivateWhileAnimPlays` | to do |  |  | NVForever never refuses activating during a reload; check what the original refuses |
| `bAllowUnsafeFastTravel` | to do |  | 1 | only cells flagged Can't Travel From Here refuse; the tweak would skip that flag |
| `bDisableHitShader` | to do |  | 1 | NVForever draws no hit shader yet |
| `bDontSetQuestWhenCompletingObjectives` | to do |  |  | pipboy data::lists picks an active quest whenever none is |
| `bDontSetQuestWhenObjectivesAdded` | to do |  |  | pipboy data::lists picks an active quest whenever none is |
| `bEnteringVATSDoesntUnholsterWeapon` | to do |  |  | a drawn-by-VATS flag and a holster on end |
| `bEquipBrokenItems` | to do |  |  | broken weapons are refused in fnv_sim equip_at; needs a World flag like power armour |
| `bForceHiResWeaponModels` | to do |  |  | nvforever actors arm_weapon model_for(true); untested in the hand |
| `bKeepBrokenItemsEquipped` | to do |  |  | the break unequips at three sites; firing a broken weapon is undefined |
| `bLessFrequentPlayerPainSounds` | to do |  | 1 | combat_lines hit_lines with a last-said time |
| `bMoveDuringOpenPipboyAnim` | to do |  |  | the Pip-Boy is a menu; the game leaves Playing at once |
| `bMoveDuringVATSPlayback` | to do |  |  | kill_cam hold_input and ControlInput::held both hold movement |
| `bNoAutoContinueDialog` | to do |  |  | Game::update_dialogue would hold the line timer |
| `bNoEquippedWeaponMovementPenalty` | to do |  |  | combat::run_pace slows two-handed weapons only |
| `bNoExteriorLoadScreens` | to do |  |  | loading::begin interior-to-exterior |
| `bNoKillcamKillSound` | to do |  |  | NVForever plays no sound when a kill camera starts |
| `bNoLTRTOnPipboy` | n/a |  |  | the original's controller texture swap |
| `bNoPoisonConfirm` | to do |  |  | not built: no weapon poison |
| `bNoScreenshotPopup` | to do |  | 3 | NVForever shows no screenshot message yet |
| `bNPCsDropWeaponHolsteredWeapon` | to do |  |  | combat::death_drops from the equipped weapon |
| `bPauseOnSaveLoad` | to do |  |  | loading::finish asks pause::open; tell a load from a new game |
| `bPickLocksEvenWithKey` | to do |  | 1 | activate::activate before activate_by_hand |
| `bRemoveFollowerTopics` | to do |  |  | not built: companions make no such comments |
| `bRemoveLandingAnim` | to do |  |  | fnv_actor jump::JumpState plays the landing clip |
| `bRepairRequiresWeaponSkill` | to do |  |  | pipboy repair keys with the weapon skill requirement |
| `bRobotCompanionsHealWithScrapMetal` | to do |  | 4 | robot detection, another item and a heal formula |
| `bScaleExplosionShake` | to do |  | 2 | not built: explosions do not shake the camera |
| `bScopeVisibilityDelay` | to do |  | 4 | aim_zoom with a held timer |
| `bSkipVigorTesterSpecialPages` | to do |  |  | chargen vigor page and its pose clip |
| `bSlotsAutoSpinHotkey` | to do |  |  | slots keys; check Spin while reels turn |
| `bSwapAmmoWithWeaponHolstered` | to do |  |  | combat::fire before the not-ready return |
| `bToggleControllerIfAttackPressed` | to do |  | 1 | not built: the device changes only by setting |
| `bTurnFurtherWhileSeated` | to do |  |  | nvforever player_body NECK_TURN; drive_body is at 16 params |
| `bUnequipBrokenArmor` | to do |  | 1 | combat::wear_armor w.broke; a bool from its four callers |
| `bWeaponCycleUpDownHotkeys` | to do |  |  | the D-pad is fixed to hotkeys; needs a current slot |

### HUD

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAddRGBSliders` | done | yes | 1 | already a row: Interface tab, hud-red / hud-green / hud-blue |
| `bHideRedCrosshairOnDistantInvisibleTargets` | done | yes |  | Invisible Foes Stay Hidden (at any distance) |
| `bCustomAddToInventoryMessageTimer` | done |  | 1 | Item Added Line Time (slider, seconds) |
| `bCustomSubtitleDistance` | done |  | 1 | Subtitle Distance (slider, game units) |
| `bDisableCompassEdgeAlpha` | done |  |  | already so: the marks are not faded at the edge |
| `bDisableGrenadeIndicator` | done |  |  | already so: NVForever has no grenade indicator |
| `bDisableHUDCrippledLimbIndicator` | done |  |  | No Crippled Limb Figure |
| `bDistanceBasedQuestMarkerVisibilty` | done |  | 2 | Quest Markers Within (slider in game units, one distance indoors and out) |
| `bHideAmmoLabel` | done |  | 1 | No Ammo Counter (the total-remaining option not built) |
| `bHideCapsAddedMessages` | done |  |  | No Caps Added Line |
| `bHideCrosshairWhileReloading` | done |  |  | No Crosshair While Reloading |
| `bHostilesUseNeutralColorOnCompass` | done |  |  | Foes in the Usual Colour |
| `bLocationDiscoveredCornerMessage` | done |  |  | Discoveries in the Corner |
| `bNoCompassPipsIfNotInDanger` | done |  |  | Compass Pips Only in Danger |
| `bNoCrippleCriticalMessages` | done |  | 3 | already so: no crippled or critical popups about the target are shown |
| `bNoKarmaMessages` | done |  | 7 | Quiet Karma (messages and sounds together; no thresholds yet) |
| `bNoLocationPopup` | done |  |  | No Location Banner |
| `bNoQuestFailedPopup` | done |  | 1 | No Quest Failed Banner |
| `bNoRedCrosshairOnEnemies` | done |  |  | No Red Crosshair (native and tile HUD alike) |
| `bNoUnconciousMessage` | done |  |  | No Unconscious Message |
| `bNoXPBarInCombat` | done |  | 1 | No XP Meter in Combat (no delay option) |
| `bNoXPMessages` | done |  | 7 | Quiet Experience (all sources at once; LEVEL UP still shows) |
| `bPatchUnseenCellName` | done |  | 3 | Mark Unvisited Doors (a star; the rename and respawn options not built) |
| `bRemoveDeadNPCCrippleCriticalMessages` | done |  |  | already so: no cripple or critical messages |
| `bRemoveQuestObjectiveAddedText` | done |  | 1 | No Objective List |
| `bRemoveSneakLabel` | done |  |  | No Sneak Label (native and tile HUD) |
| `bScopeVisibleAPHP` | done |  | 1 | already so: HP and AP stay up while scoped |
| `bShowNearestUndiscoveredLocation` | done |  | 2 | Nearest Unfound Place (the compass; the map option not built) |
| `bShowSneakLabelWhileStanding` | done |  |  | Sneak Label While Standing |
| `bSubtitlesShowActorNames` | done |  |  | Subtitles Name the Speaker |
| `iHUDMaxCompassNPCTicks` | done |  | 1 | already so: the compass has no cap on actors |
| `bDescriptiveMarkerAddedMessage` | to do | yes |  | NVForever shows no marker-added popup |
| `bEnemyHealthbarShowBuffedHP` | to do | yes |  | max_health leaves out temporary modifiers; choose how bars share them |
| `bHideGrenadeIndicatorForNoDamageExplosions` | to do | yes |  | not built: no grenade indicator |
| `bMorePreciseRadMeter` | to do | yes | 1 | unclear whether the HUD gain or the Pip-Boy total is meant |
| `bArmorConditionLabel` | to do |  | 9 | no armour condition text on the HUD yet |
| `bColoredHUDBars` | to do |  | 2 | hud::pips colour by threshold |
| `bColorWeaponCndLabel` | to do |  | 2 | hud::draw condition bar colour |
| `bCompanionPipColorChange` | to do |  | 1 | a companion mark kind in compass and tiles |
| `bCompassFadeLeftSide` | to do |  |  | compass::draw alpha on both paths |
| `bCompassLocationDistanceBasedAlpha` | to do |  |  | the compass view carries no distance |
| `bCompassNPCDistanceBasedAlpha` | to do |  |  | the compass draws no alpha yet |
| `bCompassQuestHeightIndicator` | to do |  |  | the compass marks are 2D; needs art |
| `bCriticalHitMessagesIncludeLimbName` | to do |  |  | not built: no critical-hit messages |
| `bDetectedByWhom` | to do |  | 5 | Sneak is a Copy enum; names need a new type |
| `bDisableNeedsMessages` | to do |  | 4 | those messages are the game's own scripts; needs a filter by message |
| `bExplodingDestructibleIndicator` | to do |  |  | not built: no grenade indicator |
| `bHideCrosshairInFirstPerson` | to do |  |  | the crosshair and the prompt share a root in reticle.rs |
| `bHideCrosshairInThirdPerson` | to do |  |  | the crosshair and the prompt share a root in reticle.rs |
| `bHideInvisibleUndiscoveredLocations` | to do |  |  | compass::gather marker loop; markers_of does not carry the map rule |
| `bHideUnknownNPCNames` | to do |  | 1 | activate::find_target with extra.talked_to_pc; needs a label for the unnamed |
| `bHUDFatigueIndicator` | to do |  |  | the needs words are a fixed five |
| `bHUDMarkerNameIndicator` | to do |  | 7 | no such HUD text yet |
| `bHUDShowRegionNames` | to do |  | 1 | not built: the HUD has no region text |
| `bHUDWeaponNameLabel` | unsure, skipped |  | 5 | needs the plugin's own menu XML |
| `bKeepCrosshairWhenAiming` | to do |  |  | reticle::draw is at 16 parameters; goes into a tuple |
| `bMoveAmmoTypeLabel` | to do |  |  | hud::texts label position |
| `bNoHUDHotkeyPopup` | to do |  |  | weapon_wheel step held-to-open |
| `bPlacedMarkerHeightIndicator` | to do |  | 1 | not built: no player-placed markers |
| `bRaceMenuAllowFemaleFacialHair` | to do |  |  | creator model female checks at four places |
| `bRecentlyDeadNPCIndicator` | to do |  | 4 | the compass skips the dead; needs per-actor state |
| `bShowDoorsOnCompass` | to do |  | 5 | a door mark kind, icons and gathering |
| `bShowQuestObjectivesOnCellChange` | to do |  | 1 | cell change detection and objectives for the active quest |
| `bShowUsePasswordOnTerminalsWithNote` | to do |  | 1 | a labelled Action variant for the note |
| `bTalkWhileSneakingIfShiftIsHeld` | to do |  |  | activate::actor_action_in needs the Shift state |
| `bWaterSourcesShowH2O` | to do |  | 1 | not built: water prompts carry no line |
| `iDateFormat` | to do |  | 1 | three date formatters |

### Menus

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bCapitalizeRecipeCategories` | done | yes |  | Recipe Categories in Capitals |
| `bClickingShowsTerminalText` | done | yes |  | already so: terminal text is shown whole at once |
| `bConsoleNumpadSupport` | done | yes |  | already so: numpad digits type into the console (with Num Lock) |
| `bImprovedRaceMenu` | done | yes | 3 | already so: the creator camera pans and zooms close |
| `bInstantContinueButton` | done | yes |  | already so: Continue loads at once |
| `bLivingAnatomyShowDR` | done | yes | 1 | Living Anatomy Shows DR (the decimal and zero-hiding options not built) |
| `bMapRemembersPosition` | done | yes | 1 | Map Remembers Its Place (in the session; the save-persistent option not built) |
| `bNumberedComputerHotkeys` | done | yes | 1 | Number Keys on Terminals (1-9, 0 the tenth; the number prefix option not built) |
| `bNumberedDialogHotkeys` | done | yes | 2 | already so for 1-9 (the 0 key and number prefixes not built) |
| `bShowBookEffects` | done | yes | 1 | Skill Book Effects |
| `bShowWeaponAmmoUseInMenus` | done | yes |  | Rounds per Shot |
| `bSkipSkillMenuIfNoPointsToAssign` | done | yes |  | Perks First When No Points |
| `bSynchronizeContainerCategories` | done | yes | 1 | Matching Categories (the option for emptying a category not built) |
| `bTakeAllInCompanionContainers` | done | yes |  | already so: Take All shows for companions |
| `b12HourPipboyClock` | done |  |  | 12-Hour Pip-Boy Clock |
| `b24HourSleepWaitClock` | done |  |  | already so: the wait clock is 24-hour |
| `bAddInventoryDropItemHotkey` | done |  |  | Q Drops Items |
| `bAllowRebindNumkeys` | done |  |  | already so: the hotkeys and the D-pad are on the Controls tab |
| `bChangingContainerCategoryScrollsToTop` | done |  |  | already so: a category change goes to the top |
| `bCompareWeaponStats` | done |  |  | Compare Weapons (DAM and DPS, both at full condition) |
| `bContainerEncumbranceIndicator` | done |  |  | Weight Warning (the selected row, the whole stack) |
| `bContainerTakeAllDoesntCloseMenu` | done |  |  | Take All Stays Open |
| `bDialogueKeepVoiceActingNotes` | done |  |  | Keep Acting Notes (spoken lines; topics as before) |
| `bDisableInventoryRightClickDrop` | done |  |  | No Right-Click Drop |
| `bDisableLoadingScreenTips` | done |  |  | No Loading Tips |
| `bDisableShowQuestLocation` | done |  |  | No Show Location |
| `bHideCompletedQuests` | done |  |  | Hide Finished Quests |
| `bHideEquippedItemsInBarter` | done |  |  | Hide Worn Items in Barter |
| `bHideEquippedItemsInContainers` | done |  |  | Hide Worn Items in Containers |
| `bHideMapMarkerFactionReputation` | done |  |  | No Reputation on Map Markers |
| `bHideQuestItemWeightAndValue` | done |  |  | No Figures for Quest Items |
| `bHideRanksInTraitMenu` | done |  |  | No Ranks for Traits |
| `bHideUnavailableRadios` | done |  |  | already so: the Pip-Boy lists only stations in range |
| `bIndividialItemStats` | done |  |  | One Item's Figures (the Pip-Boy; containers already show one) |
| `bInvertContainerTitleScrollwheelDirection` | done |  |  | Reverse Category Wheel |
| `bInvertPipboyRepairMenuSorting` | done |  |  | Repair List: Low Condition First |
| `bKarmaTitleShowsValue` | done |  |  | already so: the karma number is under its title |
| `bLockpickAllowMouse` | done |  |  | Mouse Turns the Lock |
| `bMapMarkersShowFactionName` | done |  | 1 | already so: the reputation line names the faction (worded "Goodsprings: Neutral") |
| `bNoCapitaliseContainerCategories` | done |  |  | Quiet Category Titles |
| `bNoExitConfirm` | done |  |  | already so: the main menu's Quit asks nothing |
| `bNoExitHacking` | done |  | 1 | No Leaving a Hack (the fail-on-exit option not built) |
| `bPauseHolotapes` | done |  | 1 | already so for pausing: picking the playing holotape pauses it (reset and dialogue options not built) |
| `bPreventNoFastTravelMessage` | done |  | 1 | Quiet Undiscovered Markers (the sound option not built) |
| `bReputationShowsFameInfamy` | done |  | 3 | Show Fame and Infamy (numbers; the percentage option not built) |
| `bSavingDoesntClosePauseMenu` | done |  |  | already so: saving leaves the pause menu open |
| `bShowActiveQuestNotesShowsAllStartedQuests` | done |  |  | Notes of Every Quest |
| `bShowAlternateAmmoTypesAvailableInMenus` | done |  |  | Mark Other Ammo |
| `bShowCaravanCardsInMiscTabs` | done |  |  | already so: carried cards show in Misc |
| `bShowCurrencyInContainers` | done |  |  | already so: caps and faction currency are listed in Misc |
| `bShowModNameInPerkDescriptions` | done |  | 3 | Perk Plugins (the level-up menu; the Pip-Boy and hide-list options not built) |
| `bShowQuestAndNoDRDTItemsInRepairMenu` | done |  |  | Repairers Take Any Armor (quest items were listed already) |
| `bSkipLoadSaveConfirmationPrompt` | done |  | 1 | already so: a pick on the load page loads at once |
| `bSleepWaitSliderShowsWakeTime` | done |  |  | already so: the hours line shows the wake time |
| `bSortMiscStats` | done |  | 1 | Sorted Statistics (by name; other modes not built) |
| `bSortPerkMenu` | done |  | 1 | already so for one order: available, level, name (other modes not built) |
| `bSortPipboyNotes` | done |  | 1 | already so: notes are alphabetical (by-read not built) |
| `bSortPipboyQuests` | done |  | 2 | already so for one order (modes and active-first not built) |
| `bSortPipboyRepairMenu` | done |  | 2 | Repair List: Same Item First (by name otherwise; iSortMode not done) |
| `bSortRecipeMenu` | done |  | 1 | already so for one order: makeable first (modes not built) |
| `bStatsMenuPercentageXP` | done |  |  | Experience as a Percentage (the way through the level) |
| `bStatsMenuShowEffectTimeRemaining` | done |  | 1 | Effect Time Left (seconds) |
| `bTabClosesPipboyFromKeyAndLimbMenus` | done |  |  | already so: Tab closes the Pip-Boy from every page |
| `bTerminalInstantDisplayHotkey` | done |  |  | already so: terminal text is shown whole |
| `bBarterShowCapsChange` | to do | yes | 1 | half_figure has no settings to read; pass the tweak in |
| `bChallengeMenuIcons` | unsure, skipped | yes |  | needs another mod (challenge icons) |
| `bColorRecentlyAddedMapMarkers` | to do | yes |  | needs a per-session found set (newly_found is drained by the compass) |
| `bDisplayItemEffectTotals` | to do | yes |  | aid_preview returns totals only; the per-second part is lost |
| `bHideCursorInDialog` | to do | yes | 1 | the pointer logic (player::grab_cursor) is shared with every menu |
| `bLevelUpScrollWheelSupport` | to do | yes |  | the wheel scrolls the rows; needs the hovered row to take it |
| `bMainMenuContinueIcon` | to do | yes |  | thumbnails are on the load page only |
| `bPlaceMarkersAtLocations` | to do | yes | 1 | placing a marker on the map is not built |
| `bRecurringChallengeIndicator` | to do | yes | 1 | challenges::Row has no recurring flag yet |
| `bRememberPipboyScrollPositions` | to do | yes |  | the inventory is a new MenuModel on each opening |
| `bRemoveDownloadsButton` | n/a | yes |  | NVForever has no Downloads button |
| `bSaveCharacterSelector` | unsure, skipped | yes | 1 | needs the plugin's own menu XML |
| `bSeparateHorizontalSensitivity` | to do | yes |  | one mouse sensitivity; mouse_look is at 15 parameters |
| `bShowWeaponPoisonEffects` | to do | yes |  | NVForever has no weapon poison yet |
| `bSortEquipableAmmo` | to do | yes |  | rows sort by name (menu/model.rs sort_rows) |
| `bAmmoLabelUseLongName` | unsure, skipped |  | 1 | needs a UI mod |
| `bBarterQuantityMenuShowsPrice` | to do |  |  | the quantity box has no price |
| `bBetterMapZoom` | to do |  |  | pipboy data take_input zoom around the pointer |
| `bBlackJackTotalDisplay` | to do |  |  | blackjack screen rows |
| `bClickingActiveTabTogglesSorting` | to do |  |  | not built: one fixed order |
| `bCompanionsUseLocationFontOnMap` | to do |  |  | not built: companions are not on the map |
| `bCompareArmorStats` | to do |  |  | needs the worn pieces of the same slots summed |
| `bContainerMenuStoreAllHotkey` | to do |  | 1 | a Store All command |
| `bContainerQuantityMenuShowsWeight` | to do |  |  | the quantity box has no weight |
| `bContainersShowEquippedAmmo` | to do |  |  | not built: ammo is never equipped |
| `bControllerBackButtonDoesntClosePipBoy` | to do |  |  | pipboy close_keys with the pad East button |
| `bCustomHackingAnswerLength` | to do |  | 8 | Difficulty::word_lengths and the perk |
| `bCustomPopupIcons` | to do |  | 17 | per-kind icons at each message |
| `bEmboldenTagSkills` | to do |  |  | no tagged flag on rows and no bold face |
| `bHideCursorInMessageMenu` | to do |  |  | player::grab_cursor with the message panel |
| `bHideHotkeyedItemsInBarter` | to do |  |  | the menu model has no hotkey view |
| `bHideHotkeyedItemsInContainers` | to do |  |  | the menu model has no hotkey view |
| `bHideHotkeyedItemsInRepairLists` | to do |  |  | repair offer has no hotkeys |
| `bHideReadNotes` | to do |  |  | data lists with note_read; the shown note marks itself read |
| `bHideReputationTabIfEmpty` | to do |  |  | stats view and switch; the flag on Snapshot |
| `bInteriorExteriorMapDoorIcon` | to do |  |  | doors carry no leads-outside field |
| `bKeyRepeatAcceleration` | to do |  | 4 | not built: menu keys do not repeat |
| `bLocalMapRespawnedCellIndicator` | to do |  | 2 | not built: the local map has no cell grid |
| `bLockPickMenuKeyboardMovement` | to do |  | 3 | lockpick keys and torque_held |
| `bMapLocationDisplayDistance` | to do |  | 3 | world_map detail line |
| `bMapRecenterHotkey` | to do |  | 2 | pipboy data take_input |
| `bMapShowUnconsciousCompanions` | to do |  |  | companions are not drawn on the world map yet |
| `bMarkNotesUnread` | to do |  |  | a right click calling SetNoteRead 0 |
| `bMoreDetailedStatsMenu` | to do |  | 1 | stats read value and cap |
| `bNoMinCompanionMapDistance` | to do |  |  | companions are not drawn on the world map yet |
| `bNoPlaceMarkerPopup` | to do |  | 2 | not built: no custom markers |
| `bNoteMenuShowSeparateMenu` | unsure, skipped |  | 1 | needs another mod (Book Menu Restored) |
| `bPreviewPerksOnLevelUp` | to do |  |  | a count-0 perk sheet |
| `bQuantityMenuRespectsCompanionCarryCap` | to do |  |  | asks_quantity has no world |
| `bQuestTextVisibleInMenus` | to do |  |  | quest_messages held; check what draws over the Pip-Boy |
| `bRepairAllConfirmation` | to do |  | 2 | a confirmation box in repair |
| `bRepairItemsPreview` | to do |  | 1 | a preview session in the repair list |
| `bRightClickChangesToPreviousContainerCategory` | to do |  |  | menu_mouse right click over a title |
| `bShowBarterTotalWhenOverSellLimit` | to do |  |  | tiles barter snapshot amount; a menu bool |
| `bShowInventorySortButton` | unsure, skipped |  | 11 | needs the plugin's own menu XML |
| `bStartMenuQuickLoad` | to do |  |  | save::requests in the pause menu |
| `bTabBackInStartMenu` | to do |  |  | pause::choose back conditions; Tab steps tabs on two pages |
| `bTabClosesTerminals` | to do |  |  | Tab is Back; from the top menu it closes already |
| `bTakeAllConfirmation` | to do |  | 2 | a confirmation prompt in the menu model |
| `bTerminalGreyReadNotes` | to do |  |  | no record of entries read |
| `bTimescaleAffectsMinigames` | to do |  |  | each minigame steps on AidTime |
| `bVATSAPDisplayIncludesReloadCost` | to do |  |  | not built: V.A.T.S. queues never reload |

### Hotkeys

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `iDisableCollisionKey` | done |  | 1 | a Controls key: No Collision (held) |
| `iExitGameKey` | done |  | 1 | a Controls key: Quit Game |
| `iExitToMainMenuKey` | done |  | 1 | a Controls key: Main Menu |
| `iHolsterWeaponKey` | done |  | 1 | a Controls key: Holster Weapon (unbound until set; the hold to holster stays) |
| `iNextRadioStationKey` | done |  | 1 | already so: ] steps to the next station in the Pip-Boy |
| `iOpenLocalMapKey` | done |  | 1 | Pip-Boy: Local Map key |
| `iOpenMapKey` | done |  | 2 | Pip-Boy: World Map key |
| `iOpenQuestsKey` | done |  | 1 | Pip-Boy: Quests key |
| `iOpenRadioKey` | done |  | 1 | Pip-Boy: Radio key |
| `iPipboyDataKey` | done |  | 1 | Pip-Boy: Data key (Controls page) |
| `iPipboyItemsKey` | done |  | 1 | Pip-Boy: Items key (Controls page) |
| `iPipboyStatsKey` | done |  | 1 | Pip-Boy: Stats key (Controls page) |
| `iPrevRadioStationKey` | done |  | 1 | already so: [ steps to the previous station in the Pip-Boy |
| `iToggleCrosshairKey` | done |  | 1 | a Controls key: Show/Hide Crosshair |
| `iToggleMenusKey` | done |  | 1 | a Controls key: Show/Hide HUD (as ToggleMenus) |
| `iTogglePipboyLightKey` | done |  | 2 | a Controls key: Pip-Boy Light (the night vision part not built) |
| `iToggleSneakIndicatorKey` | done |  | 1 | a Controls key: Show/Hide Sneak Label |
| `iToggleTrueIronSightsKey` | done |  | 1 | True Iron Sights key |
| `iVATSAcceptKey` | done |  | 1 | already so: accepting is the Activate control, rebindable on the Controls page |
| `iConsoleKey` | to do |  | 1 | console toggle Backquote; RESERVED_KEYS |
| `iDropEquippedWeaponKey` | to do |  | 1 | World::drop_equipped from a key; check the visuals follow |
| `iEquipLastWeaponKey` | to do |  | 4 | not built: no previous weapon recorded |
| `iHideHUDKey` | to do |  | 6 | per-element hiding in each HUD module |
| `iPlaceMapMarkerAtPlayerPosKey` | to do |  | 3 | not built: no player-placed markers |
| `iRadioVolumeDownKey` | to do |  | 1 | a key setting the radio sound volume |
| `iRadioVolumeUpKey` | to do |  | 1 | a key setting the radio sound volume |
| `iRepeatActivateKey` | to do |  | 2 | activate is at 16 params; a timer in a tuple |
| `iScreenshotKey` | to do |  | 1 | not built: no player screenshot key outside F12 in nvforever |
| `iSkipRadioSongKey` | to do |  | 1 | the playing line and the broadcast both move |
| `iToggleSightingNodeHotkey` | to do |  | 2 | a second sighting node path per weapon |
| `iToggleSmoothCameraKey` | to do |  | 2 | not built: no look smoothing |

### Cheat

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAllowPickpocketIfAlreadyCaught` | done |  |  | Pickpocket Again |
| `bFastTravelWithEnemiesNearby` | done |  |  | Fast Travel Near Enemies |
| `bGodModeAllowsCraftingWithoutRequirements` | done |  |  | Free Crafting in God Mode |
| `bHackTerminalsWithoutSkill` | done |  |  | Hack Any Terminal |
| `bNoCasinoBans` | done |  |  | No Casino Bans |
| `bNoHackingRetryDelay` | done |  |  | already so: no retry delay (a failed hack locks out) |
| `bNoMaxCasinoBet` | done |  |  | No Casino Bet Limit (no limit; the 500-step option not built) |
| `bPickpocketWornItems` | done |  | 1 | Pickpocket Worn Items (the holstered-weapons-only option not built) |
| `bSleepInOwnedBeds` | done |  |  | Sleep in Owned Beds |
| `bSleepWaitAnywhere` | done |  | 6 | Wait Anywhere (one switch for all; a script ban still refuses) |
| `bWeightlessAidItems` | done |  |  | covered by Weightless Aid (in Hardcore too) |
| `bWeightlessItems` | done |  | 5 | Weightless Armor / Aid / Ammo / Weapons / Misc Items (five rows) |
| `bAddSetHomeCommand` | to do |  |  | not built: a console command with a kept home |
| `bDebugLockpickMenu` | to do |  |  | the meter is a developer aid; developer escape hatches stay out of player builds (owner rule), ask first |
| `bReallocateSkillPointsOnLevelup` | to do |  |  | earlier levels spent points are not recorded |

### Misc

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bCompressDuplicateConsoleMessages` | done | yes |  | Count Repeated Console Lines |
| `bConsolePrintsIncludeTimestamp` | done | yes | 1 | Console Times (time since the game started; a wall-clock format would need a date crate) |
| `bMeleeImpactEffectsFollowTarget` | done | yes |  | already so: decals on actors hang from a bone |
| `bPowerArmorPreventsScreenBlood` | done | yes |  | No Screen Blood in Power Armor (any hit while power armor is worn on the body; hits are not told apart by body part here) |
| `bRadioStaticDecreasesSongVolume` | done | yes |  | already so: the song fades as the static rises |
| `bAllowDuplicateControlBinds` | done |  |  | Shared Keys |
| `bAutoContinueGame` | done |  |  | Continue at Start (once per start of the program; Shift held stays on the menu) |
| `bChanceBasedKillcams` | done |  | 1 | Kill Camera Chance (slider; V.A.T.S. kill cameras, the only kind) |
| `bContainerRespawnsMessage` | done |  | 2 | already so: respawning containers say so (not on NPCs) |
| `bDetachedBeams` | done |  |  | Detached Beams |
| `bDisableGodmodeOnLoad` | done |  |  | God Mode Off on Load |
| `bDisableSellingItems` | done |  |  | No Selling |
| `bDisableStealthEffectInPipboy` | done |  |  | No Stealth Shimmer in the Pip-Boy |
| `bDisableWeaponHotkeys` | done |  |  | No Weapon Hotkeys (the wheel still picks) |
| `bDisarmingMinesPicksThemUp` | done |  |  | Pick Up Disarmed Mines |
| `bDontOpenDoorsAfterLockpick` | done |  |  | Picked Doors Stay Shut |
| `bDontShowContainerAfterLockpick` | done |  |  | Picked Containers Stay Shut |
| `bHapticPipboy` | done |  |  | Pip-Boy Buzz (with rumble on) |
| `bInvertCameraX` | done |  |  | Invert Look Left/Right |
| `bKeepPipboyLightOnCellChange` | done |  | 1 | already so: the light stays on through cell changes |
| `bKeepSelectedConsoleRef` | done |  |  | already so: closing the console keeps the selection |
| `bLethalHitsIncreaseLimbChallenges` | done |  |  | Killing Knockdowns Count (crippling already counted on a kill) |
| `bMenuFadesIgnoreTimescale` | done |  |  | already so: menu fades run on real time while the world waits |
| `bNoAltTabPause` | done |  | 1 | already so: losing focus opens no pause menu |
| `bNoAudioDistortion` | done |  | 1 | No Helmet Voice Filter (the apply-to-everything option not built) |
| `bNoFastTravelTimeChange` | done |  |  | Fast Travel Takes No Time |
| `bNoHotkeyEquipSounds` | done |  |  | No Hotkey Sounds |
| `bNoScrollwheelPOVChange` | done |  | 2 | already so: the wheel only zooms the third-person camera |
| `bNoSkipDialog` | done |  |  | No Skipping Lines |
| `bOnlyChangeCameraHeightIfPOVKeyHeld` | done |  |  | Wheel Zoom Needs Toggle View (NVForever's wheel moves the camera distance) |
| `bOpenPipboyToInventoryByDefault` | done |  | 1 | Pip-Boy Opens on Items (the tab option not built) |
| `bPipBoyTabHotkeysDontCloseMenu` | done |  |  | already so: F1-F3 change page, only Tab or Esc close |
| `bPreventNPCComments` | done |  | 1 | already so: NPCs make no such comments |
| `bPreventTeammateFootstepSounds` | done |  |  | Quiet Companion Footsteps |
| `bRestoreLockedDoorSound` | done |  |  | already so: a refused lock plays DRSLocked |
| `bScaleAshpileSize` | done |  | 2 | Ash Piles Fit the Body (the actor's scale, half to twice; not its bounds) |
| `bScaleRadioVolume` | done |  | 2 | Radio Songs Volume / Radio Talk Volume (two sliders) |
| `bTogglePOVWhenHolsteringWeapon` | done |  |  | View Follows the Weapon |
| `bAllowKeyboardAndMouseWithControllerConnected` | to do | yes |  | the Input Device setting picks one device; needs both at once |
| `bArmorPreventsBloodDecals` | to do | yes |  | the impact is sent before DT is worked out |
| `bArmorSoundsPlayIn3D` | to do | yes |  | the player's foley plays at the listener (anim_sounds player_keys) |
| `bBetterFlycam` | to do | yes | 5 | up/down and wheel speed are already there; auto-move is not |
| `bConsoleBackground` | unsure, skipped | yes |  | needs the plugin's own menu XML |
| `bConsoleTextShadow` | to do | yes |  | the console draws green text on a dark panel, no shadow |
| `bCustomScreenshotFormat` | n/a | yes | 1 | NVForever writes its own screenshots |
| `bDontAllowConsoleTillLoadingIsDone` | n/a | yes |  | the original's start-up order |
| `bHeatbeatSoundsFade` | to do | yes | 2 | NVForever plays no heartbeat yet |
| `bImprovedWeather` | to do | yes | 1 | the renderer does not know a fast travel happened |
| `bModConsolePrintsIncludeName` | to do | yes | 1 | console print does not know which plugin printed |
| `bMoreResponsiveControllerAiming` | to do | yes |  | the stick curve is x times |x| (player.rs); choose the new curve |
| `bMultithreadedHackingMenu` | n/a | yes |  | performance work on the original |
| `bNoDespawnVisibleStuckProjectiles` | to do | yes |  | projectiles do not stick in NVForever yet |
| `bNoInteriorBlackLoadingScreen` | to do | yes | 1 | unsure what to leave out: NVForever shows the picture, not black |
| `bNoPlayerNameLimit` | to do | yes |  | the name box is laid out for 24 characters; check its drawing first |
| `bPartialReloads` | unsure, skipped | yes |  | depends on animation file naming conventions not yet checked |
| `bRememberConsoleHistory` | to do | yes | 1 | needs a file in the NVForever home |
| `bUseAnimVariants` | unsure, skipped | yes | 3 | depends on animation file naming conventions not yet checked |
| `bVATSStopFollowingDeadTargets` | to do | yes |  | needs the kill camera shot's start frame kept |
| `bWeaponImpactEffects` | to do | yes |  | hits on the hand weapon use the flesh impact; holstered weapons have no hit part |
| `bAlternateLevelupSounds` | to do |  | 4 | xp level_up sound; the sound cache takes static names |
| `bAnimDebugging` | n/a |  |  | a mod-author debug print |
| `bAutoReadNotes` | unsure, skipped |  | 3 | needs another mod (Book Menu Restored) |
| `bClickToExitLoadScreens` | to do |  | 2 | loading hold_input and finish, plus a prompt |
| `bContinueGameHotkey` | n/a |  |  | a mod-author debug key |
| `bControllerTriggerDeadzones` | to do |  | 2 | Bevy GamepadSettings for the triggers |
| `bCustomLocationDiscoveredSound` | to do |  | 1 | quest_messages sound; the cache takes static names |
| `bDialogMenuDebugging` | n/a |  |  | a mod-author debug print |
| `bDisableAchievements` | n/a |  |  | NVForever has no achievements |
| `bDisableCharacterRespec` | to do |  |  | the respec is plugin script data |
| `bDisableControllerDeadzones` | to do |  | 2 | pad STICK_DEAD and Bevy axis settings |
| `bDisableRegionBordersKeepMessage` | to do |  |  | not built: no world borders |
| `bExtraConsoleDetails (enabled by default)` | to do |  | 4 |  |
| `bInvalidBSFadeNodePrints` | n/a |  |  | the plugin's own warning |
| `bNoAmmoCasings` | to do |  |  | NVForever gives no casings yet |
| `bNoPipboyOnAltTab` | n/a |  |  | the original's alt-tab handling |
| `bNoSkillBooksAbove100` | to do |  |  | the refusal belongs before equip_at takes the book (Pip-Boy, hotkeys, EquipItem) |
| `bNoTurningAnim` | to do |  |  | nvforever player_body turn_rate; drive_body at 16 params |
| `bPrintErrorsToConsole` | n/a |  | 5 | the original's error log |
| `bPrintNewModsOnLoad` | n/a |  |  | the plugin's own load report |
| `bRadiationCausesRadioStatic` | to do |  | 1 | radio play share with the radiation taken |
| `bRadioKeepPositionWhenLoading` | to do |  |  | the radio position is not saved |
| `bReloadSoundsAffectedByTimescale` | to do |  |  | one global sound speed |
| `bReplaceHelpWithConsole` | to do |  |  | no Help entry; a Console entry is new |
| `bSilentSneakPowerAttacks` | to do |  |  | unsure whether a grunt plays; anim_sounds player_keys |
| `bSplashDamageTorsoGibbing` | to do |  |  | combat::fly torso dismember check |
| `bSwapKeyboardYZKeys` | n/a |  |  | the original's keyboard handling; NVForever's keys can be rebound |
| `bTerminalFadeTime` | to do |  | 1 | not built: terminals do not fade |
| `bUKKeyboard` | n/a |  |  | the original's keyboard handling |
| `bUseConsoleOutputFile` | to do |  | 1 | console out_file from sync; a file name to choose |
| `bUseFallout3AudioDistortion` | unsure, skipped |  |  | needs the original sequel's filter described first |
| `bUseHitLocationInCinematicKillcams` | to do |  |  | not built: no cinematic kill camera outside V.A.T.S. |
| `bVanityCamReturnsToFirstPerson` | to do |  |  | not built: no vanity camera |
| `bWeaponVisibleDuringDialogue` | to do |  |  | probably already so (nothing hides the weapon); confirm in game |

### Fixes

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAmmoBurstCaseCountFix` | skipped: fix | yes |  |  |
| `bCacheQuestAndNoteMenu` | skipped: fix | yes |  |  |
| `bExplosionKnockbackDirectionFix` | skipped: fix | yes |  |  |
| `bFixKillChallengeSourceWeapon` | skipped: fix | yes |  |  |
| `bNoGibSoundWhenEnteringCells` | skipped: fix | yes |  |  |
| `bPreventInactiveWindowScrolling` | skipped: fix | yes |  |  |
| `bSortUnavailableRadiosToBottom` | skipped: fix | yes |  |  |
| `bSpinWeaponsSoundFix` | skipped: fix | yes |  |  |
| `bFixEnhancedCameraGroundSinkBug` | skipped: fix |  |  |  |
| `bUltrawideSupport` | skipped: fix |  | 1 |  |
