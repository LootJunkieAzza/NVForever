//! Detection: how well one actor (the detector) perceives another (the target), as a
//! number, and whether that number makes the target hidden, noticed or detected
//! (docs/spec-notes/actor-behaviour.md §2, docs/design/detection.md).
//!
//! The value is the editor documentation's formula [documented: GECK wiki
//! Category:Detection, owner export 2026-10-09]:
//!
//! ```text
//! D = fSneakBaseValue + Sound + Visual + fSneakSkillMult · (DetectorSkill − TargetSkill)
//! ```
//!
//! - every term but the target's skill is multiplied by the distance falloff
//!   `((R − d) / R) ^ fSneakDistanceAttenuationExponent`, `R` = fSneakMaxDistance (times
//!   fSneakExteriorDistanceMult outdoors);
//! - **Sound** = falloff · fSneakSoundsMult · (movement + action), the falloff further ×
//!   fSneakSoundLosMult without line of sight; movement is 0 standing still, else
//!   (fSneakBootWeightBase + fSneakBootWeightMult · body armour weight) × fSneakRunningMult
//!   when running; action = the loudest action sound × fSneakActionMult;
//! - **Visual** = falloff · seen · light · movement multiplier · blindness · stealth boy:
//!   light = (fDetectionSneakLightMod + light level × night eye) · fSneakLightMult, night
//!   eye = fDetectionNightEyeBonus (the product capped at 100) when the detector has Night
//!   Eye; movement multiplier = 1 + moving · fSneakLightMoveMult + running ·
//!   fSneakLightRunMult; blindness is always 1 in New Vegas; stealth boy =
//!   fSneakStealthboyMult when the target's Invisibility or Chameleon is at least 1;
//! - **Detector skill** = falloff · Perception skill · (1 + alert · fSneakAlertMod + asleep
//!   · fSneakSleepBonus + busy · fSneakCombatMod), Perception skill = fSneakPerceptionSkillMin
//!   + (Max − Min) · Perception / 10;
//! - **Target skill** (New Vegas) = sneaking · (Sneak + iSneakLevelBonus · (target level −
//!   detector level) + max(iSneakStartBonus − iSneakStartBonusLevelPenatly · target level,
//!   0) − armour penalty), the penalty iMediumArmorStealthPenalty or
//!   iHeavyArmorStealthPenalty for medium or heavy armour in the torso slot.
//!
//! Thresholds [documented: same page]: above 0 the target is detected; above
//! fSneakNoticedMin (−20) it is noticed (the alert threshold); else it is hidden.
//!
//! The model is split so it can be tested without a world: [`Settings`] (the game
//! settings, read once), [`Pair`] (what the two actors' records and actor values say,
//! [`Game::detection_pair`]) and [`Situation`] (what the world shows this moment: distance,
//! sight, light, movement; the engine's to measure). [`Detections`] holds the latest
//! value of each pair for the script functions.

use crate::actor_values::{capped, PERCEPTION, SNEAK};
use crate::combat::setting;
use crate::functions::actor::actor_value;
use crate::functions::{Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::forms::{ArmorClass, ItemKind};
use fnv_sim::FormId;
use std::collections::BTreeMap;

pub const INVISIBILITY: u16 = 48;
pub const CHAMELEON: u16 = 49;
pub const NIGHT_EYE: u16 = 50;

/// The biped slot of body armour, "Upper Body" [documented: xEdit wbBMDT, the torso slot
/// the Target Skill's armour penalty reads].
pub const UPPER_BODY: u32 = 1 << 2;

/// The detection game settings, with the defaults the editor documentation gives
/// [documented: GECK wiki Category:Detection]. The masters may set other values; the
/// game's are read where they exist.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub base_value: f64,
    pub skill_mult: f64,
    pub max_distance: f64,
    pub exterior_distance_mult: f64,
    pub attenuation_exponent: f64,
    pub sound_los_mult: f64,
    pub boot_weight_base: f64,
    pub boot_weight_mult: f64,
    pub running_mult: f64,
    pub action_mult: f64,
    pub sounds_mult: f64,
    pub light_move_mult: f64,
    pub light_run_mult: f64,
    pub light_mod: f64,
    pub light_mult: f64,
    pub stealthboy_mult: f64,
    pub night_eye_bonus: f64,
    pub perception_skill_min: f64,
    pub perception_skill_max: f64,
    pub alert_mod: f64,
    pub sleep_bonus: f64,
    pub combat_mod: f64,
    pub level_bonus: f64,
    pub start_bonus: f64,
    pub start_bonus_level_penalty: f64,
    pub medium_armor_penalty: f64,
    pub heavy_armor_penalty: f64,
    /// The alert threshold, fSneakNoticedMin.
    pub noticed_min: f64,
    /// The firing sound levels, iSoundLevelLoud, Normal and Silent.
    pub sound_loud: f64,
    pub sound_normal: f64,
    pub sound_silent: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            base_value: -35.0,
            skill_mult: 0.5,
            max_distance: 2500.0,
            exterior_distance_mult: 2.0,
            attenuation_exponent: 2.0,
            sound_los_mult: 0.1,
            boot_weight_base: 12.0,
            boot_weight_mult: 0.5,
            running_mult: 1.5,
            action_mult: 2.0,
            sounds_mult: 1.6,
            light_move_mult: 0.01,
            light_run_mult: 0.2,
            light_mod: 0.0,
            light_mult: 1.4,
            stealthboy_mult: 0.0,
            night_eye_bonus: 3.0,
            perception_skill_min: 10.0,
            perception_skill_max: 90.0,
            alert_mod: 0.2,
            sleep_bonus: -0.2,
            combat_mod: -0.4,
            level_bonus: 5.0,
            start_bonus: 50.0,
            start_bonus_level_penalty: 10.0,
            medium_armor_penalty: 10.0,
            heavy_armor_penalty: 20.0,
            noticed_min: -20.0,
            sound_loud: 100.0,
            sound_normal: 50.0,
            sound_silent: 10.0,
        }
    }
}

impl Settings {
    /// The settings as the loaded masters have them, the documented default for each one
    /// they lack.
    pub fn read(g: &Game) -> Settings {
        let d = Settings::default();
        let s = |name: &str, default: f64| setting(g, name, default);
        Settings {
            base_value: s("fSneakBaseValue", d.base_value),
            skill_mult: s("fSneakSkillMult", d.skill_mult),
            max_distance: s("fSneakMaxDistance", d.max_distance),
            exterior_distance_mult: s("fSneakExteriorDistanceMult", d.exterior_distance_mult),
            attenuation_exponent: s("fSneakDistanceAttenuationExponent", d.attenuation_exponent),
            sound_los_mult: s("fSneakSoundLosMult", d.sound_los_mult),
            boot_weight_base: s("fSneakBootWeightBase", d.boot_weight_base),
            boot_weight_mult: s("fSneakBootWeightMult", d.boot_weight_mult),
            running_mult: s("fSneakRunningMult", d.running_mult),
            action_mult: s("fSneakActionMult", d.action_mult),
            sounds_mult: s("fSneakSoundsMult", d.sounds_mult),
            light_move_mult: s("fSneakLightMoveMult", d.light_move_mult),
            light_run_mult: s("fSneakLightRunMult", d.light_run_mult),
            light_mod: s("fDetectionSneakLightMod", d.light_mod),
            light_mult: s("fSneakLightMult", d.light_mult),
            stealthboy_mult: s("fSneakStealthboyMult", d.stealthboy_mult),
            night_eye_bonus: s("fDetectionNightEyeBonus", d.night_eye_bonus),
            perception_skill_min: s("fSneakPerceptionSkillMin", d.perception_skill_min),
            perception_skill_max: s("fSneakPerceptionSkillMax", d.perception_skill_max),
            alert_mod: s("fSneakAlertMod", d.alert_mod),
            sleep_bonus: s("fSneakSleepBonus", d.sleep_bonus),
            combat_mod: s("fSneakCombatMod", d.combat_mod),
            level_bonus: s("iSneakLevelBonus", d.level_bonus),
            start_bonus: s("iSneakStartBonus", d.start_bonus),
            // The setting's name is spelled so [documented: GECK wiki Category:Detection].
            start_bonus_level_penalty: s("iSneakStartBonusLevelPenatly", d.start_bonus_level_penalty),
            medium_armor_penalty: s("iMediumArmorStealthPenalty", d.medium_armor_penalty),
            heavy_armor_penalty: s("iHeavyArmorStealthPenalty", d.heavy_armor_penalty),
            noticed_min: s("fSneakNoticedMin", d.noticed_min),
            sound_loud: s("iSoundLevelLoud", d.sound_loud),
            sound_normal: s("iSoundLevelNormal", d.sound_normal),
            sound_silent: s("iSoundLevelSilent", d.sound_silent),
        }
    }

    /// The farthest a detector perceives anything (fSneakMaxDistance, × the exterior
    /// multiplier outdoors).
    pub fn max_distance(&self, exterior: bool) -> f64 {
        self.max_distance * if exterior { self.exterior_distance_mult } else { 1.0 }
    }

    /// The distance falloff every term but the target's skill takes, 0 at and beyond the
    /// maximum distance.
    pub fn attenuation(&self, distance: f64, exterior: bool) -> f64 {
        let r = self.max_distance(exterior);
        if r <= 0.0 || distance >= r {
            return 0.0;
        }
        ((r - distance.max(0.0)) / r).powf(self.attenuation_exponent)
    }

    /// The action sound of firing a weapon of this WEAP sound level ("Loud", "Normal",
    /// "Silent"); 0 for anything else.
    pub fn firing_sound(&self, level: &str) -> f64 {
        match level.to_ascii_lowercase().as_str() {
            "loud" => self.sound_loud,
            "normal" => self.sound_normal,
            "silent" => self.sound_silent,
            _ => 0.0,
        }
    }

    /// The band a detection value falls in.
    pub fn band(&self, value: f64) -> Band {
        if value > 0.0 {
            Band::Detected
        } else if value > self.noticed_min {
            Band::Noticed
        } else {
            Band::Hidden
        }
    }

    /// The detection value of a pair in a situation, with its parts.
    pub fn evaluate(&self, p: &Pair, s: &Situation) -> Detection {
        let a = self.attenuation(s.distance, s.exterior);
        let moving = s.moving || s.running;

        // Sound.
        let movement = if moving {
            let running = if s.running && !p.ignore_running { self.running_mult } else { 1.0 };
            (self.boot_weight_base + self.boot_weight_mult * p.target_armor_weight) * running
        } else {
            0.0
        };
        let action = s.action_sound * self.action_mult;
        let sound_falloff = if s.line_of_sight { a } else { a * self.sound_los_mult };
        let sound = sound_falloff * self.sounds_mult * (movement + action);

        // Visual.
        let seen = s.line_of_sight && s.in_view;
        let light_level = if p.detector_night_eye { (s.light * self.night_eye_bonus).min(100.0) } else { s.light };
        let light = (self.light_mod + light_level) * self.light_mult;
        let motion = 1.0 + if moving { self.light_move_mult } else { 0.0 } + if s.running { self.light_run_mult } else { 0.0 };
        let stealth_boy = if p.target_invisible { self.stealthboy_mult } else { 1.0 };
        let visual = if seen { a * light * motion * stealth_boy } else { 0.0 };

        // Skills.
        let perception_skill =
            self.perception_skill_min + (self.perception_skill_max - self.perception_skill_min) * p.detector_perception / 10.0;
        let state = 1.0
            + if s.detector_alert { self.alert_mod } else { 0.0 }
            + if s.detector_asleep { self.sleep_bonus } else { 0.0 }
            + if s.detector_busy { self.combat_mod } else { 0.0 };
        let detector_skill = a * perception_skill * state;
        let target_skill = if s.sneaking {
            let penalty = match p.target_armor_class {
                ArmorClass::Heavy => self.heavy_armor_penalty,
                ArmorClass::Medium => self.medium_armor_penalty,
                ArmorClass::Light => 0.0,
            };
            p.target_sneak
                + self.level_bonus * (p.target_level - p.detector_level)
                + (self.start_bonus - self.start_bonus_level_penalty * p.target_level).max(0.0)
                - penalty
        } else {
            0.0
        };

        let value = self.base_value + sound + visual + self.skill_mult * (detector_skill - target_skill);
        Detection { value, band: self.band(value), attenuation: a, sound, visual, detector_skill, target_skill, seen: s.camera_sight.unwrap_or(seen), clear: s.line_of_sight }
    }
}

/// What the two actors' records and actor values say, for one detector and one target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// The detector's Perception, 1–10.
    pub detector_perception: f64,
    pub detector_level: f64,
    /// The detector's Night Eye actor value is not 0.
    pub detector_night_eye: bool,
    /// The target's Sneak, 0–100.
    pub target_sneak: f64,
    pub target_level: f64,
    /// The target's Invisibility or Chameleon is at least 1.
    pub target_invisible: bool,
    /// The weight and class of the armour in the target's torso slot (0 and light with
    /// none).
    pub target_armor_weight: f64,
    pub target_armor_class: ArmorClass,
    /// The "Ignore Running During Detection" perk entry point applies to the target.
    /// Perk entry points are not modelled yet, so this is always false.
    pub ignore_running: bool,
}

impl Default for Pair {
    fn default() -> Pair {
        Pair {
            detector_perception: 5.0,
            detector_level: 1.0,
            detector_night_eye: false,
            target_sneak: 0.0,
            target_level: 1.0,
            target_invisible: false,
            target_armor_weight: 0.0,
            target_armor_class: ArmorClass::Light,
            ignore_running: false,
        }
    }
}

/// What the world shows of one detector and one target at one moment; the engine
/// measures it (distance, sight, light) and knows the actors' movement and state.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Situation {
    /// Distance between the two, in game units.
    pub distance: f64,
    /// Both are in an exterior (the longer maximum distance).
    pub exterior: bool,
    /// Nothing solid between the detector's eyes and the target.
    pub line_of_sight: bool,
    /// The target is within the detector's field of view.
    pub in_view: bool,
    /// The light level at the target, 0–100.
    pub light: f64,
    pub moving: bool,
    pub running: bool,
    pub sneaking: bool,
    /// The loudest action sound the target made this moment (a shot: 100, 50 or 10).
    pub action_sound: f64,
    /// The detector is in a combat state (Alert, Combat or Lost).
    pub detector_alert: bool,
    pub detector_asleep: bool,
    /// The detector is fighting someone other than the target.
    pub detector_busy: bool,
    /// What GetLineOfSight reads when it differs from the sight above: the player's is the
    /// camera's, its detection the body's [observed: capture detection-2 det4, 2026-10-17,
    /// third person: player.GetDetected Ringo 1 to 90° off the facing on both sides, 0
    /// from 100°; player.GetLineOfSight 0 at 90° left but 1 to 110° right].
    pub camera_sight: Option<bool>,
}

/// Where a detection value falls against the two thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Band {
    /// At or below fSneakNoticedMin (−20).
    #[default]
    Hidden,
    /// Above fSneakNoticedMin, at or below 0: the alert band.
    Noticed,
    /// Above 0.
    Detected,
}

impl Band {
    pub fn name(self) -> &'static str {
        match self {
            Band::Hidden => "hidden",
            Band::Noticed => "noticed",
            Band::Detected => "detected",
        }
    }

    /// The word of the original's sneak meter for this band: HIDDEN, CAUTION, DANGER
    /// [hypothesis: the meter shows the highest band any actor has the player in, with
    /// these bands; not checked against a capture].
    pub fn meter(self) -> &'static str {
        match self {
            Band::Hidden => "HIDDEN",
            Band::Noticed => "CAUTION",
            Band::Detected => "DANGER",
        }
    }
}

/// A detection value and its parts.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Detection {
    pub value: f64,
    pub band: Band,
    /// The distance falloff.
    pub attenuation: f64,
    pub sound: f64,
    pub visual: f64,
    pub detector_skill: f64,
    pub target_skill: f64,
    /// The detector could see the target (line of sight and field of view; for the player
    /// the camera's, `Situation::camera_sight`): what GetLineOfSight reads.
    pub seen: bool,
    /// Nothing solid between the detector's eyes and the target (walls, closed doors),
    /// whichever way the detector faces: `Situation::line_of_sight`. A force greet needs
    /// it ([`Game::greet_sight`]).
    pub clear: bool,
}

/// Seconds of game time an actor the player woke stays up ([`Game::wake`]) [our choice].
pub const WAKE_SECONDS: f64 = 30.0;

/// The latest detection of each (detector, target) pair the engine measured, and when.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Detections {
    pub pairs: BTreeMap<(FormId, FormId), (Detection, f64)>,
    /// The engine measures the actors' sight of the player (set by the engine at each
    /// measurement; off without one, and under the `tdetect` toggle or the test aid
    /// that leaves the player unseen, where no pair to the player is measured).
    pub measures_player: bool,
}

impl Detections {
    /// Record a pair's detection at game time `time` (seconds); returns the band it had
    /// before, `None` when the pair is new.
    pub fn record(&mut self, detector: FormId, target: FormId, d: Detection, time: f64) -> Option<Band> {
        self.pairs.insert((detector, target), (d, time)).map(|(old, _)| old.band)
    }

    pub fn get(&self, detector: FormId, target: FormId) -> Option<&Detection> {
        self.pairs.get(&(detector, target)).map(|(d, _)| d)
    }

    /// The highest band any detector has the target in (the sneak meter's band for the
    /// player), with that detector; `None` when no one measured it.
    pub fn highest(&self, target: FormId) -> Option<(FormId, Detection)> {
        self.pairs
            .iter()
            .filter(|((_, t), _)| *t == target)
            .max_by(|a, b| a.1 .0.value.total_cmp(&b.1 .0.value))
            .map(|((d, _), (det, _))| (*d, *det))
    }

    /// Forget every pair one of whose actors is `r` (it unloaded, died or was disabled).
    pub fn forget(&mut self, r: FormId) {
        self.pairs.retain(|(d, t), _| *d != r && *t != r);
    }

    /// Forget pairs not measured since `time`.
    pub fn forget_older_than(&mut self, time: f64) {
        self.pairs.retain(|_, (_, t)| *t >= time);
    }
}

impl Game<'_> {
    /// Whether `actor` may force-greet `target` as far as sight goes: nothing solid
    /// (a wall, a closed door) between them at the engine's last measurement of the pair
    /// [owner statement 2026-10-23, playtest list 7 items 41/42: Davison and Harland must
    /// not start a conversation through a closed door or a wall; hypothesis: the
    /// original asks for a clear line, not for the speaker's field of view, the line
    /// being what the owner names]. A pair the engine has not measured blocks the greet
    /// while the engine measures the player at all (the pair comes within a fraction of
    /// a second); with no measuring (headless runs, `tdetect`, the unseen test aid) sight
    /// does not hold anyone back [engineering choice].
    pub fn greet_sight(&self, actor: FormId, target: FormId) -> bool {
        match self.detection.get(actor, target) {
            Some(d) => d.clear,
            None => !self.detection.measures_player,
        }
    }

    /// What the records and actor values say for `detector` perceiving `target`.
    pub fn detection_pair(&mut self, detector: FormId, target: FormId) -> Pair {
        let av = |g: &mut Game, r: FormId, code: u16| actor_value(g, r, code).unwrap_or(0.0);
        let detector_perception = capped(PERCEPTION, av(self, detector, PERCEPTION));
        let detector_night_eye = av(self, detector, NIGHT_EYE) != 0.0;
        let target_sneak = capped(SNEAK, av(self, target, SNEAK));
        let target_invisible = av(self, target, INVISIBILITY) >= 1.0 || av(self, target, CHAMELEON) >= 1.0;
        let (target_armor_weight, target_armor_class) = self.body_armor(target).unwrap_or((0.0, ArmorClass::Light));
        Pair {
            detector_perception,
            detector_level: self.actor_level(detector),
            detector_night_eye,
            target_sneak,
            target_level: self.actor_level(target),
            target_invisible,
            target_armor_weight,
            target_armor_class,
            ignore_running: false,
        }
    }

    /// The weight and class of the armour an actor wears in the torso slot.
    pub fn body_armor(&mut self, actor: FormId) -> Option<(f64, ArmorClass)> {
        let forms = self.world.forms;
        let inv = self.world.get(actor).ok()?.inventory.as_ref()?;
        inv.equipped().into_iter().find_map(|i| {
            let it = forms.item(i)?;
            (it.kind == ItemKind::Armor && it.slots.is_some_and(|s| s & UPPER_BODY != 0))
                .then(|| (it.weight.unwrap_or(0.0) as f64, it.armor_class))
        })
    }

    /// The player talks to `actor`, asleep in its bed: it wakes and gets up first, and
    /// stays up for [`WAKE_SECONDS`] of game time (the conversation's own time not
    /// counted: the game is held then), after which its package takes it back to bed
    /// (`fnv_ai`) [owner's playtest list 7 item 38: "talking to a sleeping npc should get
    /// them out of their bed/sleep animation", as the original does; the time awake is
    /// our choice].
    pub fn wake(&mut self, actor: FormId) {
        let until = self.time + WAKE_SECONDS;
        self.extra.entry(actor).or_default().woken_until = Some(until);
    }

    /// Whether `actor` was woken by the player lately ([`Game::wake`]) and stays up.
    pub fn woken(&self, actor: FormId) -> bool {
        self.extra.get(&actor).and_then(|e| e.woken_until).is_some_and(|t| self.time < t)
    }

    /// The actor lies on a bed (furniture marker 1–10, on it, not getting on or off): it
    /// detects as a sleeper [hypothesis: lying on a bed is what "sleeping" means to the
    /// detector skill].
    pub fn asleep(&self, actor: FormId) -> bool {
        self.furniture_use(actor).is_some_and(|u| crate::functions::furniture::bed(&u) && u.state == 3)
    }

    /// The detection of `target` by `detector` in situation `s`, recorded for the script
    /// functions at the current game time. An enemy of a ghost does not detect it
    /// ([`Game::ghost_hidden_from`]): the value is held at the top of the Hidden band
    /// [engineering choice: the note says only that it is not detected], sight
    /// (GetLineOfSight) as measured.
    pub fn detect(&mut self, settings: &Settings, detector: FormId, target: FormId, s: &Situation) -> (Detection, Option<Band>) {
        let p = self.detection_pair(detector, target);
        let mut d = settings.evaluate(&p, s);
        if self.ghost_hidden_from(detector, target) {
            d.value = d.value.min(settings.noticed_min);
            d.band = Band::Hidden;
        }
        let before = self.detection.record(detector, target, d, self.time);
        (d, before)
    }
}

/// `detector.GetDetected target`: 1 when the calling actor detects the target
/// [documented: catalogue notes], that is when its latest detection value is above 0; 0
/// for a pair the engine has not measured (an actor out of reach or not loaded)
/// [engineering choice]. The player is a detector too, measured from the camera with the
/// same formula (its Perception against the target's Sneak) [observed: player.GetDetected
/// Ringo 1 and Chet 1, function probes fp1/fp2 2026-10-17, also with `tdetect` on;
/// hypothesis: the same formula].
pub fn get_detected(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let detector = ctx.subject()?;
    let target = a.form(0)?;
    let d = g.detection.get(detector, target).is_some_and(|d| d.band == Band::Detected);
    Ok(Value::Num(if d { 1.0 } else { 0.0 }))
}

/// `detector.GetDetectionLevel target`: 0 lost, 1 unseen, 2 noticed, 3 seen
/// [documented: catalogue notes]. [hypothesis: unseen, noticed and seen are the hidden,
/// noticed and detected bands; "lost" is a pair not measured. The catalogue notes that
/// the original returns only 0 for the player as the target; NVForever does not copy
/// that.]
pub fn get_detection_level(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let detector = ctx.subject()?;
    let target = a.form(0)?;
    let level = match g.detection.get(detector, target).map(|d| d.band) {
        None => 0.0,
        Some(Band::Hidden) => 1.0,
        Some(Band::Noticed) => 2.0,
        Some(Band::Detected) => 3.0,
    };
    Ok(Value::Num(level))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// An NPC of Perception 5, level 1, 500 units from a level 1 player outdoors.
    fn outdoors(light: f64) -> Situation {
        Situation { distance: 500.0, exterior: true, line_of_sight: true, in_view: true, light, ..Default::default() }
    }

    #[test]
    fn falloff_reaches_zero_at_the_maximum_distance() {
        let s = Settings::default();
        assert!(near(s.attenuation(0.0, false), 1.0));
        assert!(near(s.attenuation(1250.0, false), 0.25));
        assert!(near(s.attenuation(2500.0, true), 0.25));
        assert_eq!(s.attenuation(2500.0, false), 0.0);
        assert_eq!(s.attenuation(9000.0, true), 0.0);
    }

    #[test]
    fn a_player_standing_in_the_light_is_detected() {
        let s = Settings::default();
        let d = s.evaluate(&Pair::default(), &outdoors(50.0));
        // a = (4500/5000)² = 0.81; visual 0.81 · 50 · 1.4 = 56.7; skill 0.81 · 50 = 40.5.
        assert!(near(d.attenuation, 0.81));
        assert!(near(d.visual, 56.7));
        assert!(near(d.detector_skill, 40.5));
        assert!(near(d.value, -35.0 + 56.7 + 0.5 * 40.5));
        assert_eq!(d.band, Band::Detected);
    }

    #[test]
    fn sneaking_in_the_dark_hides_and_the_dark_alone_is_noticed() {
        let s = Settings::default();
        let p = Pair { target_sneak: 15.0, ..Pair::default() };
        let dark = outdoors(5.0);
        let standing = s.evaluate(&p, &dark);
        assert!(near(standing.value, -35.0 + 0.81 * 7.0 + 0.5 * 40.5));
        assert_eq!(standing.band, Band::Noticed);
        let sneaking = s.evaluate(&p, &Situation { sneaking: true, ..dark });
        // Target skill: 15 + 5 · 0 + max(50 − 10, 0) = 55.
        assert!(near(sneaking.target_skill, 55.0));
        assert_eq!(sneaking.band, Band::Hidden);
    }

    #[test]
    fn running_is_louder_and_more_visible_than_walking() {
        let s = Settings::default();
        let p = Pair { target_armor_weight: 20.0, ..Pair::default() };
        let walk = s.evaluate(&p, &Situation { moving: true, ..outdoors(50.0) });
        let run = s.evaluate(&p, &Situation { running: true, ..outdoors(50.0) });
        // Movement sound (12 + 0.5 · 20) = 22, × 1.5 running; × 0.81 · 1.6.
        assert!(near(walk.sound, 0.81 * 1.6 * 22.0));
        assert!(near(run.sound, 0.81 * 1.6 * 33.0));
        assert!(near(walk.visual, 56.7 * 1.01));
        assert!(near(run.visual, 56.7 * 1.21));
    }

    #[test]
    fn walls_stop_sight_and_muffle_sound() {
        let s = Settings::default();
        let open = Situation { moving: true, ..outdoors(50.0) };
        let wall = Situation { line_of_sight: false, ..open };
        let a = s.evaluate(&Pair::default(), &open);
        let b = s.evaluate(&Pair::default(), &wall);
        assert_eq!(b.visual, 0.0);
        assert!(!b.seen);
        assert!(near(b.sound, a.sound * 0.1));
        let behind = s.evaluate(&Pair::default(), &Situation { in_view: false, ..open });
        assert_eq!(behind.visual, 0.0);
        assert!(near(behind.sound, a.sound));
    }

    #[test]
    fn a_shot_is_heard_through_walls_nearby() {
        let s = Settings::default();
        let p = Pair::default();
        // Sneaking at Sneak 0 behind a wall indoors: −35 + 0.5 · (0.7744 · 50 − 40).
        let quiet = Situation { distance: 300.0, sneaking: true, ..Default::default() };
        assert_eq!(s.evaluate(&p, &quiet).band, Band::Hidden);
        let shot = Situation { action_sound: s.firing_sound("Loud"), ..quiet };
        // a = (2200/2500)² = 0.7744; sound 0.7744 · 0.1 · 1.6 · 200 ≈ 24.8.
        let d = s.evaluate(&p, &shot);
        assert!(near(d.sound, 0.7744 * 0.1 * 1.6 * 200.0));
        assert_eq!(d.band, Band::Noticed);
    }

    #[test]
    fn levels_armour_and_the_start_bonus_change_the_target_skill() {
        let s = Settings::default();
        let sneak = Situation { sneaking: true, ..outdoors(50.0) };
        let low = Pair { target_sneak: 30.0, target_level: 3.0, detector_level: 1.0, ..Pair::default() };
        // 30 + 5 · 2 + max(50 − 30, 0) = 60.
        assert!(near(s.evaluate(&low, &sneak).target_skill, 60.0));
        let high = Pair { target_level: 8.0, detector_level: 10.0, ..low };
        // 30 + 5 · (−2) + 0 = 20.
        assert!(near(s.evaluate(&high, &sneak).target_skill, 20.0));
        let heavy = Pair { target_armor_class: ArmorClass::Heavy, ..high };
        assert!(near(s.evaluate(&heavy, &sneak).target_skill, 0.0));
        let medium = Pair { target_armor_class: ArmorClass::Medium, ..high };
        assert!(near(s.evaluate(&medium, &sneak).target_skill, 10.0));
        // Not sneaking: no target skill at all.
        assert_eq!(s.evaluate(&low, &outdoors(50.0)).target_skill, 0.0);
    }

    #[test]
    fn perception_night_eye_stealth_boys_and_the_detectors_state() {
        let s = Settings::default();
        let sit = outdoors(40.0);
        let sharp = Pair { detector_perception: 10.0, ..Pair::default() };
        assert!(near(s.evaluate(&sharp, &sit).detector_skill, 0.81 * 90.0));
        let dull = Pair { detector_perception: 1.0, ..Pair::default() };
        assert!(near(s.evaluate(&dull, &sit).detector_skill, 0.81 * 18.0));
        let eye = Pair { detector_night_eye: true, ..Pair::default() };
        // 40 · 3 capped at 100.
        assert!(near(s.evaluate(&eye, &sit).visual, 0.81 * 100.0 * 1.4));
        let boy = Pair { target_invisible: true, ..Pair::default() };
        assert_eq!(s.evaluate(&boy, &sit).visual, 0.0);
        let asleep = s.evaluate(&Pair::default(), &Situation { detector_asleep: true, ..sit });
        assert!(near(asleep.detector_skill, 0.81 * 50.0 * 0.8));
        let alert_busy = s.evaluate(&Pair::default(), &Situation { detector_alert: true, detector_busy: true, ..sit });
        assert!(near(alert_busy.detector_skill, 0.81 * 50.0 * 0.8));
    }

    #[test]
    fn beyond_the_maximum_distance_only_the_target_skill_counts() {
        let s = Settings::default();
        let far = Situation { distance: 6000.0, ..outdoors(100.0) };
        let d = s.evaluate(&Pair::default(), &far);
        assert_eq!(d.value, -35.0);
        assert_eq!(d.band, Band::Hidden);
    }

    #[test]
    fn bands_follow_the_thresholds() {
        let s = Settings::default();
        assert_eq!(s.band(0.5), Band::Detected);
        assert_eq!(s.band(0.0), Band::Noticed);
        assert_eq!(s.band(-19.9), Band::Noticed);
        assert_eq!(s.band(-20.0), Band::Hidden);
        assert_eq!(Band::Noticed.meter(), "CAUTION");
    }

    #[test]
    fn the_table_keeps_the_latest_value_and_the_highest_band() {
        let mut t = Detections::default();
        let (a, b, p) = (FormId(0x100), FormId(0x200), FormId::PLAYER_REF);
        let hid = Detection { value: -30.0, band: Band::Hidden, ..Default::default() };
        let seen = Detection { value: 12.0, band: Band::Detected, ..Default::default() };
        assert_eq!(t.record(a, p, hid, 1.0), None);
        assert_eq!(t.record(b, p, seen, 1.0), None);
        assert_eq!(t.highest(p).map(|(d, x)| (d, x.band)), Some((b, Band::Detected)));
        assert_eq!(t.record(b, p, hid, 2.0), Some(Band::Detected));
        t.forget_older_than(1.5);
        assert!(t.get(a, p).is_none() && t.get(b, p).is_some());
        t.forget(b);
        assert!(t.pairs.is_empty());
    }
}
