//! Clean-room support for `ManOnFireNVSE.ini` actor fire-spread mechanics
//! (`docs/spec-notes/ext-small.md` §5).
//!
//! Configuration is read lazily from `Data\Config\ManOnFireNVSE.ini` [documented:
//! Man On Fire NVSE v1.0.1 (`https://www.nexusmods.com/newvegas/mods/94962`), shipped
//! `Config\ManOnFireNVSE.ini` installed to `Data\Config\ManOnFireNVSE.ini`], falling
//! back to `Data\NVSE\Plugins\ManOnFireNVSE.ini` [design: NVForever fallback lookup
//! path; not in the shipped v1.0.1 release]. Only the public Nexus Mods page and the
//! shipped INI file and comments were consulted; the plugin's DLL and source code were
//! never consulted. When neither file is present (unmodded vanilla), no fire-spread
//! logic runs and combat behaviour is unchanged [design].

use std::collections::BTreeMap;

use fnv_sim::FormId;

use crate::combat_lines::FIRE_RESISTANCE;
use crate::data::MagicEffectDef;
use crate::game::{ActiveEffect, Game};

/// Primary Data-relative INI path [documented: Man On Fire NVSE v1.0.1
/// (`https://www.nexusmods.com/newvegas/mods/94962`), archive install path
/// `Data\Config\ManOnFireNVSE.ini`].
pub const PRIMARY_INI_PATH: &str = "Config\\ManOnFireNVSE.ini";
/// Fallback Data-relative INI path when the primary path is absent [design: NVForever
/// fallback lookup path, not in the shipped v1.0.1 release].
pub const FALLBACK_INI_PATH: &str = "NVSE\\Plugins\\ManOnFireNVSE.ini";

/// Default seconds of full-proximity exposure before ignition [documented: Man On Fire
/// NVSE v1.0.1 (`https://www.nexusmods.com/newvegas/mods/94962`), shipped
/// `Data\Config\ManOnFireNVSE.ini` `[Settings]` `fIgnitionThreshold = 1.0`].
pub const DEFAULT_IGNITION_THRESHOLD: f64 = 1.0;
/// Default exposure decay per second when away from fire [documented: Man On Fire NVSE
/// v1.0.1 (`https://www.nexusmods.com/newvegas/mods/94962`), shipped
/// `Data\Config\ManOnFireNVSE.ini` `[Settings]` `fDecayRate = 0.3`].
pub const DEFAULT_DECAY_RATE: f64 = 0.3;
/// Default maximum spread radius in game units [documented: Man On Fire NVSE v1.0.1
/// (`https://www.nexusmods.com/newvegas/mods/94962`), shipped
/// `Data\Config\ManOnFireNVSE.ini` `[Settings]` `fMaxRange = 100.0`].
pub const DEFAULT_MAX_RANGE: f64 = 100.0;

/// Upper bound on concurrently tracked actor exposure entries so retained state stays
/// bounded even in extreme synthetic scenes [design].
pub const MAX_TRACKED_EXPOSURE: usize = 1024;

/// Parsed `[Settings]` block from `ManOnFireNVSE.ini`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ManOnFireConfig {
    /// Seconds of full-proximity exposure before catching fire (`fIgnitionThreshold`).
    pub ignition_threshold: f64,
    /// Exposure decay per second when not exposed to fire (`fDecayRate`).
    pub decay_rate: f64,
    /// Maximum 3D distance in game units for fire spread (`fMaxRange`).
    pub max_range: f64,
    /// Whether the target's Fire Resistance actor value reduces/blocks spread
    /// (`bUseFireResist`).
    pub use_fire_resist: bool,
    /// Whether the player is immune to fire spread (`bPlayerImmune`).
    pub player_immune: bool,
    /// Whether hired companions (`Game::is_teammate`) are immune to fire spread
    /// (`bCompanionsImmune`).
    pub companions_immune: bool,
}

impl Default for ManOnFireConfig {
    /// Defaults matching Man On Fire NVSE v1.0.1
    /// (`https://www.nexusmods.com/newvegas/mods/94962`), shipped
    /// `Data\Config\ManOnFireNVSE.ini` (`bUseFireResist = 1`, `bPlayerImmune = 0`,
    /// `bCompanionsImmune = 0`) [documented].
    fn default() -> Self {
        Self {
            ignition_threshold: DEFAULT_IGNITION_THRESHOLD,
            decay_rate: DEFAULT_DECAY_RATE,
            max_range: DEFAULT_MAX_RANGE,
            use_fire_resist: true,
            player_immune: false,
            companions_immune: false,
        }
    }
}

impl ManOnFireConfig {
    /// Whether spread is active (`fMaxRange > 0`) [design].
    pub fn is_enabled(&self) -> bool {
        self.max_range > 0.0
    }

    /// Parse `ManOnFireNVSE.ini` text: BOM-safe (both UTF-8 `\u{feff}` and Latin-1
    /// decoded UTF-8 BOM `\u{00ef}\u{00bb}\u{00bf}`), case-insensitive section and key
    /// names, `;` and `#` comments stripped, invalid values ignored in favour of
    /// defaults, negative numeric settings clamped to `0.0` [design].
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        let text = text.strip_prefix('\u{feff}').or_else(|| text.strip_prefix("\u{00ef}\u{00bb}\u{00bf}")).unwrap_or(text);
        let mut in_settings = false;
        for raw_line in text.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                in_settings = section.trim().eq_ignore_ascii_case("Settings");
                continue;
            }
            if !in_settings {
                continue;
            }
            let Some((raw_key, raw_val)) = line.split_once('=') else { continue };
            let key = raw_key.trim();
            let val = raw_val.trim().trim_matches('"').trim();
            match key.to_ascii_lowercase().as_str() {
                "fignitionthreshold" => {
                    if let Some(v) = parse_non_neg_f64(val) {
                        cfg.ignition_threshold = v;
                    }
                }
                "fdecayrate" => {
                    if let Some(v) = parse_non_neg_f64(val) {
                        cfg.decay_rate = v;
                    }
                }
                "fmaxrange" => {
                    if let Some(v) = parse_non_neg_f64(val) {
                        cfg.max_range = v;
                    }
                }
                "busefireresist" => {
                    if let Some(b) = parse_bool(val) {
                        cfg.use_fire_resist = b;
                    }
                }
                "bplayerimmune" => {
                    if let Some(b) = parse_bool(val) {
                        cfg.player_immune = b;
                    }
                }
                "bcompanionsimmune" => {
                    if let Some(b) = parse_bool(val) {
                        cfg.companions_immune = b;
                    }
                }
                _ => {}
            }
        }
        cfg
    }
}

/// Session-only cached configuration and per-actor exposure accumulator.
#[derive(Debug, Default)]
pub struct ManOnFireState {
    /// `(roots_generation, mod_files_generation)` for which `config` was resolved.
    cache_key: Option<(u64, u64)>,
    /// `None` when neither INI path is present.
    config: Option<ManOnFireConfig>,
    /// Accumulated exposure seconds per non-burning candidate actor.
    exposure: BTreeMap<FormId, f64>,
}

impl ManOnFireState {
    /// Store or update an actor's exposure while enforcing `cap` on distinct tracked
    /// entries [design]. Returns `true` when stored.
    fn record_exposure(&mut self, actor: FormId, value: f64, cap: usize) -> bool {
        if let Some(slot) = self.exposure.get_mut(&actor) {
            *slot = value;
            return true;
        }
        if self.exposure.len() >= cap {
            return false;
        }
        self.exposure.insert(actor, value);
        true
    }
}

struct BurningSource {
    actor: FormId,
    space: FormId,
    pos: [f32; 3],
    source_item: FormId,
    caster: Option<FormId>,
    magnitude: u32,
}

fn strip_comment(line: &str) -> &str {
    match line.find([';', '#']) {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn parse_non_neg_f64(s: &str) -> Option<f64> {
    let v: f64 = s.parse().ok()?;
    v.is_finite().then(|| v.max(0.0))
}

fn parse_bool(s: &str) -> Option<bool> {
    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes") || s.eq_ignore_ascii_case("on") {
        return Some(true);
    }
    if s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("no") || s.eq_ignore_ascii_case("off") {
        return Some(false);
    }
    let v: f64 = s.parse().ok()?;
    v.is_finite().then_some(v != 0.0)
}

fn decode_ini_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

impl Game<'_> {
    /// Read the active `ManOnFireNVSE.ini` text (`Config\ManOnFireNVSE.ini` first,
    /// then `NVSE\Plugins\ManOnFireNVSE.ini`), returning `None` when neither exists.
    fn read_man_on_fire_ini(&self) -> Option<String> {
        for path in [PRIMARY_INI_PATH, FALLBACK_INI_PATH] {
            if let Some(text) = self.mod_files.read_text(path) {
                return Some(text);
            }
            if let Some(bytes) = crate::mod_files::read_data_file(path) {
                return Some(decode_ini_bytes(&bytes));
            }
        }
        None
    }

    /// Resolve and cache the active [`ManOnFireConfig`], reloading automatically if
    /// file roots or session `mod_files` changed.
    pub fn man_on_fire_config(&mut self) -> Option<ManOnFireConfig> {
        let key = (crate::mod_files::roots_generation(), self.mod_files.generation());
        if self.small.man_on_fire.cache_key == Some(key) {
            return self.small.man_on_fire.config;
        }
        let cfg = self.read_man_on_fire_ini().map(|text| ManOnFireConfig::parse(&text));
        self.small.man_on_fire.cache_key = Some(key);
        self.small.man_on_fire.config = cfg;
        if cfg.is_none() {
            self.small.man_on_fire.exposure.clear();
        }
        cfg
    }

    /// Current accumulated fire-spread exposure (in seconds) for `actor`.
    pub fn man_on_fire_exposure(&self, actor: FormId) -> f64 {
        self.small.man_on_fire.exposure.get(&actor).copied().unwrap_or(0.0)
    }

    /// Whether `mgef` is a harmful fire-resisted effect (`resistance == FIRE_RESISTANCE`
    /// and Hostile or Detrimental) [observed: hit-effects.md §3; grass-fire.md §1].
    fn is_fire_mgef(&self, effect: FormId) -> bool {
        self.magic_effect_def(effect).is_some_and(|m| {
            m.resistance == Some(FIRE_RESISTANCE) && (m.has(MagicEffectDef::HOSTILE) || m.has(MagicEffectDef::DETRIMENTAL))
        })
    }

    /// Whether an active effect is a live burning effect on its target.
    fn is_burning_effect(&self, e: &ActiveEffect) -> bool {
        !e.inactive && !e.permanent && e.worn.is_none() && self.is_fire_mgef(e.effect)
    }

    /// Whether `actor` currently has an active harmful fire-resisted effect.
    pub fn is_actor_burning(&self, actor: FormId) -> bool {
        self.effects.iter().any(|e| e.target == actor && self.is_burning_effect(e))
    }

    fn actor_place(&self, r: FormId) -> Option<(FormId, [f32; 3])> {
        match self.world.peek(r) {
            Some(s) => Some((s.cell?, s.pos)),
            None => self.world.placed.get(r).and_then(|p| Some((p.cell?, p.pos))),
        }
    }

    fn space_for_cell(&self, cell: FormId) -> FormId {
        self.world.placed.world_of(cell).unwrap_or(cell)
    }

    fn is_spread_immune(&self, target: FormId, cfg: &ManOnFireConfig) -> bool {
        if !self.effects_loaded(target) || self.is_dead(target) || self.is_immune(target) {
            return true;
        }
        if target == FormId::PLAYER_REF && (cfg.player_immune || self.toggles.god_mode) {
            return true;
        }
        cfg.companions_immune && self.is_teammate(target)
    }

    /// Advance actor-to-actor fire spread by `dt` seconds of game-mode time.
    ///
    /// Returns immediately with zero allocations when `ManOnFireNVSE.ini` is absent or
    /// disabled (`fMaxRange <= 0`), leaving unmodded vanilla combat unchanged.
    pub(crate) fn tick_man_on_fire(&mut self, dt: f64) {
        if dt <= 0.0 || !dt.is_finite() || self.gaps.jip3.time_frozen {
            return;
        }
        let Some(cfg) = self.man_on_fire_config() else {
            return;
        };
        if !cfg.is_enabled() {
            self.small.man_on_fire.exposure.clear();
            return;
        }
        if self.effects.is_empty() && self.small.man_on_fire.exposure.is_empty() {
            return;
        }

        // Collect unique loaded burning source actors (corpses with No Death Dispel
        // burns stay valid sources while their burn lasts [design]).
        let mut sources: BTreeMap<FormId, BurningSource> = BTreeMap::new();
        for e in &self.effects {
            if !self.is_burning_effect(e) || !self.effects_loaded(e.target) {
                continue;
            }
            let Some((cell, pos)) = self.actor_place(e.target) else { continue };
            let space = self.space_for_cell(cell);
            match sources.get_mut(&e.target) {
                Some(existing) if existing.magnitude >= e.magnitude => {}
                slot => {
                    let entry = BurningSource {
                        actor: e.target,
                        space,
                        pos,
                        source_item: e.source,
                        caster: e.caster,
                        magnitude: e.magnitude,
                    };
                    match slot {
                        Some(existing) => *existing = entry,
                        None => {
                            sources.insert(e.target, entry);
                        }
                    }
                }
            }
        }

        // Fast decay path when nobody is currently burning.
        if sources.is_empty() {
            let decay = cfg.decay_rate * dt;
            let tracked: Vec<FormId> = self.small.man_on_fire.exposure.keys().copied().collect();
            for actor in tracked {
                if self.is_spread_immune(actor, &cfg) {
                    self.small.man_on_fire.exposure.remove(&actor);
                    continue;
                }
                let next = self.small.man_on_fire.exposure.get(&actor).copied().unwrap_or(0.0) - decay;
                if next <= 1e-9 {
                    self.small.man_on_fire.exposure.remove(&actor);
                } else if let Some(slot) = self.small.man_on_fire.exposure.get_mut(&actor) {
                    *slot = next;
                }
            }
            return;
        }

        let mut actors = self.package_actors();
        actors.insert(FormId::PLAYER_REF);

        // Drop stale exposure entries for actors no longer in the loaded candidate set.
        if !self.small.man_on_fire.exposure.is_empty() {
            self.small.man_on_fire.exposure.retain(|actor, _| actors.contains(actor));
        }

        let mut ignitions: Vec<(FormId, FormId, Option<FormId>)> = Vec::new();
        for target in actors {
            // Already-burning actors do not accumulate spread exposure, preventing
            // infinite fire ping-pong between two burning actors [design].
            if sources.contains_key(&target) || self.is_spread_immune(target, &cfg) {
                self.small.man_on_fire.exposure.remove(&target);
                continue;
            }
            let Some((tgt_cell, tgt_pos)) = self.actor_place(target) else {
                self.small.man_on_fire.exposure.remove(&target);
                continue;
            };
            let tgt_space = self.space_for_cell(tgt_cell);

            // Linear proximity falloff: 1.0 at distance 0, 0.0 at fMaxRange [hypothesis].
            let mut proximity_sum = 0.0_f64;
            let mut closest: Option<(f64, FormId, Option<FormId>)> = None;
            for src in sources.values() {
                if src.space != tgt_space {
                    continue;
                }
                let dx = f64::from(tgt_pos[0] - src.pos[0]);
                let dy = f64::from(tgt_pos[1] - src.pos[1]);
                let dz = f64::from(tgt_pos[2] - src.pos[2]);
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist < cfg.max_range {
                    let factor = (1.0 - dist / cfg.max_range).clamp(0.0, 1.0);
                    proximity_sum += factor;
                    if closest.is_none_or(|(best_d, _, _)| dist < best_d) {
                        closest = Some((dist, src.source_item, src.caster.or(Some(src.actor))));
                    }
                }
            }

            let resist_factor = if cfg.use_fire_resist && proximity_sum > 0.0 {
                let resist = crate::functions::actor::actor_value(self, target, FIRE_RESISTANCE).unwrap_or(0.0);
                (1.0 - (resist / 100.0).clamp(0.0, 1.0)).clamp(0.0, 1.0)
            } else {
                1.0
            };

            let gain = proximity_sum * resist_factor * dt;
            if gain > 0.0 {
                let next = self.small.man_on_fire.exposure.get(&target).copied().unwrap_or(0.0) + gain;
                if next + 1e-9 >= cfg.ignition_threshold {
                    self.small.man_on_fire.exposure.remove(&target);
                    if let Some((_, source_item, caster)) = closest {
                        ignitions.push((target, source_item, caster));
                    }
                } else {
                    self.small.man_on_fire.record_exposure(target, next, MAX_TRACKED_EXPOSURE);
                }
            } else if let Some(&cur) = self.small.man_on_fire.exposure.get(&target) {
                let next = cur - cfg.decay_rate * dt;
                if next <= 1e-9 {
                    self.small.man_on_fire.exposure.remove(&target);
                } else if let Some(slot) = self.small.man_on_fire.exposure.get_mut(&target) {
                    *slot = next;
                }
            }
        }

        for (target, source_item, caster) in ignitions {
            if self.is_spread_immune(target, &cfg) || self.is_actor_burning(target) {
                continue;
            }
            self.ignite_from_spread(target, source_item, caster, cfg.use_fire_resist);
        }
    }

    /// Apply the fire effect(s) of `source_item` to `target` upon fire-spread ignition.
    ///
    /// When every effect on `source_item` is fire-resisted (the standard case for flamer,
    /// incinerator, and incendiary items), applies the item as a whole; for mixed items,
    /// applies only the fire-resisted effect indices [design].
    fn ignite_from_spread(&mut self, target: FormId, source_item: FormId, caster: Option<FormId>, use_fire_resist: bool) {
        let items = {
            let own = self.applied_effects(source_item);
            if own.is_empty() && self.clones.contains_key(&source_item) {
                self.applied_effects(self.unclone(source_item))
            } else {
                own
            }
        };
        if items.is_empty() {
            return;
        }
        let fire_indices: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|&(idx, e)| !self.effect_removed(source_item, idx) && self.is_fire_mgef(e.effect))
            .map(|(idx, _)| idx)
            .collect();
        if fire_indices.is_empty() {
            return;
        }
        let all_fire = fire_indices.len() == items.iter().enumerate().filter(|&(idx, _)| !self.effect_removed(source_item, idx)).count();
        let applied = if all_fire {
            let mods = crate::aid::Mods { hit: use_fire_resist, ..crate::aid::Mods::default() };
            self.apply_magic_item_with(target, source_item, caster, false, &mods)
        } else {
            let mut out = Vec::new();
            for idx in fire_indices {
                let mods = crate::aid::Mods { hit: use_fire_resist, only: Some(idx), ..crate::aid::Mods::default() };
                out.extend(self.apply_magic_item_with(target, source_item, caster, false, &mods));
            }
            out
        };
        self.effect_started_lines(target, caster, &applied);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_public_ini_schema() {
        let cfg = ManOnFireConfig::default();
        assert_eq!(cfg.ignition_threshold, 1.0);
        assert_eq!(cfg.decay_rate, 0.3);
        assert_eq!(cfg.max_range, 100.0);
        assert!(cfg.use_fire_resist);
        assert!(!cfg.player_immune);
        assert!(!cfg.companions_immune);
        assert!(cfg.is_enabled());
    }

    #[test]
    fn parses_standard_settings_case_insensitively_with_comments_and_bom() {
        let text = "\u{feff}; leading comment\r\n[sEtTiNgS] ; inline section comment\r\n\
            fignitionthreshold = 2.5 ; slower\r\n\
            FDECAYRATE = 0.75 # hash comment\r\n\
            fMaxRange = \"150.0\"\r\n\
            bUseFireResist = 0\r\n\
            bPlayerImmune = true\r\n\
            bCompanionsImmune = 1\r\n";
        let cfg = ManOnFireConfig::parse(text);
        assert_eq!(cfg.ignition_threshold, 2.5);
        assert_eq!(cfg.decay_rate, 0.75);
        assert_eq!(cfg.max_range, 150.0);
        assert!(!cfg.use_fire_resist);
        assert!(cfg.player_immune);
        assert!(cfg.companions_immune);
    }

    #[test]
    fn strips_latin1_decoded_utf8_bom() {
        let raw = b"\xEF\xBB\xBF[Settings]\nfMaxRange = 80.0\nbPlayerImmune = 1\n";
        let latin1: String = raw.iter().map(|&b| b as char).collect();
        let cfg = ManOnFireConfig::parse(&latin1);
        assert_eq!(cfg.max_range, 80.0);
        assert!(cfg.player_immune);
    }

    #[test]
    fn ignores_other_sections_malformed_lines_and_sanitizes_invalid_numbers() {
        let text = "fMaxRange = 500.0\n\
            [Other]\n\
            fMaxRange = 500.0\n\
            bPlayerImmune = 1\n\
            [Settings]\n\
            not_a_key_value_line\n\
            fIgnitionThreshold = NaN\n\
            fDecayRate = -2.0\n\
            fMaxRange = -10.0\n\
            bUseFireResist = invalid\n\
            bCompanionsImmune = yes\n\
            bCompanionsImmune = off\n";
        let cfg = ManOnFireConfig::parse(text);
        assert_eq!(cfg.ignition_threshold, DEFAULT_IGNITION_THRESHOLD);
        assert_eq!(cfg.decay_rate, 0.0);
        assert_eq!(cfg.max_range, 0.0);
        assert!(!cfg.is_enabled());
        assert!(cfg.use_fire_resist);
        assert!(!cfg.player_immune);
        assert!(!cfg.companions_immune);
    }

    #[test]
    fn exposure_tracker_enforces_safety_cap_while_allowing_updates() {
        let mut state = ManOnFireState::default();
        assert!(state.record_exposure(FormId(1), 0.25, 2));
        assert!(state.record_exposure(FormId(2), 0.50, 2));
        // Updating an already-tracked actor succeeds at capacity.
        assert!(state.record_exposure(FormId(1), 0.75, 2));
        // Inserting a new actor past capacity is refused.
        assert!(!state.record_exposure(FormId(3), 0.10, 2));
        assert_eq!(state.exposure.len(), 2);
        assert_eq!(state.exposure.get(&FormId(1)), Some(&0.75));
        assert_eq!(state.exposure.get(&FormId(3)), None);
    }
}
