//! The static data scripts run against: compiled scripts (decoded once), quests from
//! `QUST`, dialogue and package result scripts, messages, form lists, actor bases, and
//! Editor IDs. Built once per load order from the winning record of every form; field
//! meanings come from the Layer 1 schema (`schemas/fnv`), never from byte offsets here.

use crate::cond::{self, Condition, ParamTypes};
use crate::quest::{Objective, QuestDef, StageEntry, Target};
use fnv_esm::decode::{decode_sub, DecodeCtx};
use fnv_esm::{LoadOrder, Record, Schema};
use fnv_script::unit::{read_record, ReadIssues, ScriptUnit, UnitOwner, VarType};
use fnv_script::{decode, FunctionTable, Program};
use fnv_sim::extract::Issues;
use fnv_sim::{FormId, RefTable};
use rayon::prelude::*;
use serde_json::Value as Json;
use std::collections::{BTreeMap, HashMap};

/// A unit with its decoded program.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub unit: ScriptUnit,
    pub program: Program,
    /// Undecodable pieces (none in the shipped masters).
    pub raw: usize,
}

impl Compiled {
    pub fn key(&self) -> String {
        self.unit.key()
    }

    pub fn has_block(&self, event: u16) -> bool {
        self.program.blocks.iter().any(|b| b.event == event)
    }

    /// Declared type of a local variable.
    pub fn var_type(&self, index: u16) -> Option<&VarType> {
        self.unit.var(index as u32).map(|v| &v.ty)
    }

    /// Index of a local variable by name (case-insensitive).
    pub fn var_index(&self, name: &str) -> Option<u16> {
        self.unit.variables.iter().find(|v| v.name.eq_ignore_ascii_case(name)).map(|v| v.index as u16)
    }

    pub fn var_name(&self, index: u16) -> Option<&str> {
        self.unit.var(index as u32).map(|v| v.name.as_str())
    }
}

/// Index into [`GameData::units`].
pub type UnitId = usize;

#[derive(Debug, Clone, Default)]
pub struct InfoDef {
    pub quest: Option<FormId>,
    /// ANAM: the speaker the line is restricted to.
    pub speaker: Option<FormId>,
    pub conditions: Vec<Condition>,
    /// Result script before `NEXT` (runs at the start of the line) and after it (at the
    /// end) [documented: xEdit "Script (Begin)"/"Script (End)"].
    pub begin: Option<UnitId>,
    pub end: Option<UnitId>,
}

#[derive(Debug, Clone, Default)]
pub struct MessageDef {
    pub text: Option<String>,
    pub buttons: Vec<String>,
    pub message_box: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ActorDef {
    pub sig: [u8; 4],
    /// SNAM entries: faction and rank.
    pub factions: Vec<(FormId, i8)>,
    pub voice: Option<FormId>,
    pub female: bool,
    pub essential: bool,
    pub race: Option<FormId>,
    pub class: Option<FormId>,
}

pub struct GameData {
    pub table: FunctionTable,
    pub param_types: ParamTypes,
    pub load_order: Vec<String>,
    pub units: Vec<Compiled>,
    /// SCPT FormID → unit.
    pub scripts: HashMap<FormId, UnitId>,
    pub quests: BTreeMap<FormId, QuestDef>,
    pub infos: HashMap<FormId, InfoDef>,
    /// PACK begin / end / change result scripts.
    pub packages: HashMap<FormId, [Option<UnitId>; 3]>,
    pub messages: HashMap<FormId, MessageDef>,
    pub form_lists: HashMap<FormId, Vec<FormId>>,
    pub actors: HashMap<FormId, ActorDef>,
    /// SCRI of every winning record that has one (base forms, quests).
    pub form_script: HashMap<FormId, FormId>,
    /// Lower-case Editor ID → FormID (every winning record with an EDID).
    pub edids: HashMap<String, FormId>,
    pub edid_of: HashMap<FormId, String>,
    pub sigs: HashMap<FormId, [u8; 4]>,
    /// Placed references whose base runs a script, by cell, in FormID order.
    pub scripted_by_cell: BTreeMap<FormId, Vec<FormId>>,
    pub issues: Issues,
    pub read_issues: ReadIssues,
}

fn hexid(v: &Json) -> Option<u32> {
    let s = v.as_str()?;
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

fn has_flag(flags: Option<&Json>, name: &str) -> bool {
    flags
        .and_then(|f| f.get("set"))
        .and_then(Json::as_array)
        .is_some_and(|a| a.iter().any(|n| n.as_str() == Some(name)))
}

fn flag_bits(flags: Option<&Json>) -> u32 {
    flags.and_then(|f| f.get("value")).and_then(Json::as_u64).unwrap_or(0) as u32
}

/// Everything read from one winning record.
#[derive(Default)]
struct Read {
    edid: Option<String>,
    script: Option<FormId>,
    quest: Option<QuestDef>,
    info: Option<InfoDef>,
    message: Option<MessageDef>,
    form_list: Option<Vec<FormId>>,
    actor: Option<ActorDef>,
    units: Vec<ScriptUnit>,
    issues: Issues,
    read_issues: ReadIssues,
}

const UNIT_SIGS: &[&[u8; 4]] = &[b"SCPT", b"QUST", b"INFO", b"PACK"];

impl GameData {
    /// Build from a load order. `functions` is the parsed `functions.json` (for the
    /// condition parameter types); `table` the function table built from it.
    pub fn build(lo: &LoadOrder, schema: &Schema, table: FunctionTable, functions: &Json, refs: &RefTable) -> GameData {
        let param_types = ParamTypes::from_catalogue(functions);
        let winners: Vec<(u32, usize, &Record)> = lo
            .index
            .iter()
            .filter_map(|(&g, defs)| {
                let d = defs.last()?;
                Some((g, d.plugin as usize, lo.records[d.plugin as usize][d.ordinal as usize]))
            })
            .collect();
        let reads: Vec<(FormId, [u8; 4], Read)> = winners
            .par_iter()
            .map(|&(g, pi, r)| (FormId(g), r.header.sig, read_one(lo, schema, &param_types, g, pi, r)))
            .collect();
        let mut d = GameData {
            table,
            param_types,
            load_order: lo.plugins.iter().map(|p| p.name.clone()).collect(),
            units: Vec::new(),
            scripts: HashMap::new(),
            quests: BTreeMap::new(),
            infos: HashMap::new(),
            packages: HashMap::new(),
            messages: HashMap::new(),
            form_lists: HashMap::new(),
            actors: HashMap::new(),
            form_script: HashMap::new(),
            edids: HashMap::new(),
            edid_of: HashMap::new(),
            sigs: HashMap::new(),
            scripted_by_cell: BTreeMap::new(),
            issues: Issues::default(),
            read_issues: ReadIssues::default(),
        };
        // Decode every unit once.
        let mut all_units: Vec<(FormId, ScriptUnit)> = Vec::new();
        let mut reads = reads;
        reads.sort_by_key(|r| r.0);
        for (id, _, r) in &mut reads {
            for u in r.units.drain(..) {
                all_units.push((*id, u));
            }
        }
        let compiled: Vec<(FormId, Compiled)> = all_units
            .into_par_iter()
            .map(|(id, unit)| {
                let dec = decode(&unit.bytes, &d.table);
                let raw = dec.report.raw_total();
                (id, Compiled { unit, program: dec.program, raw })
            })
            .collect();
        let mut unit_of: HashMap<String, UnitId> = HashMap::new();
        for (_, c) in compiled {
            unit_of.insert(c.unit.owner.label(), d.units.len());
            d.units.push(c);
        }
        let label = |o: UnitOwner| o.label();
        for (id, sig, r) in reads {
            d.sigs.insert(id, sig);
            if let Some(e) = r.edid {
                d.edids.entry(e.to_ascii_lowercase()).or_insert(id);
                d.edid_of.insert(id, e);
            }
            if let Some(s) = r.script {
                d.form_script.insert(id, s);
            }
            d.issues.merge(r.issues);
            d.read_issues.merge(r.read_issues);
            match &sig {
                b"SCPT" => {
                    if let Some(&u) = unit_of.get(&label(UnitOwner::Script { form: id.0 })) {
                        d.scripts.insert(id, u);
                    }
                }
                b"QUST" => {
                    if let Some(mut q) = r.quest {
                        for (stage, entries) in q.stages.iter_mut() {
                            for (n, e) in entries.iter_mut().enumerate() {
                                e.unit = unit_of
                                    .get(&label(UnitOwner::QuestStage { form: id.0, stage: *stage as i64, entry: n as u32 }))
                                    .copied()
                                    .filter(|&u| d.units[u].unit.has_scda);
                            }
                        }
                        d.quests.insert(id, q);
                    }
                }
                b"INFO" => {
                    let mut info = r.info.unwrap_or_default();
                    let unit = |end| {
                        unit_of.get(&label(UnitOwner::Info { form: id.0, end })).copied().filter(|&u| d.units[u].unit.has_scda)
                    };
                    info.begin = unit(false);
                    info.end = unit(true);
                    d.infos.insert(id, info);
                }
                b"PACK" => {
                    use fnv_script::unit::PackageEvent as P;
                    let unit = |event| {
                        unit_of.get(&label(UnitOwner::Package { form: id.0, event })).copied().filter(|&u| d.units[u].unit.has_scda)
                    };
                    let p = [unit(P::Begin), unit(P::End), unit(P::Change)];
                    if p.iter().any(Option::is_some) {
                        d.packages.insert(id, p);
                    }
                }
                _ => {}
            }
            if let Some(m) = r.message {
                d.messages.insert(id, m);
            }
            if let Some(l) = r.form_list {
                d.form_lists.insert(id, l);
            }
            if let Some(a) = r.actor {
                d.actors.insert(id, a);
            }
        }
        for (id, p) in &refs.refs {
            if let (Some(cell), Some(_)) = (p.cell, d.script_of_base(p.base)) {
                d.scripted_by_cell.entry(cell).or_default().push(*id);
            }
        }
        d
    }

    pub fn unit(&self, id: UnitId) -> &Compiled {
        &self.units[id]
    }

    /// The SCPT a base form or quest runs, if it is a compiled script.
    pub fn script_of_base(&self, base: FormId) -> Option<FormId> {
        let s = *self.form_script.get(&base)?;
        self.scripts.contains_key(&s).then_some(s)
    }

    pub fn script(&self, script: FormId) -> Option<&Compiled> {
        self.scripts.get(&script).map(|&u| &self.units[u])
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }

    pub fn name(&self, id: FormId) -> String {
        match self.edid_of.get(&id) {
            Some(e) => e.clone(),
            None => id.to_string(),
        }
    }

    pub fn sig(&self, id: FormId) -> Option<[u8; 4]> {
        self.sigs.get(&id).copied()
    }

    pub fn is_quest(&self, id: FormId) -> bool {
        self.quests.contains_key(&id)
    }
}

fn read_one(lo: &LoadOrder, schema: &Schema, types: &ParamTypes, g: u32, pi: usize, r: &Record) -> Read {
    let mut out = Read::default();
    let sig = r.header.sig;
    let subs = match r.subrecords() {
        Ok(s) => s,
        Err(e) => {
            out.issues.note("record unreadable", || format!("0x{g:08X}: {e}"));
            return out;
        }
    };
    let map = |l: u32| lo.to_global(pi, l);
    let mut ctx = DecodeCtx { schema, on_formid: None };
    let mut val = |s: &fnv_esm::Subrecord| decode_sub(&sig, s, &mut ctx).value;
    let wanted = matches!(&sig, b"QUST" | b"INFO" | b"MESG" | b"FLST" | b"NPC_" | b"CREA");
    for s in &subs {
        match &s.sig {
            b"EDID" if out.edid.is_none() => out.edid = val(s).and_then(|v| v.as_str().map(str::to_string)),
            b"SCRI" if out.script.is_none() => out.script = val(s).as_ref().and_then(hexid).and_then(map).map(FormId),
            _ => {}
        }
    }
    if UNIT_SIGS.contains(&&sig) {
        let plugin = &lo.plugins[pi].name;
        let to_global = |l: u32| lo.to_global(pi, l);
        out.units = read_record(r, plugin, &to_global, schema, &mut out.read_issues);
    }
    if !wanted {
        return out;
    }
    let decoded: Vec<(&[u8; 4], Option<Json>)> = subs.iter().map(|s| (&s.sig, val(s))).collect();
    let fid = |v: &Option<Json>| v.as_ref().and_then(hexid).and_then(map).map(FormId);
    let cond = |v: &Option<Json>, issues: &mut Issues| -> Option<Condition> {
        match v.as_ref().map(|v| cond::parse(v, types, &map)) {
            Some(Ok(c)) => Some(c),
            Some(Err(e)) => {
                issues.note("condition unreadable", || format!("0x{g:08X}: {e}"));
                None
            }
            None => {
                issues.note("condition not decoded", || format!("0x{g:08X}"));
                None
            }
        }
    };
    match &sig {
        b"QUST" => {
            let mut q = QuestDef { form: FormId(g), edid: out.edid.clone(), script: out.script, ..Default::default() };
            // Where the next CTDA belongs.
            enum At {
                Quest,
                Entry,
                Target,
            }
            let mut at = At::Quest;
            let mut stage: Option<i32> = None;
            let mut objective: Option<i32> = None;
            for (s, v) in &decoded {
                match *s {
                    b"FULL" => q.name = v.as_ref().and_then(|v| v.as_str().map(str::to_string)),
                    b"DATA" => {
                        if let Some(v) = v {
                            q.flags = flag_bits(v.get("flags")) as u8;
                            q.priority = v.get("priority").and_then(Json::as_u64).unwrap_or(0) as u8;
                            q.delay = v.get("quest_delay").and_then(Json::as_f64).map(|d| d as f32);
                        }
                    }
                    b"INDX" => {
                        stage = v.as_ref().and_then(Json::as_i64).map(|x| x as i32);
                        objective = None;
                        if let Some(st) = stage {
                            q.stages.entry(st).or_default();
                        }
                        at = At::Quest;
                    }
                    b"QSDT" => {
                        if let Some(st) = stage {
                            let flags = flag_bits(v.as_ref()) as u8;
                            q.stages.entry(st).or_default().push(StageEntry { flags, ..Default::default() });
                            at = At::Entry;
                        }
                    }
                    b"CNAM" => {
                        if let Some(e) = stage.and_then(|st| q.stages.get_mut(&st)).and_then(|e| e.last_mut()) {
                            e.log = v.as_ref().and_then(|v| v.as_str().map(str::to_string));
                        }
                    }
                    b"NAM0" => {
                        let next = fid(v);
                        match stage.and_then(|st| q.stages.get_mut(&st)).and_then(|e| e.last_mut()) {
                            Some(e) => e.next_quest = next,
                            None => q.next_quest = next,
                        }
                    }
                    b"QOBJ" => {
                        let i = v.as_ref().and_then(Json::as_i64).map(|x| x as i32);
                        objective = i;
                        stage = None;
                        if let Some(i) = i {
                            q.objectives.insert(i, Objective { index: i, ..Default::default() });
                        }
                        at = At::Quest;
                    }
                    b"NNAM" => {
                        if let Some(o) = objective.and_then(|i| q.objectives.get_mut(&i)) {
                            o.text = v.as_ref().and_then(|v| v.as_str().map(str::to_string));
                        }
                    }
                    b"QSTA" => {
                        if let Some(o) = objective.and_then(|i| q.objectives.get_mut(&i)) {
                            let t = v.as_ref().and_then(|v| v.get("target")).and_then(hexid).and_then(map).map(FormId);
                            o.targets.push(Target { target: t, conditions: Vec::new() });
                            at = At::Target;
                        }
                    }
                    b"CTDA" => {
                        let Some(c) = cond(v, &mut out.issues) else { continue };
                        match at {
                            At::Quest => q.conditions.push(c),
                            At::Entry => {
                                if let Some(e) = stage.and_then(|st| q.stages.get_mut(&st)).and_then(|e| e.last_mut()) {
                                    e.conditions.push(c);
                                }
                            }
                            At::Target => {
                                if let Some(t) =
                                    objective.and_then(|i| q.objectives.get_mut(&i)).and_then(|o| o.targets.last_mut())
                                {
                                    t.conditions.push(c);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            out.quest = Some(q);
        }
        b"INFO" => {
            let mut info = InfoDef::default();
            let mut after_next = false;
            for (s, v) in &decoded {
                match *s {
                    b"QSTI" => info.quest = fid(v),
                    b"ANAM" => info.speaker = fid(v),
                    b"NEXT" => after_next = true,
                    b"CTDA" if !after_next => {
                        if let Some(c) = cond(v, &mut out.issues) {
                            info.conditions.push(c);
                        }
                    }
                    _ => {}
                }
            }
            out.info = Some(info);
        }
        b"MESG" => {
            let mut m = MessageDef::default();
            for (s, v) in &decoded {
                match *s {
                    b"DESC" => m.text = v.as_ref().and_then(|v| v.as_str().map(str::to_string)),
                    b"ITXT" => m.buttons.push(v.as_ref().and_then(|v| v.as_str()).unwrap_or_default().to_string()),
                    b"DNAM" => m.message_box = has_flag(v.as_ref(), "Message Box"),
                    _ => {}
                }
            }
            out.message = Some(m);
        }
        b"FLST" => {
            out.form_list = Some(decoded.iter().filter(|(s, _)| *s == b"LNAM").filter_map(|(_, v)| fid(v)).collect());
        }
        b"NPC_" | b"CREA" => {
            let mut a = ActorDef { sig, ..Default::default() };
            for (s, v) in &decoded {
                match *s {
                    b"SNAM" => {
                        if let Some(f) = v.as_ref().and_then(|v| v.get("faction")).and_then(hexid).and_then(map) {
                            let rank = v.as_ref().and_then(|v| v.get("rank")).and_then(Json::as_i64).unwrap_or(0) as i8;
                            a.factions.push((FormId(f), rank));
                        }
                    }
                    b"VTCK" => a.voice = fid(v),
                    b"RNAM" if &sig == b"NPC_" => a.race = fid(v),
                    b"CNAM" if &sig == b"NPC_" => a.class = fid(v),
                    b"ACBS" => {
                        let f = v.as_ref().and_then(|v| v.get("flags"));
                        a.female = has_flag(f, "Female");
                        a.essential = has_flag(f, "Essential");
                    }
                    _ => {}
                }
            }
            out.actor = Some(a);
        }
        _ => {}
    }
    out
}
