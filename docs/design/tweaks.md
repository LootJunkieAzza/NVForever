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
| No Weapon Movement Penalty | `bNoEquippedWeaponMovementPenalty` | `combat::run_pace` leaves out the two-handed slowdown |
| Unequip Broken Armour | `bUnequipBrokenArmor` | `combat::wear_armor` takes off a piece that breaks (`set_unequip_broken_armor`, set by `tweaks::sync`); no message |
| No Crosshair in First / Third Person | `bHideCrosshairInFirstPerson`, `bHideCrosshairInThirdPerson` | `reticle::draw` hides the arrows and bars by view; the prompt stays |
| No Skill Books at 100 | `bNoSkillBooksAbove100` | `Game::book_refused` checked in `menu::apply_input` (the Pip-Boy); the hotkeys and EquipItem still read the book |
| Red Health / Action Point Bar Below | `bColoredHUDBars` | `hud::pips` colours the ticks with the hostile red below each share |
| Red / Flashing Weapon Condition Below | `bColorWeaponCndLabel` | `hud::draw` colours the condition bar (red below one share, flashing twice a second below the other) |
| Blackjack Hand Total | `bBlackJackTotalDisplay` | `blackjack::screen::hand_line` under the status line (our words) |
| Less Frequent Pain Sounds (0–10 s) | `bLessFrequentPlayerPainSounds` | `Game::hit_lines` keeps the player's Hit line apart by `CombatLines::next_said` |
| Pause On Load | `bPauseOnSaveLoad` | `loading::finish`: a start from a save (`Runtime::restored_from_save`) asks the pause menu as ShowPauseMenu does |
| Swap Ammo While Holstered | `bSwapAmmoWithWeaponHolstered` | `combat::fire` takes the Ammo Swap before the weapon-not-ready return; the reload follows once drawn |
| Slots: Hold to Spin | `bSlotsAutoSpinHotkey` | `slots::keys` sends Spin while W is held and the machine is waiting for a bet |
| Click to Continue Dialogue | `bNoAutoContinueDialog` | `Game::update_dialogue` holds a response that ran out until `skip_response` (the click) |
| Shift to Talk While Sneaking | `bTalkWhileSneakingIfShiftIsHeld` | `activate::shift_talks` sets a switch read by `actor_action_in`, which then offers Talk |
| No Active Quest From New Objectives / After Completing One | `bDontSetQuestWhenObjectivesAdded`, `bDontSetQuestWhenCompletingObjectives` | `pipboy::data` stops picking the first quest under way by itself; either switch does it, as NVForever has no separate objective-added moment there |
| Hide Read Notes | `bHideReadNotes` | `pipboy::data` leaves out notes `Game::note_read` says are read, but for the one on screen |
| Console Key | `iConsoleKey` | `tweaks::CONSOLE_KEYS` (the original's `` ` `` first, then keys nothing else takes) read by `console::toggle` |
| Tab Closes Terminals | `bTabClosesTerminals` | `terminal::keys` sends Exit on Tab |
| Tab Goes Back in the Pause Menu | `bTabBackInStartMenu` | `pause::choose`: Tab is Back on the main page's question and the load list; Settings and Controls keep Tab for their tabs |
| Fast Travel From Anywhere | `bAllowUnsafeFastTravel` | `pipboy::data::travel::tweak_checks` drops the place refusal (Can't Travel From Here, Can't Fast Travel) |
| V.A.T.S. Burst Stops on a Kill | `bVATSStopBurstIfTargetDead` | `vats_mode`: a burst's later rounds are held once the mark's target is dead |
| Repair Needs the Weapon's Skill | `bRepairRequiresWeaponSkill` | `pipboy::repair::keys` refuses to open the list for a weapon whose `skill_req` is above the player's Repair |
| Date Format | `iDateFormat` | `wait::model::DATE_STYLES` (as the game, day first, month first, year first; ours), set from `tweaks::sync` and read by the wait menu and the Pip-Boy heading |
| Hide Unknown Names | `bHideUnknownNPCNames` | `activate::find_target`: Talk targets without `talked_to_pc` get no name; bodies (Search) keep theirs |
| V.A.T.S. Puts the Weapon Back | `bEnteringVATSDoesntUnholsterWeapon` | `vats_mode`: `drew_weapon` when the mode draws it; `end` with nothing fired asks `put_away`, done in `read_keys` |
| Drop Weapon key | `iDropEquippedWeaponKey` | a new unbound Control; `tweaks::drop_weapon` drops the equipped weapon as the Pip-Boy's Drop does (`World::drop_equipped`, `RefChanged`) |
| Radio Volume Up / Down keys | `iRadioVolumeUpKey`, `iRadioVolumeDownKey` | two new unbound Controls; `tweaks::radio_volume_keys` steps `SoundVolumes`' Radio level a tenth |
| No Hotkey Wheel | `bNoHUDHotkeyPopup` | `weapon_wheel::Frame::no_hold_open`: a held hotkey never opens the wheel; let go, it is a tap |
| Ammo Label Under the Count | `bMoveAmmoTypeLabel` | `hud::texts` puts the label right-aligned `AMMO_LABEL_DROP` below the count, inside the panel |
| Companion Compass Colour | `bCompanionPipColorChange` | `compass::Mark::Companion` for teammates, drawn in `tweaks::COMPANION_COLOURS` (a choice of ours in place of the tweak's RGB value) |
| Facial Hair for Women | `bRaceMenuAllowFemaleFacialHair` | `creator::model::Model::female_beards` (set by `creator::sync`) keeps the facial hair row and its shading for a woman; a change of sex to female still clears the beard |
| Show Use Password | `bShowUsePasswordOnTerminalsWithNote` | `fnv_game::terminal::opens_by_password` gives `Action::UsePassword` ("Locked (Use Password)", our words) until the terminal has been opened (`Terminals::opened`, this session only) |
| Lockpicking With the Movement Keys | `bLockPickMenuKeyboardMovement` | `lockpick::keys` moves the pin with the strafe keys (Shift twice as fast, Ctrl half: ours); `torque_held` takes Forward, Back and Jump |
| Store All | `bContainerMenuStoreAllHotkey` | `menu::model::Command::StoreAll`, sent for Shift with Take All while `MenuModel::store_all`; the player's listed rows go in, worn and quest items stay |
| Map Recentre Key | `bMapRecenterHotkey` | `pipboy::data::take_input`: C (our key) clears the dragged map's centre, which falls back on the player; at once, no easing |
| Map Marker Distance | `bMapLocationDisplayDistance` | `pipboy::data::world_map` adds `model::distance_text` (metres or kilometres, our words, 70 units a metre [hypothesis]) to the marker's detail line; the travel-time modes are not done |
| Quantity Box Shows the Price / Weight | `bBarterQuantityMenuShowsPrice`, `bContainerQuantityMenuShowsWeight` | `menu::look::quantity_line` under the amount of the hand-built "How many?" box (our words); the box drawn from the game's tiles does not show it yet |
| Hide an Empty Reputation List | `bHideReputationTabIfEmpty` | `pipboy::stats::model::Snapshot::hide_reputation`; the hand-built General tab keeps to the statistics and draws no switch; the tiles Stats page does not hide it yet |
| Quick Load From the Menus | `bStartMenuQuickLoad` | `save::requests` takes the QuickLoad key in `PlayState::Menu` too (QuickSave stays play-only) |
| Preview Perks on Level Up | `bPreviewPerksOnLevelUp` | `levelup::model::LevelUp::preview_perks`: with no perk due, Continue still leads to the perks page, its `PerkSheet::count` 0 |
| Last Weapon key | `iEquipLastWeaponKey` | a new unbound Control; `tweaks::last_weapon` follows the equipped weapon each frame and equips the one before, put away |
| Pick Locks Even With the Key | `bPickLocksEvenWithKey` | `fnv_sim::World::set_player_ignores_keys`, set by `activate::shift_talks` while Shift is held: `World::activate` leaves the key unused, so the lock is picked; locks only, not terminals; the prompt still says Unlock |
| Smooth Camera key | `iToggleSmoothCameraKey` | a new unbound Control flips `TweakSettings::smooth_look`; `player::mouse_look` eases the look at `SMOOTH_LOOK_RATE` (ours), never while aiming |
| Right Click for the Previous Category | `bRightClickChangesToPreviousContainerCategory` | `menu::menu_mouse` sends `Command::Category(side, -1)` for a right click on a list heading; container and barter, not the recipe menu |
| Mark Notes Unread | `bMarkNotesUnread` | `pipboy::data::take_input`: a right click on the notes list calls `Game::mark_note_unread` for the selected note |
| Barter Total Past the Merchant's Caps | `bShowBarterTotalWhenOverSellLimit` | `MenuModel::full_barter_total`: the hand-built barter figure shows `Trade::balance` instead of the capped `Trade::paid`; the tiles barter menu not yet |
| Compass Fades on the Left | `bCompassFadeLeftSide` | `compass::left_fade` shades the marks left of the middle down to a fifth (ours); with it on the marks are drawn again each frame; quest marks stay whole |
| Controller Dead Zones | `bDisableControllerDeadzones` | `pad::set_dead_zones` from `tweaks::sync`; `pad::stick` reads each stick's own; Bevy's own small dead zone still comes first |
| Trigger Dead Zones | `bControllerTriggerDeadzones` | `tweaks::trigger_dead_zones` sets LT's and RT's press points in each controller's `GamepadSettings` (released at 85% of it, ours); for every use of the triggers, not only attack |
| More Responsive Controller Aiming | `bMoreResponsiveControllerAiming` | `player::mouse_look`: the right stick's curve is straight from the dead zone's edge (ours) in place of the square |
| Faster Controller Camera Circling | `bFasterControllerPOVRotate` | `player::mouse_look` doubles a controller's turn while Toggle POV circles the camera (the factor is ours) |
| Horizontal Look Speed (x0–x2) | `bSeparateHorizontalSensitivity` | `player::mouse_look` scales the look's left and right by `TweakSettings::horizontal_look` (a Tweaks tab slider in place of a second sensitivity slider) |
| Shift Ignores Friends in V.A.T.S. | `bShiftIgnoresFriendlyVATS` | `vats_mode::barred` bars anyone who would not fight the player while Shift is held (`SHIFT_HELD`, set in `read_keys`); `bHideFriendliesByDefault` is not done |
| Remember Console History | `bRememberConsoleHistory` | `console::Console::remember` reads `ConsoleHistory.txt` from NVForever's folder once; each new line writes the last 100 back |
| Console Output File | `bUseConsoleOutputFile` | `Console::auto_output` sets the output file to `ConsoleOutput.txt` in NVForever's folder once a session (never the game's folder; the name is ours) |
| Console Text Shadow | `bConsoleTextShadow` | `console::spawn` draws the scrollback a darker green with Bevy's `TextShadow` (our shade and offset) |
| Press to Leave Loading Screens | `bClickToExitLoadScreens` | `loading::finish`: with `Loading::click_to_exit` the finished screen shows `PRESS_LINE` (ours) and comes down on the next key or button; every screen, `bLoadGameOnly` not done |
| Radiation Brings Radio Static | `bRadiationCausesRadioStatic` | `radio`: the Pip-Boy station's static share is at least the player's rads a second over `RADS_FOR_FULL_STATIC` (10), at most `MAX_RAD_STATIC` (0.8); both ours |
| Barter Shows Caps After | `bBarterShowCapsChange` | `menu::look::half_figure` writes "Caps  N (to M)" (ours) with `MenuModel::barter_caps_change`; the hand-built barter menu only, `iMode` not offered |
| No Self Meltdown Damage | `bNoSelfMeltdownDamage` | `combat::meltdown` notes its explosions in `Combat::meltdowns`; the blast loop spares the player and teammates from them (the tweak's three options as one) |
| Level Cap (0–100) | `iMaxCharacterLevel` | `experience::max_level` returns the tweak's cap when set, without the add-ons' +5 |
| Sneak Attacks Standing | `bSneakAttackWithoutCrouching` | `combat::sneak_attack` takes the player as sneaking whenever the target has not detected them |
| No Companion Item Damage | `bNoCompanionItemDamage` | `combat::wear_weapon` returns before any wear for a teammate (armour wears only on the player in NVForever) |
| Weapon Mods Need the Skill | `bWeaponModdingSkillRequirement` | `pipboy::item_mod::keys` refuses to open the mod list when the player's skill (`WeaponStats::skill`) is under `skill_req` |
| Planting Grenades Needs the Skill | `bPlantingLiveGrenadesRequiresWeaponSkill` | `pickpocket` (engine): under the explosive's skill requirement it is placed but does not go live |
| Fast Travel Costs a Nuka-Cola | `bFastTravelCostsSpecialNukaBottles` | `pipboy::data::travel`: `Refusal::NoBottle` (our words) without one of `TRAVEL_BOTTLES` (`NukaColaQuantum`, `NukaColaQuartz`, `NukaColaVictory` [observed: FalloutNV.esm]); `go` takes one |
| Jump Swims Upwards | `bJumpSwimsUpwards` | `player`: Jump held (`JUMP_HELD`) raises the swim velocity's upward part to the walking stroke's speed; `fSpeedScale` and `bSwimUpWhileAutowalking` not offered |
| Armour Causes Sinking | `bArmorCausesSinking` | `combat::stat_speed` puts a sinking speed by `worn_armor_class` in `StatSpeed`'s third number (heavy 60, medium 25 units/s, ours); the swim step takes it off the upward part |
| Remember Bobby Pin Wear | `bRememberBobbyPinHealth` | `fnv_game::game::RefExtra::pin_health` on the player (saved), written each lockpicking step and read by the next `Picking` |
| Forcing Breaks the Pin, Not the Lock | `bForceLockpickNoBreakLock` | `lockpick::Settings::force_spares_lock`: `Picking::force` failing goes to `PinBroken` (or out of pins) instead of `LockBroken`; the engine takes the pin and plays its break |
| No Condition Damage Penalty / Buffer, Automatic Criticals, Critical Damage | `bNoWeaponConditionDamagePenalty`, `bRemoveWeaponDamageBuffer`, `bCritChanceIgnoresFireRate`, `bScaleCriticalDamage` | `combat::sync_condition_penalty` sets `fnv_combat::tuning::Rules` each frame: the penalty 0, the threshold 1, `crit_ignores_fire_rate`, `crit_damage_scale` (the new fields are left out of the serialized dumps) |
| NPC Armour DR / DT Ignores Condition | `bNoScaleNpcDamageResistanceByCondition`, `bNoScaleNpcDamageThresholdByCondition` | `combat::target_armor` takes an NPC's DR or DT from its armour at no condition penalty |
| Explosion Radius Buff | `bExplosionRadiusBuff` | `CombatTuning::explosion_inner` / `explosion_inner_edge` (left out of the dumps) in `damage::explosion_damage`: the inner 30% whole, 20% at the edge; `bAffectPlayer` not offered |
| NPCs Sneak Attack the Player | `bNPCsCanSneakCritPlayer` | `combat::sneak_attack` lets an NPC's hit on the player count when the player's measurement of that NPC is below Detected (an unmeasured pair never counts); NPC blows and shots |
| Fatal Criticals Always Gib | `bFatalNonSneakCritsAlwaysGib` | `combat::crit_gib` gives a killing non-sneak critical's dismember check a set fate (`fnv_combat::dismember::scripted`: explode where it can, else sever); player melee and shots |
| Deadly Falls Take the Legs | `bFallDeathsCauseDismemberment` | `falls::hurt` queues `dismember::scripted_check` for Left Leg 1 and Right Leg 1 when a fall kills an NPC (exploded where the part can be, else severed) |
| Splash Kills Can Gib the Torso | `bSplashDamageTorsoGibbing` | the blast loop: a blast kill by an actor with Bloody Mess explodes the torso at `SPLASH_TORSO_CHANCE` (one in four, ours) |
| NPCs Drop Holstered Weapons | `bNPCsDropWeaponHolsteredWeapon` | `combat::death_drops` follows an NPC's equipped weapon too when none is in its hand |
| NPCs Keep Spent Cases | `bNPCsEarnAmmoCasings` | the NPC shot path rolls the ammo's Consumed Percentage and adds the Consumed Ammo item to the shooter (no perks) |
| First Person V.A.T.S. | `bFirstPersonVATS` | the view switches to first person while the mode fires and goes back to the earlier view when it ends |
| No Spent Cases | `bNoAmmoCasings` | the player's shots give no Consumed Ammo item (cases, hulls, drained cells) |
| Crosshair Down the Sights | `bKeepCrosshairWhenAiming` | the crosshair stays on down the iron sights; a scope still hides it |
| Move During V.A.T.S. Camera | `bMoveDuringVATSPlayback` | the kill camera leaves walking and running (keys and stick) to the player; jumping, firing, aiming and reloading stay held |
| Whole-Stack Pickup Figures | `bBetterPickupPrompt` | `reticle::Prompt::of` multiplies the weight and value by the stack's count (`STACK_FIGURES`); the carried total is not shown |
| Turn Further While Seated | `bTurnFurtherWhileSeated` | nvforever `player_body::seated_heading` uses 115 degrees for 80 (`tweaks::SEATED_WIDE`); third person on furniture had no limit |
| No Turning Animation | `bNoTurningAnim` | nvforever `player_body` plays no Turn clip (`tweaks::NO_TURN_CLIP`) |
| No Landing Animation | `bRemoveLandingAnim` | nvforever `player_body` plays no landing clip for the player (`tweaks::NO_LAND_CLIP`); NPCs' landings (`actor_jumps`) are unchanged |
| Compass: Only Mapped Places | `bHideInvisibleUndiscoveredLocations` | `compass::gather` leaves out unfound markers that `DataState::unmapped_of` finds neither Visible-flagged nor shown by a script, the nearest-unfound tweak's pick too |
| S.P.E.C.I.A.L. Machine: Straight to the Summary | `bSkipVigorTesterSpecialPages` | `chargen::vigor::open_close` starts on `Page::Review` in the end pose of the turn from Charisma's page; the earlier pages are still there with ← |
| Scope Delay (ms) | `bScopeVisibilityDelay` | `aim_zoom::zoom` counts the time at full zoom (`AimZoom::full_for`) and puts the scope up only after the slider's milliseconds; a reload already took the sights down |
| No Pointer Over Message Boxes | `bHideCursorInMessageMenu` | `player::grab_cursor` hides the pointer (without capturing it) while `MessagePanel` has a box |
| No Pointer in Conversations | `bHideCursorInDialog` | the same while `DialogueMenu` shows choices |
| Level Up: Wheel Sets Skills | `bLevelUpScrollWheelSupport` | `levelup::input` turns the wheel into Raise / Lower of the chosen row on the skills page; the perks page still scrolls |
| Compare Armor | `bCompareArmorStats` | `pipboy::items::info_of` adds the difference from the DT of the worn pieces sharing a biped slot, for armour not worn |
| (already so) | `bSilentSneakPowerAttacks` | the player makes no sound of effort on any attack, so nothing to silence |
| Detailed Stats | `bMoreDetailedStatsMenu` | `pipboy::stats::read` leaves the values uncapped and writes the mark as the signed difference from the permanent value (`Row::mark` is a `String` now) |
| Lockpicking Follows the Time Speed | `bTimescaleAffectsMinigames` | `lockpick::step` multiplies its frame time by SetGlobalTimeMultiplier's speed; the card and slot games keep their pace |
| No Name Length Limit | `bNoPlayerNameLimit` | the name menu and `chargen::apply_name` take up to `NAME_LONG_CHARS` (256) for 24; the box wraps |
| Activate Doesn't Stand Up | `bActivatingDoesntStandUp` | nvforever `furniture` and `activate` leave `PlayerSeat::leave` alone on Activate (`tweaks::SEAT_KEEPS_ON_ACTIVATE`); a movement control still gets up (`player.rs`) |
| Plain Screen Going Outside | `bNoExteriorLoadScreens` | `loading::begin` marks a trip from an interior to a worldspace (`plain_once`); `spawn_screen` then chooses no screen, so black alone; a save's cover is not a trip |
| (already so) | `bLuckDoesntAffectGambling` | NVForever's blackjack, roulette, slots and Caravan do not read Luck |
| Mark Repeating Challenges | `bRecurringChallengeIndicator` | `challenges::rows` sets `Row::recurring` from the CHAL Recurring flag (less SetChallengeRecurring 0); the Pip-Boy adds "(repeats)", our word |
| Hide Hotkeyed Items in Containers | `bHideHotkeyedItemsInContainers` | `MenuModel::refresh` drops the player's rows whose item is on a hotkey (`MenuModel::hotkeyed`, from `Hotkeys` in `tweaks::sync`) |
| Hide Hotkeyed Items in Barter | `bHideHotkeyedItemsInBarter` | the same in `refresh_trade` for the player's side |
| Hide Hotkeyed Items in Repair | `bHideHotkeyedItemsInRepairLists` | `pipboy::repair::current` drops parts on a hotkey (`MenuModel::hide_hotkeyed_repair`); no offer when none are left |
| (already so) | `bDisplayItemEffectTotals` | `Game::aid_preview` counts an effect over time whole, so the EFFECTS figure is the total; the per-second breakdown is not shown |
| Map Zooms at the Pointer | `bBetterMapZoom` | `pipboy::data::take_input` adds a drag of the pointer's offset from `MAP_MID` times (1 − growth) after a wheel zoom, so the place under it stays (both maps; the world map's edge clamp still applies) |
| Console Homes | `bAddSetHomeCommand` | `console::typing` takes `sethome`, `home`, `back` and `delhome` (an optional name, "home" without one) while `Console::homes_on`; the homes are travels kept for the session, not saved |
| Screenshot Key | `iScreenshotKey` | nvforever `keys` takes the picture on `tweaks::screenshot_key`: F12, Print Screen, F11, Scroll Lock or Pause (`SCREENSHOT_KEYS`, keys nothing else takes) |
| Shift Screenshot Hides the HUD | `bShiftScreenshotHidesMenus` | with Shift held, nvforever `keys` sets `MenusHidden` (what `tm` sets), takes the picture three frames later and puts it back after six; an open menu stays in the picture |
| Console Lines Name Their Plugin | `bModConsolePrintsIncludeName` | the script runner's `print` puts `[plugin]` (the unit key's first part) before the line; the base game's scripts too |
| Weapon Requirements Matter | `bWeaponRequirementsMatter` | `Game::weapon_requirement_unmet` compares the weapon's DNAM Strength and skill requirements with the player's values now; the Pip-Boy (`menu.rs`) and the hotkeys (`weapon_wheel::read`, not for the weapon already equipped) refuse with "Needs <stat> <n>." |
| Console Shows Models | `bExtraConsoleDetails` | `console::describe` adds the base's MODL path (`MODEL_DETAILS`); the cell's Editor ID and name were already in the details |
| Backward Pace, Sideways Pace | `bMovementPenalties` | the player's step multiplies the pace by the backward and sideways shares, a diagonal weighted by its forward and side parts; aiming already slows to a walk (`aiming_walks`) |
| Hacking: Symbols Do Nothing, No Repeat Guesses, Dud When Attempts Full, Duds Spare Tried Words | `bImprovedHacking` | `hacking::Puzzle::rules` (`HackRules`, set by `terminal.rs` from the game's tweaks) in `Puzzle::choose`; `Puzzle::guessed` keeps the words denied; the options about printing are not done |
| Run While Overencumbered | `bOverencumberedTweak` | `player::encumbered_walks` no longer forces a walk and sets `OVERENCUMBERED`; `tweaks::ap_costs` adds the overencumbered running and jump costs (4 a second and 10 a jump by default, our numbers), walking and refusing the jump without the points |
| Enemy Blows Hit Where They Land | `bLocationalMeleeDamage` | an NPC's blow on the player takes `player_body_part`'s `hit_part` in place of `HitPart::default()`; the player's blows already used the struck part's; NPC on NPC has no part lookup yet |
| Disarming Needs Skill | `bDisarmRequiresSkill` | `Game::disarm_chances` (the readme's formulas: disarm = mult × (skill + Perception − 5) × 100 / required; set off = mult × (required − skill + 5 − Luck) × 2, each held to 0–100); `activate` rolls before the disarm and on a failure either queues the mine in `Mines::set_off` (no fuse) or says it is still armed |
| Skills Capped by S.P.E.C.I.A.L. | `bCapSkillsBySPECIAL` | `SkillSheet::of_player` fills `SkillSheet::caps` from the readme's formula with the governing attribute (`actor_values::governing_attribute`); `value`, `can_raise` and `complete` read `cap(i)`. The readme gives no default numbers: 50 + 5 a point of the attribute, no Luck, at most 100 are ours |
| Hold Breath Through a Scope | `bScopeHoldBreath` | `tweaks::hold_breath` sets `BREATH_HELD` while Shift is held with the scope up and pays `breath_ap` a second (let go with no AP left); nvforever `scope_view::sway` multiplies the sway's radius by `breath_sway` (20 % and 5 AP a second by default, ours) |
| Wheel Zooms Scopes | `bAdjustableScopeZoom` | `aim_zoom::zoom` keeps `AimZoom::scope_zoom`, a share of the Sight FOV the wheel steps by 1.1 a notch between 0.5 and 2 while the scope is up, and forgets it when the scope goes down; the first-person wheel had no other use |
| Mouse Wheel Changes Weapon | `bMousewheelScrollsWeaponHotkeys` | `weapon_wheel::read` steps from the slot of the weapon in hand to the next (up) or previous slot whose weapon `takes_hand`, and uses it as a tap does; first person only, not with a scope up (Wheel Zooms Scopes) or the wheel open |
| Fade Pips by Distance | `bCompassNPCDistanceBasedAlpha` | `compass::place` keeps `CompassAlpha` beside `CompassView` (from `marks_far`'s distances): an actor's pip at 1 − distance / (fSneakMaxDistance, times fSneakExteriorDistanceMult outdoors), no fainter than 25 %; `draw` applies it |
| Fade Places by Distance | `bCompassLocationDistanceBasedAlpha` | the same for places found and not, over the reach `gather` shows them at (Perception × the per-point reach, `PLACE_RANGE`) |
| Health Bars Count Buffs | `bEnemyHealthbarShowBuffedHP` | `hud_target` adds the target's positive temporary Health modifier (`actor_value_temporary`) to `max_health` for the bar |
| Recently Dead on the Compass | `bRecentlyDeadNPCIndicator` | `compass::gather` notes when it first finds an actor dead (`DeadSeen`) and keeps a plain pip for it for the slider's seconds, until the crosshair's target has been that body |
| Armour Rustle From the Body | `bArmorSoundsPlayIn3D` | nvforever `anim_sounds::sound_heard` gives the third-person rig's foley the player's feet as its place (`tweaks::ARMOR_3D`); the first-person rig's stays at the listener |
| Mine Blasts Are Heard | `bNPCsDetectMineExplosions` | `mines::tick` keeps who set a mine off (`Burning::by`) and, when it goes off, queues an explosion detection event (level 100, ours) owned by whoever laid it, else whoever set it off; nvforever `detection` measures it as any other event |
| Explosions Leave Items Be | `bExplosionsDontPushTakeableItems` | the explosion's push uses `fnv_phys::World::push_dynamic_but`, sparing bodies whose reference's base is an item |
| Held Items Weigh | `bEncumbranceIncludesGrabbedItems` | `player::encumbered_walks` adds the grabbed reference's item weight times its count (`GripOn::Ref`) to the carried weight before judging over the limit |
| Usable Ammo First | `bSortEquipableAmmo` | `tweaks::sync` puts the ammo choices of the player's carried weapons in `MenuModel::usable_ammo`; `MenuModel::refresh` sorts those rows first in the inventory, a stable sort that keeps the names' order |
| Better Free Camera | `bBetterFlycam` | `free_cam::fly` takes Space / left Ctrl for up and down in place of E / Q, toggles `FreeCam::auto` (forward by itself) on Q, ends it on S, and turns on Z / C at 1.5 rad/s (ours); the wheel already set the speed |
| New Places Red on the Map | `bColorRecentlyAddedMapMarkers` | `DataState::found_this_run` keeps the markers found since the program started (a loaded save starts it again); the world map draws those red; markers a script shows are not counted |
| Repeat Activate | `iRepeatActivateKey` | a new unbound control, `Control::RepeatActivate`; held while playing, `tweaks::repeat_activate` queues the crosshair's target in `ActivateNext` every 0.25 s (ours), only things to take unless the switch is off |
| Second Iron Sights | `iToggleSightingNodeHotkey` | a new unbound control, `Control::ToggleSightingNode`, flips `tweaks::SECOND_SIGHT`; nvforever `player_body` finds `##SightingNode2` beside the first sighting node, aims down it while set, and clears the flag for each new weapon model |
| Fine Weapon Models for Everyone | `bForceHiResWeaponModels` | nvforever `actors::arm_weapon` asks `model_for(true, …)` (the WEAP's first-person model) while `tweaks::HI_RES_NPC_WEAPONS` is set; not checked in game |
| (already so) | `bAllowActivateWhileAnimPlays` | `activate` refuses nothing for a clip playing (reload, shot, jump, landing) |
| Reload Swaps Spent Ammo | `bReloadingWithNoAmmoSwapsAmmoTypes` | a reload asked for with none of `Combat::ammo` carried takes the Ammo Swap path (the next carried type); loading a save with none is not handled |
| Objectives on Arrival | `bShowQuestObjectivesOnCellChange` | `tweaks::objectives_on_arrival` sees `Location` change (another interior, or a worldspace) and pushes the active quest's shown, open objectives as a `QuestNotice::Objectives`, at most once in the gap (30 s by default, ours) |
| No Place Marker Question | `bNoPlaceMarkerPopup` | `pipboy::data::place_marker` answers at once with `marker::quick`: toggled, or (`bPlaceMarkerShiftToReset`) always placed and taken away with Shift, or (`bResetIfHovered`) taken away when the highlight box is within the snap distance of the marker drawn (`DataState::custom_on_map`) |
| Clear Marker When Near | `bClearNearbyPlayerMarker` | `tweaks::clear_near_marker` clears the game's marker (`jg8.custom_marker`) when `marker::near` (same space, across the ground) after the player was once farther, so a marker placed underfoot stays |
| Place Marker Here | `iPlaceMapMarkerAtPlayerPosKey` | a new unbound control, `Control::PlaceMarkerHere`; `tweaks::place_marker_here` sets the game's marker to the player's space and feet, with a corner line in our words unless switched off |
| (already so) | `bNoKillcamKillSound`, `bDisableHitShader`, `bNoScreenshotPopup`, `bNoInteriorBlackLoadingScreen` | NVForever plays no kill-camera kill sound, draws no hit shader, shows no screenshot message and shows an interior's picture rather than black; nothing to switch |

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
| done | 504 |
| to do | 99 |
| unsure, skipped | 17 |
| n/a | 15 |
| skipped: fix | 10 |

### Gameplay

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bActorsStickToSlopes` | done |  | 2 | already so: characters settle onto walkable ground going down (fnv-phys character) |
| `bAdjustableScopeZoom` | done |  | 14 | Wheel Zooms Scopes (half to double; the rate, key and reset options not built) |
| `bAgilityScalesJumpHeight` | done |  | 1 | Agility Scales Jump Height (1 + mult x (Agility - 1)) |
| `bAgilityScalesMovementSpeed` | done |  | 3 | Agility Moves Faster (slider, every actor; the player-only and NPC-only options not built) |
| `bAimingSpeed` | done |  | 2 | Full Pace While Aiming (aiming and blocking together) |
| `bAimingUnholstersWeapons` | done |  |  | Aiming Draws the Weapon |
| `bAllowAidAtMaxHealth` | done |  | 3 | Aid at Full Health (the STATS page hints; per-item options not built) |
| `bAllowSleepWaitWithReducedMaxHealth` | done |  |  | already so: no wait refusal looks at health |
| `bAmmoSwapWindowMatchesReloadTime` | done |  |  | already so: an ammo swap is taken at any moment, mid-reload too |
| `bArmorCausesSinking` | done |  | 4 | Armour Causes Sinking (heavy 60, medium 25 units/s: ours; light and none do not sink) |
| `bAutoUnlockTerminals` | done |  | 2 | Skip Easy Hacks (switch + margin slider; the XP option not built) |
| `bAutoWeaponJamWhileFiring` | done |  |  | already so: every shot rolls the jam, mid-burst too |
| `bBarterAffectsRepairCosts` | done |  | 3 | Barter Lowers Repair Costs (our rule: a percent off at Barter 100, in proportion below; no perk modifiers) |
| `bBarterCheckActorBuySellFlags` | done |  | 2 | Merchants Keep to Their Lists (the ammo and perk options not built) |
| `bBillyGoatMode` | done |  | 3 | Steepest Walkable Slope (30-80 degrees; one angle for walking, jumping and auto-walk) |
| `bCapSkillsBySPECIAL` | done |  | 5 | Skills Capped by S.P.E.C.I.A.L. (defaults ours: 50 + 5 per attribute point, cap 100) |
| `bClearNearbyPlayerMarker` | done |  | 1 | Clear Marker When Near (0 to 2000 units; goes once the player has been farther) |
| `bCompanionsDontUseAmmo` | done |  |  | already so: NPCs carry no ammo and reload without spending any |
| `bContactMines` | done |  | 2 | Contact Mines (Player) and (Others): a mine stood on, within 24 units of the body's axis, goes off with no fuse |
| `bCrippledLegsScaleJumpHeight` | done |  | 2 | Jumping on a Crippled Leg (one part per crippled leg) |
| `bCripplingDoesntDisarm` | done |  |  | Crippling Never Disarms |
| `bCritChanceIgnoresFireRate` | done |  |  | Automatic Criticals Not Cut by Fire Rate |
| `bCustomArmorConditionPenalty` | done |  | 1 | Armour Condition Penalty (x0 to x2 on the penalty below half condition; the repair menus still show DT at x1) |
| `bCustomSpecialPoints` | done |  | 1 | S.P.E.C.I.A.L. Points (30 to 70; a script's own total still wins) |
| `bDisableFastTravel` | done |  |  | No Fast Travel |
| `bDisableReloadingNonEmptyClip` | done |  | 2 | Reload Only When Empty (the energy-only and looping options not built) |
| `bDisarmRequiresSkill` | done |  | 2 | Disarming Needs Skill, with the two multipliers |
| `bDrowningDrainsAP` | done |  |  | Drowning Takes AP First |
| `bEncumbranceIncludesGrabbedItems` | done |  |  | Held Items Weigh |
| `bEnduranceScalesMinFallHeight` | done |  | 2 | Endurance Softens Falls (slider; the base bonus option not built) |
| `bExplosionKnockdownAvoidanceUsesStrength` | done |  |  | Strength Resists Blasts |
| `bExplosionRadiusBuff` | done |  | 3 | Explosion Radius Buff (30% inner, 20% edge as the description gives; bAffectPlayer not offered) |
| `bExplosionsDontPushTakeableItems` | done |  |  | Explosions Leave Items Be |
| `bExtendLockpickSpotRange` | done |  |  | already so: the sweet spot can sit anywhere on the half circle |
| `bFallDeathsCauseDismemberment` | done |  |  | Deadly Falls Take the Legs (both legs, the part's own fate) |
| `bFastTravelCostsSpecialNukaBottles` | done |  | 3 | Fast Travel Costs a Nuka-Cola (Quantum, Quartz or Victory by editor ID; our refusal words; the item messages not shown) |
| `bFastTravelOverencumbered` | done |  |  | Fast Travel Encumbered |
| `bForceLockpickNoBreakLock` | done |  | 3 | Forcing Breaks the Pin, Not the Lock (the sub-options not offered; our status words) |
| `bFreeWildWasteland` | done |  |  | Free Wild Wasteland (editor ID WildWasteland, a trait on offer) |
| `bFrenzyGrantsPlayerXP` | done |  |  | Frenzied Kills Earn Experience (Aggression 3) |
| `bGodModePreventsJamAnims` | done |  |  | No Jams in God Mode |
| `bGrabbingItemsIsCrime` | done |  | 1 | Grabbing Is Stealing (any value; the minimum-value option not built) |
| `bHittingWeaponsDoesntDamageThem` | done |  |  | Shots Spare Weapons |
| `bHoldToHolsterMeleeWeapons` | done |  |  | already so: a tap reloads, holstering needs the hold, and melee drops the reload |
| `bImprovedHacking` | done |  | 10 | four Hacking switches (symbols do nothing, no repeat guesses, dud when attempts full, duds spare tried words); the print options not built |
| `bJumpingCostsAP` | done |  | 3 | Action Points per Jump (slider 0-50; the combat-only and jump-anyway options not built) |
| `bJumpSwimsUpwards` | done |  | 2 | Jump Swims Upwards (at the walking stroke speed; the sub-options not offered) |
| `bJumpWhileOverencumbered` | done |  |  | already so: NVForever never refuses the jump when over-encumbered |
| `bLevelDifferenceAffectsCombatXP` | done |  | 2 | Kill XP by Level (slider; scales the game's kill XP per level of difference instead of the plugin's base) |
| `bLightStepAffectsCompanions` | done |  |  | Light Step for Companions |
| `bLockNeedsKeyShowName` | done |  |  | Name the Key |
| `bLockpickForceBonusNearSweetSpot` | done |  | 2 | Force Lock Near the Sweet Spot (our falloff: full bonus on the middle, none 90 degrees away) |
| `bLockpickRememberBobbyPinAngle` | done |  |  | already so: a new pin keeps the angle |
| `bLootUnconsciousVictims` | done |  |  | Search the Unconscious |
| `bLuckDoesntAffectGambling` | done |  | 1 | already so: NVForever's casino games do not read Luck |
| `bMarkNearbyLocationsOnMap` | done |  | 1 | Mark Nearby Places (slider, game units) |
| `bModifySkillPointsEarned` | done |  | 2 | Own Skill Points Rule (switch + base and per-Intelligence sliders) |
| `bMousewheelScrollsWeaponHotkeys` | done |  | 1 | Mouse Wheel Changes Weapon (no invert option) |
| `bMovementPenalties` | done |  | 11 | Backward Pace and Sideways Pace (aiming already walks; left and right share one slider) |
| `bNoAmmoFromTakingNPCWeapon` | done |  |  | already so: taking a weapon adds only its placed ammo |
| `bNoAPRegenWhileOverencumbered` | done |  | 2 | Action Points Back When Over-Encumbered (slider; the while-moving option not built) |
| `bNoCompanionItemDamage` | done |  |  | No Companion Item Damage (weapons; NPC armour does not wear in NVForever) |
| `bNoDamageMeleeWeaponIfTargetDead` | done |  |  | Spare the Blade on the Dead |
| `bNoDialogueZoom` | done |  |  | already a row: Gameplay tab, dialogue-zoom |
| `bNoEnvironmentRadiationInFullPowerArmor` | done |  |  | Power Armor Blocks Radiation (body armour; the helmet is not asked for) |
| `bNoFastTravelIfLegsCrippled` | done |  | 1 | No Travel on a Crippled Leg (NVForever's own refusal text) |
| `bNoFreeBarterItems` | done |  | 1 | Price for Worthless Things (slider, 0-10 caps) |
| `bNoGrabOwnedItems` | done |  |  | No Grabbing Owned Items |
| `bNoLockFailedTerminals` | done |  |  | No Terminal Lockout |
| `bNoPlayerStaggerAnims` | done |  |  | No Player Stagger |
| `bNoSkillTags` | done |  | 6 | No Skill Tags (the outline, percent and XP options not built) |
| `bNPCsDetectMineExplosions` | done |  |  | Mine Blasts Are Heard |
| `bNPCsEarnAmmoCasings` | done |  |  | NPCs Keep Spent Cases (the ammo's own chance, no perks) |
| `bNPCsTakeLimbFallDamage` | done |  |  | already so: falls hit a leg for NPCs too |
| `bOnlyAllowWaitWhileSitting` | done |  | 3 | Wait Only When Seated (NVForever's own refusal text) |
| `bOverencumberedTweak` | done |  | 8 | Run While Overencumbered, with AP a second and a jump (the weight-scaled drain and speed options not built) |
| `bPerksAffectFistWeaponSpeed` | done |  |  | already so: the fists are the weapon the perks are asked about |
| `bPickLocksWithoutSkill` | done |  | 1 | Pick Any Lock (the lock plays as before; the difficulty option not built) |
| `bPickpocketCaughtCausesDetection` | done |  |  | Caught Pickpocketing Means Detected |
| `bPickpocketFleeingActors` | done |  |  | Pickpocket the Fleeing |
| `bPlantingLiveGrenadesRequiresWeaponSkill` | done |  |  | Planting Grenades Needs the Skill (under it the explosive stays a plain item) |
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
| `bRememberBobbyPinHealth` | done |  |  | Remember Bobby Pin Wear (kept on the player in the save) |
| `bRepairScalesCraftingCondition` | done |  |  | Crafting Uses Repair (half condition at Repair 0, whole at 100) |
| `bRepairsRewardXP` | done |  | 1 | Experience per Repair (slider; Pip-Boy repairs) |
| `bRunningCostsAP` | done |  | 8 | Action Points per Second Running (slider 0-20, a flat rate; the Endurance formula not built) |
| `bRunSlowerInWater` | done |  | 1 | Wading Pace (slider; deeper water slows more) |
| `bScalePlayerMeleeReach` | done |  | 3 | Melee Reach (multiplier slider; the min and max options not built) |
| `bScopeHoldBreath` | done |  | 7 | Hold Breath Through a Scope (Shift; the weapon requirement and key options not built) |
| `bSellStolenItems` | done |  |  | already so: barter never checks stolen items |
| `bSemiAutoQueue` | done |  | 2 | Semi-Auto Shot Queue (slider, up to 0.5 s before the weapon is ready) |
| `bSeparateAlcoholEffects` | done |  |  | already so: effects are kept per item, so different drinks stack |
| `bSkipVeryEasyLocksAtMaxSkill` | done |  | 2 | Skip Easy Locks (+ margin; full XP; use again to go through) |
| `bSleepHealingMinDuration` | done |  | 2 | Sleep Healing Min Duration (hours; every bed, the bed's owner is not known; bMiscStatRequiresSleepDuration not done) |
| `bSleepOnChairs` | done |  |  | Sleep on Chairs |
| `bSlowBreathRegen` | done |  | 2 | Slow Breath Regen (seconds of breath a second at the surface; the meter already stays up until whole) |
| `bSneakAttackWithoutCrouching` | done |  |  | Sneak Attacks Standing |
| `bSpeedMultScalesJumpHeight` | done |  |  | Speed Scales the Jump (the player; NPCs do not jump) |
| `bStealingSendsNoAlarm` | done |  |  | Quiet Theft |
| `bUnequipWeaponMods` | done |  | 7 | already so: attached mods can be taken off and come back (item_mod detach) |
| `bUseCustomSniperZoomRate` | done |  | 2 | Scope Zoom Time (slider; the zoom-out option not built) |
| `bUsingKeysRewardsXP` | done |  | 3 | Experience for Keys (a share of the picking XP; the base and skill options not built) |
| `bUsingNotesRewardsXP` | done |  | 3 | Experience for Passwords (a share of the hacking XP, once; the base and skill options not built) |
| `bVATSHipFire` | done |  |  | V.A.T.S. Hip Fire (entered not aiming: every gun shot from the hip) |
| `bVATSIgnoreNonExplosiveProjectiles` | done |  |  | already so: no projectile can be targeted in V.A.T.S. |
| `bWeaponModdingSkillRequirement` | done |  |  | Weapon Mods Need the Skill (the Pip-Boy mod list) |
| `bWeightlessWornArmor` | done |  | 1 | Worn Armor Weighs Nothing (weightless; the scale option not built) |
| `bWeightlessWornPowerArmor` | done |  | 3 | Worn Power Armor Weighs Nothing (weightless; the scale and requirement options not built) |
| `iMaxCharacterLevel` | done |  | 1 | Level Cap (0 to 100; no add-on levels) |
| `iPerksPerLevel` | done |  | 1 | Perks per Level (1 to 5) |
| `bHiddenFragsFix` | to do | yes |  | NPCs' own unplayable explosives are hidden from the pickpocket list |
| `bMoreFrequentNPCLightUpdates` | unsure, skipped | yes | 1 | NVForever lights actors its own way; unclear what to change |
| `bCharacterRadiation` | to do |  | 1 | not built: only the player takes environment radiation |
| `bCrippledLimbsPlayPainSoundWhenFalling` | to do |  | 1 | falls::hurt with a crippled leg; NVForever has no pain sound, only the landing ones |
| `bHardcoreTweaks` | to do |  | 3 | unsure which way each option goes outside Hardcore |
| `bHipFireAnimsWhileScoped` | unsure, skipped |  |  | depends on alternate animations |
| `bHoldToActivate` | to do |  | 9 | a hold on E is taken by grab; needs a timer and per-kind choices |
| `bHostilesPrioritizePlayer` | to do |  | 3 | Awareness::combat_target has no game or rolls |
| `bKeepFallHeightOnLoad` | to do |  |  | the fall top is not saved |
| `bMapSelectableCompanions` | to do |  | 2 | not built: companions are not on the map and cannot be summoned |
| `bNoLockEncounterZoneLevels` | unsure, skipped |  |  | encounter-zone level locking not yet checked in NVForever |
| `bNPCsDropLiveGrenades` | to do |  |  | not built: NPC grenades have no in-hand phase |
| `bPerBulletLoopingReloads` | to do |  | 2 | a press counter on the attack Cycle |
| `bPickpocketOverhaul` | to do |  | 15 | Steal::chance needs weight, Perception and detection |
| `bPreventRepairIfNotAtWorkbench` | to do |  |  | workbenches are activators whose own scripts open the recipe menu; nothing marks one for the crosshair |
| `bQuickUse` | to do |  | 9 | two input paths that do not exist yet |
| `bReviveUnconsciousCompanions` | to do |  | 3 | not built: no revive action |
| `bRunAndGunAPInCombatOnly` | to do |  |  | not built: no run-and-gun AP rule |
| `bScaleWeaponVolume` | to do |  | 2 | PlaySound has no volume |
| `bSneakingDoesntForcePowerAttacks` | unsure, skipped |  |  | depends on separate animations |
| `bStrengthAffectsAllThrowables` | to do |  |  | not built: fThrowingStrengthPenalty is not read |
| `bTargetProjectilesInVATS` | to do |  | 1 | not built: projectiles in flight are not targetable |
| `bUseArmorRepairKitsInRepairMenu` | to do |  |  | the repair menu lists only parts |
| `bUseCustomBarterPriceMultipliers` | to do |  | 13 | refresh_trade price; ten multipliers |
| `bUseWeaponRepairKitsInRepairMenu` | to do |  | 2 | the repair menu lists only parts |
| `bVATSDequeueWeaponShotsOnDisarm` | to do |  |  | vats_mode::shoot needs ActorParts |
| `bVATSTargetPlayerProjectiles` | to do |  |  | not built: projectiles in flight are not targetable |
| `bVATSThrowablesUseVisibility` | to do |  |  | not built: V.A.T.S. chance has no visibility term |

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
| `bFatalNonSneakCritsAlwaysGib` | done |  | 1 | Fatal Criticals Always Gib (melee and shots; bOnlyCritsGib not offered) |
| `bFirstPersonVATS` | done |  |  | First Person V.A.T.S. |
| `bHideEnemyMarkers` | done |  | 1 | No Enemies on the Compass (the firing-enemies option is not built) |
| `bIgnoreCompanionsVATS` | done |  | 1 | V.A.T.S. Ignores Companions (always; bCombatOnly not done) |
| `bKillsRewardAP` | done |  | 1 | Action Points per Kill (slider 0-50; given in VATS too) |
| `bLocationalMeleeDamage` | done |  | 3 | Enemy Blows Hit Where They Land (the player's blows already did; NPC on NPC not built) |
| `bMaxVATSDistanceUsesWeaponRange` | done |  | 3 | already so: V.A.T.S. reach is the projectile range |
| `bNoCompanionKillXP` | done |  | 1 | No XP for Companion Kills (also leaves their damage out of the kill share) |
| `bNoDisarmCompanions` | done |  |  | Companions Keep Their Weapons |
| `bNoHealingInCombat` | done |  | 1 | No Healing In Combat (Pip-Boy inventory and Stats page; the weapon hotkeys still use aid) |
| `bNoPipBoyInCombat` | done |  | 5 | No Pip-Boy In Combat (a corner message; the AP option bAllowPipboyUsingActionPoints not done) |
| `bNoScaleNpcDamageByCondition` | done |  |  | NPC Weapons Hit Whole |
| `bNoScaleNpcDamageResistanceByCondition` | done |  |  | NPC Armour DR Ignores Condition |
| `bNoScaleNpcDamageThresholdByCondition` | done |  |  | NPC Armour DT Ignores Condition |
| `bNoSelfExplosionDamage` | done |  |  | Own Blasts Spare the Player |
| `bNoSelfMeltdownDamage` | done |  | 3 | No Self Meltdown Damage (the player and companions; the three sub-options as one) |
| `bNoSneakAttacksCriticals` | done |  |  | No Sneak Criticals |
| `bNoVATSTargetInvisible` | done |  |  | already so: cloaked actors are barred without Enhanced Sensors |
| `bNoWeaponConditionDamagePenalty` | done |  |  | No Condition Damage Penalty |
| `bNPCsCanSneakCritPlayer` | done |  |  | NPCs Sneak Attack the Player (when the player has measured them below Detected) |
| `bPlayerMeleeDamageIgnoresScale` | done |  |  | already so: no damage formula reads actor scale |
| `bPlayerNonMeleeDamageIgnoresScale` | done |  |  | already so: no damage formula reads actor scale |
| `bPowerAttackWhileOverencumbered` | done |  |  | already so: nothing gates power attacks on weight |
| `bRemoveWeaponDamageBuffer` | done |  |  | No Condition Damage Buffer (full damage only at full condition) |
| `bScaleCriticalDamage` | done |  | 1 | Critical Damage (x0 to x2) |
| `bSneakCriticalsHeadshotsOnly` | done |  |  | Sneak Criticals Head Only |
| `bSneakCriticalsOnlyMeleeWeapons` | done |  |  | Sneak Criticals Melee Only |
| `bVATSStopBurstIfTargetDead` | done |  |  | V.A.T.S. Burst Stops on a Kill |
| `bWeaponRequirementsMatter` | done |  | 7 | Weapon Requirements Matter (Strength and skill both; the ignore options not built) |
| `bHoldAndReleaseThrowables` | to do |  | 1 | not built: no hold-to-release throw |
| `bNPCsCanDisarmPlayer` | to do |  |  | the player weapon lives in Combat; dropping it is new |
| `bVATSAutoTargetHead` | to do |  |  | NVForever's V.A.T.S. marks the part under the crosshair; there is no default part to make the head |

### Quality-of-life

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bBetterAutoWalk` | done | yes | 1 | already so: strafing does not end auto-walk |
| `bBetterPickupPrompt` | done | yes |  | Whole-Stack Pickup Figures (the stack's weight and value; no carried total) |
| `bConsoleHistoryNoStoreDuplicates` | done | yes |  | already so: the console keeps no repeated line |
| `bCrouchWhileEquippingWeapons` | done | yes |  | already so: crouching is never refused while (un)equipping |
| `bDecreasedDialogueClickDelay` | done | yes |  | already so: topics can be chosen at once |
| `bDelayPostCombatLevelUp` | done | yes |  | already so: the level-up menu waits for combat to end (after_combat) |
| `bDelayPostCombatReputationPopup` | done | yes |  | already so: the reputation box waits for combat to end |
| `bDeselectHotkeys` | done | yes |  | Unassign Hotkeys |
| `bFasterControllerPOVRotate` | done | yes |  | Faster Controller Camera Circling (twice as fast, ours) |
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
| `bReloadingWithNoAmmoSwapsAmmoTypes` | done | yes |  | Reload Swaps Spent Ammo (on a reload; not on loading a save) |
| `bScaleMusicVolumeDuringDialogue` | done | yes | 1 | Music While Talking (slider) |
| `bScaleRadioSongVolumeDuringDialogue` | done | yes | 1 | Radio While Talking (slider) |
| `bShiftIgnoresFriendlyVATS` | done | yes | 1 | Shift Ignores Friends in V.A.T.S. (bHideFriendliesByDefault not done) |
| `bShiftScreenshotHidesMenus` | done | yes |  | Shift Screenshot Hides the HUD (as tm hides it; open menus stay) |
| `bSkipIntroVideo` | done | yes |  | already so: the intro plays only after New Game and any key skips it |
| `bVatsExitKey` | done | yes | 1 | Skip Kill Camera (right click; Tab is the Pip-Boy key here) |
| `bActivatingDoesntStandUp` | done |  |  | Activate Doesn't Stand Up (moving already stood up) |
| `bAllowActivateWhileAnimPlays` | done |  |  | already so: activating is never refused for a clip playing |
| `bAllowFiringWhileLanding` | done |  |  | already so: firing waits only for the weapon to be ready |
| `bAllowOpenPipboyDuringCameraShake` | done |  |  | already so: no camera shake blocks the Pip-Boy |
| `bAllowUnsafeFastTravel` | done |  | 1 | Fast Travel From Anywhere (no warning; bShowWarning not done) |
| `bAllowVATSWhileAnimsPlay` | done |  |  | already so: no animation refuses V.A.T.S. |
| `bAllowWeaponHotkeysAndPipBoyWhileReloading` | done |  |  | already so: nothing refuses them while reloading |
| `bAllowWeaponHotkeysWhileEquipping` | done |  |  | already so: the swap waits its turn |
| `bAllowWeaponHotkeysWhileFiring` | done |  |  | already so: nothing refuses hotkeys while firing |
| `bDisableCompanionKillcam` | done |  |  | already so: only the player's VATS kills start a kill camera |
| `bDisableExplosionInFaceIMOD` | done |  |  | No Explosion Blur |
| `bDisableHitShader` | done |  | 1 | already so: NVForever draws no hit shader on the player |
| `bDisableHolsteredWeaponFOVZoom` | done |  |  | already so: a holstered weapon gives no zoom |
| `bDisableInteriorFog` | done |  | 1 | No Interior Fog (from the next interior; the threshold option not built) |
| `bDisableTutorialMessages` | done |  |  | No Minigame Help (lockpick, blackjack, roulette, slots; Caravan not yet) |
| `bDisableWeaponFOV` | done |  | 1 | No Iron Sight Zoom (the non-sighted option not built) |
| `bDontSetQuestWhenCompletingObjectives` | done |  |  | No Active Quest After Completing One |
| `bDontSetQuestWhenObjectivesAdded` | done |  |  | No Active Quest From New Objectives (both quest tweaks keep the Pip-Boy from picking one) |
| `bEnteringVATSDoesntUnholsterWeapon` | done |  |  | V.A.T.S. Puts the Weapon Back (when it drew it and nothing was fired) |
| `bFireWhileAiming` | done |  |  | already so: the trigger does not wait for the zoom |
| `bForceHiResWeaponModels` | done |  |  | Fine Weapon Models for Everyone (not checked in game) |
| `bHideMiscQuestItems` | done |  | 1 | Hide Quest Items in Misc (the star caps option not built) |
| `bHoldCrouchToSneak` | done |  |  | Hold to Sneak |
| `bHoldWaitKeyToShowMenu` | done |  | 1 | Hold to Wait (half a second; the hold time option not built) |
| `bHotkeyHolstersWeaponIfEquipped` | done |  | 1 | Hotkey Draws and Holsters (the never-holster option not built) |
| `bHUDRotatesWithVanityCam` | done |  |  | already so: the compass follows the orbiting camera |
| `bJumpingDoesntDropGrabbedItem` | done |  |  | already so: jumping does not let go |
| `bKeepHolotapePlayingWhenSelectingOtherNotes` | done |  |  | already so: only voice and sound notes start playing |
| `bKeepRouletteBetAmount` | done |  |  | already so: the bet stays between spins |
| `bKeepXPBarWhenClosingMenus` | done |  |  | already so: the XP meter holds while a menu is up |
| `bLessFrequentPlayerPainSounds` | done |  | 1 | Less Frequent Pain Sounds (seconds between the player's Hit lines) |
| `bLockpickHackingMessageShowsCurrentSkill` | done |  | 2 | Say My Skill |
| `bMidairFastTravel` | done |  |  | Fast Travel in the Air |
| `bMinBloodSplatterHealthDamage` | done |  | 1 | Blood Only on Big Hits (slider) |
| `bMinigamesPlayXPSoundAtMaxLevel` | done |  |  | XP Sound at the Level Cap (any XP at the cap, not only minigames) |
| `bMoveDuringVATSPlayback` | done |  |  | Move During V.A.T.S. Camera (walk and run only) |
| `bNoAutoContinueDialog` | done |  |  | Click to Continue Dialogue |
| `bNoDropWeaponSoundOnPlayerDeath` | done |  |  | already so: the player weapon is not put away at death |
| `bNoEquippedWeaponMovementPenalty` | done |  |  | No Weapon Movement Penalty (the two-handed run slowdown) |
| `bNoExteriorLoadScreens` | done |  |  | Plain Screen Going Outside |
| `bNoFoodWornOffMessage` | done |  | 1 | already so: no worn-off message is shown |
| `bNoKillcamKillSound` | done |  |  | already so: NVForever plays no sound when a kill camera starts |
| `bNoKnockdownInGodmode` | done |  |  | No Knockdowns in God Mode |
| `bNoMapMarkerAddedPopup` | done |  |  | already so: NVForever shows no marker-added popup |
| `bNoPipboyIdleAnims` | done |  |  | already so: only the raise clip plays |
| `bNoReputationMessages` | done |  | 5 | Quiet Reputation (box and sound together; per-kind options not built) |
| `bNoScreenshotPopup` | done |  | 3 | already so: NVForever shows no message after a screenshot |
| `bNPCsDropWeaponHolsteredWeapon` | done |  |  | NPCs Drop Holstered Weapons |
| `bPauseOnSaveLoad` | done |  |  | Pause On Load (a loaded save opens the pause menu) |
| `bPickLocksEvenWithKey` | done |  | 1 | Pick Locks Even With the Key (Shift held; locks, not terminals; bInvert not done) |
| `bPopupMenusDontMoveCursor` | done |  |  | already so: nothing moves the pointer |
| `bPreventBushPassthroughSounds` | done |  |  | already so: walking through plants makes no sound |
| `bQuestTextVisibleWhileAiming` | done |  |  | already so: aiming hides only the reticle |
| `bRandomizeKillcamMode` | done |  |  | n/a: NVForever has one kill camera kind |
| `bRemoveChemWarnOffIMOD` | done |  |  | No Chem Wore-Off Effect |
| `bRemoveLandingAnim` | done |  |  | No Landing Animation (the player) |
| `bRepairRequiresWeaponSkill` | done |  |  | Repair Needs the Weapon's Skill (Pip-Boy repair) |
| `bRepeatJumping` | done |  |  | Hold to Keep Jumping |
| `bScopeVisibilityDelay` | done |  | 4 | Scope Delay (ms) (stops aiming while reloading was already so; the other options not built) |
| `bSkipVigorTesterSpecialPages` | done |  |  | S.P.E.C.I.A.L. Machine: Straight to the Summary |
| `bSlotsAutoSpinHotkey` | done |  |  | Slots: Hold to Spin (W held) |
| `bSwapAmmoWithWeaponHolstered` | done |  |  | Swap Ammo While Holstered (reloads when drawn) |
| `bTurnFurtherWhileSeated` | done |  |  | Turn Further While Seated (first person 115 degrees; third person was already free) |
| `bTurnSlowerWhileAiming` | done |  | 2 | Aiming Turn Speed: Across / Up and Down (two sliders) |
| `bUnequipBrokenArmor` | done |  | 1 | Unequip Broken Armour (no message, sArmorBreakMessage not done) |
| `bUnspokenNPCIndicator` | done |  |  | Mark Strangers |
| `bUseFirstPersonEmptyClipSound` | done |  |  | already so: one empty-clip sound, played at the listener |
| `bDoubleTapReloadToChangeAmmoType` | to do | yes | 2 | the owner moved Lower Weapon off a double tap because it held reloads back; ask first |
| `bRestore2Hotkey` | unsure, skipped | yes | 2 | needs the plugin's own menu XML |
| `bSelectUnavailableRadios` | to do | yes |  | the Pip-Boy lists only stations in range |
| `bSkipDeathcamHotkey` | to do | yes | 2 | the death camera (nvforever death_cam) has no end but a load yet |
| `bAddItemUsesItemIcon` | unsure, skipped |  |  | needs an icon replacer |
| `bAddNightVisionToggle` | to do |  | 4 | a night vision state plus flashlight and scope_view |
| `bEquipBrokenItems` | to do |  |  | broken weapons are refused in fnv_sim equip_at; needs a World flag like power armour |
| `bKeepBrokenItemsEquipped` | to do |  |  | the break unequips at three sites; firing a broken weapon is undefined |
| `bMoveDuringOpenPipboyAnim` | to do |  |  | the Pip-Boy is a menu; the game leaves Playing at once |
| `bNoLTRTOnPipboy` | n/a |  |  | the original's controller texture swap |
| `bNoPoisonConfirm` | to do |  |  | not built: no weapon poison |
| `bRemoveFollowerTopics` | to do |  |  | not built: companions make no such comments |
| `bRobotCompanionsHealWithScrapMetal` | to do |  | 4 | robot detection, another item and a heal formula |
| `bScaleExplosionShake` | to do |  | 2 | not built: explosions do not shake the camera |
| `bToggleControllerIfAttackPressed` | to do |  | 1 | not built: the device changes only by setting |
| `bWeaponCycleUpDownHotkeys` | to do |  |  | D-pad left and right are bindings (Hotkey 4, Ammo Swap) a player can change; cycling would override a binding |

### HUD

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAddRGBSliders` | done | yes | 1 | already a row: Interface tab, hud-red / hud-green / hud-blue |
| `bEnemyHealthbarShowBuffedHP` | done | yes |  | Health Bars Count Buffs |
| `bHideRedCrosshairOnDistantInvisibleTargets` | done | yes |  | Invisible Foes Stay Hidden (at any distance) |
| `bColoredHUDBars` | done |  | 2 | Red Health / Action Point Bar Below (two shares) |
| `bColorWeaponCndLabel` | done |  | 2 | Red / Flashing Weapon Condition Below (the bar, the HUD has no label) |
| `bCompanionPipColorChange` | done |  | 1 | Companion Compass Colour (five colours of ours instead of an RGB value) |
| `bCompassFadeLeftSide` | done |  |  | Compass Fades on the Left (down to a fifth at the left end, ours; quest marks stay whole) |
| `bCompassLocationDistanceBasedAlpha` | done |  |  | Fade Places by Distance |
| `bCompassNPCDistanceBasedAlpha` | done |  |  | Fade Pips by Distance |
| `bCustomAddToInventoryMessageTimer` | done |  | 1 | Item Added Line Time (slider, seconds) |
| `bCustomSubtitleDistance` | done |  | 1 | Subtitle Distance (slider, game units) |
| `bDisableCompassEdgeAlpha` | done |  |  | already so: the marks are not faded at the edge |
| `bDisableGrenadeIndicator` | done |  |  | already so: NVForever has no grenade indicator |
| `bDisableHUDCrippledLimbIndicator` | done |  |  | No Crippled Limb Figure |
| `bDistanceBasedQuestMarkerVisibilty` | done |  | 2 | Quest Markers Within (slider in game units, one distance indoors and out) |
| `bHideAmmoLabel` | done |  | 1 | No Ammo Counter (the total-remaining option not built) |
| `bHideCapsAddedMessages` | done |  |  | No Caps Added Line |
| `bHideCrosshairInFirstPerson` | done |  |  | No Crosshair in First Person (the prompt stays) |
| `bHideCrosshairInThirdPerson` | done |  |  | No Crosshair in Third Person (the prompt stays) |
| `bHideCrosshairWhileReloading` | done |  |  | No Crosshair While Reloading |
| `bHideInvisibleUndiscoveredLocations` | done |  |  | Compass: Only Mapped Places |
| `bHideUnknownNPCNames` | done |  | 1 | Hide Unknown Names (bodies are named, as bShowNameOnDeadNPCs) |
| `bHostilesUseNeutralColorOnCompass` | done |  |  | Foes in the Usual Colour |
| `bKeepCrosshairWhenAiming` | done |  |  | Crosshair Down the Sights |
| `bLocationDiscoveredCornerMessage` | done |  |  | Discoveries in the Corner |
| `bMoveAmmoTypeLabel` | done |  |  | Ammo Label Under the Count |
| `bNoCompassPipsIfNotInDanger` | done |  |  | Compass Pips Only in Danger |
| `bNoCrippleCriticalMessages` | done |  | 3 | already so: no crippled or critical popups about the target are shown |
| `bNoHUDHotkeyPopup` | done |  |  | No Hotkey Wheel (a held hotkey is a tap) |
| `bNoKarmaMessages` | done |  | 7 | Quiet Karma (messages and sounds together; no thresholds yet) |
| `bNoLocationPopup` | done |  |  | No Location Banner |
| `bNoQuestFailedPopup` | done |  | 1 | No Quest Failed Banner |
| `bNoRedCrosshairOnEnemies` | done |  |  | No Red Crosshair (native and tile HUD alike) |
| `bNoUnconciousMessage` | done |  |  | No Unconscious Message |
| `bNoXPBarInCombat` | done |  | 1 | No XP Meter in Combat (no delay option) |
| `bNoXPMessages` | done |  | 7 | Quiet Experience (all sources at once; LEVEL UP still shows) |
| `bPatchUnseenCellName` | done |  | 3 | Mark Unvisited Doors (a star; the rename and respawn options not built) |
| `bRaceMenuAllowFemaleFacialHair` | done |  |  | Facial Hair for Women (creator and barber rows) |
| `bRecentlyDeadNPCIndicator` | done |  | 4 | Recently Dead on the Compass (seconds slider; the killer filters not built) |
| `bRemoveDeadNPCCrippleCriticalMessages` | done |  |  | already so: no cripple or critical messages |
| `bRemoveQuestObjectiveAddedText` | done |  | 1 | No Objective List |
| `bRemoveSneakLabel` | done |  |  | No Sneak Label (native and tile HUD) |
| `bScopeVisibleAPHP` | done |  | 1 | already so: HP and AP stay up while scoped |
| `bShowNearestUndiscoveredLocation` | done |  | 2 | Nearest Unfound Place (the compass; the map option not built) |
| `bShowQuestObjectivesOnCellChange` | done |  | 1 | Objectives on Arrival, with the gap slider |
| `bShowSneakLabelWhileStanding` | done |  |  | Sneak Label While Standing |
| `bShowUsePasswordOnTerminalsWithNote` | done |  | 1 | Show Use Password (until first opened this session; sUsePassword is our words) |
| `bSubtitlesShowActorNames` | done |  |  | Subtitles Name the Speaker |
| `bTalkWhileSneakingIfShiftIsHeld` | done |  |  | Shift to Talk While Sneaking |
| `iDateFormat` | done |  | 1 | Date Format (four styles of ours) |
| `iHUDMaxCompassNPCTicks` | done |  | 1 | already so: the compass has no cap on actors |
| `bDescriptiveMarkerAddedMessage` | to do | yes |  | NVForever shows no marker-added popup |
| `bHideGrenadeIndicatorForNoDamageExplosions` | to do | yes |  | not built: no grenade indicator |
| `bMorePreciseRadMeter` | to do | yes | 1 | unclear whether the HUD gain or the Pip-Boy total is meant |
| `bArmorConditionLabel` | to do |  | 9 | no armour condition text on the HUD yet |
| `bCompassQuestHeightIndicator` | to do |  |  | the compass marks are 2D; needs art |
| `bCriticalHitMessagesIncludeLimbName` | to do |  |  | not built: no critical-hit messages |
| `bDetectedByWhom` | to do |  | 5 | Sneak is a Copy enum; names need a new type |
| `bDisableNeedsMessages` | to do |  | 4 | those messages are the game's own scripts; needs a filter by message |
| `bExplodingDestructibleIndicator` | to do |  |  | not built: no grenade indicator |
| `bHUDFatigueIndicator` | to do |  |  | the needs words are a fixed five |
| `bHUDMarkerNameIndicator` | to do |  | 7 | no such HUD text yet |
| `bHUDShowRegionNames` | to do |  | 1 | not built: the HUD has no region text |
| `bHUDWeaponNameLabel` | unsure, skipped |  | 5 | needs the plugin's own menu XML |
| `bShowDoorsOnCompass` | to do |  | 5 | a door mark kind, icons and gathering |
| `bWaterSourcesShowH2O` | to do |  | 1 | not built: water prompts carry no line |
| `bPlacedMarkerHeightIndicator` | to do |  | 1 | the compass draws no custom marker yet |

### Menus

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bBarterShowCapsChange` | done | yes | 1 | Barter Shows Caps After (hand-built barter menu; one mode, our words; the tiles menu not yet) |
| `bCapitalizeRecipeCategories` | done | yes |  | Recipe Categories in Capitals |
| `bClickingShowsTerminalText` | done | yes |  | already so: terminal text is shown whole at once |
| `bColorRecentlyAddedMapMarkers` | done | yes |  | New Places Red on the Map (places found; markers given by scripts not) |
| `bConsoleNumpadSupport` | done | yes |  | already so: numpad digits type into the console (with Num Lock) |
| `bDisplayItemEffectTotals` | done | yes |  | already so: the EFFECTS figure counts an effect over time whole (no per-second breakdown) |
| `bHideCursorInDialog` | done | yes | 1 | No Pointer in Conversations |
| `bImprovedRaceMenu` | done | yes | 3 | already so: the creator camera pans and zooms close |
| `bInstantContinueButton` | done | yes |  | already so: Continue loads at once |
| `bLevelUpScrollWheelSupport` | done | yes |  | Level Up: Wheel Sets Skills |
| `bLivingAnatomyShowDR` | done | yes | 1 | Living Anatomy Shows DR (the decimal and zero-hiding options not built) |
| `bMapRemembersPosition` | done | yes | 1 | Map Remembers Its Place (in the session; the save-persistent option not built) |
| `bNumberedComputerHotkeys` | done | yes | 1 | Number Keys on Terminals (1-9, 0 the tenth; the number prefix option not built) |
| `bNumberedDialogHotkeys` | done | yes | 2 | already so for 1-9 (the 0 key and number prefixes not built) |
| `bRecurringChallengeIndicator` | done | yes | 1 | Mark Repeating Challenges |
| `bSeparateHorizontalSensitivity` | done | yes |  | Horizontal Look Speed (a factor on left and right, on the Tweaks tab) |
| `bShowBookEffects` | done | yes | 1 | Skill Book Effects |
| `bShowWeaponAmmoUseInMenus` | done | yes |  | Rounds per Shot |
| `bSkipSkillMenuIfNoPointsToAssign` | done | yes |  | Perks First When No Points |
| `bSortEquipableAmmo` | done | yes |  | Usable Ammo First |
| `bSynchronizeContainerCategories` | done | yes | 1 | Matching Categories (the option for emptying a category not built) |
| `bTakeAllInCompanionContainers` | done | yes |  | already so: Take All shows for companions |
| `b12HourPipboyClock` | done |  |  | 12-Hour Pip-Boy Clock |
| `b24HourSleepWaitClock` | done |  |  | already so: the wait clock is 24-hour |
| `bAddInventoryDropItemHotkey` | done |  |  | Q Drops Items |
| `bAllowRebindNumkeys` | done |  |  | already so: the hotkeys and the D-pad are on the Controls tab |
| `bBarterQuantityMenuShowsPrice` | done |  |  | Quantity Box Shows the Price (the hand-built box; the tiles box not yet) |
| `bBetterMapZoom` | done |  |  | Map Zooms at the Pointer |
| `bBlackJackTotalDisplay` | done |  |  | Blackjack Hand Total |
| `bChangingContainerCategoryScrollsToTop` | done |  |  | already so: a category change goes to the top |
| `bCompareArmorStats` | done |  |  | Compare Armor (DT; DR is 0 on New Vegas armour) |
| `bCompareWeaponStats` | done |  |  | Compare Weapons (DAM and DPS, both at full condition) |
| `bContainerEncumbranceIndicator` | done |  |  | Weight Warning (the selected row, the whole stack) |
| `bContainerMenuStoreAllHotkey` | done |  | 1 | Store All (Shift with Take All; worn and quest items stay) |
| `bContainerQuantityMenuShowsWeight` | done |  |  | Quantity Box Shows the Weight (the hand-built box; the tiles box not yet) |
| `bContainerTakeAllDoesntCloseMenu` | done |  |  | Take All Stays Open |
| `bDialogueKeepVoiceActingNotes` | done |  |  | Keep Acting Notes (spoken lines; topics as before) |
| `bDisableInventoryRightClickDrop` | done |  |  | No Right-Click Drop |
| `bDisableLoadingScreenTips` | done |  |  | No Loading Tips |
| `bDisableShowQuestLocation` | done |  |  | No Show Location |
| `bHideCompletedQuests` | done |  |  | Hide Finished Quests |
| `bHideCursorInMessageMenu` | done |  |  | No Pointer Over Message Boxes |
| `bHideEquippedItemsInBarter` | done |  |  | Hide Worn Items in Barter |
| `bHideEquippedItemsInContainers` | done |  |  | Hide Worn Items in Containers |
| `bHideHotkeyedItemsInBarter` | done |  |  | Hide Hotkeyed Items in Barter |
| `bHideHotkeyedItemsInContainers` | done |  |  | Hide Hotkeyed Items in Containers |
| `bHideHotkeyedItemsInRepairLists` | done |  |  | Hide Hotkeyed Items in Repair |
| `bHideMapMarkerFactionReputation` | done |  |  | No Reputation on Map Markers |
| `bHideQuestItemWeightAndValue` | done |  |  | No Figures for Quest Items |
| `bHideRanksInTraitMenu` | done |  |  | No Ranks for Traits |
| `bHideReadNotes` | done |  |  | Hide Read Notes (the note on screen stays) |
| `bHideReputationTabIfEmpty` | done |  |  | Hide an Empty Reputation List (the hand-built Stats page; the tiles page not yet) |
| `bHideUnavailableRadios` | done |  |  | already so: the Pip-Boy lists only stations in range |
| `bIndividialItemStats` | done |  |  | One Item's Figures (the Pip-Boy; containers already show one) |
| `bInvertContainerTitleScrollwheelDirection` | done |  |  | Reverse Category Wheel |
| `bInvertPipboyRepairMenuSorting` | done |  |  | Repair List: Low Condition First |
| `bKarmaTitleShowsValue` | done |  |  | already so: the karma number is under its title |
| `bLockpickAllowMouse` | done |  |  | Mouse Turns the Lock |
| `bLockPickMenuKeyboardMovement` | done |  | 3 | Lockpicking With the Movement Keys (Shift x2, Ctrl x0.5: ours) |
| `bMapLocationDisplayDistance` | done |  | 3 | Map Marker Distance (metres or kilometres; the travel-time modes not done) |
| `bMapMarkersShowFactionName` | done |  | 1 | already so: the reputation line names the faction (worded "Goodsprings: Neutral") |
| `bMapRecenterHotkey` | done |  | 2 | Map Recentre Key (C, at once; no easing) |
| `bMarkNotesUnread` | done |  |  | Mark Notes Unread (right click on the Pip-Boy notes list) |
| `bMoreDetailedStatsMenu` | done |  | 1 | Detailed Stats |
| `bNoCapitaliseContainerCategories` | done |  |  | Quiet Category Titles |
| `bNoExitConfirm` | done |  |  | already so: the main menu's Quit asks nothing |
| `bNoExitHacking` | done |  | 1 | No Leaving a Hack (the fail-on-exit option not built) |
| `bNoPlaceMarkerPopup` | done |  | 2 | No Place Marker Question, with Shift Removes and Remove When On It |
| `bPauseHolotapes` | done |  | 1 | already so for pausing: picking the playing holotape pauses it (reset and dialogue options not built) |
| `bPlaceMarkersAtLocations` | done | yes | 1 | already so: the highlight box takes to a place or door near the pointer, and Place Marker puts the marker on it (both maps) |
| `bPreventNoFastTravelMessage` | done |  | 1 | Quiet Undiscovered Markers (the sound option not built) |
| `bPreviewPerksOnLevelUp` | done |  |  | Preview Perks on Level Up (the page with none to take) |
| `bReputationShowsFameInfamy` | done |  | 3 | Show Fame and Infamy (numbers; the percentage option not built) |
| `bRightClickChangesToPreviousContainerCategory` | done |  |  | Right Click for the Previous Category (container and barter; not the recipe menu) |
| `bSavingDoesntClosePauseMenu` | done |  |  | already so: saving leaves the pause menu open |
| `bShowActiveQuestNotesShowsAllStartedQuests` | done |  |  | Notes of Every Quest |
| `bShowAlternateAmmoTypesAvailableInMenus` | done |  |  | Mark Other Ammo |
| `bShowBarterTotalWhenOverSellLimit` | done |  |  | Barter Total Past the Merchant's Caps (hand-built barter menu; the tiles menu not yet) |
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
| `bStartMenuQuickLoad` | done |  |  | Quick Load From the Menus |
| `bStatsMenuPercentageXP` | done |  |  | Experience as a Percentage (the way through the level) |
| `bStatsMenuShowEffectTimeRemaining` | done |  | 1 | Effect Time Left (seconds) |
| `bTabBackInStartMenu` | done |  |  | Tab Goes Back in the Pause Menu (where Tab steps no tabs) |
| `bTabClosesPipboyFromKeyAndLimbMenus` | done |  |  | already so: Tab closes the Pip-Boy from every page |
| `bTabClosesTerminals` | done |  |  | Tab Closes Terminals (Backspace still goes back) |
| `bTerminalInstantDisplayHotkey` | done |  |  | already so: terminal text is shown whole |
| `bTimescaleAffectsMinigames` | done |  |  | Lockpicking Follows the Time Speed (the lock only; the card and slot games keep their pace) |
| `bChallengeMenuIcons` | unsure, skipped | yes |  | needs another mod (challenge icons) |
| `bMainMenuContinueIcon` | to do | yes |  | thumbnails are on the load page only |
| `bRememberPipboyScrollPositions` | to do | yes |  | the inventory is a new MenuModel on each opening |
| `bRemoveDownloadsButton` | n/a | yes |  | NVForever has no Downloads button |
| `bSaveCharacterSelector` | unsure, skipped | yes | 1 | needs the plugin's own menu XML |
| `bShowWeaponPoisonEffects` | to do | yes |  | NVForever has no weapon poison yet |
| `bAmmoLabelUseLongName` | unsure, skipped |  | 1 | needs a UI mod |
| `bClickingActiveTabTogglesSorting` | to do |  |  | not built: one fixed order |
| `bCompanionsUseLocationFontOnMap` | to do |  |  | not built: companions are not on the map |
| `bContainersShowEquippedAmmo` | to do |  |  | not built: ammo is never equipped |
| `bControllerBackButtonDoesntClosePipBoy` | to do |  |  | unsure which pad button is meant: East (B) works as Esc in every menu, View/Select opens nothing there |
| `bCustomHackingAnswerLength` | to do |  | 8 | the readme gives no defaults and does not say how the attempts are worked out |
| `bCustomPopupIcons` | to do |  | 17 | per-kind icons at each message |
| `bEmboldenTagSkills` | to do |  |  | no tagged flag on rows and no bold face |
| `bInteriorExteriorMapDoorIcon` | to do |  |  | doors carry no leads-outside field |
| `bKeyRepeatAcceleration` | to do |  | 4 | not built: menu keys do not repeat |
| `bLocalMapRespawnedCellIndicator` | to do |  | 2 | not built: the local map has no cell grid |
| `bMapShowUnconsciousCompanions` | to do |  |  | companions are not drawn on the world map yet |
| `bNoMinCompanionMapDistance` | to do |  |  | companions are not drawn on the world map yet |
| `bNoteMenuShowSeparateMenu` | unsure, skipped |  | 1 | needs another mod (Book Menu Restored) |
| `bQuantityMenuRespectsCompanionCarryCap` | to do |  |  | asks_quantity has no world |
| `bQuestTextVisibleInMenus` | to do |  |  | quest_messages held; check what draws over the Pip-Boy |
| `bRepairAllConfirmation` | to do |  | 2 | a confirmation box in repair |
| `bRepairItemsPreview` | to do |  | 1 | a preview session in the repair list |
| `bShowInventorySortButton` | unsure, skipped |  | 11 | needs the plugin's own menu XML |
| `bTakeAllConfirmation` | to do |  | 2 | a confirmation prompt in the menu model |
| `bTerminalGreyReadNotes` | to do |  |  | no record of entries read |
| `bVATSAPDisplayIncludesReloadCost` | to do |  |  | not built: V.A.T.S. queues never reload |

### Hotkeys

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `iConsoleKey` | done |  | 1 | Console Key (a choice of six keys) |
| `iDisableCollisionKey` | done |  | 1 | a Controls key: No Collision (held) |
| `iDropEquippedWeaponKey` | done |  | 1 | Drop Weapon key (Controls page; not a quest item or one that can't be dropped) |
| `iEquipLastWeaponKey` | done |  | 4 | Last Weapon key (Controls page; the weapon before the one now, put away) |
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
| `iPlaceMapMarkerAtPlayerPosKey` | done |  | 3 | Place Marker Here key (Controls page) and Place Marker Here Says So; the sound option is not built (no sound by editor ID) |
| `iPrevRadioStationKey` | done |  | 1 | already so: [ steps to the previous station in the Pip-Boy |
| `iRadioVolumeDownKey` | done |  | 1 | Radio Volume Down key |
| `iRadioVolumeUpKey` | done |  | 1 | Radio Volume Up key (a tenth of the Radio volume) |
| `iRepeatActivateKey` | done |  | 2 | Repeat Activate key (Controls page) and Repeat Activate Takes Only |
| `iScreenshotKey` | done |  | 1 | Screenshot Key (a choice of five) |
| `iToggleCrosshairKey` | done |  | 1 | a Controls key: Show/Hide Crosshair |
| `iToggleMenusKey` | done |  | 1 | a Controls key: Show/Hide HUD (as ToggleMenus) |
| `iTogglePipboyLightKey` | done |  | 2 | a Controls key: Pip-Boy Light (the night vision part not built) |
| `iToggleSightingNodeHotkey` | done |  | 2 | Second Iron Sights key (Controls page) |
| `iToggleSmoothCameraKey` | done |  | 2 | Smooth Camera key (Controls page; never while aiming; the rate is ours) |
| `iToggleSneakIndicatorKey` | done |  | 1 | a Controls key: Show/Hide Sneak Label |
| `iToggleTrueIronSightsKey` | done |  | 1 | True Iron Sights key |
| `iVATSAcceptKey` | done |  | 1 | already so: accepting is the Activate control, rebindable on the Controls page |
| `iHideHUDKey` | to do |  | 6 | the HUD parts have no roots of their own to hide; Toggle Menus already hides the whole HUD |
| `iSkipRadioSongKey` | to do |  | 1 | the playing line and the broadcast both move |

### Cheat

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAddSetHomeCommand` | done |  |  | Console Homes (kept for the session) |
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
| `bDebugLockpickMenu` | to do |  |  | the meter is a developer aid; developer escape hatches stay out of player builds (owner rule), ask first |
| `bReallocateSkillPointsOnLevelup` | to do |  |  | earlier levels spent points are not recorded |

### Misc

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bArmorSoundsPlayIn3D` | done | yes |  | Armour Rustle From the Body |
| `bBetterFlycam` | done | yes | 5 | Better Free Camera (the aim and run speed multipliers not built) |
| `bCompressDuplicateConsoleMessages` | done | yes |  | Count Repeated Console Lines |
| `bConsolePrintsIncludeTimestamp` | done | yes | 1 | Console Times (time since the game started; a wall-clock format would need a date crate) |
| `bConsoleTextShadow` | done | yes |  | Console Text Shadow (our shade and offset) |
| `bMeleeImpactEffectsFollowTarget` | done | yes |  | already so: decals on actors hang from a bone |
| `bModConsolePrintsIncludeName` | done | yes | 1 | Console Lines Name Their Plugin (no script ID option) |
| `bMoreResponsiveControllerAiming` | done | yes |  | More Responsive Controller Aiming (straight curve past the dead zone, ours) |
| `bNoInteriorBlackLoadingScreen` | done | yes | 1 | already so: an interior's loading screen shows its picture, never a black one |
| `bNoPlayerNameLimit` | done | yes |  | No Name Length Limit (256 characters) |
| `bPowerArmorPreventsScreenBlood` | done | yes |  | No Screen Blood in Power Armor (any hit while power armor is worn on the body; hits are not told apart by body part here) |
| `bRadioStaticDecreasesSongVolume` | done | yes |  | already so: the song fades as the static rises |
| `bRememberConsoleHistory` | done | yes | 1 | Remember Console History (ConsoleHistory.txt in NVForever's folder, last 100 lines) |
| `bAllowDuplicateControlBinds` | done |  |  | Shared Keys |
| `bAutoContinueGame` | done |  |  | Continue at Start (once per start of the program; Shift held stays on the menu) |
| `bChanceBasedKillcams` | done |  | 1 | Kill Camera Chance (slider; V.A.T.S. kill cameras, the only kind) |
| `bClickToExitLoadScreens` | done |  | 2 | Press to Leave Loading Screens (every screen; bHideWheel and bLoadGameOnly not done; our prompt words) |
| `bContainerRespawnsMessage` | done |  | 2 | already so: respawning containers say so (not on NPCs) |
| `bControllerTriggerDeadzones` | done |  | 2 | Trigger Dead Zones (press points per trigger, on every use of LT/RT) |
| `bDetachedBeams` | done |  |  | Detached Beams |
| `bDisableControllerDeadzones` | done |  | 2 | Controller Dead Zones (ours on top of Bevy's own small dead zone) |
| `bDisableGodmodeOnLoad` | done |  |  | God Mode Off on Load |
| `bDisableSellingItems` | done |  |  | No Selling |
| `bDisableStealthEffectInPipboy` | done |  |  | No Stealth Shimmer in the Pip-Boy |
| `bDisableWeaponHotkeys` | done |  |  | No Weapon Hotkeys (the wheel still picks) |
| `bDisarmingMinesPicksThemUp` | done |  |  | Pick Up Disarmed Mines |
| `bDontOpenDoorsAfterLockpick` | done |  |  | Picked Doors Stay Shut |
| `bDontShowContainerAfterLockpick` | done |  |  | Picked Containers Stay Shut |
| `bExtraConsoleDetails (enabled by default)` | done |  | 4 | Console Shows Models (cell name and Editor ID were already shown; full help not built) |
| `bHapticPipboy` | done |  |  | Pip-Boy Buzz (with rumble on) |
| `bInvertCameraX` | done |  |  | Invert Look Left/Right |
| `bKeepPipboyLightOnCellChange` | done |  | 1 | already so: the light stays on through cell changes |
| `bKeepSelectedConsoleRef` | done |  |  | already so: closing the console keeps the selection |
| `bLethalHitsIncreaseLimbChallenges` | done |  |  | Killing Knockdowns Count (crippling already counted on a kill) |
| `bMenuFadesIgnoreTimescale` | done |  |  | already so: menu fades run on real time while the world waits |
| `bNoAltTabPause` | done |  | 1 | already so: losing focus opens no pause menu |
| `bNoAmmoCasings` | done |  |  | No Spent Cases (the player) |
| `bNoAudioDistortion` | done |  | 1 | No Helmet Voice Filter (the apply-to-everything option not built) |
| `bNoFastTravelTimeChange` | done |  |  | Fast Travel Takes No Time |
| `bNoHotkeyEquipSounds` | done |  |  | No Hotkey Sounds |
| `bNoScrollwheelPOVChange` | done |  | 2 | already so: the wheel only zooms the third-person camera |
| `bNoSkillBooksAbove100` | done |  |  | No Skill Books at 100 (Pip-Boy only; hotkeys and EquipItem still read it) |
| `bNoSkipDialog` | done |  |  | No Skipping Lines |
| `bNoTurningAnim` | done |  |  | No Turning Animation |
| `bOnlyChangeCameraHeightIfPOVKeyHeld` | done |  |  | Wheel Zoom Needs Toggle View (NVForever's wheel moves the camera distance) |
| `bOpenPipboyToInventoryByDefault` | done |  | 1 | Pip-Boy Opens on Items (the tab option not built) |
| `bPipBoyTabHotkeysDontCloseMenu` | done |  |  | already so: F1-F3 change page, only Tab or Esc close |
| `bPreventNPCComments` | done |  | 1 | already so: NPCs make no such comments |
| `bPreventTeammateFootstepSounds` | done |  |  | Quiet Companion Footsteps |
| `bRadiationCausesRadioStatic` | done |  | 1 | Radiation Brings Radio Static (whole at 10 rads/s, at most 80%: ours) |
| `bRestoreLockedDoorSound` | done |  |  | already so: a refused lock plays DRSLocked |
| `bScaleAshpileSize` | done |  | 2 | Ash Piles Fit the Body (the actor's scale, half to twice; not its bounds) |
| `bScaleRadioVolume` | done |  | 2 | Radio Songs Volume / Radio Talk Volume (two sliders) |
| `bSilentSneakPowerAttacks` | done |  |  | already so: the player makes no sound of effort on any attack |
| `bSplashDamageTorsoGibbing` | done |  |  | Splash Kills Can Gib the Torso (one in four with Bloody Mess, ours) |
| `bTogglePOVWhenHolsteringWeapon` | done |  |  | View Follows the Weapon |
| `bUseConsoleOutputFile` | done |  | 1 | Console Output File (ConsoleOutput.txt in NVForever's folder; the name is ours, sFilename not offered) |
| `bAllowKeyboardAndMouseWithControllerConnected` | to do | yes |  | the Input Device setting picks one device; needs both at once |
| `bArmorPreventsBloodDecals` | to do | yes |  | the impact is sent before DT is worked out |
| `bConsoleBackground` | unsure, skipped | yes |  | needs the plugin's own menu XML |
| `bCustomScreenshotFormat` | n/a | yes | 1 | NVForever writes its own screenshots |
| `bDontAllowConsoleTillLoadingIsDone` | n/a | yes |  | the original's start-up order |
| `bHeatbeatSoundsFade` | to do | yes | 2 | NVForever plays no heartbeat yet |
| `bImprovedWeather` | to do | yes | 1 | the renderer does not know a fast travel happened |
| `bMultithreadedHackingMenu` | n/a | yes |  | performance work on the original |
| `bNoDespawnVisibleStuckProjectiles` | to do | yes |  | projectiles do not stick in NVForever yet |
| `bPartialReloads` | unsure, skipped | yes |  | depends on animation file naming conventions not yet checked |
| `bUseAnimVariants` | unsure, skipped | yes | 3 | depends on animation file naming conventions not yet checked |
| `bVATSStopFollowingDeadTargets` | to do | yes |  | needs the kill camera shot's start frame kept |
| `bWeaponImpactEffects` | to do | yes |  | hits on the hand weapon use the flesh impact; holstered weapons have no hit part |
| `bAlternateLevelupSounds` | to do |  | 4 | xp level_up sound; the sound cache takes static names |
| `bAnimDebugging` | n/a |  |  | a mod-author debug print |
| `bAutoReadNotes` | unsure, skipped |  | 3 | needs another mod (Book Menu Restored) |
| `bContinueGameHotkey` | n/a |  |  | a mod-author debug key |
| `bCustomLocationDiscoveredSound` | to do |  | 1 | quest_messages sound; the cache takes static names |
| `bDialogMenuDebugging` | n/a |  |  | a mod-author debug print |
| `bDisableAchievements` | n/a |  |  | NVForever has no achievements |
| `bDisableCharacterRespec` | to do |  |  | the respec is plugin script data |
| `bDisableRegionBordersKeepMessage` | to do |  |  | not built: no world borders |
| `bInvalidBSFadeNodePrints` | n/a |  |  | the plugin's own warning |
| `bNoPipboyOnAltTab` | n/a |  |  | the original's alt-tab handling |
| `bPrintErrorsToConsole` | n/a |  | 5 | the original's error log |
| `bPrintNewModsOnLoad` | n/a |  |  | the plugin's own load report |
| `bRadioKeepPositionWhenLoading` | to do |  |  | the radio position is not saved |
| `bReloadSoundsAffectedByTimescale` | to do |  |  | one global sound speed |
| `bReplaceHelpWithConsole` | to do |  |  | no Help entry; the console opens only from play, so a pause-menu entry needs a state change |
| `bSwapKeyboardYZKeys` | n/a |  |  | the original's keyboard handling; NVForever's keys can be rebound |
| `bTerminalFadeTime` | to do |  | 1 | not built: terminals do not fade |
| `bUKKeyboard` | n/a |  |  | the original's keyboard handling |
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
