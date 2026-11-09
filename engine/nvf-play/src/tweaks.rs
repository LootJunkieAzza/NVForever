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
        // ---- rest ----
        switch!(app, "sleep-in-owned-beds", "Sleep in Owned Beds", "A bed that belongs to someone else can be slept in. (Stewie's Tweaks bSleepInOwnedBeds)", game.sleep_in_owned_beds);
        // ---- items ----
        switch!(app, "power-armor-no-training", "Power Armor Without Training", "Power armor can be worn without Power Armor Training. (Stewie's Tweaks bPowerArmorNeedsNoTraining)", game.power_armor_needs_no_training);
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

/// Hand the game's part to the running game when it differs, and log the change.
fn sync(tweaks: Res<TweakSettings>, mut script: Option<ResMut<ScriptState>>, quests: Option<ResMut<crate::quest_messages::QuestMessages>>, loading: Option<ResMut<crate::loading::Loading>>, console: Option<ResMut<crate::console::Console>>, mut active: Option<ResMut<crate::menu::ActiveMenu>>) {
    if let Some(mut l) = loading {
        if l.hide_tips != tweaks.no_loading_tips {
            l.hide_tips = tweaks.no_loading_tips;
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
        let menu = (tweaks.take_all_stays, tweaks.hide_equipped_barter, tweaks.hide_equipped_containers, tweaks.title_case_categories);
        if (m.take_all_stays, m.hide_equipped_barter, m.hide_equipped_containers, m.title_case_categories) != menu {
            (m.take_all_stays, m.hide_equipped_barter, m.hide_equipped_containers, m.title_case_categories) = menu;
        }
    }
    if let Some(mut c) = console {
        if (c.compress_repeats, c.timestamps) != (tweaks.console_compress_repeats, tweaks.console_timestamps) {
            c.compress_repeats = tweaks.console_compress_repeats;
            c.timestamps = tweaks.console_timestamps;
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
