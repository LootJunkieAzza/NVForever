//! The records that decide what an actor looks like, read through the Layer 1 schema:
//! NPC_, RACE, HAIR, EYES, HDPT, ARMO (worn models and protection), LVLN, and WEAP (the
//! model held and the animation set it selects).
//!
//! Field meanings: docs/spec-notes/actors.md §1 (layouts from xEdit's public
//! definitions, `schemas/fnv/gameplay.json` and `objects.json`).

use std::collections::{BTreeMap, HashMap};

use fnv_esm::{LoadOrder, Record, Schema};
use fnv_sim::extract::{self, all, first, global, has_flag, int, local_fid, num, text, Decoded, Issues};
use fnv_sim::FormId;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

/// NPC_ template flags (ACBS) [documented: xEdit wbTemplateFlags, FO3/FNV branch].
pub mod template {
    pub const TRAITS: u16 = 1 << 0;
    pub const INVENTORY: u16 = 1 << 8;
    pub const MODEL_ANIMATION: u16 = 1 << 6;
}

/// The three FaceGen coefficient arrays: symmetric and asymmetric geometry, symmetric
/// texture (FGGS 50, FGGA 30, FGTS 50 floats) [documented: xEdit wbFaceGen]. Stored f32
/// values widened to f64, so reports print them exactly.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FaceGen {
    pub fggs: Vec<f64>,
    pub fgga: Vec<f64>,
    pub fgts: Vec<f64>,
}

impl FaceGen {
    fn from_decoded(d: &[Decoded]) -> Option<FaceGen> {
        let floats = |sig: &[u8; 4]| -> Option<Vec<f64>> {
            Some(first(d, sig)?.as_array()?.iter().filter_map(Value::as_f64).collect())
        };
        let (fggs, fgga, fgts) = (floats(b"FGGS"), floats(b"FGGA"), floats(b"FGTS"));
        if fggs.is_none() && fgga.is_none() && fgts.is_none() {
            return None;
        }
        Some(FaceGen { fggs: fggs.unwrap_or_default(), fgga: fgga.unwrap_or_default(), fgts: fgts.unwrap_or_default() })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Npc {
    pub id: FormId,
    pub edid: Option<String>,
    pub name: Option<String>,
    pub female: bool,
    pub template: Option<FormId>,
    pub template_flags: u16,
    /// MODL: the skeleton, relative to `meshes\` [observed: `Characters\_Male\Skeleton.NIF`].
    pub skeleton: Option<String>,
    pub race: Option<FormId>,
    pub hair: Option<FormId>,
    pub hair_length: Option<f64>,
    pub hair_color: Option<[u8; 3]>,
    pub eyes: Option<FormId>,
    pub head_parts: Vec<FormId>,
    pub facegen: Option<FaceGen>,
    /// NAM6 "Height" [documented: xEdit]; meaning in game to be captured.
    pub height: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RacePart {
    /// INDX: head parts 0 Head, 1 Ears, 2 Mouth, 3 Teeth Lower, 4 Teeth Upper, 5 Tongue,
    /// 6 Left Eye, 7 Right Eye; body parts 0 Upper Body, 1 Left Hand, 2 Right Hand,
    /// 3 Upper Body Texture [documented: xEdit wbHeadPartIndexEnum, wbBodyPartIndexEnum].
    pub index: u32,
    pub model: Option<String>,
    pub icon: Option<String>,
}

/// Per sex: index 0 male, 1 female.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Race {
    pub id: FormId,
    pub edid: Option<String>,
    /// DATA male/female height.
    pub height: [f64; 2],
    pub head: [Vec<RacePart>; 2],
    pub body: [Vec<RacePart>; 2],
    pub default_hair: [Option<FormId>; 2],
    pub facegen: [Option<FaceGen>; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hair {
    pub model: Option<String>,
    /// ICON, named "Texture" by xEdit.
    pub texture: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Eyes {
    pub texture: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeadPart {
    pub model: Option<String>,
    pub extra: Vec<FormId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Armor {
    pub edid: Option<String>,
    /// BMDT biped flags.
    pub slots: u32,
    /// Worn (biped) models: MODL male, MOD3 female.
    pub worn: [Option<String>; 2],
    pub dt: f32,
    /// DNAM DR as stored (xEdit shows it divided by 100).
    pub dr_raw: i32,
    pub power_armor: bool,
}

/// What a weapon looks like in an actor's hands.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Weapon {
    pub edid: Option<String>,
    /// MODL: the third-person model, relative to `meshes\`.
    pub model: Option<String>,
    /// DNAM "Animation Type" (0–13) [documented: xEdit wbWeaponAnimTypeEnum]; the clip
    /// prefix is `locomotion::weapon_prefix`.
    pub animation_type: Option<u32>,
    /// WNAM "1st Person Model": a form (a STAT [observed: e.g. the 9mm pistol's]) whose
    /// model the first-person view shows [documented: xEdit WEAP WNAM].
    pub first_person: Option<FormId>,
    /// That form's MODL, relative to `meshes\`.
    pub first_person_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeveledNpc {
    pub chance_none: u8,
    pub all_levels: bool,
    /// (level, actor base or another LVLN)
    pub entries: Vec<(u16, FormId)>,
}

#[derive(Default)]
pub struct ActorForms {
    pub npcs: BTreeMap<FormId, Npc>,
    pub races: HashMap<FormId, Race>,
    pub hairs: HashMap<FormId, Hair>,
    pub eyes: HashMap<FormId, Eyes>,
    pub head_parts: HashMap<FormId, HeadPart>,
    pub armors: HashMap<FormId, Armor>,
    pub leveled: HashMap<FormId, LeveledNpc>,
    pub weapons: HashMap<FormId, Weapon>,
    /// Signature of the winning record of every actor-related form, for reports.
    pub sigs: HashMap<FormId, [u8; 4]>,
    pub edids: HashMap<String, FormId>,
    pub issues: Issues,
}

const SIGS: &[&[u8; 4]] = &[b"NPC_", b"RACE", b"HAIR", b"EYES", b"HDPT", b"ARMO", b"LVLN", b"CREA", b"LVLC", b"WEAP"];

enum Loaded {
    Npc(Box<Npc>),
    Race(Box<Race>),
    Hair(Hair),
    Eyes(Eyes),
    HeadPart(HeadPart),
    Armor(Armor),
    Leveled(LeveledNpc),
    Weapon(Weapon),
    Other,
}

impl ActorForms {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> ActorForms {
        let mut out = ActorForms::default();
        let mut wanted: Vec<(FormId, usize, &Record)> = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if !SIGS.contains(&&r.header.sig) || r.header.flags & 0x20 != 0 {
                continue;
            }
            wanted.push((FormId(g), d.plugin as usize, r));
        }
        wanted.sort_by_key(|w| w.0);
        let loaded: Vec<_> = wanted
            .par_iter()
            .map(|&(id, pi, r)| {
                let mut issues = Issues::default();
                let res = match extract::decode(r, schema) {
                    Ok(d) => {
                        let edid = first(&d, b"EDID").and_then(text);
                        (edid, load_one(lo, pi, id, &r.header.sig, &d, &mut issues))
                    }
                    Err(e) => {
                        issues.note("record does not decode", || format!("{id}: {e}"));
                        (None, Loaded::Other)
                    }
                };
                (id, r.header.sig, res, issues)
            })
            .collect();
        for (id, sig, (edid, l), issues) in loaded {
            out.issues.merge(issues);
            out.sigs.insert(id, sig);
            if let Some(e) = edid {
                out.edids.insert(e.to_ascii_lowercase(), id);
            }
            match l {
                Loaded::Npc(n) => {
                    out.npcs.insert(id, *n);
                }
                Loaded::Race(r) => {
                    out.races.insert(id, *r);
                }
                Loaded::Hair(h) => {
                    out.hairs.insert(id, h);
                }
                Loaded::Eyes(e) => {
                    out.eyes.insert(id, e);
                }
                Loaded::HeadPart(h) => {
                    out.head_parts.insert(id, h);
                }
                Loaded::Armor(a) => {
                    out.armors.insert(id, a);
                }
                Loaded::Leveled(l) => {
                    out.leveled.insert(id, l);
                }
                Loaded::Weapon(w) => {
                    out.weapons.insert(id, w);
                }
                Loaded::Other => {}
            }
        }
        // The forms weapons name as their first-person model (WNAM), read for their MODL
        // only: they are statics, too many to load as a type.
        for w in out.weapons.values_mut() {
            let Some(fp) = w.first_person else { continue };
            let Some(d) = lo.index.get(&fp.0).and_then(|defs| defs.last()) else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if r.header.flags & 0x20 != 0 {
                continue;
            }
            if let Ok(dec) = extract::decode(r, schema) {
                w.first_person_model = path(first(&dec, b"MODL"));
            }
        }
        out
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }
}

fn fid(lo: &LoadOrder, pi: usize, id: FormId, sig: &str, v: Option<&Value>, issues: &mut Issues) -> Option<FormId> {
    let local = local_fid(v?)?;
    match global(lo, pi, local) {
        Ok(g) => g,
        Err(l) => {
            issues.note("FormID outside master list", || format!("{id} {sig} 0x{l:08X}"));
            None
        }
    }
}

fn path(v: Option<&Value>) -> Option<String> {
    v.and_then(text).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn load_one(lo: &LoadOrder, pi: usize, id: FormId, sig: &[u8; 4], d: &[Decoded], issues: &mut Issues) -> Loaded {
    match sig {
        b"NPC_" => Loaded::Npc(Box::new(npc(lo, pi, id, d, issues))),
        b"RACE" => Loaded::Race(Box::new(race(lo, pi, id, d, issues))),
        b"HAIR" => Loaded::Hair(Hair { model: path(first(d, b"MODL")), texture: path(first(d, b"ICON")) }),
        b"EYES" => Loaded::Eyes(Eyes { texture: path(first(d, b"ICON")) }),
        b"HDPT" => Loaded::HeadPart(HeadPart {
            model: path(first(d, b"MODL")),
            extra: all(d, b"HNAM").filter_map(|v| fid(lo, pi, id, "HNAM", Some(v), issues)).collect(),
        }),
        b"ARMO" => Loaded::Armor(armor(d)),
        b"LVLN" => Loaded::Leveled(leveled_npc(lo, pi, id, d, issues)),
        b"WEAP" => Loaded::Weapon(Weapon {
            edid: first(d, b"EDID").and_then(text),
            model: path(first(d, b"MODL")),
            animation_type: first(d, b"DNAM").and_then(|v| weapon_animation_type(v.get("animation_type")?)),
            first_person: fid(lo, pi, id, "WNAM", first(d, b"WNAM"), issues),
            first_person_model: None,
        }),
        _ => Loaded::Other,
    }
}

fn npc(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Npc {
    let acbs = first(d, b"ACBS");
    let female = acbs.and_then(|a| a.get("flags")).is_some_and(|f| has_flag(f, "Female"));
    let template_flags = acbs.and_then(|a| a.get("template_flags")).and_then(|f| num(f, "value")).unwrap_or(0.0) as u16;
    let hair_color = first(d, b"HCLR").and_then(|c| Some([int(c, "red")? as u8, int(c, "green")? as u8, int(c, "blue")? as u8]));
    Npc {
        id,
        edid: first(d, b"EDID").and_then(text),
        name: first(d, b"FULL").and_then(text),
        female,
        template: fid(lo, pi, id, "TPLT", first(d, b"TPLT"), issues),
        template_flags,
        skeleton: path(first(d, b"MODL")),
        race: fid(lo, pi, id, "RNAM", first(d, b"RNAM"), issues),
        hair: fid(lo, pi, id, "HNAM", first(d, b"HNAM"), issues),
        hair_length: first(d, b"LNAM").and_then(Value::as_f64),
        hair_color,
        eyes: fid(lo, pi, id, "ENAM", first(d, b"ENAM"), issues),
        head_parts: all(d, b"PNAM").filter_map(|v| fid(lo, pi, id, "PNAM", Some(v), issues)).collect(),
        facegen: FaceGen::from_decoded(d),
        height: first(d, b"NAM6").and_then(Value::as_f64),
    }
}

/// RACE subrecords are positional (docs/spec-notes/actors.md §1.1): `NAM0` opens the head
/// data and `NAM1` the body data, each with an `MNAM` (male) and an `FNAM` (female) list
/// of `INDX` + `MODL`/`ICON` parts; after the body data's `HNAM`/`ENAM` arrays, `MNAM`
/// and `FNAM` open the male and female FaceGen data.
fn race(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Race {
    #[derive(PartialEq)]
    enum Section {
        Start,
        Head,
        Body,
        FaceGen,
    }
    let mut out = Race {
        id,
        edid: first(d, b"EDID").and_then(text),
        height: [1.0, 1.0],
        head: Default::default(),
        body: Default::default(),
        default_hair: [None, None],
        facegen: [None, None],
    };
    if let Some(data) = first(d, b"DATA") {
        out.height = [
            num(data, "male_height").unwrap_or(1.0),
            num(data, "female_height").unwrap_or(1.0),
        ];
    }
    if let Some(h) = first(d, b"DNAM") {
        out.default_hair = [
            fid(lo, pi, id, "DNAM", h.get("male"), issues),
            fid(lo, pi, id, "DNAM", h.get("female"), issues),
        ];
    }
    let mut section = Section::Start;
    let mut sex = 0usize;
    let mut fg_start: [Option<usize>; 2] = [None, None];
    for (i, s) in d.iter().enumerate() {
        match &s.sig {
            b"NAM0" => section = Section::Head,
            b"NAM1" => section = Section::Body,
            b"HNAM" | b"ENAM" if section == Section::Body => section = Section::FaceGen,
            b"MNAM" | b"FNAM" => {
                sex = usize::from(&s.sig == b"FNAM");
                if section == Section::FaceGen {
                    fg_start[sex] = Some(i + 1);
                }
            }
            b"INDX" => {
                let Some(index) = s.value.as_ref().and_then(Value::as_u64) else {
                    issues.note("RACE INDX does not decode", || format!("{id}"));
                    continue;
                };
                let part = RacePart { index: index as u32, ..RacePart::default() };
                match section {
                    Section::Head => out.head[sex].push(part),
                    Section::Body => out.body[sex].push(part),
                    _ => issues.note("RACE INDX outside head/body data", || format!("{id}")),
                }
            }
            b"MODL" | b"ICON" => {
                let list = match section {
                    Section::Head => &mut out.head[sex],
                    Section::Body => &mut out.body[sex],
                    _ => continue,
                };
                let Some(part) = list.last_mut() else { continue };
                let v = path(s.value.as_ref());
                if &s.sig == b"MODL" {
                    part.model = v;
                } else {
                    part.icon = v;
                }
            }
            _ => {}
        }
    }
    for (sex, start) in fg_start.iter().enumerate() {
        if let Some(start) = *start {
            let end = d[start..].iter().position(|s| &s.sig == b"MNAM" || &s.sig == b"FNAM").map_or(d.len(), |p| start + p);
            out.facegen[sex] = FaceGen::from_decoded(&d[start..end]);
        }
    }
    out
}

fn armor(d: &[Decoded]) -> Armor {
    let bmdt = first(d, b"BMDT");
    let slots = bmdt.and_then(|b| b.get("biped_flags")).and_then(|f| num(f, "value")).unwrap_or(0.0) as u32;
    let power_armor = bmdt.and_then(|b| b.get("general_flags")).is_some_and(|f| has_flag(f, "Power Armor"));
    let dnam = first(d, b"DNAM");
    Armor {
        edid: first(d, b"EDID").and_then(text),
        slots,
        worn: [path(first(d, b"MODL")), path(first(d, b"MOD3"))],
        dt: dnam.and_then(|v| num(v, "dt")).unwrap_or(0.0) as f32,
        dr_raw: dnam.and_then(|v| int(v, "dr")).unwrap_or(0) as i32,
        power_armor,
    }
}

/// WEAP DNAM "Animation Type" names in value order [documented: xEdit
/// wbWeaponAnimTypeEnum, as in `schemas/fnv/objects.json`].
const WEAPON_ANIMATION_TYPES: [&str; 14] = [
    "Hand to Hand",
    "Melee (1 Hand)",
    "Melee (2 Hand)",
    "Pistol - Ballistic (1 Hand)",
    "Pistol - Energy (1 Hand)",
    "Rifle - Ballistic (2 Hand)",
    "Rifle - Automatic (2 Hand)",
    "Rifle - Energy (2 Hand)",
    "Handle (2 Hand)",
    "Launcher (2 Hand)",
    "Grenade Throw (1 Hand)",
    "Land Mine (1 Hand)",
    "Mine Drop (1 Hand)",
    "Thrown (1 Hand)",
];

/// The decoded animation type: its name (a value the schema names) or the raw number.
fn weapon_animation_type(v: &Value) -> Option<u32> {
    match v {
        Value::String(n) => WEAPON_ANIMATION_TYPES.iter().position(|t| t == n).map(|i| i as u32),
        _ => v.as_u64().map(|n| n as u32),
    }
}

fn leveled_npc(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> LeveledNpc {
    let flags = first(d, b"LVLF");
    LeveledNpc {
        chance_none: first(d, b"LVLD").and_then(Value::as_u64).unwrap_or(0).min(255) as u8,
        all_levels: flags.is_some_and(|f| has_flag(f, "Calculate from all levels <= player's level")),
        entries: all(d, b"LVLO")
            .filter_map(|v| Some((int(v, "level")? as u16, fid(lo, pi, id, "LVLO", v.get("reference"), issues)?)))
            .collect(),
    }
}
