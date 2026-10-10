//! The static data scripts run against: compiled scripts (decoded once), quests from
//! `QUST`, dialogue and package result scripts, messages, form lists, actor bases, and
//! Editor IDs. Built once per load order from the winning record of every form; field
//! meanings come from the Layer 1 schema (`schemas/fnv`), never from byte offsets here.

use crate::cond::{self, Condition, ParamTypes};
use crate::quest::{Objective, QuestDef, StageEntry, Target};
use fnv_esm::decode::{decode_sub, DecodeCtx};
use fnv_esm::schema::TypeDef;
use fnv_esm::{LoadOrder, Record, Schema};
use fnv_script::unit::{read_record, ReadIssues, ScriptUnit, UnitOwner, VarType};
use fnv_script::{decode, FunctionTable, Program};
use fnv_sim::extract::Issues;
use fnv_sim::{FormId, RefTable};
use rayon::prelude::*;
use serde_json::Value as Json;
use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// The topic (DIAL) the line belongs to: the label of its enclosing "topic children"
    /// group [documented: xEdit group types; observed: every INFO of the masters sits in
    /// such a group].
    pub topic: Option<FormId>,
    /// DATA flags: Say Once (an actor says the line only once [documented: GECK wiki
    /// Category:Dialogue, owner export 2026-10-09]) and Goodbye.
    pub say_once: bool,
    pub goodbye: bool,
    /// Characters of each response text (NAM1), in record order: the stand-in for the
    /// length of the voice file (docs/spec-notes/scripts.md §7.7). Same order as
    /// [`InfoDef::responses`].
    pub response_chars: Vec<usize>,
    /// The record's header flags (0x2000 is set on about a quarter of the masters'
    /// lines; its meaning on INFO is not documented, docs/spec-notes/dialogue.md §2).
    pub record_flags: u32,
    /// DATA type code (0 Topic … 7 Radio, [`topic_type`]) [documented: xEdit INFO DATA].
    pub info_type: u8,
    /// DATA next speaker (0 Target, 1 Self, 2 Either) [documented: xEdit wbNextSpeaker];
    /// read, not used (conversations between NPCs).
    pub next_speaker: u8,
    /// DATA flags: the first flag byte in bits 0–7, the second in bits 8–15
    /// ([`info_flag`]) [documented: xEdit INFO DATA; the same combined value the GECK
    /// wiki's GetDialogueInfoFlags page lists, owner export].
    pub flags: u16,
    /// RNAM: the text shown to the player instead of the topic's name
    /// [documented: Category:Dialogue, owner export].
    pub prompt: Option<String>,
    /// The responses (TRDT with its NAM1, NAM2, NAM3, SNAM, LNAM), in record order.
    pub responses: Vec<Response>,
    /// TCLT, in record order: the topics offered after this line ("Choices" in the
    /// editor; xEdit calls the list "Link To") [documented: xEdit INFO; Category:Dialogue].
    pub choices: Vec<FormId>,
    /// TCLF, in record order ("Link From") [documented: xEdit INFO]. Read, not used
    /// (dialogue.md §5).
    pub link_from: Vec<FormId>,
    /// TCFU, in record order: follow-up lines (INFO) [documented: xEdit INFO "Follow Up"].
    pub follow_ups: Vec<FormId>,
    /// NAME, in record order: topics the line adds [documented: xEdit INFO "Add Topics"].
    pub add_topics: Vec<FormId>,
    /// KNAM: the actor value (AVIF) or perk shown with the choice [documented: xEdit
    /// INFO "ActorValue/Perk"; Category:Dialogue "AV/Perk"].
    pub challenge_form: Option<FormId>,
    /// DNAM: speech challenge difficulty code (0 none, 1 Very Easy … 5 Very Hard)
    /// [documented: xEdit INFO "Speech Challenge"].
    pub challenge_difficulty: Option<u32>,
    /// PNAM "Previous INFO": whether the record has one, and the line it names (none:
    /// the field is 0) [documented: xEdit INFO; Category:Dialogue "Edit Response Order"].
    pub has_previous: bool,
    pub previous: Option<FormId>,
    /// TPIC "Previous Topic" [documented: xEdit INFO]; absent in the ten masters.
    pub previous_topic: Option<FormId>,
}

/// One response of a line (TRDT and the subrecords after it) [documented: xEdit INFO
/// "Responses"; schema `gameplay.json`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Response {
    /// TRDT response number (the order responses are said in).
    pub number: u8,
    /// TRDT emotion type code (0 Neutral, 1 Anger, 2 Disgust, 3 Fear, 4 Sad, 5 Happy,
    /// 6 Surprise, 7 Pained) and value.
    pub emotion: u8,
    pub emotion_value: i32,
    /// TRDT sound (SOUN), played instead of the voice [documented: Category:Dialogue].
    pub sound: Option<FormId>,
    pub use_emotion_animation: bool,
    /// NAM1: the response text.
    pub text: String,
    /// NAM2 "Script Notes" (acting notes).
    pub notes: String,
    /// SNAM / LNAM: speaker and listener idle animations (IDLE).
    pub speaker_idle: Option<FormId>,
    pub listener_idle: Option<FormId>,
}

/// INFO DATA flag bits in [`InfoDef::flags`] [documented: xEdit INFO DATA flag names;
/// meanings: GECK wiki Category:Dialogue, owner export].
pub mod info_flag {
    pub const GOODBYE: u16 = 0x0001;
    pub const RANDOM: u16 = 0x0002;
    pub const SAY_ONCE: u16 = 0x0004;
    pub const RUN_IMMEDIATELY: u16 = 0x0008;
    pub const INFO_REFUSAL: u16 = 0x0010;
    pub const RANDOM_END: u16 = 0x0020;
    pub const RUN_FOR_RUMORS: u16 = 0x0040;
    pub const SPEECH_CHALLENGE: u16 = 0x0080;
    pub const SAY_ONCE_A_DAY: u16 = 0x0100;
    pub const ALWAYS_DARKEN: u16 = 0x0200;
    pub const LOW_INTELLIGENCE: u16 = 0x1000;
    pub const HIGH_INTELLIGENCE: u16 = 0x2000;
    /// Names for listings.
    pub const NAMES: &[(u16, &str)] = &[
        (GOODBYE, "Goodbye"),
        (RANDOM, "Random"),
        (SAY_ONCE, "Say Once"),
        (RUN_IMMEDIATELY, "Run Immediately"),
        (INFO_REFUSAL, "Info Refusal"),
        (RANDOM_END, "Random End"),
        (RUN_FOR_RUMORS, "Run for Rumors"),
        (SPEECH_CHALLENGE, "Speech Challenge"),
        (SAY_ONCE_A_DAY, "Say Once a Day"),
        (ALWAYS_DARKEN, "Always Darken"),
        (0x0400, "Unknown 2"),
        (0x0800, "Unknown 3"),
        (LOW_INTELLIGENCE, "Low Intelligence"),
        (HIGH_INTELLIGENCE, "High Intelligence"),
    ];
}

/// DIAL / INFO DATA type codes [documented: xEdit DIAL DATA].
pub mod topic_type {
    pub const TOPIC: u8 = 0;
    pub const CONVERSATION: u8 = 1;
    pub const COMBAT: u8 = 2;
    pub const PERSUASION: u8 = 3;
    pub const DETECTION: u8 = 4;
    pub const SERVICE: u8 = 5;
    pub const MISCELLANEOUS: u8 = 6;
    pub const RADIO: u8 = 7;
    pub const NAMES: [&str; 8] = ["Topic", "Conversation", "Combat", "Persuasion", "Detection", "Service", "Miscellaneous", "Radio"];
}

impl InfoDef {
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    /// Indices into [`InfoDef::responses`] in the order they are said: by response
    /// number, record order among equal numbers [documented: xEdit "Response number";
    /// hypothesis that the number, not the record order, decides where the two differ,
    /// dialogue.md §2].
    pub fn response_order(&self) -> Vec<usize> {
        let mut ix: Vec<usize> = (0..self.responses.len()).collect();
        ix.sort_by_key(|&i| self.responses[i].number);
        ix
    }
}

/// A dialogue topic (DIAL) [documented: xEdit DIAL; schema `gameplay.json`].
#[derive(Debug, Clone)]
pub struct TopicDef {
    pub form: FormId,
    pub edid: Option<String>,
    /// FULL: the text shown to the player when the line has no prompt
    /// [documented: Category:Dialogue "Prompt"].
    pub name: Option<String>,
    /// DATA type code ([`topic_type`]).
    pub topic_type: u8,
    /// DATA flags: 1 Rumors, 2 Top-level.
    pub flags: u8,
    /// PNAM: priority, 50 when absent [documented: xEdit default].
    pub priority: f32,
    /// QSTI of the winning record, in record order ("Added Quests").
    pub quests: Vec<FormId>,
    /// QSTR ("Removed Quests"); none in the masters.
    pub removed_quests: Vec<FormId>,
    /// INFC / INFX pairs ("Shared Infos") with the QSTI they follow. What the game does
    /// with them is not documented; they are kept raw and not used (dialogue.md §1.2).
    pub shared_infos: Vec<SharedInfo>,
    /// TDUM "Dumb Response" [documented: xEdit DIAL]; not used.
    pub dumb_response: Option<String>,
    /// Where the topic was first defined: plugin index and record ordinal in that file
    /// (the "record order" tie-break of the offered list).
    pub order: (usize, usize),
}

/// One INFC / INFX pair of a DIAL.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SharedInfo {
    pub quest: Option<FormId>,
    pub info: Option<FormId>,
    pub index: Option<i32>,
}

impl Default for TopicDef {
    fn default() -> Self {
        TopicDef {
            form: FormId(0),
            edid: None,
            name: None,
            topic_type: 0,
            flags: 0,
            priority: 50.0,
            quests: Vec::new(),
            removed_quests: Vec::new(),
            shared_infos: Vec::new(),
            dumb_response: None,
            order: (usize::MAX, 0),
        }
    }
}

impl TopicDef {
    pub const RUMORS: u8 = 1;
    pub const TOP_LEVEL: u8 = 2;

    pub fn top_level(&self) -> bool {
        self.flags & Self::TOP_LEVEL != 0
    }
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
    /// PKID: the AI package list, in record order.
    pub packages: Vec<FormId>,
    /// TPLT: the template actor, and whether the template supplies the AI packages
    /// (ACBS template flag "AI Packages") [documented: xEdit; schema `gameplay.json`].
    pub template: Option<FormId>,
    pub template_packages: bool,
    /// CREA DATA creature type code (0 Animal … 7 Giant) [documented: xEdit
    /// wbCreatureTypeEnum; schema `gameplay.json`].
    pub creature_type: Option<i64>,
    /// Combat (step C5) [documented: xEdit field names; GECK wiki Stats Tab pages]:
    /// NPC_ DATA Base Health (added to the health derived from Endurance and level) or
    /// CREA DATA Health (the creature's health).
    pub base_health: i64,
    /// DATA Endurance.
    pub endurance: i64,
    /// ACBS level, or the player-level multiplier ×1000 with "PC Level Mult", and the
    /// calculated level's bounds.
    pub level_raw: i64,
    pub pc_level_mult: bool,
    pub calc_min: i64,
    pub calc_max: i64,
    /// ACBS template flag "Stats": the template supplies the stats.
    pub template_stats: bool,
    /// CREA ACBS "Invulnerable".
    pub invulnerable: bool,
}

/// A region's areas (REGN WNAM, RPLI/RPLD): each area a polygon of worldspace x, y
/// points (schema `sky.json`).
#[derive(Debug, Clone, Default)]
pub struct RegionDef {
    pub world: Option<FormId>,
    pub areas: Vec<Vec<[f32; 2]>>,
}

impl RegionDef {
    /// Whether a worldspace point lies inside one of the areas (even-odd rule)
    /// [hypothesis: an area is the plain polygon of its points; the edge fall-off
    /// (RPLI) is ignored].
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.areas.iter().any(|pts| {
            let mut inside = false;
            let n = pts.len();
            for i in 0..n {
                let [xi, yi] = pts[i];
                let [xj, yj] = pts[(i + n - 1) % n];
                if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                    inside = !inside;
                }
            }
            inside
        })
    }
}

/// A base effect (MGEF DATA) [documented: xEdit MGEF; schema `gameplay.json`].
#[derive(Debug, Clone, Default)]
pub struct MagicEffectDef {
    pub flags: u32,
    pub archetype: i64,
    /// The associated script when the archetype is 1 (Script).
    pub script: Option<FormId>,
}

/// MGEF archetype "Script" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_SCRIPT: i64 = 1;

/// One effect of a magic item (EFID + EFIT + its CTDA) on ALCH, SPEL, ENCH and INGR
/// [documented: xEdit wbEffects; schema `common.json`].
#[derive(Debug, Clone)]
pub struct EffectItem {
    pub effect: FormId,
    pub magnitude: u32,
    pub area: u32,
    /// Seconds.
    pub duration: u32,
    pub conditions: Vec<Condition>,
}

/// Destructible object data (DEST, then DSTD per stage) [documented: xEdit wbDEST].
#[derive(Debug, Clone, Default)]
pub struct DestructionDef {
    pub health: i32,
    pub stages: Vec<DestructionStage>,
}

/// One DSTD stage, with its DMDL replacement model when one follows [documented: xEdit
/// wbDEST; meanings: GECK wiki Destructible Object Stage, owner export 2026-10-09].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DestructionStage {
    pub health_percent: u8,
    pub index: u8,
    /// Model damage stage (0–8; `BSDamageStage` nodes).
    pub damage_stage: u8,
    /// 1 cap damage, 2 disable, 4 destroy.
    pub flags: u8,
    pub self_damage_per_second: i32,
    pub explosion: Option<FormId>,
    pub debris: Option<FormId>,
    pub debris_count: i32,
    pub model: Option<String>,
}

impl DestructionStage {
    pub const CAP_DAMAGE: u8 = 1;
    pub const DISABLE: u8 = 2;
    pub const DESTROY: u8 = 4;
}

pub use crate::package::PackageDef;

pub struct GameData {
    pub table: FunctionTable,
    pub param_types: ParamTypes,
    pub load_order: Vec<String>,
    pub units: Vec<Compiled>,
    /// SCPT FormID → unit.
    pub scripts: HashMap<FormId, UnitId>,
    pub quests: BTreeMap<FormId, QuestDef>,
    pub infos: HashMap<FormId, InfoDef>,
    /// The dialogue lines (INFO) of each topic (DIAL), in topic order: the order of the
    /// plugin that first defines each line, then its place in that file; lines a later
    /// plugin adds follow the earlier plugins' [hypothesis: the masters' INFOs carry no
    /// PNAM ordering, so file order stands for the editor's list order].
    pub topics: HashMap<FormId, Vec<FormId>>,
    /// Every topic (DIAL) from its winning record.
    pub topic_defs: HashMap<FormId, TopicDef>,
    /// The top-level topics of type Topic, in the order they are offered: priority,
    /// highest first, then record order ([`TopicDef::order`]).
    pub top_level: Vec<FormId>,
    /// The special topics GREETING and GOODBYE, by Editor ID [documented: GECK wiki
    /// Topics Tab and Conversation Tab, owner export].
    pub greeting: Option<FormId>,
    pub goodbye: Option<FormId>,
    /// PACK begin / end / change result scripts.
    pub packages: HashMap<FormId, [Option<UnitId>; 3]>,
    /// Every PACK as typed data ([`crate::package`]).
    pub package_defs: HashMap<FormId, PackageDef>,
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
    /// REPU DATA: the reputation's scale (maximum fame or infamy).
    pub reputations: HashMap<FormId, f32>,
    /// CHAL DATA flags (1 Start Disabled, 2 Recurring, 4 Show Zero Progress).
    pub challenges: HashMap<FormId, u32>,
    pub regions: HashMap<FormId, RegionDef>,
    pub magic_effects: HashMap<FormId, MagicEffectDef>,
    /// The effects of every ALCH, SPEL, ENCH and INGR, in record order.
    pub effect_items: HashMap<FormId, Vec<EffectItem>>,
    /// Races with the Child flag (RACE DATA).
    pub child_races: HashSet<FormId>,
    /// FACT XNAM: (other faction, group combat reaction 0 neutral, 1 enemy, 2 ally,
    /// 3 friend).
    pub faction_reactions: HashMap<FormId, Vec<(FormId, i64)>>,
    pub destructibles: HashMap<FormId, DestructionDef>,
    /// WEAP DNAM skill (actor value code).
    pub weapon_skills: HashMap<FormId, i64>,
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
    topic: Option<TopicDef>,
    message: Option<MessageDef>,
    form_list: Option<Vec<FormId>>,
    actor: Option<ActorDef>,
    package: Option<PackageDef>,
    reputation: Option<f32>,
    challenge: Option<u32>,
    region: Option<RegionDef>,
    magic_effect: Option<MagicEffectDef>,
    effects: Option<Vec<EffectItem>>,
    child_race: bool,
    faction: Option<Vec<(FormId, i64)>>,
    destruction: Option<DestructionDef>,
    weapon_skill: Option<i64>,
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
            topics: HashMap::new(),
            topic_defs: HashMap::new(),
            top_level: Vec::new(),
            greeting: None,
            goodbye: None,
            packages: HashMap::new(),
            package_defs: HashMap::new(),
            messages: HashMap::new(),
            form_lists: HashMap::new(),
            actors: HashMap::new(),
            form_script: HashMap::new(),
            edids: HashMap::new(),
            edid_of: HashMap::new(),
            sigs: HashMap::new(),
            scripted_by_cell: BTreeMap::new(),
            reputations: HashMap::new(),
            challenges: HashMap::new(),
            regions: HashMap::new(),
            magic_effects: HashMap::new(),
            effect_items: HashMap::new(),
            child_races: HashSet::new(),
            faction_reactions: HashMap::new(),
            destructibles: HashMap::new(),
            weapon_skills: HashMap::new(),
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
            if let Some(mut t) = r.topic {
                t.form = id;
                d.topic_defs.insert(id, t);
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
            if let Some(p) = r.package {
                d.package_defs.insert(id, p);
            }
            if let Some(v) = r.reputation {
                d.reputations.insert(id, v);
            }
            if let Some(v) = r.challenge {
                d.challenges.insert(id, v);
            }
            if let Some(v) = r.region {
                d.regions.insert(id, v);
            }
            if let Some(v) = r.magic_effect {
                d.magic_effects.insert(id, v);
            }
            if let Some(v) = r.effects {
                d.effect_items.insert(id, v);
            }
            if r.child_race {
                d.child_races.insert(id);
            }
            if let Some(v) = r.faction {
                d.faction_reactions.insert(id, v);
            }
            if let Some(v) = r.destruction {
                d.destructibles.insert(id, v);
            }
            if let Some(v) = r.weapon_skill {
                d.weapon_skills.insert(id, v);
            }
        }
        let (lines, order) = topic_lines(lo);
        d.topics = lines;
        for (topic, lines) in &d.topics {
            for i in lines {
                if let Some(info) = d.infos.get_mut(i) {
                    info.topic = Some(*topic);
                }
            }
        }
        for (id, t) in d.topic_defs.iter_mut() {
            t.order = order.get(id).copied().unwrap_or((usize::MAX, 0));
        }
        let mut top: Vec<&TopicDef> = d.topic_defs.values().filter(|t| t.top_level() && t.topic_type == topic_type::TOPIC).collect();
        top.sort_by(|a, b| b.priority.total_cmp(&a.priority).then(a.order.cmp(&b.order)));
        d.top_level = top.into_iter().map(|t| t.form).collect();
        d.greeting = d.by_edid("GREETING").filter(|t| d.topic_defs.contains_key(t));
        d.goodbye = d.by_edid("GOODBYE").filter(|t| d.topic_defs.contains_key(t));
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

    /// The AI package list of an actor base (NPC_, CREA): its own `PKID` list, or its
    /// template's when the template supplies the AI packages
    /// ([`crate::game::Game::package_list`]).
    pub fn base_package_list(&self, mut base: FormId) -> Vec<FormId> {
        for _ in 0..8 {
            let Some(a) = self.actors.get(&base) else { break };
            match a.template.filter(|t| a.template_packages && self.actors.contains_key(t)) {
                Some(t) => base = t,
                None => return a.packages.clone(),
            }
        }
        self.actors.get(&base).map(|a| a.packages.clone()).unwrap_or_default()
    }

    /// The priority of the quest a line belongs to (0 without a quest).
    pub fn line_priority(&self, info: FormId) -> u8 {
        self.infos.get(&info).and_then(|d| d.quest).and_then(|q| self.quests.get(&q)).map_or(0, |q| q.priority)
    }

    /// A topic's lines in the order they are tried: lines of higher-priority quests
    /// first, and within one priority the stored order ([`GameData::topics`]); a stable
    /// sort, so lines of one quest stay top to bottom [documented: GECK wiki Quest Data
    /// Tab, priority; owner statement 2026-10-10 for equal priorities and for the order
    /// inside a quest; docs/spec-notes/dialogue.md §4].
    pub fn lines_by_priority(&self, topic: FormId) -> Vec<FormId> {
        let mut lines: Vec<FormId> = self.topics.get(&topic).cloned().unwrap_or_default();
        lines.sort_by_key(|i| std::cmp::Reverse(self.line_priority(*i)));
        lines
    }

    /// The text the player sees for a topic whose line would be `info`: the line's
    /// prompt, else the topic's name, else nothing [documented: GECK wiki
    /// Category:Dialogue, "Prompt"].
    pub fn choice_text(&self, topic: FormId, info: FormId) -> &str {
        match self.infos.get(&info).and_then(|d| d.prompt.as_deref()) {
            Some(p) => p,
            None => self.topic_defs.get(&topic).and_then(|t| t.name.as_deref()).unwrap_or(""),
        }
    }
}

/// Every topic's dialogue lines in topic order ([`GameData::topics`]), and where each
/// topic (DIAL) is first defined (plugin, record ordinal).
type TopicLines = (HashMap<FormId, Vec<FormId>>, HashMap<FormId, (usize, usize)>);

fn topic_lines(lo: &LoadOrder) -> TopicLines {
    use fnv_esm::raw::{group_type, visit_records};
    // Line -> (topic, plugin, ordinal) of its first definition.
    let mut first: HashMap<u32, (u32, usize, usize)> = HashMap::new();
    let mut dial: HashMap<FormId, (usize, usize)> = HashMap::new();
    for (pi, p) in lo.plugins.iter().enumerate() {
        let mut ord = 0usize;
        visit_records(p.nodes.get(1..).unwrap_or_default(), &mut |r, groups| {
            ord += 1;
            if &r.header.sig == b"DIAL" {
                if let Some(t) = lo.to_global(pi, r.header.form_id) {
                    dial.entry(FormId(t)).or_insert((pi, ord));
                }
            }
            if &r.header.sig != b"INFO" {
                return;
            }
            let Some(g) = groups.iter().rev().find(|g| g.group_type == group_type::TOPIC_CHILDREN) else { return };
            let (Some(info), Some(topic)) = (lo.to_global(pi, r.header.form_id), lo.to_global(pi, g.label_u32())) else { return };
            first.entry(info).or_insert((topic, pi, ord));
        });
    }
    let mut by_topic: HashMap<FormId, Vec<(usize, usize, FormId)>> = HashMap::new();
    for (info, (topic, pi, ord)) in first {
        by_topic.entry(FormId(topic)).or_default().push((pi, ord, FormId(info)));
    }
    let lines = by_topic
        .into_iter()
        .map(|(t, mut v)| {
            v.sort();
            (t, v.into_iter().map(|x| x.2).collect())
        })
        .collect();
    (lines, dial)
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
    let wanted = matches!(
        &sig,
        b"QUST"
            | b"INFO"
            | b"DIAL"
            | b"MESG"
            | b"FLST"
            | b"NPC_"
            | b"CREA"
            | b"PACK"
            | b"REPU"
            | b"CHAL"
            | b"REGN"
            | b"MGEF"
            | b"ALCH"
            | b"SPEL"
            | b"ENCH"
            | b"INGR"
            | b"RACE"
            | b"FACT"
            | b"WEAP"
    ) || subs.iter().any(|s| &s.sig == b"DEST");
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
    if let Some((_, Some(h))) = decoded.iter().find(|(s, _)| *s == b"DEST") {
        let mut dd = DestructionDef { health: h.get("health").and_then(Json::as_i64).unwrap_or(0) as i32, stages: Vec::new() };
        for (sig, v) in decoded.iter() {
            match *sig {
                b"DSTD" => {
                    let Some(v) = v else { continue };
                    let n = |k: &str| v.get(k).and_then(Json::as_u64).unwrap_or(0) as u8;
                    let i = |k: &str| v.get(k).and_then(Json::as_i64).unwrap_or(0) as i32;
                    // FormID 0 is "none".
                    let form = |k: &str| v.get(k).and_then(hexid).filter(|&x| x != 0).and_then(map).map(FormId);
                    dd.stages.push(DestructionStage {
                        health_percent: n("health_percent"),
                        index: n("index"),
                        damage_stage: n("damage_stage"),
                        flags: flag_bits(v.get("flags")) as u8,
                        self_damage_per_second: i("self_damage_per_second"),
                        explosion: form("explosion"),
                        debris: form("debris"),
                        debris_count: i("debris_count"),
                        model: None,
                    });
                }
                // DMDL follows its stage's DSTD.
                b"DMDL" => {
                    if let (Some(st), Some(Json::String(m))) = (dd.stages.last_mut(), v) {
                        st.model = Some(m.trim().to_string()).filter(|m| !m.is_empty());
                    }
                }
                _ => {}
            }
        }
        out.destruction = Some(dd);
    }
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
            let mut info = InfoDef { record_flags: r.header.flags, ..Default::default() };
            let mut after_next = false;
            let text = |v: &Option<Json>| v.as_ref().and_then(Json::as_str).unwrap_or_default().to_string();
            // FormID 0 is "none".
            let link = |v: &Option<Json>| v.as_ref().and_then(hexid).filter(|&x| x != 0).and_then(map).map(FormId);
            for (s, v) in &decoded {
                match *s {
                    b"QSTI" => info.quest = fid(v),
                    b"ANAM" => info.speaker = link(v),
                    b"DATA" => {
                        let Some(v) = v else { continue };
                        let code = |k: &str| v.get(k).and_then(|t| field_code(schema, &sig, b"DATA", Some(k), t)).unwrap_or(0) as u8;
                        info.info_type = code("type");
                        info.next_speaker = code("next_speaker");
                        info.flags = (flag_bits(v.get("flags1")) as u16 & 0xFF) | ((flag_bits(v.get("flags2")) as u16 & 0xFF) << 8);
                        info.say_once = info.has(info_flag::SAY_ONCE);
                        info.goodbye = info.has(info_flag::GOODBYE);
                    }
                    b"TPIC" => info.previous_topic = link(v),
                    b"PNAM" => {
                        info.has_previous = true;
                        info.previous = link(v);
                    }
                    b"NAME" => info.add_topics.extend(link(v)),
                    b"TRDT" => {
                        let mut resp = Response::default();
                        if let Some(v) = v {
                            resp.number = v.get("response_number").and_then(Json::as_u64).unwrap_or(0) as u8;
                            resp.emotion =
                                v.get("emotion_type").and_then(|t| field_code(schema, &sig, b"TRDT", Some("emotion_type"), t)).unwrap_or(0) as u8;
                            resp.emotion_value = v.get("emotion_value").and_then(Json::as_i64).unwrap_or(0) as i32;
                            resp.sound = v.get("sound").and_then(hexid).filter(|&x| x != 0).and_then(map).map(FormId);
                            resp.use_emotion_animation = match v.get("use_emotion_animation") {
                                Some(Json::Bool(b)) => *b,
                                Some(x) => x.as_u64().unwrap_or(0) != 0,
                                None => false,
                            };
                        }
                        info.responses.push(resp);
                        info.response_chars.push(0);
                    }
                    b"NAM1" => {
                        let t = text(v);
                        // A text without its TRDT (none in the masters) is a response of
                        // its own.
                        if info.responses.len() != info.response_chars.len() || info.responses.last().is_none_or(|x| !x.text.is_empty()) {
                            info.responses.push(Response::default());
                            info.response_chars.push(0);
                        }
                        if let (Some(resp), Some(n)) = (info.responses.last_mut(), info.response_chars.last_mut()) {
                            *n = t.chars().count();
                            resp.text = t;
                        }
                    }
                    b"NAM2" => {
                        if let Some(resp) = info.responses.last_mut() {
                            resp.notes = text(v);
                        }
                    }
                    b"SNAM" => {
                        if let Some(resp) = info.responses.last_mut() {
                            resp.speaker_idle = link(v);
                        }
                    }
                    b"LNAM" => {
                        if let Some(resp) = info.responses.last_mut() {
                            resp.listener_idle = link(v);
                        }
                    }
                    b"TCLT" => info.choices.extend(link(v)),
                    b"TCLF" => info.link_from.extend(link(v)),
                    b"TCFU" => info.follow_ups.extend(link(v)),
                    b"RNAM" => info.prompt = Some(text(v)).filter(|t| !t.is_empty()),
                    b"KNAM" => info.challenge_form = link(v),
                    b"DNAM" => {
                        info.challenge_difficulty =
                            v.as_ref().and_then(|t| field_code(schema, &sig, b"DNAM", None, t)).map(|c| c as u32);
                    }
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
        b"DIAL" => {
            let mut t = TopicDef { form: FormId(g), edid: out.edid.clone(), ..Default::default() };
            let mut quest: Option<FormId> = None;
            for (s, v) in &decoded {
                match *s {
                    b"FULL" => t.name = v.as_ref().and_then(|v| v.as_str().map(str::to_string)),
                    b"PNAM" => t.priority = v.as_ref().and_then(Json::as_f64).unwrap_or(50.0) as f32,
                    b"TDUM" => t.dumb_response = v.as_ref().and_then(|v| v.as_str().map(str::to_string)),
                    b"DATA" => {
                        if let Some(v) = v {
                            t.topic_type = v.get("type").and_then(|x| field_code(schema, &sig, b"DATA", Some("type"), x)).unwrap_or(0) as u8;
                            t.flags = flag_bits(v.get("flags")) as u8;
                        }
                    }
                    b"QSTI" => {
                        quest = fid(v);
                        t.quests.extend(quest);
                    }
                    b"QSTR" => t.removed_quests.extend(fid(v)),
                    b"INFC" => t.shared_infos.push(SharedInfo { quest, info: fid(v).filter(|f| f.0 != 0), index: None }),
                    b"INFX" => {
                        let x = v.as_ref().and_then(Json::as_i64).map(|x| x as i32);
                        match t.shared_infos.last_mut() {
                            Some(si) if si.index.is_none() => si.index = x,
                            _ => t.shared_infos.push(SharedInfo { quest, info: None, index: x }),
                        }
                    }
                    _ => {}
                }
            }
            out.topic = Some(t);
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
                    b"PKID" => a.packages.extend(fid(v)),
                    b"TPLT" => a.template = fid(v),
                    b"RNAM" if &sig == b"NPC_" => a.race = fid(v),
                    b"CNAM" if &sig == b"NPC_" => a.class = fid(v),
                    b"DATA" if &sig == b"CREA" => {
                        a.creature_type =
                            v.as_ref().and_then(|v| v.get("type")).and_then(|t| field_code(schema, &sig, b"DATA", Some("type"), t));
                        a.base_health = v.as_ref().and_then(|v| v.get("health")).and_then(Json::as_i64).unwrap_or(0);
                        a.endurance = v.as_ref().and_then(|v| v.get("attributes")).and_then(|x| x.get("endurance")).and_then(Json::as_i64).unwrap_or(0);
                    }
                    b"DATA" if &sig == b"NPC_" => {
                        a.base_health = v.as_ref().and_then(|v| v.get("base_health")).and_then(Json::as_i64).unwrap_or(0);
                        a.endurance = v.as_ref().and_then(|v| v.get("attributes")).and_then(|x| x.get("endurance")).and_then(Json::as_i64).unwrap_or(0);
                    }
                    b"ACBS" => {
                        let f = v.as_ref().and_then(|v| v.get("flags"));
                        a.female = has_flag(f, "Female");
                        a.essential = has_flag(f, "Essential");
                        a.invulnerable = has_flag(f, "Invulnerable");
                        a.pc_level_mult = has_flag(f, "PC Level Mult");
                        let n = |k: &str| v.as_ref().and_then(|v| v.get(k)).and_then(Json::as_i64).unwrap_or(0);
                        (a.level_raw, a.calc_min, a.calc_max) = (n("level_or_level_mult_raw"), n("calc_min"), n("calc_max"));
                        let tf = v.as_ref().and_then(|v| v.get("template_flags"));
                        a.template_packages = has_flag(tf, "AI Packages");
                        a.template_stats = has_flag(tf, "Stats");
                    }
                    _ => {}
                }
            }
            out.actor = Some(a);
        }
        b"PACK" => {
            let code = |sub: &[u8; 4], field: &str, v: &Json| field_code(schema, &sig, sub, Some(field), v);
            let ctx = crate::package::ReadCtx { code: &code, map: &map };
            let mut issues = Issues::default();
            out.package = Some(crate::package::read(&decoded, &ctx, &mut |v| cond(v, &mut issues)));
            out.issues.merge(issues);
        }
        b"REPU" => {
            out.reputation = decoded.iter().find(|(s, _)| *s == b"DATA").and_then(|(_, v)| v.as_ref()?.as_f64()).map(|x| x as f32);
        }
        b"CHAL" => {
            out.challenge = decoded.iter().find(|(s, _)| *s == b"DATA").and_then(|(_, v)| v.as_ref()).map(|v| flag_bits(v.get("flags")));
        }
        b"REGN" => {
            let mut rg = RegionDef::default();
            for (s, v) in &decoded {
                match *s {
                    b"WNAM" => rg.world = fid(v),
                    b"RPLI" => rg.areas.push(Vec::new()),
                    b"RPLD" => {
                        let pts: Vec<[f32; 2]> = v
                            .as_ref()
                            .and_then(Json::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|p| Some([p.get("x")?.as_f64()? as f32, p.get("y")?.as_f64()? as f32]))
                            .collect();
                        match rg.areas.last_mut() {
                            Some(a) if a.is_empty() => *a = pts,
                            _ => rg.areas.push(pts),
                        }
                    }
                    _ => {}
                }
            }
            rg.areas.retain(|a| a.len() >= 3);
            out.region = Some(rg);
        }
        b"MGEF" => {
            if let Some((_, Some(v))) = decoded.iter().find(|(s, _)| *s == b"DATA") {
                let archetype = v.get("archetype").and_then(|t| field_code(schema, &sig, b"DATA", Some("archetype"), t)).unwrap_or(-1);
                let item = v.get("associated_item").and_then(hexid).filter(|&x| x != 0).and_then(map).map(FormId);
                out.magic_effect = Some(MagicEffectDef {
                    flags: flag_bits(v.get("flags")),
                    archetype,
                    script: item.filter(|_| archetype == ARCHETYPE_SCRIPT),
                });
            }
        }
        b"ALCH" | b"SPEL" | b"ENCH" | b"INGR" => {
            let mut list: Vec<EffectItem> = Vec::new();
            for (s, v) in &decoded {
                match *s {
                    b"EFID" => list.extend(fid(v).map(|effect| EffectItem { effect, magnitude: 0, area: 0, duration: 0, conditions: Vec::new() })),
                    b"EFIT" => {
                        if let (Some(e), Some(v)) = (list.last_mut(), v.as_ref()) {
                            let n = |k: &str| v.get(k).and_then(Json::as_u64).unwrap_or(0) as u32;
                            e.magnitude = n("magnitude");
                            e.area = n("area");
                            e.duration = n("duration");
                        }
                    }
                    b"CTDA" => {
                        if let Some(c) = cond(v, &mut out.issues) {
                            if let Some(e) = list.last_mut() {
                                e.conditions.push(c);
                            }
                        }
                    }
                    _ => {}
                }
            }
            out.effects = Some(list);
        }
        b"RACE" => {
            out.child_race = decoded
                .iter()
                .find(|(s, _)| *s == b"DATA")
                .is_some_and(|(_, v)| has_flag(v.as_ref().and_then(|v| v.get("flags")), "Child"));
        }
        b"FACT" => {
            out.faction = Some(
                decoded
                    .iter()
                    .filter(|(s, _)| *s == b"XNAM")
                    .filter_map(|(_, v)| {
                        let v = v.as_ref()?;
                        let f = v.get("faction").and_then(hexid).and_then(map)?;
                        let r = field_code(schema, &sig, b"XNAM", Some("group_combat_reaction"), v.get("group_combat_reaction")?)?;
                        Some((FormId(f), r))
                    })
                    .collect(),
            );
        }
        b"WEAP" => {
            out.weapon_skill = decoded
                .iter()
                .find(|(s, _)| *s == b"DNAM")
                .and_then(|(_, v)| v.as_ref()?.get("skill").and_then(|t| field_code(schema, &sig, b"DNAM", Some("skill"), t)));
        }
        _ => {}
    }
    out
}

fn resolve<'s>(schema: &'s Schema, t: &'s TypeDef) -> &'s TypeDef {
    match t {
        TypeDef::Named(n) => schema.types.get(n).map(|t| resolve(schema, t)).unwrap_or(t),
        _ => t,
    }
}

/// The integer code of a decoded enumeration value (the decoder gives the value's
/// name, or the number when the schema has no name for it), from the schema layout of
/// `sig`/`sub` (and the struct field `field`).
fn field_code(schema: &Schema, sig: &[u8; 4], sub: &[u8; 4], field: Option<&str>, v: &Json) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    let name = v.as_str()?;
    let def = schema.records.get(sig).and_then(|r| r.subrecords.get(sub)).or_else(|| schema.common.get(sub))?;
    let mut t = resolve(schema, &def.ty);
    if let Some(f) = field {
        let TypeDef::Struct(fields) = t else { return None };
        t = resolve(schema, &fields.iter().find(|x| x.name == f)?.ty);
    }
    match t {
        TypeDef::Enum { values, .. } => values.iter().find(|(_, n)| n == name).map(|(k, _)| *k),
        _ => None,
    }
}
