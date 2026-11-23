//! A reader of the original game's save files (`.fos`), for the extender lab and
//! offline `fnvtool save` conversion/inspection (issue #1023, docs/spec-notes/save-files.md).
//! The game runtime itself never loads or writes a `.fos`.
//!
//! The layout is in docs/spec-notes/save-files.md; every part read here is tagged there.
//! Decoded sections: header (including screenshot RGB bytes and play time), plugin list,
//! global variables (type 3), misc statistics (type 0), player location/active quest
//! (type 1), death-count prefix (type 2), sky/weather (type 8), quests (flags, delay,
//! stages, objectives, variables), player SPECIAL, tag skills, AV arrays, and inventory
//! (with actor extra-data list and item extra-data types), container `REFR`
//! inventories (bits 5/27, including extra `0x2F`), map marker `REFR` discovery/visibility
//! flags (extra `0x2C`), created `0xFF` `REFR` headers, placed reference form flags and
//! places, faction relations, form lists (`FLST`), leveled item lists (`LVLI`),
//! reputations (`REPU`), challenges (`CHAL`), notes (`NOTE`), and said-once dialogue
//! lines (`INFO`). Anything else is kept as raw bytes per changed form.

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
    /// The flags byte (change flag 1): bit 0 running, bit 1 completed [hypothesis].
    pub flags: Option<u8>,
    /// SetQuestDelay's value (change flag 2) [hypothesis].
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
    /// Sub-stack count from extra data type 0x24 (`i16`), when present [hypothesis: stack count].
    pub count: Option<i16>,
    /// Extra data type 0x16 (worn marker, 0 payload bytes) [hypothesis: worn].
    pub worn: bool,
    /// Item health from extra data type 0x25 (`f32` HP), when present [hypothesis: condition].
    pub condition: Option<f32>,
    /// Owner reference from extra data type 0x21 (`ref3`), when present (`Some(None)` = zero or
    /// unresolved reference) [hypothesis: owner].
    pub owner: Option<Option<u32>>,
    /// Weapon mod bitmask from extra data type 0x8D (`u8 & 0x07`) [hypothesis: weapon mods].
    pub weapon_mods: u8,
    /// Quick-slot hotkey byte from extra data type 0x4A (`u8`), when present [hypothesis: hotkey].
    pub hotkey: Option<u8>,
    /// Loaded ammo `(ammo_form_id, rounds)` from extra data type 0x6E (`ref3 | u32 |`), when
    /// present [hypothesis: weapon magazine ammo and round count].
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
    /// Any extra data list carries the worn marker (type 0x16) [hypothesis: worn].
    pub worn: bool,
    /// Item health from extra data type 0x25 (f32 HP), when present [hypothesis: condition].
    pub condition: Option<f32>,
    /// Owner reference from extra data type 0x21, when present [hypothesis: owner].
    pub owner: Option<Option<u32>>,
    /// Weapon mod bitmask from extra data type 0x8D (`u8 & 0x07`) [hypothesis: weapon mods].
    pub weapon_mods: u8,
    /// Quick-slot hotkey byte from extra data type 0x4A, when present [hypothesis: hotkey].
    pub hotkey: Option<u8>,
    /// Loaded ammo `(ammo_form_id, rounds)` from extra data type 0x6E, when present
    /// [hypothesis: weapon magazine ammo and round count].
    pub ammo: Option<(Option<u32>, u32)>,
    /// Per-extra-data-list sub-stack entries decoded for this item, in save order.
    pub entries: Vec<FosItemEntry>,
}

/// The player's state that the reader decodes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosPlayer {
    /// The worldspace (outdoors) or interior cell, position and rotation (radians).
    pub place: Option<Place>,
    /// Strength … Luck from the player's base form (NPC_ 0x7, change flag 2).
    pub special: Option<[u8; 7]>,
    /// Tag skills, as actor value codes (PlayerClass, change flag 1).
    pub tag_skills: Vec<u32>,
    /// Whether the 232-entry actor-value block matched the observed bar-separated layout.
    pub av_parsed: bool,
    /// The first array of actor-value modifiers by code (§6.4), non-zero entries
    /// [observed: nvfairport's equal the worn Vault 21 Jumpsuit's enchantment; hypothesis:
    /// active effects' modifiers].
    pub av_modifiers: Vec<(u32, f32)>,
    /// The second array of actor-value entries by code (§6.4), non-zero entries
    /// [hypothesis: play-modified base values such as Karma (23) and Experience (24)].
    pub av_base: Vec<(u32, f32)>,
    /// The inventory changes, `None` when the record's layout was not understood.
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

    /// Dialogue responses marked said-once (INFO, change flag 31, empty data; §6.2).
    pub fn said_infos(&self) -> Vec<u32> {
        self.records(TYPE_INFO)
            .filter(|f| f.has(31) && f.data.is_empty() && f.form != 0)
            .map(|f| f.form)
            .collect()
    }

    /// Notes owned by the player (NOTE, change flag 31, empty data; §6.3).
    pub fn notes(&self) -> Vec<u32> {
        self.records(TYPE_NOTE)
            .filter(|f| f.has(31) && f.data.is_empty() && f.form != 0)
            .map(|f| f.form)
            .collect()
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

    /// The player's place, SPECIAL, tags, modifiers and inventory.
    pub fn player(&self) -> Result<FosPlayer, FosError> {
        let mut p = FosPlayer::default();
        if let Some(npc) = self.record(PLAYER_BASE, TYPE_NPC).filter(|f| f.has(2)) {
            p.special = Some(npc.data.get(..7).ok_or_else(|| FosError("short SPECIAL".into()))?.try_into().unwrap());
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
            }
        }
        if av_ok {
            p.av_parsed = true;
            p.av_modifiers = mods;
            p.av_base = base;
            p.inventory = self.player_inventory(av_cur).ok();
        }
        Ok(p)
    }

    /// The player's inventory list, following the `0x00 | f32 |` header and the
    /// `vsval`-counted actor extra-data list after the actor-value block (save-files.md
    /// §6.4). Every separator and extra-data type byte is verified; an unknown actor or
    /// item extra-data type fails soft so no inventory is guessed out of misaligned bytes.
    fn player_inventory(&self, mut c: Cur<'_>) -> Result<Vec<FosItem>, FosError> {
        c.expect_u8(0)?;
        c.bar()?;
        let scale = c.f32()?;
        if !scale.is_finite() {
            return err("non-finite float after actor values");
        }
        c.bar()?;
        let actor_extras = c.vsval()?;
        c.bar()?;
        for _ in 0..actor_extras {
            let ty = c.u8()?;
            c.bar()?;
            match ty {
                // Faction changes: vsval | n * (ref3 | u8 |)
                0x5E => {
                    let n = c.vsval()?;
                    c.bar()?;
                    for _ in 0..n {
                        c.ref3()?;
                        c.bar()?;
                        c.u8()?;
                        c.bar()?;
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
                // Ref list: vsval | n * (ref3 |)
                0x1D => {
                    let n = c.vsval()?;
                    c.bar()?;
                    for _ in 0..n {
                        c.ref3()?;
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
        self.parse_item_list(&mut c)
    }

    /// Shared bar-delimited inventory item-list parser used by both the player record (§6.4)
    /// and container `REFR` records (§6.2). Supports all 9 observed player item extra-data
    /// types plus container extra type `0x2F` (`u32 | u8 |`).
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
                            c.ref3()?;
                            c.bar()?;
                            let nv = c.vsval()?;
                            c.bar()?;
                            for _ in 0..nv {
                                let idx = c.u32()?;
                                c.bar()?;
                                if idx & 0x8000_0000 != 0 {
                                    c.ref3()?;
                                } else {
                                    c.f64()?;
                                }
                                c.bar()?;
                            }
                            for _ in 0..2 {
                                if c.vsval()? != 0 {
                                    return err("non-empty trailing list in item script extra");
                                }
                                c.bar()?;
                            }
                        }
                        // Worn item marker (0 payload bytes)
                        0x16 => {
                            c.bar()?;
                            entry.worn = true;
                        }
                        // Original reference (ref3 |)
                        0x1C => {
                            c.bar()?;
                            c.ref3()?;
                            c.bar()?;
                        }
                        // Owner (ref3 |)
                        0x21 => {
                            c.bar()?;
                            entry.owner = Some(self.reference(c.ref3()?).ok().flatten());
                            c.bar()?;
                        }
                        // Stack count (i16 |)
                        0x24 => {
                            c.bar()?;
                            entry.count = Some(c.i16()?);
                            c.bar()?;
                        }
                        // Condition HP (f32 HP |)
                        0x25 => {
                            c.bar()?;
                            entry.condition = Some(c.f32()?);
                            c.bar()?;
                        }
                        // Container-extra 0x2F (u32 | u8 |) [hypothesis]
                        0x2F => {
                            c.bar()?;
                            c.u32()?;
                            c.bar()?;
                            c.u8()?;
                            c.bar()?;
                        }
                        // Hotkey (u8 |)
                        0x4A => {
                            c.bar()?;
                            entry.hotkey = Some(c.u8()?);
                            c.bar()?;
                        }
                        // Magazine ammo (ref3 | u32 |)
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
                    condition: Some(75.5),
                    owner: None,
                    weapon_mods: 5,
                    hotkey: Some(3),
                    ammo: Some((Some(0x5678), 12)),
                },
                FosItemEntry {
                    count: Some(2),
                    worn: false,
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
}
