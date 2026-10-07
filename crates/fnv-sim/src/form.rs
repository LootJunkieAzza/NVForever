//! Global (load-order) FormIDs.
//!
//! Every FormID inside `fnv-sim` is *global*: its top byte is the load-order index of
//! the plugin that owns the form, as produced by [`fnv_esm::LoadOrder::to_global`].
//! File-local FormIDs never leave the loaders (`forms`, `refs`).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormId(pub u32);

impl FormId {
    /// The player's placed reference. [hypothesis: 4,566 FormID fields in the masters
    /// point at 0x14, docs/design/layer1.md §2.4; checked by `fnvtool sim forms`]
    pub const PLAYER_REF: FormId = FormId(0x14);
    /// The player's base NPC_ record. [hypothesis; checked by `fnvtool sim forms`]
    pub const PLAYER_BASE: FormId = FormId(0x7);
    /// First FormID handed out to references created at run time (`PlaceAtMe`, dropped
    /// items). The 0xFF load-order slot is never used by a plugin (at most 255 load,
    /// indices 0x00–0xFE). [hypothesis: the original engine uses the same slot for
    /// created forms; to be confirmed by a savegame or xNVSE capture]
    pub const FIRST_DYNAMIC: FormId = FormId(0xFF00_0000);

    pub fn plugin_index(self) -> u8 {
        (self.0 >> 24) as u8
    }

    pub fn is_dynamic(self) -> bool {
        self.0 >= Self::FIRST_DYNAMIC.0
    }
}

impl fmt::Display for FormId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}", self.0)
    }
}

impl fmt::Debug for FormId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a hex FormID: {0:?}")]
pub struct BadFormId(pub String);

impl FromStr for FormId {
    type Err = BadFormId;
    fn from_str(s: &str) -> Result<FormId, BadFormId> {
        let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
        if hex.is_empty() || hex.len() > 8 {
            return Err(BadFormId(s.to_string()));
        }
        u32::from_str_radix(hex, 16).map(FormId).map_err(|_| BadFormId(s.to_string()))
    }
}

impl Serialize for FormId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for FormId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<FormId, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_forms() {
        assert_eq!("0x0001F4A2".parse::<FormId>().unwrap(), FormId(0x1F4A2));
        assert_eq!("1f4a2".parse::<FormId>().unwrap(), FormId(0x1F4A2));
        assert!("".parse::<FormId>().is_err());
        assert!("0x123456789".parse::<FormId>().is_err());
        assert!("zz".parse::<FormId>().is_err());
        assert_eq!(FormId(0x14).to_string(), "0x00000014");
        let j = serde_json::to_string(&FormId(0xFF000001)).unwrap();
        assert_eq!(j, "\"0xFF000001\"");
        assert_eq!(serde_json::from_str::<FormId>(&j).unwrap(), FormId(0xFF000001));
        assert!(FormId(0xFF000001).is_dynamic());
        assert!(!FormId(0xFE000001).is_dynamic());
    }
}
