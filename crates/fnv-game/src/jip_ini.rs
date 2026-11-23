//! Clean-room loader and rules for `Data\NVSE\Plugins\jip_nvse.ini` `[GamePatches]`.
//!
//! Only the public `jip_nvse.ini` schema/comments shipped in JIP LN Settings INI v56.24
//! (for JIP LN NVSE Plugin 57.30) and public GECK Wiki documentation were consulted; no
//! JIP LN DLL, binary, PDB, or plugin source code was inspected.
//!
//! When `jip_nvse.ini` is absent (or a patch key is `0`/omitted), every patch defaults to
//! off so unmodded vanilla behavior remains unchanged. See `docs/spec-notes/jip-math-utility.md`
//! §6 for JIP LN's shipped INI defaults and `[hypothesis]`/`[design]` boundary notes.

use crate::mod_files::ModFiles;

/// Data-relative path of JIP LN NVSE's configuration file (matched case-insensitively).
pub const JIP_INI_REL_PATH: &str = "NVSE\\Plugins\\jip_nvse.ini";

/// Active `[GamePatches]` settings parsed from `Data\NVSE\Plugins\jip_nvse.ini`.
///
/// Every field stored here has a live engine consumer. Keys whose behavior NVForever's
/// engine already provides unconditionally (such as `bProjImpactDmgFix`,
/// `bGameDaysPassedFix`, `bHardcoreNeedsFix`, `bNoFailedScriptLocks`, `bDoublePrecision`)
/// and unbuilt optional patches (`bEnableFO3Repair`, `bEnableBigGunsSkill`,
/// `bEnableNVACAlerts`, `bCreatureSpreadFix`) are intentionally not kept as dead fields;
/// see `docs/spec-notes/jip-math-utility.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JipGamePatches {
    /// `bIgnoreDTDRFix` (JIP shipped default `1`): prerequisite gate for `bLocalizedDTDR`
    /// [documented: JIP LN 57.30 / JIP LN Settings INI v56.24 `jip_nvse.ini`, "(Note: Requires
    /// bIgnoreDTDRFix to be enabled)"]. NVForever's `fnv_combat::damage::hit` already subtracts
    /// DT before DR and honors `WEAP` `Ignores Normal Weapon Resistance`
    /// (`docs/spec-notes/weapon-data-fields.md` §6).
    pub ignore_dt_dr_fix: bool,
    /// `bLocalizedDTDR` (JIP shipped default `0`): head hits use worn head-armor DT/DR only;
    /// body hits use worn body-armor DT/DR only (requires `ignore_dt_dr_fix`) [documented:
    /// JIP LN 57.30 / INI v56.24 `jip_nvse.ini`]. Full worn armor on explosions and actor
    /// innate DT/DR (AV 76/18) applying everywhere are `[hypothesis]`.
    pub localized_dt_dr: bool,
    /// `bQttSelectShortKeys` (JIP shipped default `1`): Shift transfers/drops the entire stack
    /// and Ctrl transfers/drops 1 item in Inventory, Container, and Barter menus, skipping
    /// `QuantityMenu` [documented: JIP LN 57.30 / INI v56.24 `jip_nvse.ini`]. Shift-over-Ctrl
    /// precedence and Q/right-click shortcut routing are `[design]`; retaining NVForever's
    /// `ASK_QUANTITY_ABOVE` threshold for weightless stacks is `[hypothesis]`.
    pub qtt_select_short_keys: bool,
    /// `bNPCWeaponMods` (JIP shipped default `0`): NPCs have a level-scaled chance for their
    /// main weapon to include weapon mods [documented: JIP LN 57.30 / INI v56.24 `jip_nvse.ini`].
    /// Restricting to human NPCs (`NPC_` / `LVLN` -> `NPC_`) is `[hypothesis]`.
    pub npc_weapon_mods: bool,
    /// `uWMChancePerLevel` (JIP shipped default `2`): percent chance increase per NPC level
    /// [documented: JIP LN 57.30 / INI v56.24 `jip_nvse.ini`]; clamped to `0..=100` `[design]`.
    pub wm_chance_per_level: u8,
    /// `uWMChanceMin` (JIP shipped default `10`): minimum percent chance [documented: JIP LN
    /// 57.30 / INI v56.24 `jip_nvse.ini`]; clamped to `0..=100` `[design]`.
    pub wm_chance_min: u8,
    /// `uWMChanceMax` (JIP shipped default `60`): maximum percent chance [documented: JIP LN
    /// 57.30 / INI v56.24 `jip_nvse.ini`]; clamped to `0..=100` `[design]`.
    pub wm_chance_max: u8,
    /// `bSneakBoundingBoxFix` (JIP shipped default `1`): shrinks crouch collision height for
    /// the player and human companions so crouching passes through low openings [documented:
    /// JIP LN 57.30 / INI v56.24 `jip_nvse.ini`]. Tracking crouching human companions via
    /// `is_human_npc` + teammate sneak state is `[hypothesis]`; `CROUCH_HEIGHT_SCALE = 0.62`
    /// and overhead headroom hold are `[design]`.
    pub sneak_bounding_box_fix: bool,
    /// `bLoadScreenFix` (JIP shipped default `0`): gives priority to location-specific load
    /// screens when any match current location [documented: JIP LN 57.30 / INI v56.24
    /// `jip_nvse.ini`]; falling back to generic screens when none match is `[hypothesis]`.
    pub load_screen_fix: bool,
    /// `uNPCPerks` (JIP shipped default `0`): `1` unlocks script perk commands on any NPC and
    /// `2` also auto-assigns 1 random starting Trait and 1 random Perk per 3 levels up to 10
    /// max to human NPCs [documented: JIP LN 57.30 / INI v56.24 `jip_nvse.ini`]. Keeping
    /// `uNPCPerks=0` identical to `1` for script perk commands is `[design]` (preserving
    /// `nvse_jip10.rs` `[hypothesis: NVForever gives them]`); floor division `(level / 3).min(10)`
    /// and playable/non-hidden/`min_level` filters are `[hypothesis]`.
    pub npc_perks: u8,
    /// `bVoiceModulationFix` (JIP shipped default `1`): adds voice modulation for talking
    /// activators and holotapes [documented: JIP LN 57.30 / INI v56.24 `jip_nvse.ini`].
    /// Scoping holotapes to queued voice holotapes and yielding to Stewie's `no_voice_filter`
    /// are `[hypothesis]`; reusing NVForever's `VoiceFilter` DSP is `[design]`.
    pub voice_modulation_fix: bool,
}

impl Default for JipGamePatches {
    fn default() -> Self {
        Self {
            ignore_dt_dr_fix: false,
            localized_dt_dr: false,
            qtt_select_short_keys: false,
            npc_weapon_mods: false,
            wm_chance_per_level: 2,
            wm_chance_min: 10,
            wm_chance_max: 60,
            sneak_bounding_box_fix: false,
            load_screen_fix: false,
            npc_perks: 0,
            voice_modulation_fix: false,
        }
    }
}

impl JipGamePatches {
    /// Load `Data\NVSE\Plugins\jip_nvse.ini` via `ModFiles` (session writes first, then
    /// case-insensitive read roots and host archives). Returns [`Self::default`] (all
    /// patches off) when absent.
    pub fn load(mod_files: &ModFiles) -> Self {
        let bytes = mod_files
            .read_written_bytes(JIP_INI_REL_PATH)
            .or_else(|| crate::mod_files::read_data_file(JIP_INI_REL_PATH));
        match bytes {
            Some(raw) => Self::parse(&raw),
            None => Self::default(),
        }
    }

    /// Parse `[GamePatches]` directly from raw `&[u8]` bytes.
    ///
    /// Byte-safe: strips UTF-8 BOM (`0xEF 0xBB 0xBF`), ignores `;` and `#` comments
    /// (full-line and inline), matches section and key names ASCII case-insensitively,
    /// filters non-finite numbers (`NaN`/`inf`), and never slices `&str` by byte indices.
    pub fn parse(bytes: &[u8]) -> Self {
        let mut out = Self::default();
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        let mut in_game_patches = false;
        for raw_line in bytes.split(|&b| b == b'\n') {
            let line = trim_ascii(raw_line);
            if line.is_empty() || line[0] == b';' || line[0] == b'#' {
                continue;
            }
            if line[0] == b'[' {
                if let Some(close) = line.iter().position(|&b| b == b']') {
                    let section = trim_ascii(&line[1..close]);
                    in_game_patches = section.eq_ignore_ascii_case(b"GamePatches");
                }
                continue;
            }
            if !in_game_patches {
                continue;
            }
            let Some(eq) = line.iter().position(|&b| b == b'=') else { continue };
            let key = trim_ascii(&line[..eq]);
            let val_raw = &line[eq + 1..];
            let val_no_comment = match val_raw.iter().position(|&b| b == b';' || b == b'#') {
                Some(c) => &val_raw[..c],
                None => val_raw,
            };
            let val = trim_ascii(val_no_comment);
            let Some(num) = parse_finite_f64(val) else { continue };
            if key.eq_ignore_ascii_case(b"bIgnoreDTDRFix") {
                out.ignore_dt_dr_fix = num != 0.0;
            } else if key.eq_ignore_ascii_case(b"bLocalizedDTDR") {
                out.localized_dt_dr = num != 0.0;
            } else if key.eq_ignore_ascii_case(b"bQttSelectShortKeys") {
                out.qtt_select_short_keys = num != 0.0;
            } else if key.eq_ignore_ascii_case(b"bNPCWeaponMods") {
                out.npc_weapon_mods = num != 0.0;
            } else if key.eq_ignore_ascii_case(b"uWMChancePerLevel") {
                out.wm_chance_per_level = num.clamp(0.0, 100.0).round() as u8;
            } else if key.eq_ignore_ascii_case(b"uWMChanceMin") {
                out.wm_chance_min = num.clamp(0.0, 100.0).round() as u8;
            } else if key.eq_ignore_ascii_case(b"uWMChanceMax") {
                out.wm_chance_max = num.clamp(0.0, 100.0).round() as u8;
            } else if key.eq_ignore_ascii_case(b"bSneakBoundingBoxFix") {
                out.sneak_bounding_box_fix = num != 0.0;
            } else if key.eq_ignore_ascii_case(b"bLoadScreenFix") {
                out.load_screen_fix = num != 0.0;
            } else if key.eq_ignore_ascii_case(b"uNPCPerks") {
                out.npc_perks = num.clamp(0.0, 2.0).trunc() as u8;
            } else if key.eq_ignore_ascii_case(b"bVoiceModulationFix") {
                out.voice_modulation_fix = num != 0.0;
            }
        }
        out
    }

    /// Whether localized head/body armor DT/DR separation is active: requires both
    /// `bLocalizedDTDR=1` and `bIgnoreDTDRFix=1` [documented: JIP LN 57.30 / INI v56.24
    /// `jip_nvse.ini`].
    pub fn localized_dt_dr_active(&self) -> bool {
        self.ignore_dt_dr_fix && self.localized_dt_dr
    }

    /// NPC weapon-mod attachment chance in percent (`0..=100`):
    /// `Min(uWMChanceMax, Max(uWMChanceMin, Level * uWMChancePerLevel))`
    /// [documented: JIP LN 57.30 / INI v56.24 `jip_nvse.ini`]. Returns `0` when
    /// `bNPCWeaponMods=0`.
    pub fn npc_weapon_mod_chance(&self, level: u16) -> u8 {
        if !self.npc_weapon_mods {
            return 0;
        }
        let scaled = u32::from(level).saturating_mul(u32::from(self.wm_chance_per_level));
        scaled
            .max(u32::from(self.wm_chance_min))
            .min(u32::from(self.wm_chance_max))
            .min(100) as u8
    }

    /// Deterministically roll a weapon-mod bitmask (`bit 0..2` = slots 1..3) for an NPC
    /// of `level` holding a weapon with `available_slots`.
    ///
    /// First mod uses [`Self::npc_weapon_mod_chance`]; additional mods cascade at
    /// `chance / 4` (2nd mod) and `chance / 8` (3rd mod) [design: implements the
    /// documented "2 mods is rare, and a fully-modded weapon is uncommon" progression].
    pub fn roll_npc_weapon_mods(&self, level: u16, available_slots: [bool; 3], seed: u64) -> u8 {
        let chance = self.npc_weapon_mod_chance(level);
        if chance == 0 {
            return 0;
        }
        let mut slots: Vec<u8> = (0u8..3).filter(|&i| available_slots[i as usize]).collect();
        if slots.is_empty() {
            return 0;
        }
        let mut state = splitmix64(seed ^ 0x4A49_5057_4D4F_4453);
        let mut next = || -> u64 {
            state = splitmix64(state.wrapping_add(0x9E37_79B9_7F4A_7C15));
            state
        };
        let mut mask = 0u8;
        let thresholds = [chance, chance / 4, chance / 8];
        for threshold in thresholds {
            if slots.is_empty() || threshold == 0 {
                break;
            }
            if (next() % 100) < u64::from(threshold) {
                let pick = (next() as usize) % slots.len();
                let slot = slots.remove(pick);
                mask |= 1u8 << slot;
            } else {
                break;
            }
        }
        mask
    }

    /// Number of random non-trait perks auto-assigned to a human NPC of `level` under
    /// `uNPCPerks=2`: 1 per 3 levels, capped at 10 [documented: JIP LN 57.30 / INI v56.24
    /// `jip_nvse.ini`; floor division `(level / 3).min(10)` is `[hypothesis]`].
    /// Returns `0` when `uNPCPerks < 2`.
    pub fn npc_auto_perk_count(&self, level: u16) -> usize {
        if self.npc_perks < 2 {
            return 0;
        }
        (usize::from(level) / 3).min(10)
    }
}

fn trim_ascii(mut s: &[u8]) -> &[u8] {
    while let Some(&first) = s.first() {
        if first.is_ascii_whitespace() {
            s = &s[1..];
        } else {
            break;
        }
    }
    while let Some(&last) = s.last() {
        if last.is_ascii_whitespace() {
            s = &s[..s.len() - 1];
        } else {
            break;
        }
    }
    s
}

fn parse_finite_f64(bytes: &[u8]) -> Option<f64> {
    std::str::from_utf8(bytes)
        .ok()?
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}

pub(crate) fn splitmix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_or_empty_ini_preserves_vanilla_defaults() {
        let files = ModFiles::default();
        assert_eq!(JipGamePatches::load(&files), JipGamePatches::default());
        let p = JipGamePatches::parse(b"");
        assert!(!p.ignore_dt_dr_fix);
        assert!(!p.localized_dt_dr);
        assert!(!p.localized_dt_dr_active());
        assert!(!p.qtt_select_short_keys);
        assert!(!p.npc_weapon_mods);
        assert_eq!((p.wm_chance_per_level, p.wm_chance_min, p.wm_chance_max), (2, 10, 60));
        assert!(!p.sneak_bounding_box_fix);
        assert!(!p.load_screen_fix);
        assert_eq!(p.npc_perks, 0);
        assert!(!p.voice_modulation_fix);
    }

    #[test]
    fn parses_bom_comments_case_insensitive_and_ignores_other_sections() {
        let raw = concat!(
            "\u{FEFF}; leading comment\r\n",
            "# hash comment\r\n",
            "[OtherSection]\r\n",
            "bLocalizedDTDR=1\r\n",
            "uNPCPerks=2\r\n",
            "  [ gamepatches ]  \r\n",
            "bignoredtdrfix = 1 ; inline semicolon\r\n",
            "BLOCALIZEDDTDR=1 # inline hash\r\n",
            "bQttSelectShortKeys=1\r\n",
            "bNPCWeaponMods=1\r\n",
            "uWMChancePerLevel=3\r\n",
            "uWMChanceMin=15\r\n",
            "uWMChanceMax=75\r\n",
            "bSneakBoundingBoxFix=1\r\n",
            "bLoadScreenFix=1\r\n",
            "uNPCPerks=2\r\n",
            "bVoiceModulationFix=1\r\n",
            "[Trailing]\r\n",
            "bLoadScreenFix=0\r\n",
        );
        let p = JipGamePatches::parse(raw.as_bytes());
        assert!(p.ignore_dt_dr_fix);
        assert!(p.localized_dt_dr);
        assert!(p.localized_dt_dr_active());
        assert!(p.qtt_select_short_keys);
        assert!(p.npc_weapon_mods);
        assert_eq!((p.wm_chance_per_level, p.wm_chance_min, p.wm_chance_max), (3, 15, 75));
        assert!(p.sneak_bounding_box_fix);
        assert!(p.load_screen_fix);
        assert_eq!(p.npc_perks, 2);
        assert!(p.voice_modulation_fix);
    }

    #[test]
    fn handles_non_ascii_invalid_utf8_and_non_finite_values_without_panicking() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\xEF\xBB\xBF[GamePatches]\n");
        // Non-ASCII UTF-8 comments and bogus keys (Cyrillic, CJK, emoji)
        bytes.extend_from_slice(b"; \xD0\x9F\xD1\x80\xD0\xB8\xD0\xB2\xD0\xB5\xD1\x82 \xF0\x9F\x94\xA5\n");
        bytes.extend_from_slice(b"\xE6\x97\xA5\xE6\x9C\xAC\xE8\xAA\x9E=1\n");
        // Raw invalid UTF-8 bytes in section/key/value/comment
        bytes.extend_from_slice(b"\xFF\xFE\x80=\x80\xFF\n");
        bytes.extend_from_slice(b"bLoadScreenFix=NaN\n");
        bytes.extend_from_slice(b"uWMChancePerLevel=inf\n");
        bytes.extend_from_slice(b"uWMChanceMin=-inf\n");
        bytes.extend_from_slice(b"uNPCPerks=99\n");
        bytes.extend_from_slice(b"bSneakBoundingBoxFix=1 ; \xFF\xFE trailing invalid bytes\n");
        let p = JipGamePatches::parse(&bytes);
        assert!(!p.load_screen_fix, "NaN must be ignored");
        assert_eq!(p.wm_chance_per_level, 2, "inf must keep default");
        assert_eq!(p.wm_chance_min, 10, "-inf must keep default");
        assert_eq!(p.npc_perks, 2, "uNPCPerks clamped to 2");
        assert!(p.sneak_bounding_box_fix);
    }

    #[test]
    fn localized_dt_dr_requires_ignore_dt_dr_fix() {
        let only_loc = JipGamePatches::parse(b"[GamePatches]\nbIgnoreDTDRFix=0\nbLocalizedDTDR=1\n");
        assert!(!only_loc.localized_dt_dr_active());
        let both = JipGamePatches::parse(b"[GamePatches]\nbIgnoreDTDRFix=1\nbLocalizedDTDR=1\n");
        assert!(both.localized_dt_dr_active());
    }

    #[test]
    fn npc_weapon_mod_chance_and_roll_respect_formula_and_slots() {
        let mut p = JipGamePatches::default();
        assert_eq!(p.npc_weapon_mod_chance(20), 0);
        assert_eq!(p.roll_npc_weapon_mods(20, [true, true, true], 42), 0);

        p.npc_weapon_mods = true;
        assert_eq!(p.npc_weapon_mod_chance(1), 10);
        assert_eq!(p.npc_weapon_mod_chance(5), 10);
        assert_eq!(p.npc_weapon_mod_chance(12), 24);
        assert_eq!(p.npc_weapon_mod_chance(30), 60);
        assert_eq!(p.npc_weapon_mod_chance(500), 60);

        p.wm_chance_min = 100;
        p.wm_chance_max = 100;
        let mask = p.roll_npc_weapon_mods(10, [false, true, false], 123);
        assert_eq!(mask, 0b010, "100% chance with only slot 2 available attaches slot 2");
        assert_eq!(p.roll_npc_weapon_mods(10, [false, false, false], 123), 0);
    }

    #[test]
    fn npc_auto_perk_count_scales_and_caps() {
        let mut p = JipGamePatches::default();
        assert_eq!(p.npc_auto_perk_count(30), 0);
        p.npc_perks = 1;
        assert_eq!(p.npc_auto_perk_count(30), 0);
        p.npc_perks = 2;
        assert_eq!(p.npc_auto_perk_count(1), 0);
        assert_eq!(p.npc_auto_perk_count(3), 1);
        assert_eq!(p.npc_auto_perk_count(9), 3);
        assert_eq!(p.npc_auto_perk_count(30), 10);
        assert_eq!(p.npc_auto_perk_count(50), 10);
    }

    #[test]
    fn reload_npc_weapon_mods_and_npc_perks_end_to_end() {
        use crate::common::*;
        use crate::data::GameData;
        use crate::game::{Game, GameConfig};
        use fnv_esm::{LoadOrder, Plugin, Schema};
        use fnv_script::FunctionTable;
        use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
        use std::collections::BTreeSet;

        const CELL: FormId = FormId(0x200);
        const NPC_REF: FormId = FormId(0x301);
        const LVLN_REF: FormId = FormId(0x302);
        const CREA_REF: FormId = FormId(0x303);
        const NPC_BASE: FormId = FormId(0x801);
        const LVLN_BASE: FormId = FormId(0x802);
        const CREA_BASE: FormId = FormId(0x803);
        const WEAPON: FormId = FormId(0x810);
        const MOD_SLOT_2: FormId = FormId(0x811);
        const TRAIT_ID: FormId = FormId(0x820);
        const PERK_A: FormId = FormId(0x821);
        const PERK_B: FormId = FormId(0x822);
        const PERK_HIDDEN: FormId = FormId(0x823);

        // NPC_/CREA ACBS (24 bytes): flags(4), fatigue(2), barter_gold(2), level(2)=9, ...
        let mut acbs = vec![0u8; 24];
        acbs[8..10].copy_from_slice(&9i16.to_le_bytes());
        // CNTO: item FormID (4) + count (4)
        let cnto = [WEAPON.0.to_le_bytes(), 1i32.to_le_bytes()].concat();
        // WEAP DATA (15 bytes): value(4), health(4), weight(4), damage(2), clip(1)
        let mut wdata = Vec::new();
        wdata.extend_from_slice(&100i32.to_le_bytes());
        wdata.extend_from_slice(&100i32.to_le_bytes());
        wdata.extend_from_slice(&5.0f32.to_le_bytes());
        wdata.extend_from_slice(&25i16.to_le_bytes());
        wdata.push(10);

        // WEAP DNAM (204 bytes): animation_type u32 = 5 (2HandRifle) so drawable == true.
        let mut wdnam = vec![0u8; 204];
        wdnam[0..4].copy_from_slice(&5u32.to_le_bytes());

        let perk_rec = |id: FormId, edid: &str, is_trait: u8, min_level: u8, playable: u8, hidden: u8| {
            record(
                b"PERK",
                0,
                id.0,
                &[
                    sub(b"EDID", z(edid)),
                    sub(b"FULL", z(edid)),
                    sub(b"DATA", vec![is_trait, min_level, 1, playable, hidden]),
                ],
            )
        };

        let placed = [
            record(
                b"ACHR",
                0,
                NPC_REF.0,
                &[sub(b"EDID", z("TestNpcRef")), sub(b"NAME", NPC_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
            ),
            record(
                b"ACHR",
                0,
                LVLN_REF.0,
                &[sub(b"EDID", z("TestLvlnRef")), sub(b"NAME", LVLN_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
            ),
            record(
                b"ACRE",
                0,
                CREA_REF.0,
                &[sub(b"EDID", z("TestCreaRef")), sub(b"NAME", CREA_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
            ),
        ]
        .concat();
        let lvln = record(
            b"LVLN",
            0,
            LVLN_BASE.0,
            &[
                sub(b"EDID", z("TestLvln")),
                sub(b"LVLD", vec![0]),
                sub(b"LVLF", vec![0]),
                sub(
                    b"LVLO",
                    [1u16.to_le_bytes().to_vec(), vec![0; 2], NPC_BASE.0.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), vec![0; 2]].concat(),
                ),
            ],
        );
        let cell = [
            record(b"CELL", 0, CELL.0, &[sub(b"DATA", vec![1])]),
            group(CELL.0.to_le_bytes(), 6, &group(CELL.0.to_le_bytes(), 8, &placed)),
        ]
        .concat();

        let bytes = [
            tes4(),
            top(b"IMOD", &record(b"IMOD", 0, MOD_SLOT_2.0, &[sub(b"EDID", z("TestMod2"))])),
            top(
                b"WEAP",
                &record(
                    b"WEAP",
                    0,
                    WEAPON.0,
                    &[
                        sub(b"EDID", z("TestRifle")),
                        sub(b"FULL", z("Test Rifle")),
                        sub(b"MODL", z("weapons\\test.nif")),
                        sub(b"DATA", wdata),
                        sub(b"DNAM", wdnam),
                        sub(b"WMI2", MOD_SLOT_2.0.to_le_bytes().to_vec()),
                    ],
                ),
            ),
            top(
                b"PERK",
                &[
                    perk_rec(TRAIT_ID, "TestTrait", 1, 1, 1, 0),
                    perk_rec(PERK_A, "PerkA", 0, 2, 1, 0),
                    perk_rec(PERK_B, "PerkB", 0, 6, 1, 0),
                    perk_rec(PERK_HIDDEN, "PerkHidden", 0, 1, 1, 1),
                ]
                .concat(),
            ),
            top(
                b"NPC_",
                &[
                    record(b"NPC_", 0, FormId::PLAYER_BASE.0, &[sub(b"EDID", z("Player"))]),
                    record(b"NPC_", 0, NPC_BASE.0, &[sub(b"EDID", z("TestNpc")), sub(b"ACBS", acbs.clone()), sub(b"CNTO", cnto.clone())]),
                ]
                .concat(),
            ),
            top(b"LVLN", &lvln),
            top(
                b"CREA",
                &record(b"CREA", 0, CREA_BASE.0, &[sub(b"EDID", z("TestCrea")), sub(b"ACBS", acbs), sub(b"CNTO", cnto)]),
            ),
            top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
        ]
        .concat();

        let plugins = vec![Plugin::parse(&bytes, "JipIniTest.esm".into()).unwrap()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let schema = Schema::load_dir(root.join("fnv")).unwrap();
        let path = root.join("script/functions.json");
        let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let forms = FormStore::build(&lo, &schema);
        let refs = RefTable::build(&lo, &schema);
        let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);

        // 1. Absent INI: vanilla behavior — no auto weapon mods, no auto perks on cell load.
        let mut game = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
        game.set_loaded(BTreeSet::from([CELL]));
        assert!(game.is_human_npc(NPC_REF));
        assert!(game.is_human_npc(LVLN_REF), "placed LVLN resolving to NPC_ must classify as human NPC");
        assert!(!game.is_human_npc(CREA_REF), "placed CREA must not classify as human NPC");
        assert_eq!(game.world.peek(NPC_REF).unwrap().inventory.as_ref().unwrap().equipped_mods(WEAPON), 0);
        assert_eq!(game.world.peek(LVLN_REF).unwrap().inventory.as_ref().unwrap().equipped_mods(WEAPON), 0);
        assert!(game.held_perks(NPC_REF).is_empty());
        assert!(game.held_perks(LVLN_REF).is_empty());

        // 2. Load jip_nvse.ini via ModFiles session write with bNPCWeaponMods=1 (100% chance) + uNPCPerks=2.
        let mut game2 = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
        game2.mod_files.write(
            JIP_INI_REL_PATH,
            "[GamePatches]\nbNPCWeaponMods=1\nuWMChanceMin=100\nuWMChanceMax=100\nuNPCPerks=2\n".into(),
        );
        game2.reload_jip_ini();
        assert!(game2.jip_ini.npc_weapon_mods);
        assert_eq!(game2.jip_ini.npc_perks, 2);

        game2.set_loaded(BTreeSet::from([CELL]));
        for actor in [NPC_REF, LVLN_REF] {
            assert_eq!(
                game2.world.peek(actor).unwrap().inventory.as_ref().unwrap().equipped_mods(WEAPON),
                0b010,
                "only slot 2 is valid on TestRifle for {actor:?}"
            );
            let held = game2.held_perks(actor);
            assert_eq!(held.len(), 3, "level 9 => 1 trait + 2 available playable perks for {actor:?}: {held:?}");
            assert!(held.iter().any(|&(p, _)| p == TRAIT_ID), "trait assigned to {actor:?}");
            assert!(held.iter().any(|&(p, _)| p == PERK_A), "PerkA assigned to {actor:?}");
            assert!(held.iter().any(|&(p, _)| p == PERK_B), "PerkB assigned to {actor:?}");
            assert!(!held.iter().any(|&(p, _)| p == PERK_HIDDEN), "hidden perk excluded for {actor:?}");
        }
        // Creature control: receives neither weapon mods nor auto perks.
        assert_eq!(game2.world.peek(CREA_REF).unwrap().inventory.as_ref().unwrap().equipped_mods(WEAPON), 0);
        assert!(game2.held_perks(CREA_REF).is_empty());
    }
}
