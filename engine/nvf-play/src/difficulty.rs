//! The game's difficulty in play (docs/spec-notes/difficulty.md): a row of the settings
//! page's Gameplay group (kept in `NVForever.ini` as every row is) and the console's
//! `SetGameDifficulty` / `GetGameDifficulty`. The rules are `fnv_game::difficulty`'s;
//! this keeps the chosen level and hands it to the running game each frame (the game is
//! made after the settings are read, and again for a loaded save).
//!
//! The list holds the five levels and Custom; while Custom is chosen, three sliders under
//! it set the damage the player deals, the damage the player takes and the XP for a kill,
//! each 50-300% of Normal's in steps of 5% (100% until the player moves them), kept in the
//! INI like every row (`difficulty-player-damage`, `difficulty-npc-damage`,
//! `difficulty-kill-xp`) [design]. The sliders are greyed while another level is chosen
//! (`nvf_settings_layout::NEEDS_CHOICE`) and change nothing then.
//!
//! The list's note names every level's three multipliers ([`LEVELS_NOTE`]: the vanilla
//! masters' numbers, which the launcher shows); in a game the page fills the same words with
//! the load order's own numbers ([`levels_note_now`]), so a plugin's `fDiffMult*` shows too.
//!
//! A script's `SetGameDifficulty` changes the running game's level and asks for it to be
//! kept (`fnv_game::Game::difficulty_request`): [`sync`] takes that as the chosen level,
//! as the console's does, instead of handing its own back on the next frame (spec §4).

use crate::script::ScriptState;
use crate::settings::{add_choice_list, add_setting, choice_index, choice_of, ChoiceList, Setting, Slider};
use bevy::prelude::*;
use fnv_game::difficulty::{CustomDifficulty, Difficulty, CHOICES, CUSTOM_MAX, CUSTOM_MIN, LEVELS};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// The chosen level, for the process, as its place in [`CHOICES`]: the settings page, the
/// console and scripts set it.
static LEVEL: AtomicUsize = AtomicUsize::new(2);

/// The Custom level's multipliers (f32 bits): damage dealt, damage taken, kill XP; 100% each
/// until the player sets them.
static CUSTOM: [AtomicU32; 3] = [AtomicU32::new(1.0f32.to_bits()), AtomicU32::new(1.0f32.to_bits()), AtomicU32::new(1.0f32.to_bits())];

/// The Custom sliders' positions: 50% to 300% in steps of 5% [design].
pub const CUSTOM_STEPS: usize = 51;

/// The chosen level.
pub fn level() -> Difficulty {
    CHOICES[LEVEL.load(Ordering::Relaxed).min(CHOICES.len() - 1)]
}

/// Choose a level; the game takes it on its next frame.
pub fn set_level(d: Difficulty) {
    let i = CHOICES.iter().position(|c| *c == d).unwrap_or(2);
    LEVEL.store(i, Ordering::Relaxed);
}

/// The Custom level's multipliers as chosen.
pub fn custom() -> CustomDifficulty {
    let at = |i: usize| f64::from(f32::from_bits(CUSTOM[i].load(Ordering::Relaxed)));
    CustomDifficulty { dealt: at(0), taken: at(1), kill_xp: at(2) }
}

/// Set Custom multiplier `i` (0 dealt, 1 taken, 2 kill XP), brought into 50-300%.
pub fn set_custom(i: usize, mult: f64) {
    let m = mult.clamp(CUSTOM_MIN, CUSTOM_MAX) as f32;
    CUSTOM[i].store(m.to_bits(), Ordering::Relaxed);
}

/// A multiplier as its slider's part: 50% is 0, 300% is 1.
pub fn part_of(mult: f64) -> f32 {
    ((mult.clamp(CUSTOM_MIN, CUSTOM_MAX) - CUSTOM_MIN) / (CUSTOM_MAX - CUSTOM_MIN)) as f32
}

/// A slider's part as its multiplier, on a 5% step.
pub fn mult_of(part: f32) -> f64 {
    let pct = 50.0 + (f64::from(part.clamp(0.0, 1.0)) * 250.0 / 5.0).round() * 5.0;
    pct / 100.0
}

/// The words a Custom slider shows for its part (`150%`).
fn percent(part: f32) -> String {
    format!("{:.0}%", mult_of(part) * 100.0)
}

pub struct DifficultyPlugin;

impl Plugin for DifficultyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync.in_set(crate::PlaySet::Input));
        // A list of the five levels and Custom [design: the menu overhaul, owner 2026-11-15: a
        // row of 7 or fewer values is a list]; the INI's older slider number still loads.
        add_choice_list(
            app,
            Setting {
                key: "difficulty",
                group: crate::pause::GROUP_MENUS,
                label: "Difficulty",
                note: LEVELS_NOTE,
                switches: "",
                get: |_| false,
                set: |_, _| {},
                slider: None,
            },
            ChoiceList { names: |_| names(), chosen: |_| level().name().to_string(), pick: |_, n| take_level(n), save: |_| level().name().to_string(), load: |_, t| take_level(t), ask_keep: false },
        );
        // In a game, the note names the load order's own multipliers (module notes).
        crate::settings::set_note_now(app, "difficulty", levels_note_now);
        // Custom's three, shown only while it is chosen (nvf-settings-layout SHOWN_WITH).
        add_setting(
            app,
            Setting::slider(
                "difficulty-player-damage",
                crate::pause::GROUP_MENUS,
                "Player damage",
                "Custom difficulty: how much damage your hits do, from 50% to 300%.",
                Slider::choice(CUSTOM_STEPS, |_| part_of(custom().dealt), |_, v| set_custom(0, mult_of(v)), percent),
            ),
        );
        add_setting(
            app,
            Setting::slider(
                "difficulty-npc-damage",
                crate::pause::GROUP_MENUS,
                "NPC damage",
                "Custom difficulty: how much damage you take from NPCs and creatures (and falls and blasts), from 50% to 300%.",
                Slider::choice(CUSTOM_STEPS, |_| part_of(custom().taken), |_, v| set_custom(1, mult_of(v)), percent),
            ),
        );
        add_setting(
            app,
            Setting::slider(
                "difficulty-kill-xp",
                crate::pause::GROUP_MENUS,
                "Kill XP",
                "Custom difficulty: how much XP a kill earns, from 50% to 300%.",
                Slider::choice(CUSTOM_STEPS, |_| part_of(custom().kill_xp), |_, v| set_custom(2, mult_of(v)), percent),
            ),
        );
    }
}

/// The Difficulty row's note: every level's multipliers, as the original's masters give them
/// (`fnv_game::difficulty`: the damage defaults, FalloutNV.esm's kill XP of 1 at every level).
/// The launcher shows this; in a game the page shows [`levels_note_now`], the same words with
/// the load order's own numbers.
pub const LEVELS_NOTE: &str = "You deal / take / kill XP: Very Easy 200/50/100%, Easy 150/75/100%, Normal 100/100/100%, Hard 75/150/100%, Very Hard 50/200/100%.";

/// [`LEVELS_NOTE`] with `{levels}` for the five levels' numbers ([`levels_text`]).
const LEVELS_NOTE_NOW: &str = "You deal / take / kill XP: {levels}.";

/// The five levels' numbers as the note gives them: `Very Easy 200/50/100%, Easy …`, each
/// level's name as `name` gives it (the language shown).
fn levels_text(of: impl Fn(Difficulty) -> CustomDifficulty, name: impl Fn(Difficulty) -> String) -> String {
    let pct = |m: f64| format!("{:.0}", m * 100.0);
    LEVELS
        .iter()
        .map(|d| {
            let m = of(*d);
            format!("{} {}/{}/{}%", name(*d), pct(m.dealt), pct(m.taken), pct(m.kill_xp))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The note in a game: every level's multipliers as the load order sets them (a plugin's
/// `fDiffMult*` too); `None` before a game is running (the row's own note then).
fn levels_note_now(world: &World) -> Option<String> {
    let game = &world.get_resource::<ScriptState>()?.rt.as_ref()?.game;
    let levels = levels_text(|d| game.difficulty_multipliers(d), |d| fnv_util::words::tr(d.name()).to_string());
    Some(fnv_util::words::trf(LEVELS_NOTE_NOW, &[("levels", &levels)]))
}

/// The choices' names, in order.
fn names() -> Vec<String> {
    CHOICES.iter().map(|d| d.name().to_string()).collect()
}

/// The level `text` names (or an older slider's number stands for: one of the five);
/// false: none.
fn take_level(text: &str) -> bool {
    match choice_of(text, &names(), |v| choice_index(v, LEVELS.len())) {
        Some(i) => {
            set_level(CHOICES[i]);
            true
        }
        None => false,
    }
}

/// What [`sync`] does with the running game's level and Custom multipliers: a script's
/// request becomes the chosen level first (`Some` of it, to log), then the chosen level and
/// multipliers are handed over when they differ (`Some` of the game's line).
fn hand_over(game: &mut fnv_game::Game) -> (Option<Difficulty>, Option<String>) {
    let asked = game.difficulty_request.take();
    if let Some(d) = asked {
        set_level(d);
    }
    let (d, c) = (level(), custom());
    let line = (game.difficulty != d || game.difficulty_custom != c).then(|| {
        game.set_custom_difficulty(c);
        game.set_difficulty(d)
    });
    (asked, line)
}

/// Hand the chosen level to the game when they differ, and log the change; a script's
/// level is kept as the chosen one (module notes).
fn sync(mut script: Option<ResMut<ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let (asked, line) = hand_over(&mut rt.game);
    if let Some(d) = asked {
        info!("difficulty: a script chose {} ({}); kept as the chosen level", d.name(), d.code());
    }
    if let Some(line) = line {
        info!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sliders_reach_50_to_300_percent_in_fives() {
        assert_eq!((percent(0.0), percent(1.0)), ("50%".to_string(), "300%".to_string()));
        let s = Slider::choice(CUSTOM_STEPS, |_| 0.0, |_, _| {}, percent);
        let parts: Vec<f32> = (0..CUSTOM_STEPS).map(|i| s.fit(i as f32 * s.step)).collect();
        let shown: Vec<String> = parts.iter().map(|p| percent(*p)).collect();
        assert_eq!(shown.len(), 51);
        assert!(shown.iter().enumerate().all(|(i, t)| *t == format!("{}%", 50 + 5 * i)), "{shown:?}");
        // 100% (the default) is a step the keys reach, and a part reads back as its multiplier.
        assert_eq!(percent(s.fit(part_of(1.0))), "100%");
        for m in [0.5, 1.0, 1.25, 2.0, 3.0] {
            assert_eq!(mult_of(s.fit(part_of(m))), m);
        }
    }

    /// The shared rules that grey the three sliders (`settings_layout::NEEDS_CHOICE`) name
    /// the list's Custom, and no rule hides them.
    #[test]
    fn the_sliders_are_greyed_unless_custom() {
        let names: Vec<&str> = CHOICES.iter().map(|d| d.name()).collect();
        for key in ["difficulty-player-damage", "difficulty-npc-damage", "difficulty-kill-xp"] {
            let rules = crate::settings_layout::needs(key);
            assert_eq!(rules.iter().map(|r| (r.parent, r.choices)).collect::<Vec<_>>(), [("difficulty", &["Custom"][..])], "{key}");
            assert!(rules[0].choices.iter().all(|c| names.contains(c)), "{key}");
            assert_eq!(crate::settings_layout::shown_with(key), None, "{key}: greyed, not hidden");
        }
    }

    /// The row's own note is the game's note filled with the vanilla masters' numbers (the
    /// damage defaults, kill XP 1 at every level), so the launcher's words are the game's.
    #[test]
    fn the_note_names_every_levels_multipliers() {
        let vanilla = |d: Difficulty| CustomDifficulty { dealt: fnv_game::difficulty::BY_PC[d.code()], taken: fnv_game::difficulty::TO_PC[d.code()], kill_xp: 1.0 };
        let filled = fnv_util::words::fill(LEVELS_NOTE_NOW, &[("levels", &levels_text(vanilla, |d| d.name().to_string()))]);
        assert_eq!(filled, LEVELS_NOTE);
        assert!(crate::settings::note_over_budget(LEVELS_NOTE).is_none());
    }

    #[test]
    fn older_numbers_load_as_one_of_the_five_and_names_as_any() {
        let before = LEVEL.load(Ordering::Relaxed);
        assert!(take_level("1.0"));
        assert_eq!(level(), Difficulty::VeryHard, "an older slider's 1.0: Very Hard, never Custom");
        assert!(take_level("custom"));
        assert_eq!(level(), Difficulty::Custom);
        assert!(!take_level("Impossible"));
        LEVEL.store(before, Ordering::Relaxed);
    }
}
