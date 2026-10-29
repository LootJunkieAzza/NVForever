//! `fnvtool dialogue …`: dialogue topics (DIAL) and lines (INFO) as `fnv-game` reads
//! them, and headless conversations. Output contracts: docs/spec-notes/dialogue.md §8.
//! Texts (topic names, prompts, responses) are game data: they are printed only with
//! `--text`; without it the listings give their lengths.

use crate::esm_cmd::open_load_order;
use crate::script_cmd::{table_path, ScriptArgs};
use anyhow::{bail, Context};
use clap::{Args, Subcommand};
use fnv_esm::{LoadOrder, Schema};
use fnv_game::data::{info_flag, topic_type, InfoDef, TopicDef};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum DialogueCmd {
    /// Topics (DIAL): counts over the load order, and the topics a filter selects
    /// (fnv.dialogue.topics/1).
    Topics {
        #[command(flatten)]
        a: ScriptArgs,
        #[command(flatten)]
        f: Filter,
        /// Only top-level topics.
        #[arg(long)]
        top_level: bool,
        /// Only topics of this type (Topic, Conversation, Combat, Persuasion, Detection,
        /// Service, Miscellaneous, Radio, or the code 0–7).
        #[arg(long = "type")]
        ty: Option<String>,
    },
    /// Lines (INFO): counts over the load order, and the lines a filter selects, in
    /// topic order (fnv.dialogue.lines/1).
    Lines {
        #[command(flatten)]
        a: ScriptArgs,
        #[command(flatten)]
        f: Filter,
        /// One line, by load-order FormID (hex).
        #[arg(long, value_parser = crate::parse_formid)]
        info: Option<u32>,
    },
    /// Run a scripted conversation headless and print its trace (fnv.dialogue.actions/1
    /// in, fnv.dialogue.trace/1 out).
    Talk {
        #[command(flatten)]
        a: ScriptArgs,
        /// Actions file (JSON, fnv.dialogue.actions/1).
        #[arg(long)]
        actions: PathBuf,
        /// Include the texts said and offered (game data: for reading, never to commit).
        #[arg(long)]
        text: bool,
        /// Keep every response at the length fitted to its text instead of its voice
        /// file's (as `script run --no-voice-lengths`, docs/spec-notes/voice.md §8).
        #[arg(long)]
        no_voice_lengths: bool,
    },
}

#[derive(Args)]
pub struct Filter {
    /// Lines of this quest (Editor ID or load-order FormID); topics that have such lines.
    #[arg(long)]
    pub quest: Option<String>,
    /// This topic (Editor ID or load-order FormID).
    #[arg(long)]
    pub topic: Option<String>,
    /// Lines whose fixed speaker (ANAM) is this actor: an NPC or creature (Editor ID or
    /// FormID) or a placed reference of one; topics that have such lines.
    #[arg(long)]
    pub speaker: Option<String>,
    /// List every topic or line (large); by default only what a filter selects.
    #[arg(long)]
    pub all: bool,
    /// Include texts (topic names, prompts, responses, acting notes).
    #[arg(long)]
    pub text: bool,
}

struct Loaded<'p> {
    lo: LoadOrder<'p>,
    data: GameData,
    refs: RefTable,
}

fn with_data<T>(a: &ScriptArgs, f: impl FnOnce(&Loaded) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let tp = table_path(a);
    let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
    let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
    let table = FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
    let plugins = open_load_order(&a.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let schema = Schema::load_dir(&a.schema)?;
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &fjson, &refs);
    f(&Loaded { lo, data, refs })
}

pub fn run(cmd: DialogueCmd) -> anyhow::Result<Value> {
    match cmd {
        DialogueCmd::Topics { a, f, top_level, ty } => with_data(&a, |l| topics(l, &f, top_level, ty.as_deref())),
        DialogueCmd::Lines { a, f, info } => with_data(&a, |l| lines(l, &f, info)),
        DialogueCmd::Talk { a, actions, text, no_voice_lengths } => talk(&a, &actions, text, !no_voice_lengths),
    }
}

/// A form by Editor ID or hex FormID.
fn form(l: &Loaded, s: &str) -> anyhow::Result<FormId> {
    if s.starts_with("0x") || s.starts_with("0X") {
        return Ok(FormId(crate::parse_formid(s).map_err(anyhow::Error::msg)?));
    }
    match l.data.by_edid(s).or_else(|| l.refs.by_edid(s)) {
        Some(f) => Ok(f),
        None => bail!("no form named {s:?}"),
    }
}

struct Selection {
    quest: Option<FormId>,
    topic: Option<FormId>,
    /// The speaker's base form.
    speaker: Option<FormId>,
}

impl Selection {
    fn new(l: &Loaded, f: &Filter) -> anyhow::Result<Selection> {
        let quest = f.quest.as_deref().map(|s| form(l, s)).transpose()?;
        if let Some(q) = quest.filter(|q| !l.data.is_quest(*q)) {
            bail!("{} is not a quest", l.data.name(q));
        }
        let topic = f.topic.as_deref().map(|s| form(l, s)).transpose()?;
        if let Some(t) = topic.filter(|t| !l.data.topic_defs.contains_key(t)) {
            bail!("{} is not a topic (DIAL)", l.data.name(t));
        }
        let speaker = f.speaker.as_deref().map(|s| form(l, s)).transpose()?.map(|s| l.refs.get(s).map_or(s, |p| p.base));
        Ok(Selection { quest, topic, speaker })
    }

    fn any(&self) -> bool {
        self.quest.is_some() || self.topic.is_some() || self.speaker.is_some()
    }

    fn line(&self, d: &InfoDef) -> bool {
        self.quest.is_none_or(|q| d.quest == Some(q)) && self.topic.is_none_or(|t| d.topic == Some(t)) && self.speaker.is_none_or(|s| d.speaker == Some(s))
    }
}

fn id(f: FormId) -> Value {
    json!(f.to_string())
}

fn named(l: &Loaded, f: FormId) -> Value {
    match l.data.edid_of.get(&f) {
        Some(e) => json!({"form": f.to_string(), "edid": e}),
        None => json!({"form": f.to_string()}),
    }
}

fn names(l: &Loaded, v: &[FormId]) -> Value {
    json!(v.iter().map(|f| named(l, *f)).collect::<Vec<_>>())
}

fn count<K: Ord + ToString>(m: &BTreeMap<K, u64>) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.to_string(), json!(v))).collect())
}

/// INFO DATA next speaker [documented: xEdit wbNextSpeaker].
fn speaker_name(code: u8) -> &'static str {
    ["Target", "Self", "Either"].get(code as usize).copied().unwrap_or("?")
}

fn type_name(t: u8) -> String {
    topic_type::NAMES.get(t as usize).map(|s| s.to_string()).unwrap_or_else(|| t.to_string())
}

/// Subrecord signatures of DIAL and INFO records over every plugin, with those the
/// reader does not take a value from: `ignored` (documented as unused or as editor
/// notes) and `unread` (anything else: residue).
fn subrecord_census(l: &Loaded) -> Value {
    const DIAL_READ: &[&[u8; 4]] = &[b"EDID", b"QSTI", b"QSTR", b"INFC", b"INFX", b"FULL", b"PNAM", b"TDUM", b"DATA"];
    const INFO_READ: &[&[u8; 4]] = &[
        b"DATA", b"QSTI", b"TPIC", b"PNAM", b"NAME", b"TRDT", b"NAM1", b"NAM2", b"SNAM", b"LNAM", b"CTDA", b"TCLT", b"TCLF", b"TCFU", b"SCHR", b"SCDA",
        b"SCTX", b"SCRO", b"SCRV", b"SLSD", b"SCVR", b"NEXT", b"RNAM", b"ANAM", b"KNAM", b"DNAM",
    ];
    // NAM3 "Edits" (an editor comment, empty in the masters), SNDD "Unused", and the
    // DIAL info-order lists xEdit writes for its own use.
    const IGNORED: &[&[u8; 4]] = &[b"NAM3", b"SNDD", b"INOM", b"INOA"];
    let mut out = Map::new();
    for (sig, read) in [(b"DIAL", DIAL_READ), (b"INFO", INFO_READ)] {
        let mut seen: BTreeMap<String, u64> = BTreeMap::new();
        let mut records = 0u64;
        let mut unreadable = 0u64;
        for recs in &l.lo.records {
            for r in recs.iter().filter(|r| &r.header.sig == sig) {
                records += 1;
                match r.subrecords() {
                    Ok(subs) => {
                        for s in subs {
                            *seen.entry(String::from_utf8_lossy(&s.sig).into_owned()).or_default() += 1;
                        }
                    }
                    Err(_) => unreadable += 1,
                }
            }
        }
        let is = |set: &[&[u8; 4]], k: &String| set.iter().any(|s| &s[..] == k.as_bytes());
        let pick = |want: &dyn Fn(&String) -> bool| -> Value { Value::Object(seen.iter().filter(|(k, _)| want(k)).map(|(k, v)| (k.clone(), json!(v))).collect()) };
        out.insert(
            String::from_utf8_lossy(sig).into_owned(),
            json!({
                "records": records,
                "unreadable_records": unreadable,
                "read": pick(&|k| is(read, k)),
                "ignored": pick(&|k| is(IGNORED, k)),
                "unread": pick(&|k| !is(read, k) && !is(IGNORED, k)),
            }),
        );
    }
    Value::Object(out)
}

/// How much rides on the order of lines of different quests inside one topic: topics
/// where one actor could in principle say lines of two quests (the same fixed speaker,
/// or no fixed speaker on one side), split by whether those quests differ in priority.
fn quest_mix(l: &Loaded) -> Value {
    let d = &l.data;
    let (mut multi, mut diff, mut equal, mut both) = (0u64, 0u64, 0u64, 0u64);
    let mut examples: Vec<Value> = Vec::new();
    let mut topics: Vec<&FormId> = d.topics.keys().collect();
    topics.sort();
    for t in topics {
        // Speaker (None: anyone) -> quests with their priorities.
        let mut by_speaker: HashMap<Option<FormId>, BTreeSet<(FormId, u8)>> = HashMap::new();
        for i in &d.topics[t] {
            let Some(def) = d.infos.get(i) else { continue };
            let Some(q) = def.quest else { continue };
            by_speaker.entry(def.speaker).or_default().insert((q, d.line_priority(*i)));
        }
        let all: BTreeSet<FormId> = by_speaker.values().flatten().map(|x| x.0).collect();
        if all.len() < 2 {
            continue;
        }
        multi += 1;
        let anyone = by_speaker.get(&None).cloned().unwrap_or_default();
        let (mut has_diff, mut has_equal) = (false, false);
        for (sp, set) in &by_speaker {
            let set: BTreeSet<(FormId, u8)> = if sp.is_some() { set.union(&anyone).copied().collect() } else { set.clone() };
            let prios: Vec<u8> = set.iter().map(|x| x.1).collect();
            let distinct: BTreeSet<u8> = prios.iter().copied().collect();
            has_diff |= distinct.len() > 1;
            has_equal |= prios.len() > distinct.len();
        }
        diff += has_diff as u64;
        equal += has_equal as u64;
        both += (has_diff && has_equal) as u64;
        if (has_diff || has_equal) && examples.len() < 5 {
            examples.push(json!({"topic": named(l, *t), "quests": all.len(), "different_priorities": has_diff, "equal_priorities": has_equal}));
        }
    }
    json!({
        "topics_with_lines_of_several_quests": multi,
        "one_actor_could_say_lines_of_quests_with_different_priorities": diff,
        "one_actor_could_say_lines_of_quests_with_equal_priority": equal,
        "both": both,
        "first_examples": examples,
    })
}

fn topic_row(l: &Loaded, t: &TopicDef, text: bool, detail: bool) -> Value {
    let d = &l.data;
    let lines = d.topics.get(&t.form).map(Vec::as_slice).unwrap_or_default();
    let mut flags = Vec::new();
    if t.flags & TopicDef::RUMORS != 0 {
        flags.push("Rumors");
    }
    if t.top_level() {
        flags.push("Top-level");
    }
    let mut line_quests: Vec<FormId> = Vec::new();
    for i in lines {
        if let Some(q) = d.infos.get(i).and_then(|x| x.quest).filter(|q| !line_quests.contains(q)) {
            line_quests.push(q);
        }
    }
    let mut row = json!({
        "form": id(t.form),
        "edid": t.edid,
        "type": type_name(t.topic_type),
        "flags": flags,
        "priority": t.priority,
        "name_chars": t.name.as_ref().map(|n| n.chars().count()),
        "quests": names(l, &t.quests),
        "lines": lines.len(),
        "line_quests": names(l, &line_quests),
        "shared_infos": t.shared_infos.len(),
        "definitions": l.lo.index.get(&t.form.0).map_or(0, Vec::len),
    });
    if text {
        row["name"] = json!(t.name);
        row["dumb_response"] = json!(t.dumb_response);
    } else if let Some(n) = &t.dumb_response {
        row["dumb_response_chars"] = json!(n.chars().count());
    }
    if detail {
        row["removed_quests"] = names(l, &t.removed_quests);
        row["line_forms"] = json!(d.lines_tried(t.form).iter().map(|i| i.to_string()).collect::<Vec<_>>());
        row["shared_info_list"] = json!(t
            .shared_infos
            .iter()
            .map(|s| json!({"quest": s.quest.map(|q| named(l, q)), "info": s.info.map(id), "index": s.index,
                            "info_topic": s.info.and_then(|i| d.infos.get(&i)).and_then(|x| x.topic).map(|x| named(l, x))}))
            .collect::<Vec<_>>());
    }
    row
}

fn topics(l: &Loaded, f: &Filter, top_level: bool, ty: Option<&str>) -> anyhow::Result<Value> {
    let d = &l.data;
    let sel = Selection::new(l, f)?;
    let ty: Option<u8> = match ty {
        None => None,
        Some(s) => Some(match topic_type::NAMES.iter().position(|n| n.eq_ignore_ascii_case(s)) {
            Some(i) => i as u8,
            None => s.parse().map_err(|_| anyhow::anyhow!("unknown topic type {s:?}"))?,
        }),
    };
    let mut all: Vec<&TopicDef> = d.topic_defs.values().collect();
    all.sort_by_key(|t| t.form);
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut top_by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut priorities: BTreeMap<String, u64> = BTreeMap::new();
    let mut lines_hist: BTreeMap<String, u64> = BTreeMap::new();
    let (mut named_n, mut no_lines, mut with_quests, mut qsti, mut rumors, mut dumb, mut shared_topics, mut shared, mut shared_known, mut shared_no_quest, mut several) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut shared_elsewhere = 0u64;
    let mut max_lines = (0usize, FormId(0));
    for t in &all {
        *by_type.entry(type_name(t.topic_type)).or_default() += 1;
        if t.top_level() {
            *top_by_type.entry(type_name(t.topic_type)).or_default() += 1;
        }
        *priorities.entry(format!("{}", t.priority)).or_default() += 1;
        let n = d.topics.get(&t.form).map_or(0, Vec::len);
        let bucket = match n {
            0..=5 => n.to_string(),
            6..=10 => "6-10".into(),
            11..=50 => "11-50".into(),
            _ => "51+".into(),
        };
        *lines_hist.entry(bucket).or_default() += 1;
        if n > max_lines.0 {
            max_lines = (n, t.form);
        }
        named_n += t.name.is_some() as u64;
        no_lines += (n == 0) as u64;
        with_quests += !t.quests.is_empty() as u64;
        qsti += t.quests.len() as u64;
        rumors += (t.flags & TopicDef::RUMORS != 0) as u64;
        dumb += t.dumb_response.is_some() as u64;
        shared_topics += !t.shared_infos.is_empty() as u64;
        several += (l.lo.index.get(&t.form.0).map_or(0, Vec::len) > 1) as u64;
        for s in &t.shared_infos {
            shared += 1;
            shared_no_quest += s.quest.is_none() as u64;
            if let Some(def) = s.info.and_then(|i| d.infos.get(&i)) {
                shared_known += 1;
                shared_elsewhere += (def.topic != Some(t.form)) as u64;
            }
        }
    }
    let listed = sel.any() || top_level || ty.is_some() || f.all;
    let rows: Vec<Value> = if listed {
        all.iter()
            .filter(|t| !top_level || t.top_level())
            .filter(|t| ty.is_none_or(|x| t.topic_type == x))
            .filter(|t| sel.topic.is_none_or(|x| t.form == x))
            .filter(|t| {
                (sel.quest.is_none() && sel.speaker.is_none())
                    || d.topics.get(&t.form).is_some_and(|ls| ls.iter().filter_map(|i| d.infos.get(i)).any(|i| sel.line(i)))
            })
            .map(|t| topic_row(l, t, f.text, sel.topic.is_some()))
            .collect()
    } else {
        Vec::new()
    };
    Ok(json!({
        "format": "fnv.dialogue.topics/1",
        "load_order": d.load_order,
        "summary": {
            "topics": all.len(),
            "by_type": count(&by_type),
            "top_level_by_type": count(&top_by_type),
            "top_level_offered": d.top_level.len(),
            "rumors": rumors,
            "with_name": named_n,
            "with_dumb_response": dumb,
            "priorities": count(&priorities),
            "with_quests": with_quests,
            "quest_entries": qsti,
            "without_lines": no_lines,
            "lines_per_topic": count(&lines_hist),
            "most_lines": {"topic": named(l, max_lines.1), "lines": max_lines.0},
            "defined_in_several_plugins": several,
            "shared_infos": {"topics": shared_topics, "entries": shared, "without_quest": shared_no_quest, "naming_a_line": shared_known,
                             "naming_a_line_stored_under_another_topic": shared_elsewhere},
            "greeting": d.greeting.map(|t| named(l, t)),
            "goodbye": d.goodbye.map(|t| named(l, t)),
            "quest_mix": quest_mix(l),
            "subrecords": subrecord_census(l),
        },
        "listed": rows.len(),
        "topics": rows,
    }))
}

fn line_row(l: &Loaded, i: FormId, def: &InfoDef, text: bool) -> Value {
    let d = &l.data;
    let position = def.topic.map(|t| d.lines_by_priority(t)).and_then(|ls| ls.iter().position(|x| *x == i));
    let conditions: Vec<Value> = def
        .conditions
        .iter()
        .map(|c| {
            json!({
                "function": d.table.get(c.opcode()).map(|f| f.name.clone()).unwrap_or_else(|| format!("0x{:04X}", c.opcode())),
                "args": c.params.iter().map(|a| fnv_game::game::arg_json(d, a)).collect::<Vec<_>>(),
                "cmp": c.cmp,
                "value": c.rhs,
                "or": c.or,
                "run_on": c.run_on,
            })
        })
        .collect();
    let responses: Vec<Value> = def
        .response_order()
        .into_iter()
        .map(|r| {
            let x = &def.responses[r];
            let mut v = json!({"number": x.number, "chars": def.response_chars[r], "emotion": x.emotion, "emotion_value": x.emotion_value,
                               "sound": x.sound.map(|s| named(l, s)), "speaker_idle": x.speaker_idle.map(|s| named(l, s)),
                               "listener_idle": x.listener_idle.map(|s| named(l, s)), "seconds_fitted": fnv_game::game::fitted_seconds(def.response_chars[r])});
            if text {
                v["text"] = json!(x.text);
                v["notes"] = json!(x.notes);
            }
            v
        })
        .collect();
    let unit = |u: Option<usize>| u.map(|u| d.unit(u).key());
    let mut row = json!({
        "form": id(i),
        "plugin": l.lo.index.get(&i.0).and_then(|v| v.first()).map(|x| d.load_order[x.plugin as usize].clone()),
        "definitions": l.lo.index.get(&i.0).map_or(0, Vec::len),
        "topic": def.topic.map(|t| named(l, t)),
        "position": position,
        "quest": def.quest.map(|q| named(l, q)),
        "quest_priority": d.line_priority(i),
        "type": type_name(def.info_type),
        "next_speaker": speaker_name(def.next_speaker),
        "flags": info_flag::NAMES.iter().filter(|(b, _)| def.has(*b)).map(|(_, n)| *n).collect::<Vec<_>>(),
        "record_flags": format!("0x{:08X}", def.record_flags),
        "speaker": def.speaker.map(|s| named(l, s)),
        "prompt_chars": def.prompt.as_ref().map(|p| p.chars().count()),
        "responses": responses,
        "conditions": conditions,
        "begin": unit(def.begin),
        "end": unit(def.end),
        "choices": names(l, &def.choices),
        "link_from": names(l, &def.link_from),
        "follow_ups": json!(def.follow_ups.iter().map(|x| x.to_string()).collect::<Vec<_>>()),
        "add_topics": names(l, &def.add_topics),
        "challenge": def.challenge_form.map(|c| json!({"form": named(l, c), "difficulty": def.challenge_difficulty})),
        "previous": if def.has_previous { json!(def.previous.map(id)) } else { json!("absent") },
    });
    if text {
        row["prompt"] = json!(def.prompt);
    }
    row
}

fn lines(l: &Loaded, f: &Filter, info: Option<u32>) -> anyhow::Result<Value> {
    let d = &l.data;
    let sel = Selection::new(l, f)?;
    let mut all: Vec<(&FormId, &InfoDef)> = d.infos.iter().collect();
    all.sort_by_key(|x| *x.0);
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut type_pairs: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_plugin: BTreeMap<String, u64> = BTreeMap::new();
    let mut flags: BTreeMap<String, u64> = BTreeMap::new();
    let mut per_line: BTreeMap<String, u64> = BTreeMap::new();
    let mut emotions: BTreeMap<String, u64> = BTreeMap::new();
    let mut next_speaker: BTreeMap<String, u64> = BTreeMap::new();
    let mut challenge_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut difficulty: BTreeMap<String, u64> = BTreeMap::new();
    let mut record_flags: BTreeMap<String, u64> = BTreeMap::new();
    let mut previous: BTreeMap<String, u64> = BTreeMap::new();
    let mut n: BTreeMap<&str, u64> = BTreeMap::new();
    let mut bump = |k: &'static str, by: u64| *n.entry(k).or_default() += by;
    for (&i, def) in &all {
        *by_type.entry(type_name(def.info_type)).or_default() += 1;
        if let Some(t) = def.topic.and_then(|t| d.topic_defs.get(&t)).filter(|t| t.topic_type != def.info_type) {
            *type_pairs.entry(format!("{} line in {} topic", type_name(def.info_type), type_name(t.topic_type))).or_default() += 1;
        }
        let defs = l.lo.index.get(&i.0);
        if let Some(p) = defs.and_then(|v| v.first()) {
            *by_plugin.entry(d.load_order[p.plugin as usize].clone()).or_default() += 1;
        }
        bump("defined_in_several_plugins", (defs.map_or(0, Vec::len) > 1) as u64);
        for (b, name) in info_flag::NAMES {
            if def.has(*b) {
                *flags.entry(name.to_string()).or_default() += 1;
            }
        }
        *per_line.entry(def.responses.len().min(10).to_string()).or_default() += 1;
        *next_speaker.entry(speaker_name(def.next_speaker).to_string()).or_default() += 1;
        *record_flags.entry(format!("0x{:08X}", def.record_flags)).or_default() += 1;
        bump("responses", def.responses.len() as u64);
        let numbers: Vec<u8> = def.responses.iter().map(|r| r.number).collect();
        bump("lines_with_responses_stored_out_of_number_order", numbers.windows(2).any(|w| w[0] > w[1]) as u64);
        bump("lines_with_response_numbers_not_1_to_n", (!numbers.iter().enumerate().all(|(k, &x)| x as usize == k + 1)) as u64);
        for r in &def.responses {
            *emotions.entry(r.emotion.to_string()).or_default() += 1;
            bump("responses_with_sound", r.sound.is_some() as u64);
            bump("responses_with_speaker_idle", r.speaker_idle.is_some() as u64);
            bump("responses_with_listener_idle", r.listener_idle.is_some() as u64);
            bump("responses_with_notes", !r.notes.is_empty() as u64);
            bump("responses_with_empty_text", r.text.trim().is_empty() as u64);
        }
        bump("with_topic", def.topic.is_some() as u64);
        bump("with_quest", def.quest.is_some() as u64);
        bump("with_fixed_speaker", def.speaker.is_some() as u64);
        bump("with_conditions", !def.conditions.is_empty() as u64);
        bump("conditions", def.conditions.len() as u64);
        bump("with_begin_script", def.begin.is_some() as u64);
        bump("with_end_script", def.end.is_some() as u64);
        bump("with_prompt", def.prompt.is_some() as u64);
        bump("with_choices", !def.choices.is_empty() as u64);
        bump("choice_links", def.choices.len() as u64);
        bump("with_link_from", !def.link_from.is_empty() as u64);
        bump("link_from_links", def.link_from.len() as u64);
        bump("with_follow_ups", !def.follow_ups.is_empty() as u64);
        bump("follow_up_links", def.follow_ups.len() as u64);
        bump("with_follow_ups_and_choices", (!def.follow_ups.is_empty() && !def.choices.is_empty()) as u64);
        bump("with_add_topics", !def.add_topics.is_empty() as u64);
        bump("add_topic_links", def.add_topics.len() as u64);
        bump("with_previous_topic", def.previous_topic.is_some() as u64);
        if let Some(c) = def.challenge_form {
            let sig = d.sig(c).map(|s| String::from_utf8_lossy(&s).into_owned()).unwrap_or_else(|| "?".into());
            *challenge_kinds.entry(sig).or_default() += 1;
        }
        if let Some(x) = def.challenge_difficulty {
            *difficulty.entry(x.to_string()).or_default() += 1;
        }
        // The previous-info link against the stored order: the line before this one
        // among the lines of the same quest in its topic.
        let key = if !def.has_previous {
            "absent"
        } else {
            let stored: Vec<FormId> = def
                .topic
                .and_then(|t| d.topics.get(&t))
                .map(|ls| ls.iter().copied().filter(|x| d.infos.get(x).is_some_and(|o| o.quest == def.quest)).collect())
                .unwrap_or_default();
            let before = stored.iter().position(|x| *x == i).and_then(|k| k.checked_sub(1)).map(|k| stored[k]);
            match (def.previous, before) {
                (None, None) => "none_and_first_of_its_quest",
                (Some(a), Some(b)) if a == b => "the_line_before_it_in_its_quest",
                (None, Some(_)) => "none_but_not_first_of_its_quest",
                (Some(a), _) if stored.contains(&a) => "another_line_of_its_quest_in_the_topic",
                (Some(_), _) => "a_line_outside_the_topic_or_quest",
            }
        };
        *previous.entry(key.to_string()).or_default() += 1;
    }
    let rows: Vec<Value> = match info {
        Some(i) => {
            let i = FormId(i);
            let def = d.infos.get(&i).ok_or_else(|| anyhow::anyhow!("{i} is not a dialogue line (INFO)"))?;
            vec![line_row(l, i, def, f.text)]
        }
        None if sel.any() || f.all => {
            // Topic order: topics by FormID, lines as they are tried.
            let mut ts: Vec<&FormId> = d.topics.keys().collect();
            ts.sort();
            let mut rows = Vec::new();
            for t in ts {
                for i in d.lines_by_priority(*t) {
                    if let Some(def) = d.infos.get(&i).filter(|x| sel.line(x)) {
                        rows.push(line_row(l, i, def, f.text));
                    }
                }
            }
            rows
        }
        None => Vec::new(),
    };
    let mut summary = Map::new();
    summary.insert("lines".into(), json!(all.len()));
    for (k, v) in &n {
        summary.insert((*k).into(), json!(v));
    }
    summary.insert("by_type".into(), count(&by_type));
    summary.insert("type_differs_from_topic".into(), count(&type_pairs));
    summary.insert("by_plugin_of_first_definition".into(), count(&by_plugin));
    summary.insert("flags".into(), count(&flags));
    summary.insert("responses_per_line".into(), count(&per_line));
    summary.insert("emotions".into(), count(&emotions));
    summary.insert("next_speaker".into(), count(&next_speaker));
    summary.insert("challenge_form_types".into(), count(&challenge_kinds));
    summary.insert("challenge_difficulty".into(), count(&difficulty));
    summary.insert("record_flags".into(), count(&record_flags));
    summary.insert("previous_info".into(), count(&previous));
    // Quests whose own conditions (QUST CTDA before the stages) gate their lines.
    let gated: BTreeSet<FormId> = d.quests.values().filter(|q| !q.conditions.is_empty()).map(|q| q.form).collect();
    let gated_lines = all.iter().filter(|(_, def)| def.quest.is_some_and(|q| gated.contains(&q))).count();
    let gated_with_lines: BTreeSet<FormId> = all.iter().filter_map(|(_, def)| def.quest.filter(|q| gated.contains(q))).collect();
    summary.insert(
        "quest_conditions".into(),
        json!({
            "quests": d.quests.len(),
            "quests_with_conditions": gated.len(),
            "conditions": d.quests.values().map(|q| q.conditions.len()).sum::<usize>(),
            "quests_with_conditions_and_lines": gated_with_lines.len(),
            "lines_of_quests_with_conditions": gated_lines,
        }),
    );
    Ok(json!({
        "format": "fnv.dialogue.lines/1",
        "load_order": d.load_order,
        "summary": summary,
        "listed": rows.len(),
        "lines": rows,
    }))
}

/// `dialogue talk`: the Layer 4 world, the Layer 5 data, and the conversation run.
fn talk(a: &ScriptArgs, actions: &std::path::Path, text: bool, voice_lengths: bool) -> anyhow::Result<Value> {
    let doc = std::fs::read_to_string(actions).with_context(|| actions.display().to_string())?;
    let input: fnv_game::runner::Input =
        serde_json::from_str(&doc).with_context(|| format!("{}: not a fnv.dialogue.actions/1 document", actions.display()))?;
    let tp = table_path(a);
    let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
    let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
    let table = FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
    let plugins = open_load_order(&a.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let schema = Schema::load_dir(&a.schema)?;
    let forms = fnv_sim::FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &fjson, &refs);
    let voice = crate::script_cmd::voice_lengths_hook(a, &lo, voice_lengths);
    fnv_game::runner::run_dialogue_with_voice(&data, &forms, &refs, &input, text, voice).map_err(anyhow::Error::msg)
}
