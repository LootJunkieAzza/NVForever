//! Base forms the simulation needs, read from the winning record of every FormID.
//!
//! Only the record types that take part in Layer 4 (items, containers, doors,
//! activators, actors' carried items, leveled item lists, globals) are interpreted;
//! every other winner is kept as its signature so a reference to it can still be
//! classified. Field meanings come from the Layer 1 schema (see `extract`).

use fnv_esm::raw::record_flags;
use fnv_esm::{LoadOrder, Record, Schema};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

use crate::extract::{self, first, global, has_flag, int, local_fid, num, text, Decoded, Issues};
use crate::form::FormId;
use crate::inventory::Extra;
use crate::settings::{unhex, Setting};

/// Kinds of form that can sit in an inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Weapon,
    Armor,
    Ammo,
    Aid,
    Book,
    Key,
    Misc,
    Note,
    Light,
    ItemMod,
    CasinoChip,
    CaravanCard,
    CaravanMoney,
}

impl ItemKind {
    /// Record types whose forms are inventory items. LIGH counts only when its
    /// "Can be Carried" flag is set. [documented: xEdit wbDefinitionsFNV.pas lists
    /// these record types as valid CNTO items; LIGH flag name from the same file]
    pub fn from_sig(sig: &[u8; 4]) -> Option<ItemKind> {
        Some(match sig {
            b"WEAP" => ItemKind::Weapon,
            b"ARMO" => ItemKind::Armor,
            b"AMMO" => ItemKind::Ammo,
            b"ALCH" => ItemKind::Aid,
            b"BOOK" => ItemKind::Book,
            b"KEYM" => ItemKind::Key,
            b"MISC" => ItemKind::Misc,
            b"NOTE" => ItemKind::Note,
            b"LIGH" => ItemKind::Light,
            b"IMOD" => ItemKind::ItemMod,
            b"CHIP" => ItemKind::CasinoChip,
            b"CCRD" => ItemKind::CaravanCard,
            b"CMNY" => ItemKind::CaravanMoney,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub kind: ItemKind,
    /// `None` when the schema has no layout for this record type's value yet.
    pub value: Option<i32>,
    pub weight: Option<f32>,
    /// ARMO BMDT biped flags: the body slots the item occupies when worn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Entry {
    pub item: FormId,
    pub count: i32,
    /// COED that follows the CNTO: condition and owner of these items.
    #[serde(skip_serializing_if = "Extra::is_plain")]
    pub extra: Extra,
}

impl Entry {
    pub fn new(item: FormId, count: i32) -> Entry {
        Entry { item, count, extra: Extra::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Container {
    pub respawns: bool,
    /// CNTO entries in record order; an entry may name a leveled item list.
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActorBase {
    /// Items the actor carries (CNTO), like a container's.
    pub entries: Vec<Entry>,
    /// NPC_ DATA attributes (S.P.E.C.I.A.L., in that order); `None` for creatures.
    pub attributes: Option<[u8; 7]>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Door {
    pub automatic: bool,
    pub minimal_use: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LeveledEntry {
    pub level: u16,
    pub item: FormId,
    pub count: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeveledList {
    pub chance_none: u8,
    /// Flag "Calculate from all levels <= player's level".
    pub all_levels: bool,
    /// Flag "Calculate for each item in count".
    pub each: bool,
    /// Flag "Use All".
    pub use_all: bool,
    /// LVLG: a global whose value replaces `chance_none`. [hypothesis]
    pub global: Option<FormId>,
    pub entries: Vec<LeveledEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseData {
    Item(Item),
    Container(Container),
    Door(Door),
    Activator,
    Furniture,
    Actor(ActorBase),
    LeveledItem(LeveledList),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaseForm {
    pub id: FormId,
    #[serde(serialize_with = "ser_sig")]
    pub sig: [u8; 4],
    pub edid: Option<String>,
    pub name: Option<String>,
    pub script: Option<FormId>,
    #[serde(flatten)]
    pub data: BaseData,
}

pub(crate) fn ser_sig<S: serde::Serializer>(sig: &[u8; 4], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&String::from_utf8_lossy(sig))
}

/// Other record types the simulation reads: globals and game settings.
pub const OTHER_SIGS: &[&[u8; 4]] = &[b"GLOB", b"GMST"];

/// Record types interpreted as base forms.
pub const BASE_SIGS: &[&[u8; 4]] = &[
    b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"KEYM", b"MISC", b"NOTE", b"LIGH", b"IMOD", b"CHIP", b"CCRD",
    b"CMNY", b"CONT", b"DOOR", b"ACTI", b"TACT", b"FURN", b"NPC_", b"CREA", b"LVLI",
];

#[derive(Default)]
pub struct FormStore {
    pub bases: BTreeMap<FormId, BaseForm>,
    /// Signature of the winning record of every global FormID.
    pub sigs: HashMap<FormId, [u8; 4]>,
    /// Lower-case Editor ID -> FormID, for the base forms and globals.
    pub edids: HashMap<String, FormId>,
    /// GLOB values as authored.
    pub globals: HashMap<FormId, f32>,
    /// GMST values by lower-case Editor ID.
    pub settings: HashMap<String, Setting>,
    pub issues: Issues,
}

impl FormStore {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> FormStore {
        let mut store = FormStore::default();
        let mut wanted: Vec<(FormId, usize, &Record)> = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if r.header.flags & record_flags::DELETED != 0 {
                // Deleted references are counted by `RefTable`; report base forms only.
                if BASE_SIGS.contains(&&r.header.sig) || OTHER_SIGS.contains(&&r.header.sig) {
                    store.issues.note("winning record is deleted", || format!("0x{g:08X} {}", r.sig_str()));
                }
                continue;
            }
            store.sigs.insert(FormId(g), r.header.sig);
            if BASE_SIGS.contains(&&r.header.sig) || OTHER_SIGS.contains(&&r.header.sig) {
                wanted.push((FormId(g), d.plugin as usize, r));
            }
        }
        wanted.sort_by_key(|w| w.0);
        let loaded: Vec<(FormId, Loaded, Issues)> = wanted
            .par_iter()
            .map(|&(id, pi, r)| {
                let mut issues = Issues::default();
                let l = load_one(lo, schema, id, pi, r, &mut issues);
                (id, l, issues)
            })
            .collect();
        for (id, l, issues) in loaded {
            store.issues.merge(issues);
            match l {
                Loaded::Base(b) => {
                    if let Some(e) = &b.edid {
                        store.edids.insert(e.to_ascii_lowercase(), id);
                    }
                    store.bases.insert(id, *b);
                }
                Loaded::Global(edid, v) => {
                    if let Some(e) = edid {
                        store.edids.insert(e.to_ascii_lowercase(), id);
                    }
                    store.globals.insert(id, v);
                }
                Loaded::Setting(edid, v) => {
                    if let Setting::Raw(_) = v {
                        store.issues.note("GMST with unknown prefix or size", || edid.clone());
                    }
                    store.settings.insert(edid.to_ascii_lowercase(), v);
                }
                Loaded::Skip => {}
            }
        }
        store
    }

    pub fn get(&self, id: FormId) -> Option<&BaseForm> {
        self.bases.get(&id)
    }

    pub fn item(&self, id: FormId) -> Option<&Item> {
        match &self.bases.get(&id)?.data {
            BaseData::Item(i) => Some(i),
            _ => None,
        }
    }

    pub fn leveled(&self, id: FormId) -> Option<&LeveledList> {
        match &self.bases.get(&id)?.data {
            BaseData::LeveledItem(l) => Some(l),
            _ => None,
        }
    }

    /// Carried-item entries of a container or actor base.
    pub fn entries(&self, id: FormId) -> Option<&[Entry]> {
        match &self.bases.get(&id)?.data {
            BaseData::Container(c) => Some(&c.entries),
            BaseData::Actor(a) => Some(&a.entries),
            _ => None,
        }
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }

    /// A numeric game setting by Editor ID.
    pub fn setting_f32(&self, edid: &str) -> Option<f32> {
        self.settings.get(&edid.to_ascii_lowercase())?.as_f32()
    }

    /// A global's authored value by Editor ID.
    pub fn global_by_edid(&self, edid: &str) -> Option<(FormId, f32)> {
        let id = self.by_edid(edid)?;
        Some((id, *self.globals.get(&id)?))
    }

    pub fn sig(&self, id: FormId) -> Option<[u8; 4]> {
        self.sigs.get(&id).copied()
    }
}

enum Loaded {
    Base(Box<BaseForm>),
    Global(Option<String>, f32),
    Setting(String, Setting),
    Skip,
}

fn load_one(lo: &LoadOrder, schema: &Schema, id: FormId, pi: usize, r: &Record, issues: &mut Issues) -> Loaded {
    let sig = r.header.sig;
    let sig_s = String::from_utf8_lossy(&sig).into_owned();
    let d = match extract::decode(r, schema) {
        Ok(d) => d,
        Err(e) => {
            issues.note("record undecodable", || format!("{id} {sig_s}: {e}"));
            return Loaded::Skip;
        }
    };
    let edid = first(&d, b"EDID").and_then(text);
    if &sig == b"GMST" {
        let Some(edid) = edid else {
            issues.note("GMST without EDID", || format!("{id}"));
            return Loaded::Skip;
        };
        return match first(&d, b"DATA").and_then(Value::as_str).and_then(unhex) {
            Some(bytes) => Loaded::Setting(edid.clone(), Setting::from_bytes(&edid, &bytes)),
            None => {
                issues.note("GMST without DATA", || edid.clone());
                Loaded::Skip
            }
        };
    }
    if &sig == b"GLOB" {
        return match first(&d, b"FLTV").and_then(Value::as_f64) {
            Some(v) => Loaded::Global(edid, v as f32),
            None => {
                issues.note("GLOB without FLTV value", || format!("{id}"));
                Loaded::Skip
            }
        };
    }
    let mut fid = |v: Option<&Value>, what: &str| -> Option<FormId> {
        let local = local_fid(v?)?;
        match global(lo, pi, local) {
            Ok(g) => g,
            Err(l) => {
                issues.note("FormID outside master list", || format!("{id} {sig_s} {what} 0x{l:08X}"));
                None
            }
        }
    };
    let name = first(&d, b"FULL").and_then(text);
    let script = fid(first(&d, b"SCRI"), "SCRI");
    let data = if let Some(kind) = ItemKind::from_sig(&sig) {
        match item(kind, &d) {
            Some(i) => {
                if i.value.is_none() || i.weight.is_none() {
                    issues.note(&format!("{sig_s}: no value/weight layout in the schema"), || format!("{id}"));
                }
                BaseData::Item(i)
            }
            None => return Loaded::Skip, // non-carriable light
        }
    } else {
        match &sig {
            b"CONT" => {
                let respawns = first(&d, b"DATA").and_then(|v| v.get("flags")).is_some_and(|f| has_flag(f, "Respawns"));
                BaseData::Container(Container { respawns, entries: entries(lo, pi, id, &d, issues) })
            }
            b"NPC_" | b"CREA" => BaseData::Actor(ActorBase {
                entries: entries(lo, pi, id, &d, issues),
                attributes: if &sig == b"NPC_" { attributes(&d) } else { None },
            }),
            b"DOOR" => {
                let f = first(&d, b"FNAM");
                BaseData::Door(Door {
                    automatic: f.is_some_and(|f| has_flag(f, "Automatic Door")),
                    minimal_use: f.is_some_and(|f| has_flag(f, "Minimal Use")),
                })
            }
            b"ACTI" | b"TACT" => BaseData::Activator,
            b"FURN" => BaseData::Furniture,
            b"LVLI" => BaseData::LeveledItem(leveled(lo, pi, id, &d, issues)),
            _ => return Loaded::Skip,
        }
    };
    Loaded::Base(Box::new(BaseForm { id, sig, edid, name, script, data }))
}

fn item(kind: ItemKind, d: &[Decoded]) -> Option<Item> {
    let data = first(d, b"DATA");
    let (value, weight) = match kind {
        ItemKind::Weapon | ItemKind::Armor | ItemKind::Book | ItemKind::Key | ItemKind::Misc => {
            (data.and_then(|v| int(v, "value")), data.and_then(|v| num(v, "weight")))
        }
        ItemKind::Ammo => (data.and_then(|v| int(v, "value")), first(d, b"DAT2").and_then(|v| num(v, "weight"))),
        ItemKind::Aid => (first(d, b"ENIT").and_then(|v| int(v, "value")), data.and_then(Value::as_f64)),
        ItemKind::Light => {
            let data = data?;
            if !data.get("flags").is_some_and(|f| has_flag(f, "Can be Carried")) {
                return None;
            }
            (int(data, "value"), num(data, "weight"))
        }
        // No value/weight layout in the schema yet (NOTE has none at all; IMOD, CHIP,
        // CCRD, CMNY have no record layout). Reported by the caller.
        _ => (None, None),
    };
    let slots = match kind {
        ItemKind::Armor => first(d, b"BMDT").and_then(|v| v.get("biped_flags")).and_then(|f| int(f, "value")),
        _ => None,
    };
    Some(Item { kind, value: value.map(|v| v as i32), weight: weight.map(|w| w as f32), slots: slots.map(|s| s as u32) })
}

/// S.P.E.C.I.A.L. names in NPC_ DATA order.
pub const SPECIAL: [&str; 7] = ["strength", "perception", "endurance", "charisma", "intelligence", "agility", "luck"];

fn attributes(d: &[Decoded]) -> Option<[u8; 7]> {
    let a = first(d, b"DATA")?.get("attributes")?;
    let mut out = [0u8; 7];
    for (o, name) in out.iter_mut().zip(SPECIAL) {
        *o = int(a, name)? as u8;
    }
    Some(out)
}

fn entries(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Vec<Entry> {
    // COED belongs to the CNTO right before it [documented: xEdit wbCNTO/wbCOED
    // struct; observed: all 1,661 COED in the masters directly follow a CNTO].
    let mut out = Vec::new();
    let mut last_kept = false;
    for x in d {
        let Some(v) = x.value.as_ref() else {
            if &x.sig == b"CNTO" || &x.sig == b"COED" {
                issues.note("CNTO/COED undecodable", || format!("{id}"));
            }
            last_kept = false;
            continue;
        };
        match &x.sig {
            b"CNTO" => {
                last_kept = false;
                let (Some(local), Some(count)) = (v.get("item").and_then(local_fid), int(v, "count")) else {
                    issues.note("CNTO without item/count", || format!("{id}"));
                    continue;
                };
                match global(lo, pi, local) {
                    Ok(Some(item)) => {
                        out.push(Entry::new(item, count as i32));
                        last_kept = true;
                    }
                    Ok(None) => issues.note("CNTO with null item", || format!("{id}")),
                    Err(l) => issues.note("FormID outside master list", || format!("{id} CNTO 0x{l:08X}")),
                }
            }
            b"COED" if last_kept => {
                let e = &mut out.last_mut().expect("last_kept").extra;
                e.condition = num(v, "item_condition").map(|c| c as f32);
                e.owner = match v.get("owner").and_then(local_fid).map(|l| global(lo, pi, l)) {
                    Some(Ok(g)) => g,
                    Some(Err(l)) => {
                        issues.note("FormID outside master list", || format!("{id} COED 0x{l:08X}"));
                        None
                    }
                    None => None,
                };
            }
            b"COED" => issues.note("COED not after a kept CNTO", || format!("{id}")),
            _ => last_kept = false,
        }
    }
    out
}

fn leveled(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> LeveledList {
    let flags = first(d, b"LVLF");
    let flag = |n: &str| flags.is_some_and(|f| has_flag(f, n));
    let mut out = LeveledList {
        chance_none: first(d, b"LVLD").and_then(Value::as_u64).unwrap_or(0).min(255) as u8,
        all_levels: flag("Calculate from all levels <= player's level"),
        each: flag("Calculate for each item in count"),
        use_all: flag("Use All"),
        global: None,
        entries: Vec::new(),
    };
    if let Some(l) = first(d, b"LVLG").and_then(local_fid) {
        match global(lo, pi, l) {
            Ok(g) => out.global = g,
            Err(l) => issues.note("FormID outside master list", || format!("{id} LVLG 0x{l:08X}")),
        }
    }
    for v in extract::all(d, b"LVLO") {
        let (Some(level), Some(local)) = (int(v, "level"), v.get("reference").and_then(local_fid)) else {
            issues.note("LVLO without level/reference", || format!("{id}"));
            continue;
        };
        // The count field is optional in the layout; a missing count means one.
        // [hypothesis: xEdit marks it optional (SetOptionalFrom(3)); every LVLO in the
        // shipped masters is checked by `fnvtool sim forms`]
        let count = match int(v, "count") {
            Some(c) => c as u16,
            None => {
                issues.note("LVLO without count (defaulted to 1)", || format!("{id}"));
                1
            }
        };
        match global(lo, pi, local) {
            Ok(Some(item)) => out.entries.push(LeveledEntry { level: level as u16, item, count }),
            Ok(None) => issues.note("LVLO with null reference", || format!("{id}")),
            Err(l) => issues.note("FormID outside master list", || format!("{id} LVLO 0x{l:08X}")),
        }
    }
    out
}
