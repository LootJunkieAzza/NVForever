//! The settings page's Tweaks tab (docs/design/tweaks.md): optional changes to how the
//! game plays or what it shows, each off (or at "no change") unless the player sets it.
//! The list follows the features of Stewie's Tweaks, a script-extender plugin for the
//! original, each written here from what the plugin's readme says it does; neither its
//! code nor its files are used, and the labels and notes are NVForever's own words. A
//! row's note names the plugin's INI key it stands for, so a player who knows the plugin
//! can find theirs. Rows are kept in `NVForever.ini` `[Settings]` as every row is
//! (`crate::settings`), under keys starting `tweak-`.
//!
//! The values live in [`TweakSettings`]: the game's rules' part (`fnv_game::tweaks`) is
//! handed to the running game each frame ([`sync`], as the difficulty is: the game is
//! made after the settings are read, and again for a loaded save); the engine's own
//! modules read the rest from the resource.

use crate::script::ScriptState;
use crate::settings::{add_setting, choice_index, choice_part, Setting, Slider};
use bevy::prelude::*;
use fnv_game::tweaks::Tweaks;

/// The settings page's group (its tab) for the tweaks.
pub const GROUP: &str = "Tweaks";

/// Turn Further While Seated, for the player's body (the nvforever crate), whose systems
/// have no room for another parameter [design: docs/design/tweaks.md].
pub static SEATED_WIDE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// No Turning Animation, for the player's body [design: docs/design/tweaks.md].
pub static NO_TURN_CLIP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Remove Landing Animation, for the player's body [design: docs/design/tweaks.md].
pub static NO_LAND_CLIP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Activating Doesn't Stand Up: on furniture only a movement control gets the player up
/// [design: docs/design/tweaks.md].
pub static SEAT_KEEPS_ON_ACTIVATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Every tweak's value. `Default`: the original's behaviour.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TweakSettings {
    /// The part the game's rules read (`fnv_game::Game::tweaks`).
    pub game: Tweaks,
    /// `bNoKarmaMessages`: karma changes put up no corner message and play no sound.
    pub no_karma_messages: bool,
    /// `bNoXPMessages`: experience gains do not bring up the XP meter or its sound.
    pub no_xp_messages: bool,
    /// `bNoQuestFailedPopup`: no "Quest failed" banner.
    pub no_quest_failed_popup: bool,
    /// `bNoLocationPopup`: no banner when a place is discovered.
    pub no_location_popup: bool,
    /// `bRemoveQuestObjectiveAddedText`: no list of new objectives on the HUD.
    pub no_objective_popup: bool,
    /// `bDisableFastTravel`: fast travel is always refused.
    pub disable_fast_travel: bool,
    /// `bMidairFastTravel`: fast travel is not refused for being in the air.
    pub midair_fast_travel: bool,
    /// `bFastTravelWithEnemiesNearby`: fast travel is not refused for enemies nearby.
    pub fast_travel_enemies_nearby: bool,
    /// `bFastTravelOverencumbered`: fast travel is not refused for carrying too much.
    pub fast_travel_overencumbered: bool,
    /// `bNoRedCrosshairOnEnemies`: the crosshair does not turn red over a hostile actor.
    pub no_red_crosshair: bool,
    /// `bHideEnemyMarkers`: hostile actors are not shown on the compass.
    pub hide_enemy_markers: bool,
    /// `bDisableCombatMusic`: no battle music.
    pub no_combat_music: bool,
    /// `bDisableLoadingScreenTips`: loading screens show no tip.
    pub no_loading_tips: bool,
    /// `bNoReputationMessages`: reputation changes put up no box and play no sound.
    pub no_reputation_messages: bool,
    /// `bDisableExplosionInFaceIMOD`: a nearby explosion does not blur the screen.
    pub no_explosion_blur: bool,
    /// `bRemoveSneakLabel`: no [HIDDEN] / [DETECTED] label while sneaking.
    pub no_sneak_label: bool,
    /// `bVatsExitKey`: a right click ends a kill camera.
    pub kill_cam_exit_key: bool,
    /// `bNumberedComputerHotkeys`: 1 to 9 and 0 choose a terminal's rows.
    pub numbered_terminal_keys: bool,
    /// `bDeselectHotkeys`: assigning an item to the hotkey it has takes it off.
    pub deselect_hotkeys: bool,
    /// `bScaleMusicVolumeDuringDialogue`/`fDialogueMusicVolumeMult`: the music's volume
    /// in a conversation, as a part of its own; 1 changes nothing.
    pub dialogue_music_volume: f32,
    /// `bScaleRadioSongVolumeDuringDialogue`/`fDialogueSongVolumeMult`: the radio's.
    pub dialogue_radio_volume: f32,
    /// `bNonSelectablePlayerInConsole`: a console click never picks the player.
    pub console_skips_player: bool,
    /// `bPipboyLightAndHolsteringIgnoreTimescale`: holding Ready Item to holster counts
    /// real time (the Pip-Boy light already does).
    pub holster_real_time: bool,
    /// `bPowerArmorPreventsScreenBlood`: no blood on the screen in power armour.
    pub power_armor_no_screen_blood: bool,
    /// `bHideRedCrosshairOnDistantInvisibleTargets`: an invisible foe does not turn the
    /// crosshair red.
    pub no_red_crosshair_on_cloaked: bool,
    /// `bGodmodePreventsLegCrippleSound`: no hard-landing sound for the player in god mode.
    pub god_mode_quiet_landing: bool,
    /// `bCompressDuplicateConsoleMessages`: a console line said again right after
    /// itself is counted on the same line.
    pub console_compress_repeats: bool,
    /// `bConsolePrintsIncludeTimestamp`: console lines start with the time since the
    /// game started.
    pub console_timestamps: bool,
    /// `bLivingAnatomyShowDR`: Living Anatomy's line shows the DR too.
    pub living_anatomy_dr: bool,
    /// `bSynchronizeContainerCategories`: a container's or barter's two lists change
    /// category together.
    pub sync_categories: bool,
    /// `bNoFastTravelTimeChange`: fast travel takes no time.
    pub no_travel_time: bool,
    /// `bPowerArmorScalesFallDamage`/`fFallDamageMult`: fall damage to anyone in power
    /// armour is multiplied by this; 1 changes nothing.
    pub power_armor_fall_damage: f32,
    /// `bNoFastTravelIfLegsCrippled`: fast travel is refused with a crippled leg.
    pub no_travel_crippled_leg: bool,
    /// `bJumpingCostsAP`/`iJumpAPCost`: Action Points a jump costs; 0 changes nothing.
    pub jump_ap_cost: f32,
    /// `bRunningCostsAP`: Action Points running costs a second; 0 changes nothing.
    pub run_ap_per_second: f32,
    /// `bDisableSellingItems`: the player cannot sell in a barter.
    pub no_selling: bool,
    /// The Own Skill Points Rule's numbers, kept while the rule is off.
    pub skill_point_base: i32,
    pub skill_points_per_int: f64,
    /// `bAimingSpeed`: aiming or blocking does not slow the player to a walk.
    pub no_aiming_slowdown: bool,
    /// `bRepeatJumping`: holding Jump jumps again on landing.
    pub repeat_jumping: bool,
    /// `bHoldCrouchToSneak`: sneak while the key is held.
    pub hold_to_sneak: bool,
    /// `bChargedAttacksCostAP`/`iChargedAttackAPCost`: Action Points a power attack
    /// costs; 0 changes nothing.
    pub power_attack_ap: f32,
    /// `bDistanceBasedQuestMarkerVisibilty`: quest targets farther than this are left off
    /// the compass; 0 changes nothing.
    pub quest_marker_reach: f32,
    /// `bUseCustomSniperZoomRate`/`fScopeFOVTimeChange`: a scope's zoom time as a part of
    /// the usual; 1 changes nothing.
    pub scope_zoom_time: f32,
    /// `bAddInventoryDropItemHotkey`: Q drops the selected Pip-Boy item.
    pub drop_key: bool,
    /// `bAimingUnholstersWeapons`: aiming draws a holstered weapon.
    pub aiming_draws_weapon: bool,
    /// `bBillyGoatMode`/`fWalkAngle`: the steepest slope the player walks up, degrees.
    pub walk_angle: f32,
    /// `bCrippledLegsScaleJumpHeight`: the jump height is multiplied by this for each
    /// crippled leg; 1 changes nothing.
    pub crippled_leg_jump: f32,
    /// `bBarterCheckActorBuySellFlags`: a merchant takes from the player only what its
    /// list says it buys.
    pub vendor_lists_both: bool,
    /// `bNoPlayerStaggerAnims`: the player never staggers from a hit.
    pub no_player_stagger: bool,
    /// `bPreventReloadWeaponUnholstering`: Ready Item does not draw a holstered weapon.
    pub ready_does_not_draw: bool,
    /// `bReloadingPreventsWeaponHotkeys`: weapon hotkeys do nothing while reloading.
    pub no_hotkeys_while_reloading: bool,
    /// `bRunSlowerInWater`/`fWadingMovementMult`: the pace at a swimming depth of water,
    /// as a part of the usual (shallower, less slowed); 1 changes nothing.
    pub wading_pace: f32,
    /// `bMinBloodSplatterHealthDamage`/`fMinHealthDamage`: hits that take less health
    /// put no blood on the screen; 0 changes nothing.
    pub min_splatter_damage: f32,
    /// `bMinigamesPlayXPSoundAtMaxLevel`: the experience sound plays at the level cap.
    pub xp_sound_at_cap: bool,
    /// `bHoldWaitKeyToShowMenu`: the Wait key is held to open the menu.
    pub hold_to_wait: bool,
    /// `bDisableWeaponFOV`: aiming down iron sights does not zoom.
    pub no_sight_zoom: bool,
    /// `bHideMiscQuestItems`: quest items are left out of the Pip-Boy's Misc list.
    pub hide_misc_quest: bool,
    /// `bHotkeyHolstersWeaponIfEquipped`: the hotkey of the weapon in hand draws or
    /// holsters it.
    pub hotkey_toggles_in_hand: bool,
    /// `bDisableInteriorFog`: interior fog is pushed out of sight (from the next cell).
    pub no_interior_fog: bool,
    /// `bNoCompassPipsIfNotInDanger`: actors are on the compass only in danger.
    pub compass_pips_in_danger_only: bool,
    /// `bHostilesUseNeutralColorOnCompass`: foes on the compass in the usual colour.
    pub neutral_foes_on_compass: bool,
    /// `bShowSneakLabelWhileStanding`: the sneak label shows while standing too.
    pub sneak_label_standing: bool,
    /// `bNoXPBarInCombat`: the XP meter waits for the fight to end.
    pub no_xp_meter_in_combat: bool,
    /// `bDisableHUDCrippledLimbIndicator`: no crippled-limb figure on the HUD.
    pub no_limb_indicator: bool,
    /// `bContainerTakeAllDoesntCloseMenu`: Take All leaves the container open.
    pub take_all_stays: bool,
    /// `bHideEquippedItemsInBarter`: equipped items are not offered in barter.
    pub hide_equipped_barter: bool,
    /// `bHideEquippedItemsInContainers`: equipped items are not listed in containers.
    pub hide_equipped_containers: bool,
    /// `bNoCapitaliseContainerCategories`: category titles in title case.
    pub title_case_categories: bool,
    /// `bDisableInventoryRightClickDrop`: a right click in the inventory does not drop.
    pub no_right_click_drop: bool,
    /// `bPreventNoFastTravelMessage`: an undiscovered marker refuses travel quietly.
    pub quiet_undiscovered: bool,
    /// The crosshair is hidden by its key (`Control::ToggleCrosshair`; not kept).
    pub crosshair_hidden: bool,
    /// `bTurnSlowerWhileAiming`/`fAimingScaleX`/`fAimingScaleY`: look speed while
    /// aiming, as a part of the usual, across and up and down; 1 changes nothing.
    pub aim_turn_x: f32,
    pub aim_turn_y: f32,
    /// `bScaleRadioVolume`/`fMusicScale`/`fDialogueScale`: the radio's songs and its
    /// talk, each as a part of their volume; 1 changes nothing.
    pub radio_music_volume: f32,
    pub radio_talk_volume: f32,
    /// `bOnlyChangeCameraHeightIfPOVKeyHeld`: the wheel moves the third-person camera
    /// only with Toggle View held.
    pub camera_wheel_needs_pov: bool,
    /// `bNoHotkeyEquipSounds`: weapon hotkeys make no equip sound.
    pub quiet_hotkeys: bool,
    /// `bDisableWeaponHotkeys`: the number keys and the D-pad pick no weapon.
    pub no_weapon_hotkeys: bool,
    /// `bOpenPipboyToInventoryByDefault`: the Pip-Boy opens on the inventory.
    pub pipboy_opens_items: bool,
    /// `bHideCrosshairWhileReloading`: no crosshair while reloading.
    pub hide_crosshair_reloading: bool,
    /// `bInvertCameraX`: looking left and right the other way.
    pub invert_x: bool,
    /// `bShowNearestUndiscoveredLocation`: the nearest place not yet found shows on the
    /// compass at any distance.
    pub nearest_unfound_on_compass: bool,
    /// `bLocationDiscoveredCornerMessage`: a place found is a corner line, not a banner.
    pub discovered_in_corner: bool,
    /// `bInvertContainerTitleScrollwheelDirection`: the wheel over a category title
    /// steps the other way.
    pub invert_title_wheel: bool,
    /// `bAutoContinueGame`: the main menu continues the newest save at once.
    pub auto_continue: bool,
    /// `bHideAmmoLabel`: no ammo counter on the HUD.
    pub hide_ammo_label: bool,
    /// `bNoFreeBarterItems`/`fFreeItemCost`: the price of things worth nothing in a
    /// barter; 0 changes nothing.
    pub free_item_price: f32,
    /// `bAllowDuplicateControlBinds`: one input may serve two controls.
    pub duplicate_binds: bool,
    /// `bTogglePOVWhenHolsteringWeapon`: drawing goes to first person, holstering to third.
    pub pov_follows_weapon: bool,
    /// `bChanceBasedKillcams`/`fChance`: the share of kill cameras that play; 1 changes
    /// nothing.
    pub kill_cam_chance: f32,
    /// `bContainerEncumbranceIndicator`: the weight in red when taking would overload.
    pub warn_encumbrance: bool,
    /// `bHapticPipboy`: a short controller buzz on a Pip-Boy page or tab change.
    pub pipboy_buzz: bool,
    /// `bPowerAttackIfBlocking`: while blocking, a press of Use is a power attack at once.
    pub power_attack_if_blocking: bool,
    /// `bVATSHipFire`: V.A.T.S. entered without aiming fires every shot from the hip.
    pub vats_hip_fire: bool,
    /// `bNoPipBoyInCombat`: the Pip-Boy does not open while something fights the player.
    pub no_pipboy_in_combat: bool,
    /// `bNoEquippedWeaponMovementPenalty`: a drawn two-handed weapon does not slow the run.
    pub no_weapon_move_penalty: bool,
    /// `bUnequipBrokenArmor`: the player's armour comes off when it breaks.
    pub unequip_broken_armor: bool,
    /// `bHideCrosshairInFirstPerson`, `bHideCrosshairInThirdPerson`: no crosshair in that
    /// view; the activation prompt stays.
    pub hide_crosshair_first: bool,
    pub hide_crosshair_third: bool,
    /// `bColoredHUDBars` (`fRedHealthThreshold`, `fRedActionPointsThreshold`): the HP and
    /// AP bars turn red below these shares; 0 never.
    pub red_hp_below: f32,
    pub red_ap_below: f32,
    /// `bColorWeaponCndLabel` (`fColorThreshold`, `fFlashThreshold`): the weapon's
    /// condition bar red below the first share, flashing red below the second; 0 never.
    pub weapon_cnd_red_below: f32,
    pub weapon_cnd_flash_below: f32,
    /// `bBlackJackTotalDisplay`: the Blackjack screen shows the player's hand's total.
    pub blackjack_total: bool,
    /// `bSlotsAutoSpinHotkey`: W held spins the slot machine again each time it stops.
    pub slots_hold_to_spin: bool,
    /// `bTalkWhileSneakingIfShiftIsHeld`: Shift held while sneaking talks to a person
    /// instead of picking their pocket.
    pub shift_talks_sneaking: bool,
    /// `iConsoleKey`: which of [`CONSOLE_KEYS`] opens the console (0: the original's).
    pub console_key: usize,
    /// `bTabClosesTerminals`: Tab closes a terminal from any of its pages.
    pub tab_closes_terminals: bool,
    /// `bTabBackInStartMenu`: Tab goes back in the pause menu where it steps no tabs.
    pub tab_back_in_pause: bool,
    /// `bAllowUnsafeFastTravel`: fast travel from places flagged against it (indoors).
    pub unsafe_fast_travel: bool,
    /// `iDateFormat`: the style of `crate::wait::model::DATE_STYLES`.
    pub date_style: usize,
    /// `bEnteringVATSDoesntUnholsterWeapon`: a weapon V.A.T.S. drew goes away again when
    /// the mode ends with nothing fired.
    pub vats_keeps_holstered: bool,
    /// `bNoHUDHotkeyPopup`: holding a hotkey does not open the weapon wheel.
    pub no_hotkey_wheel: bool,
    /// `bMoveAmmoTypeLabel`: the HUD's ammo label under the ammo count.
    pub ammo_label_under: bool,
    /// `bCompanionPipColorChange`: which of [`COMPANION_COLOURS`] companions' compass
    /// pips take (0: as anyone's).
    pub companion_pip: usize,
    /// `bRaceMenuAllowFemaleFacialHair`: the character menus offer facial hair to women.
    pub female_facial_hair: bool,
    /// `bContainerMenuStoreAllHotkey`: Shift with Take All puts everything listed on the
    /// player's side in the container.
    pub store_all: bool,
    /// `bStartMenuQuickLoad`: the QuickLoad key works in the main and pause menus.
    pub quick_load_in_menus: bool,
    /// `bPickLocksEvenWithKey`: with Shift held a lock the player has the key of is
    /// picked instead.
    pub pick_locks_with_key: bool,
    /// `iToggleSmoothCameraKey`: the look eases after the input (the Smooth Camera key
    /// flips it; not kept in the INI).
    pub smooth_look: bool,
    /// `bRightClickChangesToPreviousContainerCategory`: a right click on a container's
    /// or a barter's list heading steps back a category.
    pub right_click_previous_category: bool,
    /// `bShowBarterTotalWhenOverSellLimit`: the barter figure shows the whole total when
    /// it is more than the merchant's caps.
    pub full_barter_total: bool,
    /// `bCompassFadeLeftSide`: compass marks fade toward the left end.
    pub compass_fade_left: bool,
    /// `bDisableControllerDeadzones` (`iControllerDeadzone`, `iControllerDeadzoneRS`): the
    /// sticks' own dead zones, as shares of a full push (0 none).
    pub custom_dead_zones: bool,
    pub left_dead_zone: f32,
    pub right_dead_zone: f32,
    /// `bControllerTriggerDeadzones` (`iDeadzoneLT`, `iDeadzoneRT`): how far each trigger
    /// goes down before it counts as pressed.
    pub custom_trigger_dead_zones: bool,
    pub lt_dead_zone: f32,
    pub rt_dead_zone: f32,
    /// `bMoreResponsiveControllerAiming`: the right stick turns in a straight line with
    /// the push past the dead zone.
    pub responsive_stick: bool,
    /// `bFasterControllerPOVRotate`: a controller circles the camera (Toggle POV held)
    /// twice as fast.
    pub faster_pad_orbit: bool,
    /// `bSeparateHorizontalSensitivity`: a factor on the look's left and right speed only.
    pub horizontal_look: f64,
    /// `bRememberConsoleHistory`: the console's history is kept in NVForever's folder.
    pub remember_console_history: bool,
    /// `bUseConsoleOutputFile`: console lines are added to a file in NVForever's folder.
    pub console_output_file: bool,
    /// `bConsoleTextShadow`: the console's scrollback darker, over a faint shadow.
    pub console_text_shadow: bool,
    /// `bClickToExitLoadScreens`: a loading screen waits for a press once all is in.
    pub click_to_exit_loading: bool,
    /// `bBarterShowCapsChange`: a barter shows each side's caps after the trade too.
    pub barter_caps_change: bool,
    /// `bJumpSwimsUpwards`: holding Jump under water swims up.
    pub jump_swims_up: bool,
    /// `bFirstPersonVATS`: V.A.T.S.'s firing plays in first person.
    pub first_person_vats: bool,
    /// `bKeepCrosshairWhenAiming`: the crosshair stays down the iron sights.
    pub crosshair_when_aiming: bool,
    /// `bMoveDuringVATSPlayback`: walking and running stay the player's while
    /// V.A.T.S.'s camera plays.
    pub move_during_kill_cam: bool,
    /// `bBetterPickupPrompt`: a stack's prompt gives the whole stack's weight and value.
    pub stack_figures: bool,
    /// `bTurnFurtherWhileSeated`: seated in first person the view turns 115 degrees to
    /// either side, not 80.
    pub seated_wide: bool,
    /// `bNoTurningAnim`: the player's body plays no turning-in-place clip.
    pub no_turn_clip: bool,
    /// `bRemoveLandingAnim`: the player's body plays no landing clip.
    pub no_land_clip: bool,
    /// `bHideInvisibleUndiscoveredLocations`: a place not found yet is on the compass only
    /// when the map shows it.
    pub hide_unmapped_places: bool,
    /// `bSkipVigorTesterSpecialPages`: the S.P.E.C.I.A.L. machine opens on its review
    /// page.
    pub vigor_review_first: bool,
    /// `bScopeVisibilityDelay`: milliseconds a scope waits after the zoom is full before
    /// it shows; 0 none.
    pub scope_delay_ms: f32,
    /// `bHideCursorInMessageMenu`: no mouse pointer over a message box.
    pub no_pointer_in_message_box: bool,
    /// `bHideCursorInDialog`: no mouse pointer over a conversation's choices.
    pub no_pointer_in_dialogue: bool,
    /// `bLevelUpScrollWheelSupport`: the wheel raises and lowers skills on the level-up
    /// page.
    pub levelup_wheel: bool,
    /// `bActivatingDoesntStandUp`: Activate does not get the player up from furniture;
    /// moving does.
    pub seat_keeps_on_activate: bool,
    /// `bNoExteriorLoadScreens`: leaving an interior shows a black screen with no
    /// picture or tip (not a loaded save).
    pub plain_going_out: bool,
    /// `bHideHotkeyedItemsInContainers`: items on a hotkey are not listed on the player's
    /// side of a container.
    pub hide_hotkeyed_containers: bool,
    /// `bHideHotkeyedItemsInBarter`: items on a hotkey are not offered in barter.
    pub hide_hotkeyed_barter: bool,
    /// `bHideHotkeyedItemsInRepairLists`: items on a hotkey are not offered as repair
    /// parts in the Pip-Boy.
    pub hide_hotkeyed_repair: bool,
    /// `bAddSetHomeCommand`: the console's `sethome`, `home`, `back` and `delhome`.
    pub console_homes: bool,
    /// `iScreenshotKey`: which of [`SCREENSHOT_KEYS`] takes a screenshot.
    pub screenshot_key: usize,
    /// `bShiftScreenshotHidesMenus`: with Shift held, a screenshot leaves out the HUD.
    pub shift_screenshot_hides: bool,
    /// `bExtraConsoleDetails`/`bMeshPath`: a selected reference's console details give
    /// its model.
    pub console_model_details: bool,
    /// `bMovementPenalties`: the pace walking or running backwards and sideways, as a
    /// part of the usual; 1 changes nothing.
    pub back_pace: f32,
    pub strafe_pace: f32,
}

/// The colours a companion's compass pip may take, ours; the first leaves it as anyone's.
pub const COMPANION_COLOURS: [(&str, [f32; 3]); 6] = [("Off", [0.0; 3]), ("Green", [0.35, 1.0, 0.4]), ("Blue", [0.4, 0.6, 1.0]), ("Cyan", [0.3, 0.95, 1.0]), ("White", [1.0, 1.0, 1.0]), ("Yellow", [1.0, 0.95, 0.3])];

/// The companion pip colour the tweak picks, if any.
pub fn companion_colour(t: Option<&TweakSettings>) -> Option<Color> {
    let i = t.map_or(0, |t| t.companion_pip).min(COMPANION_COLOURS.len() - 1);
    (i > 0).then(|| {
        let [r, g, b] = COMPANION_COLOURS[i].1;
        Color::srgb(r, g, b)
    })
}

/// The keys the console may open on, the original's first; ones nothing else in the
/// game takes [design: docs/design/tweaks.md].
pub const CONSOLE_KEYS: [(KeyCode, &str); 6] = [(KeyCode::Backquote, "` (default)"), (KeyCode::Insert, "Insert"), (KeyCode::Home, "Home"), (KeyCode::End, "End"), (KeyCode::Delete, "Delete"), (KeyCode::F10, "F10")];

/// The key that opens the console under the iConsoleKey tweak.
pub fn console_key(t: Option<&TweakSettings>) -> KeyCode {
    CONSOLE_KEYS[t.map_or(0, |t| t.console_key).min(CONSOLE_KEYS.len() - 1)].0
}

/// The keys a screenshot may be taken with, NVForever's own first; ones nothing else in
/// the game takes [design: docs/design/tweaks.md].
pub const SCREENSHOT_KEYS: [(KeyCode, &str); 5] = [(KeyCode::F12, "F12 (default)"), (KeyCode::PrintScreen, "Print Screen"), (KeyCode::F11, "F11"), (KeyCode::ScrollLock, "Scroll Lock"), (KeyCode::Pause, "Pause")];

/// The key that takes a screenshot under the iScreenshotKey tweak.
pub fn screenshot_key(t: Option<&TweakSettings>) -> KeyCode {
    SCREENSHOT_KEYS[t.map_or(0, |t| t.screenshot_key).min(SCREENSHOT_KEYS.len() - 1)].0
}

/// The walk-angle choices: 30 to 80 degrees by 5; 45 is the physics' own.
const ANGLES: usize = 11;
fn walk_angle(i: usize) -> f32 {
    30.0 + 5.0 * i as f32
}

/// The Own Skill Points Rule's numbers until the player sets them: the game's rule
/// without its odd-level point (10 + Intelligence / 2) [design].
const SKILL_POINTS_START: (i32, f64) = (10, 0.5);
const SKILL_BASES: usize = 31;

impl TweakSettings {
    /// Hand the skill-point numbers to the rule while it is on.
    fn apply_skill_points(&mut self) {
        if self.game.skill_points.is_some() {
            self.game.skill_points = Some((self.skill_point_base, self.skill_points_per_int));
        }
    }
}

/// Whether `actor` wears power armour on the upper body.
pub fn wears_power_armor(world: &fnv_sim::World, actor: fnv_sim::FormId) -> bool {
    use fnv_sim::forms::ItemKind;
    world.equipment(actor).unwrap_or_default().into_iter().filter_map(|i| world.forms.item(i)).any(|it| it.kind == ItemKind::Armor && it.power_armor && it.slots.is_some_and(|s| s & fnv_game::detection::UPPER_BODY != 0))
}

/// The perks-per-level choices (1 to 5) and the S.P.E.C.I.A.L. totals (30 to 70 by 2).
const PERKS: usize = 5;
const SPECIAL_LOW: i32 = 30;
const SPECIAL_STEP: i32 = 2;
const SPECIALS: usize = 21;

impl Default for TweakSettings {
    fn default() -> TweakSettings {
        TweakSettings {
            game: Tweaks::default(),
            no_karma_messages: false,
            no_xp_messages: false,
            no_quest_failed_popup: false,
            no_location_popup: false,
            no_objective_popup: false,
            disable_fast_travel: false,
            midair_fast_travel: false,
            fast_travel_enemies_nearby: false,
            fast_travel_overencumbered: false,
            no_red_crosshair: false,
            hide_enemy_markers: false,
            no_combat_music: false,
            no_loading_tips: false,
            no_reputation_messages: false,
            no_explosion_blur: false,
            no_sneak_label: false,
            kill_cam_exit_key: false,
            numbered_terminal_keys: false,
            deselect_hotkeys: false,
            dialogue_music_volume: 1.0,
            dialogue_radio_volume: 1.0,
            console_skips_player: false,
            holster_real_time: false,
            power_armor_no_screen_blood: false,
            no_red_crosshair_on_cloaked: false,
            god_mode_quiet_landing: false,
            console_compress_repeats: false,
            console_timestamps: false,
            living_anatomy_dr: false,
            sync_categories: false,
            no_travel_time: false,
            power_armor_fall_damage: 1.0,
            no_travel_crippled_leg: false,
            jump_ap_cost: 0.0,
            run_ap_per_second: 0.0,
            no_selling: false,
            skill_point_base: SKILL_POINTS_START.0,
            skill_points_per_int: SKILL_POINTS_START.1,
            no_aiming_slowdown: false,
            repeat_jumping: false,
            hold_to_sneak: false,
            power_attack_ap: 0.0,
            quest_marker_reach: 0.0,
            scope_zoom_time: 1.0,
            drop_key: false,
            aiming_draws_weapon: false,
            walk_angle: fnv_phys::CharacterParams::default().max_slope_deg,
            crippled_leg_jump: 1.0,
            vendor_lists_both: false,
            no_player_stagger: false,
            ready_does_not_draw: false,
            no_hotkeys_while_reloading: false,
            wading_pace: 1.0,
            min_splatter_damage: 0.0,
            xp_sound_at_cap: false,
            hold_to_wait: false,
            no_sight_zoom: false,
            hide_misc_quest: false,
            hotkey_toggles_in_hand: false,
            no_interior_fog: false,
            compass_pips_in_danger_only: false,
            neutral_foes_on_compass: false,
            sneak_label_standing: false,
            no_xp_meter_in_combat: false,
            no_limb_indicator: false,
            take_all_stays: false,
            hide_equipped_barter: false,
            hide_equipped_containers: false,
            title_case_categories: false,
            no_right_click_drop: false,
            quiet_undiscovered: false,
            crosshair_hidden: false,
            aim_turn_x: 1.0,
            aim_turn_y: 1.0,
            radio_music_volume: 1.0,
            radio_talk_volume: 1.0,
            camera_wheel_needs_pov: false,
            quiet_hotkeys: false,
            no_weapon_hotkeys: false,
            pipboy_opens_items: false,
            hide_crosshair_reloading: false,
            invert_x: false,
            nearest_unfound_on_compass: false,
            discovered_in_corner: false,
            invert_title_wheel: false,
            auto_continue: false,
            hide_ammo_label: false,
            free_item_price: 0.0,
            duplicate_binds: false,
            pov_follows_weapon: false,
            kill_cam_chance: 1.0,
            warn_encumbrance: false,
            pipboy_buzz: false,
            power_attack_if_blocking: false,
            vats_hip_fire: false,
            no_pipboy_in_combat: false,
            no_weapon_move_penalty: false,
            unequip_broken_armor: false,
            hide_crosshair_first: false,
            hide_crosshair_third: false,
            red_hp_below: 0.0,
            red_ap_below: 0.0,
            weapon_cnd_red_below: 0.0,
            weapon_cnd_flash_below: 0.0,
            blackjack_total: false,
            slots_hold_to_spin: false,
            shift_talks_sneaking: false,
            console_key: 0,
            tab_closes_terminals: false,
            tab_back_in_pause: false,
            unsafe_fast_travel: false,
            date_style: 0,
            vats_keeps_holstered: false,
            no_hotkey_wheel: false,
            ammo_label_under: false,
            companion_pip: 0,
            female_facial_hair: false,
            store_all: false,
            quick_load_in_menus: false,
            pick_locks_with_key: false,
            smooth_look: false,
            right_click_previous_category: false,
            full_barter_total: false,
            compass_fade_left: false,
            custom_dead_zones: false,
            left_dead_zone: 0.0,
            right_dead_zone: 0.0,
            custom_trigger_dead_zones: false,
            lt_dead_zone: 0.75,
            rt_dead_zone: 0.75,
            responsive_stick: false,
            faster_pad_orbit: false,
            horizontal_look: 1.0,
            remember_console_history: false,
            console_output_file: false,
            console_text_shadow: false,
            click_to_exit_loading: false,
            barter_caps_change: false,
            jump_swims_up: false,
            first_person_vats: false,
            crosshair_when_aiming: false,
            move_during_kill_cam: false,
            stack_figures: false,
            seated_wide: false,
            no_turn_clip: false,
            no_land_clip: false,
            hide_unmapped_places: false,
            vigor_review_first: false,
            scope_delay_ms: 0.0,
            no_pointer_in_message_box: false,
            no_pointer_in_dialogue: false,
            levelup_wheel: false,
            seat_keeps_on_activate: false,
            plain_going_out: false,
            hide_hotkeyed_containers: false,
            hide_hotkeyed_barter: false,
            hide_hotkeyed_repair: false,
            console_homes: false,
            screenshot_key: 0,
            shift_screenshot_hides: false,
            console_model_details: false,
            back_pace: 1.0,
            strafe_pace: 1.0,
        }
    }
}

/// The value of a tweak, for modules that only have the world.
pub fn tweaks(world: &World) -> TweakSettings {
    world.get_resource::<TweakSettings>().cloned().unwrap_or_default()
}

/// An on/off row of the Tweaks tab for the field `$($f).+` of [`TweakSettings`].
macro_rules! switch {
    ($app:expr, $key:literal, $label:literal, $note:literal, $($f:ident).+) => {
        add_setting(
            $app,
            Setting {
                key: concat!("tweak-", $key),
                group: GROUP,
                label: $label,
                note: $note,
                get: |w| w.get_resource::<TweakSettings>().is_some_and(|t| t.$($f).+),
                set: |w, on| {
                    if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                        t.$($f).+ = on;
                    }
                },
                slider: None,
            },
        )
    };
}

/// The S.P.E.C.I.A.L. total of choice `i`, and back.
fn special_total(i: usize) -> i32 {
    SPECIAL_LOW + SPECIAL_STEP * i as i32
}

fn special_index(total: i32) -> usize {
    (((total - SPECIAL_LOW) / SPECIAL_STEP).max(0) as usize).min(SPECIALS - 1)
}

/// A multiplier as a slider: the part `v` is the factor `2v` (0 to 2 in tenths, 1 in the
/// middle).
fn factor(v: f32) -> f64 {
    f64::from(v) * 2.0
}

fn part(f: f64) -> f32 {
    (f / 2.0).clamp(0.0, 1.0) as f32
}

fn shown_factor(v: f32) -> String {
    format!("x{:.1}", factor(v))
}

/// An on/off row for one `WEIGHTLESS_*` bit of the game's `weightless_kinds`.
macro_rules! weightless {
    ($app:expr, $key:literal, $label:literal, $note:literal, $bit:expr) => {
        add_setting(
            $app,
            Setting {
                key: concat!("tweak-", $key),
                group: GROUP,
                label: $label,
                note: $note,
                get: |w| w.get_resource::<TweakSettings>().is_some_and(|t| t.game.weightless_kinds & $bit != 0),
                set: |w, on| {
                    if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                        t.game.weightless_kinds = if on { t.game.weightless_kinds | $bit } else { t.game.weightless_kinds & !$bit };
                    }
                },
                slider: None,
            },
        )
    };
}

/// A slider row for an amount from 0 to `$max` in twentieths, shown as a whole number,
/// in the field `$($f).+` (of type `$t`).
macro_rules! amount {
    ($app:expr, $key:literal, $label:literal, $note:literal, $max:expr, $t:ty, $($f:ident).+) => {
        add_setting(
            $app,
            Setting::slider(
                concat!("tweak-", $key),
                GROUP,
                $label,
                $note,
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(0.0, |t| (t.$($f).+ as f32 / $max as f32).clamp(0.0, 1.0)),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.$($f).+ = (v * $max as f32).round() as $t;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| format!("{:.0}", v * $max as f32)),
                },
            ),
        )
    };
}

/// A slider row of the Tweaks tab for a part (0 to 100% in fives) in the field
/// `$($f).+` of [`TweakSettings`].
macro_rules! part {
    ($app:expr, $key:literal, $label:literal, $note:literal, $($f:ident).+) => {
        add_setting(
            $app,
            Setting::slider(
                concat!("tweak-", $key),
                GROUP,
                $label,
                $note,
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(1.0, |t| t.$($f).+),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.$($f).+ = v;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: None,
                },
            ),
        )
    };
}

pub struct TweaksPlugin;

impl Plugin for TweaksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TweakSettings>()
            .add_systems(Update, sync.in_set(crate::PlaySet::Input))
            .add_systems(Update, (keys, drop_weapon, radio_volume_keys, last_weapon).in_set(crate::PlaySet::Input))
            .add_systems(PreUpdate, trigger_dead_zones.before(bevy::input::InputSystems))
            .add_systems(Update, ap_costs.after(crate::PlaySet::Input).after(crate::player::encumbered_walks).before(crate::PlaySet::Physics));
        // ---- experience ----
        add_setting(
            app,
            Setting::slider(
                "tweak-xp-multiplier",
                GROUP,
                "Experience Earned",
                "Every experience gain is multiplied by this, after the perks; x1.0 changes nothing. (Stewie's Tweaks bReduceXP)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| t.game.xp_multiplier)),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.xp_multiplier = factor(v);
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        switch!(app, "no-xp-messages", "Quiet Experience", "Experience gains do not bring up the XP meter or its sound; levelling up still does. (Stewie's Tweaks bNoXPMessages)", no_xp_messages);
        // ---- messages ----
        switch!(app, "no-karma-messages", "Quiet Karma", "Karma changes show no corner message and play no sound. (Stewie's Tweaks bNoKarmaMessages)", no_karma_messages);
        switch!(app, "no-quest-failed", "No Quest Failed Banner", "A failed quest puts up no banner; the Pip-Boy still lists it. (Stewie's Tweaks bNoQuestFailedPopup)", no_quest_failed_popup);
        switch!(app, "no-location-popup", "No Location Banner", "Discovering a place puts up no banner; it is still marked on the map. (Stewie's Tweaks bNoLocationPopup)", no_location_popup);
        switch!(app, "no-objective-popup", "No Objective List", "New and completed quest objectives are not listed on the HUD. (Stewie's Tweaks bRemoveQuestObjectiveAddedText)", no_objective_popup);
        // ---- fast travel ----
        switch!(app, "disable-fast-travel", "No Fast Travel", "Fast travel is always refused. (Stewie's Tweaks bDisableFastTravel)", disable_fast_travel);
        switch!(app, "midair-fast-travel", "Fast Travel in the Air", "Being in the air does not stop fast travel. (Stewie's Tweaks bMidairFastTravel)", midair_fast_travel);
        switch!(app, "fast-travel-enemies", "Fast Travel Near Enemies", "Enemies nearby do not stop fast travel. (Stewie's Tweaks bFastTravelWithEnemiesNearby)", fast_travel_enemies_nearby);
        switch!(app, "fast-travel-encumbered", "Fast Travel Encumbered", "Carrying too much does not stop fast travel, as with Long Haul. (Stewie's Tweaks bFastTravelOverencumbered)", fast_travel_overencumbered);
        // ---- HUD ----
        switch!(app, "no-red-crosshair", "No Red Crosshair", "The crosshair keeps its colour over hostile actors. (Stewie's Tweaks bNoRedCrosshairOnEnemies)", no_red_crosshair);
        switch!(app, "hide-enemy-markers", "No Enemies on the Compass", "Hostile actors are left off the compass. (Stewie's Tweaks bHideEnemyMarkers)", hide_enemy_markers);
        switch!(app, "no-sneak-label", "No Sneak Label", "Sneaking shows no Hidden / Detected label on the HUD. (Stewie's Tweaks bRemoveSneakLabel)", no_sneak_label);
        switch!(app, "no-reputation-messages", "Quiet Reputation", "Reputation changes put up no box and play no sound; the Pip-Boy still shows them. (Stewie's Tweaks bNoReputationMessages)", no_reputation_messages);
        // ---- sound and screen ----
        switch!(app, "no-combat-music", "No Battle Music", "Fights do not switch the music to battle tracks. (Stewie's Tweaks bDisableCombatMusic)", no_combat_music);
        switch!(app, "no-loading-tips", "No Loading Tips", "Loading screens show their picture without the tip. (Stewie's Tweaks bDisableLoadingScreenTips)", no_loading_tips);
        switch!(app, "no-explosion-blur", "No Explosion Blur", "A blast close by does not blur the screen. (Stewie's Tweaks bDisableExplosionInFaceIMOD)", no_explosion_blur);
        switch!(app, "no-chem-wore-off", "No Chem Wore-Off Effect", "A chem wearing off plays no screen effect. (Stewie's Tweaks bRemoveChemWarnOffIMOD)", game.no_chem_wore_off_effect);
        // ---- VATS ----
        switch!(app, "kill-cam-exit-key", "Skip Kill Camera", "A right click ends a kill camera at once. (Stewie's Tweaks bVatsExitKey)", kill_cam_exit_key);
        // ---- menus ----
        switch!(app, "skip-skill-page", "Perks First When No Points", "A level-up with no skill points to spend opens on the perks. (Stewie's Tweaks bSkipSkillMenuIfNoPointsToAssign)", game.skip_skill_page_without_points);
        switch!(app, "no-empty-book-message", "No Message for Spent Books", "A skill book that gives no points (the skill at its top) shows no message. (Stewie's Tweaks bNoSkillMessageIfIncreaseIsZero)", game.no_book_message_without_gain);
        switch!(app, "recipe-caps", "Recipe Categories in Capitals", "The crafting menu's categories are shown in capitals. (Stewie's Tweaks bCapitalizeRecipeCategories)", game.capitalize_recipe_categories);
        switch!(app, "weapon-ammo-use", "Rounds per Shot", "A weapon that uses more than one round a shot says how many beside its ammo in the Pip-Boy. (Stewie's Tweaks bShowWeaponAmmoUseInMenus)", game.show_weapon_ammo_use);
        switch!(app, "book-effects", "Skill Book Effects", "A skill book says which skill it raises and by how much in the Pip-Boy. (Stewie's Tweaks bShowBookEffects)", game.show_book_effects);
        switch!(app, "deselect-hotkeys", "Unassign Hotkeys", "Assigning an item to the hotkey it already has takes it off. (Stewie's Tweaks bDeselectHotkeys)", deselect_hotkeys);
        switch!(app, "map-remembers", "Map Remembers Its Place", "The Pip-Boy maps open where they were left. (Stewie's Tweaks bMapRemembersPosition)", game.remember_map_position);
        switch!(app, "terminal-number-keys", "Number Keys on Terminals", "1 to 9 and 0 choose a terminal's rows. (Stewie's Tweaks bNumberedComputerHotkeys)", numbered_terminal_keys);
        switch!(app, "faster-hacking", "Faster Hacking Exit", "A successful hack opens the terminal after half a second. (Stewie's Tweaks bFasterHackingTransition)", game.faster_hacking_transition);
        // ---- conversations ----
        part!(app, "dialogue-music", "Music While Talking", "The music's volume in a conversation, as a part of its own; 100% changes nothing. (Stewie's Tweaks bScaleMusicVolumeDuringDialogue)", dialogue_music_volume);
        part!(app, "dialogue-radio", "Radio While Talking", "The radio's volume in a conversation, as a part of its own; 100% changes nothing. (Stewie's Tweaks bScaleRadioSongVolumeDuringDialogue)", dialogue_radio_volume);
        // ---- batch 4 ----
        switch!(app, "console-skips-player", "Console Clicks Skip the Player", "Clicking in the console never selects the player. (Stewie's Tweaks bNonSelectablePlayerInConsole)", console_skips_player);
        switch!(app, "holster-real-time", "Holster in Real Time", "Holding Ready Item to holster counts real time, so slowed time does not lengthen it. (Stewie's Tweaks bPipboyLightAndHolsteringIgnoreTimescale)", holster_real_time);
        switch!(app, "power-armor-no-screen-blood", "No Screen Blood in Power Armor", "Hits put no blood on the screen while power armor is worn. (Stewie's Tweaks bPowerArmorPreventsScreenBlood)", power_armor_no_screen_blood);
        switch!(app, "no-red-crosshair-cloaked", "Invisible Foes Stay Hidden", "An invisible foe does not turn the crosshair red. (Stewie's Tweaks bHideRedCrosshairOnDistantInvisibleTargets)", no_red_crosshair_on_cloaked);
        switch!(app, "god-mode-quiet-landing", "Quiet Landings in God Mode", "In god mode the player's hard landings make no pain sound. (Stewie's Tweaks bGodmodePreventsLegCrippleSound)", god_mode_quiet_landing);
        switch!(app, "console-compress-repeats", "Count Repeated Console Lines", "A console line said again right after itself is counted on one line (line -#2). (Stewie's Tweaks bCompressDuplicateConsoleMessages)", console_compress_repeats);
        switch!(app, "console-timestamps", "Console Times", "Console lines start with the time since the game started. (Stewie's Tweaks bConsolePrintsIncludeTimestamp)", console_timestamps);
        switch!(app, "living-anatomy-dr", "Living Anatomy Shows DR", "With Living Anatomy the target's line shows its Damage Resistance as well. (Stewie's Tweaks bLivingAnatomyShowDR)", living_anatomy_dr);
        switch!(app, "sync-categories", "Matching Categories", "In containers and barter both lists change category together. (Stewie's Tweaks bSynchronizeContainerCategories)", sync_categories);
        // ---- batch 6: gameplay ----
        add_setting(
            app,
            Setting::slider(
                "tweak-perks-per-level",
                GROUP,
                "Perks per Level",
                "How many perks a level-up's perk page lets the player take; 1 changes nothing. (Stewie's Tweaks iPerksPerLevel)",
                Slider::choice(
                    PERKS,
                    |w| choice_part(w.get_resource::<TweakSettings>().map_or(1, |t| t.game.perks_per_level).clamp(1, PERKS) - 1, PERKS),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.perks_per_level = choice_index(v, PERKS) + 1;
                        }
                    },
                    |v| (choice_index(v, PERKS) + 1).to_string(),
                ),
            ),
        );
        add_setting(
            app,
            Setting::slider(
                "tweak-special-points",
                GROUP,
                "S.P.E.C.I.A.L. Points",
                "The total the S.P.E.C.I.A.L. menu lets the player share out (when no script set one); 40 changes nothing. (Stewie's Tweaks bCustomSpecialPoints)",
                Slider::choice(
                    SPECIALS,
                    |w| choice_part(special_index(w.get_resource::<TweakSettings>().map_or(fnv_game::chargen::SPECIAL_POINTS, |t| t.game.special_points)), SPECIALS),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.special_points = special_total(choice_index(v, SPECIALS));
                        }
                    },
                    |v| special_total(choice_index(v, SPECIALS)).to_string(),
                ),
            ),
        );
        switch!(app, "pick-without-skill", "Pick Any Lock", "A lock above the player's Lockpick skill can still be picked. (Stewie's Tweaks bPickLocksWithoutSkill)", game.pick_locks_without_skill);
        switch!(app, "hack-without-skill", "Hack Any Terminal", "A terminal above the player's Science skill can still be hacked. (Stewie's Tweaks bHackTerminalsWithoutSkill)", game.hack_terminals_without_skill);
        switch!(app, "no-terminal-lockout", "No Terminal Lockout", "A failed hack does not lock the terminal. (Stewie's Tweaks bNoLockFailedTerminals)", game.no_terminal_lockout);
        switch!(app, "no-casino-bans", "No Casino Bans", "Winning big never bans the player from a casino. (Stewie's Tweaks bNoCasinoBans)", game.no_casino_bans);
        switch!(app, "loot-unconscious", "Search the Unconscious", "An unconscious actor can be searched like a dead one. (Stewie's Tweaks bLootUnconsciousVictims)", game.loot_unconscious);
        switch!(app, "pickpocket-worn", "Pickpocket Worn Items", "What an actor wears can be pickpocketed (never a child's). (Stewie's Tweaks bPickpocketWornItems)", game.pickpocket_worn_items);
        switch!(app, "no-sneak-criticals", "No Sneak Criticals", "The player's sneak attacks are not criticals. (Stewie's Tweaks bNoSneakAttacksCriticals)", game.no_sneak_criticals);
        switch!(app, "reload-when-empty", "Reload Only When Empty", "The player cannot reload a magazine that still has rounds. (Stewie's Tweaks bDisableReloadingNonEmptyClip)", game.reload_only_when_empty);
        switch!(app, "no-travel-time", "Fast Travel Takes No Time", "Fast travel moves the clock (and hardcore needs) on by nothing. (Stewie's Tweaks bNoFastTravelTimeChange)", no_travel_time);
        add_setting(
            app,
            Setting::slider(
                "tweak-power-armor-fall",
                GROUP,
                "Falls in Power Armor",
                "Fall damage to anyone in power armor is multiplied by this; x1.0 changes nothing. (Stewie's Tweaks bPowerArmorScalesFallDamage)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| f64::from(t.power_armor_fall_damage))),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.power_armor_fall_damage = factor(v) as f32;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        // ---- weight ----
        weightless!(app, "weightless-armor", "Weightless Armor", "Armor weighs nothing. (Stewie's Tweaks bWeightlessItems, bArmor)", fnv_sim::inventory::WEIGHTLESS_ARMOR);
        weightless!(app, "weightless-aid", "Weightless Aid", "Aid items weigh nothing. (Stewie's Tweaks bWeightlessItems, bAid)", fnv_sim::inventory::WEIGHTLESS_AID);
        weightless!(app, "weightless-ammo", "Weightless Ammo", "Ammo weighs nothing (it only weighs anything in Hardcore). (Stewie's Tweaks bWeightlessItems, bAmmo)", fnv_sim::inventory::WEIGHTLESS_AMMO);
        weightless!(app, "weightless-weapons", "Weightless Weapons", "Weapons weigh nothing. (Stewie's Tweaks bWeightlessItems, bWeapons)", fnv_sim::inventory::WEIGHTLESS_WEAPONS);
        weightless!(app, "weightless-misc", "Weightless Misc Items", "Misc items weigh nothing. (Stewie's Tweaks bWeightlessItems, bMisc)", fnv_sim::inventory::WEIGHTLESS_MISC);
        switch!(app, "weightless-worn-armor", "Worn Armor Weighs Nothing", "Armor weighs nothing while worn. (Stewie's Tweaks bWeightlessWornArmor)", game.weightless_worn_armor);
        switch!(app, "weightless-worn-power-armor", "Worn Power Armor Weighs Nothing", "Power armor weighs nothing while worn. (Stewie's Tweaks bWeightlessWornPowerArmor)", game.weightless_worn_power_armor);
        // ---- batch 8 ----
        switch!(app, "no-travel-crippled-leg", "No Travel on a Crippled Leg", "Fast travel is refused while a leg is crippled. (Stewie's Tweaks bNoFastTravelIfLegsCrippled)", no_travel_crippled_leg);
        switch!(app, "wait-only-seated", "Wait Only When Seated", "Waiting is refused unless the player sits (god mode excepted). (Stewie's Tweaks bOnlyAllowWaitWhileSitting)", game.wait_only_seated);
        amount!(app, "kill-reward-ap", "Action Points per Kill", "Action Points a kill by the player gives back; 0 changes nothing. (Stewie's Tweaks bKillsRewardAP)", 50.0, f64, game.kill_reward_ap);
        amount!(app, "jump-ap-cost", "Action Points per Jump", "Action Points a jump costs; with too few the player cannot jump. 0 changes nothing. (Stewie's Tweaks bJumpingCostsAP)", 50.0, f32, jump_ap_cost);
        amount!(app, "run-ap-cost", "Action Points per Second Running", "Action Points running costs each second; with none left the player walks. 0 changes nothing. (Stewie's Tweaks bRunningCostsAP)", 20.0, f32, run_ap_per_second);
        // ---- batch 9 ----
        switch!(app, "no-selling", "No Selling", "The player cannot sell items in a barter (buying still works). (Stewie's Tweaks bDisableSellingItems)", no_selling);
        add_setting(
            app,
            Setting {
                key: "tweak-custom-skill-points",
                group: GROUP,
                label: "Own Skill Points Rule",
                note: "A level-up gives the base below plus Intelligence times the amount below (rounded up) instead of the game's rule. (Stewie's Tweaks bModifySkillPointsEarned)",
                get: |w| w.get_resource::<TweakSettings>().is_some_and(|t| t.game.skill_points.is_some()),
                set: |w, on| {
                    if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                        let numbers = (t.skill_point_base, t.skill_points_per_int);
                        t.game.skill_points = on.then_some(numbers);
                    }
                },
                slider: None,
            },
        );
        add_setting(
            app,
            Setting::slider(
                "tweak-skill-point-base",
                GROUP,
                "Skill Points: Base",
                "The base of the Own Skill Points Rule (0 to 30). (Stewie's Tweaks iSkillPointBase)",
                Slider::choice(
                    SKILL_BASES,
                    |w| choice_part(w.get_resource::<TweakSettings>().map_or(SKILL_POINTS_START.0, |t| t.skill_point_base) as usize, SKILL_BASES),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.skill_point_base = choice_index(v, SKILL_BASES) as i32;
                            t.apply_skill_points();
                        }
                    },
                    |v| choice_index(v, SKILL_BASES).to_string(),
                ),
            ),
        );
        add_setting(
            app,
            Setting::slider(
                "tweak-skill-points-per-int",
                GROUP,
                "Skill Points: per Intelligence",
                "Points per point of Intelligence in the Own Skill Points Rule (0 to 2). (Stewie's Tweaks fPointsPerInt)",
                Slider {
                    get: |w| (w.get_resource::<TweakSettings>().map_or(SKILL_POINTS_START.1, |t| t.skill_points_per_int) / 2.0) as f32,
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.skill_points_per_int = f64::from(v) * 2.0;
                            t.apply_skill_points();
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| format!("{:.1}", v * 2.0)),
                },
            ),
        );
        switch!(app, "no-unconscious-message", "No Unconscious Message", "No line saying an actor is unconscious. (Stewie's Tweaks bNoUnconciousMessage)", game.no_unconscious_message);
        switch!(app, "no-voice-filter", "No Helmet Voice Filter", "Voices from behind helmets and speakers play unfiltered. (Stewie's Tweaks bNoAudioDistortion)", game.no_voice_filter);
        switch!(app, "no-skip-dialogue", "No Skipping Lines", "A click or key does not cut a spoken line short. (Stewie's Tweaks bNoSkipDialog)", game.no_skip_dialogue);
        switch!(app, "lockpick-mouse", "Mouse Turns the Lock", "In lockpicking, the left mouse button turns the screwdriver as the movement keys do. (Stewie's Tweaks bLockpickAllowMouse)", game.lockpick_mouse_turn);
        switch!(app, "no-max-casino-bet", "No Casino Bet Limit", "Blackjack, roulette and the slots take any bet the player can pay. (Stewie's Tweaks bNoMaxCasinoBet)", game.no_max_casino_bet);
        switch!(app, "no-aiming-slowdown", "Full Pace While Aiming", "Aiming or blocking does not slow the player to a walk. (Stewie's Tweaks bAimingSpeed)", no_aiming_slowdown);
        switch!(app, "repeat-jumping", "Hold to Keep Jumping", "Holding Jump jumps again on landing. (Stewie's Tweaks bRepeatJumping)", repeat_jumping);
        switch!(app, "hold-to-sneak", "Hold to Sneak", "Sneak only while the Crouch/Sneak key is held. (Stewie's Tweaks bHoldCrouchToSneak)", hold_to_sneak);
        // ---- batch 10 ----
        amount!(app, "vats-entry-ap", "Action Points to Enter V.A.T.S.", "Action Points entering V.A.T.S. costs; refused without them. 0 changes nothing. (Stewie's Tweaks bEnteringVATSCostsAP)", 50.0, f64, game.vats_entry_ap);
        amount!(app, "power-attack-ap", "Action Points per Power Attack", "Action Points a power attack costs; 0 changes nothing. (Stewie's Tweaks bChargedAttacksCostAP)", 50.0, f32, power_attack_ap);
        amount!(app, "quest-marker-reach", "Quest Markers Within", "Quest targets farther than this (game units) are left off the compass; 0 changes nothing. (Stewie's Tweaks bDistanceBasedQuestMarkerVisibilty)", 20000.0, f32, quest_marker_reach);
        part!(app, "scope-zoom-time", "Scope Zoom Time", "How long a scope takes to zoom, as a part of the usual; 100% changes nothing. (Stewie's Tweaks bUseCustomSniperZoomRate)", scope_zoom_time);
        switch!(app, "drop-key", "Q Drops Items", "In the Pip-Boy's inventory, Q drops the selected item. (Stewie's Tweaks bAddInventoryDropItemHotkey)", drop_key);
        // ---- batch 11 ----
        switch!(app, "no-container-after-pick", "Picked Containers Stay Shut", "A container stays shut after its lock is picked; open it yourself. (Stewie's Tweaks bDontShowContainerAfterLockpick)", game.no_container_after_pick);
        switch!(app, "no-door-after-pick", "Picked Doors Stay Shut", "A door is not used at once after its lock is picked. (Stewie's Tweaks bDontOpenDoorsAfterLockpick)", game.no_door_after_pick);
        switch!(app, "mark-unvisited-doors", "Mark Unvisited Doors", "A door to a place never visited ends its prompt with a star. (Stewie's Tweaks bPatchUnseenCellName)", game.mark_unvisited_doors);
        switch!(app, "no-exit-hacking", "No Leaving a Hack", "Once a guess is made, the hacking game cannot be left before it ends. (Stewie's Tweaks bNoExitHacking)", game.no_exit_hacking);
        switch!(app, "no-grab-owned", "No Grabbing Owned Items", "Someone else's things cannot be picked up with Grab. (Stewie's Tweaks bNoGrabOwnedItems)", game.no_grab_owned);
        // ---- batch 12 ----
        add_setting(
            app,
            Setting::slider(
                "tweak-agility-speed",
                GROUP,
                "Agility Moves Faster",
                "Everyone's speed gains this much per point of Agility above 5, and loses it below; 0% changes nothing. (Stewie's Tweaks bAgilityScalesMovementSpeed)",
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(0.0, |t| (t.game.agility_speed_per_point * 10.0) as f32),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.agility_speed_per_point = f64::from(v) / 10.0;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| format!("{:.1}%", v * 10.0)),
                },
            ),
        );
        switch!(app, "auto-unlock-terminals", "Skip Easy Hacks", "A terminal opens without the hacking game when Science is at least its lock plus the margin below. (Stewie's Tweaks bAutoUnlockTerminals)", game.auto_unlock_terminals);
        amount!(app, "auto-unlock-margin", "Skip Easy Hacks: Margin", "How far over the lock Science must be to skip the hacking game (0 to 50). (Stewie's Tweaks bAutoUnlockTerminals, iThreshold)", 50.0, f32, game.auto_unlock_margin);
        switch!(app, "weapons-unhurt-by-hits", "Shots Spare Weapons", "A shot that hits a weapon in hand does not wear it. (Stewie's Tweaks bHittingWeaponsDoesntDamageThem)", game.weapons_unhurt_by_hits);
        switch!(app, "god-mode-no-jams", "No Jams in God Mode", "In god mode the player's weapon never jams. (Stewie's Tweaks bGodModePreventsJamAnims)", game.god_mode_no_jams);
        switch!(app, "knockdown-by-strength", "Strength Resists Blasts", "An explosion's knockdown is resisted by Strength instead of Agility. (Stewie's Tweaks bExplosionKnockdownAvoidanceUsesStrength)", game.knockdown_by_strength);
        switch!(app, "drowning-drains-ap", "Drowning Takes AP First", "Drowning takes Action Points before health. (Stewie's Tweaks bDrowningDrainsAP)", game.drowning_drains_ap);
        switch!(app, "aiming-draws-weapon", "Aiming Draws the Weapon", "Aiming with the weapon holstered draws it. (Stewie's Tweaks bAimingUnholstersWeapons)", aiming_draws_weapon);
        add_setting(
            app,
            Setting::slider(
                "tweak-walk-angle",
                GROUP,
                "Steepest Walkable Slope",
                "The steepest slope the player can walk up, in degrees; 45 is the usual. (Stewie's Tweaks bBillyGoatMode)",
                Slider::choice(
                    ANGLES,
                    |w| choice_part(((w.get_resource::<TweakSettings>().map_or(45.0, |t| t.walk_angle) - 30.0) / 5.0).round().clamp(0.0, (ANGLES - 1) as f32) as usize, ANGLES),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.walk_angle = walk_angle(choice_index(v, ANGLES));
                        }
                    },
                    |v| format!("{:.0} degrees", walk_angle(choice_index(v, ANGLES))),
                ),
            ),
        );
        part!(app, "crippled-leg-jump", "Jumping on a Crippled Leg", "The jump's height for each crippled leg, as a part of the usual; 100% changes nothing. (Stewie's Tweaks bCrippledLegsScaleJumpHeight)", crippled_leg_jump);
        // ---- batch 13 ----
        switch!(app, "aid-at-full-health", "Aid at Full Health", "The STATS page's Stimpak and Doctor's Bag can be used with nothing hurt. (Stewie's Tweaks bAllowAidAtMaxHealth)", game.aid_at_full_health);
        switch!(app, "grabbing-is-theft", "Grabbing Is Stealing", "Grabbing someone else's thing counts as stealing it. (Stewie's Tweaks bGrabbingItemsIsCrime)", game.grabbing_is_theft);
        switch!(app, "vendor-lists", "Merchants Keep to Their Lists", "A merchant buys from the player only what its list says it deals in. (Stewie's Tweaks bBarterCheckActorBuySellFlags)", vendor_lists_both);
        switch!(app, "key-name", "Name the Key", "A lock that needs a key names it. (Stewie's Tweaks bLockNeedsKeyShowName)", game.lock_needs_key_name);
        switch!(app, "no-skill-tags", "No Skill Tags", "Dialogue topics show no skill or perk tag. (Stewie's Tweaks bNoSkillTags)", game.no_skill_tags);
        switch!(app, "no-npc-addiction", "No NPC Addiction", "Only the player becomes addicted to chems. (Stewie's Tweaks bPreventNPCAddiction)", game.no_npc_addiction);
        switch!(app, "pickpocket-fleeing", "Pickpocket the Fleeing", "An actor running away can be pickpocketed. (Stewie's Tweaks bPickpocketFleeingActors)", game.pickpocket_fleeing);
        switch!(app, "full-reload-swaps", "Full Reload Swaps Ammo", "Reloading a full magazine changes to the next ammo. (Stewie's Tweaks bReloadingWithFullClipSwitchesAmmoType)", game.full_reload_swaps_ammo);
        add_setting(
            app,
            Setting::slider(
                "tweak-melee-reach",
                GROUP,
                "Melee Reach",
                "The player's melee reach is multiplied by this; x1.0 changes nothing. (Stewie's Tweaks bScalePlayerMeleeReach)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| f64::from(t.game.melee_reach_mult))),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.melee_reach_mult = factor(v) as f32;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        amount!(app, "repair-xp", "Experience per Repair", "Experience a Pip-Boy repair gives; 0 changes nothing. (Stewie's Tweaks bRepairsRewardXP)", 50.0, f64, game.repair_xp);
        switch!(app, "repair-caps-container", "Repair Caps Go to the Till", "Caps paid for a repair go into the merchant's container, not its pockets. (Stewie's Tweaks bPreventStealingCapsAfterRepair)", game.repair_caps_to_container);
        switch!(app, "no-wear-on-dead", "Spare the Blade on the Dead", "Melee blows that land only on the dead do not wear the weapon. (Stewie's Tweaks bNoDamageMeleeWeaponIfTargetDead)", game.no_wear_on_dead);
        add_setting(
            app,
            Setting::slider(
                "tweak-encumbered-ap-regen",
                GROUP,
                "Action Points Back When Over-Encumbered",
                "How fast Action Points come back while over-encumbered, as a part of the usual; 100% changes nothing. (Stewie's Tweaks bNoAPRegenWhileOverencumbered)",
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(1.0, |t| t.game.encumbered_ap_regen as f32),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.encumbered_ap_regen = f64::from(v);
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: None,
                },
            ),
        );
        switch!(app, "light-step-companions", "Light Step for Companions", "Teammates are spared mines whenever the player would be (Light Step). (Stewie's Tweaks bLightStepAffectsCompanions)", game.light_step_companions);
        switch!(app, "no-player-stagger", "No Player Stagger", "The player never staggers from a hit. (Stewie's Tweaks bNoPlayerStaggerAnims)", no_player_stagger);
        switch!(app, "ready-does-not-draw", "Ready Does Not Draw", "Ready Item reloads or holsters a drawn weapon but does not draw a holstered one. (Stewie's Tweaks bPreventReloadWeaponUnholstering)", ready_does_not_draw);
        // ---- batch 14 ----
        switch!(app, "jam-reload-pace", "Jams at Reload Pace", "Clearing a jam takes as long as the reload's own pace makes it. (Stewie's Tweaks bReloadJamsAffectedByAgility)", game.jam_at_reload_pace);
        switch!(app, "no-hotkeys-reloading", "No Hotkeys While Reloading", "Weapon hotkeys do nothing while reloading. (Stewie's Tweaks bReloadingPreventsWeaponHotkeys)", no_hotkeys_while_reloading);
        part!(app, "wading-pace", "Wading Pace", "The pace in water up to swimming depth, as a part of the usual (shallower water slows less); 100% changes nothing. (Stewie's Tweaks bRunSlowerInWater)", wading_pace);
        switch!(app, "sleep-on-chairs", "Sleep on Chairs", "Waiting while sitting on a chair is sleeping. (Stewie's Tweaks bSleepOnChairs)", game.sleep_on_chairs);
        switch!(app, "stealing-no-alarm", "Quiet Theft", "Taking someone's things raises no alarm; pickpocketing still does. (Stewie's Tweaks bStealingSendsNoAlarm)", game.stealing_no_alarm);
        switch!(app, "no-companion-kill-xp", "No XP for Companion Kills", "A teammate's kills give the player no experience. (Stewie's Tweaks bNoCompanionKillXP)", game.no_companion_kill_xp);
        switch!(app, "no-disarm-companions", "Companions Keep Their Weapons", "A crippled arm does not make a teammate drop its weapon. (Stewie's Tweaks bNoDisarmCompanions)", game.no_disarm_companions);
        switch!(app, "no-self-explosion-damage", "Own Blasts Spare the Player", "The player's own explosions do not hurt the player. (Stewie's Tweaks bNoSelfExplosionDamage)", game.no_self_explosion_damage);
        switch!(app, "sneak-criticals-melee", "Sneak Criticals Melee Only", "Only melee and unarmed sneak attacks are criticals. (Stewie's Tweaks bSneakCriticalsOnlyMeleeWeapons)", game.sneak_criticals_melee_only);
        switch!(app, "teammates-meltdown", "Meltdown for Companions", "Teammates' energy weapon kills use the player's Meltdown perk. (Stewie's Tweaks bAllowTeammatesUseMeltdown)", game.teammates_meltdown);
        switch!(app, "speed-scales-jump", "Speed Scales the Jump", "The player's Speed Mult (and armour's pace) scales the jump's height. (Stewie's Tweaks bSpeedMultScalesJumpHeight)", game.speed_scales_jump);
        // ---- batch 15 ----
        switch!(app, "no-tutorials", "No Minigame Help", "Lockpicking, blackjack, roulette and the slots show no first-time help. (Stewie's Tweaks bDisableTutorialMessages)", game.no_tutorials);
        switch!(app, "no-knockdown-god", "No Knockdowns in God Mode", "In god mode the player is never knocked down. (Stewie's Tweaks bNoKnockdownInGodmode)", game.no_knockdown_in_god_mode);
        switch!(app, "show-current-skill", "Say My Skill", "When a lock or terminal needs more skill, the line says the player's own. (Stewie's Tweaks bLockpickHackingMessageShowsCurrentSkill)", game.show_current_skill);
        amount!(app, "min-splatter", "Blood Only on Big Hits", "Hits that take less health than this put no blood on the screen; 0 changes nothing. (Stewie's Tweaks bMinBloodSplatterHealthDamage)", 50.0, f32, min_splatter_damage);
        switch!(app, "xp-sound-at-cap", "XP Sound at the Level Cap", "Experience earned at the level cap still plays its sound. (Stewie's Tweaks bMinigamesPlayXPSoundAtMaxLevel)", xp_sound_at_cap);
        switch!(app, "hold-to-wait", "Hold to Wait", "The Wait key opens the menu when held half a second, not when tapped. (Stewie's Tweaks bHoldWaitKeyToShowMenu)", hold_to_wait);
        switch!(app, "no-sight-zoom", "No Iron Sight Zoom", "Aiming down iron sights does not zoom the view (scopes still do). (Stewie's Tweaks bDisableWeaponFOV)", no_sight_zoom);
        switch!(app, "hide-misc-quest", "Hide Quest Items in Misc", "The Pip-Boy's Misc list leaves quest items out. (Stewie's Tweaks bHideMiscQuestItems)", hide_misc_quest);
        // ---- batch 16 ----
        switch!(app, "hotkey-toggles-in-hand", "Hotkey Draws and Holsters", "Pressing the hotkey of the weapon in hand draws or holsters it. (Stewie's Tweaks bHotkeyHolstersWeaponIfEquipped)", hotkey_toggles_in_hand);
        switch!(app, "no-interior-fog", "No Interior Fog", "Interiors have no fog (from the next room entered). (Stewie's Tweaks bDisableInteriorFog)", no_interior_fog);
        switch!(app, "mark-unspoken", "Mark Strangers", "Someone not yet spoken to ends their prompt with a star. (Stewie's Tweaks bUnspokenNPCIndicator)", game.mark_unspoken);
        switch!(app, "no-caps-added", "No Caps Added Line", "Getting caps shows no line. (Stewie's Tweaks bHideCapsAddedMessages)", game.no_caps_added_message);
        switch!(app, "subtitle-names", "Subtitles Name the Speaker", "General subtitles start with the speaker's name. (Stewie's Tweaks bSubtitlesShowActorNames)", game.subtitle_names);
        switch!(app, "compass-pips-danger", "Compass Pips Only in Danger", "Actors show on the compass only while in danger. (Stewie's Tweaks bNoCompassPipsIfNotInDanger)", compass_pips_in_danger_only);
        switch!(app, "neutral-foes-compass", "Foes in the Usual Colour", "Hostile actors show on the compass in the HUD colour, not red. (Stewie's Tweaks bHostilesUseNeutralColorOnCompass)", neutral_foes_on_compass);
        switch!(app, "sneak-label-standing", "Sneak Label While Standing", "The Hidden / Detected label shows while standing too. (Stewie's Tweaks bShowSneakLabelWhileStanding)", sneak_label_standing);
        switch!(app, "no-xp-meter-combat", "No XP Meter in Combat", "Experience gained in a fight shows once it is over, all together. (Stewie's Tweaks bNoXPBarInCombat)", no_xp_meter_in_combat);
        switch!(app, "no-limb-indicator", "No Crippled Limb Figure", "The crippled-limb figure stays off the HUD. (Stewie's Tweaks bDisableHUDCrippledLimbIndicator)", no_limb_indicator);
        // ---- batch 17: menus ----
        switch!(app, "keep-acting-notes", "Keep Acting Notes", "Spoken lines keep their {acting notes} in the subtitles. (Stewie's Tweaks bDialogueKeepVoiceActingNotes)", game.keep_acting_notes);
        switch!(app, "hide-completed-quests", "Hide Finished Quests", "The Pip-Boy lists only quests under way. (Stewie's Tweaks bHideCompletedQuests)", game.hide_completed_quests);
        switch!(app, "hide-marker-reputation", "No Reputation on Map Markers", "Map markers show no reputation line. (Stewie's Tweaks bHideMapMarkerFactionReputation)", game.hide_marker_reputation);
        switch!(app, "no-show-quest-location", "No Show Location", "The quests page has no Show Location button. (Stewie's Tweaks bDisableShowQuestLocation)", game.no_show_quest_location);
        switch!(app, "hide-trait-ranks", "No Ranks for Traits", "The trait menu shows no Ranks line. (Stewie's Tweaks bHideRanksInTraitMenu)", game.hide_trait_ranks);
        switch!(app, "hide-quest-item-figures", "No Figures for Quest Items", "Quest items and keys show no weight or value in the Pip-Boy. (Stewie's Tweaks bHideQuestItemWeightAndValue)", game.hide_quest_item_figures);
        switch!(app, "single-item-figures", "One Item's Figures", "A stack shows one item's weight and value in the Pip-Boy. (Stewie's Tweaks bIndividialItemStats)", game.single_item_figures);
        switch!(app, "mark-other-ammo", "Mark Other Ammo", "A weapon's ammo line ends with + when another ammo it takes is carried. (Stewie's Tweaks bShowAlternateAmmoTypesAvailableInMenus)", game.mark_other_ammo);
        switch!(app, "show-fame-infamy", "Show Fame and Infamy", "Reputations show their fame and infamy. (Stewie's Tweaks bReputationShowsFameInfamy)", game.show_fame_infamy);
        switch!(app, "take-all-stays", "Take All Stays Open", "Take All leaves the container open. (Stewie's Tweaks bContainerTakeAllDoesntCloseMenu)", take_all_stays);
        switch!(app, "hide-equipped-barter", "Hide Worn Items in Barter", "Equipped items are not offered in barter. (Stewie's Tweaks bHideEquippedItemsInBarter)", hide_equipped_barter);
        switch!(app, "hide-equipped-containers", "Hide Worn Items in Containers", "Equipped items are not listed on the player's side of a container. (Stewie's Tweaks bHideEquippedItemsInContainers)", hide_equipped_containers);
        switch!(app, "title-case-categories", "Quiet Category Titles", "Container category titles in title case, not capitals. (Stewie's Tweaks bNoCapitaliseContainerCategories)", title_case_categories);
        switch!(app, "no-right-click-drop", "No Right-Click Drop", "A right click in the Pip-Boy inventory does not drop the item. (Stewie's Tweaks bDisableInventoryRightClickDrop)", no_right_click_drop);
        switch!(app, "quiet-undiscovered", "Quiet Undiscovered Markers", "Clicking an undiscovered map marker puts up no box. (Stewie's Tweaks bPreventNoFastTravelMessage)", quiet_undiscovered);
        // ---- batch 19 ----
        switch!(app, "repair-any-armor", "Repairers Take Any Armor", "A repairer lists armor with no DT too. (Stewie's Tweaks bShowQuestAndNoDRDTItemsInRepairMenu)", game.repair_any_armor);
        switch!(app, "sort-misc-stats", "Sorted Statistics", "The STATS page's misc statistics in name order. (Stewie's Tweaks bSortMiscStats)", game.sort_misc_stats);
        switch!(app, "effect-time-left", "Effect Time Left", "Effects on the STATS page show the seconds left. (Stewie's Tweaks bStatsMenuShowEffectTimeRemaining)", game.effect_time_left);
        switch!(app, "perk-plugin-names", "Perk Plugins", "A level-up perk's description names the plugins it comes from. (Stewie's Tweaks bShowModNameInPerkDescriptions)", game.perk_plugin_names);
        amount!(app, "item-added-seconds", "Item Added Line Time", "Seconds an item-added line stays; 0 changes nothing. (Stewie's Tweaks bCustomAddToInventoryMessageTimer)", 10.0, f32, game.item_added_seconds);
        switch!(app, "pipboy-12-hour", "12-Hour Pip-Boy Clock", "The Pip-Boy's clock in 12 hours with AM and PM. (Stewie's Tweaks b12HourPipboyClock)", game.pipboy_12_hour);
        // ---- batch 20 ----
        switch!(app, "notes-of-all-quests", "Notes of Every Quest", "Show Active Quest Notes takes in every quest under way. (Stewie's Tweaks bShowActiveQuestNotesShowsAllStartedQuests)", game.notes_of_all_quests);
        part!(app, "aim-turn-x", "Aiming Turn Speed: Across", "How fast the view turns left and right while aiming, as a part of the usual; 100% changes nothing. (Stewie's Tweaks bTurnSlowerWhileAiming)", aim_turn_x);
        part!(app, "aim-turn-y", "Aiming Turn Speed: Up and Down", "How fast the view turns up and down while aiming, as a part of the usual; 100% changes nothing. (Stewie's Tweaks bTurnSlowerWhileAiming)", aim_turn_y);
        switch!(app, "pickpocket-after-caught", "Pickpocket Again", "Someone who caught the player pickpocketing can be tried again. (Stewie's Tweaks bAllowPickpocketIfAlreadyCaught)", game.pickpocket_after_caught);
        switch!(app, "wait-anywhere", "Wait Anywhere", "Only a script's ban stops waiting or sleeping: not combat, enemies, water, falling or someone's bed. (Stewie's Tweaks bSleepWaitAnywhere)", game.wait_anywhere);
        part!(app, "radio-music", "Radio Songs Volume", "The radio's songs, as a part of their volume; 100% changes nothing. (Stewie's Tweaks bScaleRadioVolume)", radio_music_volume);
        part!(app, "radio-talk", "Radio Talk Volume", "The radio's talk, as a part of its volume; 100% changes nothing. (Stewie's Tweaks bScaleRadioVolume)", radio_talk_volume);
        switch!(app, "camera-wheel-pov", "Wheel Zoom Needs Toggle View", "The mouse wheel moves the third-person camera only while Toggle View is held. (Stewie's Tweaks bOnlyChangeCameraHeightIfPOVKeyHeld)", camera_wheel_needs_pov);
        switch!(app, "quiet-hotkeys", "No Hotkey Sounds", "Weapon hotkeys make no equip sound. (Stewie's Tweaks bNoHotkeyEquipSounds)", quiet_hotkeys);
        // ---- batch 21 ----
        switch!(app, "detached-beams", "Detached Beams", "The player's beams stay where they were fired instead of following the barrel. (Stewie's Tweaks bDetachedBeams)", game.detached_beams);
        switch!(app, "god-mode-off-on-load", "God Mode Off on Load", "Loading a save turns god mode off. (Stewie's Tweaks bDisableGodmodeOnLoad)", game.god_mode_off_on_load);
        switch!(app, "no-stealth-shimmer-pipboy", "No Stealth Shimmer in the Pip-Boy", "The stealth shimmer is off while the Pip-Boy is up. (Stewie's Tweaks bDisableStealthEffectInPipboy)", game.no_stealth_shimmer_in_pipboy);
        switch!(app, "quiet-teammate-steps", "Quiet Companion Footsteps", "Teammates' footsteps make no sound. (Stewie's Tweaks bPreventTeammateFootstepSounds)", game.quiet_teammate_steps);
        switch!(app, "no-weapon-hotkeys", "No Weapon Hotkeys", "The number keys and the D-pad pick no weapon (the wheel still does). (Stewie's Tweaks bDisableWeaponHotkeys)", no_weapon_hotkeys);
        switch!(app, "pipboy-opens-items", "Pip-Boy Opens on Items", "The Pip-Boy always opens on the inventory. (Stewie's Tweaks bOpenPipboyToInventoryByDefault)", pipboy_opens_items);
        switch!(app, "hide-crosshair-reloading", "No Crosshair While Reloading", "The crosshair is hidden while reloading. (Stewie's Tweaks bHideCrosshairWhileReloading)", hide_crosshair_reloading);
        // ---- batch 22 ----
        add_setting(
            app,
            Setting::slider(
                "tweak-kill-xp-per-level",
                GROUP,
                "Kill XP by Level",
                "Kill experience gains this much for each level the target is above the player, and loses it below; 0% changes nothing. (Stewie's Tweaks bLevelDifferenceAffectsCombatXP)",
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(0.0, |t| (t.game.kill_xp_per_level * 5.0) as f32),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.kill_xp_per_level = f64::from(v) / 5.0;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| format!("{:.0}%", v * 20.0)),
                },
            ),
        );
        part!(app, "key-xp-share", "Experience for Keys", "Opening a lock with its key gives this share of picking it; 0% changes nothing. (Stewie's Tweaks bUsingKeysRewardsXP)", game.key_xp_share);
        part!(app, "password-xp-share", "Experience for Passwords", "Opening a terminal with its password gives this share of hacking it, once; 0% changes nothing. (Stewie's Tweaks bUsingNotesRewardsXP)", game.password_xp_share);
        // ---- batch 23 ----
        switch!(app, "invert-x", "Invert Look Left/Right", "The mouse and the stick turn the view the other way left and right. (Stewie's Tweaks bInvertCameraX)", invert_x);
        switch!(app, "nearest-unfound", "Nearest Unfound Place", "The nearest place not yet found shows on the compass at any distance. (Stewie's Tweaks bShowNearestUndiscoveredLocation)", nearest_unfound_on_compass);
        switch!(app, "discovered-in-corner", "Discoveries in the Corner", "A place found is a corner line, not a banner. (Stewie's Tweaks bLocationDiscoveredCornerMessage)", discovered_in_corner);
        // ---- batch 24 ----
        switch!(app, "power-armor-radiation", "Power Armor Blocks Radiation", "Power armor on the body keeps the world's radiation off; water and chems still count. (Stewie's Tweaks bNoEnvironmentRadiationInFullPowerArmor)", game.power_armor_blocks_radiation);
        add_setting(
            app,
            Setting::slider(
                "tweak-power-armor-limbs",
                GROUP,
                "Limb Damage in Power Armor",
                "Limb damage to anyone in power armor is multiplied by this; x1.0 changes nothing. (Stewie's Tweaks bPowerArmorScalesLimbDamage)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| t.game.power_armor_limb_damage)),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.power_armor_limb_damage = factor(v);
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        add_setting(
            app,
            Setting::slider(
                "tweak-fall-height-endurance",
                GROUP,
                "Endurance Softens Falls",
                "The player's safe fall height gains this much per point of Endurance above 5, and loses it below; 0% changes nothing. (Stewie's Tweaks bEnduranceScalesMinFallHeight)",
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(0.0, |t| t.game.fall_height_per_endurance * 5.0),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.fall_height_per_endurance = v / 5.0;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| format!("{:.0}%", v * 20.0)),
                },
            ),
        );
        // ---- batch 25 ----
        switch!(app, "invert-title-wheel", "Reverse Category Wheel", "The mouse wheel over a container's category title steps the other way. (Stewie's Tweaks bInvertContainerTitleScrollwheelDirection)", invert_title_wheel);
        switch!(app, "auto-continue", "Continue at Start", "The main menu continues the newest save at once, the first time it opens; hold Shift to stay on the menu. (Stewie's Tweaks bAutoContinueGame)", auto_continue);
        // ---- batch 26 ----
        switch!(app, "hide-ammo-label", "No Ammo Counter", "The HUD shows no ammo counter. (Stewie's Tweaks bHideAmmoLabel)", hide_ammo_label);
        amount!(app, "free-item-price", "Price for Worthless Things", "What merchants pay or ask for things worth nothing (spent casings); 0 changes nothing. (Stewie's Tweaks bNoFreeBarterItems)", 10.0, f32, free_item_price);
        // ---- batch 27 ----
        switch!(app, "skip-easy-locks", "Skip Easy Locks", "A lock well under the player's Lockpick opens at once (with its experience); use it again to go through. (Stewie's Tweaks bSkipVeryEasyLocksAtMaxSkill)", game.skip_easy_locks);
        amount!(app, "skip-lock-margin", "Skip Easy Locks: Margin", "How far over the lock Lockpick must be to skip the game (0 to 50). (Stewie's Tweaks bSkipVeryEasyLocksAtMaxSkill, iThreshold)", 50.0, f32, game.skip_lock_margin);
        switch!(app, "pick-up-disarmed", "Pick Up Disarmed Mines", "A mine the player disarms goes straight into the inventory. (Stewie's Tweaks bDisarmingMinesPicksThemUp)", game.pick_up_disarmed);
        // ---- batch 28 ----
        switch!(app, "npc-weapons-whole", "NPC Weapons Hit Whole", "NPCs' weapons hit as hard as whole ones, whatever their condition. (Stewie's Tweaks bNoScaleNpcDamageByCondition)", game.npc_weapons_whole);
        switch!(app, "lethal-knockdowns", "Killing Knockdowns Count", "A knockdown that kills still counts for the knockdown challenges. (Stewie's Tweaks bLethalHitsIncreaseLimbChallenges)", game.lethal_knockdowns_count);
        switch!(app, "duplicate-binds", "Shared Keys", "Binding an input to a control leaves other controls with it as they are (no swap). (Stewie's Tweaks bAllowDuplicateControlBinds)", duplicate_binds);
        // ---- batch 29 ----
        switch!(app, "crippling-never-disarms", "Crippling Never Disarms", "A crippled arm never makes anyone drop their weapon. (Stewie's Tweaks bCripplingDoesntDisarm)", game.crippling_never_disarms);
        switch!(app, "no-spin-up", "No Spin-Up", "The player's automatic weapons fire without spinning up first. (Stewie's Tweaks bAutoWeaponNoFiringDelay)", game.no_spin_up);
        switch!(app, "god-mode-free-crafting", "Free Crafting in God Mode", "In god mode recipes need no parts or skill. (Stewie's Tweaks bGodModeAllowsCraftingWithoutRequirements)", game.god_mode_free_crafting);
        switch!(app, "pov-follows-weapon", "View Follows the Weapon", "Drawing the weapon goes to first person, holstering it to third. (Stewie's Tweaks bTogglePOVWhenHolsteringWeapon)", pov_follows_weapon);
        part!(app, "kill-cam-chance", "Kill Camera Chance", "The share of kill cameras that play (after the Kill Camera setting's own choice); 100% changes nothing. (Stewie's Tweaks bChanceBasedKillcams)", kill_cam_chance);
        amount!(app, "subtitle-distance", "Subtitle Distance", "General subtitles show for speakers within this many units; 0 changes nothing. (Stewie's Tweaks bCustomSubtitleDistance)", 10000.0, f32, game.subtitle_distance);
        switch!(app, "sneak-criticals-head", "Sneak Criticals Head Only", "Sneak attacks are criticals only on the head. (Stewie's Tweaks bSneakCriticalsHeadshotsOnly)", game.sneak_criticals_head_only);
        switch!(app, "compare-weapons", "Compare Weapons", "A weapon's DAM and DPS in the Pip-Boy show the difference from the weapon in hand (both whole). (Stewie's Tweaks bCompareWeaponStats)", game.compare_weapons);
        switch!(app, "warn-encumbrance", "Weight Warning", "In a container, the player's weight shows red when taking the selected item would be too much to carry. (Stewie's Tweaks bContainerEncumbranceIndicator)", warn_encumbrance);
        amount!(app, "mark-nearby-places", "Mark Nearby Places", "Map markers within this many units of the player are marked on the map; 0 changes nothing. (Stewie's Tweaks bMarkNearbyLocationsOnMap)", 10000.0, f32, game.mark_nearby_places);
        switch!(app, "scale-ash-piles", "Ash Piles Fit the Body", "An ash pile takes the size of the actor it was (between half and twice). (Stewie's Tweaks bScaleAshpileSize)", game.scale_ash_piles);
        switch!(app, "pipboy-buzz", "Pip-Boy Buzz", "A short controller buzz on each Pip-Boy page or tab change (with rumble on). (Stewie's Tweaks bHapticPipboy)", pipboy_buzz);
        switch!(app, "reload-while-firing", "Reload While Firing", "A reload can start while the last shot's recovery still runs. (Stewie's Tweaks bReloadWhileFiring)", game.reload_while_firing);
        add_setting(
            app,
            Setting::slider(
                "tweak-semi-auto-queue",
                GROUP,
                "Semi-Auto Shot Queue",
                "A semi-automatic pull this close before the weapon is ready again is kept and fired then (0 to 0.5 s); 0 changes nothing. (Stewie's Tweaks bSemiAutoQueue)",
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(0.0, |t| t.game.semi_auto_queue * 2.0),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.semi_auto_queue = v / 2.0;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| format!("{:.3} s", v / 2.0)),
                },
            ),
        );
        switch!(app, "repair-scales-crafting", "Crafting Uses Repair", "Weapons and armour made at a workbench come out at a condition set by Repair: half at 0, whole at 100. (Stewie's Tweaks bRepairScalesCraftingCondition)", game.repair_scales_crafting);
        switch!(app, "no-healing-in-combat", "No Healing In Combat", "Aid that restores health or a limb cannot be used from the Pip-Boy while something fights the player. (Stewie's Tweaks bNoHealingInCombat)", game.no_healing_in_combat);
        switch!(app, "power-attack-if-blocking", "Power Attack From a Block", "While blocking with a melee weapon, a press of attack is a power attack at once, with no hold. (Stewie's Tweaks bPowerAttackIfBlocking)", power_attack_if_blocking);
        amount!(app, "breath-regain", "Slow Breath Regen", "Seconds of breath each second at the surface gives back; the meter stays up until it is whole. 0 refills it at once. (Stewie's Tweaks bSlowBreathRegen)", 20.0, f32, game.breath_regain);
        switch!(app, "vats-hip-fire", "V.A.T.S. Hip Fire", "V.A.T.S. entered without aiming fires every shot from the hip (the chance to hit is the same). (Stewie's Tweaks bVATSHipFire)", vats_hip_fire);
        switch!(app, "pickpocket-caught-detects", "Caught Pickpocketing Means Detected", "An actor that catches the player pickpocketing detects the player, so a hostile one does not leave them hidden. (Stewie's Tweaks bPickpocketCaughtCausesDetection)", game.pickpocket_caught_detects);
        add_setting(
            app,
            Setting::slider(
                "tweak-agility-jump",
                GROUP,
                "Agility Scales Jump Height",
                "The jump height times 1 + this x (Agility - 1), from 0 to 0.2 per point; 0 changes nothing. (Stewie's Tweaks bAgilityScalesJumpHeight)",
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(0.0, |t| t.game.agility_jump_mult * 5.0),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.agility_jump_mult = v / 5.0;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(|v| format!("{:.2}", v / 5.0)),
                },
            ),
        );
        amount!(app, "lockpick-force-near", "Force Lock Near the Sweet Spot", "Percent added to Force Lock with the pin on the sweet spot, less the further away, none at 90 degrees; 0 changes nothing. (Stewie's Tweaks bLockpickForceBonusNearSweetSpot)", 50.0, f32, game.lockpick_force_near_bonus);
        switch!(app, "contact-mines-player", "Contact Mines (Player)", "A mine the player stands on goes off at once, with no fuse. (Stewie's Tweaks bContactMines, bPlayer)", game.contact_mines_player);
        switch!(app, "contact-mines-npcs", "Contact Mines (Others)", "A mine anyone else stands on goes off at once, with no fuse. (Stewie's Tweaks bContactMines, bNPCs)", game.contact_mines_npcs);
        amount!(app, "sleep-heal-min-hours", "Sleep Healing Min Duration", "Hours a sleep must last before it restores Health and heals limbs; 0 heals from the first hour. (Stewie's Tweaks bSleepHealingMinDuration)", 12.0, u32, game.sleep_heal_min_hours);
        amount!(app, "barter-repair-discount", "Barter Lowers Repair Costs", "Percent of a vendor's repair price the player's Barter takes off at 100 (in proportion below); 0 changes nothing. (Stewie's Tweaks bBarterAffectsRepairCosts)", 60.0, f32, game.barter_repair_discount);
        add_setting(
            app,
            Setting::slider(
                "tweak-armor-condition-penalty",
                GROUP,
                "Armour Condition Penalty",
                "How fast armour below half condition loses its DT and DR (to half at x1.0, to none at x2.0); x1.0 changes nothing. (Stewie's Tweaks bCustomArmorConditionPenalty)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| t.game.armor_condition_penalty)),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.armor_condition_penalty = factor(v);
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        switch!(app, "vats-ignores-companions", "V.A.T.S. Ignores Companions", "Companions cannot be targeted in V.A.T.S. (Stewie's Tweaks bIgnoreCompanionsVATS)", game.vats_ignores_companions);
        switch!(app, "no-pipboy-in-combat", "No Pip-Boy In Combat", "The Pip-Boy does not open while something fights the player; a corner message says so. (Stewie's Tweaks bNoPipBoyInCombat)", no_pipboy_in_combat);
        switch!(app, "frenzy-kill-xp", "Frenzied Kills Earn Experience", "A kill by a frenzied actor (Aggression Frenzied) earns the player experience. (Stewie's Tweaks bFrenzyGrantsPlayerXP)", game.frenzy_kill_xp);
        switch!(app, "free-wild-wasteland", "Free Wild Wasteland", "Wild Wasteland can be taken without using one of the trait choices. (Stewie's Tweaks bFreeWildWasteland)", game.free_wild_wasteland);
        switch!(app, "stats-xp-percent", "Experience as a Percentage", "The Stats page shows experience as how far through the level the player is, in percent. (Stewie's Tweaks bStatsMenuPercentageXP)", game.stats_xp_percent);
        switch!(app, "repair-same-first", "Repair List: Same Item First", "In the Pip-Boy repair list, parts that are the same item come first, the rest by name. (Stewie's Tweaks bSortPipboyRepairMenu)", game.repair_same_first);
        switch!(app, "repair-low-first", "Repair List: Low Condition First", "In the Pip-Boy repair list, parts in worse condition come first and the best at the bottom. (Stewie's Tweaks bInvertPipboyRepairMenuSorting)", game.repair_low_first);
        switch!(app, "no-weapon-move-penalty", "No Weapon Movement Penalty", "A drawn two-handed weapon does not slow the player's run. (Stewie's Tweaks bNoEquippedWeaponMovementPenalty)", no_weapon_move_penalty);
        switch!(app, "unequip-broken-armor", "Unequip Broken Armour", "The player's armour is taken off when it breaks, staying in the inventory. (Stewie's Tweaks bUnequipBrokenArmor)", unequip_broken_armor);
        switch!(app, "hide-crosshair-first", "No Crosshair in First Person", "The crosshair is not drawn in first person; the activation prompt stays. (Stewie's Tweaks bHideCrosshairInFirstPerson)", hide_crosshair_first);
        switch!(app, "hide-crosshair-third", "No Crosshair in Third Person", "The crosshair is not drawn in third person; the activation prompt stays. (Stewie's Tweaks bHideCrosshairInThirdPerson)", hide_crosshair_third);
        switch!(app, "no-skill-books-at-max", "No Skill Books at 100", "A skill book whose skill is already at 100 is not read from the Pip-Boy, so it is not wasted. (Stewie's Tweaks bNoSkillBooksAbove100)", game.no_skill_books_at_max);
        part!(app, "red-hp-below", "Red Health Bar Below", "The HUD's health bar turns red below this share of health; 0% never. (Stewie's Tweaks bColoredHUDBars)", red_hp_below);
        part!(app, "red-ap-below", "Red Action Point Bar Below", "The HUD's Action Point bar turns red below this share; 0% never. (Stewie's Tweaks bColoredHUDBars)", red_ap_below);
        part!(app, "weapon-cnd-red-below", "Red Weapon Condition Below", "The HUD's weapon condition bar turns red below this condition; 0% never. (Stewie's Tweaks bColorWeaponCndLabel)", weapon_cnd_red_below);
        part!(app, "weapon-cnd-flash-below", "Flashing Weapon Condition Below", "The HUD's weapon condition bar flashes red below this condition; 0% never. (Stewie's Tweaks bColorWeaponCndLabel)", weapon_cnd_flash_below);
        switch!(app, "blackjack-total", "Blackjack Hand Total", "The Blackjack screen shows the total of the hand being played. (Stewie's Tweaks bBlackJackTotalDisplay)", blackjack_total);
        amount!(app, "player-pain-gap", "Less Frequent Pain Sounds", "Seconds after a pain sound before the player makes another; 0 changes nothing. (Stewie's Tweaks bLessFrequentPlayerPainSounds)", 10.0, f64, game.player_pain_gap);
        switch!(app, "pause-on-load", "Pause On Load", "A loaded save starts in the pause menu instead of straight into play. (Stewie's Tweaks bPauseOnSaveLoad)", game.pause_on_load);
        switch!(app, "swap-ammo-holstered", "Swap Ammo While Holstered", "Ammo Swap changes the ammo type with the weapon put away; it reloads when drawn. (Stewie's Tweaks bSwapAmmoWithWeaponHolstered)", game.swap_ammo_holstered);
        switch!(app, "slots-hold-to-spin", "Slots: Hold to Spin", "Holding W at a slot machine spins it again each time the reels stop. (Stewie's Tweaks bSlotsAutoSpinHotkey)", slots_hold_to_spin);
        switch!(app, "no-auto-continue-dialogue", "Click to Continue Dialogue", "In a conversation each line waits for a click before the next, giving time to read. (Stewie's Tweaks bNoAutoContinueDialog)", game.no_auto_continue_dialogue);
        switch!(app, "shift-talks-sneaking", "Shift to Talk While Sneaking", "While sneaking, holding Shift talks to a person instead of picking their pocket. (Stewie's Tweaks bTalkWhileSneakingIfShiftIsHeld)", shift_talks_sneaking);
        switch!(app, "no-quest-set-on-objective", "No Active Quest From New Objectives", "With no quest active, a new objective does not make its quest the active one. (Stewie's Tweaks bDontSetQuestWhenObjectivesAdded)", game.no_quest_set_on_objective);
        switch!(app, "no-quest-set-on-complete", "No Active Quest After Completing One", "When the active quest is done, no other quest becomes active by itself. (Stewie's Tweaks bDontSetQuestWhenCompletingObjectives)", game.no_quest_set_on_complete);
        switch!(app, "hide-read-notes", "Hide Read Notes", "The Pip-Boy's notes list leaves out notes already read. (Stewie's Tweaks bHideReadNotes)", game.hide_read_notes);
        add_setting(
            app,
            Setting::slider(
                "tweak-console-key",
                GROUP,
                "Console Key",
                "The key that opens and closes the console. (Stewie's Tweaks iConsoleKey)",
                Slider::choice(
                    CONSOLE_KEYS.len(),
                    |w| choice_part(w.get_resource::<TweakSettings>().map_or(0, |t| t.console_key).min(CONSOLE_KEYS.len() - 1), CONSOLE_KEYS.len()),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.console_key = choice_index(v, CONSOLE_KEYS.len());
                        }
                    },
                    |v| CONSOLE_KEYS[choice_index(v, CONSOLE_KEYS.len())].1.to_string(),
                ),
            ),
        );
        switch!(app, "tab-closes-terminals", "Tab Closes Terminals", "Tab closes a terminal from any page instead of going back a page (Backspace still goes back). (Stewie's Tweaks bTabClosesTerminals)", tab_closes_terminals);
        switch!(app, "tab-back-in-pause", "Tab Goes Back in the Pause Menu", "Tab goes back a page in the pause menu, as Backspace does (the Settings and Controls pages keep Tab for their tabs). (Stewie's Tweaks bTabBackInStartMenu)", tab_back_in_pause);
        switch!(app, "unsafe-fast-travel", "Fast Travel From Anywhere", "Fast travel works from places flagged against it, such as interiors; leaving a scripted place this way can break it. (Stewie's Tweaks bAllowUnsafeFastTravel)", unsafe_fast_travel);
        switch!(app, "vats-burst-stops-on-kill", "V.A.T.S. Burst Stops on a Kill", "The rest of a burst in V.A.T.S. is not fired once its target is dead. (Stewie's Tweaks bVATSStopBurstIfTargetDead)", game.vats_burst_stops_on_kill);
        switch!(app, "repair-needs-weapon-skill", "Repair Needs the Weapon's Skill", "The player can repair a weapon only with Repair at least the weapon's skill requirement. (Stewie's Tweaks bRepairRequiresWeaponSkill)", game.repair_needs_weapon_skill);
        add_setting(
            app,
            Setting::slider(
                "tweak-date-style",
                GROUP,
                "Date Format",
                "How the Pip-Boy and the wait menu write the date. (Stewie's Tweaks iDateFormat)",
                Slider::choice(
                    crate::wait::model::DATE_STYLES.len(),
                    |w| choice_part(w.get_resource::<TweakSettings>().map_or(0, |t| t.date_style).min(crate::wait::model::DATE_STYLES.len() - 1), crate::wait::model::DATE_STYLES.len()),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.date_style = choice_index(v, crate::wait::model::DATE_STYLES.len());
                        }
                    },
                    |v| crate::wait::model::DATE_STYLES[choice_index(v, crate::wait::model::DATE_STYLES.len())].to_string(),
                ),
            ),
        );
        switch!(app, "hide-unknown-names", "Hide Unknown Names", "The activation prompt gives no name for someone who has not spoken to the player yet (bodies are still named). (Stewie's Tweaks bHideUnknownNPCNames)", game.hide_unknown_names);
        switch!(app, "vats-keeps-holstered", "V.A.T.S. Puts the Weapon Back", "A weapon that V.A.T.S. drew is put away again when the mode ends with nothing fired. (Stewie's Tweaks bEnteringVATSDoesntUnholsterWeapon)", vats_keeps_holstered);
        switch!(app, "no-hotkey-wheel", "No Hotkey Wheel", "Holding a hotkey does not open the weapon wheel; letting go uses the slot as a tap does. (Stewie's Tweaks bNoHUDHotkeyPopup)", no_hotkey_wheel);
        switch!(app, "ammo-label-under", "Ammo Label Under the Count", "The HUD writes the ammo type under the ammo count instead of beside it. (Stewie's Tweaks bMoveAmmoTypeLabel)", ammo_label_under);
        add_setting(
            app,
            Setting::slider(
                "tweak-companion-pip",
                GROUP,
                "Companion Compass Colour",
                "The colour of companions' marks on the compass; Off leaves them as anyone's. (Stewie's Tweaks bCompanionPipColorChange)",
                Slider::choice(
                    COMPANION_COLOURS.len(),
                    |w| choice_part(w.get_resource::<TweakSettings>().map_or(0, |t| t.companion_pip).min(COMPANION_COLOURS.len() - 1), COMPANION_COLOURS.len()),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.companion_pip = choice_index(v, COMPANION_COLOURS.len());
                        }
                    },
                    |v| COMPANION_COLOURS[choice_index(v, COMPANION_COLOURS.len())].0.to_string(),
                ),
            ),
        );
        switch!(app, "female-facial-hair", "Facial Hair for Women", "The character creator and the barber offer facial hair to a woman too. (Stewie's Tweaks bRaceMenuAllowFemaleFacialHair)", female_facial_hair);
        switch!(app, "terminal-password-prompt", "Show Use Password", "A locked terminal whose password note the player holds says so in its prompt until first opened. (Stewie's Tweaks bShowUsePasswordOnTerminalsWithNote)", game.terminal_password_prompt);
        switch!(app, "lockpick-move-keys", "Lockpicking With the Movement Keys", "Picking a lock, the strafe keys move the pin (Shift faster, Ctrl slower) and Forward, Back or Jump turn the screwdriver. (Stewie's Tweaks bLockPickMenuKeyboardMovement)", game.lockpick_move_keys);
        switch!(app, "store-all", "Store All", "In a container, Shift with Take All puts everything listed on the player's side in it, but what is worn or a quest's. (Stewie's Tweaks bContainerMenuStoreAllHotkey)", store_all);
        switch!(app, "map-recentre-key", "Map Recentre Key", "C puts the Pip-Boy's local or world map back on the player after it was dragged. (Stewie's Tweaks bMapRecenterHotkey)", game.map_recentre_key);
        switch!(app, "map-marker-distance", "Map Marker Distance", "The World Map says how far the marker under the pointer is from the player. (Stewie's Tweaks bMapLocationDisplayDistance)", game.map_marker_distance);
        switch!(app, "quantity-shows-price", "Quantity Box Shows the Price", "In a barter, the \"How many?\" box says what the amount costs. (Stewie's Tweaks bBarterQuantityMenuShowsPrice)", game.quantity_shows_price);
        switch!(app, "quantity-shows-weight", "Quantity Box Shows the Weight", "In a container, the \"How many?\" box says what the amount weighs. (Stewie's Tweaks bContainerQuantityMenuShowsWeight)", game.quantity_shows_weight);
        switch!(app, "hide-empty-reputation", "Hide an Empty Reputation List", "With no reputations yet, the Stats page's General tab shows the statistics only, with no switch to the reputations. (Stewie's Tweaks bHideReputationTabIfEmpty)", game.hide_empty_reputation);
        switch!(app, "quick-load-in-menus", "Quick Load From the Menus", "The QuickLoad key loads the quicksave from the main and pause menus too. (Stewie's Tweaks bStartMenuQuickLoad)", quick_load_in_menus);
        switch!(app, "preview-perks", "Preview Perks on Level Up", "A level-up without a perk still shows the perks page, to look at with none to take. (Stewie's Tweaks bPreviewPerksOnLevelUp)", game.preview_perks);
        switch!(app, "pick-locks-with-key", "Pick Locks Even With the Key", "Holding Shift, a lock the player has the key of opens the lockpicking game instead. (Stewie's Tweaks bPickLocksEvenWithKey)", pick_locks_with_key);
        switch!(app, "right-click-previous-category", "Right Click for the Previous Category", "In a container or a barter, a right click on a list's heading steps back to the previous category. (Stewie's Tweaks bRightClickChangesToPreviousContainerCategory)", right_click_previous_category);
        switch!(app, "mark-notes-unread", "Mark Notes Unread", "A right click on a note in the Pip-Boy marks it unread again. (Stewie's Tweaks bMarkNotesUnread)", game.mark_notes_unread);
        switch!(app, "full-barter-total", "Barter Total Past the Merchant's Caps", "The barter figure shows the whole total even when it is more than the merchant's caps (the merchant still pays only what it has). (Stewie's Tweaks bShowBarterTotalWhenOverSellLimit)", full_barter_total);
        switch!(app, "compass-fade-left", "Compass Fades on the Left", "Marks on the compass fade toward its left end (quest marks stay whole), for a compass seen from its middle. (Stewie's Tweaks bCompassFadeLeftSide)", compass_fade_left);
        switch!(app, "custom-dead-zones", "Controller Dead Zones", "The sticks use the dead zones below instead of the game's (12% of a push); 0% takes every movement. (Stewie's Tweaks bDisableControllerDeadzones)", custom_dead_zones);
        part!(app, "left-dead-zone", "Left Stick Dead Zone", "The share of a push the left stick ignores, with Controller Dead Zones on. (Stewie's Tweaks iControllerDeadzone)", left_dead_zone);
        part!(app, "right-dead-zone", "Right Stick Dead Zone", "The share of a push the right stick ignores, with Controller Dead Zones on. (Stewie's Tweaks iControllerDeadzoneRS)", right_dead_zone);
        switch!(app, "custom-trigger-dead-zones", "Trigger Dead Zones", "LT and RT count as pressed at the points below instead of three quarters down. (Stewie's Tweaks bControllerTriggerDeadzones)", custom_trigger_dead_zones);
        part!(app, "lt-dead-zone", "Left Trigger Press Point", "How far LT goes down before it counts as pressed, with Trigger Dead Zones on. (Stewie's Tweaks iDeadzoneLT)", lt_dead_zone);
        part!(app, "rt-dead-zone", "Right Trigger Press Point", "How far RT goes down before it counts as pressed, with Trigger Dead Zones on. (Stewie's Tweaks iDeadzoneRT)", rt_dead_zone);
        switch!(app, "responsive-stick", "More Responsive Controller Aiming", "The right stick's turn grows in a straight line with the push past its dead zone instead of with its square, so small pushes answer at once. (Stewie's Tweaks bMoreResponsiveControllerAiming)", responsive_stick);
        switch!(app, "faster-pad-orbit", "Faster Controller Camera Circling", "With a controller, the camera circles the player twice as fast while Toggle POV is held. (Stewie's Tweaks bFasterControllerPOVRotate)", faster_pad_orbit);
        add_setting(
            app,
            Setting::slider(
                "tweak-horizontal-look",
                GROUP,
                "Horizontal Look Speed",
                "Left and right look speed apart from up and down, on top of the sensitivity; x1.0 changes nothing. (Stewie's Tweaks bSeparateHorizontalSensitivity)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| t.horizontal_look)),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.horizontal_look = factor(v);
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        switch!(app, "vats-shift-ignores-friends", "Shift Ignores Friends in V.A.T.S.", "Holding Shift, V.A.T.S. marks no one who would not fight the player. (Stewie's Tweaks bShiftIgnoresFriendlyVATS)", game.vats_shift_ignores_friends);
        switch!(app, "remember-console-history", "Remember Console History", "The console's entered lines are kept between runs, in ConsoleHistory.txt in NVForever's folder. (Stewie's Tweaks bRememberConsoleHistory)", remember_console_history);
        switch!(app, "console-output-file", "Console Output File", "Console lines are added to ConsoleOutput.txt in NVForever's folder from the start. (Stewie's Tweaks bUseConsoleOutputFile)", console_output_file);
        switch!(app, "console-text-shadow", "Console Text Shadow", "The console's lines are a darker green over a faint shadow, easier to read on a bright view. (Stewie's Tweaks bConsoleTextShadow)", console_text_shadow);
        switch!(app, "click-to-exit-loading", "Press to Leave Loading Screens", "A loading screen stays up once everything is in until a key or a button is pressed. (Stewie's Tweaks bClickToExitLoadScreens)", click_to_exit_loading);
        switch!(app, "radiation-radio-static", "Radiation Brings Radio Static", "Taking radiation brings static into the Pip-Boy's radio, more the faster the rads come. (Stewie's Tweaks bRadiationCausesRadioStatic)", game.radiation_radio_static);
        switch!(app, "barter-caps-change", "Barter Shows Caps After", "In a barter, each side's caps after the trade are shown beside its caps now. (Stewie's Tweaks bBarterShowCapsChange)", barter_caps_change);
        switch!(app, "no-self-meltdown", "No Self Meltdown Damage", "Blasts given off by the Meltdown perk hurt neither the player nor the companions. (Stewie's Tweaks bNoSelfMeltdownDamage)", game.no_self_meltdown);
        add_setting(
            app,
            Setting::slider(
                "tweak-max-level",
                GROUP,
                "Level Cap",
                "The highest level the player can reach, with no levels added for the add-ons; 0 keeps the game's. (Stewie's Tweaks iMaxCharacterLevel)",
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(0.0, |t| t.game.max_level as f32 / MAX_LEVEL_CAP),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.max_level = (v * MAX_LEVEL_CAP).round() as u32;
                        }
                    },
                    min: 0.0,
                    step: 1.0 / MAX_LEVEL_CAP,
                    shown: Some(|v| match (v * MAX_LEVEL_CAP).round() as u32 {
                        0 => "the game's".to_string(),
                        n => n.to_string(),
                    }),
                },
            ),
        );
        switch!(app, "standing-sneak-attacks", "Sneak Attacks Standing", "An attack on someone who has not detected the player is a sneak attack without crouching too. (Stewie's Tweaks bSneakAttackWithoutCrouching)", game.standing_sneak_attacks);
        switch!(app, "no-companion-item-damage", "No Companion Item Damage", "The companions' weapons do not wear with use. (Stewie's Tweaks bNoCompanionItemDamage)", game.no_companion_item_damage);
        switch!(app, "modding-needs-skill", "Weapon Mods Need the Skill", "A weapon can be modded only by a player who meets its skill requirement. (Stewie's Tweaks bWeaponModdingSkillRequirement)", game.modding_needs_skill);
        switch!(app, "planting-needs-skill", "Planting Grenades Needs the Skill", "A grenade or mine placed in someone's pocket goes live only if the player meets its skill requirement. (Stewie's Tweaks bPlantingLiveGrenadesRequiresWeaponSkill)", game.planting_needs_skill);
        switch!(app, "travel-costs-bottle", "Fast Travel Costs a Nuka-Cola", "Each fast travel takes a Nuka-Cola Quantum, Quartz or Victory, and is refused without one. (Stewie's Tweaks bFastTravelCostsSpecialNukaBottles)", game.travel_costs_bottle);
        switch!(app, "jump-swims-up", "Jump Swims Upwards", "Holding Jump under water swims up, at the walking stroke's speed. (Stewie's Tweaks bJumpSwimsUpwards)", jump_swims_up);
        switch!(app, "armor-sinks", "Armour Causes Sinking", "Medium and heavy body armour pull the player down in water (heavy faster). (Stewie's Tweaks bArmorCausesSinking)", game.armor_sinks);
        switch!(app, "remember-pin-health", "Remember Bobby Pin Wear", "The bobby pin keeps its wear from one lock to the next, kept in the save. (Stewie's Tweaks bRememberBobbyPinHealth)", game.remember_pin_health);
        switch!(app, "force-spares-lock", "Forcing Breaks the Pin, Not the Lock", "A failed Force Lock breaks the bobby pin in it and the lock can be tried again. (Stewie's Tweaks bForceLockpickNoBreakLock)", game.force_spares_lock);
        switch!(app, "no-condition-damage-penalty", "No Condition Damage Penalty", "Weapons deal their full damage whatever their condition. (Stewie's Tweaks bNoWeaponConditionDamagePenalty)", game.no_condition_damage_penalty);
        switch!(app, "no-condition-damage-buffer", "No Condition Damage Buffer", "A weapon's damage falls as soon as its condition is below full, not only below 75%. (Stewie's Tweaks bRemoveWeaponDamageBuffer)", game.no_condition_damage_buffer);
        switch!(app, "crit-ignores-fire-rate", "Automatic Criticals Not Cut by Fire Rate", "Automatic weapons' critical chance is not divided by their rate of fire. (Stewie's Tweaks bCritChanceIgnoresFireRate)", game.crit_ignores_fire_rate);
        add_setting(
            app,
            Setting::slider(
                "tweak-crit-damage-scale",
                GROUP,
                "Critical Damage",
                "Every critical hit's added damage is multiplied by this; x1.0 changes nothing. (Stewie's Tweaks bScaleCriticalDamage)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| t.game.crit_damage_scale)),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.crit_damage_scale = factor(v);
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        switch!(app, "npc-dr-ignores-condition", "NPC Armour DR Ignores Condition", "An NPC's armour keeps its whole Damage Resistance however worn. (Stewie's Tweaks bNoScaleNpcDamageResistanceByCondition)", game.npc_dr_ignores_condition);
        switch!(app, "npc-dt-ignores-condition", "NPC Armour DT Ignores Condition", "An NPC's armour keeps its whole Damage Threshold however worn. (Stewie's Tweaks bNoScaleNpcDamageThresholdByCondition)", game.npc_dt_ignores_condition);
        switch!(app, "explosion-radius-buff", "Explosion Radius Buff", "The inner 30% of an explosion's radius deals full damage, falling to 20% at the edge. (Stewie's Tweaks bExplosionRadiusBuff)", game.explosion_radius_buff);
        switch!(app, "npcs-sneak-crit-player", "NPCs Sneak Attack the Player", "An NPC the player has not detected lands sneak attack criticals on them, as the player does. (Stewie's Tweaks bNPCsCanSneakCritPlayer)", game.npcs_sneak_crit_player);
        switch!(app, "fatal-crits-gib", "Fatal Criticals Always Gib", "A killing critical hit that is no sneak attack always blows off or severs the part struck. (Stewie's Tweaks bFatalNonSneakCritsAlwaysGib)", game.fatal_crits_gib);
        switch!(app, "fall-deaths-dismember", "Deadly Falls Take the Legs", "An actor killed by a fall loses its legs, blown off where they can be. (Stewie's Tweaks bFallDeathsCauseDismemberment)", game.fall_deaths_dismember);
        switch!(app, "splash-torso-gib", "Splash Kills Can Gib the Torso", "A blast kill by someone with Bloody Mess has a one-in-four chance to blow the torso apart. (Stewie's Tweaks bSplashDamageTorsoGibbing)", game.splash_torso_gib);
        switch!(app, "npcs-drop-holstered", "NPCs Drop Holstered Weapons", "An NPC that dies with its weapon put away drops it, as one with it drawn does. (Stewie's Tweaks bNPCsDropWeaponHolsteredWeapon)", game.npcs_drop_holstered);
        switch!(app, "npcs-earn-casings", "NPCs Keep Spent Cases", "NPCs keep the cases, hulls and drained cells their shots leave, as the player does, to be found on them. (Stewie's Tweaks bNPCsEarnAmmoCasings)", game.npcs_earn_casings);
        switch!(app, "first-person-vats", "First Person V.A.T.S.", "V.A.T.S.'s shots play in first person; the view comes back after. (Stewie's Tweaks bFirstPersonVATS)", first_person_vats);
        switch!(app, "no-ammo-casings", "No Spent Cases", "The player's shots leave no cases, hulls or drained cells in the inventory. (Stewie's Tweaks bNoAmmoCasings)", game.no_ammo_casings);
        switch!(app, "crosshair-when-aiming", "Crosshair Down the Sights", "The crosshair stays on screen while aiming down iron sights; a scope still hides it. (Stewie's Tweaks bKeepCrosshairWhenAiming)", crosshair_when_aiming);
        switch!(app, "move-during-vats-camera", "Move During V.A.T.S. Camera", "You can still walk and run while V.A.T.S.'s camera plays the shots; jumping, firing and aiming stay held. (Stewie's Tweaks bMoveDuringVATSPlayback)", move_during_kill_cam);
        switch!(app, "stack-figures", "Whole-Stack Pickup Figures", "Looking at a stack of items, the weight and value shown are the whole stack's, not one item's. (Stewie's Tweaks bBetterPickupPrompt)", stack_figures);
        switch!(app, "seated-wide", "Turn Further While Seated", "Seated in first person, you can look 115 degrees to either side instead of 80. (Stewie's Tweaks bTurnFurtherWhileSeated)", seated_wide);
        switch!(app, "no-turn-clip", "No Turning Animation", "Your character's body does not step round when you turn on the spot, so it does not slide on after you stop. (Stewie's Tweaks bNoTurningAnim)", no_turn_clip);
        switch!(app, "no-land-clip", "No Landing Animation", "Your character does not play a landing movement after a jump or a fall. (Stewie's Tweaks bRemoveLandingAnim)", no_land_clip);
        switch!(app, "hide-unmapped-places", "Compass: Only Mapped Places", "A place you have not found yet shows on the compass only if it is already marked on your map. (Stewie's Tweaks bHideInvisibleUndiscoveredLocations)", hide_unmapped_places);
        switch!(app, "vigor-review-first", "S.P.E.C.I.A.L. Machine: Straight to the Summary", "The machine that sets your S.P.E.C.I.A.L. opens on its last page, all seven at once, instead of one page each. (Stewie's Tweaks bSkipVigorTesterSpecialPages)", vigor_review_first);
        amount!(app, "scope-delay", "Scope Delay (ms)", "How long, in milliseconds up to 1000, a scope waits after the view has zoomed before it covers the screen; 0 shows it at once. (Stewie's Tweaks bScopeVisibilityDelay)", 1000.0, f32, scope_delay_ms);
        switch!(app, "no-pointer-message-box", "No Pointer Over Message Boxes", "The mouse pointer is hidden while a message box is up; choose with the keys or the controller. (Stewie's Tweaks bHideCursorInMessageMenu)", no_pointer_in_message_box);
        switch!(app, "no-pointer-dialogue", "No Pointer in Conversations", "The mouse pointer is hidden over a conversation's choices; choose with the number keys, the arrows, the wheel or the controller. (Stewie's Tweaks bHideCursorInDialog)", no_pointer_in_dialogue);
        switch!(app, "levelup-wheel", "Level Up: Wheel Sets Skills", "On the level-up skills page, the mouse wheel raises (up) or lowers (down) the skill under the pointer instead of scrolling the list. (Stewie's Tweaks bLevelUpScrollWheelSupport)", levelup_wheel);
        switch!(app, "compare-armor", "Compare Armor", "A piece of armour's DT in the Pip-Boy shows the difference from what you wear in the same places. (Stewie's Tweaks bCompareArmorStats)", game.compare_armor);
        switch!(app, "detailed-stats", "Detailed Stats", "In the Pip-Boy's stats, a raised or lowered value says by how much, as (+5) for (+), and values show past 10 and 100. (Stewie's Tweaks bMoreDetailedStatsMenu)", game.detailed_stats);
        switch!(app, "minigames-timescale", "Lockpicking Follows the Time Speed", "When a script speeds up or slows down time, the lock turns and the pick wears at that speed too. (Stewie's Tweaks bTimescaleAffectsMinigames)", game.minigames_timescale);
        switch!(app, "no-name-limit", "No Name Length Limit", "Your character's name may be longer than 24 letters. (Stewie's Tweaks bNoPlayerNameLimit)", game.no_name_limit);
        switch!(app, "seat-keeps-on-activate", "Activate Doesn't Stand Up", "Seated or lying down, the activate key no longer gets you up; a movement key does. (Stewie's Tweaks bActivatingDoesntStandUp)", seat_keeps_on_activate);
        switch!(app, "plain-going-out", "Plain Screen Going Outside", "Going out of a building shows a plain black screen while the world loads, without a picture or a tip; loading a save is unchanged. (Stewie's Tweaks bNoExteriorLoadScreens)", plain_going_out);
        switch!(app, "mark-recurring-challenges", "Mark Repeating Challenges", "A challenge that starts again each time it is done says \"(repeats)\" beside its progress in the Pip-Boy. (Stewie's Tweaks bRecurringChallengeIndicator)", game.mark_recurring_challenges);
        switch!(app, "hide-hotkeyed-containers", "Hide Hotkeyed Items in Containers", "Things on a hotkey are not listed on your side of a container. (Stewie's Tweaks bHideHotkeyedItemsInContainers)", hide_hotkeyed_containers);
        switch!(app, "hide-hotkeyed-barter", "Hide Hotkeyed Items in Barter", "Things on a hotkey are not offered when trading. (Stewie's Tweaks bHideHotkeyedItemsInBarter)", hide_hotkeyed_barter);
        switch!(app, "hide-hotkeyed-repair", "Hide Hotkeyed Items in Repair", "Things on a hotkey are not offered as parts when repairing in the Pip-Boy. (Stewie's Tweaks bHideHotkeyedItemsInRepairLists)", hide_hotkeyed_repair);
        switch!(app, "map-zoom-at-pointer", "Map Zooms at the Pointer", "The mouse wheel zooms the Pip-Boy's maps in and out about the place under the pointer, not the middle. (Stewie's Tweaks bBetterMapZoom)", game.map_zoom_at_pointer);
        switch!(app, "console-homes", "Console Homes", "The console gets sethome [name] (remember this spot), home [name] (go there), back (return from where home or back left) and delhome [name]; homes last until you quit. (Stewie's Tweaks bAddSetHomeCommand)", console_homes);
        add_setting(
            app,
            Setting::slider(
                "tweak-screenshot-key",
                GROUP,
                "Screenshot Key",
                "The key that saves a picture of the screen. (Stewie's Tweaks iScreenshotKey)",
                Slider::choice(
                    SCREENSHOT_KEYS.len(),
                    |w| choice_part(w.get_resource::<TweakSettings>().map_or(0, |t| t.screenshot_key).min(SCREENSHOT_KEYS.len() - 1), SCREENSHOT_KEYS.len()),
                    |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.screenshot_key = choice_index(v, SCREENSHOT_KEYS.len());
                        }
                    },
                    |v| SCREENSHOT_KEYS[choice_index(v, SCREENSHOT_KEYS.len())].1.to_string(),
                ),
            ),
        );
        switch!(app, "shift-screenshot-hides", "Shift Screenshot Hides the HUD", "Holding Shift as you take a screenshot leaves the HUD, crosshair and messages out of the picture. (Stewie's Tweaks bShiftScreenshotHidesMenus)", shift_screenshot_hides);
        switch!(app, "print-plugin-names", "Console Lines Name Their Plugin", "A line a script prints to the console starts with the name of the plugin it comes from, in brackets. (Stewie's Tweaks bModConsolePrintsIncludeName)", game.print_plugin_names);
        switch!(app, "weapon-requirements-matter", "Weapon Requirements Matter", "A weapon cannot be equipped while your Strength or its skill is below what it asks. (Stewie's Tweaks bWeaponRequirementsMatter)", game.weapon_requirements_matter);
        switch!(app, "console-model-details", "Console Shows Models", "Selecting something in the console also prints the path of its model; its cell's Editor ID and name are always shown. (Stewie's Tweaks bExtraConsoleDetails)", console_model_details);
        part!(app, "back-pace", "Backward Pace", "How fast you move backwards, as a part of your usual pace; 100% changes nothing. (Stewie's Tweaks bMovementPenalties)", back_pace);
        part!(app, "strafe-pace", "Sideways Pace", "How fast you move sideways, as a part of your usual pace; 100% changes nothing. Diagonals fall between. (Stewie's Tweaks bMovementPenalties)", strafe_pace);
        // ---- rest ----
        switch!(app, "sleep-in-owned-beds", "Sleep in Owned Beds", "A bed that belongs to someone else can be slept in. (Stewie's Tweaks bSleepInOwnedBeds)", game.sleep_in_owned_beds);
        // ---- items ----
        switch!(app, "power-armor-no-training", "Power Armor Without Training", "Power armor can be worn without Power Armor Training. (Stewie's Tweaks bPowerArmorNeedsNoTraining)", game.power_armor_needs_no_training);
    }
}

/// The Controller Trigger Dead Zones tweak: how far LT and RT go down before they count
/// as pressed, set on each controller's settings (Bevy's own press point, 0.75, with the
/// tweak off); released a little under that [design: docs/design/tweaks.md].
fn trigger_dead_zones(tweaks: Res<TweakSettings>, mut pads: Query<&mut bevy::input::gamepad::GamepadSettings>) {
    use bevy::input::gamepad::{ButtonSettings, GamepadButton};
    let (lt, rt) = if tweaks.custom_trigger_dead_zones { (tweaks.lt_dead_zone, tweaks.rt_dead_zone) } else { (0.75, 0.75) };
    for mut s in &mut pads {
        for (button, press) in [(GamepadButton::LeftTrigger2, lt), (GamepadButton::RightTrigger2, rt)] {
            let press = press.clamp(0.02, 1.0);
            if s.button_settings.get(&button).is_some_and(|b| (b.press_threshold() - press).abs() < 1e-4) {
                continue;
            }
            if let Ok(b) = ButtonSettings::new(press, press * 0.85) {
                s.button_settings.insert(button, b);
            }
        }
    }
}

/// The Last Weapon key: the weapon the player held before the one now is equipped
/// again (put away, as a hotkey's new weapon is) [design: docs/design/tweaks.md]. The weapons are followed
/// each frame: `(now, before)`.
fn last_weapon(input: crate::bindings::ControlInput, state: Res<State<crate::PlayState>>, mut sim: ResMut<crate::sim::SimState>, mut control: ResMut<crate::control::PlayerControl>, mut seen: Local<(Option<fnv_sim::FormId>, Option<fnv_sim::FormId>)>) {
    use fnv_sim::FormId;
    let forms = sim.forms;
    let equipped = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map(|i| i.equipped()).unwrap_or_default();
    let now = equipped.into_iter().find(|&id| forms.item(id).is_some_and(|it| it.kind == fnv_sim::forms::ItemKind::Weapon));
    if now != seen.0 {
        if seen.0.is_some() {
            seen.1 = seen.0;
        }
        seen.0 = now;
    }
    if *state.get() != crate::PlayState::Playing || !input.just_pressed(crate::bindings::Control::LastWeapon) {
        return;
    }
    let Some(before) = seen.1.filter(|&w| sim.world.item_count(FormId::PLAYER_REF, w).is_ok_and(|n| n > 0)) else {
        info!("tweaks: Last Weapon: none to go back to");
        return;
    };
    match sim.world.equip(FormId::PLAYER_REF, before) {
        Ok(_) => {
            control.weapon_out = false;
            info!("tweaks: Last Weapon: {before} equipped (key)");
        }
        Err(e) => info!("tweaks: Last Weapon: {before} not equipped: {e}"),
    }
}

/// The Radio Volume Up and Down keys: the Settings page's Radio volume a tenth either
/// way, between none and whole [design: docs/design/tweaks.md].
fn radio_volume_keys(input: crate::bindings::ControlInput, volumes: Option<Res<crate::audio_out::SoundVolumes>>) {
    use crate::audio_out::SoundCategory;
    use crate::bindings::Control;
    let Some(v) = volumes else { return };
    let step = if input.just_pressed(Control::RadioVolumeUp) {
        0.1
    } else if input.just_pressed(Control::RadioVolumeDown) {
        -0.1
    } else {
        return;
    };
    let level = ((v.get(SoundCategory::Radio) + step) * 10.0).round() / 10.0;
    v.set(SoundCategory::Radio, level.clamp(0.0, 1.0));
    info!("tweaks: Radio volume {:.0}% (key)", v.get(SoundCategory::Radio) * 100.0);
}

/// The Drop Weapon key: the equipped weapon falls in front of the player as the
/// Pip-Boy's Drop puts it (a new reference turned as the player faces, `RefChanged`)
/// [design: docs/design/tweaks.md]. Not one that can't be dropped (a quest's).
fn drop_weapon(input: crate::bindings::ControlInput, state: Res<State<crate::PlayState>>, mut sim: ResMut<crate::sim::SimState>, mut changed: MessageWriter<crate::RefChanged>) {
    use fnv_sim::FormId;
    if *state.get() != crate::PlayState::Playing || !input.just_pressed(crate::bindings::Control::DropWeapon) {
        return;
    }
    let forms = sim.forms;
    let equipped = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map(|i| i.equipped()).unwrap_or_default();
    let Some(item) = equipped.into_iter().find(|&id| forms.item(id).is_some_and(|it| it.kind == fnv_sim::forms::ItemKind::Weapon)) else {
        info!("tweaks: Drop Weapon: nothing in hand");
        return;
    };
    if forms.item(item).is_some_and(|it| it.cant_drop || it.quest_item) {
        info!("tweaks: Drop Weapon: {item} can't be dropped");
        return;
    }
    match sim.world.drop_equipped(FormId::PLAYER_REF, item) {
        Ok(refs) => {
            let heading = sim.world.peek(FormId::PLAYER_REF).map_or(0.0, |p| p.rot[2]);
            for id in refs {
                if let Ok(s) = sim.world.get(id) {
                    s.rot = [0.0, 0.0, heading];
                }
                info!("tweaks: Drop Weapon: {item} dropped as reference {id}");
                changed.write(crate::RefChanged::new(id.0));
            }
        }
        Err(e) => info!("tweaks: Drop Weapon: {item} not dropped: {e}"),
    }
}

/// The Action Point costs of jumping and running ([`TweakSettings::jump_ap_cost`],
/// [`TweakSettings::run_ap_per_second`]) [design: docs/design/tweaks.md]: a jump is paid
/// when asked for and refused without the points; running drains each frame and, with
/// none left, the player walks. Not in god mode.
fn ap_costs(
    time: Res<Time>,
    tweaks: Res<TweakSettings>,
    mut intent: ResMut<crate::player::MoveIntent>,
    mut script: Option<ResMut<ScriptState>>,
    sim: Option<ResMut<crate::sim::SimState>>,
    mut jumped: Local<bool>,
) {
    let (jump, run) = (tweaks.jump_ap_cost, tweaks.run_ap_per_second);
    let asked = intent.jump;
    let fresh_jump = asked && !*jumped;
    *jumped = asked;
    if (jump <= 0.0 && run <= 0.0) || crate::combat::god_mode() {
        return;
    }
    let (Some(rt), Some(mut sim)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), sim) else { return };
    let running = (intent.forward != 0.0 || intent.right != 0.0) && !intent.walk && !intent.sneak;
    let dt = time.delta_secs();
    let intent = &mut *intent;
    crate::script::with_world(rt, &mut sim, |rt| {
        use fnv_game::actor_values::ACTION_POINTS;
        let g = &mut rt.game;
        let p = fnv_sim::FormId::PLAYER_REF;
        let mut ap = fnv_game::functions::actor_value(g, p, ACTION_POINTS).unwrap_or(0.0);
        if fresh_jump && jump > 0.0 {
            if ap >= f64::from(jump) {
                fnv_game::functions::add_actor_value_damage(g, p, ACTION_POINTS, f64::from(jump));
                ap -= f64::from(jump);
            } else {
                intent.jump = false;
                info!("tweaks: jump refused, {ap:.0} AP of {jump:.0}");
            }
        }
        if running && run > 0.0 {
            if ap > 0.0 {
                fnv_game::functions::add_actor_value_damage(g, p, ACTION_POINTS, f64::from(run * dt).min(ap));
            } else {
                intent.walk = true;
            }
        }
    });
}

/// The Tweaks tab's keys on the Controls page (`crate::bindings::Control`), none bound
/// unless the player binds them [design: docs/design/tweaks.md]: the crosshair and the sneak label shown or hidden,
/// the menus hidden as ToggleMenus does, the main menu or quit at once, and no collision
/// while held. Only while playing.
#[allow(clippy::too_many_arguments)]
fn keys(
    input: crate::bindings::ControlInput,
    state: Res<State<crate::PlayState>>,
    mut tweaks: ResMut<TweakSettings>,
    mut script: Option<ResMut<ScriptState>>,
    noclip: Option<ResMut<crate::player::NoClip>>,
    mut to_main: MessageWriter<crate::pause::MainMenuRequest>,
    mut exit: MessageWriter<AppExit>,
    (pip, mut inventory): (Option<ResMut<crate::pipboy::PipBoy>>, MessageWriter<crate::menu::OpenInventory>),
) {
    use crate::bindings::Control;
    use crate::pipboy::Page;
    if *state.get() != crate::PlayState::Playing {
        return;
    }
    // The Pip-Boy page and tab keys: open on that page [design: docs/design/tweaks.md].
    const PAGES: [(Control, Page, Option<usize>); 7] = [
        (Control::PipBoyStats, Page::Stats, None),
        (Control::PipBoyItems, Page::Items, None),
        (Control::PipBoyData, Page::Data, None),
        (Control::OpenLocalMap, Page::Data, Some(0)),
        (Control::OpenWorldMap, Page::Data, Some(1)),
        (Control::OpenQuests, Page::Data, Some(2)),
        (Control::OpenRadio, Page::Data, Some(4)),
    ];
    if let Some(mut pip) = pip {
        if let Some(&(_, page, tab)) = PAGES.iter().find(|(c, _, _)| input.just_pressed(*c)) {
            let tab = tab.unwrap_or(pip.tabs[page.index()]);
            pip.open_at = Some((page, tab));
            inventory.write(crate::menu::OpenInventory);
            info!("tweaks: Pip-Boy opened on {} tab {} (key)", page.title(), page.tabs()[tab]);
        }
    }
    if input.just_pressed(Control::SmoothCamera) {
        tweaks.smooth_look = !tweaks.smooth_look;
        info!("tweaks: smooth camera {}", if tweaks.smooth_look { "on" } else { "off" });
    }
    if input.just_pressed(Control::ToggleCrosshair) {
        tweaks.crosshair_hidden = !tweaks.crosshair_hidden;
        info!("tweaks: crosshair {}", if tweaks.crosshair_hidden { "hidden" } else { "shown" });
    }
    if input.just_pressed(Control::ToggleSneakLabel) {
        tweaks.no_sneak_label = !tweaks.no_sneak_label;
        info!("tweaks: sneak label {}", if tweaks.no_sneak_label { "hidden" } else { "shown" });
    }
    if input.just_pressed(Control::ToggleMenus) {
        if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            let c = &mut rt.game.ui.camera;
            c.menus_hidden = !c.menus_hidden;
            info!("tweaks: menus {}", if c.menus_hidden { "hidden" } else { "shown" });
        }
    }
    if let Some(mut n) = noclip {
        if input.just_pressed(Control::NoCollision) {
            n.0 = true;
            info!("tweaks: no collision (key held)");
        } else if input.just_released(Control::NoCollision) {
            n.0 = false;
            info!("tweaks: collision again");
        }
    }
    if input.just_pressed(Control::ExitToMainMenu) {
        info!("tweaks: back to the main menu (key)");
        to_main.write(crate::pause::MainMenuRequest);
    }
    if input.just_pressed(Control::ExitGame) {
        info!("tweaks: quit (key)");
        exit.write(AppExit::Success);
    }
}

/// The Level Cap slider's top (ours).
const MAX_LEVEL_CAP: f32 = 100.0;

/// What the Pip-Boy says when the No Skill Books Above 100 tweak keeps a book (our words).
pub const BOOK_AT_MAX_LINE: &str = "That skill cannot go any higher.";

/// The corner message when the No Pip-Boy In Combat tweak keeps it shut (our words).
pub const NO_PIPBOY_LINE: &str = "The Pip-Boy cannot be used in combat.";

/// What the Pip-Boy says when the No Healing In Combat tweak refuses an item (our words).
pub const NO_HEALING_LINE: &str = "Healing items cannot be used in combat.";

/// The No Healing In Combat tweak: whether using `item` now is refused, as it restores
/// health or a limb and something fights the player (`fighting`,
/// `crate::after_combat::AfterCombat`) [design: docs/design/tweaks.md].
pub fn healing_barred(rt: &mut crate::script::Runtime, sim: &mut crate::sim::SimState, item: fnv_sim::FormId, fighting: bool) -> bool {
    if !fighting || !rt.game.tweaks.no_healing_in_combat {
        return false;
    }
    let gains = crate::script::with_world(rt, sim, |rt| rt.game.aid_preview(fnv_sim::FormId::PLAYER_REF, item));
    gains.iter().any(|&(av, amount)| amount > 0.0 && (av == fnv_game::actor_values::HEALTH || fnv_game::actor_values::LIMB_CONDITIONS.contains(&av)))
}

/// Hand the game's part to the running game when it differs, and log the change.
#[allow(clippy::too_many_arguments)]
fn sync(tweaks: Res<TweakSettings>, mut script: Option<ResMut<ScriptState>>, quests: Option<ResMut<crate::quest_messages::QuestMessages>>, loading: Option<ResMut<crate::loading::Loading>>, console: Option<ResMut<crate::console::Console>>, mut active: Option<ResMut<crate::menu::ActiveMenu>>, bindings: Option<ResMut<crate::bindings::Bindings>>, hotkeys: Option<Res<crate::hotkeys::Hotkeys>>) {
    crate::combat::set_unequip_broken_armor(tweaks.unequip_broken_armor);
    crate::kill_cam::set_move_during(tweaks.move_during_kill_cam);
    crate::console::set_model_details(tweaks.console_model_details);
    crate::reticle::set_stack_figures(tweaks.stack_figures);
    SEATED_WIDE.store(tweaks.seated_wide, std::sync::atomic::Ordering::Relaxed);
    NO_TURN_CLIP.store(tweaks.no_turn_clip, std::sync::atomic::Ordering::Relaxed);
    NO_LAND_CLIP.store(tweaks.no_land_clip, std::sync::atomic::Ordering::Relaxed);
    SEAT_KEEPS_ON_ACTIVATE.store(tweaks.seat_keeps_on_activate, std::sync::atomic::Ordering::Relaxed);
    crate::pad::set_dead_zones(tweaks.custom_dead_zones.then_some((tweaks.left_dead_zone, tweaks.right_dead_zone)));
    crate::wait::model::set_date_style(tweaks.date_style as u8);
    if let Some(mut b) = bindings {
        if b.allow_duplicates != tweaks.duplicate_binds {
            b.allow_duplicates = tweaks.duplicate_binds;
        }
    }
    if let Some(mut l) = loading {
        if l.hide_tips != tweaks.no_loading_tips {
            l.hide_tips = tweaks.no_loading_tips;
        }
        if l.click_to_exit != tweaks.click_to_exit_loading {
            l.click_to_exit = tweaks.click_to_exit_loading;
        }
        if l.plain_going_out != tweaks.plain_going_out {
            l.plain_going_out = tweaks.plain_going_out;
        }
    }
    let hidden = crate::quest_messages::Hidden { failed: tweaks.no_quest_failed_popup, discovered: tweaks.no_location_popup, objectives: tweaks.no_objective_popup };
    if let Some(m) = active.as_deref_mut().and_then(|a| a.0.as_mut()) {
        if m.sync_categories != tweaks.sync_categories {
            m.sync_categories = tweaks.sync_categories;
        }
        if m.no_selling != tweaks.no_selling {
            m.no_selling = tweaks.no_selling;
        }
        if m.vendor_lists_both != tweaks.vendor_lists_both {
            m.vendor_lists_both = tweaks.vendor_lists_both;
        }
        if m.hide_misc_quest != tweaks.hide_misc_quest {
            m.hide_misc_quest = tweaks.hide_misc_quest;
        }
        if m.free_item_price != tweaks.free_item_price {
            m.free_item_price = tweaks.free_item_price;
        }
        if m.warn_encumbrance != tweaks.warn_encumbrance {
            m.warn_encumbrance = tweaks.warn_encumbrance;
        }
        if m.store_all != tweaks.store_all {
            m.store_all = tweaks.store_all;
        }
        if m.full_barter_total != tweaks.full_barter_total {
            m.full_barter_total = tweaks.full_barter_total;
        }
        if m.barter_caps_change != tweaks.barter_caps_change {
            m.barter_caps_change = tweaks.barter_caps_change;
        }
        let menu = (tweaks.take_all_stays, tweaks.hide_equipped_barter, tweaks.hide_equipped_containers, tweaks.title_case_categories);
        if (m.take_all_stays, m.hide_equipped_barter, m.hide_equipped_containers, m.title_case_categories) != menu {
            (m.take_all_stays, m.hide_equipped_barter, m.hide_equipped_containers, m.title_case_categories) = menu;
        }
        let hotkeyed: Vec<fnv_sim::FormId> = hotkeys.as_deref().map(|h| h.0.iter().flatten().map(|s| fnv_sim::FormId(s.item)).collect()).unwrap_or_default();
        let hide = (tweaks.hide_hotkeyed_containers, tweaks.hide_hotkeyed_barter, tweaks.hide_hotkeyed_repair);
        if (m.hide_hotkeyed_containers, m.hide_hotkeyed_barter, m.hide_hotkeyed_repair) != hide || m.hotkeyed != hotkeyed {
            (m.hide_hotkeyed_containers, m.hide_hotkeyed_barter, m.hide_hotkeyed_repair) = hide;
            m.hotkeyed = hotkeyed;
        }
    }
    if let Some(mut c) = console {
        if (c.compress_repeats, c.timestamps) != (tweaks.console_compress_repeats, tweaks.console_timestamps) {
            c.compress_repeats = tweaks.console_compress_repeats;
            c.timestamps = tweaks.console_timestamps;
        }
        if c.homes_on != tweaks.console_homes {
            c.homes_on = tweaks.console_homes;
        }
        if c.remember_history != tweaks.remember_console_history {
            c.remember(tweaks.remember_console_history);
        }
        if tweaks.console_output_file && !c.auto_output_done {
            c.auto_output();
        }
    }
    if let Some(mut q) = quests {
        if q.hidden != hidden {
            q.hidden = hidden;
        }
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.tweaks != tweaks.game {
        info!("tweaks: the game takes {:?}", tweaks.game);
        rt.game.tweaks = tweaks.game.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_multiplier_slider_has_one_in_the_middle() {
        assert_eq!(factor(0.5), 1.0);
        assert_eq!(part(1.0), 0.5);
        assert_eq!(shown_factor(0.75), "x1.5");
        assert_eq!(part(9.0), 1.0, "brought into the slider");
    }

    #[test]
    fn the_special_slider_keeps_forty_and_its_ends() {
        assert_eq!(special_total(special_index(40)), 40);
        assert_eq!(special_total(0), SPECIAL_LOW);
        assert_eq!(special_total(SPECIALS - 1), 70);
        assert_eq!(special_index(5), 0, "below the slider");
        assert_eq!(special_index(99), SPECIALS - 1, "above it");
    }

    #[test]
    fn every_tweak_is_off_by_default() {
        let t = TweakSettings::default();
        assert_eq!(t.game, Tweaks::default());
        assert_eq!(t.game.xp_multiplier, 1.0);
        assert!(!t.no_karma_messages && !t.disable_fast_travel);
    }
}
