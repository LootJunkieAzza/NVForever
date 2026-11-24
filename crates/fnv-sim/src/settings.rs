//! Game settings (GMST): named engine constants authored in the plugins.
//!
//! GMST DATA is a union decided by the first letter of the Editor ID: `f` → f32,
//! `i` → i32, `s` → NUL-terminated string [documented: xEdit wbDefinitionsFNV.pas GMST;
//! observed: all 459 `f` and 61 `i` settings in the masters are 4 bytes]. The schema
//! keeps DATA as bytes (schema v1 has no unions) and this module interprets them.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Setting {
    Float(f32),
    Int(i32),
    Str(String),
    /// A prefix or size the rule above does not cover (reported by the loader).
    Raw(Vec<u8>),
}

/// Built-in engine-default GMSTs that may be absent from loaded ESM/ESP records while
/// still readable and mutable at run time (`SetGameSetting`/`setgs`, `con_SetGameSetting`,
/// `GetNumericGameSetting`, `SetNumericGameSetting`, `ModNumericGameSetting`,
/// `GetGameSetting`) [docs/spec-notes/builtin-gmst-defaults.md].
///
/// Precedence in [`crate::World`]: live session override (`setting_changes`) > load-order
/// `GMST` record (`FormStore::settings`) > this table [design].
pub const BUILTIN_DEFAULTS: &[(&str, Setting)] = &[
    // [documented: https://geckwiki.com/index.php?title=FCombatEnvironmentBloodChance,
    // "Default = 0.7500"; docs/spec-notes/impact-decals.md §4a].
    ("fCombatEnvironmentBloodChance", Setting::Float(0.75)),
    // [documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings,
    // "iDismemberBloodDecalCount, 10.000000"].
    ("iDismemberBloodDecalCount", Setting::Int(10)),
    // [documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings,
    // "fGunWobbleChaseDriftTime, 0.750000"].
    ("fGunWobbleChaseDriftTime", Setting::Float(0.75)),
    // [documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings,
    // "fIronSightsFOVTimeChange, 0.250000"].
    ("fIronSightsFOVTimeChange", Setting::Float(0.25)),
    // [documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings,
    // "fCharacterDefaultTurningSpeed, 90.000000"; setting name also on
    // https://geckwiki.com/index.php?title=Settings, docs/spec-notes/actors.md §8.9].
    ("FCharacterDefaultTurningSpeed", Setting::Float(90.0)),
    // [documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings,
    // "iAntiCheatDuration, 60.000000"].
    ("iAntiCheatDuration", Setting::Int(60)),
    // [documented: https://geckwiki.com/index.php?title=Complete_List_of_GameSettings,
    // "fSeenDataUpdateRadius, 1024.000000"].
    ("fSeenDataUpdateRadius", Setting::Float(1024.0)),
];

/// Look up a built-in engine-default GMST by Editor ID (ASCII case-insensitive).
pub fn builtin_default(edid: &str) -> Option<&'static Setting> {
    BUILTIN_DEFAULTS.iter().find(|(name, _)| name.eq_ignore_ascii_case(edid)).map(|(_, value)| value)
}

impl Setting {
    /// Interpret DATA bytes for a setting with this Editor ID.
    pub fn from_bytes(edid: &str, data: &[u8]) -> Setting {
        match (edid.as_bytes().first().map(u8::to_ascii_lowercase), data.len()) {
            (Some(b'f'), 4) => Setting::Float(f32::from_le_bytes(data.try_into().expect("4 bytes"))),
            (Some(b'i'), 4) => Setting::Int(i32::from_le_bytes(data.try_into().expect("4 bytes"))),
            (Some(b's'), _) => Setting::Str(fnv_util::text::decode_z(data)),
            _ => Setting::Raw(data.to_vec()),
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Setting::Float(v) => Some(v),
            Setting::Int(v) => Some(v as f32),
            _ => None,
        }
    }
}

/// Parse the lower-case hex the schema decoder prints for `bytes` fields.
pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_by_prefix() {
        assert_eq!(Setting::from_bytes("fAVDCarryWeightsBase", &150f32.to_le_bytes()), Setting::Float(150.0));
        assert_eq!(Setting::from_bytes("FCharacterDefaultTurningSpeed", &90f32.to_le_bytes()), Setting::Float(90.0));
        assert_eq!(Setting::from_bytes("iLevelCap", &30i32.to_le_bytes()), Setting::Int(30));
        assert_eq!(Setting::from_bytes("IAntiCheatDuration", &60i32.to_le_bytes()), Setting::Int(60));
        assert_eq!(Setting::from_bytes("sOk", b"Ok\0"), Setting::Str("Ok".into()));
        assert_eq!(Setting::from_bytes("SOk", b"Ok\0"), Setting::Str("Ok".into()));
        assert_eq!(Setting::from_bytes("bX", &[1]), Setting::Raw(vec![1]));
        assert_eq!(Setting::from_bytes("fShort", &[0, 0]), Setting::Raw(vec![0, 0]));
        assert_eq!(unhex("00001643"), Some(vec![0, 0, 0x16, 0x43]));
        assert_eq!(unhex("0"), None);
        assert_eq!(unhex("zz"), None);
        assert_eq!(Setting::Int(3).as_f32(), Some(3.0));
    }

    #[test]
    fn builtin_defaults_match_prefixes_and_ignore_case() {
        let mut seen = std::collections::HashSet::new();
        for (name, value) in BUILTIN_DEFAULTS {
            assert!(seen.insert(name.to_ascii_lowercase()), "duplicate builtin GMST {name}");
            match (name.as_bytes()[0].to_ascii_lowercase(), value) {
                (b'f', Setting::Float(_)) | (b'i', Setting::Int(_)) | (b's', Setting::Str(_)) => {}
                _ => panic!("prefix mismatch for {name}: {value:?}"),
            }
            assert_eq!(builtin_default(name), Some(value));
            assert_eq!(builtin_default(&name.to_ascii_lowercase()), Some(value));
            assert_eq!(builtin_default(&name.to_ascii_uppercase()), Some(value));
        }
        assert_eq!(builtin_default("fCombatEnvironmentBloodChance"), Some(&Setting::Float(0.75)));
        assert_eq!(builtin_default("iDismemberBloodDecalCount"), Some(&Setting::Int(10)));
        assert_eq!(builtin_default("fGunWobbleChaseDriftTime"), Some(&Setting::Float(0.75)));
        assert_eq!(builtin_default("fIronSightsFOVTimeChange"), Some(&Setting::Float(0.25)));
        assert_eq!(builtin_default("fKnockbackForceMax"), None);
        assert_eq!(builtin_default("FCharacterDefaultTurningSpeed"), Some(&Setting::Float(90.0)));
        assert_eq!(builtin_default("fCharacterDefaultTurningSpeed"), Some(&Setting::Float(90.0)));
        assert_eq!(builtin_default("iAntiCheatDuration"), Some(&Setting::Int(60)));
        assert_eq!(builtin_default("fSeenDataUpdateRadius"), Some(&Setting::Float(1024.0)));
        assert_eq!(builtin_default("uSyntheticUnknownSetting"), None);
        assert_eq!(builtin_default("fOwnedUnknownSetting"), None);
    }
}
