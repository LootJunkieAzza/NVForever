//! A reader of the original game's save files (`.fos`), for the extender lab and
//! offline `fnvtool save` conversion/inspection (issue #1023, docs/spec-notes/save-files.md).
//! The game runtime itself never loads or writes a `.fos`.
//!
//! The layout is in docs/spec-notes/save-files.md; every part read here is tagged there.
//! Decoded sections: header (including screenshot RGB bytes and play time), plugin list,
//! global variables (type 3), misc statistics (type 0), player location/active quest
//! (type 1), death-count prefix (type 2), sky/weather (type 8), radio playback (type 10),
//! quests (flags, delay, stages, objectives, variables), player base (sex, spells,
//! SPECIAL, appearance), tag skills, player AV arrays (modifiers, permanent base,
//! damage, health damage), player actor extras (factions, teammates), player inventory
//! (with item extra-data types), player spent skill points, player & teammate perks,
//! non-player actor changes (flags/dead, scale, script vars, factions, inventories),
//! container `REFR` inventories (including locked containers), map marker `REFR`
//! discovery/visibility flags (extra `0x2C`), `REFR` locks (`0x2A`), door open state,
//! owners (`0x21`) and script variables (`0x0D`), created `0xFF` `REFR` headers,
//! encounter zones (`ECZN`), cells (`CELL`), placed reference form flags and places,
//! faction relations, form lists (`FLST`), leveled item lists (`LVLI`), reputations
//! (`REPU`), challenges (`CHAL`), notes (`NOTE`), and said-once dialogue lines (`INFO`).
//! Anything else is kept as raw bytes per changed form.

use std::fmt;

/// A parse failure: where and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FosError(pub String);

impl fmt::Display for FosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FosError {}

fn err<T>(msg: impl Into<String>) -> Result<T, FosError> {
    Err(FosError(msg.into()))
}

/// The separator byte after nearly every field (save-files.md §2).
const BAR: u8 = 0x7C;
const MAGIC: &[u8] = b"FO3SAVEGAME";

/// Changed-form record types (save-files.md §6.1, §6.3).
pub const TYPE_REFR: u8 = 0;
pub const TYPE_ACHR: u8 = 1;
pub const TYPE_ACRE: u8 = 2;
pub const TYPE_PMIS: u8 = 3;
pub const TYPE_PGRE: u8 = 4;
pub const TYPE_CELL: u8 = 7;
pub const TYPE_INFO: u8 = 8;
pub const TYPE_QUST: u8 = 9;
pub const TYPE_NPC: u8 = 10;
pub const TYPE_CREA: u8 = 11;
pub const TYPE_TACT: u8 = 13;
pub const TYPE_NOTE: u8 = 31;
pub const TYPE_ECZN: u8 = 32;
pub const TYPE_CLAS: u8 = 33;
pub const TYPE_FACT: u8 = 34;
pub const TYPE_PACK: u8 = 35;
pub const TYPE_FLST: u8 = 37;
pub const TYPE_LVLI: u8 = 40;
pub const TYPE_REPU: u8 = 43;
pub const TYPE_CHAL: u8 = 50;

/// Human-readable record tag for a `.fos` changed-form type code (save-files.md §6.1, §6.3).
pub fn kind_name(kind: u8) -> &'static str {
    match kind {
        TYPE_REFR => "REFR",
        TYPE_ACHR => "ACHR",
        TYPE_ACRE => "ACRE",
        TYPE_PMIS => "PMIS",
        TYPE_PGRE => "PGRE",
        TYPE_CELL => "CELL",
        TYPE_INFO => "INFO",
        TYPE_QUST => "QUST",
        TYPE_NPC => "NPC_",
        TYPE_CREA => "CREA",
        TYPE_TACT => "TACT",
        TYPE_NOTE => "NOTE",
        TYPE_ECZN => "ECZN",
        TYPE_CLAS => "CLAS",
        TYPE_FACT => "FACT",
        TYPE_PACK => "PACK",
        TYPE_FLST => "FLST",
        TYPE_LVLI => "LVLI",
        TYPE_REPU => "REPU",
        TYPE_CHAL => "CHAL",
        _ => "UNKNOWN",
    }
}

/// The player's reference and base form.
pub const PLAYER_REF: u32 = 0x14;
pub const PLAYER_BASE: u32 = 0x7;
/// The player's class, whose record holds the tag skills (FalloutNV.esm PlayerClass; the
/// one CLAS the saves change, save-files.md §6.1) [observed].
pub const PLAYER_CLASS: u32 = 0x0005_7E6A;

/// A cursor over the file's bytes.
#[derive(Clone)]
struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn new(b: &'a [u8], p: usize) -> Self {
        Cur { b, p }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], FosError> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(|| FosError(format!("{n} bytes past the end at {}", self.p)))?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    /// A byte that must have this value: a fixed marker of the layout.
    fn expect_u8(&mut self, want: u8) -> Result<(), FosError> {
        match self.u8()? {
            b if b == want => Ok(()),
            b => err(format!("expected {want:#04x} at {}, found {b:#04x}", self.p - 1)),
        }
    }
    fn u8(&mut self) -> Result<u8, FosError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FosError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, FosError> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32, FosError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, FosError> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, FosError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, FosError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn bar(&mut self) -> Result<(), FosError> {
        match self.u8()? {
            BAR => Ok(()),
            b => err(format!("expected | at {}, found {b:#04x}", self.p - 1)),
        }
    }
    /// A variable-size count: the low 2 bits of the first byte give the width (1, 2 or
    /// 4 bytes), the value is the little-endian number shifted right by 2 (§5.4).
    fn vsval(&mut self) -> Result<u32, FosError> {
        let width = match self.b.get(self.p).copied().unwrap_or(0) & 3 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => return err(format!("bad vsval at {}", self.p)),
        };
        let mut v = 0u32;
        for (i, x) in self.take(width)?.iter().enumerate() {
            v |= (*x as u32) << (8 * i);
        }
        Ok(v >> 2)
    }
    /// A 3-byte reference, big-endian (§5.4).
    fn ref3(&mut self) -> Result<[u8; 3], FosError> {
        Ok(self.take(3)?.try_into().unwrap())
    }
    /// A `.fos` header/plugin string field followed by a trailing `|` when non-empty
    /// [hypothesis: empty strings write `00 00 7C` with no second `|`].
    fn string_field(&mut self, trailing_bar: bool) -> Result<String, FosError> {
        let n = self.u16()? as usize;
        self.bar()?;
        let s = fnv_util::text::decode(self.take(n)?);
        if trailing_bar && n > 0 {
            self.bar()?;
        }
        Ok(s)
    }
    fn at_end(&self) -> bool {
        self.p >= self.b.len()
    }
}

/// The header fields (§3.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosHeader {
    pub save_number: u32,
    pub player_name: String,
    pub karma_title: String,
    pub level: u32,
    pub location: String,
    pub play_time: String,
    pub screenshot_width: u32,
    pub screenshot_height: u32,
}

impl FosHeader {
    /// Parse the header's `HHH.MM.SS` play-time string into seconds (§3.1).
    pub fn play_seconds(&self) -> Option<f64> {
        let mut parts = self.play_time.trim().split('.');
        let h: u64 = parts.next()?.parse().ok()?;
        let m: u64 = parts.next()?.parse().ok()?;
        let s: u64 = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let total = h.checked_mul(3600)?.checked_add(m.checked_mul(60)?)?.checked_add(s)?;
        Some(total as f64)
    }
}

/// One changed form (§6): its form ID (resolved through the form ID array), change
/// flags, record type and data.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangedForm {
    pub form: u32,
    pub flags: u32,
    pub kind: u8,
    pub data: Vec<u8>,
}

impl ChangedForm {
    fn has(&self, bit: u32) -> bool {
        self.flags & (1 << bit) != 0
    }
}

/// A script variable's saved value (§6.2): a number, or a reference when the index's top
/// bit is set (`None`: no reference).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarValue {
    Num(f64),
    Ref(Option<u32>),
    /// A reference the form ID array does not hold.
    Unresolved,
}

/// A quest's saved changes (§6.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosQuest {
    pub form: u32,
    /// The flags byte (change flag 1): bit 0 running, bit 1 completed
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub flags: Option<u8>,
    /// SetQuestDelay's value (change flag 2) [observed: vanilla 1.4.0.525 A/B save pairs].
    pub delay: Option<f32>,
    /// (stage index, done) for each stage the save lists (change flag 31).
    pub stages: Vec<(u8, bool)>,
    /// Script variables by index (change flag 30).
    pub vars: Vec<(u32, VarValue)>,
    /// (objective index, state) (change flag 29); bit 0 displayed, bit 1 completed
    /// [hypothesis: states 1 and 3 seen].
    pub objectives: Vec<(u32, u32)>,
}

impl FosQuest {
    pub fn running(&self) -> Option<bool> {
        self.flags.map(|f| f & 1 != 0)
    }
    pub fn completed(&self) -> Option<bool> {
        self.flags.map(|f| f & 2 != 0)
    }
}

/// One extra-data list attached to a player inventory item (§6.4): describes a sub-stack or
/// individual instance (count, worn state, condition, owner, weapon mods, hotkey, ammo).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FosItemEntry {
    /// Sub-stack count from extra data type 0x24 (`i16`), when present [observed: vanilla 1.4.0.525 A/B save pairs].
    pub count: Option<i16>,
    /// Extra data type 0x16 (worn marker, 0 payload bytes) [observed: vanilla 1.4.0.525 A/B save pairs]
    /// or 0x17 (defensive secondary-worn pass-through [design; not emitted by vanilla 1.4.0.525]).
    pub worn: bool,
    /// True when worn state came from defensive pass-through extra data type 0x17 rather than 0x16
    /// [design; not emitted by vanilla 1.4.0.525].
    pub worn_secondary: bool,
    /// Item health from extra data type 0x25 (`f32` HP), when present [observed: vanilla 1.4.0.525 A/B save pairs].
    pub condition: Option<f32>,
    /// Owner reference from extra data type 0x21 (`ref3`), when present (`Some(None)` = zero or
    /// unresolved reference) [observed: vanilla 1.4.0.525 A/B save pairs].
    pub owner: Option<Option<u32>>,
    /// Weapon mod bitmask from extra data type 0x8D (`u8 & 0x07`) [observed: vanilla 1.4.0.525 A/B save pairs].
    pub weapon_mods: u8,
    /// Quick-slot hotkey byte from extra data type 0x4A (`u8`), when present [observed: vanilla 1.4.0.525 A/B save pairs].
    pub hotkey: Option<u8>,
    /// Loaded ammo `(ammo_form_id, rounds)` from extra data type 0x6E (`ref3 | u32 |`), when
    /// present [observed framing on NPC-dropped weapon REFRs in vanilla 1.4.0.525; player
    /// inventory weapons emit zero 0x6E and track active rounds in ACHR 0x14 tok[76]].
    pub ammo: Option<(Option<u32>, u32)>,
}

/// An inventory entry of the player (§6.4): the item, its count change, summary extra
/// fields, and every decoded per-instance extra-data list.
#[derive(Debug, Clone, PartialEq)]
pub struct FosItem {
    /// `None`: a reference the form ID array does not hold.
    pub item: Option<u32>,
    /// The change against the base form's inventory [hypothesis: the Pip-Boy, which the
    /// player's base carries, is listed worn with 0].
    pub count: i32,
    /// Any extra data list carries the worn marker (type 0x16) [observed: vanilla 1.4.0.525 A/B save pairs].
    pub worn: bool,
    /// Item health from extra data type 0x25 (f32 HP), when present [observed: vanilla 1.4.0.525 A/B save pairs].
    pub condition: Option<f32>,
    /// Owner reference from extra data type 0x21, when present [observed: vanilla 1.4.0.525 A/B save pairs].
    pub owner: Option<Option<u32>>,
    /// Weapon mod bitmask from extra data type 0x8D (`u8 & 0x07`) [observed: vanilla 1.4.0.525 A/B save pairs].
    pub weapon_mods: u8,
    /// Quick-slot hotkey byte from extra data type 0x4A, when present [observed: vanilla 1.4.0.525 A/B save pairs].
    pub hotkey: Option<u8>,
    /// Loaded ammo `(ammo_form_id, rounds)` from extra data type 0x6E, when present
    /// [observed framing on NPC-dropped weapon REFRs; zero 0x6E on vanilla player inventory].
    pub ammo: Option<(Option<u32>, u32)>,
    /// Per-extra-data-list sub-stack entries decoded for this item, in save order.
    pub entries: Vec<FosItemEntry>,
}

/// FaceGen appearance morphs and head parts from `NPC_ 0x7` change flag 11
/// (save-files.md §6.2, §6.4) [observed: vanilla 1.4.0.525 A/B save pairs].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosPlayerAppearance {
    pub fggs: Vec<f32>,
    pub fgga: Vec<f32>,
    pub fgts: Vec<f32>,
    pub hair: Option<u32>,
    pub eyes: Option<u32>,
    pub hair_length: f32,
    pub hair_color: [u8; 3],
    pub head_parts: Vec<Option<u32>>,
}

/// The player's state that the reader decodes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosPlayer {
    /// The worldspace (outdoors) or interior cell, position and rotation (radians).
    pub place: Option<Place>,
    /// Strength … Luck from the player's base form (NPC_ 0x7, change flag 2).
    pub special: Option<[u8; 7]>,
    /// Player sex from `NPC_ 0x7` change flag 24 (`u8 female |`, `Player.SexChange`) [observed:
    /// vanilla 1.4.0.525 A/B save pairs] or change flag 1 (ACBS bit 0 = female) [hypothesis].
    pub female: Option<bool>,
    /// True when [`FosPlayer::female`] was set from observed `NPC_ 0x7` change flag 24.
    pub female_from_bit24: bool,
    /// Spells/abilities listed on `NPC_ 0x7` change flag 4 (first ref list) [observed: vanilla 1.4.0.525 A/B save pairs].
    pub spells: Vec<Option<u32>>,
    /// FaceGen morphs, hair, eyes, hair colour, and head parts from `NPC_ 0x7` change
    /// flag 11 [observed: vanilla 1.4.0.525 A/B save pairs].
    pub appearance: Option<FosPlayerAppearance>,
    /// Tag skills, as actor value codes (PlayerClass, change flag 1) [observed: vanilla 1.4.0.525 A/B save pairs].
    pub tag_skills: Vec<u32>,
    /// Whether the 232-entry actor-value block matched the observed bar-separated layout.
    pub av_parsed: bool,
    /// The first array of actor-value modifiers by code (§6.4), non-zero entries
    /// [observed: nvfairport's equal the worn Vault 21 Jumpsuit's enchantment; vanilla 1.4.0.525
    /// A/B save pairs confirm Buffout/Rad-X and SleepDeprivation active-effect modifiers].
    pub av_modifiers: Vec<(u32, f32)>,
    /// The second array of actor-value entries by code (§6.4), non-zero entries
    /// [observed: vanilla 1.4.0.525 A/B save pairs: permanent `ModAV` modifiers, Karma (23),
    /// Experience (24), and `ModAV`-applied radiation (54) / hardcore needs (73–75)].
    pub av_base: Vec<(u32, f32)>,
    /// The third array of actor-value entries by code (§6.4), non-zero entries
    /// [observed: vanilla 1.4.0.525 A/B save pairs: current damage deltas (negative for AP 12
    /// and limb conditions 25–31); positive AV3 growing meters remain [hypothesis]].
    pub av_damage: Vec<(u32, f32)>,
    /// The 232nd float immediately after the 3×77 actor-value arrays (§6.4), when non-zero
    /// [observed: vanilla 1.4.0.525 A/B save pairs: player Health damage delta, negative when injured].
    pub health_damage: Option<f32>,
    /// Post-AV finite float (`0x00 | f32 |`, gated on `ACHR 0x14` change flag bit 4, §6.4)
    /// [observed: vanilla 1.4.0.525 A/B save pairs: player scale].
    pub scale: Option<f32>,
    /// Player faction rank overrides from actor extra data `0x5E` (`-1` = removed)
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub factions: Vec<(Option<u32>, i8)>,
    /// Active teammate/companion references from actor extra data `0x1D`
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub teammates: Vec<Option<u32>>,
    /// The inventory changes, `None` when the record's layout was not understood.
    pub inventory: Option<Vec<FosItem>>,
    /// Active equipped weapon magazine round count from `ACHR 0x14` process-block `tok[76]`
    /// (`u32 |`) when `tok[70] != u32::MAX` (weapon equipped) [observed: vanilla 1.4.0.525 A/B
    /// save pairs].
    pub active_weapon_magazine: Option<u32>,
    /// All base-AV overrides (`av_code < 77`, `SetAV`) from the post-inventory bit-22 list
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub base_av_overrides: Vec<(u8, f32)>,
    /// Skill entries (`32..=45`, `pts >= 0.0`) from the post-inventory bit-22 list
    /// [observed: framing in vanilla 1.4.0.525 A/B save pairs].
    pub skill_points: Vec<(u8, f32)>,
    /// Acquired note form IDs from the player `ACHR 0x14` tail notes list
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub notes: Vec<u32>,
    /// Player perks `(perk_form_id, rank)` from the post-inventory perk list
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub perks: Vec<(Option<u32>, u8)>,
    /// Teammate perks `(perk_form_id, rank)` from the post-inventory teammate-perk list
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub teammate_perks: Vec<(Option<u32>, u8)>,
}

/// Global table 1 type 10: Pip-Boy radio playback state (§5.1, §6.3) [observed: vanilla 1.4.0.525
/// A/B save pairs for the second `vsval`-counted active-station list (`ref3 | u32 playback_ms | ...`);
/// first-list fallback remains [hypothesis]].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FosRadioState {
    pub on: bool,
    pub song: String,
    pub station: Option<u32>,
    /// Playback position in milliseconds from the active Pip-Boy station list, when active.
    pub playback_ms: Option<u32>,
    /// True when decoded from the observed second `vsval`-counted active-station list.
    pub from_active_list: bool,
}

/// An encounter zone's saved level and reset stamps (`ECZN`, kind 32, bit 31; §6.3)
/// [observed: vanilla 1.4.0.525 A/B save pairs].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FosEncounterZone {
    pub form: u32,
    pub stamps: [u32; 3],
    pub level: u32,
}

/// A cell's saved detach time, flags, owner, and fog-of-war presence (`CELL`, kind 7;
/// §6.2) [hypothesis].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FosCell {
    pub form: u32,
    /// Game hours passed at detach (change flag 30) [hypothesis].
    pub detach_hours: Option<u32>,
    /// Cell flags byte (change flag 1) [hypothesis].
    pub cell_flags: Option<u8>,
    /// Cell owner form (change flag 3) [hypothesis].
    pub owner: Option<Option<u32>>,
    /// Whether local-map fog-of-war data is present (change flag 31) [hypothesis].
    pub has_fog: bool,
}

/// A saved lock on a placed door or container (`REFR` extra data `0x2A`, bit 12; §6.2)
/// [hypothesis].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FosLock {
    pub level: u8,
    /// Bit 0 (`0x01`) = locked [hypothesis].
    pub flags: u8,
    pub key: Option<u32>,
    pub num_tries: u32,
    pub num_times_unlocked: u32,
}

impl FosLock {
    pub fn is_locked(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

/// Script variable instance decoded from extra data `0x0D` (`ref3` script + vars)
/// [hypothesis].
#[derive(Debug, Clone, PartialEq)]
pub struct FosScriptVars {
    pub script: Option<u32>,
    pub vars: Vec<(u32, VarValue)>,
}

/// Decoded extra-data, lock, door animation, and optional locked-container inventory for
/// a non-created `REFR` record (§6.2) [hypothesis].
#[derive(Debug, Clone, PartialEq)]
pub struct FosRefExtras {
    pub form: u32,
    pub flags: u32,
    pub form_flags: Option<u32>,
    pub scale: Option<f32>,
    pub lock: Option<FosLock>,
    pub owner: Option<Option<u32>>,
    pub script_vars: Option<FosScriptVars>,
    /// Door open/closed state from bit-23/28 animation block (`"Open"` / `"Close"`).
    pub door_open: Option<bool>,
    /// Inventory for locked containers (when bit 12 + bits 5/27 are set).
    pub inventory: Option<Vec<FosItem>>,
}

/// Decoded per-actor state for a non-player, non-created `ACHR`/`ACRE` record (§6.2,
/// §6.5) [hypothesis].
#[derive(Debug, Clone, PartialEq)]
pub struct FosActorChange {
    pub form: u32,
    pub kind: u8,
    pub flags: u32,
    pub place: Option<Place>,
    pub process_level: u8,
    pub form_flags: Option<u32>,
    pub scale: Option<f32>,
    pub script_vars: Option<FosScriptVars>,
    pub factions: Vec<(Option<u32>, i8)>,
    pub inventory: Option<Vec<FosItem>>,
}

/// Global table 1 type 1: player location and active quest (§5.1, §6.4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FosLocationGlobal {
    pub next_created_id: u32,
    pub world: Option<u32>,
    pub grid: [i32; 2],
    pub cell_or_world: Option<u32>,
    pub pos: [f32; 3],
    pub active_quest: Option<u32>,
}

/// Global table 1 type 8: sky and weather references (§5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FosWeather {
    pub current: Option<u32>,
    pub second: Option<u32>,
}

/// A form list's saved membership (FLST, change flag 31; save-files.md §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FosFormList {
    pub form: u32,
    pub forms: Vec<Option<u32>>,
}

/// One entry in a saved leveled item list (LVLI, change flag 31; save-files.md §6.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FosLeveledEntry {
    pub item: Option<u32>,
    pub level: u16,
    pub count: u16,
    pub condition: f32,
}

/// A leveled item list's saved additions (LVLI, change flag 31; save-files.md §6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct FosLeveledList {
    pub form: u32,
    pub entries: Vec<FosLeveledEntry>,
}

/// A reputation's saved fame and infamy (REPU, change flag 1; save-files.md §6.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FosReputation {
    pub form: u32,
    pub fame: f32,
    pub infamy: f32,
}

/// A challenge's saved progress and state word (CHAL, change flag 1; save-files.md §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FosChallenge {
    pub form: u32,
    pub progress: u32,
    pub flags: u32,
}

/// Per-base-form death counts decoded from global table 1 type 2 (§5.1, §6.3) [hypothesis].
pub type DeadCounts = Vec<(Option<u32>, u16)>;

/// A cell or worldspace, a position and a rotation (radians).
pub type Place = (u32, [f32; 3], [f32; 3]);

/// Form flag bits in a reference's saved flag word (change flag 0, save-files.md §6.2)
/// [hypothesis, each from the references that carry it]: removed from the world (the
/// opening quest's taken note), talked to the player (Doc Mitchell and Ringo), disabled
/// (the ESM's "Initially Disabled" bit, set and cleared at run time).
pub const FORM_DELETED: u32 = 0x20;
pub const FORM_TALKED_TO_PC: u32 = 0x40;
pub const FORM_DISABLED: u32 = 0x800;
/// Actor dead form flag bit in `ACHR`/`ACRE` change flag 0 (`0x0001_0000`) [hypothesis].
pub const FORM_DEAD: u32 = 0x0001_0000;

/// A placed container reference's saved inventory deltas (`REFR` change flags bit 5 and/or
/// bit 27; save-files.md §6.2) [hypothesis].
#[derive(Debug, Clone, PartialEq)]
pub struct FosContainerInventory {
    pub form: u32,
    pub items: Vec<FosItem>,
}

/// A placed map marker reference's saved visibility/travel flags (`REFR` change flag bit 31
/// carrying extra-data type `0x2C`; save-files.md §6.2) [hypothesis: `0x01` visible, `0x02`
/// fast-travelable].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FosMapMarker {
    pub form: u32,
    pub flags: u8,
}

/// A created placed reference (`REFR`, form ID `0xFFxxxxxx`) decoded from its 32-byte header
/// (`cell_or_world_ref3 + 6*f32 + u8 + base_form_ref3 + |`; save-files.md §6.3) [hypothesis].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FosCreatedRef {
    pub form: u32,
    pub flags: u32,
    pub base: Option<u32>,
    pub place: Option<Place>,
    pub header_flag: u8,
}

/// A placed reference's saved changes as far as the lab reads them (save-files.md §6.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FosRefChange {
    pub form: u32,
    /// The changed form's record type: 0 REFR, 1 ACHR, 2 ACRE.
    pub kind: u8,
    /// The reference's whole form flag word, when the save lists it (change flag 0).
    pub form_flags: Option<u32>,
    /// The cell or worldspace, position and rotation (radians), when moved.
    pub place: Option<Place>,
}

/// A faction's saved relation list (FACT change flag 2) [observed: nvfairport's
/// PlayerFaction lists 48 relations, each (faction, i32, u32); the u32 is 0-3 and the i32
/// one of 0, 30, 50, 100, -100] [hypothesis: the XNAM fields, modifier and group combat reaction
/// 0 neutral, 1 enemy, 2 ally, 3 friend].
#[derive(Debug, Clone, PartialEq)]
pub struct FosFaction {
    pub form: u32,
    pub relations: Vec<(Option<u32>, i32, u32)>,
}

/// A parsed `.fos`.
#[derive(Debug, Clone, Default)]
pub struct Fos {
    pub header: FosHeader,
    /// Uncompressed RGB screenshot pixels (`screenshot_width * screenshot_height * 3`, §3.2).
    pub screenshot_rgb: Vec<u8>,
    /// The load order the save was made with; a form ID's top byte indexes it (§4).
    pub plugins: Vec<String>,
    pub form_ids: Vec<u32>,
    pub worlds: Vec<u32>,
    /// Global data entries of both tables: (type, bytes) (§5).
    pub globals_data: Vec<(u32, Vec<u8>)>,
    pub changed: Vec<ChangedForm>,
}

/// The number of actor values in each modifier array of the player's record.
const AV_COUNT: usize = 77;
/// f32 entries between the place and the rest of the player's record: three arrays of
/// [`AV_COUNT`] and one more [observed: 232 in every save looked at].
const AV_ENTRIES: usize = 232;

impl Fos {
    pub fn parse(b: &[u8]) -> Result<Fos, FosError> {
        if !b.starts_with(MAGIC) {
            return err("not a FO3SAVEGAME file");
        }
        let mut c = Cur::new(b, MAGIC.len());
        let header_size = c.u32()? as usize;
        let header_start = c.p;
        let mut h = FosHeader::default();
        c.u32()?;
        c.bar()?;
        c.take(64)?;
        c.bar()?;
        let width = c.u32()? as usize;
        c.bar()?;
        let height = c.u32()? as usize;
        c.bar()?;
        h.screenshot_width = width as u32;
        h.screenshot_height = height as u32;
        h.save_number = c.u32()?;
        c.bar()?;
        h.player_name = c.string_field(true)?;
        h.karma_title = c.string_field(true)?;
        h.level = c.u32()?;
        c.bar()?;
        h.location = c.string_field(true)?;
        h.play_time = c.string_field(false)?;
        // The screenshot follows the header (§3.2).
        let shot_start = header_start
            .checked_add(header_size)
            .ok_or_else(|| FosError("a header size past any file".into()))?;
        let shot_len = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(3))
            .ok_or_else(|| FosError("a screenshot size past any file".into()))?;
        let shot_end = shot_start
            .checked_add(shot_len)
            .filter(|&end| end <= b.len())
            .ok_or_else(|| FosError("a header or screenshot size past any file".into()))?;
        let screenshot_rgb = b[shot_start..shot_end].to_vec();
        c.p = shot_end;
        let _form_version = c.u8()?;
        let plugin_block = c.u32()? as usize;
        let plugins_end = c.p.checked_add(plugin_block).ok_or_else(|| FosError("a plugin block size past any file".into()))?;
        let n = c.u8()?;
        c.bar()?;
        let mut plugins = Vec::new();
        for _ in 0..n {
            plugins.push(c.string_field(true)?);
        }
        c.p = plugins_end;
        let mut t = [0u32; 8];
        for v in &mut t {
            *v = c.u32()?;
        }
        let [fid_off, _tail, g1_off, cf_off, g2_off, g1_count, g2_count, cf_count] = t.map(|v| v as usize);
        let mut a = Cur::new(b, fid_off);
        let nf = a.u32()? as usize;
        let form_ids = (0..nf).map(|_| a.u32()).collect::<Result<Vec<_>, _>>()?;
        let nw = a.u32()? as usize;
        let worlds = (0..nw).map(|_| a.u32()).collect::<Result<Vec<_>, _>>()?;
        let mut globals_data = Vec::new();
        for (off, count) in [(g1_off, g1_count), (g2_off, g2_count)] {
            let mut g = Cur::new(b, off);
            for _ in 0..count {
                let ty = g.u32()?;
                let len = g.u32()? as usize;
                globals_data.push((ty, g.take(len)?.to_vec()));
            }
        }
        let mut fos = Fos { header: h, screenshot_rgb, plugins, form_ids, worlds, globals_data, changed: Vec::new() };
        let mut r = Cur::new(b, cf_off);
        for _ in 0..cf_count {
            let rf = r.ref3()?;
            let flags = r.u32()?;
            let ty = r.u8()?;
            let _version = r.u8()?;
            let len = match ty >> 6 {
                0 => r.u8()? as usize,
                1 => r.u16()? as usize,
                2 => r.u32()? as usize,
                _ => return err(format!("bad length width at {}", r.p)),
            };
            let data = r.take(len)?.to_vec();
            fos.changed.push(ChangedForm { form: fos.resolve(rf).unwrap_or(0), flags, kind: ty & 0x3F, data });
        }
        if r.p != g2_off {
            return err(format!("the changed forms end at {}, not at table 2 ({g2_off})", r.p));
        }
        Ok(fos)
    }

    /// A reference's form ID (§5.4): kind 0 indexes the form ID array (1-based, 0 = none),
    /// kind 1 is a form ID of the first master, kind 2 a created form.
    /// `None` for no reference and for one the save does not hold ([`Fos::reference`]
    /// tells them apart).
    pub fn resolve(&self, r: [u8; 3]) -> Option<u32> {
        self.reference(r).ok().flatten()
    }

    /// [`Fos::resolve`], with `Err` for an index past the form ID array or an unknown kind,
    /// and `Ok(None)` for "no reference".
    pub fn reference(&self, r: [u8; 3]) -> Result<Option<u32>, FosError> {
        let v = u32::from_be_bytes([0, r[0], r[1], r[2]]);
        let value = v & 0x3F_FFFF;
        match v >> 22 {
            0 if value == 0 => Ok(None),
            0 => self.form_ids.get(value as usize - 1).copied().map(Some).ok_or_else(|| FosError(format!("reference {value} past the form ID array"))),
            1 => Ok(Some(value)),
            2 => Ok(Some(0xFF00_0000 | value)),
            k => err(format!("reference of unknown kind {k}")),
        }
    }

    fn global(&self, ty: u32) -> Option<&[u8]> {
        self.globals_data.iter().find(|(t, _)| *t == ty).map(|(_, d)| d.as_slice())
    }

    /// The global variables (global table type 3): (form ID, value).
    pub fn global_variables(&self) -> Result<Vec<(u32, f32)>, FosError> {
        let Some(d) = self.global(3) else { return Ok(Vec::new()) };
        let mut c = Cur::new(d, 0);
        let n = c.vsval()?;
        c.bar()?;
        let mut out = Vec::new();
        for _ in 0..n {
            let r = c.ref3()?;
            c.bar()?;
            let v = c.f32()?;
            c.bar()?;
            if let Some(id) = self.resolve(r) {
                out.push((id, v));
            }
        }
        Ok(out)
    }

    /// Global table 1 type 0: miscellaneous statistics in code order (43 entries, §5.1).
    pub fn misc_stats(&self) -> Result<Vec<u32>, FosError> {
        let Some(d) = self.global(0) else { return Ok(Vec::new()) };
        let mut c = Cur::new(d, 0);
        let n = c.u32()? as usize;
        c.bar()?;
        let mut out = Vec::with_capacity(n.min(256));
        for _ in 0..n {
            out.push(c.u32()?);
            c.bar()?;
        }
        Ok(out)
    }

    /// Global table 1 type 1: player location, created-form counter and active quest
    /// (§5.1, §6.4).
    pub fn player_location_global(&self) -> Option<FosLocationGlobal> {
        let d = self.global(1)?;
        let mut c = Cur::new(d, 0);
        let next_created_id = c.u32().ok()?;
        c.bar().ok()?;
        let world = self.resolve(c.ref3().ok()?);
        c.bar().ok()?;
        let gx = c.i32().ok()?;
        c.bar().ok()?;
        let gy = c.i32().ok()?;
        c.bar().ok()?;
        let cell_or_world = self.resolve(c.ref3().ok()?);
        c.bar().ok()?;
        let px = c.f32().ok()?;
        let py = c.f32().ok()?;
        let pz = c.f32().ok()?;
        c.bar().ok()?;
        let active_quest = self.resolve(c.ref3().ok()?);
        Some(FosLocationGlobal {
            next_created_id,
            world,
            grid: [gx, gy],
            cell_or_world,
            pos: [px, py, pz],
            active_quest,
        })
    }

    /// The quest that global table 1's type 1 entry names (§5.1) [hypothesis: the active
    /// quest].
    pub fn active_quest(&self) -> Option<u32> {
        self.player_location_global()?.active_quest
    }

    /// Global table 1 type 2 prefix: `vsval` count, `|`, then `count × (ref3, |, u16, |)`
    /// followed by unmapped trailing bytes (§5.1, §6.3) [hypothesis: per-base-form death
    /// counts].
    pub fn dead_counts(&self) -> Result<DeadCounts, FosError> {
        Ok(self.dead_counts_with_tail()?.0)
    }

    /// Like [`Fos::dead_counts`], also returning the number of unmapped trailing bytes
    /// remaining in global table 1 type 2 after the parsed `(ref, |, u16, |)` prefix.
    pub fn dead_counts_with_tail(&self) -> Result<(DeadCounts, usize), FosError> {
        let Some(d) = self.global(2) else {
            return Ok((Vec::new(), 0));
        };
        let mut c = Cur::new(d, 0);
        let n = c.vsval()? as usize;
        c.bar()?;
        let mut out = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            let base = self.reference(c.ref3()?).ok().flatten();
            c.bar()?;
            let count = c.u16()?;
            c.bar()?;
            out.push((base, count));
        }
        Ok((out, d.len().saturating_sub(c.p)))
    }

    /// Global table 1 type 8: current and second weather references (§5.1).
    pub fn weather(&self) -> Result<Option<FosWeather>, FosError> {
        let Some(d) = self.global(8) else { return Ok(None) };
        let mut c = Cur::new(d, 0);
        let current = self.resolve(c.ref3()?);
        c.bar()?;
        let second = self.resolve(c.ref3()?);
        c.bar()?;
        Ok(Some(FosWeather { current, second }))
    }

    /// Global table 1 type 10: Pip-Boy radio playback state (§5.1, §6.3).
    ///
    /// After the opening `u32 | u8 | string | vsval n1 | n1 * (ref3 |)` header, the immediately
    /// following `vsval`-counted list holds the active Pip-Boy radio station entry
    /// (`ref3 station | u32 playback_ms | u32 | u8 | u8 | u32 | u8 | u8 |`) [observed: vanilla
    /// 1.4.0.525 A/B save pairs]. When the second list is absent or empty, falls back to the
    /// opening `u8 on` byte and first list [hypothesis].
    pub fn radio_state(&self) -> Result<Option<FosRadioState>, FosError> {
        let Some(d) = self.global(10) else { return Ok(None) };
        if d.len() < 8 {
            return Ok(None);
        }
        let mut c = Cur::new(d, 0);
        let _u0 = c.u32()?;
        c.bar()?;
        let on_byte = c.u8()?;
        c.bar()?;
        if on_byte > 1 {
            return err(format!("unexpected radio state flag {on_byte}"));
        }
        let song = c.string_field(true)?;
        let n1 = c.vsval()?;
        c.bar()?;
        let mut list1_station = None;
        for i in 0..n1 {
            let r = self.reference(c.ref3()?).ok().flatten();
            c.bar()?;
            if i == 0 {
                list1_station = r;
            }
        }
        if !c.at_end() {
            let n2 = c.vsval()?;
            c.bar()?;
            if n2 > 0 {
                let station = self.reference(c.ref3()?).ok().flatten();
                c.bar()?;
                let playback_ms = c.u32()?;
                c.bar()?;
                // Verify the remaining fields of the 27-byte active-station entry when present.
                if !c.at_end() {
                    c.u32()?;
                    c.bar()?;
                    c.u8()?;
                    c.bar()?;
                    c.u8()?;
                    c.bar()?;
                    c.u32()?;
                    c.bar()?;
                    c.u8()?;
                    c.bar()?;
                    c.u8()?;
                    c.bar()?;
                }
                return Ok(Some(FosRadioState {
                    on: true,
                    song,
                    station,
                    playback_ms: Some(playback_ms),
                    from_active_list: true,
                }));
            }
            if on_byte == 0 {
                return Ok(Some(FosRadioState {
                    on: false,
                    song,
                    station: None,
                    playback_ms: None,
                    from_active_list: true,
                }));
            }
        }
        Ok(Some(FosRadioState {
            on: on_byte != 0,
            song,
            station: list1_station,
            playback_ms: None,
            from_active_list: false,
        }))
    }

    /// Encounter zones changed in play (`ECZN`, kind 32, bit 31: four `u32` LE followed
    /// by `|`, where the fourth is the locked zone level; §6.3)
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub fn encounter_zones(&self) -> Vec<Result<FosEncounterZone, FosError>> {
        self.records(TYPE_ECZN)
            .filter(|f| f.has(31))
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let s0 = c.u32()?;
                let s1 = c.u32()?;
                let s2 = c.u32()?;
                let level = c.u32()?;
                c.bar()?;
                if !c.at_end() {
                    return err("bytes left after encounter zone");
                }
                Ok(FosEncounterZone { form: f.form, stamps: [s0, s1, s2], level })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("encounter zone: {e}"))))
            .collect()
    }

    /// Cells changed in play (`CELL`, kind 7: optional bit 29 4-byte exterior header,
    /// bit 30 `u32 |` detach hours, bit 1 `u8 |` flags, bit 31 local-map fog, and bit 3
    /// `ref3 |` owner serialized AFTER bit-31 fog; §6.2) [observed: vanilla 1.4.0.525 A/B
    /// save pairs for bits 1, 3, 30, 31; bit 29 remains [hypothesis]].
    pub fn cells(&self) -> Vec<Result<FosCell, FosError>> {
        const KNOWN_BITS: u32 = (1 << 1) | (1 << 3) | (1 << 29) | (1 << 30) | (1 << 31);
        self.records(TYPE_CELL)
            .filter(|f| f.flags != 0 && (f.flags & !KNOWN_BITS) == 0)
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                if f.has(29) {
                    c.take(4)?;
                }
                let detach_hours = if f.has(30) {
                    let h = c.u32()?;
                    c.bar()?;
                    Some(h)
                } else {
                    None
                };
                let cell_flags = if f.has(1) {
                    let fl = c.u8()?;
                    c.bar()?;
                    Some(fl)
                } else {
                    None
                };
                let has_fog = f.has(31);
                let owner = if f.has(3) {
                    if has_fog {
                        // Bit 3 (cell owner `ref3 |`) is serialized AFTER the bit-31 fog block.
                        if f.data.len() < c.p + 4 {
                            return err("cell bit 3 owner truncated after fog");
                        }
                        let mut oc = Cur::new(&f.data, f.data.len() - 4);
                        let o = self.reference(oc.ref3()?).ok().flatten();
                        oc.bar()?;
                        Some(o)
                    } else {
                        let o = self.reference(c.ref3()?).ok().flatten();
                        c.bar()?;
                        Some(o)
                    }
                } else {
                    None
                };
                if !has_fog && !c.at_end() {
                    return err(format!("{} bytes left after cell", f.data.len() - c.p));
                }
                Ok(FosCell { form: f.form, detach_hours, cell_flags, owner, has_fog })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("cell: {e}"))))
            .collect()
    }

    /// Dialogue responses marked said-once (INFO, change flag 31, empty data; §6.2).
    pub fn said_infos(&self) -> Vec<u32> {
        self.records(TYPE_INFO)
            .filter(|f| f.has(31) && f.data.is_empty() && f.form != 0)
            .map(|f| f.form)
            .collect()
    }

    /// Empty-payload `TYPE_NOTE` changed forms (change flag 31, empty data; §6.3)
    /// [hypothesis: opened/read notes].
    pub fn note_records(&self) -> Vec<u32> {
        self.records(TYPE_NOTE)
            .filter(|f| f.has(31) && f.data.is_empty() && f.form != 0)
            .map(|f| f.form)
            .collect()
    }

    /// Notes acquired by the player stored in the `ACHR 0x14` tail notes list (§6.4)
    /// [observed: vanilla 1.4.0.525 A/B save pairs].
    pub fn player_tail_notes(&self) -> Vec<u32> {
        let Some(r) = self.record(PLAYER_REF, TYPE_ACHR) else {
            return Vec::new();
        };
        self.parse_player_notes(&r.data)
    }

    /// All notes owned by the player: deduplicated union of the observed `ACHR 0x14` tail
    /// notes list (§6.4) and any `TYPE_NOTE` change-flag-31 records (§6.3).
    pub fn notes(&self) -> Vec<u32> {
        let mut out = self.player_tail_notes();
        for id in self.note_records() {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    /// Form lists changed by script (FLST, change flag 31: `u32 count |`, then `count * (ref |)`)
    /// [hypothesis: save-files.md §6.2; count width not re-verified against named saves].
    pub fn form_lists(&self) -> Vec<Result<FosFormList, FosError>> {
        self.records(TYPE_FLST)
            .filter(|f| f.has(31))
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let n = c.u32()? as usize;
                c.bar()?;
                let mut forms = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let r = self.reference(c.ref3()?).ok().flatten();
                    c.bar()?;
                    forms.push(r);
                }
                if !c.at_end() {
                    return err("bytes left after form list entries");
                }
                Ok(FosFormList { form: f.form, forms })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("form list: {e}"))))
            .collect()
    }

    /// Leveled item lists changed by script (LVLI, change flag 31: `vsval count |`, then
    /// `ref | u16 level | u16 count | f32 condition |`; §6.2) [hypothesis].
    pub fn leveled_lists(&self) -> Vec<Result<FosLeveledList, FosError>> {
        self.records(TYPE_LVLI)
            .filter(|f| f.has(31))
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let n = c.vsval()? as usize;
                c.bar()?;
                let mut entries = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let item = self.reference(c.ref3()?).ok().flatten();
                    c.bar()?;
                    let level = c.u16()?;
                    c.bar()?;
                    let count = c.u16()?;
                    c.bar()?;
                    let condition = c.f32()?;
                    c.bar()?;
                    entries.push(FosLeveledEntry { item, level, count, condition });
                }
                if !c.at_end() {
                    return err("bytes left after leveled list entries");
                }
                Ok(FosLeveledList { form: f.form, entries })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("leveled list: {e}"))))
            .collect()
    }

    /// Reputations changed in play (REPU, change flag 1: `f32 fame | f32 infamy |`; §6.3)
    /// [hypothesis].
    pub fn reputations(&self) -> Vec<Result<FosReputation, FosError>> {
        self.records(TYPE_REPU)
            .filter(|f| f.has(1))
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let fame = c.f32()?;
                c.bar()?;
                let infamy = c.f32()?;
                c.bar()?;
                if !c.at_end() {
                    return err("bytes left after reputation");
                }
                Ok(FosReputation { form: f.form, fame, infamy })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("reputation: {e}"))))
            .collect()
    }

    /// Challenges changed in play (CHAL, change flag 1: `u32 progress | u32 flags |`; §6.2)
    /// [hypothesis].
    pub fn challenges(&self) -> Vec<Result<FosChallenge, FosError>> {
        self.records(TYPE_CHAL)
            .filter(|f| f.has(1))
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let progress = c.u32()?;
                c.bar()?;
                let flags = c.u32()?;
                c.bar()?;
                if !c.at_end() {
                    return err("bytes left after challenge");
                }
                Ok(FosChallenge { form: f.form, progress, flags })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("challenge: {e}"))))
            .collect()
    }

    fn records(&self, kind: u8) -> impl Iterator<Item = &ChangedForm> {
        self.changed.iter().filter(move |f| f.kind == kind)
    }

    fn record(&self, form: u32, kind: u8) -> Option<&ChangedForm> {
        self.records(kind).find(|f| f.form == form)
    }

    /// Every quest record's changes (§6.2), each on its own: a record the reader does not
    /// understand is an `Err` and leaves the others.
    pub fn quests(&self) -> Vec<Result<FosQuest, FosError>> {
        self.records(TYPE_QUST).map(|f| self.quest(f).map_err(|e| FosError(format!("quest {:08X}: {e}", f.form)))).collect()
    }

    fn quest(&self, f: &ChangedForm) -> Result<FosQuest, FosError> {
        let mut q = FosQuest { form: f.form, ..FosQuest::default() };
        let mut c = Cur::new(&f.data, 0);
        if f.has(1) {
            q.flags = Some(c.u8()?);
            c.bar()?;
        }
        if f.has(2) {
            q.delay = Some(c.f32()?);
            c.bar()?;
        }
        if f.has(31) {
            let n = c.vsval()?;
            c.bar()?;
            for _ in 0..n {
                let stage = c.u8()?;
                c.bar()?;
                let done = c.u8()? != 0;
                c.bar()?;
                q.stages.push((stage, done));
                let entries = c.vsval()?;
                c.bar()?;
                for _ in 0..entries {
                    c.u8()?;
                    c.bar()?;
                    let done = c.u8()? != 0;
                    c.bar()?;
                    if done {
                        // A date stamp (§6.2).
                        c.take(4)?;
                        c.bar()?;
                    }
                }
            }
        }
        if f.has(30) {
            let n = c.vsval()?;
            c.bar()?;
            for _ in 0..n {
                let index = c.u32()?;
                c.bar()?;
                let value = if index & 0x8000_0000 != 0 {
                    self.reference(c.ref3()?).map_or(VarValue::Unresolved, VarValue::Ref)
                } else {
                    VarValue::Num(c.f64()?)
                };
                c.bar()?;
                q.vars.push((index & 0x7FFF_FFFF, value));
            }
            // Two more lists, empty in every quest looked at.
            for _ in 0..2 {
                if c.vsval()? != 0 {
                    return err("a non-empty list after the variables");
                }
                c.bar()?;
            }
        }
        if f.has(29) {
            let n = c.vsval()?;
            c.bar()?;
            for _ in 0..n {
                let index = c.u32()?;
                c.bar()?;
                let state = c.u32()?;
                c.bar()?;
                q.objectives.push((index, state));
            }
        }
        if !c.at_end() {
            return err(format!("{} bytes left over", f.data.len() - c.p));
        }
        Ok(q)
    }

    /// The flag word and place of every non-created placed reference but the player (record
    /// types REFR, ACHR, ACRE) whose change flags have a layout the reader knows
    /// (save-files.md §6.2): REFR flag 0 alone (the flag word), REFR flags 1 or 2 without 0
    /// (the place), ACHR/ACRE flag 1 without 2 (the place, a byte, then the flag word when
    /// flag 0 is set). Created `0xFF` `REFR`s use a 32-byte header decoded by
    /// [`Fos::created_refs`]. Any other record of these types, or one whose markers do not
    /// fit, is an `Err` and leaves the others.
    pub fn reference_changes(&self) -> Vec<Result<FosRefChange, FosError>> {
        self.changed
            .iter()
            .filter(|f| {
                f.kind <= TYPE_ACRE
                    && f.form != PLAYER_REF
                    && (f.form >> 24) != 0xFF
                    && f.flags & 0b111 != 0
            })
            .map(|f| self.reference_change(f).map_err(|e| FosError(format!("reference {:08X}: {e}", f.form))))
            .collect()
    }

    /// Non-created container `REFR` inventories (change flags bit 5 and/or bit 27, with
    /// optional bit 28 trailing block; save-files.md §6.2) [hypothesis].
    pub fn container_inventories(&self) -> Vec<Result<FosContainerInventory, FosError>> {
        const INV_BITS: u32 = (1 << 5) | (1 << 27);
        const ALLOWED: u32 = INV_BITS | (1 << 28);
        self.records(TYPE_REFR)
            .filter(|f| (f.form >> 24) != 0xFF && (f.flags & INV_BITS) != 0 && (f.flags & !ALLOWED) == 0)
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let items = self.parse_item_list(&mut c)?;
                if !f.has(28) && !c.at_end() {
                    return err(format!("{} bytes left after container inventory", f.data.len() - c.p));
                }
                Ok(FosContainerInventory { form: f.form, items })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("container: {e}"))))
            .collect()
    }

    /// Map marker visibility/travel flags from non-created `REFR` records with change flag
    /// bit 31 (and optionally bit 29) whose extra-data list carries type `0x2C` (`u8 |`) and
    /// optional `0x74` (`ref3 |`) (save-files.md §6.2) [hypothesis]. Bit-31 `REFR` records
    /// with other extra-data types or payloads are omitted rather than returned as errors.
    pub fn map_markers(&self) -> Vec<Result<FosMapMarker, FosError>> {
        const ALLOWED: u32 = (1 << 29) | (1 << 31);
        self.records(TYPE_REFR)
            .filter(|f| (f.form >> 24) != 0xFF && (f.flags & (1 << 31)) != 0 && (f.flags & !ALLOWED) == 0)
            .filter_map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let count = c.vsval().ok()?;
                c.bar().ok()?;
                let mut marker_flags = None;
                for _ in 0..count {
                    let ty = c.u8().ok()?;
                    c.bar().ok()?;
                    match ty {
                        0x2C => {
                            marker_flags = Some(c.u8().ok()?);
                            c.bar().ok()?;
                        }
                        0x74 => {
                            c.ref3().ok()?;
                            c.bar().ok()?;
                        }
                        _ => return None,
                    }
                }
                let flags = marker_flags?;
                if !c.at_end() {
                    return Some(err(format!(
                        "map marker {:08X}: {} bytes left after extra list",
                        f.form,
                        f.data.len() - c.p
                    )));
                }
                Some(Ok(FosMapMarker { form: f.form, flags }))
            })
            .collect()
    }

    /// Created placed references (`REFR`, `form >> 24 == 0xFF`) decoded from their 32-byte
    /// header (`cell_or_world_ref3 + 6*f32 + u8 + base_form_ref3 + |`; save-files.md §6.3)
    /// [hypothesis].
    pub fn created_refs(&self) -> Vec<Result<FosCreatedRef, FosError>> {
        self.records(TYPE_REFR)
            .filter(|f| (f.form >> 24) == 0xFF)
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let at = self.reference(c.ref3()?).ok().flatten();
                let mut v = [0.0f32; 6];
                for x in &mut v {
                    *x = c.f32()?;
                }
                let header_flag = c.u8()?;
                let base = self.reference(c.ref3()?).ok().flatten();
                c.bar()?;
                if !v.iter().all(|x| x.is_finite()) {
                    return err("non-finite position or rotation in created ref");
                }
                let place = at.map(|a| (a, [v[0], v[1], v[2]], [v[3], v[4], v[5]]));
                Ok(FosCreatedRef { form: f.form, flags: f.flags, base, place, header_flag })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("created ref: {e}"))))
            .collect()
    }

    fn reference_change(&self, f: &ChangedForm) -> Result<FosRefChange, FosError> {
        let mut r = FosRefChange { form: f.form, kind: f.kind, form_flags: None, place: None };
        let mut c = Cur::new(&f.data, 0);
        let flag_word = |c: &mut Cur<'_>| -> Result<u32, FosError> {
            let v = c.u32()?;
            c.bar()?;
            Ok(v)
        };
        match (f.kind, f.has(0), f.has(1), f.has(2)) {
            (TYPE_REFR, true, false, false) => r.form_flags = Some(flag_word(&mut c)?),
            (TYPE_REFR, false, _, _) => r.place = self.place(&mut c)?,
            (TYPE_ACHR | TYPE_ACRE, flags, true, false) => {
                r.place = self.place(&mut c)?;
                c.u8()?;
                c.bar()?;
                if flags {
                    r.form_flags = Some(flag_word(&mut c)?);
                }
            }
            (TYPE_ACHR | TYPE_ACRE, true, false, false) => {
                c.u8()?;
                c.bar()?;
                r.form_flags = Some(flag_word(&mut c)?);
            }
            _ => return err(format!("change flags {:#010x}: layout not known", f.flags)),
        }
        Ok(r)
    }

    /// Every faction record's relation list (FACT, change flag 2; save-files.md §6.2):
    /// (faction, [(other faction or `None` outside the form ID array, modifier, group
    /// combat reaction)]). A record the reader does not understand is an `Err`.
    pub fn faction_relations(&self) -> Vec<Result<FosFaction, FosError>> {
        self.records(TYPE_FACT)
            .filter(|f| f.has(2))
            .map(|f| {
                let mut c = Cur::new(&f.data, 0);
                let n = c.vsval()?;
                c.bar()?;
                let mut relations = Vec::new();
                for _ in 0..n {
                    let other = self.reference(c.ref3()?).ok().flatten();
                    c.bar()?;
                    let modifier = c.i32()?;
                    c.bar()?;
                    let reaction = c.u32()?;
                    c.bar()?;
                    relations.push((other, modifier, reaction));
                }
                if !c.at_end() {
                    return err("bytes left after the relations");
                }
                Ok(FosFaction { form: f.form, relations })
            })
            .map(|r| r.map_err(|e: FosError| FosError(format!("faction: {e}"))))
            .collect()
    }

    /// A place: a cell or worldspace reference, six f32 (position, rotation), `|`.
    fn place(&self, c: &mut Cur<'_>) -> Result<Option<Place>, FosError> {
        let at = self.reference(c.ref3()?)?;
        let mut v = [0f32; 6];
        for x in &mut v {
            *x = c.f32()?;
        }
        c.bar()?;
        Ok(at.map(|at| (at, [v[0], v[1], v[2]], [v[3], v[4], v[5]])))
    }

    /// Parse an extra-data `0x0D` script-variable instance (`ref3 | vsval nv | nv * (u32 |
    /// ref3/f64 |) | vsval 0 | vsval 0 |`) [hypothesis].
    fn parse_script_extra(&self, c: &mut Cur<'_>) -> Result<FosScriptVars, FosError> {
        let script = self.reference(c.ref3()?).ok().flatten();
        c.bar()?;
        let nv = c.vsval()?;
        c.bar()?;
        let mut vars = Vec::with_capacity((nv as usize).min(1024));
        for _ in 0..nv {
            let idx = c.u32()?;
            c.bar()?;
            let value = if idx & 0x8000_0000 != 0 {
                self.reference(c.ref3()?).map_or(VarValue::Unresolved, VarValue::Ref)
            } else {
                VarValue::Num(c.f64()?)
            };
            c.bar()?;
            vars.push((idx & 0x7FFF_FFFF, value));
        }
        for _ in 0..2 {
            if c.vsval()? != 0 {
                return err("non-empty trailing list in script extra");
            }
            c.bar()?;
        }
        Ok(FosScriptVars { script, vars })
    }

    /// Decoded extra-data, lock (`0x2A`), owner (`0x21`, bit 6/31), script variables (`0x0D`),
    /// door open animation (bit 23/28), and container inventory combined with extra-data lists
    /// (bits 6/10/12/29/31 + bits 5/27) for non-created `REFR` records (§6.2) [observed: vanilla
    /// 1.4.0.525 A/B save pairs for bit 4 scale, bit 6 `0x21` owner, bit 10 `0x1C` original/holder
    /// ref, bit 12 `0x2A` lock, `0x22`/`0x23`/`0x25`/`0x30`/`0x39`/`0x6E` extras, bits 5/27
    /// container inventories, and bit 28 Open/Close; auxiliary extras remain [hypothesis]].
    pub fn ref_extras(&self) -> Vec<FosRefExtras> {
        const INTERESTING: u32 = (1 << 6) | (1 << 10) | (1 << 12) | (1 << 23) | (1 << 28) | (1 << 31);
        const INV_BITS: u32 = (1 << 5) | (1 << 27);
        const EXTRA_LIST_BITS: u32 = (1 << 6) | (1 << 10) | (1 << 12) | (1 << 29) | (1 << 31);
        self.records(TYPE_REFR)
            .filter(|f| (f.form >> 24) != 0xFF && (f.flags & INTERESTING) != 0 && (f.flags & 0b110) == 0)
            .filter_map(|f| self.parse_ref_extras(f, INV_BITS, EXTRA_LIST_BITS).ok().flatten())
            .collect()
    }

    fn parse_ref_extras(
        &self,
        f: &ChangedForm,
        inv_bits: u32,
        extra_list_bits: u32,
    ) -> Result<Option<FosRefExtras>, FosError> {
        let mut c = Cur::new(&f.data, 0);
        let form_flags = if f.has(0) {
            let v = c.u32()?;
            c.bar()?;
            Some(v)
        } else {
            None
        };
        let scale = if f.has(4) {
            let s = c.f32()?;
            c.bar()?;
            Some(s)
        } else {
            None
        };
        let mut lock = None;
        let mut owner = None;
        let mut script_vars = None;
        if f.flags & extra_list_bits != 0 {
            let count = c.vsval()?;
            c.bar()?;
            for _ in 0..count {
                let ty = c.u8()?;
                c.bar()?;
                match ty {
                    0x0D => script_vars = Some(self.parse_script_extra(&mut c)?),
                    0x18 => {
                        c.ref3()?;
                        c.bar()?;
                        c.take(12)?;
                        c.bar()?;
                        c.f32()?;
                        c.bar()?;
                    }
                    0x1B | 0x7C => {
                        let n = c.vsval()?;
                        c.bar()?;
                        for _ in 0..n {
                            c.ref3()?;
                            c.bar()?;
                        }
                    }
                    0x1C | 0x22 | 0x39 | 0x46 | 0x74 => {
                        c.ref3()?;
                        c.bar()?;
                    }
                    0x21 => {
                        owner = Some(self.reference(c.ref3()?).ok().flatten());
                        c.bar()?;
                    }
                    0x23 => {
                        c.i32()?;
                        c.bar()?;
                    }
                    0x25 | 0x26 | 0x28 | 0x30 => {
                        c.f32()?;
                        c.bar()?;
                    }
                    0x2A => {
                        let level = c.u8()?;
                        c.bar()?;
                        let flags = c.u8()?;
                        c.bar()?;
                        let key = self.reference(c.ref3()?).ok().flatten();
                        c.bar()?;
                        let num_tries = c.u32()?;
                        c.bar()?;
                        let num_times_unlocked = c.u32()?;
                        c.bar()?;
                        lock = Some(FosLock { level, flags, key, num_tries, num_times_unlocked });
                    }
                    0x2C | 0x73 => {
                        c.u8()?;
                        c.bar()?;
                    }
                    0x2E => {
                        c.ref3()?;
                        c.bar()?;
                        c.ref3()?;
                        c.bar()?;
                        c.u32()?;
                        c.bar()?;
                    }
                    0x2F => {
                        c.u32()?;
                        c.bar()?;
                        c.u8()?;
                        c.bar()?;
                    }
                    0x56 | 0x60 => {
                        c.u32()?;
                        c.bar()?;
                    }
                    0x6E => {
                        c.ref3()?;
                        c.bar()?;
                        c.u32()?;
                        c.bar()?;
                    }
                    0x75 => {
                        c.ref3()?;
                        c.bar()?;
                        c.ref3()?;
                        c.bar()?;
                        c.u8()?;
                        c.bar()?;
                    }
                    0x8B => {
                        c.take(12)?;
                        c.bar()?;
                        c.ref3()?;
                        c.bar()?;
                        c.u32()?;
                        c.bar()?;
                        c.u8()?;
                        c.bar()?;
                    }
                    _ => return Ok(None),
                }
            }
        }
        // Locked containers (bit 12 + bits 5/27) carry their item list right after the extra list.
        let inventory = if (f.flags & inv_bits) != 0 && (f.flags & extra_list_bits) != 0 {
            Some(self.parse_item_list(&mut c)?)
        } else {
            None
        };
        // Bit 23 / bit 28 door animation state: `0x28 | u16 len | anim_name |`
        let mut door_open = None;
        if (f.has(23) || f.has(28)) && !c.at_end() {
            let mut ac = c.clone();
            if ac.u8().ok() == Some(0x28) && ac.bar().is_ok() {
                if let Ok(anim) = ac.string_field(true) {
                    if anim.eq_ignore_ascii_case("Open") {
                        door_open = Some(true);
                    } else if anim.eq_ignore_ascii_case("Close") {
                        door_open = Some(false);
                    }
                }
            }
        }
        if lock.is_none() && owner.is_none() && script_vars.is_none() && door_open.is_none() && inventory.is_none() {
            return Ok(None);
        }
        Ok(Some(FosRefExtras {
            form: f.form,
            flags: f.flags,
            form_flags,
            scale,
            lock,
            owner,
            script_vars,
            door_open,
            inventory,
        }))
    }

    /// Per-actor decoded changes for non-player, non-created `ACHR`/`ACRE` records
    /// without bit-2 ragdoll prefix: place, process level, form flags (`FORM_DEAD` etc.),
    /// scale, actor extra data (`0x0D` script vars, `0x5E` faction ranks), and inventory
    /// (bits 5/27) (§6.2, §6.5) [hypothesis]. Records with unknown extra types fail soft
    /// after extracting any prefix fields already verified by `|`.
    pub fn actor_changes(&self) -> Vec<FosActorChange> {
        self.changed
            .iter()
            .filter(|f| {
                matches!(f.kind, TYPE_ACHR | TYPE_ACRE)
                    && f.form != PLAYER_REF
                    && (f.form >> 24) != 0xFF
                    && !f.has(2)
            })
            .filter_map(|f| self.parse_actor_change(f).ok())
            .collect()
    }

    fn parse_actor_change(&self, f: &ChangedForm) -> Result<FosActorChange, FosError> {
        const EXTRA_BITS: u32 = (1 << 11) | (1 << 17) | (1 << 18) | (1 << 29) | (1 << 31);
        const INV_BITS: u32 = (1 << 5) | (1 << 27);
        let mut c = Cur::new(&f.data, 0);
        let place = if f.has(1) { self.place(&mut c)? } else { None };
        let process_level = c.u8()?;
        c.bar()?;
        let form_flags = if f.has(0) {
            let v = c.u32()?;
            c.bar()?;
            Some(v)
        } else {
            None
        };
        let scale = if f.has(4) {
            let s = c.f32()?;
            c.bar()?;
            Some(s)
        } else {
            None
        };
        let mut script_vars = None;
        let mut factions = Vec::new();
        let mut extras_ok = true;
        if f.flags & EXTRA_BITS != 0 {
            let mut ec = c.clone();
            let parsed = (|| -> Result<(), FosError> {
                let count = ec.vsval()?;
                ec.bar()?;
                for _ in 0..count {
                    let ty = ec.u8()?;
                    ec.bar()?;
                    match ty {
                        0x04 => {
                            ec.ref3()?;
                            ec.bar()?;
                            ec.u8()?;
                            ec.bar()?;
                        }
                        0x0D => script_vars = Some(self.parse_script_extra(&mut ec)?),
                        0x18 => {
                            ec.ref3()?;
                            ec.bar()?;
                            ec.take(12)?;
                            ec.bar()?;
                            ec.f32()?;
                            ec.bar()?;
                        }
                        0x19 => {
                            ec.ref3()?;
                            ec.bar()?;
                            ec.ref3()?;
                            ec.bar()?;
                            ec.u32()?;
                            ec.bar()?;
                            ec.u8()?;
                            ec.bar()?;
                            ec.u8()?;
                            ec.bar()?;
                            ec.u8()?;
                            ec.bar()?;
                        }
                        0x1B | 0x1D | 0x7C => {
                            let n = ec.vsval()?;
                            ec.bar()?;
                            for _ in 0..n {
                                ec.ref3()?;
                                ec.bar()?;
                            }
                        }
                        0x1F => {}
                        0x2E => {
                            ec.ref3()?;
                            ec.bar()?;
                            ec.ref3()?;
                            ec.bar()?;
                            ec.u32()?;
                            ec.bar()?;
                        }
                        0x46 | 0x74 => {
                            ec.ref3()?;
                            ec.bar()?;
                        }
                        0x56 | 0x60 => {
                            ec.u32()?;
                            ec.bar()?;
                        }
                        0x5E => {
                            let n = ec.vsval()?;
                            ec.bar()?;
                            for _ in 0..n {
                                let fac = self.reference(ec.ref3()?).ok().flatten();
                                ec.bar()?;
                                let rank = ec.u8()? as i8;
                                ec.bar()?;
                                factions.push((fac, rank));
                            }
                        }
                        0x73 => {
                            ec.u8()?;
                            ec.bar()?;
                        }
                        0x75 => {
                            ec.ref3()?;
                            ec.bar()?;
                            ec.ref3()?;
                            ec.bar()?;
                            ec.u8()?;
                            ec.bar()?;
                        }
                        0x8B => {
                            ec.take(12)?;
                            ec.bar()?;
                            ec.ref3()?;
                            ec.bar()?;
                            ec.u32()?;
                            ec.bar()?;
                            ec.u8()?;
                            ec.bar()?;
                        }
                        t => return err(format!("actor extra {t:#04x}")),
                    }
                }
                Ok(())
            })();
            if parsed.is_ok() {
                c = ec;
            } else {
                extras_ok = false;
            }
        }
        let inventory = if extras_ok && (f.flags & INV_BITS) != 0 {
            self.parse_item_list(&mut c).ok()
        } else {
            None
        };
        Ok(FosActorChange {
            form: f.form,
            kind: f.kind,
            flags: f.flags,
            place,
            process_level,
            form_flags,
            scale,
            script_vars,
            factions,
            inventory,
        })
    }

    /// The player's place, base stats/spells/appearance, tags, AV arrays, actor extras,
    /// inventory, spent skill points, and perks.
    pub fn player(&self) -> Result<FosPlayer, FosError> {
        let mut p = FosPlayer::default();
        if let Some(npc) = self.record(PLAYER_BASE, TYPE_NPC) {
            let _ = self.parse_player_base(npc, &mut p);
        }
        if let Some(clas) = self.record(PLAYER_CLASS, TYPE_CLAS).filter(|f| f.has(1)) {
            let mut c = Cur::new(&clas.data, 0);
            for _ in 0..4 {
                let v = c.u32()?;
                c.bar()?;
                if v != u32::MAX {
                    p.tag_skills.push(v);
                }
            }
        }
        let Some(r) = self.record(PLAYER_REF, TYPE_ACHR) else { return Ok(p) };
        let mut c = Cur::new(&r.data, 0);
        let at = self.resolve(c.ref3()?);
        let mut v = [0f32; 6];
        for x in &mut v {
            *x = c.f32()?;
        }
        c.bar()?;
        if let Some(at) = at {
            p.place = Some((at, [v[0], v[1], v[2]], [v[3], v[4], v[5]]));
        }
        // Parse the 232-entry AV block fail-soft: saves with unmapped player change-flag
        // blocks before the AV array still keep place, SPECIAL, and tag skills intact.
        let mut av_cur = c.clone();
        let mut mods = Vec::new();
        let mut base = Vec::new();
        let mut dmg = Vec::new();
        let mut health_damage = None;
        let mut av_ok = true;
        for i in 0..AV_ENTRIES {
            let Ok(x) = av_cur.f32() else {
                av_ok = false;
                break;
            };
            if av_cur.bar().is_err() {
                av_ok = false;
                break;
            }
            if i < AV_COUNT && x != 0.0 {
                mods.push((i as u32, x));
            } else if (AV_COUNT..2 * AV_COUNT).contains(&i) && x != 0.0 {
                base.push(((i - AV_COUNT) as u32, x));
            } else if (2 * AV_COUNT..3 * AV_COUNT).contains(&i) && x != 0.0 {
                dmg.push(((i - 2 * AV_COUNT) as u32, x));
            } else if i == 3 * AV_COUNT && x != 0.0 {
                health_damage = Some(x);
            }
        }
        if av_ok {
            p.av_parsed = true;
            p.av_modifiers = mods;
            p.av_base = base;
            p.av_damage = dmg;
            p.health_damage = health_damage;
            if let Ok((items, scale, factions, teammates, post_inv)) =
                self.player_inventory_and_extras(av_cur, r.flags)
            {
                p.inventory = Some(items);
                p.scale = scale;
                p.factions = factions;
                p.teammates = teammates;
                let tail = &r.data[post_inv..];
                p.active_weapon_magazine = Self::parse_player_active_magazine(tail);
                if r.has(22) {
                    p.base_av_overrides = Self::parse_player_base_av_overrides(tail);
                    p.skill_points = p
                        .base_av_overrides
                        .iter()
                        .copied()
                        .filter(|&(code, pts)| (32..=45).contains(&code) && pts >= 0.0)
                        .collect();
                }
                p.notes = self.parse_player_notes(tail);
                p.perks = self.parse_player_perks(tail);
                p.teammate_perks = self.parse_player_teammate_perks(tail);
            }
        }
        Ok(p)
    }

    /// Walk `NPC_ 0x7` (player base form) in change-flag order: bit 1 (24-byte base stats
    /// block), bit 4 (two spell/ability ref lists), bit 2 (7-byte SPECIAL), bit 3 (20-byte
    /// AI data), bit 9 (28-byte skills), bit 5 (name), bit 11 (FaceGen morphs + head parts),
    /// bit 24 (`u8 female |`, `Player.SexChange`) (§6.2, §6.4) [observed: vanilla 1.4.0.525
    /// A/B save pairs for bits 1, 2, 4, 5, 11, 24 and multi-flag order `1 -> 4 -> 2 -> 5 -> 24`;
    /// bits 3 and 9 remain [hypothesis]].
    fn parse_player_base(&self, npc: &ChangedForm, p: &mut FosPlayer) -> Result<(), FosError> {
        let mut c = Cur::new(&npc.data, 0);
        if npc.has(1) {
            let acbs = c.take(24)?;
            c.bar()?;
            let acbs_flags = u32::from_le_bytes(acbs[0..4].try_into().unwrap());
            p.female = Some(acbs_flags & 1 != 0);
            p.female_from_bit24 = false;
        }
        if npc.has(4) {
            let n1 = c.vsval()? as usize;
            c.bar()?;
            let mut spells = Vec::with_capacity(n1.min(256));
            for _ in 0..n1 {
                spells.push(self.reference(c.ref3()?).ok().flatten());
                c.bar()?;
            }
            let n2 = c.vsval()? as usize;
            c.bar()?;
            for _ in 0..n2 {
                c.ref3()?;
                c.bar()?;
            }
            p.spells = spells;
        }
        if npc.has(2) {
            let sp: [u8; 7] = c.take(7)?.try_into().unwrap();
            c.bar()?;
            p.special = Some(sp);
        }
        if npc.has(3) {
            c.take(20)?;
            c.bar()?;
        }
        if npc.has(9) {
            c.take(28)?;
            c.bar()?;
        }
        if npc.has(5) {
            let _name = c.string_field(true)?;
        }
        if npc.has(11) {
            if let Ok(app) = self.parse_player_appearance(&mut c) {
                p.appearance = Some(app);
            }
        }
        if npc.has(24) {
            let sex = c.u8()?;
            c.bar()?;
            p.female = Some(sex != 0);
            p.female_from_bit24 = true;
        }
        Ok(())
    }

    fn parse_player_appearance(&self, c: &mut Cur<'_>) -> Result<FosPlayerAppearance, FosError> {
        let _hdr = c.u8()?;
        c.bar()?;
        let mut fggs = Vec::with_capacity(50);
        for _ in 0..50 {
            fggs.push(c.f32()?);
            c.bar()?;
        }
        let mut fgga = Vec::with_capacity(30);
        for _ in 0..30 {
            fgga.push(c.f32()?);
            c.bar()?;
        }
        let mut fgts = Vec::with_capacity(50);
        for _ in 0..50 {
            fgts.push(c.f32()?);
            c.bar()?;
        }
        let hair = self.reference(c.ref3()?).ok().flatten();
        c.bar()?;
        let eyes = self.reference(c.ref3()?).ok().flatten();
        c.bar()?;
        let hair_length = c.f32()?;
        c.bar()?;
        let hair_color_u32 = c.u32()?;
        c.bar()?;
        let color_bytes = hair_color_u32.to_le_bytes();
        let hair_color = [color_bytes[0], color_bytes[1], color_bytes[2]];
        let n_parts = c.vsval()? as usize;
        c.bar()?;
        let mut head_parts = Vec::with_capacity(n_parts.min(64));
        for _ in 0..n_parts {
            head_parts.push(self.reference(c.ref3()?).ok().flatten());
            c.bar()?;
        }
        Ok(FosPlayerAppearance { fggs, fgga, fgts, hair, eyes, hair_length, hair_color, head_parts })
    }

    /// Decode a 1- or 2-byte `vsval(k) |` header right before `st` in `buf`.
    fn match_vsval_header(buf: &[u8], st: usize, k: usize) -> Option<usize> {
        if k < 64 {
            if st >= 2 && buf[st - 2] == ((k as u8) << 2) && buf[st - 1] == BAR {
                return Some(st - 2);
            }
        } else if k < 16384 {
            let v = ((k as u16) << 2) | 1;
            let le = v.to_le_bytes();
            if st >= 3 && buf[st - 3] == le[0] && buf[st - 2] == le[1] && buf[st - 1] == BAR {
                return Some(st - 3);
            }
        }
        None
    }

    /// Decode the player's active equipped weapon magazine round count from `ACHR 0x14`
    /// post-inventory High-Process block `tok[76]` (`u32 |`), gated on `tok[70] != u32::MAX`
    /// (equipped weapon state) (§6.4) [observed: vanilla 1.4.0.525 A/B save pairs].
    /// Every preceding field width and `|` separator is verified; any mismatch returns `None`.
    fn parse_player_active_magazine(tail: &[u8]) -> Option<u32> {
        const PROCESS_PREFIX_WIDTHS: [usize; 77] = [
            1, 1, 1, 1, 1, 1, 1, 1, 4, 4, 1, 1, 3, 3, 4, 4, 4, 3, 1, 4, 3, 4, 4, 4, 1, 2, 4, 4, 3,
            3, 3, 3, 3, 1, 4, 1, 1, 1, 4, 4, 4, 1, 12, 4, 1, 1, 1, 2, 12, 1, 1, 1, 1, 4, 1, 4, 4,
            1, 1, 1, 2, 4, 1, 4, 4, 1, 1, 4, 4, 4, 4, 1, 3, 3, 3, 3, 4,
        ];
        let mut c = Cur::new(tail, 0);
        let mut tok70 = u32::MAX;
        let mut tok76 = 0u32;
        for (idx, &w) in PROCESS_PREFIX_WIDTHS.iter().enumerate() {
            let bytes = c.take(w).ok()?;
            c.bar().ok()?;
            if idx == 70 {
                tok70 = u32::from_le_bytes(bytes.try_into().ok()?);
            } else if idx == 76 {
                tok76 = u32::from_le_bytes(bytes.try_into().ok()?);
            }
        }
        (tok70 != u32::MAX).then_some(tok76)
    }

    /// Extract the bit-22 base-AV override list (`vsval n | n * (u8 av_code | f32 val |)`)
    /// immediately preceding the `FF FF | FF FF | 00 | 01 01 00 00 |` boundary in the player
    /// record's post-inventory tail (§6.4) [observed: vanilla 1.4.0.525 A/B save pairs].
    /// Accepts all valid AV codes `< 77` (not only skills `32..=45`) so non-skill `SetAV`
    /// overrides (e.g. `Player.SetAV Luck 9`) do not discard the block.
    fn parse_player_base_av_overrides(tail: &[u8]) -> Vec<(u8, f32)> {
        const ANCHOR: &[u8] = b"\xFF\xFF\x7C\xFF\xFF\x7C\x00\x7C\x01\x01\x00\x00\x7C";
        let Some(anc) = tail.windows(ANCHOR.len()).position(|w| w == ANCHOR) else {
            return Vec::new();
        };
        for k in (1..=AV_COUNT).rev() {
            let Some(st) = anc.checked_sub(k * 7) else { continue };
            let Some(hdr_start) = Self::match_vsval_header(tail, st, k) else {
                continue;
            };
            if hdr_start > 0 && tail[hdr_start - 1] != BAR {
                continue;
            }
            let mut out = Vec::with_capacity(k);
            let mut seen = [false; AV_COUNT];
            let mut ok = true;
            for j in 0..k {
                let off = st + j * 7;
                if tail[off + 1] != BAR || tail[off + 6] != BAR {
                    ok = false;
                    break;
                }
                let code = tail[off];
                let val = f32::from_le_bytes(tail[off + 2..off + 6].try_into().unwrap());
                if usize::from(code) >= AV_COUNT || seen[usize::from(code)] || !val.is_finite() {
                    ok = false;
                    break;
                }
                seen[usize::from(code)] = true;
                out.push((code, val));
            }
            if ok {
                return out;
            }
        }
        Vec::new()
    }

    /// Extract acquired note form IDs (`vsval n | n * (ref3 |)`) from the player `ACHR 0x14`
    /// tail right after the 11 bar-delimited `ref3 |` slots and immediately preceding the
    /// known-actors / perks / 5-zero-`u32` section (§6.4) [observed: vanilla 1.4.0.525 A/B
    /// save pairs]. Fails closed if any structural separator or count does not match.
    fn parse_player_notes(&self, buf: &[u8]) -> Vec<u32> {
        const DEAD_MARK: &[u8] = b"\xAD\xDE\xAD\xDE\x7C";
        let Some(dead) = buf.windows(DEAD_MARK.len()).rposition(|w| w == DEAD_MARK) else {
            return Vec::new();
        };
        let start = dead + DEAD_MARK.len();
        if buf.len() < start + 28 + 44 + 35 {
            return Vec::new();
        }
        // Scan for the 11-slot `ref3 |` array preceded by `4*(u32 |) + u8 | + u32 |`
        // (27 bytes + leading `|`) and followed by `vsval(0) |`, `vsval(n_notes) | n_notes*(ref3 |)`,
        // `vsval(0) |`, known-actors `vsval(nm) | nm*(ref3 | u8 | u8 |)`, perks `vsval(kp) |
        // kp*(ref3 | u8 |)`, and the post-perk 4*(u8 |) (or 13-byte list) + 5*(u32 |) boundary.
        for i in (start + 28)..buf.len().saturating_sub(44) {
            if buf[i - 1] != BAR
                || buf[i - 6] != BAR
                || buf[i - 8] != BAR
                || buf[i - 13] != BAR
                || buf[i - 18] != BAR
                || buf[i - 23] != BAR
                || buf[i - 28] != BAR
            {
                continue;
            }
            if !(0..11).all(|j| buf[i + j * 4 + 3] == BAR) {
                continue;
            }
            let mut c = Cur::new(buf, i + 44);
            if c.vsval().ok() != Some(0) || c.bar().is_err() {
                continue;
            }
            let Ok(n_notes) = c.vsval() else { continue };
            if n_notes > 512 || c.bar().is_err() {
                continue;
            }
            let mut notes = Vec::with_capacity(n_notes as usize);
            let mut ok = true;
            for _ in 0..n_notes {
                let Ok(r3) = c.ref3() else {
                    ok = false;
                    break;
                };
                if c.bar().is_err() {
                    ok = false;
                    break;
                }
                match self.reference(r3) {
                    Ok(Some(fid)) => {
                        if !notes.contains(&fid) {
                            notes.push(fid);
                        }
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            if c.vsval().ok() != Some(0) || c.bar().is_err() {
                continue;
            }
            let Ok(nm) = c.vsval() else { continue };
            if nm > 1024 || c.bar().is_err() {
                continue;
            }
            for _ in 0..nm {
                if c.ref3().is_err()
                    || c.bar().is_err()
                    || c.u8().is_err()
                    || c.bar().is_err()
                    || c.u8().is_err()
                    || c.bar().is_err()
                {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            let Ok(kp) = c.vsval() else { continue };
            if kp > 512 || c.bar().is_err() {
                continue;
            }
            for _ in 0..kp {
                if c.ref3().is_err() || c.bar().is_err() || c.u8().is_err() || c.bar().is_err() {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            let mut tail_ok = false;
            {
                let mut t1 = c.clone();
                if (0..4).all(|_| t1.u8().is_ok() && t1.bar().is_ok())
                    && (0..5).all(|_| t1.u32().is_ok() && t1.bar().is_ok())
                {
                    tail_ok = true;
                }
            }
            if !tail_ok {
                let mut t2 = c.clone();
                if t2.u8().is_ok() && t2.bar().is_ok() {
                    if let Ok(mm) = t2.vsval() {
                        if mm <= 64
                            && t2.bar().is_ok()
                            && (0..mm).all(|_| {
                                t2.u32().is_ok()
                                    && t2.bar().is_ok()
                                    && t2.u32().is_ok()
                                    && t2.bar().is_ok()
                                    && t2.u16().is_ok()
                                    && t2.bar().is_ok()
                            })
                            && (0..2).all(|_| t2.u8().is_ok() && t2.bar().is_ok())
                            && (0..5).all(|_| t2.u32().is_ok() && t2.bar().is_ok())
                        {
                            tail_ok = true;
                        }
                    }
                }
            }
            if tail_ok {
                return notes;
            }
        }
        Vec::new()
    }

    /// Extract the player's perks (`vsval n | n * (ref3 | u8 rank |)`) from the post-inventory
    /// tail right before the post-perk 5-zero-`u32` boundary (§6.4) [hypothesis].
    fn parse_player_perks(&self, tail: &[u8]) -> Vec<(Option<u32>, u8)> {
        const DEAD_MARK: &[u8] = b"\xAD\xDE\xAD\xDE\x7C";
        const FIVE_ZEROS: &[u8] = &[
            0, 0, 0, 0, BAR, 0, 0, 0, 0, BAR, 0, 0, 0, 0, BAR, 0, 0, 0, 0, BAR, 0, 0, 0, 0, BAR,
        ];
        if tail.len() < 85 {
            return Vec::new();
        }
        let search_start = tail.windows(DEAD_MARK.len()).position(|w| w == DEAD_MARK).unwrap_or(0);
        let search_end = tail.len().saturating_sub(60);
        if search_start >= search_end {
            return Vec::new();
        }
        let mut cursor_end = search_end;
        while cursor_end >= search_start + FIVE_ZEROS.len() {
            let slice = &tail[search_start..cursor_end];
            let Some(rel) = slice.windows(FIVE_ZEROS.len()).rposition(|w| w == FIVE_ZEROS) else {
                break;
            };
            let z5 = search_start + rel;
            cursor_end = z5 + FIVE_ZEROS.len() - 1;
            let mut candidates = Vec::new();
            if z5 >= 8 && &tail[z5 - 8..z5] == b"\x00\x7C\x00\x7C\x00\x7C\x00\x7C" {
                candidates.push(z5 - 8);
            }
            if z5 >= 4 && &tail[z5 - 4..z5] == b"\x00\x7C\x00\x7C" {
                for m in 1..=16usize {
                    let Some(mst) = (z5 - 4).checked_sub(m * 13) else { break };
                    if mst >= 4 && tail[mst - 4..mst] == [0, BAR, (m as u8) << 2, BAR] {
                        let bars_ok = (0..m).all(|j| {
                            let e = mst + j * 13;
                            tail[e + 4] == BAR && tail[e + 9] == BAR && tail[e + 12] == BAR
                        });
                        if bars_ok {
                            candidates.push(mst - 4);
                        }
                    }
                }
            }
            for perk_end in candidates {
                for k in 1..=512usize {
                    let Some(st) = perk_end.checked_sub(k * 6) else { break };
                    let Some(hdr_start) = Self::match_vsval_header(tail, st, k) else { continue };
                    // Verify preceding bar-terminated `u8 | u8 |` boundary (when preceded by the prior list).
                    if hdr_start >= 5 && (tail[hdr_start - 1] != BAR || tail[hdr_start - 3] != BAR || tail[hdr_start - 5] != BAR) {
                        continue;
                    }
                    let mut out = Vec::with_capacity(k);
                    let mut ok = true;
                    for j in 0..k {
                        let off = st + j * 6;
                        if tail[off + 3] != BAR || tail[off + 5] != BAR {
                            ok = false;
                            break;
                        }
                        let rf = self.reference([tail[off], tail[off + 1], tail[off + 2]]);
                        let rank = tail[off + 4];
                        let Ok(Some(fid)) = rf else {
                            ok = false;
                            break;
                        };
                        if rank == 0 || rank > 20 {
                            ok = false;
                            break;
                        }
                        out.push((Some(fid), rank));
                    }
                    if ok {
                        return out;
                    }
                }
            }
        }
        Vec::new()
    }

    /// Extract the teammate-perks list (`vsval n | n * (ref3 | u8 rank |)`) immediately
    /// preceding the 54-byte end trailer (`00 | 01 | 01 | 8*(u32 |) | 4*(u8 |)`) of the
    /// player record (§6.4) [hypothesis].
    fn parse_player_teammate_perks(&self, tail: &[u8]) -> Vec<(Option<u32>, u8)> {
        if tail.len() < 56 {
            return Vec::new();
        }
        let end54 = tail.len() - 54;
        if &tail[end54..end54 + 6] != b"\x00\x7C\x01\x7C\x01\x7C" {
            return Vec::new();
        }
        // Verify all 8 u32 | + 4 u8 | bar markers in the trailer.
        for j in 0..8 {
            if tail[end54 + 6 + j * 5 + 4] != BAR {
                return Vec::new();
            }
        }
        for j in 0..4 {
            if tail[end54 + 46 + j * 2 + 1] != BAR {
                return Vec::new();
            }
        }
        for k in 1..=64usize {
            let Some(st) = end54.checked_sub(k * 6) else { break };
            let Some(hdr_start) = Self::match_vsval_header(tail, st, k) else { continue };
            // Preceded by 3 u32 | entries (15 bytes) when present.
            if hdr_start >= 15
                && (tail[hdr_start - 1] != BAR || tail[hdr_start - 6] != BAR || tail[hdr_start - 11] != BAR)
            {
                continue;
            }
            let mut out = Vec::with_capacity(k);
            let mut ok = true;
            for j in 0..k {
                let off = st + j * 6;
                if tail[off + 3] != BAR || tail[off + 5] != BAR {
                    ok = false;
                    break;
                }
                let Ok(Some(fid)) = self.reference([tail[off], tail[off + 1], tail[off + 2]]) else {
                    ok = false;
                    break;
                };
                let rank = tail[off + 4];
                if rank == 0 || rank > 20 {
                    ok = false;
                    break;
                }
                out.push((Some(fid), rank));
            }
            if ok {
                return out;
            }
        }
        Vec::new()
    }

    /// The player's inventory list plus bit-4-gated post-AV scale, `0x5E` faction ranks,
    /// `0x1D` teammates, and post-inventory byte offset (save-files.md §6.4). Every separator
    /// and extra-data type byte is verified; an unknown actor or item extra-data type
    /// fails soft so no inventory is guessed out of misaligned bytes.
    #[allow(clippy::type_complexity)]
    fn player_inventory_and_extras(
        &self,
        mut c: Cur<'_>,
        flags: u32,
    ) -> Result<(Vec<FosItem>, Option<f32>, Vec<(Option<u32>, i8)>, Vec<Option<u32>>, usize), FosError> {
        const EXTRA_BITS: u32 = (1 << 11) | (1 << 17) | (1 << 18) | (1 << 29) | (1 << 31);
        const INV_BITS: u32 = (1 << 5) | (1 << 27);
        c.expect_u8(0)?;
        c.bar()?;
        let scale = if (flags & (1 << 4)) != 0 {
            let s = c.f32()?;
            if !s.is_finite() {
                return err("non-finite float after actor values");
            }
            c.bar()?;
            Some(s)
        } else {
            None
        };
        if (flags & (EXTRA_BITS | INV_BITS)) == 0 {
            return Ok((Vec::new(), scale, Vec::new(), Vec::new(), c.p));
        }
        let actor_extras = c.vsval()?;
        c.bar()?;
        let mut factions = Vec::new();
        let mut teammates = Vec::new();
        for _ in 0..actor_extras {
            let ty = c.u8()?;
            c.bar()?;
            match ty {
                // Faction changes: vsval | n * (ref3 | u8 |)
                0x5E => {
                    let n = c.vsval()?;
                    c.bar()?;
                    for _ in 0..n {
                        let fac = self.reference(c.ref3()?).ok().flatten();
                        c.bar()?;
                        let rank = c.u8()? as i8;
                        c.bar()?;
                        factions.push((fac, rank));
                    }
                }
                // Observed in early saves: ref3 | 12 bytes | f32 |
                0x18 => {
                    c.ref3()?;
                    c.bar()?;
                    c.take(12)?;
                    c.bar()?;
                    c.f32()?;
                    c.bar()?;
                }
                // Observed with change flag 29: ref3 |
                0x74 => {
                    c.ref3()?;
                    c.bar()?;
                }
                // Active teammates/followers ref list: vsval | n * (ref3 |)
                0x1D => {
                    let n = c.vsval()?;
                    c.bar()?;
                    for _ in 0..n {
                        teammates.push(self.reference(c.ref3()?).ok().flatten());
                        c.bar()?;
                    }
                }
                // Single u32: u32 |
                0x60 => {
                    c.u32()?;
                    c.bar()?;
                }
                // Pair of refs + byte: ref3 | ref3 | u8 |
                0x75 => {
                    c.ref3()?;
                    c.bar()?;
                    c.ref3()?;
                    c.bar()?;
                    c.u8()?;
                    c.bar()?;
                }
                t => return err(format!("actor extra data type {t:#04x} of unknown size")),
            }
        }
        let items = if (flags & INV_BITS) != 0 {
            self.parse_item_list(&mut c)?
        } else {
            Vec::new()
        };
        Ok((items, scale, factions, teammates, c.p))
    }

    /// Shared bar-delimited inventory item-list parser used by the player record (§6.4),
    /// non-player actors, and container `REFR` records (§6.2).
    fn parse_item_list(&self, c: &mut Cur<'_>) -> Result<Vec<FosItem>, FosError> {
        let n = c.vsval()?;
        c.bar()?;
        let mut items = Vec::new();
        for _ in 0..n {
            let item = self.reference(c.ref3()?).ok().flatten();
            c.bar()?;
            let count = c.i32()?;
            c.bar()?;
            let lists = c.vsval()?;
            c.bar()?;
            let mut worn = false;
            let mut condition = None;
            let mut owner = None;
            let mut weapon_mods = 0u8;
            let mut hotkey = None;
            let mut ammo = None;
            let mut entries = Vec::with_capacity(lists as usize);
            for _ in 0..lists {
                let extras = c.vsval()?;
                c.bar()?;
                let mut entry = FosItemEntry::default();
                for _ in 0..extras {
                    match c.u8()? {
                        // Item script instance: ref3 | vsval(nv) | nv * (u32 idx | ref3/f64 |) | vsval(0) | vsval(0) |
                        0x0D => {
                            c.bar()?;
                            let _ = self.parse_script_extra(c)?;
                        }
                        // Worn item marker (0 payload bytes) [observed] / defensive secondary worn
                        // pass-through (0x17) [design; not emitted by vanilla 1.4.0.525]
                        0x16 => {
                            c.bar()?;
                            entry.worn = true;
                        }
                        0x17 => {
                            c.bar()?;
                            entry.worn = true;
                            entry.worn_secondary = true;
                        }
                        // Original reference (0x1C: ref3 |) / ownership global (0x22: ref3 |)
                        // [observed: vanilla 1.4.0.525 + clean-room test-plugin A/B save pairs]
                        0x1C | 0x22 => {
                            c.bar()?;
                            c.ref3()?;
                            c.bar()?;
                        }
                        // Owner (ref3 |) [observed: vanilla 1.4.0.525 A/B save pairs]
                        0x21 => {
                            c.bar()?;
                            entry.owner = Some(self.reference(c.ref3()?).ok().flatten());
                            c.bar()?;
                        }
                        // Owner rank (i32 |) [observed: vanilla 1.4.0.525 A/B save pairs]
                        0x23 => {
                            c.bar()?;
                            c.i32()?;
                            c.bar()?;
                        }
                        // Stack count (i16 |) [observed: vanilla 1.4.0.525 A/B save pairs]
                        0x24 => {
                            c.bar()?;
                            entry.count = Some(c.i16()?);
                            c.bar()?;
                        }
                        // Condition HP (f32 HP |) [observed: vanilla 1.4.0.525 A/B save pairs]
                        0x25 => {
                            c.bar()?;
                            entry.condition = Some(c.f32()?);
                            c.bar()?;
                        }
                        // Item scale (0x30: f32 |) [observed: vanilla 1.4.0.525 + test-plugin A/B]
                        // and defensive pass-throughs 0x26 (time left) / 0x28 (charge)
                        // [design; not emitted by vanilla 1.4.0.525]
                        0x26 | 0x28 | 0x30 => {
                            c.bar()?;
                            c.f32()?;
                            c.bar()?;
                        }
                        // Container-extra 0x2F (u32 | u8 |) [observed: vanilla 1.4.0.525 A/B save pairs]
                        0x2F => {
                            c.bar()?;
                            c.u32()?;
                            c.bar()?;
                            c.u8()?;
                            c.bar()?;
                        }
                        // No-unequip item flag (0 payload bytes; EquipItem <item> 1)
                        // [observed: vanilla 1.4.0.525 A/B save pairs]
                        0x3E => {
                            c.bar()?;
                        }
                        // Hotkey (u8 |) [observed: vanilla 1.4.0.525 A/B save pairs]
                        0x4A => {
                            c.bar()?;
                            entry.hotkey = Some(c.u8()?);
                            c.bar()?;
                        }
                        // Magazine ammo (ref3 | u32 |) [observed framing on NPC-dropped WEAP REFRs]
                        0x6E => {
                            c.bar()?;
                            let ammo_form = self.reference(c.ref3()?).ok().flatten();
                            c.bar()?;
                            let rounds = c.u32()?;
                            c.bar()?;
                            entry.ammo = Some((ammo_form, rounds));
                        }
                        // Weapon mods (u8 |)
                        0x8D => {
                            c.bar()?;
                            entry.weapon_mods = c.u8()? & 0x07;
                            c.bar()?;
                        }
                        t => return err(format!("extra data type {t:#04x} of unknown size")),
                    }
                }
                worn |= entry.worn;
                if entry.condition.is_some() {
                    condition = entry.condition;
                }
                if entry.owner.is_some() {
                    owner = entry.owner;
                }
                if entry.weapon_mods != 0 {
                    weapon_mods = entry.weapon_mods;
                }
                if entry.hotkey.is_some() {
                    hotkey = entry.hotkey;
                }
                if entry.ammo.is_some() {
                    ammo = entry.ammo;
                }
                entries.push(entry);
            }
            items.push(FosItem { item, count, worn, condition, owner, weapon_mods, hotkey, ammo, entries });
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes built the way the reader expects, for a save with one plugin.
    struct Build {
        b: Vec<u8>,
    }

    impl Build {
        fn bar(&mut self) -> &mut Self {
            self.b.push(BAR);
            self
        }
        fn u8(&mut self, v: u8) -> &mut Self {
            self.b.push(v);
            self
        }
        fn u16(&mut self, v: u16) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn i16(&mut self, v: i16) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn u32(&mut self, v: u32) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn f32(&mut self, v: f32) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn f64(&mut self, v: f64) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn s(&mut self, v: &str) -> &mut Self {
            self.b.extend((v.len() as u16).to_le_bytes());
            self.bar();
            self.b.extend(v.as_bytes());
            self
        }
        /// A kind-0 reference to the form ID array's 1-based `i`.
        fn r(&mut self, i: u32) -> &mut Self {
            self.b.extend(&i.to_be_bytes()[1..]);
            self
        }
        fn vs(&mut self, n: u32) -> &mut Self {
            assert!(n < 64);
            self.u8((n << 2) as u8)
        }
    }

    fn record(out: &mut Vec<u8>, r: u32, flags: u32, kind: u8, data: &[u8]) {
        out.extend(&r.to_be_bytes()[1..]);
        out.extend(flags.to_le_bytes());
        out.push(kind | 0x40);
        out.push(27);
        out.extend((data.len() as u16).to_le_bytes());
        out.extend(data);
    }

    const PLAYER_FLAGS: u32 = 1 << 1 | 1 << 4 | 1 << 5;

    /// The player record's data: the place, the actor values, the unknown part (with
    /// `marker` where 0x5E stands), the flag-29 block when `bit29`, and two items, the
    /// second with a worn extra and one of type `extra` (0x25: an f32).
    fn player_data(bit29: bool, marker: u8, extra: u8) -> Vec<u8> {
        let mut p = Build { b: Vec::new() };
        p.r(3);
        for v in [37000.0f32, -117159.0, 6057.0, 0.0, 0.0, 1.309] {
            p.f32(v);
        }
        p.bar();
        for i in 0..AV_ENTRIES {
            p.f32(if i == 38 { 2.0 } else { 0.0 }).bar();
        }
        p.u8(0).bar().f32(1.0).bar().vs(if bit29 { 3 } else { 2 }).bar().u8(marker).bar().vs(1).bar().r(4).bar().u8(1).bar();
        p.u8(0x18).bar().r(2).bar().f32(1.0).f32(2.0).f32(3.0).bar().f32(3.5).bar();
        if bit29 {
            p.u8(0x74).bar().r(8).bar();
        }
        p.vs(2).bar();
        p.r(4).bar().u32(9).bar().vs(0).bar();
        p.r(4).bar().u32(1).bar().vs(1).bar().vs(2).bar().u8(0x16).bar().u8(extra).bar().f32(41.25).bar();
        p.b
    }

    /// A save: form IDs [quest 0x1234, global 0x5678, world 0xDA726, item 0x15169,
    /// player 0x14, player base 7, class 0x57E6A, ref var target 0x4242].
    fn save() -> Vec<u8> {
        let fids = [0x1234u32, 0x5678, 0xDA726, 0x15169, 0x14, 0x7, 0x57E6A, 0x4242];
        let mut h = Build { b: Vec::new() };
        h.u32(0x30).bar();
        h.b.extend([0u8; 64]);
        h.bar().u32(2).bar().u32(1).bar().u32(15).bar().s("Courier").bar().s("Drifter").bar().u32(3).bar().s("Goodsprings").bar().s("000.11.50");
        let mut b = MAGIC.to_vec();
        b.extend((h.b.len() as u32).to_le_bytes());
        b.extend(&h.b);
        b.extend([0u8; 6]); // 2 x 1 screenshot
        let mut pl = Build { b: Vec::new() };
        pl.u8(1).bar().s("FalloutNV.esm").bar();
        b.push(27);
        b.extend((pl.b.len() as u32).to_le_bytes());
        b.extend(&pl.b);
        let table_at = b.len();
        b.extend([0u8; 32 + 78]);
        // Global table 1: type 1 (place) and type 3 (globals).
        let g1 = b.len();
        let mut t1 = Build { b: Vec::new() };
        t1.u32(0xFF00_1000).bar().r(3).bar().u32(9).bar().u32(-29i32 as u32).bar().r(3).bar().f32(1.0).f32(2.0).f32(3.0).bar().r(1).bar();
        b.extend(1u32.to_le_bytes());
        b.extend((t1.b.len() as u32).to_le_bytes());
        b.extend(&t1.b);
        let mut t3 = Build { b: Vec::new() };
        t3.vs(1).bar().r(2).bar().f32(12.5).bar();
        b.extend(3u32.to_le_bytes());
        b.extend((t3.b.len() as u32).to_le_bytes());
        b.extend(&t3.b);
        // Changed forms.
        let cf = b.len();
        let mut q = Build { b: Vec::new() };
        q.u8(0x31).bar();
        q.vs(1).bar().u8(10).bar().u8(1).bar().vs(1).bar().u8(0).bar().u8(1).bar().u32(0x08E9_0123).bar();
        q.vs(2).bar().u32(3).bar().f64(1.5).bar().u32(0x8000_0004).bar().r(8).bar().vs(0).bar().vs(0).bar();
        q.vs(1).bar().u32(10).bar().u32(3).bar();
        record(&mut b, 1, 1 << 1 | 1 << 31 | 1 << 30 | 1 << 29, TYPE_QUST, &q.b);
        let mut npc = Build { b: Vec::new() };
        npc.b.extend([5, 6, 5, 5, 5, 5, 10]);
        npc.bar();
        record(&mut b, 6, 1 << 2, TYPE_NPC, &npc.b);
        let mut clas = Build { b: Vec::new() };
        clas.u32(38).bar().u32(43).bar().u32(44).bar().u32(u32::MAX).bar();
        record(&mut b, 7, 1 << 1, TYPE_CLAS, &clas.b);
        record(&mut b, 5, PLAYER_FLAGS, TYPE_ACHR, &player_data(false, 0x5E, 0x25));
        let g2 = b.len();
        b.extend(1000u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        let fid = b.len();
        b.extend((fids.len() as u32).to_le_bytes());
        for f in fids {
            b.extend(f.to_le_bytes());
        }
        b.extend(1u32.to_le_bytes());
        b.extend(0xDA726u32.to_le_bytes());
        let tail = b.len();
        b.extend([5, 0, 0, 0, 0, 0, 0, 0, 0x7C]);
        let table = [fid, tail, g1, cf, g2, 2, 1, 4].map(|v| v as u32);
        for (i, v) in table.iter().enumerate() {
            b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn reads_header_plugins_and_globals() {
        let f = Fos::parse(&save()).unwrap();
        assert_eq!(f.header.player_name, "Courier");
        assert_eq!(f.header.level, 3);
        assert_eq!(f.header.location, "Goodsprings");
        assert_eq!(f.plugins, ["FalloutNV.esm"]);
        assert_eq!(f.worlds, [0xDA726]);
        assert_eq!(f.changed.len(), 4);
        assert_eq!(f.global_variables().unwrap(), [(0x5678, 12.5)]);
        assert_eq!(f.active_quest(), Some(0x1234));
    }

    #[test]
    fn reads_quest_stages_variables_and_objectives() {
        let f = Fos::parse(&save()).unwrap();
        let q = f.quests().remove(0).unwrap();
        assert_eq!(q.form, 0x1234);
        assert_eq!((q.running(), q.completed()), (Some(true), Some(false)));
        assert_eq!(q.stages, [(10, true)]);
        assert_eq!(q.vars, [(3, VarValue::Num(1.5)), (4, VarValue::Ref(Some(0x4242)))]);
        assert_eq!(q.objectives, [(10, 3)]);
    }

    #[test]
    fn reads_the_player() {
        let f = Fos::parse(&save()).unwrap();
        let p = f.player().unwrap();
        let (at, pos, rot) = p.place.unwrap();
        assert_eq!((at, pos[0], rot[2]), (0xDA726, 37000.0, 1.309));
        assert_eq!(p.special, Some([5, 6, 5, 5, 5, 5, 10]));
        assert_eq!(p.tag_skills, [38, 43, 44]);
        assert_eq!(p.av_modifiers, [(38, 2.0)]);
        assert_eq!(
            p.inventory.unwrap(),
            [
                FosItem {
                    item: Some(0x15169),
                    count: 9,
                    worn: false,
                    condition: None,
                    owner: None,
                    weapon_mods: 0,
                    hotkey: None,
                    ammo: None,
                    entries: Vec::new(),
                },
                FosItem {
                    item: Some(0x15169),
                    count: 1,
                    worn: true,
                    condition: Some(41.25),
                    owner: None,
                    weapon_mods: 0,
                    hotkey: None,
                    ammo: None,
                    entries: vec![FosItemEntry { worn: true, condition: Some(41.25), ..FosItemEntry::default() }],
                },
            ]
        );
    }

    /// The player record with other data in place of the test save's.
    fn with_player(data: Vec<u8>, flags: u32) -> FosPlayer {
        let mut f = Fos::parse(&save()).unwrap();
        let r = f.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
        (r.data, r.flags) = (data, flags);
        f.player().unwrap()
    }

    #[test]
    fn the_inventory_after_the_flag_29_block_and_none_for_another_shape() {
        // Flag 29: `74 |` and a ref inside the actor extra-data list before the inventory.
        let p = with_player(player_data(true, 0x5E, 0x25), PLAYER_FLAGS | 1 << 29);
        assert_eq!(p.inventory.map(|i| i.len()), Some(2));
        // An unknown actor extra-data type: no inventory, not items read out of other data.
        assert_eq!(with_player(player_data(false, 0x5F, 0x25), PLAYER_FLAGS).inventory, None);
        // A mismatched actor extra count claiming a third entry when only two follow: none.
        let mut mismatch = player_data(false, 0x5E, 0x25);
        let count_pos = 3 + 24 + 1 + AV_ENTRIES * 5 + 2 + 5;
        assert_eq!(mismatch[count_pos], 2 << 2);
        mismatch[count_pos] = 3 << 2;
        assert_eq!(with_player(mismatch, PLAYER_FLAGS).inventory, None);
        // An item extra-data type of unknown size: none.
        let p = with_player(player_data(false, 0x5E, 0xFE), PLAYER_FLAGS);
        assert_eq!(p.inventory, None);
        assert_eq!(p.tag_skills, [38, 43, 44], "the rest is read");
    }

    #[test]
    fn reads_midgame_actor_extras_all_nine_item_extras_and_dead_counts() {
        let mut p = Build { b: Vec::new() };
        p.r(3);
        for v in [10.0f32, 20.0, 30.0, 0.0, 0.0, 0.5] {
            p.f32(v);
        }
        p.bar();
        for _ in 0..AV_ENTRIES {
            p.f32(0.0).bar();
        }
        // Finite non-1.0 float (1.1) and 5-entry actor extra-data list: 0x5E, 0x74, 0x1D, 0x60, 0x75 (no 0x18).
        p.u8(0).bar().f32(1.1).bar().vs(5).bar();
        p.u8(0x5E).bar().vs(1).bar().r(4).bar().u8(2).bar();
        p.u8(0x74).bar().r(8).bar();
        p.u8(0x1D).bar().vs(2).bar().r(1).bar().r(2).bar();
        p.u8(0x60).bar().u32(0x1234_5678).bar();
        p.u8(0x75).bar().r(1).bar().r(8).bar().u8(1).bar();

        // 2 inventory items covering all 9 observed item extra-data types.
        p.vs(2).bar();
        // Item 1: count 3, 2 extra-data lists:
        //   list 1: 0x16 (worn), 0x25 (cond 75.5), 0x6E (ammo ref r(2), 12 rounds), 0x8D (mods 0x05), 0x4A (hotkey 3), 0x0D (script with 1 num + 1 ref var)
        //   list 2: 0x24 (count 2), 0x21 (owner r(8)), 0x1C (orig ref r(1))
        p.r(4).bar().u32(3).bar().vs(2).bar();
        p.vs(6).bar();
        p.u8(0x16).bar();
        p.u8(0x25).bar().f32(75.5).bar();
        p.u8(0x6E).bar().r(2).bar().u32(12).bar();
        p.u8(0x8D).bar().u8(0x85).bar(); // masked with 0x07 -> 5
        p.u8(0x4A).bar().u8(3).bar();
        p.u8(0x0D).bar().r(1).bar().vs(2).bar().u32(1).bar().f64(9.5).bar().u32(0x8000_0002).bar().r(8).bar().vs(0).bar().vs(0).bar();
        p.vs(3).bar();
        p.u8(0x24).bar().i16(2).bar();
        p.u8(0x21).bar().r(8).bar();
        p.u8(0x1C).bar().r(1).bar();
        // Item 2: count 0 (base item), worn only.
        p.r(4).bar().u32(0).bar().vs(1).bar().vs(1).bar().u8(0x16).bar();

        let pl = with_player(p.b, PLAYER_FLAGS | 1 << 29);
        let inv = pl.inventory.expect("inventory parsed");
        assert_eq!(inv.len(), 2);
        assert_eq!(inv[0].item, Some(0x15169));
        assert_eq!(inv[0].count, 3);
        assert!(inv[0].worn);
        assert_eq!(inv[0].condition, Some(75.5));
        assert_eq!(inv[0].owner, Some(Some(0x4242)));
        assert_eq!(inv[0].weapon_mods, 5);
        assert_eq!(inv[0].hotkey, Some(3));
        assert_eq!(inv[0].ammo, Some((Some(0x5678), 12)));
        assert_eq!(
            inv[0].entries,
            [
                FosItemEntry {
                    count: None,
                    worn: true,
                    worn_secondary: false,
                    condition: Some(75.5),
                    owner: None,
                    weapon_mods: 5,
                    hotkey: Some(3),
                    ammo: Some((Some(0x5678), 12)),
                },
                FosItemEntry {
                    count: Some(2),
                    worn: false,
                    worn_secondary: false,
                    condition: None,
                    owner: Some(Some(0x4242)),
                    weapon_mods: 0,
                    hotkey: None,
                    ammo: None,
                },
            ]
        );
        assert_eq!(inv[1].count, 0);
        assert!(inv[1].worn);

        // Non-finite float after AV block must fail soft.
        let mut bad_f = player_data(false, 0x5E, 0x25);
        let f_pos = 3 + 24 + 1 + AV_ENTRIES * 5 + 2;
        bad_f[f_pos..f_pos + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(with_player(bad_f, PLAYER_FLAGS).inventory, None);

        // Global table 1 type 2: dead_counts prefix with n = 0 and n = 2 + unmapped trailing bytes.
        let mut f = Fos::parse(&save()).unwrap();
        let mut t2_empty = Build { b: Vec::new() };
        t2_empty.vs(0).bar();
        t2_empty.b.extend([0u8; 105]);
        f.globals_data.push((2, t2_empty.b));
        assert_eq!(f.dead_counts_with_tail().unwrap(), (Vec::new(), 105));
        assert_eq!(f.dead_counts().unwrap(), Vec::new());

        let mut t2 = Build { b: Vec::new() };
        t2.vs(2).bar().r(6).bar().u16(4).bar().r(8).bar().u16(17).bar();
        t2.b.extend([0xAAu8; 105]);
        f.globals_data.retain(|(ty, _)| *ty != 2);
        f.globals_data.push((2, t2.b));
        assert_eq!(f.dead_counts_with_tail().unwrap(), (vec![(Some(0x7), 4), (Some(0x4242), 17)], 105));
        assert_eq!(f.dead_counts().unwrap(), vec![(Some(0x7), 4), (Some(0x4242), 17)]);
    }

    #[test]
    fn an_odd_quest_leaves_the_others_and_a_reference_past_the_array_is_unresolved() {
        let mut f = Fos::parse(&save()).unwrap();
        let good = f.changed.iter().find(|c| c.kind == TYPE_QUST).unwrap().clone();
        // Bytes left over after the flags byte.
        f.changed.push(ChangedForm { form: 0x9999, flags: 1 << 1, kind: TYPE_QUST, data: vec![1, BAR, 0] });
        // A reference variable naming form 99 of an 8-entry array.
        let mut v = Build { b: Vec::new() };
        v.vs(1).bar().u32(0x8000_0002).bar().r(99).bar().vs(0).bar().vs(0).bar();
        f.changed.push(ChangedForm { form: 0x8888, flags: 1 << 30, kind: TYPE_QUST, data: v.b });
        let qs = f.quests();
        assert_eq!(qs.len(), 3);
        assert_eq!(qs[0].as_ref().unwrap().form, good.form);
        assert!(qs[1].is_err());
        assert_eq!(qs[2].as_ref().unwrap().vars, [(2, VarValue::Unresolved)]);
    }

    #[test]
    fn tag_skills_only_from_the_player_class() {
        let mut f = Fos::parse(&save()).unwrap();
        f.changed.iter_mut().find(|c| c.kind == TYPE_CLAS).unwrap().form = 0x1234_5678;
        assert_eq!(f.player().unwrap().tag_skills, Vec::<u32>::new());
    }

    #[test]
    fn sizes_past_any_file_are_errors_not_overflows() {
        // The screenshot's width and height (after the u32, the language and their bars).
        let mut b = save();
        let at = MAGIC.len() + 4 + 5 + 65;
        b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        b[at + 5..at + 9].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Fos::parse(&b).is_err());
        // The header size.
        let mut b = save();
        b[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Fos::parse(&b).is_err());
    }

    #[test]
    fn refuses_other_files_and_short_ones() {
        assert!(Fos::parse(b"NVFSAVE\0").is_err());
        let b = save();
        assert!(Fos::parse(&b[..b.len() / 2]).is_err());
    }

    #[test]
    fn parses_empty_player_name_screenshot_play_time_and_extended_sections() {
        let fids = [0x1234u32, 0x5678, 0xDA726, 0x15169, 0x14, 0x7, 0x57E6A, 0x4242, 0x9100, 0x9200, 0x9300, 0x9400, 0x9500, 0x9600];
        let mut h = Build { b: Vec::new() };
        h.u32(0x30).bar();
        h.b.extend([0u8; 64]);
        // Empty player_name has only ONE '|' after its u16(0) length (no double bar).
        h.bar()
            .u32(2)
            .bar()
            .u32(1)
            .bar()
            .u32(1)
            .bar()
            .s("")
            .s("Dweller")
            .bar()
            .u32(5)
            .bar()
            .s("Synthetic Cell")
            .bar()
            .s("001.02.03");
        let mut b = MAGIC.to_vec();
        b.extend((h.b.len() as u32).to_le_bytes());
        b.extend(&h.b);
        b.extend([255, 0, 0, 0, 255, 0]); // 2x1 RGB screenshot
        let mut pl = Build { b: Vec::new() };
        pl.u8(1).bar().s("FalloutNV.esm").bar();
        b.push(27);
        b.extend((pl.b.len() as u32).to_le_bytes());
        b.extend(&pl.b);
        let table_at = b.len();
        b.extend([0u8; 32 + 78]);
        let g1 = b.len();
        // Type 0: misc stats (3 values).
        let mut t0 = Build { b: Vec::new() };
        t0.u32(3).bar().u32(4).bar().u32(0).bar().u32(7).bar();
        b.extend(0u32.to_le_bytes());
        b.extend((t0.b.len() as u32).to_le_bytes());
        b.extend(&t0.b);
        // Type 8: weather (current=fids[8], second=none).
        let mut t8 = Build { b: Vec::new() };
        t8.r(9).bar().r(0).bar();
        b.extend(8u32.to_le_bytes());
        b.extend((t8.b.len() as u32).to_le_bytes());
        b.extend(&t8.b);

        let cf = b.len();
        // INFO said-once (fids[9])
        record(&mut b, 10, 1 << 31, TYPE_INFO, &[]);
        // NOTE owned (fids[10])
        record(&mut b, 11, 1 << 31, TYPE_NOTE, &[]);
        // FLST (fids[11]): u32 count=2 | r(4) | r(8) |
        let mut fl = Build { b: Vec::new() };
        fl.u32(2).bar().r(4).bar().r(8).bar();
        record(&mut b, 12, 1 << 31, TYPE_FLST, &fl.b);
        // LVLI (fids[12]): vs(1) | r(4) | u16(2) | u16(3) | f32(0.75) |
        let mut lv = Build { b: Vec::new() };
        lv.vs(1).bar().r(4).bar();
        lv.b.extend(2u16.to_le_bytes());
        lv.bar();
        lv.b.extend(3u16.to_le_bytes());
        lv.bar().f32(0.75).bar();
        record(&mut b, 13, 1 << 31, TYPE_LVLI, &lv.b);
        // REPU (fids[13]): f32 fame | f32 infamy |
        let mut rp = Build { b: Vec::new() };
        rp.f32(12.5).bar().f32(2.0).bar();
        record(&mut b, 14, 1 << 1, TYPE_REPU, &rp.b);
        // CHAL (fids[0]): u32 progress | u32 flags |
        let mut ch = Build { b: Vec::new() };
        ch.u32(9).bar().u32(4).bar();
        record(&mut b, 1, 1 << 1, TYPE_CHAL, &ch.b);
        // Player with unmapped bit-2 block before AVs: place still decodes fail-soft.
        let mut bad_p = Build { b: Vec::new() };
        bad_p.r(3).f32(10.0).f32(20.0).f32(30.0).f32(0.0).f32(0.0).f32(1.0).bar();
        bad_p.b.extend([0x11, 0x22, 0x33, 0x44]); // not bar-aligned AV array
        record(&mut b, 5, PLAYER_FLAGS | (1 << 2), TYPE_ACHR, &bad_p.b);

        let g2 = b.len();
        b.extend(1000u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        let fid = b.len();
        b.extend((fids.len() as u32).to_le_bytes());
        for f in fids {
            b.extend(f.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes());
        let tail = b.len();
        b.extend([5, 0, 0, 0, 0, 0, 0, 0, 0x7C]);
        let table = [fid, tail, g1, cf, g2, 2, 1, 7].map(|v| v as u32);
        for (i, v) in table.iter().enumerate() {
            b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }

        let fos = Fos::parse(&b).unwrap();
        assert_eq!(fos.header.player_name, "");
        assert_eq!(fos.header.karma_title, "Dweller");
        assert_eq!(fos.header.level, 5);
        assert_eq!(fos.header.play_seconds(), Some(3723.0));
        assert_eq!((fos.header.screenshot_width, fos.header.screenshot_height), (2, 1));
        assert_eq!(fos.screenshot_rgb, [255, 0, 0, 0, 255, 0]);
        assert_eq!(fos.misc_stats().unwrap(), [4, 0, 7]);
        assert_eq!(fos.weather().unwrap(), Some(FosWeather { current: Some(0x9100), second: None }));
        assert_eq!(fos.said_infos(), [0x9200]);
        assert_eq!(fos.notes(), [0x9300]);
        assert_eq!(fos.form_lists(), [Ok(FosFormList { form: 0x9400, forms: vec![Some(0x15169), Some(0x4242)] })]);
        assert_eq!(
            fos.leveled_lists(),
            [Ok(FosLeveledList {
                form: 0x9500,
                entries: vec![FosLeveledEntry { item: Some(0x15169), level: 2, count: 3, condition: 0.75 }],
            })]
        );
        assert_eq!(fos.reputations(), [Ok(FosReputation { form: 0x9600, fame: 12.5, infamy: 2.0 })]);
        assert_eq!(fos.challenges(), [Ok(FosChallenge { form: 0x1234, progress: 9, flags: 4 })]);
        let p = fos.player().unwrap();
        assert!(p.place.is_some());
        assert!(!p.av_parsed);
        assert!(p.inventory.is_none());

        // Regression: overflowing HHH.MM.SS strings must return None without panicking.
        let huge_h = FosHeader { play_time: format!("{}.00.00", u64::MAX), ..Default::default() };
        assert_eq!(huge_h.play_seconds(), None);
        let huge_m = FosHeader { play_time: format!("0.{}.00", u64::MAX), ..Default::default() };
        assert_eq!(huge_m.play_seconds(), None);
        let huge_sum = FosHeader { play_time: format!("{}.00.1", u64::MAX / 3600), ..Default::default() };
        assert!(huge_sum.play_seconds().is_some());
    }

    #[test]
    fn reads_container_inventories_map_markers_and_created_refs() {
        let mut f = Fos::parse(&save()).unwrap();

        // 1. Container REFR (bits 5+27): 2 items — one removed (-1), one added (+2) with 0x2F, 0x24, 0x25, 0x8D.
        let mut cont = Build { b: Vec::new() };
        cont.vs(2).bar();
        cont.r(4).bar().u32((-1i32) as u32).bar().vs(0).bar();
        cont.r(4).bar().u32(2).bar().vs(1).bar().vs(4).bar();
        cont.u8(0x2F).bar().u32(0x1122_3344).bar().u8(1).bar();
        cont.u8(0x24).bar().i16(2).bar();
        cont.u8(0x25).bar().f32(62.5).bar();
        cont.u8(0x8D).bar().u8(0x02).bar();
        f.changed.push(ChangedForm {
            form: 0x0001_0001,
            flags: (1 << 5) | (1 << 27),
            kind: TYPE_REFR,
            data: cont.b.clone(),
        });

        // Container REFR with bit 28 set: trailing 13-byte bit-28 block is allowed cleanly.
        let mut cont28 = cont.b.clone();
        cont28.extend([0u8; 13]);
        f.changed.push(ChangedForm {
            form: 0x0001_0002,
            flags: (1 << 5) | (1 << 27) | (1 << 28),
            kind: TYPE_REFR,
            data: cont28,
        });

        // Pure bit-5 container with unexpected trailing byte must fail strict at_end check.
        let mut bad_cont = Build { b: Vec::new() };
        bad_cont.vs(0).bar().u8(0xAA);
        f.changed.push(ChangedForm { form: 0x0001_0003, flags: 1 << 5, kind: TYPE_REFR, data: bad_cont.b });

        // 2. Map marker REFRs: bit 31 visible-only (0x01), bit 29+31 visible+travel (0x03 + 0x74),
        //    plus a non-marker bit-31 REFR (unknown extra 0x56) that must be omitted cleanly.
        let mut mm1 = Build { b: Vec::new() };
        mm1.vs(1).bar().u8(0x2C).bar().u8(0x01).bar();
        f.changed.push(ChangedForm { form: 0x0002_0001, flags: 1 << 31, kind: TYPE_REFR, data: mm1.b });

        let mut mm2 = Build { b: Vec::new() };
        mm2.vs(2).bar().u8(0x2C).bar().u8(0x03).bar().u8(0x74).bar().r(8).bar();
        f.changed.push(ChangedForm {
            form: 0x0002_0002,
            flags: (1 << 29) | (1 << 31),
            kind: TYPE_REFR,
            data: mm2.b,
        });

        let mut non_mm = Build { b: Vec::new() };
        non_mm.vs(1).bar().u8(0x56).bar().u32(99).bar();
        f.changed.push(ChangedForm { form: 0x0002_0003, flags: 1 << 31, kind: TYPE_REFR, data: non_mm.b });

        // 3. Created 0xFF REFR (valid 32-byte header + optional trailing bytes) and one short malformed 0xFF REFR.
        let mut cr = Build { b: Vec::new() };
        cr.r(3); // world/cell 0xDA726
        for v in [10.0f32, 20.0, 30.0, 0.0, 0.0, 1.25] {
            cr.f32(v);
        }
        cr.u8(0x01).r(4).bar(); // header_flag=1, base=0x15169, bar at byte 31
        assert_eq!(cr.b.len(), 32);
        f.changed.push(ChangedForm { form: 0xFF00_0055, flags: 1 << 1, kind: TYPE_REFR, data: cr.b });
        f.changed.push(ChangedForm { form: 0xFF00_0056, flags: 1 << 1, kind: TYPE_REFR, data: vec![0, 1, 2] });

        let conts = f.container_inventories();
        assert_eq!(conts.len(), 3);
        let c1 = conts[0].as_ref().unwrap();
        assert_eq!(c1.form, 0x0001_0001);
        assert_eq!(c1.items.len(), 2);
        assert_eq!(c1.items[0].count, -1);
        assert_eq!(c1.items[1].count, 2);
        assert_eq!(c1.items[1].condition, Some(62.5));
        assert_eq!(c1.items[1].weapon_mods, 2);
        assert_eq!(c1.items[1].entries[0].count, Some(2));
        assert_eq!(conts[1].as_ref().unwrap().form, 0x0001_0002);
        assert!(conts[2].is_err());

        let mms = f.map_markers();
        assert_eq!(
            mms,
            [
                Ok(FosMapMarker { form: 0x0002_0001, flags: 0x01 }),
                Ok(FosMapMarker { form: 0x0002_0002, flags: 0x03 }),
            ]
        );

        let crs = f.created_refs();
        assert_eq!(crs.len(), 2);
        assert_eq!(
            crs[0].as_ref().unwrap(),
            &FosCreatedRef {
                form: 0xFF00_0055,
                flags: 1 << 1,
                base: Some(0x15169),
                place: Some((0xDA726, [10.0, 20.0, 30.0], [0.0, 0.0, 1.25])),
                header_flag: 1,
            }
        );
        assert!(crs[1].is_err());
        // Created 0xFF REFRs must not show up as bogus errors in reference_changes().
        assert!(f.reference_changes().is_empty());
    }

    #[test]
    fn reads_v2_player_base_av3_perks_skills_and_new_item_extras() {
        let mut f = Fos::parse(&save()).unwrap();
        // Add extra form IDs for perks, teammate perk, hair, eyes, head part, spell, companion.
        // Existing fids length = 8 (1-based 1..=8):
        //   9 = 0x5100 (SPEL), 10 = 0x5200 (HAIR), 11 = 0x5300 (EYES),
        //  12 = 0x5400 (HDPT), 13 = 0x5500 (PERK 1), 14 = 0x5501 (PERK 2),
        //  15 = 0x5600 (TEAMMATE PERK)
        f.form_ids.extend([0x5100u32, 0x5200, 0x5300, 0x5400, 0x5500, 0x5501, 0x5600]);

        // Replace NPC_ 0x7 with multi-flag bits 1 | 2 | 4 | 5 | 11:
        // bit 1 (24B ACBS, female=1), bit 4 (spell list [0x5100] + empty list), bit 2 (SPECIAL),
        // bit 5 (name), bit 11 (FaceGen morphs + hair/eyes/color/head_parts).
        let mut npc = Build { b: Vec::new() };
        let mut acbs = [0u8; 24];
        acbs[0] = 1; // female flag
        npc.b.extend(acbs);
        npc.bar();
        npc.vs(1).bar().r(9).bar().vs(0).bar(); // bit 4 spells
        npc.b.extend([6, 7, 8, 4, 9, 5, 10]); // bit 2 SPECIAL AFTER bits 1+4
        npc.bar();
        npc.s("Wanderer").bar(); // bit 5 name
        // bit 11 FaceGen appearance
        npc.u8(1).bar();
        for i in 0..50 {
            npc.f32(if i == 0 { 0.25 } else { 0.0 }).bar();
        }
        for i in 0..30 {
            npc.f32(if i == 1 { -0.5 } else { 0.0 }).bar();
        }
        for i in 0..50 {
            npc.f32(if i == 2 { 0.75 } else { 0.0 }).bar();
        }
        npc.r(10).bar().r(11).bar().f32(0.4).bar().u32(0x0011_2233).bar();
        npc.vs(1).bar().r(12).bar();

        let npc_cf = f.changed.iter_mut().find(|c| c.form == PLAYER_BASE).unwrap();
        npc_cf.flags = (1 << 1) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 11);
        npc_cf.data = npc.b;

        // Build full player ACHR record with AV2/AV3/AV[231], scale, factions, teammates,
        // new item extras (0x17, 0x22, 0x23, 0x26, 0x28, 0x30), bit-22 skill points,
        // player perks, and teammate perks before the 54-byte end trailer.
        let mut p = Build { b: Vec::new() };
        p.r(3);
        for v in [10.0f32, 20.0, 30.0, 0.0, 0.0, 1.0] {
            p.f32(v);
        }
        p.bar();
        for i in 0..AV_ENTRIES {
            let v = match i {
                38 => 2.0,          // AV1 active mod (Melee +2)
                82 => 1.0,          // AV2 Strength permanent mod (+1)
                100 => 150.0,       // AV2 Karma (77 + 23)
                101 => 2400.0,      // AV2 XP (77 + 24)
                166 => -12.5,       // AV3 AP damage (154 + 12)
                180 => -35.0,       // AV3 limb 26 damage (154 + 26)
                208 => 110.0,       // AV3 rads (154 + 54)
                228 => 85.0,        // AV3 hunger (154 + 74)
                231 => -42.5,       // AV[231] health damage
                _ => 0.0,
            };
            p.f32(v).bar();
        }
        p.u8(0).bar().f32(1.05).bar(); // scale = 1.05
        // Actor extras: 0x5E factions (rank 2 and rank -1) + 0x1D teammates ([0x4242])
        p.vs(2).bar();
        p.u8(0x5E).bar().vs(2).bar().r(1).bar().u8(2).bar().r(2).bar().u8((-1i8) as u8).bar();
        p.u8(0x1D).bar().vs(1).bar().r(8).bar();
        // Inventory: 1 item exercising 0x17, 0x22, 0x23, 0x26, 0x28, 0x30
        p.vs(1).bar();
        p.r(4).bar().u32(1).bar().vs(1).bar().vs(6).bar();
        p.u8(0x17).bar();
        p.u8(0x22).bar().r(2).bar();
        p.u8(0x23).bar().u32(3).bar();
        p.u8(0x26).bar().f32(4.0).bar();
        p.u8(0x28).bar().f32(50.0).bar();
        p.u8(0x30).bar().f32(0.95).bar();
        // Bit-22 spent skill points + anchor
        p.vs(2).bar().u8(38).bar().f32(25.0).bar().u8(43).bar().f32(18.0).bar();
        p.b.extend_from_slice(b"\xFF\xFF\x7C\xFF\xFF\x7C\x00\x7C\x01\x01\x00\x00\x7C");
        // Post-AD DE AD DE perk section: u8 | u8 | u8 | vsval(2) | 2*(ref3 | rank |) | 4*(00|) | 5*(00000000|)
        p.b.extend_from_slice(b"\xAD\xDE\xAD\xDE\x7C");
        p.u8(0).bar().u8(0).bar().u8(0).bar();
        p.vs(2).bar().r(13).bar().u8(2).bar().r(14).bar().u8(1).bar();
        for _ in 0..4 {
            p.u8(0).bar();
        }
        for _ in 0..5 {
            p.u32(0).bar();
        }
        // Teammate perks right before 54-byte end trailer: 3*(u32|) | vsval(1) | (r(15)|1|) | 54B trailer
        for _ in 0..3 {
            p.u32(0).bar();
        }
        p.vs(1).bar().r(15).bar().u8(1).bar();
        p.u8(0).bar().u8(1).bar().u8(1).bar();
        for _ in 0..8 {
            p.u32(0).bar();
        }
        for _ in 0..4 {
            p.u8(0).bar();
        }

        let pl_cf = f.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
        pl_cf.flags = PLAYER_FLAGS | (1 << 22);
        pl_cf.data = p.b;

        let pl = f.player().unwrap();
        assert_eq!(pl.female, Some(true));
        assert_eq!(pl.spells, [Some(0x5100)]);
        assert_eq!(pl.special, Some([6, 7, 8, 4, 9, 5, 10]));
        let app = pl.appearance.as_ref().expect("appearance parsed");
        assert_eq!(app.fggs.len(), 50);
        assert_eq!(app.fggs[0], 0.25);
        assert_eq!(app.fgga.len(), 30);
        assert_eq!(app.fgga[1], -0.5);
        assert_eq!(app.fgts.len(), 50);
        assert_eq!(app.fgts[2], 0.75);
        assert_eq!(app.hair, Some(0x5200));
        assert_eq!(app.eyes, Some(0x5300));
        assert_eq!(app.hair_length, 0.4);
        assert_eq!(app.hair_color, [0x33, 0x22, 0x11]);
        assert_eq!(app.head_parts, [Some(0x5400)]);
        assert_eq!(pl.av_base, [(5, 1.0), (23, 150.0), (24, 2400.0)]);
        assert_eq!(pl.av_damage, [(12, -12.5), (26, -35.0), (54, 110.0), (74, 85.0)]);
        assert_eq!(pl.health_damage, Some(-42.5));
        assert_eq!(pl.scale, Some(1.05));
        assert_eq!(pl.factions, [(Some(0x1234), 2), (Some(0x5678), -1)]);
        assert_eq!(pl.teammates, [Some(0x4242)]);
        assert_eq!(pl.skill_points, [(38, 25.0), (43, 18.0)]);
        assert_eq!(pl.perks, [(Some(0x5500), 2), (Some(0x5501), 1)]);
        assert_eq!(pl.teammate_perks, [(Some(0x5600), 1)]);
        let inv = pl.inventory.expect("inventory parsed with 0x17/0x22/0x23/0x26/0x28/0x30");
        assert_eq!(inv.len(), 1);
        assert!(inv[0].worn);
    }

    #[test]
    fn reads_v2_actors_ref_extras_eczn_cells_and_radio() {
        let mut f = Fos::parse(&save()).unwrap();
        // Global table 1 type 10 (Pip-Boy radio)
        let mut t10 = Build { b: Vec::new() };
        t10.u32(1).bar().u8(1).bar().s("data\\sound\\songs\\track.mp3").bar().vs(1).bar().r(8).bar();
        f.globals_data.push((10, t10.b));

        // Non-player ACHR: bit 0 (FORM_DEAD), bit 4 (scale 1.15), bit 31 (0x0D script vars + 0x5E factions),
        // bits 5+27 (inventory with worn 0x17 item + 0x30 scale).
        let mut ac = Build { b: Vec::new() };
        ac.u8(0).bar(); // process level
        ac.u32(FORM_DEAD | 0x09).bar(); // bit 0 flags
        ac.f32(1.15).bar(); // bit 4 scale
        ac.vs(2).bar(); // actor extras
        ac.u8(0x0D).bar().r(1).bar().vs(1).bar().u32(2).bar().f64(7.0).bar().vs(0).bar().vs(0).bar();
        ac.u8(0x5E).bar().vs(1).bar().r(2).bar().u8(1).bar();
        ac.vs(1).bar().r(4).bar().u32(1).bar().vs(1).bar().vs(2).bar().u8(0x17).bar().u8(0x30).bar().f32(1.0).bar();
        f.changed.push(ChangedForm {
            form: 0x0003_0001,
            flags: (1 << 0) | (1 << 4) | (1 << 5) | (1 << 27) | (1 << 31),
            kind: TYPE_ACHR,
            data: ac.b,
        });

        // Placed REFR door: bit 4 (scale 0.85), bit 12 (0x2A lock), bit 28 (Open), bit 31 (0x21 owner + 0x0D script)
        let mut dr = Build { b: Vec::new() };
        dr.f32(0.85).bar();
        dr.vs(3).bar();
        dr.u8(0x2A).bar().u8(75).bar().u8(1).bar().r(4).bar().u32(0).bar().u32(0).bar();
        dr.u8(0x21).bar().r(8).bar();
        dr.u8(0x0D).bar().r(1).bar().vs(1).bar().u32(1).bar().f64(3.0).bar().vs(0).bar().vs(0).bar();
        dr.u8(0x28).bar().s("Open").bar();
        f.changed.push(ChangedForm {
            form: 0x0003_0002,
            flags: (1 << 4) | (1 << 12) | (1 << 28) | (1 << 31),
            kind: TYPE_REFR,
            data: dr.b,
        });

        // Locked container REFR (bit 12 + bits 5+27): inventory follows extra list.
        let mut lc = Build { b: Vec::new() };
        lc.vs(1).bar().u8(0x2A).bar().u8(50).bar().u8(1).bar().r(0).bar().u32(0).bar().u32(0).bar();
        lc.vs(1).bar().r(4).bar().u32(5).bar().vs(0).bar();
        f.changed.push(ChangedForm {
            form: 0x0003_0003,
            flags: (1 << 5) | (1 << 12) | (1 << 27),
            kind: TYPE_REFR,
            data: lc.b,
        });

        // ECZN bit 31 (17 bytes)
        let mut ez = Build { b: Vec::new() };
        ez.u32(10).u32(10).u32(10).u32(18).bar();
        assert_eq!(ez.b.len(), 17);
        f.changed.push(ChangedForm { form: 0x0003_0004, flags: 1 << 31, kind: TYPE_ECZN, data: ez.b });

        // CELL bits 1+3+29+30+31: bit 3 owner is serialized AFTER bit-31 fog!
        let mut cl = Build { b: Vec::new() };
        cl.b.extend([0u8; 4]); // bit 29 grid
        cl.u32(72).bar();      // bit 30 detach hours
        cl.u8(0x03).bar();     // bit 1 flags
        cl.b.extend([0xFFu8; 8]); // bit 31 fog
        cl.r(8).bar();         // bit 3 owner AFTER fog
        f.changed.push(ChangedForm {
            form: 0x0003_0005,
            flags: (1 << 1) | (1 << 3) | (1 << 29) | (1 << 30) | (1 << 31),
            kind: TYPE_CELL,
            data: cl.b,
        });

        let radio = f.radio_state().unwrap().expect("radio parsed");
        assert!(radio.on);
        assert_eq!(radio.song, "data\\sound\\songs\\track.mp3");
        assert_eq!(radio.station, Some(0x4242));

        let actors = f.actor_changes();
        assert_eq!(actors.len(), 1);
        assert_eq!(actors[0].form, 0x0003_0001);
        assert_eq!(actors[0].form_flags, Some(FORM_DEAD | 0x09));
        assert_eq!(actors[0].scale, Some(1.15));
        assert_eq!(actors[0].factions, [(Some(0x5678), 1)]);
        assert_eq!(actors[0].script_vars.as_ref().unwrap().vars, [(2, VarValue::Num(7.0))]);
        assert!(actors[0].inventory.as_ref().unwrap()[0].worn);

        let refs = f.ref_extras();
        assert_eq!(refs.len(), 2);
        let r_door = &refs[0];
        assert_eq!(r_door.form, 0x0003_0002);
        assert_eq!(r_door.scale, Some(0.85));
        assert_eq!(
            r_door.lock,
            Some(FosLock { level: 75, flags: 1, key: Some(0x15169), num_tries: 0, num_times_unlocked: 0 })
        );
        assert_eq!(r_door.door_open, Some(true));
        assert_eq!(r_door.owner, Some(Some(0x4242)));
        assert_eq!(r_door.script_vars.as_ref().unwrap().vars, [(1, VarValue::Num(3.0))]);
        let r_cont = &refs[1];
        assert_eq!(r_cont.form, 0x0003_0003);
        assert_eq!(
            r_cont.lock,
            Some(FosLock { level: 50, flags: 1, key: None, num_tries: 0, num_times_unlocked: 0 })
        );
        assert_eq!(r_cont.inventory.as_ref().unwrap()[0].count, 5);

        let zones = f.encounter_zones();
        assert_eq!(zones, [Ok(FosEncounterZone { form: 0x0003_0004, stamps: [10, 10, 10], level: 18 })]);

        let cells = f.cells();
        assert_eq!(
            cells,
            [Ok(FosCell {
                form: 0x0003_0005,
                detach_hours: Some(72),
                cell_flags: Some(0x03),
                owner: Some(Some(0x4242)),
                has_fog: true,
            })]
        );
    }

    #[test]
    fn parse_ref_extras_skips_all_listed_auxiliary_types() {
        let mut f = Fos::parse(&save()).unwrap();
        // Exercise all 18 skipped auxiliary reference extras listed as [hypothesis] in save-files.md §6.2
        // followed by actionable 0x2A lock + 0x21 owner + 0x0D script vars.
        let mut re = Build { b: Vec::new() };
        re.vs(21).bar();
        re.u8(0x18).bar().r(1).bar().f32(1.0).f32(2.0).f32(3.0).bar().f32(4.0).bar();
        re.u8(0x1B).bar().vs(1).bar().r(2).bar();
        re.u8(0x1C).bar().r(3).bar();
        re.u8(0x22).bar().r(2).bar();
        re.u8(0x23).bar().u32(1).bar();
        re.u8(0x28).bar().f32(25.0).bar();
        re.u8(0x2C).bar().u8(0x03).bar();
        re.u8(0x2E).bar().r(1).bar().r(2).bar().u32(5).bar();
        re.u8(0x2F).bar().u32(9).bar().u8(1).bar();
        re.u8(0x30).bar().f32(1.25).bar();
        re.u8(0x46).bar().r(8).bar();
        re.u8(0x56).bar().u32(7).bar();
        re.u8(0x60).bar().u32(8).bar();
        re.u8(0x73).bar().u8(2).bar();
        re.u8(0x74).bar().r(8).bar();
        re.u8(0x75).bar().r(1).bar().r(2).bar().u8(1).bar();
        re.u8(0x7C).bar().vs(1).bar().r(4).bar();
        re.u8(0x8B).bar().f32(0.0).f32(0.0).f32(0.0).bar().r(3).bar().u32(2).bar().u8(1).bar();
        // Actionable entries after all 18 skipped auxiliaries:
        re.u8(0x2A).bar().u8(100).bar().u8(1).bar().r(4).bar().u32(2).bar().u32(0).bar();
        re.u8(0x21).bar().r(8).bar();
        re.u8(0x0D).bar().r(1).bar().vs(1).bar().u32(1).bar().f64(9.0).bar().vs(0).bar().vs(0).bar();
        f.changed.push(ChangedForm {
            form: 0x0004_0001,
            flags: (1 << 12) | (1 << 31),
            kind: TYPE_REFR,
            data: re.b,
        });
        // Unknown extra type (0xFE) must fail soft and omit the record.
        let mut bad = Build { b: Vec::new() };
        bad.vs(2).bar().u8(0xFE).bar().u32(0).bar().u8(0x21).bar().r(8).bar();
        f.changed.push(ChangedForm {
            form: 0x0004_0002,
            flags: 1 << 31,
            kind: TYPE_REFR,
            data: bad.b,
        });

        let refs = f.ref_extras();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].form, 0x0004_0001);
        assert_eq!(
            refs[0].lock,
            Some(FosLock { level: 100, flags: 1, key: Some(0x15169), num_tries: 2, num_times_unlocked: 0 })
        );
        assert_eq!(refs[0].owner, Some(Some(0x4242)));
        assert_eq!(refs[0].script_vars.as_ref().unwrap().vars, [(1, VarValue::Num(9.0))]);
    }

    #[test]
    fn regression_observed_v2_decoder_fixes_bugs_1_to_5_and_8() {
        let mut f = Fos::parse(&save()).unwrap();
        f.form_ids.extend([0x5500u32]); // 1-based index 9 = PERK

        // BUG #1: Player ACHR WITHOUT bit 4 (0x90000022) has NO scale f32 between process_level and actor extras!
        let mut p_no_bit4 = Build { b: Vec::new() };
        p_no_bit4.r(3).f32(1.0).f32(2.0).f32(3.0).f32(0.0).f32(0.0).f32(0.5).bar();
        for _ in 0..AV_ENTRIES {
            p_no_bit4.f32(0.0).bar();
        }
        p_no_bit4.u8(0).bar(); // process_level = 0, NO scale float!
        p_no_bit4.vs(1).bar().u8(0x5E).bar().vs(1).bar().r(1).bar().u8(2).bar(); // 0x5E faction
        p_no_bit4.vs(1).bar().r(4).bar().u32(3).bar().vs(0).bar(); // inventory: 1 item
        // BUG #2: Bit 22 with BOTH non-skill SetAV (Luck = 11 -> 9.0) and skill (Melee = 38 -> 25.0)
        // preceded by 3 zero ref3 | fields as observed in vanilla 1.4.0.525.
        p_no_bit4.r(0).bar().r(0).bar().r(0).bar();
        p_no_bit4.vs(2).bar().u8(11).bar().f32(9.0).bar().u8(38).bar().f32(25.0).bar();
        p_no_bit4.b.extend_from_slice(b"\xFF\xFF\x7C\xFF\xFF\x7C\x00\x7C\x01\x01\x00\x00\x7C");
        p_no_bit4.b.extend_from_slice(b"\xAD\xDE\xAD\xDE\x7C");
        p_no_bit4.u8(0).bar().u8(0).bar().u8(0).bar();
        p_no_bit4.vs(1).bar().r(9).bar().u8(1).bar();
        for _ in 0..4 {
            p_no_bit4.u8(0).bar();
        }
        for _ in 0..5 {
            p_no_bit4.u32(0).bar();
        }
        for _ in 0..3 {
            p_no_bit4.u32(0).bar();
        }
        p_no_bit4.u8(0).bar().u8(1).bar().u8(1).bar();
        for _ in 0..8 {
            p_no_bit4.u32(0).bar();
        }
        for _ in 0..4 {
            p_no_bit4.u8(0).bar();
        }
        let pl_cf = f.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
        pl_cf.flags = (1 << 1) | (1 << 5) | (1 << 22) | (1 << 28) | (1 << 31); // bit 4 ABSENT!
        pl_cf.data = p_no_bit4.b;

        // BUG #8: NPC_ 0x7 with bit 24 (0x0100_0000) `01 7c` written by Player.SexChange
        let npc_cf = f.changed.iter_mut().find(|c| c.form == PLAYER_BASE).unwrap();
        npc_cf.flags = 1 << 24;
        npc_cf.data = vec![0x01, BAR];

        let pl = f.player().unwrap();
        assert_eq!(pl.scale, None);
        assert_eq!(pl.factions, [(Some(0x1234), 2)]);
        assert_eq!(pl.inventory.as_ref().map(Vec::len), Some(1));
        assert_eq!(pl.base_av_overrides, [(11, 9.0), (38, 25.0)]);
        assert_eq!(pl.skill_points, [(38, 25.0)]);
        assert_eq!(pl.perks, [(Some(0x5500), 1)]);
        assert_eq!(pl.female, Some(true));
        assert!(pl.female_from_bit24);

        // BUG #1 (bit-4-present contrast): adding bit 4 + scale 1.25 decodes scale cleanly
        let mut p_bit4 = Build { b: Vec::new() };
        p_bit4.r(3).f32(1.0).f32(2.0).f32(3.0).f32(0.0).f32(0.0).f32(0.5).bar();
        for _ in 0..AV_ENTRIES {
            p_bit4.f32(0.0).bar();
        }
        p_bit4.u8(0).bar().f32(1.25).bar();
        p_bit4.vs(1).bar().u8(0x5E).bar().vs(1).bar().r(1).bar().u8(2).bar();
        p_bit4.vs(1).bar().r(4).bar().u32(3).bar().vs(0).bar();
        let pl_cf = f.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
        pl_cf.flags = (1 << 1) | (1 << 4) | (1 << 5) | (1 << 28) | (1 << 31);
        pl_cf.data = p_bit4.b;
        let pl_s = f.player().unwrap();
        assert_eq!(pl_s.scale, Some(1.25));
        assert_eq!(pl_s.inventory.as_ref().map(Vec::len), Some(1));

        // BUG #1 (no-extras / no-inv save 0x10000002): process block starts immediately with 0xFF |
        let mut p_bare = Build { b: Vec::new() };
        p_bare.r(3).f32(0.0).f32(0.0).f32(0.0).f32(0.0).f32(0.0).f32(0.0).bar();
        for _ in 0..AV_ENTRIES {
            p_bare.f32(0.0).bar();
        }
        p_bare.u8(0).bar().u8(0xFF).bar().u8(0).bar();
        let pl_cf = f.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
        pl_cf.flags = (1 << 1) | (1 << 28);
        pl_cf.data = p_bare.b;
        let pl_bare = f.player().unwrap();
        assert_eq!(pl_bare.scale, None);
        assert_eq!(pl_bare.inventory, Some(Vec::new()));

        // BUG #3: Global[10] vanilla 1.4.0.525 layout (OFF: 2nd list = 0; ON: 2nd list = 1 with ref3 + playback_ms)
        let mut r_off = Build { b: Vec::new() };
        r_off.u32(0).bar().u8(0).bar().s("").vs(0).bar().vs(0).bar().r(0).bar().r(0).bar().vs(1).bar().r(8).bar();
        f.globals_data.retain(|(t, _)| *t != 10);
        f.globals_data.push((10, r_off.b));
        let st_off = f.radio_state().unwrap().unwrap();
        assert!(!st_off.on);
        assert_eq!(st_off.station, None);
        assert_eq!(st_off.playback_ms, None);
        assert!(st_off.from_active_list);

        let mut r_on = Build { b: Vec::new() };
        r_on.u32(0).bar().u8(0).bar().s("").vs(0).bar(); // header stays zero!
        r_on.vs(1).bar().r(8).bar().u32(9740).bar().u32(1).bar().u8(0x64).bar().u8(0x64).bar().u32(0x33).bar().u8(0).bar().u8(0).bar();
        r_on.r(0).bar().r(0).bar().vs(1).bar().r(8).bar();
        f.globals_data.retain(|(t, _)| *t != 10);
        f.globals_data.push((10, r_on.b));
        let st_on = f.radio_state().unwrap().unwrap();
        assert!(st_on.on);
        assert_eq!(st_on.station, Some(0x4242));
        assert_eq!(st_on.playback_ms, Some(9740));
        assert!(st_on.from_active_list);

        // BUG #4: REFR with bit 6 (0x40, SetOwnership 0x21) + bit 4 scale + bits 27+28 inventory
        let mut owned_cont = Build { b: Vec::new() };
        owned_cont.f32(1.5).bar();
        owned_cont.vs(1).bar().u8(0x21).bar().r(8).bar();
        owned_cont.vs(1).bar().r(4).bar().u32(1).bar().vs(0).bar();
        owned_cont.u8(0x2C).bar().s("Close").bar().u8(0).bar();
        f.changed.push(ChangedForm {
            form: 0x0005_0001,
            flags: (1 << 4) | (1 << 6) | (1 << 27) | (1 << 28), // 0x18000050!
            kind: TYPE_REFR,
            data: owned_cont.b,
        });
        let extras = f.ref_extras();
        let oc = extras.iter().find(|r| r.form == 0x0005_0001).expect("bit-6 owned container parsed");
        assert_eq!(oc.scale, Some(1.5));
        assert_eq!(oc.owner, Some(Some(0x4242)));
        assert_eq!(oc.inventory.as_ref().map(|v| v[0].count), Some(1));

        // BUG #5: CELL flags=0x8000000a (bits 1+3+31) with 20 7c + 76B fog + trailing ref3 |,
        // AND CELL flags=0x0000000a (bits 1+3 without fog)
        let mut cell_fog = Build { b: Vec::new() };
        cell_fog.u8(0x20).bar();
        cell_fog.b.extend([0x08, BAR, 0x00, BAR, 0xFF, BAR]);
        cell_fog.b.extend([0x00u8; 70]);
        cell_fog.r(8).bar();
        assert_eq!(cell_fog.b.len(), 82);
        f.changed.push(ChangedForm {
            form: 0x0005_0002,
            flags: (1 << 1) | (1 << 3) | (1 << 31),
            kind: TYPE_CELL,
            data: cell_fog.b,
        });
        let mut cell_nofog = Build { b: Vec::new() };
        cell_nofog.u8(0x20).bar().r(8).bar();
        f.changed.push(ChangedForm {
            form: 0x0005_0003,
            flags: (1 << 1) | (1 << 3),
            kind: TYPE_CELL,
            data: cell_nofog.b,
        });
        let cells: Vec<_> = f.cells().into_iter().map(Result::unwrap).collect();
        let cf1 = cells.iter().find(|c| c.form == 0x0005_0002).unwrap();
        assert_eq!(cf1.cell_flags, Some(0x20));
        assert_eq!(cf1.owner, Some(Some(0x4242)));
        assert!(cf1.has_fog);
        let cf2 = cells.iter().find(|c| c.form == 0x0005_0003).unwrap();
        assert_eq!(cf2.cell_flags, Some(0x20));
        assert_eq!(cf2.owner, Some(Some(0x4242)));
        assert!(!cf2.has_fog);
    }

    #[test]
    fn regression_bug9_player_tail_notes_and_type_note_union() {
        let mut f = Fos::parse(&save()).unwrap();
        // fids 1..=8 already exist; add 9=NOTE_A (0x0017_5AB0), 10=NOTE_B (0x000E_88A0),
        // 11=NOTE_C (0x0017_7875), 12=PERK (0x5500)
        f.form_ids.extend([0x0017_5AB0u32, 0x000E_88A0, 0x0017_7875, 0x5500]);

        let mut p = Build { b: Vec::new() };
        p.r(3).f32(1.0).f32(2.0).f32(3.0).f32(0.0).f32(0.0).f32(0.0).bar();
        for _ in 0..AV_ENTRIES {
            p.f32(0.0).bar();
        }
        p.u8(0).bar(); // bit 4 absent: no scale float
        p.vs(0).bar(); // 0 actor extras
        p.vs(0).bar(); // 0 inventory items
        // Tail: DEAD_MARK + prefix + 4*(u32|) + u8| + u32| + 11*(ref3|) (with non-zero slot 4!)
        p.b.extend_from_slice(b"\xAD\xDE\xAD\xDE\x7C");
        p.u8(0).bar();
        for _ in 0..4 {
            p.u32(0).bar();
        }
        p.u8(1).bar().u32(200).bar();
        for slot in 0..11 {
            p.r(if slot == 4 { 8 } else { 0 }).bar();
        }
        // vsval(0)| + notes list [NOTE_A, NOTE_B] + vsval(0)|
        p.vs(0).bar();
        p.vs(2).bar().r(9).bar().r(10).bar();
        p.vs(0).bar();
        // Known actors: 1 entry (r(8), 0, 0)
        p.vs(1).bar().r(8).bar().u8(0).bar().u8(0).bar();
        // Player perks: 1 entry (r(12), rank 1)
        p.vs(1).bar().r(12).bar().u8(1).bar();
        // 4*(u8|) + 5*(u32|)
        for _ in 0..4 {
            p.u8(0).bar();
        }
        for _ in 0..5 {
            p.u32(0).bar();
        }
        // End trailer
        for _ in 0..3 {
            p.u32(0).bar();
        }
        p.u8(0).bar().u8(1).bar().u8(1).bar();
        for _ in 0..8 {
            p.u32(0).bar();
        }
        for _ in 0..4 {
            p.u8(0).bar();
        }

        let pl_cf = f.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
        pl_cf.flags = (1 << 1) | (1 << 5) | (1 << 28) | (1 << 31);
        pl_cf.data = p.b;

        // 0 TYPE_NOTE records: player_tail_notes & notes both return [NOTE_A, NOTE_B]
        assert!(f.note_records().is_empty());
        assert_eq!(f.player_tail_notes(), [0x0017_5AB0, 0x000E_88A0]);
        assert_eq!(f.notes(), [0x0017_5AB0, 0x000E_88A0]);
        let pl = f.player().unwrap();
        assert_eq!(pl.notes, [0x0017_5AB0, 0x000E_88A0]);
        assert_eq!(pl.perks, [(Some(0x5500), 1)]);

        // Adding TYPE_NOTE bit 31 for duplicate NOTE_A and distinct NOTE_C deduplicates cleanly
        f.changed.push(ChangedForm {
            form: 0x0017_5AB0,
            flags: 1 << 31,
            kind: TYPE_NOTE,
            data: Vec::new(),
        });
        f.changed.push(ChangedForm {
            form: 0x0017_7875,
            flags: 1 << 31,
            kind: TYPE_NOTE,
            data: Vec::new(),
        });
        assert_eq!(f.note_records(), [0x0017_5AB0, 0x0017_7875]);
        assert_eq!(f.notes(), [0x0017_5AB0, 0x000E_88A0, 0x0017_7875]);
    }

    #[test]
    fn regression_round3_bugs_11_12_13_bit10_0x3e_0x39_and_tok76() {
        let mut f = Fos::parse(&save()).unwrap();
        // BUG #11: REFR with bit 10 (0x400) carrying 0x1C + 0x21 owner AND container with bit 10 + bits 5/27.
        let mut picked = Build { b: Vec::new() };
        picked.u32(0x0000_042A).bar(); // bit 0 form_flags
        picked.vs(2).bar().u8(0x1C).bar().r(5).bar().u8(0x21).bar().r(8).bar();
        f.changed.push(ChangedForm {
            form: 0x0006_0001,
            flags: (1 << 0) | (1 << 10),
            kind: TYPE_REFR,
            data: picked.b,
        });
        let mut bit10_cont = Build { b: Vec::new() };
        bit10_cont.vs(1).bar().u8(0x1C).bar().r(5).bar();
        bit10_cont.vs(1).bar().r(4).bar().u32(2).bar().vs(0).bar();
        f.changed.push(ChangedForm {
            form: 0x0006_0002,
            flags: (1 << 5) | (1 << 10) | (1 << 27),
            kind: TYPE_REFR,
            data: bit10_cont.b,
        });
        // BUG #13 (REFR side): dropped weapon carrying 0x25 + 0x6E + 0x39 + 0x21 owner parses cleanly.
        let mut drop_weap = Build { b: Vec::new() };
        drop_weap.vs(4).bar();
        drop_weap.u8(0x25).bar().f32(149.85).bar();
        drop_weap.u8(0x6E).bar().r(2).bar().u32(4).bar();
        drop_weap.u8(0x39).bar().r(8).bar();
        drop_weap.u8(0x21).bar().r(1).bar();
        f.changed.push(ChangedForm {
            form: 0x0006_0003,
            flags: 1 << 31,
            kind: TYPE_REFR,
            data: drop_weap.b,
        });

        // BUG #12 (actor side): non-player ACHR inventory item with 0x25 + 0x16 + 0x3E (EquipItem <item> 1)
        // must parse cleanly instead of dropping the entire inventory.
        let mut boone = Build { b: Vec::new() };
        boone.u8(0).bar();
        boone.vs(1).bar().r(4).bar().u32(1).bar().vs(1).bar().vs(3).bar();
        boone.u8(0x25).bar().f32(150.0).bar();
        boone.u8(0x16).bar();
        boone.u8(0x3E).bar();
        f.changed.push(ChangedForm {
            form: 0x0006_0004,
            flags: (1 << 5) | (1 << 27),
            kind: TYPE_ACHR,
            data: boone.b,
        });

        let refs = f.ref_extras();
        let r1 = refs.iter().find(|r| r.form == 0x0006_0001).expect("bit-10 REFR with 0x1C + 0x21 parsed");
        assert_eq!(r1.owner, Some(Some(0x4242)));
        let r2 = refs.iter().find(|r| r.form == 0x0006_0002).expect("bit-10 container with 0x1C + inv parsed");
        assert_eq!(r2.inventory.as_ref().map(|v| v[0].count), Some(2));
        let r3 = refs.iter().find(|r| r.form == 0x0006_0003).expect("REFR with 0x25 + 0x6E + 0x39 parsed");
        assert_eq!(r3.owner, Some(Some(0x1234)));

        let actors = f.actor_changes();
        let ac = actors.iter().find(|a| a.form == 0x0006_0004).expect("actor with 0x3E parsed");
        let ainv = ac.inventory.as_ref().expect("actor inventory kept with 0x3E");
        assert_eq!(ainv.len(), 1);
        assert!(ainv[0].worn);
        assert_eq!(ainv[0].condition, Some(150.0));

        // BUG #12 (player side) + BUG #13 (ACHR 0x14 process-block tok[76] active magazine rounds):
        let build_player_with_proc = |tok70: u32, tok76: u32| -> Vec<u8> {
            const WIDTHS: [usize; 77] = [
                1, 1, 1, 1, 1, 1, 1, 1, 4, 4, 1, 1, 3, 3, 4, 4, 4, 3, 1, 4, 3, 4, 4, 4, 1, 2, 4, 4,
                3, 3, 3, 3, 3, 1, 4, 1, 1, 1, 4, 4, 4, 1, 12, 4, 1, 1, 1, 2, 12, 1, 1, 1, 1, 4, 1,
                4, 4, 1, 1, 1, 2, 4, 1, 4, 4, 1, 1, 4, 4, 4, 4, 1, 3, 3, 3, 3, 4,
            ];
            let mut p = Build { b: Vec::new() };
            p.r(3).f32(1.0).f32(2.0).f32(3.0).f32(0.0).f32(0.0).f32(0.0).bar();
            for _ in 0..AV_ENTRIES {
                p.f32(0.0).bar();
            }
            p.u8(0).bar(); // bit 4 absent
            p.vs(0).bar(); // 0 actor extras
            // Player inventory: 1 worn weapon carrying 0x25 + 0x16 + 0x3E (no 0x6E!)
            p.vs(1).bar().r(4).bar().u32(1).bar().vs(1).bar().vs(3).bar();
            p.u8(0x25).bar().f32(149.0).bar().u8(0x16).bar().u8(0x3E).bar();
            // High-Process block tok[0..=76]
            for (idx, &w) in WIDTHS.iter().enumerate() {
                if idx == 70 {
                    p.u32(tok70).bar();
                } else if idx == 76 {
                    p.u32(tok76).bar();
                } else {
                    p.b.extend(std::iter::repeat_n(0u8, w));
                    p.bar();
                }
            }
            p.b
        };

        let pl_equipped = with_player(build_player_with_proc(9, 11), (1 << 1) | (1 << 5) | (1 << 28) | (1 << 31));
        assert_eq!(pl_equipped.inventory.as_ref().map(Vec::len), Some(1));
        assert!(pl_equipped.inventory.as_ref().unwrap()[0].worn);
        assert_eq!(pl_equipped.active_weapon_magazine, Some(11));

        let pl_unequipped = with_player(build_player_with_proc(u32::MAX, 1), (1 << 1) | (1 << 5) | (1 << 28) | (1 << 31));
        assert_eq!(pl_unequipped.active_weapon_magazine, None);
    }
}
