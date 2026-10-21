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
    /// DATA next speaker (0 Target, 1 Self, 2 Either) [documented: xEdit wbNextSpeaker]:
    /// who says the next line of a conversation between actors
    /// (`Game::next_conversation_line`).
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
    /// TCLF, in record order ("Link From") [documented: xEdit INFO]: the topics after
    /// whose lines this line's topic is offered to the player ([`GameData::linked_from`],
    /// dialogue.md §3.5); a HELLO line with one cannot start a conversation.
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
    /// INFC / INFX pairs ("Shared Infos") with the QSTI they follow: lines stored under
    /// another topic that count as lines of this one ([`GameData::lines_tried`],
    /// dialogue.md §1.2).
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
    /// FULL: the title of a message box.
    pub title: Option<String>,
    pub text: Option<String>,
    pub buttons: Vec<String>,
    /// CTDA after each ITXT: a button is shown only while its conditions hold
    /// [documented: GECK wiki GetButtonPressed, "certain buttons are hidden with
    /// conditions"; the indices stay those of the record]. One list per button.
    pub button_conditions: Vec<Vec<Condition>>,
    pub message_box: bool,
    /// INAM: the Menu Icon (MICN) drawn with the message when it is not a box; none
    /// means the default icon [documented: GECK wiki, Message; corner-messages.md §3].
    pub icon: Option<FormId>,
    /// TNAM: the seconds a message that is not a box stays on screen [documented: GECK
    /// wiki, Message "Display Time"]; boxes have none [observed].
    pub display_seconds: Option<u32>,
}

/// A note (NOTE): its type and, for a text note, its text [documented: xEdit
/// wbDefinitionsFNV.pas NOTE].
#[derive(Debug, Clone, Default)]
pub struct NoteDef {
    pub name: Option<String>,
    /// DATA: 0 Sound, 1 Text, 2 Image, 3 Voice.
    pub kind: u8,
    /// TNAM of a text note.
    pub text: Option<String>,
    /// XNAM of an image note: a texture path.
    pub texture: Option<String>,
}

/// A terminal (TERM) [documented: xEdit wbDefinitionsFNV.pas TERM; schema
/// `objects.json`; docs/spec-notes/hacking.md §1].
#[derive(Debug, Clone, Default)]
pub struct TerminalDef {
    pub name: Option<String>,
    /// DESC: the welcome text under the header.
    pub welcome: String,
    /// DNAM "Base Hacking Difficulty": 0 Very Easy … 4 Very Hard, 5 Requires Key
    /// (`hacking::Difficulty::from_code`).
    pub difficulty: u8,
    /// DNAM flags ([`terminal_flag`]).
    pub flags: u8,
    /// DNAM "ServerType": 0 is "-Server 1-" … 9 "-Server 10-".
    pub server: u8,
    /// PNAM: the note that holds the password.
    pub password_note: Option<FormId>,
    pub items: Vec<TerminalItem>,
}

impl TerminalDef {
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// TERM DNAM flag bits [documented: xEdit wbDefinitionsFNV.pas TERM].
pub mod terminal_flag {
    pub const LEVELED: u8 = 0x01;
    pub const UNLOCKED: u8 = 0x02;
    pub const ALTERNATE_COLORS: u8 = 0x04;
    pub const HIDE_WELCOME_WITH_IMAGE: u8 = 0x08;
}

/// One menu item of a terminal: ITXT and what follows it up to the next ITXT.
#[derive(Debug, Clone, Default)]
pub struct TerminalItem {
    /// ITXT: the line in the menu.
    pub text: String,
    /// RNAM: the text shown when it is chosen.
    pub result: String,
    /// ANAM: 1 Add Note, 2 Force Redraw.
    pub flags: u8,
    /// INAM: the note it shows (and gives, with Add Note).
    pub note: Option<FormId>,
    /// TNAM: the terminal it opens as a sub menu.
    pub submenu: Option<FormId>,
    /// CTDA: the item is listed only while these hold.
    pub conditions: Vec<Condition>,
    /// The item's result script.
    pub unit: Option<UnitId>,
}

#[derive(Debug, Clone, Default)]
pub struct ActorDef {
    pub sig: [u8; 4],
    /// SNAM entries: faction and rank. The record's own list: with
    /// [`ActorDef::template_factions`] the actor's factions are its template's
    /// ([`GameData::template_source`]).
    pub factions: Vec<(FormId, i8)>,
    /// ACBS template flag "Factions": the template supplies the factions [documented:
    /// xEdit ACBS template flags, bit 2; GECK wiki GetActorTemplateFlag, "Use Factions"].
    pub template_factions: bool,
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
    /// AIDT energy level (0–100): low-energy actors spend more of a sandbox sitting and
    /// eating, high-energy ones wandering and idling [documented: GECK wiki, Sandbox
    /// Package].
    pub energy: Option<u8>,
    /// Combat (step C5) [documented: xEdit field names; GECK wiki Stats Tab pages]:
    /// NPC_ DATA Base Health (added to the health derived from Endurance and level) or
    /// CREA DATA Health (the creature's health).
    pub base_health: i64,
    /// DATA attributes, S.P.E.C.I.A.L. in that order (NPC_ and CREA) [documented: xEdit;
    /// observed: every NPC_ and CREA of FalloutNV.esm has them, actor-values.md §2].
    pub attributes: Option<[u8; 7]>,
    /// NPC_ DNAM: the fourteen skill values and skill offsets, Barter to Unarmed (actor
    /// value codes 32–45 in order) [documented: xEdit].
    pub skills: Option<[u8; 14]>,
    pub skill_offsets: Option<[u8; 14]>,
    /// CREA DATA Combat Skill and Stealth Skill [documented: xEdit; GECK wiki Stats Tab -
    /// Creatures].
    pub combat_skill: Option<u8>,
    pub stealth_skill: Option<u8>,
    /// CREA DATA Damage, the creature's own attack damage, and RNAM Attack Reach
    /// [documented: xEdit field names].
    pub attack_damage: Option<i64>,
    pub attack_reach: Option<u8>,
    /// ACBS Fatigue, Speed Multiplier (percent) and Karma [documented: xEdit].
    pub fatigue: i64,
    pub speed_mult: i64,
    pub karma: f64,
    /// ACBS Disposition Base [documented: xEdit]: 35 on the Goodsprings people
    /// [observed], the disposition GetDisposition starts from.
    pub disposition_base: i64,
    /// AIDT Aggression, Confidence, Responsibility, Mood and Assistance as their codes
    /// [documented: xEdit enumerations; GECK wiki Confidence for that scale].
    pub aggression: Option<i64>,
    pub confidence: Option<i64>,
    pub responsibility: Option<i64>,
    pub mood: Option<i64>,
    pub assistance: Option<i64>,
    /// ACBS template flag "AI Data": the template supplies the AIDT values.
    pub template_ai: bool,
    /// AIDT "Aggro Radius Behavior" and "Aggro Radius" (game units) [documented: xEdit
    /// field names; the behaviour: `crate::aggro`].
    pub aggro_radius_behavior: bool,
    pub aggro_radius: u32,
    /// ACBS template flags "Traits" (race, sex, voice, class), "Model/Animation",
    /// "Inventory" and "Script" [documented: GECK wiki GetActorTemplateFlag, flag IDs 1,
    /// 64, 256 and 512; xEdit ACBS template flags].
    pub template_traits: bool,
    pub template_model: bool,
    pub template_inventory: bool,
    pub template_script: bool,
    /// ZNAM: the combat style (CSTY) [documented: xEdit NPC_ and CREA ZNAM].
    pub combat_style: Option<FormId>,
    /// NPC_ ACBS "Auto-calc stats".
    pub autocalc: bool,
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
    /// ACBS flag "No Knockdowns" (NPC_ and CREA): PushActorAway and explosions do not
    /// knock the actor down [documented: GECK wiki PushActorAway notes]. Never from the
    /// template: "Base Data does not include … No Knockdowns" [documented: GECK wiki
    /// Category:NPC, Template Data].
    pub no_knockdowns: bool,
    /// CREA ACBS flag "Immobile": PushActorAway does not work on it [documented: GECK
    /// wiki PushActorAway notes, "flagged as immobile"].
    pub immobile: bool,
    /// ACBS flag "No Low Level Processing": the actor's AI is not updated outside the
    /// loaded area [documented: GECK wiki Category:NPC]. With the template flag "Base
    /// Data" the template's counts, as for Essential and Invulnerable (`Game::base_data_def`).
    pub no_low_level: bool,
    /// The record has a DATA whose Base Health (NPC_) or Health (CREA) is 0: its placed
    /// actors start the game as corpses ([`crate::game::Game::starts_dead`]).
    pub zero_health: bool,
    /// ACBS flag "Respawn": "References of this NPC in the world will be recreated after
    /// three days" [documented: GECK wiki Category:NPC, Configuration]; with the template
    /// flag "Base Data" the template's flag counts ([`ActorDef::template_base`]).
    pub respawn: bool,
    /// ACBS template flag "Base Data": the template supplies the base data, the Respawn
    /// flag among it [documented: GECK wiki Category:NPC, Template Data: "Base Data does
    /// not include the Quest Item, No Knockdowns, No VATS Melee, Is CharGen Face Preset,
    /// or Can Be All Races flags"].
    pub template_base: bool,
    /// `EITM` "Unarmed Attack Effect" and `EAMT` "Unarmed Attack Animation": the magic
    /// item (an ENCH or a Poison SPEL) the actor's own unarmed blows apply, and the
    /// attack animation they must be ("Any", "AttackForwardPower", …) [documented: xEdit
    /// field names, schema `gameplay.json`; GECK wiki GetActorUnarmedEffect].
    pub unarmed_effect: Option<FormId>,
    pub unarmed_anim: Option<String>,
    /// ACBS template flag "Actor Effect List": the template supplies the actor effects,
    /// read here as covering the unarmed attack effect too [hypothesis: EITM and EAMT
    /// sit beside SPLO in the record].
    pub template_spells: bool,
    /// SPLO: the actor effects (abilities, mostly) of the record, in record order; with
    /// [`ActorDef::template_spells`] the template's list counts instead.
    pub spells: Vec<FormId>,
}

/// A leveled actor list (LVLN, LVLC) [documented: xEdit; schema `gameplay.json`]: the
/// flag "Calculate from all levels <= player's level" (LVLF) and the entries (LVLO) as
/// (level, actor base or another list), in record order.
#[derive(Debug, Clone, Default)]
pub struct LeveledActorDef {
    pub all_levels: bool,
    pub entries: Vec<(u16, FormId)>,
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
    /// DATA "Actor Value" (`None` for the record's "None").
    pub actor_value: Option<u16>,
    /// DATA "Effect Shader": the EFSH played on the target while the effect lasts
    /// (docs/spec-notes/effect-shaders.md §3).
    pub shader: Option<FormId>,
    /// DATA "Resistance Type": the actor value that resists the effect, a percentage of
    /// its magnitude ignored (`None` for "None") [documented: xEdit field; GECK wiki
    /// Stats Tab - Creatures, "Poison Resist: the percentage of poison damage that is
    /// ignored"; docs/spec-notes/hit-effects.md §3].
    pub resistance: Option<u16>,
}

impl MagicEffectDef {
    /// MGEF DATA flag bits [documented: xEdit MGEF; GECK wiki GetBaseEffectFlag].
    pub const HOSTILE: u32 = 1;
    pub const RECOVER: u32 = 2;
    pub const DETRIMENTAL: u32 = 4;
    pub const NO_DURATION: u32 = 1 << 7;
    pub const NO_MAGNITUDE: u32 = 1 << 8;
    /// "FX Persist": the effect shader's persistent part shows while the effect lasts
    /// [documented: xEdit MGEF flags; GECK wiki EffectShader].
    pub const FX_PERSIST: u32 = 1 << 10;
    /// "No Death Dispel": the effect goes on after its target dies [documented: xEdit
    /// MGEF flags; hypothesis: without it the effect ends at the death,
    /// docs/spec-notes/hit-effects.md §4].
    pub const NO_DEATH_DISPEL: u32 = 1 << 28;

    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// MGEF archetype "Value Modifier" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_VALUE_MODIFIER: i64 = 0;
/// MGEF archetype "Script" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_SCRIPT: i64 = 1;
/// MGEF archetype "Dispel" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_DISPEL: i64 = 2;
/// MGEF archetype "Invisibility" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_INVISIBILITY: i64 = 11;
/// MGEF archetype "Chameleon" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_CHAMELEON: i64 = 12;
/// MGEF archetype "Cure Addiction" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_CURE_ADDICTION: i64 = 31;
/// MGEF archetype "Value And Parts" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_VALUE_AND_PARTS: i64 = 34;
/// MGEF archetype "Limb Condition" [documented: xEdit wbArchtypeEnum].
pub const ARCHETYPE_LIMB_CONDITION: i64 = 35;

/// An ingestible's ENIT and ETYP (ALCH) [documented: xEdit ALCH; schema `objects.json`;
/// GECK wiki Ingestible]. docs/spec-notes/aid-items.md §1.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IngestibleDef {
    pub value: i32,
    /// ENIT flags: 2 Food Item, 4 Medicine.
    pub flags: u8,
    /// The actor effect (SPEL of type Addiction) given to an actor that becomes addicted.
    pub withdrawal: Option<FormId>,
    pub addiction_chance: f32,
    /// The sound played when it is used.
    pub consume_sound: Option<FormId>,
    /// ETYP: the equip type code (10 Chems, 11 Stimpak, 12 Food, 13 Alcohol; -1 none).
    pub equip_type: i64,
}

impl IngestibleDef {
    pub const FOOD: u8 = 2;
    pub const MEDICINE: u8 = 4;
}

/// One effect of a magic item (EFID + EFIT + its CTDA) on ALCH, SPEL, ENCH and INGR
/// [documented: xEdit wbEffects; schema `common.json`].
#[derive(Debug, Clone)]
pub struct EffectItem {
    pub effect: FormId,
    pub magnitude: u32,
    pub area: u32,
    /// Seconds.
    pub duration: u32,
    /// EFIT "Actor Value" (`None` for the record's "None").
    pub actor_value: Option<u16>,
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
    /// The topics with shared lines (INFC / INFX, [`TopicDef::shared_infos`]): their own
    /// lines with the shared ones put in at their Info Index, the order
    /// [`GameData::lines_tried`] starts from (dialogue.md §1.2).
    pub shared_lines: HashMap<FormId, Vec<FormId>>,
    /// Link From turned round: for a topic X, the topics with a line whose Link From
    /// names X, in topic record order ([`TopicDef::order`]). A line of X leads to them as
    /// to its choices, in player dialogue, conversations between actors and on the radio
    /// (`Game::links_after`, dialogue.md §3.5). `ANY`, named by 431 links, has no line,
    /// so nothing follows it.
    pub linked_from: HashMap<FormId, Vec<FormId>>,
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
    /// Terminals (TERM), with their menu items' result scripts.
    pub terminals: HashMap<FormId, TerminalDef>,
    /// Notes (NOTE).
    pub notes: HashMap<FormId, NoteDef>,
    pub form_lists: HashMap<FormId, Vec<FormId>>,
    /// WEAP and ARMO `REPL`: the form list of the items that can repair the item
    /// [documented: xEdit wbREPL, GECK wiki Armor "Repair Item List"]; read raw
    /// (docs/spec-notes/repair.md §5).
    pub repair_lists: HashMap<FormId, FormId>,
    /// MICN (Menu Icon) ICON: the icon's texture file as the record names it, under
    /// the textures folder [documented: GECK wiki, MenuIcon; corner-messages.md §3].
    pub menu_icons: HashMap<FormId, String>,
    pub actors: HashMap<FormId, ActorDef>,
    /// Combat styles (CSTY), the values the combat AI uses ([`CombatStyleDef`]).
    pub combat_styles: HashMap<FormId, CombatStyleDef>,
    /// Leveled actor lists (LVLN, LVLC): what a placed leveled actor or a `TPLT` naming a
    /// list stands for ([`GameData::template_source`]).
    pub leveled_actors: HashMap<FormId, LeveledActorDef>,
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
    /// REPU FULL: the reputation's name as the player sees it.
    pub reputation_names: HashMap<FormId, String>,
    /// CHAL DATA flags (1 Start Disabled, 2 Recurring, 4 Show Zero Progress).
    pub challenges: HashMap<FormId, u32>,
    /// CHAL DATA threshold: the progress at which the challenge is completed.
    pub challenge_thresholds: HashMap<FormId, u32>,
    pub regions: HashMap<FormId, RegionDef>,
    pub magic_effects: HashMap<FormId, MagicEffectDef>,
    /// The effects of every ALCH, SPEL, ENCH and INGR, in record order.
    pub effect_items: HashMap<FormId, Vec<EffectItem>>,
    /// ALCH: what using the item needs besides its effects ([`IngestibleDef`]).
    pub ingestibles: HashMap<FormId, IngestibleDef>,
    /// SPEL SPIT type (0 Actor Effect, 1 Disease, 2 Power, 3 Lesser Power, 4 Ability,
    /// 5 Poison, 10 Addiction) [documented: xEdit wbDefinitionsFNV.pas SPEL].
    pub spell_types: HashMap<FormId, i64>,
    /// RACE SPLO: the race's actor effects [hypothesis: as NPC_'s; none in FalloutNV.esm,
    /// observed].
    pub race_spells: HashMap<FormId, Vec<FormId>>,
    /// ARMO EITM: the object effect (ENCH) an armour piece carries.
    pub armor_enchantments: HashMap<FormId, FormId>,
    /// ENCH ENIT type: 2 Weapon (applied by a hit), 3 Apparel (applied while worn).
    pub enchantment_types: HashMap<FormId, u32>,
    /// Races with the Child flag (RACE DATA).
    pub child_races: HashSet<FormId>,
    /// RACE ONAM and YNAM: the older and the younger race of a race (AgeRace)
    /// [documented: xEdit wbDefinitionsFNV.pas RACE "Older", "Younger"].
    pub race_older: HashMap<FormId, FormId>,
    pub race_younger: HashMap<FormId, FormId>,
    /// FACT XNAM: (other faction, group combat reaction 0 neutral, 1 enemy, 2 ally,
    /// 3 friend).
    pub faction_reactions: HashMap<FormId, Vec<(FormId, i64)>>,
    /// FACT DATA flags as one number: the first byte's bits (1 Hidden from PC, 2 Evil,
    /// 4 Special Combat) and the second's shifted by 8 (256 Track Crime, 512 Allow Sell)
    /// [documented: xEdit wbDefinitionsFNV.pas FACT, schema `gameplay.json`; the numbering
    /// is the GECK wiki GetFactionFlags'] (docs/spec-notes/crime-karma.md §2).
    pub faction_flags: HashMap<FormId, u16>,
    /// FACT WMI1: the reputation a faction's members count towards [documented: xEdit
    /// wbDefinitionsFNV.pas FACT "Reputation", schema `gameplay.json`; observed:
    /// GoodspringsFaction 00104C6E names RepNVGoodsprings 00104C22].
    pub faction_reputations: HashMap<FormId, FormId>,
    /// Cells with an owner, the Public place flag or the Off Limits form flag
    /// (`crate::crime::CellDef`).
    pub cells: HashMap<FormId, crate::crime::CellDef>,
    /// XRNK of placed references: the faction rank that owns them (with an XOWN faction).
    pub owner_ranks: HashMap<FormId, i32>,
    pub destructibles: HashMap<FormId, DestructionDef>,
    /// WEAP DNAM skill (actor value code).
    pub weapon_skills: HashMap<FormId, i64>,
    /// WEAP DNAM animation type (the record's own numbering, 0 Hand to Hand … 13
    /// Thrown) [documented: xEdit wbWeaponAnimTypeEnum, schemas/fnv/objects.json].
    pub weapon_anim_types: HashMap<FormId, i64>,
    /// FURN marker flags (MNAM as a little-endian number; 0 when absent): bit 30
    /// furniture to sit on, bit 31 to sleep on [observed: docs/spec-notes/furniture.md
    /// §1].
    pub furniture: HashMap<FormId, u32>,
    /// IDLM: the idle collection of each idle marker base.
    pub idle_markers: HashMap<FormId, crate::package::IdleCollection>,
    /// IDLE: what playing an idle animation needs (`crate::idles`).
    pub idles: HashMap<FormId, IdleDef>,
    /// The idle tree: each IDLE's children (the roots under `None`), siblings in the
    /// order their `previous` links chain them (animation.md §2.1).
    pub idle_children: HashMap<Option<FormId>, Vec<FormId>>,
    /// IMAD: how long each image space modifier plays.
    pub image_modifiers: HashMap<FormId, ImageModifierDef>,
    /// TACT: the record-header flags of every talking activator (radio stations: the
    /// Continuous Broadcast bit, `crate::radio`; docs/spec-notes/radio.md §2).
    pub station_flags: HashMap<FormId, u32>,
    /// PERK: perks and traits ([`PerkDef`]).
    pub perks: HashMap<FormId, PerkDef>,
    /// AVIF: the name, short name, description and icon of each actor value
    /// ([`GameData::actor_value_info`]).
    pub actor_value_infos: HashMap<FormId, ActorValueInfo>,
    /// RCPE: the recipes (`crate::crafting`).
    pub recipes: HashMap<FormId, crate::crafting::RecipeDef>,
    /// RCCT: the recipe categories and subcategories.
    pub recipe_categories: HashMap<FormId, crate::crafting::RecipeCategoryDef>,
    /// CSNO: the casinos (`crate::casino`).
    pub casinos: HashMap<FormId, crate::casino::CasinoDef>,
    /// CCRD: the Caravan cards (`crate::caravan`).
    pub caravan_cards: HashMap<FormId, crate::caravan::CardDef>,
    /// CDCK: the Caravan decks.
    pub caravan_decks: HashMap<FormId, crate::caravan::DeckDef>,
    /// RADS, DEHY, HUNG, SLPD: the stages of rads and of Hardcore's needs by actor value
    /// code, lowest threshold first (`crate::needs`).
    pub need_stages: HashMap<u16, Vec<NeedStage>>,
    /// WATR XNAM: each water type's actor effect (`crate::radiation`).
    pub water_effects: HashMap<FormId, FormId>,
    /// The radiation markers (placed references with XRAD) by the space they are in: a
    /// worldspace, or an interior cell (`crate::radiation`).
    pub radiation_markers: HashMap<FormId, Vec<crate::radiation::RadMarker>>,
    pub issues: Issues,
    pub read_issues: ReadIssues,
}

/// An idle animation (IDLE) as far as playing it goes: the file it names and how often
/// it repeats. The tree and the conditions are read by `fnv_actor::idle`
/// (docs/spec-notes/animation.md §1.1).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct IdleDef {
    /// `MODL` as stored, relative to `meshes\`; only a name ending in `.kf` is an
    /// animation file (other records name a folder: the inner nodes of the tree).
    pub model: Option<String>,
    /// DATA loop minimum and maximum [documented: xEdit IDLE DATA].
    pub loop_min: u8,
    pub loop_max: u8,
    /// The record's conditions; `None` when one of them could not be read (the idle is
    /// then never chosen by its conditions, `Game::idle_allowed`).
    pub conditions: Option<Vec<Condition>>,
    /// ANAM: the parent and the previous sibling in the idle tree [documented: xEdit
    /// IDLE ANAM; animation.md §2.1].
    pub parent: Option<FormId>,
    pub previous: Option<FormId>,
    /// DATA replay delay (i16) [documented: xEdit IDLE DATA].
    pub replay_delay: i16,
}

impl IdleDef {
    /// From the raw `MODL` (a NUL-terminated name) and `DATA` (animation group section,
    /// loop minimum, loop maximum, … [documented: xEdit IDLE DATA]) subrecords.
    pub fn from_raw(modl: Option<&[u8]>, data: Option<&[u8]>) -> IdleDef {
        let model = modl
            .map(|b| String::from_utf8_lossy(b.split(|c| *c == 0).next().unwrap_or_default()).trim().to_string())
            .filter(|s| !s.is_empty());
        let byte = |i: usize| data.and_then(|d| d.get(i)).copied().unwrap_or(0);
        let replay_delay = i16::from_le_bytes([byte(4), byte(5)]);
        IdleDef { model, loop_min: byte(1), loop_max: byte(2), conditions: Some(Vec::new()), parent: None, previous: None, replay_delay }
    }

    /// Whether the record names an animation file.
    pub fn has_file(&self) -> bool {
        self.model.as_deref().is_some_and(|m| m.to_ascii_lowercase().ends_with(".kf"))
    }
}

/// The children of each IDLE (roots under `None`). Siblings follow their `previous`
/// chain: first a sibling whose previous is not in the group, then the one naming it,
/// and so on; siblings outside any chain come last in FormID order [observed: 357 of 364
/// sibling groups form one chain, animation.md §2.1].
pub fn idle_children(idles: &HashMap<FormId, IdleDef>) -> HashMap<Option<FormId>, Vec<FormId>> {
    let mut groups: HashMap<Option<FormId>, Vec<FormId>> = HashMap::new();
    for (id, d) in idles {
        groups.entry(d.parent).or_default().push(*id);
    }
    for list in groups.values_mut() {
        list.sort();
        let members: std::collections::HashSet<FormId> = list.iter().copied().collect();
        let mut after: HashMap<FormId, FormId> = HashMap::new();
        for id in list.iter() {
            if let Some(p) = idles[id].previous.filter(|p| members.contains(p)) {
                after.entry(p).or_insert(*id);
            }
        }
        let mut ordered = Vec::with_capacity(list.len());
        let mut seen = std::collections::HashSet::new();
        for first in list.iter().filter(|id| idles[id].previous.is_none_or(|p| !members.contains(&p))) {
            let mut at = Some(*first);
            while let Some(id) = at.filter(|id| seen.insert(*id)) {
                ordered.push(id);
                at = after.get(&id).copied();
            }
        }
        ordered.extend(list.iter().filter(|id| !seen.contains(id)).copied());
        *list = ordered;
    }
    groups
}

/// A stage of rads or of a Hardcore need (RADS, DEHY, HUNG, SLPD).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeedStage {
    /// The stage record.
    pub stage: FormId,
    /// The value from which it holds.
    pub threshold: u32,
    /// Its actor effect (SPEL).
    pub spell: FormId,
}

/// A perk or trait (PERK) as the character menus need it.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct PerkDef {
    /// FULL, DESC, ICON (under the textures folder).
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    /// DATA [documented: xEdit wbDefinitionsFNV.pas PERK DATA: Trait, Min Level, Ranks,
    /// Playable, Hidden, one byte each; observed: five bytes on every perk of the
    /// masters, `01 01 01 01 00` on each of the sixteen traits, docs/spec-notes/chargen.md
    /// §2].
    pub is_trait: bool,
    pub min_level: u8,
    pub ranks: u8,
    pub playable: bool,
    pub hidden: bool,
    /// The perk's requirements: the CTDA before its first effect; `None` when one of
    /// them could not be read.
    #[serde(skip)]
    pub conditions: Option<Vec<Condition>>,
    /// Its effects (PRKE … PRKF), in record order (`crate::perks`).
    #[serde(skip)]
    pub entries: Vec<PerkEntry>,
}

/// One effect of a perk (PRKE header, then its DATA and parameters, up to PRKF)
/// [documented: xEdit wbDefinitionsFNV.pas PERK, the header's type, rank and priority,
/// `schemas/fnv/gameplay.json`; observed: the DATA and EPFD layouts, docs/spec-notes/
/// aid-items.md §10].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PerkEntry {
    /// The perk rank it belongs to, 0 for the first.
    pub rank: u8,
    pub priority: u8,
    pub effect: PerkEffect,
    /// Its place among the record's effects (PRKE), 0-based: the script unit of an
    /// entry is keyed by it (`fnv_script::unit::UnitOwner::Perk`).
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum PerkEffect {
    /// Type 1: the spell is the owner's while it has the perk.
    Ability(FormId),
    /// Type 2.
    EntryPoint(EntryPointDef),
    /// Type 0: the quest is set to the stage when the rank is gained (`crate::perks::
    /// gain_rank`) [documented: xEdit wbDefinitionsFNV.pas PERK effect type "Quest +
    /// Stage"; observed: DATA is the quest's FormID then the stage in one byte, three
    /// bytes after it unused, Intense Training's `b2380000 65 cdcdcd` for Generic stage
    /// 101, whose result script reads "triggered by Intense Training perk rank 1"].
    QuestStage { quest: FormId, stage: u8 },
    /// A DATA that could not be read.
    #[default]
    Other,
}

/// An entry point effect: which entry point, which function, its number, and its
/// conditions by tab.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntryPointDef {
    /// DATA byte 0 [observed: the code identifies the entry point, e.g. 23 on Chem
    /// Resistant alone, aid-items.md §10].
    pub point: u8,
    /// DATA byte 1 [observed: 1 with 0/1 flags (Light Step, Computer Whiz), 2 with
    /// additions (Educated +2, Toughness +3), 3 with factors (Chem Resistant 0.5, Chemist
    /// 2.0): hypothesis Set, Add, Multiply].
    pub function: u8,
    /// EPFD as one float when EPFT is 1 [observed: Chem Resistant `0000003F` = 0.5].
    pub value: Option<f32>,
    /// EPFT, the kind of EPFD [documented: xEdit wbDefinitionsFNV.pas PERK, EPFT
    /// "Entry Point Function Parameter Type"; observed: 1 with one float on every Set,
    /// Add and Multiply entry of the masters, 3 with a leveled list on the functions 8, 4
    /// with a script on the functions 9].
    pub param_type: Option<u8>,
    /// EPFD as read, for the functions whose parameters are not one float (Add Range To
    /// Value, the actor value functions; `crate::perks::resolve`) [hypothesis: their
    /// layout; no entry of the masters has one].
    pub raw: Vec<u8>,
    /// The CTDA with the PRKC tab before them (0 the perk owner); `None` when one could
    /// not be read.
    pub conditions: Option<Vec<(i8, Condition)>>,
    /// EPF2 "Button Label" of an activate choice ("Devour", "Murder") [documented: xEdit
    /// wbDefinitionsFNV.pas PERK; observed: the four entries of the masters with one,
    /// `crate::activate_choices`].
    pub button: Option<String>,
    /// EPF3 "Script Flags" bit 0, Run Immediately [documented: xEdit].
    pub run_immediately: bool,
    /// The entry's script (EPFT 4, SCHR/SCDA after it).
    pub unit: Option<UnitId>,
}

impl PerkDef {
    /// The fields of the raw DATA (see the field notes); a missing byte reads 0.
    pub fn from_data(data: Option<&[u8]>) -> PerkDef {
        let byte = |i: usize| data.and_then(|d| d.get(i)).copied().unwrap_or(0);
        PerkDef { is_trait: byte(0) != 0, min_level: byte(1), ranks: byte(2), playable: byte(3) != 0, hidden: byte(4) != 0, ..Default::default() }
    }
}

/// An actor value's AVIF record: its FULL name, ANAM short name ("PER") [documented:
/// xEdit wbDefinitionsFNV.pas AVIF "Short Name"], DESC and ICON.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct ActorValueInfo {
    pub name: String,
    pub short_name: String,
    pub description: String,
    pub icon: Option<String>,
}

/// A NUL-terminated string as stored.
fn zstring(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|c| *c == 0).next().unwrap_or_default()).trim().to_string()
}

/// The timing of an image space modifier (IMAD DNAM) [documented: xEdit
/// wbDefinitionsFNV.pas IMAD DNAM "Animatable" u32, "Duration" f32;
/// docs/spec-notes/image-modifiers.md §1].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageModifierDef {
    /// The modifier plays over `duration` seconds and then ends; without the flag it
    /// stays until it is removed.
    pub animatable: bool,
    pub duration: f32,
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

/// A combat style (CSTY) [documented: layout xEdit wbDefinitionsFNV.pas CSTY, schema
/// `ai.json`; meanings GECK wiki Combat Style, owner export]: the values the combat AI
/// uses (docs/design/combat-ai.md §10.4). Percentages as the record has them (0–100).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CombatStyleDef {
    /// Melee decision: base chances to block and to attack; bonuses to the attack chance
    /// when the actor fights hand to hand.
    pub block_chance: f32,
    pub attack_chance: f32,
    pub hand_to_hand_bonus: f32,
    pub power_attack_chance: f32,
    /// Power Attacks: the shares (0–100) of the normal, forward, back, left and right
    /// power attacks [documented: GECK wiki Combat Style; layout xEdit CSTD].
    pub power_attack_dirs: [f32; 5],
    /// Power Attacks: the bonuses to the power attack chance while the opponent recoils
    /// or staggers and while it is unconscious; the bonus to the attack chance while it
    /// is unconscious; flag "Choose Attack using % Chance" (else by movement)
    /// [documented: GECK wiki Combat Style; layout xEdit CSTD].
    pub recoil_stagger_power_bonus: f32,
    pub unconscious_power_bonus: f32,
    pub unconscious_bonus: f32,
    pub power_by_chance: bool,
    /// Melee decision: how long an actor that neither attacks nor blocks holds, seconds.
    pub hold_timer: (f32, f32),
    /// Advanced: the attack skill modifier (`base + mult × skill / 100`) and the attack
    /// chance's multipliers under attack and not.
    pub attack_skill_mod: (f32, f32),
    pub attack_under_attack_mult: f32,
    pub attack_not_under_attack_mult: f32,
    /// Melee decision: the bonus to the attack chance while the opponent recoils or
    /// staggers; Advanced: the block skill modifier (`base + mult × 50 / 100`), the block
    /// chance's multipliers under attack and not, and the attack chance's multiplier while
    /// the opponent blocks [documented: GECK wiki Combat Style].
    pub recoil_stagger_bonus: f32,
    pub block_skill_mod: (f32, f32),
    pub block_under_attack_mult: f32,
    pub block_not_under_attack_mult: f32,
    pub attack_during_block_mult: f32,
    /// Simple: what the actor may fight with: 0 None, 1 Melee Only, 2 Ranged Only.
    pub weapon_restrictions: u32,
    /// Simple: multipliers of the weapon's min and max range in range calculations.
    pub range_mult: (f32, f32),
    /// Simple: multipliers of the weapon's semi-automatic firing delays.
    pub semi_auto_delay_mult: (f32, f32),
    pub max_targeting_fov: f32,
    pub combat_radius: f32,
    /// Flag "Flee Based on Personal Survival".
    pub flee_personal_survival: bool,
    /// Simple, cover: how far cover is looked for; the chance (0–100) to use cover for the
    /// next 10 seconds; how long to stay behind it, to stand out of it looking for targets,
    /// and to fire out of it once they are seen, seconds (min, max) [documented: GECK wiki
    /// Combat Style].
    pub cover_search_radius: f32,
    pub take_cover_chance: f32,
    pub wait_timer: (f32, f32),
    pub wait_to_fire_timer: (f32, f32),
    pub fire_timer: (f32, f32),
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
    terminal: Option<TerminalDef>,
    note: Option<NoteDef>,
    form_list: Option<Vec<FormId>>,
    menu_icon: Option<String>,
    actor: Option<ActorDef>,
    combat_style: Option<CombatStyleDef>,
    leveled_actor: Option<LeveledActorDef>,
    package: Option<PackageDef>,
    reputation: Option<f32>,
    reputation_name: Option<String>,
    /// CHAL DATA flags and threshold.
    challenge: Option<(u32, u32)>,
    region: Option<RegionDef>,
    magic_effect: Option<MagicEffectDef>,
    effects: Option<Vec<EffectItem>>,
    ingestible: Option<IngestibleDef>,
    spell_type: Option<i64>,
    /// RACE SPLO list; ARMO EITM; ENCH ENIT type (`GameData::race_spells` …).
    race_spells: Option<Vec<FormId>>,
    object_effect: Option<FormId>,
    repair_list: Option<FormId>,
    enchantment_type: Option<u32>,
    child_race: bool,
    race_older: Option<FormId>,
    race_younger: Option<FormId>,
    faction: Option<Vec<(FormId, i64)>>,
    faction_flags: Option<u16>,
    faction_reputation: Option<FormId>,
    cell: Option<crate::crime::CellDef>,
    owner_rank: Option<i32>,
    destruction: Option<DestructionDef>,
    weapon_skill: Option<i64>,
    weapon_anim_type: Option<i64>,
    furniture: Option<u32>,
    idle_marker: Option<crate::package::IdleCollection>,
    idle: Option<IdleDef>,
    image_modifier: Option<ImageModifierDef>,
    perk: Option<PerkDef>,
    av_info: Option<ActorValueInfo>,
    recipe: Option<crate::crafting::RecipeDef>,
    recipe_category: Option<crate::crafting::RecipeCategoryDef>,
    casino: Option<crate::casino::CasinoDef>,
    caravan_card: Option<crate::caravan::CardDef>,
    caravan_deck: Option<crate::caravan::DeckDef>,
    need_stage: Option<(u16, u32, FormId)>,
    water_effect: Option<FormId>,
    units: Vec<ScriptUnit>,
    issues: Issues,
    read_issues: ReadIssues,
}

const UNIT_SIGS: &[&[u8; 4]] = &[b"SCPT", b"QUST", b"INFO", b"PACK", b"TERM", b"PERK"];

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
            shared_lines: HashMap::new(),
            linked_from: HashMap::new(),
            topic_defs: HashMap::new(),
            top_level: Vec::new(),
            greeting: None,
            goodbye: None,
            packages: HashMap::new(),
            package_defs: HashMap::new(),
            messages: HashMap::new(),
            terminals: HashMap::new(),
            notes: HashMap::new(),
            form_lists: HashMap::new(),
            repair_lists: HashMap::new(),
            menu_icons: HashMap::new(),
            actors: HashMap::new(),
            combat_styles: HashMap::new(),
            leveled_actors: HashMap::new(),
            form_script: HashMap::new(),
            edids: HashMap::new(),
            edid_of: HashMap::new(),
            sigs: HashMap::new(),
            scripted_by_cell: BTreeMap::new(),
            reputations: HashMap::new(),
            reputation_names: HashMap::new(),
            challenges: HashMap::new(),
            challenge_thresholds: HashMap::new(),
            regions: HashMap::new(),
            magic_effects: HashMap::new(),
            effect_items: HashMap::new(),
            ingestibles: HashMap::new(),
            spell_types: HashMap::new(),
            race_spells: HashMap::new(),
            armor_enchantments: HashMap::new(),
            enchantment_types: HashMap::new(),
            child_races: HashSet::new(),
            race_older: HashMap::new(),
            race_younger: HashMap::new(),
            faction_reactions: HashMap::new(),
            faction_flags: HashMap::new(),
            faction_reputations: HashMap::new(),
            cells: HashMap::new(),
            owner_ranks: HashMap::new(),
            destructibles: HashMap::new(),
            weapon_skills: HashMap::new(),
            weapon_anim_types: HashMap::new(),
            furniture: HashMap::new(),
            idle_markers: HashMap::new(),
            idles: HashMap::new(),
            idle_children: HashMap::new(),
            image_modifiers: HashMap::new(),
            station_flags: winners.iter().filter(|w| &w.2.header.sig == b"TACT").map(|w| (FormId(w.0), w.2.header.flags)).collect(),
            perks: HashMap::new(),
            actor_value_infos: HashMap::new(),
            recipes: HashMap::new(),
            recipe_categories: HashMap::new(),
            casinos: HashMap::new(),
            caravan_cards: HashMap::new(),
            caravan_decks: HashMap::new(),
            need_stages: HashMap::new(),
            water_effects: HashMap::new(),
            radiation_markers: crate::radiation::markers(refs),
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
                b"TERM" => {
                    if let Some(mut t) = r.terminal {
                        for (n, item) in t.items.iter_mut().enumerate() {
                            item.unit = unit_of
                                .get(&label(UnitOwner::Terminal { form: id.0, item: n as u32 }))
                                .copied()
                                .filter(|&u| d.units[u].unit.has_scda);
                        }
                        d.terminals.insert(id, t);
                    }
                }
                _ => {}
            }
            if let Some(mut t) = r.topic {
                t.form = id;
                d.topic_defs.insert(id, t);
            }
            if let Some(n) = r.note {
                d.notes.insert(id, n);
            }
            if let Some(m) = r.message {
                d.messages.insert(id, m);
            }
            if let Some(l) = r.form_list {
                d.form_lists.insert(id, l);
            }
            if let Some(i) = r.menu_icon {
                d.menu_icons.insert(id, i);
            }
            if let Some(a) = r.actor {
                d.actors.insert(id, a);
            }
            if let Some(c) = r.combat_style {
                d.combat_styles.insert(id, c);
            }
            if let Some(l) = r.leveled_actor {
                d.leveled_actors.insert(id, l);
            }
            if let Some(p) = r.package {
                d.package_defs.insert(id, p);
            }
            if let Some(v) = r.reputation {
                d.reputations.insert(id, v);
            }
            if let Some(v) = r.reputation_name {
                d.reputation_names.insert(id, v);
            }
            if let Some((flags, threshold)) = r.challenge {
                d.challenges.insert(id, flags);
                d.challenge_thresholds.insert(id, threshold);
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
            if let Some(v) = r.ingestible {
                d.ingestibles.insert(id, v);
            }
            if let Some(v) = r.spell_type {
                d.spell_types.insert(id, v);
            }
            if let Some(v) = r.race_spells.filter(|v| !v.is_empty()) {
                d.race_spells.insert(id, v);
            }
            if let Some(v) = r.object_effect {
                d.armor_enchantments.insert(id, v);
            }
            if let Some(v) = r.repair_list {
                d.repair_lists.insert(id, v);
            }
            if let Some(v) = r.enchantment_type {
                d.enchantment_types.insert(id, v);
            }
            if r.child_race {
                d.child_races.insert(id);
            }
            if let Some(v) = r.race_older {
                d.race_older.insert(id, v);
            }
            if let Some(v) = r.race_younger {
                d.race_younger.insert(id, v);
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
            if let Some(v) = r.weapon_anim_type {
                d.weapon_anim_types.insert(id, v);
            }
            if let Some(v) = r.furniture {
                d.furniture.insert(id, v);
            }
            if let Some(v) = r.idle_marker {
                d.idle_markers.insert(id, v);
            }
            if let Some(v) = r.idle {
                d.idles.insert(id, v);
            }
            if let Some(v) = r.image_modifier {
                d.image_modifiers.insert(id, v);
            }
            if let Some(mut v) = r.perk {
                // An entry's script (an activate choice's, `crate::activate_choices`).
                for e in &mut v.entries {
                    if let PerkEffect::EntryPoint(ep) = &mut e.effect {
                        ep.unit = unit_of.get(&label(UnitOwner::Perk { form: id.0, entry: e.ordinal })).copied().filter(|&u| d.units[u].unit.has_scda);
                    }
                }
                d.perks.insert(id, v);
            }
            if let Some(v) = r.av_info {
                d.actor_value_infos.insert(id, v);
            }
            if let Some(v) = r.recipe {
                d.recipes.insert(id, v);
            }
            if let Some(v) = r.recipe_category {
                d.recipe_categories.insert(id, v);
            }
            if let Some(v) = r.casino {
                d.casinos.insert(id, v);
            }
            if let Some(v) = r.caravan_card {
                d.caravan_cards.insert(id, v);
            }
            if let Some(v) = r.caravan_deck {
                d.caravan_decks.insert(id, v);
            }
            if let Some(v) = r.faction_flags {
                d.faction_flags.insert(id, v);
            }
            if let Some(v) = r.faction_reputation {
                d.faction_reputations.insert(id, v);
            }
            if let Some(v) = r.cell.filter(crate::crime::CellDef::matters) {
                d.cells.insert(id, v);
            }
            if let Some(v) = r.owner_rank {
                d.owner_ranks.insert(id, v);
            }
            if let Some((code, threshold, spell)) = r.need_stage {
                d.need_stages.entry(code).or_default().push(NeedStage { stage: id, threshold, spell });
            }
            if let Some(spell) = r.water_effect {
                d.water_effects.insert(id, spell);
            }
        }
        for stages in d.need_stages.values_mut() {
            stages.sort_by_key(|s| (s.threshold, s.stage));
        }
        d.idle_children = idle_children(&d.idles);
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
        d.shared_lines = shared_lines(&d.topics, &d.topic_defs, &d.infos);
        d.linked_from = linked_from(&d.topic_defs, &d.infos);
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

    /// The AVIF record of a S.P.E.C.I.A.L. or skill code. The masters' AVIF FormIDs run
    /// in code order: 0x3E8–0x3EE for Strength to Luck, 0x4B0–0x4BD for Barter to
    /// Unarmed [observed: each record's name is its code's, Guns under the Editor ID
    /// AVSmallGuns and Survival under AVThrowing, docs/spec-notes/chargen.md §2].
    pub fn actor_value_info(&self, code: u16) -> Option<&ActorValueInfo> {
        use crate::actor_values::{BARTER, SKILLS, SPECIALS, STRENGTH};
        let id = if SPECIALS.contains(&code) {
            0x3E8 + u32::from(code - STRENGTH)
        } else if SKILLS.contains(&code) {
            0x4B0 + u32::from(code - BARTER)
        } else {
            return None;
        };
        self.actor_value_infos.get(&FormId(id))
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

    /// The AI package list of `reference`, placed with `base` (NPC_, CREA, or a leveled
    /// actor list): the `PKID` list of the "AI Packages" source of its template chain,
    /// through leveled lists with the same pick as the drawn actor
    /// ([`GameData::template_source`]; [`crate::game::Game::package_list`]). For a base
    /// alone, pass the base as the reference.
    pub fn base_package_list(&self, base: FormId, reference: FormId) -> Vec<FormId> {
        self.template_source(base, reference, |d| d.template_packages).map(|(_, d)| d.packages.clone()).unwrap_or_default()
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

    /// The lines an actor can say for `topic`, in the order they are tried: its own lines
    /// and the lines it shares from other topics (INFC / INFX, [`GameData::shared_lines`])
    /// [observed: Red Lucy's choice "What's this place?" (VMS38VMS38RedLucyTopic047) has
    /// no line of its own and shares the answer of the topic of the same name; of the 257
    /// player lines whose choices all lack lines of their own, 250 get a choice with
    /// lines this way, dialogue.md §1.2], sorted as [`GameData::lines_by_priority`].
    pub fn lines_tried(&self, topic: FormId) -> Vec<FormId> {
        let Some(merged) = self.shared_lines.get(&topic) else { return self.lines_by_priority(topic) };
        let mut lines = merged.clone();
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

/// [`GameData::shared_lines`]: for each topic whose INFC / INFX pairs name a line, its
/// own lines with each shared line put in at its Info Index (pairs in index order; an
/// index past the end, or negative, puts it last) [hypothesis: the index is the line's
/// place in the topic's list; in topics with no line of their own the indices run
/// 0, 1, 2 … as a list's would, observed, dialogue.md §1.2]. A line already in
/// the list is not added twice.
fn shared_lines(topics: &HashMap<FormId, Vec<FormId>>, defs: &HashMap<FormId, TopicDef>, infos: &HashMap<FormId, InfoDef>) -> HashMap<FormId, Vec<FormId>> {
    let mut out = HashMap::new();
    for (id, t) in defs {
        let mut pairs: Vec<(i32, FormId)> =
            t.shared_infos.iter().filter_map(|s| s.info.filter(|i| infos.contains_key(i)).map(|i| (s.index.unwrap_or(-1), i))).collect();
        if pairs.is_empty() {
            continue;
        }
        pairs.sort_by_key(|&(x, _)| if x < 0 { i32::MAX } else { x });
        let mut lines = topics.get(id).cloned().unwrap_or_default();
        for (x, i) in pairs {
            if lines.contains(&i) {
                continue;
            }
            let at = usize::try_from(x).map_or(lines.len(), |x| x.min(lines.len()));
            lines.insert(at, i);
        }
        out.insert(*id, lines);
    }
    out
}

/// [`GameData::linked_from`]: each line with a Link From list puts its topic under
/// every topic the list names, once, in the topics' record order.
fn linked_from(defs: &HashMap<FormId, TopicDef>, infos: &HashMap<FormId, InfoDef>) -> HashMap<FormId, Vec<FormId>> {
    let mut out: HashMap<FormId, Vec<FormId>> = HashMap::new();
    for i in infos.values().filter(|i| !i.link_from.is_empty()) {
        let Some(t) = i.topic else { continue };
        for &x in &i.link_from {
            let list = out.entry(x).or_default();
            if !list.contains(&t) {
                list.push(t);
            }
        }
    }
    let order = |t: &FormId| defs.get(t).map_or((usize::MAX, 0), |d| d.order);
    for list in out.values_mut() {
        list.sort_by_key(|t| (order(t), t.0));
    }
    out
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
            | b"TERM"
            | b"NOTE"
            | b"MICN"
            | b"FLST"
            | b"NPC_"
            | b"CREA"
            | b"LVLN"
            | b"LVLC"
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
            | b"IDLM"
            | b"IDLE"
            | b"PERK"
            | b"AVIF"
            | b"CSTY"
    ) || subs.iter().any(|s| &s.sig == b"DEST");
    // Abilities and worn enchantments (docs/spec-notes/aid-items.md §13), read raw: the
    // schema has no ARMO EITM or RACE SPLO. ENCH ENIT's first u32 is the type, 2 Weapon,
    // 3 Apparel [documented: schema `combat.json` EnchantmentData]; ARMO EITM names the
    // armour's object effect [documented: GECK wiki ActorValue, "object effects found on
    // armor"; observed: 186 ARMO of FalloutNV.esm carry a 4-byte EITM naming an ENCH of
    // type Apparel]; RACE SPLO is read as NPC_'s [hypothesis: FalloutNV.esm's races carry
    // none, observed].
    let word = |b: &[u8]| <[u8; 4]>::try_from(b.get(..4)?).ok().map(u32::from_le_bytes);
    match &sig {
        b"RACE" => out.race_spells = Some(subs.iter().filter(|s| &s.sig == b"SPLO").filter_map(|s| word(&s.data).and_then(map)).map(FormId).collect()),
        b"ARMO" => out.object_effect = subs.iter().find(|s| &s.sig == b"EITM").and_then(|s| word(&s.data)).filter(|&l| l != 0).and_then(map).map(FormId),
        b"ENCH" => out.enchantment_type = subs.iter().find(|s| &s.sig == b"ENIT").and_then(|s| word(&s.data)),
        _ => {}
    }
    if matches!(&sig, b"WEAP" | b"ARMO") {
        out.repair_list = subs.iter().find(|s| &s.sig == b"REPL").and_then(|s| word(&s.data)).filter(|&l| l != 0).and_then(map).map(FormId);
    }
    if &sig == b"FURN" {
        let flags = subs.iter().find(|s| &s.sig == b"MNAM").and_then(|s| <[u8; 4]>::try_from(s.data.as_slice()).ok()).map(u32::from_le_bytes);
        out.furniture = Some(flags.unwrap_or(0));
    }
    if &sig == b"IDLE" {
        out.idle = Some(IdleDef::from_raw(
            subs.iter().find(|s| &s.sig == b"MODL").map(|s| s.data.as_slice()),
            subs.iter().find(|s| &s.sig == b"DATA").map(|s| s.data.as_slice()),
        ));
    }
    // A stage of a need: the value from which it holds, and its actor effect [observed:
    // DATA is 8 bytes, a u32 of 200, 400 … 1000 and a SPEL FormID named for that number
    // (Water1: 200, Dehydration200), docs/spec-notes/aid-items.md §11].
    let need = match &sig {
        b"RADS" => Some(crate::aid::RAD_LEVEL),
        b"DEHY" => Some(crate::aid::DEHYDRATION),
        b"HUNG" => Some(crate::aid::HUNGER),
        b"SLPD" => Some(crate::aid::SLEEP_DEPRIVATION),
        _ => None,
    };
    if let Some(code) = need {
        let data = subs.iter().find(|s| &s.sig == b"DATA").map(|s| s.data.as_slice()).unwrap_or_default();
        let word = |o: usize| data.get(o..o + 4).and_then(|b| <[u8; 4]>::try_from(b).ok()).map(u32::from_le_bytes);
        if let (Some(threshold), Some(spell)) = (word(0), word(4).and_then(map)) {
            out.need_stage = Some((code, threshold, FormId(spell)));
        }
    }
    // A water type's actor effect: XNAM, a SPEL [documented: xEdit wbDefinitionsFNV.pas
    // WATR "Actor Effect"; observed: FalloutNV.esm's waters name WaterHeal1Purified …
    // WaterHeal5Terrible, the effects of drinking them]. docs/spec-notes/radiation.md §3.
    if &sig == b"WATR" {
        out.water_effect = subs.iter().find(|s| &s.sig == b"XNAM").and_then(|s| word(&s.data)).filter(|&l| l != 0).and_then(map).map(FormId);
    }
    // Recipes and their categories (`crate::crafting`): the raw layout; a recipe's
    // conditions are the common CTDA, read as any record's.
    if &sig == b"RCPE" {
        let mut ctx = DecodeCtx { schema, on_formid: None };
        let issues = &mut out.issues;
        let mut cond = |s: &fnv_esm::Subrecord| {
            let v = decode_sub(&sig, s, &mut ctx).value.or_else(|| decode_sub(b"PACK", s, &mut ctx).value)?;
            match cond::parse(&v, types, &map) {
                Ok(c) => Some(c),
                Err(e) => {
                    issues.note("condition unreadable", || format!("0x{g:08X}: {e}"));
                    None
                }
            }
        };
        let recipe = crate::crafting::RecipeDef::from_raw(FormId(g), &subs, &map, &mut cond);
        out.recipe = Some(recipe);
    }
    if &sig == b"RCCT" {
        out.recipe_category = Some(crate::crafting::RecipeCategoryDef::from_raw(&subs));
    }
    if &sig == b"CSNO" {
        out.casino = Some(crate::casino::CasinoDef::from_raw(&subs, &map));
    }
    if &sig == b"CCRD" {
        out.caravan_card = crate::caravan::CardDef::from_raw(&subs);
    }
    if &sig == b"CDCK" {
        out.caravan_deck = Some(crate::caravan::DeckDef::from_raw(&subs, &map));
    }
    if &sig == b"IMAD" {
        let word = |o: usize| subs.iter().find(|s| &s.sig == b"DNAM").and_then(|s| <[u8; 4]>::try_from(s.data.get(o..o + 4)?).ok());
        if let (Some(a), Some(d)) = (word(0), word(4)) {
            out.image_modifier = Some(ImageModifierDef { animatable: u32::from_le_bytes(a) != 0, duration: f32::from_le_bytes(d) });
        }
    }
    for s in &subs {
        match &s.sig {
            b"EDID" if out.edid.is_none() => out.edid = val(s).and_then(|v| v.as_str().map(str::to_string)),
            b"SCRI" if out.script.is_none() => out.script = val(s).as_ref().and_then(hexid).and_then(map).map(FormId),
            _ => {}
        }
    }
    // Ownership and crime (`crate::crime`): read from the raw subrecords, as the schema
    // lays them out [documented: xEdit wbDefinitionsFNV.pas CELL DATA flags, XOWN, XRNK;
    // FACT DATA; schemas `world.json`, `common.json`, `gameplay.json`].
    match &sig {
        b"CELL" => out.cell = Some(crate::crime::CellDef::from_raw(&subs, r.header.flags, &map)),
        b"REFR" | b"ACHR" | b"ACRE" | b"PGRE" => {
            out.owner_rank = subs.iter().find(|s| &s.sig == b"XRNK").and_then(|s| <[u8; 4]>::try_from(s.data.as_slice()).ok()).map(i32::from_le_bytes);
        }
        b"FACT" => {
            let data = subs.iter().find(|s| &s.sig == b"DATA").map(|s| s.data.as_slice()).unwrap_or_default();
            out.faction_flags = Some(u16::from(data.first().copied().unwrap_or(0)) | (u16::from(data.get(1).copied().unwrap_or(0)) << 8));
            out.faction_reputation = subs
                .iter()
                .find(|s| &s.sig == b"WMI1")
                .and_then(|s| <[u8; 4]>::try_from(s.data.as_slice()).ok())
                .map(u32::from_le_bytes)
                .filter(|&l| l != 0)
                .and_then(map)
                .map(FormId);
        }
        _ => {}
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
                    b"FULL" => m.title = v.as_ref().and_then(|v| v.as_str().map(str::to_string)).filter(|t| !t.trim().is_empty()),
                    b"DESC" => m.text = v.as_ref().and_then(|v| v.as_str().map(str::to_string)),
                    b"ITXT" => {
                        m.buttons.push(v.as_ref().and_then(|v| v.as_str()).unwrap_or_default().to_string());
                        m.button_conditions.push(Vec::new());
                    }
                    // A CTDA belongs to the button before it [observed: GSChemistrySetMsg
                    // has a Science condition after each of its two craft buttons].
                    b"CTDA" => match m.button_conditions.last_mut() {
                        Some(c) => c.extend(cond(v, &mut out.issues)),
                        None => out.issues.note("message condition before any button", || format!("0x{g:08X}")),
                    },
                    b"DNAM" => m.message_box = has_flag(v.as_ref(), "Message Box"),
                    b"INAM" => m.icon = fid(v).filter(|f| f.0 != 0),
                    b"TNAM" => m.display_seconds = v.as_ref().and_then(Json::as_u64).map(|x| x as u32),
                    _ => {}
                }
            }
            out.message = Some(m);
        }
        b"NOTE" => {
            let mut n = NoteDef::default();
            for (sub, v) in &decoded {
                match *sub {
                    b"FULL" => n.name = v.as_ref().and_then(|v| v.as_str()).map(str::to_string),
                    b"DATA" => {
                        n.kind = match v.as_ref() {
                            Some(Json::String(k)) => ["Sound", "Text", "Image", "Voice"].iter().position(|x| x == k).unwrap_or(0) as u8,
                            Some(x) => x.as_u64().unwrap_or(0) as u8,
                            None => 0,
                        }
                    }
                    b"XNAM" => n.texture = v.as_ref().and_then(|v| v.as_str()).map(str::to_string).filter(|t| !t.is_empty()),
                    _ => {}
                }
            }
            if n.kind == 1 {
                n.text = subs.iter().find(|s| &s.sig == b"TNAM").map(|s| fnv_util::cp1252::decode_z(&s.data));
            }
            out.note = Some(n);
        }
        b"TERM" => {
            let mut t = TerminalDef::default();
            let s = |v: &Option<Json>| v.as_ref().and_then(|v| v.as_str()).unwrap_or_default().to_string();
            for (sub, v) in &decoded {
                match *sub {
                    b"FULL" => t.name = Some(s(v)).filter(|n| !n.trim().is_empty()),
                    b"DESC" => t.welcome = s(v),
                    b"PNAM" => t.password_note = fid(v).filter(|f| f.0 != 0),
                    b"DNAM" => {
                        if let Some(v) = v {
                            t.difficulty = match v.get("base_hacking_difficulty") {
                                Some(Json::String(name)) => {
                                    ["Very Easy", "Easy", "Average", "Hard", "Very Hard"].iter().position(|n| n == name).unwrap_or(5) as u8
                                }
                                Some(n) => n.as_u64().unwrap_or(5) as u8,
                                None => 5,
                            };
                            t.flags = flag_bits(v.get("flags")) as u8;
                            t.server = v.get("server_type").and_then(Json::as_u64).unwrap_or(0) as u8;
                        }
                    }
                    b"ITXT" => t.items.push(TerminalItem { text: s(v), ..Default::default() }),
                    b"RNAM" | b"ANAM" | b"INAM" | b"TNAM" | b"CTDA" => {
                        let Some(item) = t.items.last_mut() else { continue };
                        match *sub {
                            b"RNAM" => item.result = s(v),
                            b"ANAM" => item.flags = flag_bits(v.as_ref()) as u8,
                            b"INAM" => item.note = fid(v).filter(|f| f.0 != 0),
                            b"TNAM" => item.submenu = fid(v).filter(|f| f.0 != 0),
                            _ => item.conditions.extend(cond(v, &mut out.issues)),
                        }
                    }
                    _ => {}
                }
            }
            out.terminal = Some(t);
        }
        b"CSTY" => {
            let get = |sig: &[u8; 4]| decoded.iter().find(|(s, _)| *s == sig).and_then(|(_, v)| v.as_ref());
            let (std, adv, simple) = (get(b"CSTD"), get(b"CSAD"), get(b"CSSD"));
            let n = |v: Option<&Json>, k: &str, d: f32| v.and_then(|v| v.get(k)).and_then(Json::as_f64).map_or(d, |x| x as f32);
            out.combat_style = Some(CombatStyleDef {
                block_chance: n(std, "block_chance", 0.0),
                attack_chance: n(std, "attack_chance", 100.0),
                hand_to_hand_bonus: n(std, "hand_to_hand_bonus_to_attack", 0.0),
                power_attack_chance: n(std, "power_attack_chance", 0.0),
                power_attack_dirs: ["normal", "forward", "back", "left", "right"].map(|d| n(std, &format!("power_attack_{d}"), 0.0)),
                recoil_stagger_power_bonus: n(std, "recoil_stagger_bonus_to_power", 0.0),
                unconscious_power_bonus: n(std, "unconscious_bonus_to_power_attack", 0.0),
                unconscious_bonus: n(std, "unconscious_bonus_to_attack", 0.0),
                power_by_chance: has_flag(std.and_then(|v| v.get("flags")), "Choose Attack using % Chance"),
                hold_timer: (n(std, "hold_timer_min", 0.0), n(std, "hold_timer_max", 0.0)),
                attack_skill_mod: (n(adv, "attack_skill_modifier_base", 0.0), n(adv, "attack_skill_modifier_mult", 0.0)),
                attack_under_attack_mult: n(adv, "attack_while_under_attack_mult", 1.0),
                attack_not_under_attack_mult: n(adv, "attack_not_under_attack_mult", 1.0),
                recoil_stagger_bonus: n(std, "recoil_stagger_bonus_to_attack", 0.0),
                block_skill_mod: (n(adv, "block_skill_modifier_base", 0.0), n(adv, "block_skill_modifier_mult", 0.0)),
                block_under_attack_mult: n(adv, "block_while_under_attack_mult", 1.0),
                block_not_under_attack_mult: n(adv, "block_not_under_attack_mult", 1.0),
                attack_during_block_mult: n(adv, "attack_during_block_mult", 1.0),
                weapon_restrictions: match simple.and_then(|v| v.get("weapon_restrictions")) {
                    Some(Json::String(s)) => ["None", "Melee Only", "Ranged Only"].iter().position(|x| x == s).unwrap_or(0) as u32,
                    Some(v) => v.as_u64().unwrap_or(0) as u32,
                    None => 0,
                },
                range_mult: (n(simple, "ranged_weapon_range_mult_min", 1.0), n(simple, "ranged_weapon_range_mult_max", 1.0)),
                semi_auto_delay_mult: (n(simple, "semi_auto_firing_delay_mult_min", 1.0), n(simple, "semi_auto_firing_delay_mult_max", 1.0)),
                max_targeting_fov: n(simple, "max_targeting_fov", 0.0),
                combat_radius: n(simple, "combat_radius", 0.0),
                flee_personal_survival: has_flag(std.and_then(|v| v.get("flags")), "Flee Based on Personal Survival"),
                cover_search_radius: n(simple, "cover_search_radius", 0.0),
                take_cover_chance: n(simple, "take_cover_chance", 0.0),
                wait_timer: (n(simple, "wait_timer_min", 0.0), n(simple, "wait_timer_max", 0.0)),
                wait_to_fire_timer: (n(simple, "wait_to_fire_timer_min", 0.0), n(simple, "wait_to_fire_timer_max", 0.0)),
                fire_timer: (n(simple, "fire_timer_min", 0.0), n(simple, "fire_timer_max", 0.0)),
            });
        }
        b"MICN" => {
            out.menu_icon = decoded.iter().find(|(s, _)| *s == b"ICON").and_then(|(_, v)| v.as_ref()?.as_str().map(str::to_string));
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
                    b"SPLO" => a.spells.extend(fid(v).filter(|f| f.0 != 0)),
                    b"TPLT" => a.template = fid(v),
                    b"EITM" => a.unarmed_effect = fid(v).filter(|f| f.0 != 0),
                    b"EAMT" => a.unarmed_anim = v.as_ref().and_then(Json::as_str).map(str::to_string),
                    b"ZNAM" => a.combat_style = fid(v).filter(|f| f.0 != 0),
                    b"RNAM" if &sig == b"NPC_" => a.race = fid(v),
                    b"RNAM" if &sig == b"CREA" => a.attack_reach = v.as_ref().and_then(Json::as_u64).map(|n| n.min(255) as u8),
                    b"CNAM" if &sig == b"NPC_" => a.class = fid(v),
                    b"DATA" if &sig == b"CREA" => {
                        a.creature_type =
                            v.as_ref().and_then(|v| v.get("type")).and_then(|t| field_code(schema, &sig, b"DATA", Some("type"), t));
                        a.base_health = v.as_ref().and_then(|v| v.get("health")).and_then(Json::as_i64).unwrap_or(0);
                        a.zero_health = v.as_ref().and_then(|v| v.get("health")).and_then(Json::as_i64) == Some(0);
                        a.attributes = v.as_ref().and_then(|v| v.get("attributes")).and_then(attributes);
                        let skill = |k: &str| v.as_ref().and_then(|v| v.get(k)).and_then(Json::as_u64).map(|n| n.min(255) as u8);
                        (a.combat_skill, a.stealth_skill) = (skill("combat_skill"), skill("stealth_skill"));
                        a.attack_damage = v.as_ref().and_then(|v| v.get("damage")).and_then(Json::as_i64);
                    }
                    b"DATA" if &sig == b"NPC_" => {
                        a.base_health = v.as_ref().and_then(|v| v.get("base_health")).and_then(Json::as_i64).unwrap_or(0);
                        a.zero_health = v.as_ref().and_then(|v| v.get("base_health")).and_then(Json::as_i64) == Some(0);
                        a.attributes = v.as_ref().and_then(|v| v.get("attributes")).and_then(attributes);
                    }
                    b"DNAM" if &sig == b"NPC_" => {
                        a.skills = v.as_ref().and_then(|v| v.get("skill_values")).and_then(skills);
                        a.skill_offsets = v.as_ref().and_then(|v| v.get("skill_offsets")).and_then(skills);
                    }
                    b"AIDT" => {
                        a.energy = v.as_ref().and_then(|v| v.get("energy_level")).and_then(Json::as_u64).map(|e| e.min(255) as u8);
                        let code = |k: &str| v.as_ref().and_then(|v| v.get(k)).and_then(|t| field_code(schema, &sig, b"AIDT", Some(k), t));
                        (a.aggression, a.confidence, a.mood, a.assistance) = (code("aggression"), code("confidence"), code("mood"), code("assistance"));
                        a.responsibility = v.as_ref().and_then(|v| v.get("responsibility")).and_then(Json::as_i64);
                        a.aggro_radius_behavior = v.as_ref().and_then(|v| v.get("aggro_radius_behavior")).is_some_and(|b| b.as_str() == Some("True") || b.as_bool() == Some(true) || b.as_u64().is_some_and(|n| n != 0));
                        a.aggro_radius = v.as_ref().and_then(|v| v.get("aggro_radius")).and_then(Json::as_u64).map_or(0, |n| n.min(u64::from(u32::MAX)) as u32);
                    }
                    b"ACBS" => {
                        let f = v.as_ref().and_then(|v| v.get("flags"));
                        a.female = has_flag(f, "Female");
                        a.essential = has_flag(f, "Essential");
                        a.invulnerable = has_flag(f, "Invulnerable");
                        a.no_knockdowns = has_flag(f, "No Knockdowns");
                        a.immobile = &sig == b"CREA" && has_flag(f, "Immobile");
                        a.no_low_level = has_flag(f, "No Low Level Processing");
                        a.respawn = has_flag(f, "Respawn");
                        a.pc_level_mult = has_flag(f, "PC Level Mult");
                        a.autocalc = &sig == b"NPC_" && has_flag(f, "Auto-calc stats");
                        let n = |k: &str| v.as_ref().and_then(|v| v.get(k)).and_then(Json::as_i64).unwrap_or(0);
                        (a.level_raw, a.calc_min, a.calc_max) = (n("level_or_level_mult_raw"), n("calc_min"), n("calc_max"));
                        (a.fatigue, a.speed_mult) = (n("fatigue"), n("speed_multiplier"));
                        a.karma = v.as_ref().and_then(|v| v.get("karma")).and_then(Json::as_f64).unwrap_or(0.0);
                        a.disposition_base = n("disposition_base");
                        let tf = v.as_ref().and_then(|v| v.get("template_flags"));
                        a.template_packages = has_flag(tf, "AI Packages");
                        a.template_stats = has_flag(tf, "Stats");
                        a.template_ai = has_flag(tf, "AI Data");
                        a.template_factions = has_flag(tf, "Factions");
                        a.template_base = has_flag(tf, "Base Data");
                        a.template_traits = has_flag(tf, "Traits");
                        a.template_model = has_flag(tf, "Model/Animation");
                        a.template_inventory = has_flag(tf, "Inventory");
                        a.template_script = has_flag(tf, "Script");
                        a.template_spells = has_flag(tf, "Actor Effect List");
                    }
                    _ => {}
                }
            }
            out.actor = Some(a);
        }
        b"LVLN" | b"LVLC" => {
            let mut l = LeveledActorDef::default();
            for (s, v) in &decoded {
                match *s {
                    b"LVLF" => l.all_levels = has_flag(v.as_ref(), "Calculate from all levels <= player's level"),
                    b"LVLO" => {
                        let level = v.as_ref().and_then(|v| v.get("level")).and_then(Json::as_u64).unwrap_or(0).min(u64::from(u16::MAX)) as u16;
                        if let Some(f) = v.as_ref().and_then(|v| v.get("reference")).and_then(hexid).and_then(map) {
                            l.entries.push((level, FormId(f)));
                        }
                    }
                    _ => {}
                }
            }
            out.leveled_actor = Some(l);
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
            out.reputation_name = decoded.iter().find(|(s, _)| *s == b"FULL").and_then(|(_, v)| v.as_ref()?.as_str().map(str::to_string)).filter(|n| !n.trim().is_empty());
        }
        b"CHAL" => {
            out.challenge = decoded
                .iter()
                .find(|(s, _)| *s == b"DATA")
                .and_then(|(_, v)| v.as_ref())
                .map(|v| (flag_bits(v.get("flags")), v.get("threshold").and_then(Json::as_u64).unwrap_or(0) as u32));
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
                let actor_value = v.get("actor_value").and_then(|t| field_code(schema, &sig, b"DATA", Some("actor_value"), t));
                let resistance = v.get("resistance_type").and_then(|t| field_code(schema, &sig, b"DATA", Some("resistance_type"), t));
                out.magic_effect = Some(MagicEffectDef {
                    flags: flag_bits(v.get("flags")),
                    archetype,
                    script: item.filter(|_| archetype == ARCHETYPE_SCRIPT),
                    actor_value: actor_value.and_then(|c| u16::try_from(c).ok()),
                    shader: v.get("effect_shader").and_then(hexid).filter(|&x| x != 0).and_then(map).map(FormId),
                    resistance: resistance.and_then(|c| u16::try_from(c).ok()),
                });
            }
        }
        b"ALCH" | b"SPEL" | b"ENCH" | b"INGR" => {
            let mut list: Vec<EffectItem> = Vec::new();
            for (s, v) in &decoded {
                match *s {
                    b"EFID" => list.extend(fid(v).map(|effect| EffectItem { effect, magnitude: 0, area: 0, duration: 0, actor_value: None, conditions: Vec::new() })),
                    b"EFIT" => {
                        if let (Some(e), Some(v)) = (list.last_mut(), v.as_ref()) {
                            let n = |k: &str| v.get(k).and_then(Json::as_u64).unwrap_or(0) as u32;
                            e.magnitude = n("magnitude");
                            e.area = n("area");
                            e.duration = n("duration");
                            e.actor_value = v
                                .get("actor_value")
                                .and_then(|t| field_code(schema, &sig, b"EFIT", Some("actor_value"), t))
                                .and_then(|c| u16::try_from(c).ok());
                        }
                    }
                    b"ENIT" if &sig == b"ALCH" => {
                        if let Some(v) = v.as_ref() {
                            let form = |k: &str| v.get(k).and_then(hexid).filter(|&x| x != 0).and_then(map).map(FormId);
                            let etyp = decoded.iter().find(|(s, _)| *s == b"ETYP").and_then(|(_, t)| t.as_ref());
                            out.ingestible = Some(IngestibleDef {
                                value: v.get("value").and_then(Json::as_i64).unwrap_or(0) as i32,
                                flags: flag_bits(v.get("flags")) as u8,
                                withdrawal: form("withdrawal_effect"),
                                addiction_chance: v.get("addiction_chance").and_then(Json::as_f64).unwrap_or(0.0) as f32,
                                consume_sound: form("sound_consume"),
                                equip_type: etyp.and_then(|t| field_code(schema, &sig, b"ETYP", None, t)).unwrap_or(-1),
                            });
                        }
                    }
                    b"SPIT" if &sig == b"SPEL" => {
                        out.spell_type = v.as_ref().and_then(|v| v.get("type")).and_then(|t| field_code(schema, &sig, b"SPIT", Some("type"), t));
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
            let link = |sub: &[u8; 4]| decoded.iter().find(|(s, _)| *s == sub).and_then(|(_, v)| fid(v)).filter(|f| f.0 != 0);
            out.race_older = link(b"ONAM");
            out.race_younger = link(b"YNAM");
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
        b"PERK" => {
            // The perk's own fields come before its first effect (PRKE); the CTDA and
            // DATA after it belong to the effects [documented: xEdit wbDefinitionsFNV.pas
            // PERK record order].
            let head = decoded.iter().position(|(s, _)| *s == b"PRKE").unwrap_or(decoded.len());
            let raw_data = subs[..head].iter().find(|s| &s.sig == b"DATA").map(|s| s.data.as_slice());
            let mut unread = Issues::default();
            let conds: Vec<Option<Condition>> = decoded[..head].iter().filter(|(s, _)| *s == b"CTDA").map(|(_, v)| cond(v, &mut unread)).collect();
            let text = |sig: &[u8; 4]| decoded[..head].iter().find(|(s, _)| *s == sig).and_then(|(_, v)| v.as_ref()?.as_str().map(str::to_string));
            let mut p = PerkDef::from_data(raw_data);
            p.name = text(b"FULL").unwrap_or_default();
            p.description = text(b"DESC").unwrap_or_default();
            p.icon = text(b"ICON").filter(|i| !i.trim().is_empty());
            p.conditions = conds.into_iter().collect();
            let mut entry: Option<PerkEntry> = None;
            let mut ordinal: u32 = 0;
            let mut tab: i8 = 0;
            let mut epft: Option<u8> = None;
            for (i, s) in subs.iter().enumerate().skip(head) {
                match &s.sig {
                    b"PRKE" => {
                        let b = |k: usize| s.data.get(k).copied().unwrap_or(0);
                        let effect = match b(0) {
                            0 => PerkEffect::QuestStage { quest: FormId(0), stage: 0 },
                            1 => PerkEffect::Ability(FormId(0)),
                            2 => PerkEffect::EntryPoint(EntryPointDef { conditions: Some(Vec::new()), ..Default::default() }),
                            _ => PerkEffect::Other,
                        };
                        entry = Some(PerkEntry { rank: b(1), priority: b(2), effect, ordinal });
                        ordinal += 1;
                        tab = 0;
                        epft = None;
                    }
                    b"DATA" => {
                        let Some(e) = entry.as_mut() else { continue };
                        e.effect = match &e.effect {
                            PerkEffect::Ability(_) => match <[u8; 4]>::try_from(s.data.as_slice()).ok().map(u32::from_le_bytes).and_then(map) {
                                Some(g) => PerkEffect::Ability(FormId(g)),
                                None => PerkEffect::Other,
                            },
                            PerkEffect::QuestStage { .. } if s.data.len() >= 5 => {
                                let id = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                                match map(id) {
                                    Some(q) => PerkEffect::QuestStage { quest: FormId(q), stage: s.data[4] },
                                    None => PerkEffect::Other,
                                }
                            }
                            PerkEffect::EntryPoint(ep) if s.data.len() >= 2 => {
                                PerkEffect::EntryPoint(EntryPointDef { point: s.data[0], function: s.data[1], ..ep.clone() })
                            }
                            _ => PerkEffect::Other,
                        };
                    }
                    b"PRKC" => tab = s.data.first().map_or(0, |&b| b as i8),
                    b"CTDA" => {
                        if let Some(PerkEffect::EntryPoint(ep)) = entry.as_mut().map(|e| &mut e.effect) {
                            let c = cond(&decoded[i].1, &mut unread);
                            match (ep.conditions.as_mut(), c) {
                                (Some(list), Some(c)) => list.push((tab, c)),
                                _ => ep.conditions = None,
                            }
                        }
                    }
                    b"EPFT" => epft = s.data.first().copied(),
                    b"EPF2" => {
                        if let Some(PerkEffect::EntryPoint(ep)) = entry.as_mut().map(|e| &mut e.effect) {
                            ep.button = Some(zstring(&s.data)).filter(|b| !b.is_empty());
                        }
                    }
                    b"EPF3" => {
                        if let Some(PerkEffect::EntryPoint(ep)) = entry.as_mut().map(|e| &mut e.effect) {
                            ep.run_immediately = s.data.first().is_some_and(|b| b & 1 != 0);
                        }
                    }
                    b"EPFD" => {
                        if let Some(PerkEffect::EntryPoint(ep)) = entry.as_mut().map(|e| &mut e.effect) {
                            ep.param_type = epft;
                            ep.raw = s.data.clone();
                            if epft == Some(1) {
                                ep.value = <[u8; 4]>::try_from(s.data.as_slice()).ok().map(f32::from_le_bytes);
                            }
                        }
                    }
                    b"PRKF" => p.entries.extend(entry.take()),
                    _ => {}
                }
            }
            out.perk = Some(p);
        }
        b"AVIF" => {
            let text = |sig: &[u8; 4]| decoded.iter().find(|(s, _)| *s == sig).and_then(|(_, v)| v.as_ref()?.as_str().map(str::to_string));
            let anam = subs.iter().find(|s| &s.sig == b"ANAM").map(|s| zstring(&s.data));
            out.av_info = Some(ActorValueInfo {
                name: text(b"FULL").unwrap_or_default(),
                short_name: anam.unwrap_or_default(),
                description: text(b"DESC").unwrap_or_default(),
                icon: text(b"ICON").filter(|i| !i.trim().is_empty()),
            });
        }
        b"IDLE" => {
            // A condition that cannot be read is no issue of the script data (some idle
            // conditions use functions scripts never call, animation.md §1.2): the idle
            // is only marked as one that cannot be judged.
            let mut unread = Issues::default();
            let conds: Vec<Option<Condition>> = decoded.iter().filter(|(s, _)| *s == b"CTDA").map(|(_, v)| cond(v, &mut unread)).collect();
            if let Some(i) = out.idle.as_mut() {
                i.conditions = conds.into_iter().collect();
                // ANAM: parent and previous IDLE, file-local FormIDs (null: none).
                let anam = subs.iter().find(|s| &s.sig == b"ANAM").map(|s| s.data.as_slice()).unwrap_or_default();
                let link = |at: usize| anam.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).filter(|l| *l != 0).and_then(map).map(FormId);
                i.parent = link(0);
                i.previous = link(4);
            }
        }
        b"IDLM" => {
            let code = |sub: &[u8; 4], field: &str, v: &Json| field_code(schema, &sig, sub, Some(field), v);
            let ctx = crate::package::ReadCtx { code: &code, map: &map };
            out.idle_marker = Some(crate::package::read(&decoded, &ctx, &mut |_| None).idles.unwrap_or_default());
        }
        b"WEAP" => {
            let dnam = decoded.iter().find(|(s, _)| *s == b"DNAM").and_then(|(_, v)| v.as_ref());
            let code = |field: &str| dnam?.get(field).and_then(|t| field_code(schema, &sig, b"DNAM", Some(field), t));
            out.weapon_skill = code("skill");
            out.weapon_anim_type = code("animation_type");
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
/// DATA attributes: S.P.E.C.I.A.L. in record order.
fn attributes(v: &Json) -> Option<[u8; 7]> {
    let mut out = [0u8; 7];
    for (o, name) in out.iter_mut().zip(fnv_sim::forms::SPECIAL) {
        *o = v.get(name)?.as_u64()?.min(255) as u8;
    }
    Some(out)
}

/// One of NPC_ DNAM's two skill arrays, Barter to Unarmed.
fn skills(v: &Json) -> Option<[u8; 14]> {
    let mut out = [0u8; 14];
    for (o, name) in out.iter_mut().zip(crate::actor_values::SKILL_FIELDS) {
        *o = v.get(name)?.as_u64()?.min(255) as u8;
    }
    Some(out)
}

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
