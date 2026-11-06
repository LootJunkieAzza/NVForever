//! `fnvtool voice set-draft` / `set-check`: player voice sets (docs/design/player-voices.md,
//! `fnv_game::player_voice`). A draft lists the lines one actor can say, tagged with the
//! event each answers; a person then reviews it. Sets hold FormIDs and tags, never text:
//! `--text` prints the texts beside the result for the review only (game data, never to
//! commit).

use crate::dialogue_cmd::{form, with_data, Loaded};
use crate::script_cmd::ScriptArgs;
use anyhow::{bail, Context};
use fnv_game::cond::{Cmp, Operand, RunOn};
use fnv_game::data::{topic_type, InfoDef};
use fnv_game::player_voice::{form_ref, PlayerVoice, SetLine, VoiceSet, FORMAT};
use fnv_script::vm::{ArgValue, Value as Arg};
use fnv_sim::FormId;
use fnv_voice::VoiceBank;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// The Combat-tab and Detection topics a player's voice says for what happens to it
/// (`fnv_game::combat_lines`). Crime, yield and trespass topics are the world's answers to
/// the player, not the player's, and are left out [design].
pub const COMBAT_EVENTS: &[&str] = &[
    "Attack", "PowerAttack", "FireExplosive", "Hit", "HealthHalf", "HealthQuarter", "Crippled", "Death", "NoAmmo", "WeaponBroke", "OnFire",
    "Poisoned", "ArmorIneffective", "WeaponIneffective", "Regenerating", "NormalToCombat", "AlertToCombat", "LostToCombat", "CombatToNormal",
    "CombatToLost", "StartCombatResponse", "AttackResponse", "DeathResponse", "AvoidThreat", "Flee", "KnockedBack",
];

/// Conditions that only say who the speaker is: a set's line drops them, since the
/// player says it in that voice [design].
const IDENTITY: &[&str] = &["GetIsID", "GetIsVoiceType", "GetIsSex", "GetIsRace", "GetIsReference", "GetPlayerTeammate", "GetRandomPercent", "GetIsCreature"];

/// Conditions that tie a line to a place.
const PLACE: &[&str] = &["GetInCell", "GetInWorldspace", "GetIsCurrentWorldspace", "GetInZone", "GetIsInRegion", "GetIsCurrentLocation", "GetInSameCell"];

/// Conditions that tie a line to quest progress.
const QUEST: &[&str] = &["GetStage", "GetStageDone", "GetQuestCompleted", "GetQuestRunning", "GetObjectiveCompleted", "GetObjectiveDisplayed"];

fn ref_arg(a: &ArgValue) -> Option<FormId> {
    match a {
        ArgValue::Value(Arg::Ref(r)) if *r != 0 => Some(FormId(*r)),
        _ => None,
    }
}

fn function_name(l: &Loaded, opcode: u16) -> String {
    l.data.table.get(opcode).map(|f| f.name.clone()).unwrap_or_else(|| format!("0x{opcode:04X}"))
}

/// Whether the line is said by `base` (or anyone with voice type `vt`): its fixed speaker,
/// or a `GetIsID base == 1` / `GetIsVoiceType vt == 1` on the subject.
fn said_by(l: &Loaded, d: &InfoDef, base: FormId, vt: Option<FormId>) -> bool {
    if d.speaker == Some(base) {
        return true;
    }
    d.conditions.iter().any(|c| {
        let name = function_name(l, c.opcode());
        let is_one = c.cmp == Cmp::Eq && c.rhs == Operand::Value(1.0) && c.run_on == RunOn::Subject;
        let arg = c.params.first().and_then(ref_arg);
        is_one && ((name == "GetIsID" && arg == Some(base)) || (name == "GetIsVoiceType" && vt.is_some() && arg == vt))
    })
}

/// A draft line and why it was tagged so.
struct Drafted {
    line: SetLine,
    topic: String,
}

fn draft_line(l: &Loaded, info: FormId, d: &InfoDef) -> Option<Drafted> {
    let topic = d.topic.map(|t| l.data.name(t)).unwrap_or_default();
    // Lines of a conversation (choices or a prompt) answer the player in dialogue.
    if !d.choices.is_empty() || d.prompt.as_ref().is_some_and(|p| !p.is_empty()) || d.responses.is_empty() {
        return None;
    }
    let others: Vec<String> = d.conditions.iter().map(|c| function_name(l, c.opcode())).filter(|n| !IDENTITY.contains(&n.as_str())).collect();
    let mut others_sorted = others.clone();
    others_sorted.sort();
    others_sorted.dedup();
    let note = if others_sorted.is_empty() { String::new() } else { format!("conditions: {}", others_sorted.join(", ")) };
    let combat = matches!(d.info_type, topic_type::COMBAT | topic_type::DETECTION);
    let line = |event: String, first_person: bool, use_line: bool, note: String| SetLine { info: form_ref(&l.data, info), event, first_person, use_line, note, file: None, text: None, near: vec![], inside: vec![] };
    if combat && COMBAT_EVENTS.contains(&topic.as_str()) {
        // Only the speaker's identity in its conditions: said in the voice as it is.
        let plain = others.is_empty();
        return Some(Drafted { line: line(topic.clone(), plain, plain, note), topic });
    }
    if combat {
        return None;
    }
    if let Some(rest) = topic.strip_prefix("PLAYER") {
        // A companion's word on what the player does: one player's word on another's.
        return Some(Drafted { line: line(format!("react.{}", rest.to_ascii_lowercase()), false, others.is_empty(), note), topic });
    }
    let has = |set: &[&str]| others.iter().any(|n| set.contains(&n.as_str()));
    let event = if has(PLACE) {
        "place"
    } else if has(QUEST) {
        "quest"
    } else {
        "idle"
    };
    // Place, quest and idle lines wait for a person's review (phase 4).
    Some(Drafted { line: line(event.to_string(), false, false, note), topic })
}

/// The arguments of `set-draft`.
pub struct DraftArgs<'a> {
    pub speaker: &'a str,
    pub id: &'a str,
    pub name: &'a str,
    pub sex: &'a str,
    pub rate: f32,
    pub companion: bool,
    pub out: Option<&'a Path>,
    pub text: bool,
    pub pack: bool,
    /// Every line with a file in the speaker's voice (a generic voice type's barks).
    pub voice_lines: bool,
}

/// `fnvtool voice set-draft`.
pub fn draft(a: &ScriptArgs, d: DraftArgs<'_>) -> anyhow::Result<Value> {
    let vfs = Arc::new(crate::voice_cmd::open_vfs(&a.lo.data_dir, &a.lo.plugins)?);
    with_data(a, |l| {
        let bank = VoiceBank::build(&l.lo, vfs);
        let base = form(l, d.speaker)?;
        let vt = l.data.actors.get(&base).and_then(|x| x.voice);
        // A follower mod's voice type is its own: every line with a file in it is the
        // speaker's, also those its quest's conditions give it rather than the line's.
        let mut infos: Vec<(&FormId, &InfoDef)> = l.data.infos.iter().filter(|(i, def)| said_by(l, def, base, vt) || ((d.pack || d.voice_lines) && bank.file(i.0, 0, base.0).is_some())).collect();
        infos.sort_by_key(|(i, _)| i.0);
        let mut lines = Vec::new();
        let mut texts = Vec::new();
        let (mut no_file, mut skipped, mut multi) = (0, 0, 0);
        for (&info, def) in infos {
            let Some(dr) = draft_line(l, info, def) else {
                skipped += 1;
                continue;
            };
            // Every response must have its file in the speaker's voice.
            if !(0..def.responses.len()).all(|k| bank.file(info.0, k, base.0).is_some()) {
                no_file += 1;
                continue;
            }
            let mut dr = dr;
            if d.pack {
                // One response only: a carried line is one sound and one subtitle.
                if def.responses.len() != 1 {
                    multi += 1;
                    continue;
                }
                dr.line.file = bank.file(info.0, 0, base.0);
                dr.line.text = Some(def.responses[0].text.clone());
            }
            if d.text {
                let t: Vec<&str> = def.response_order().into_iter().map(|r| def.responses[r].text.as_str()).collect();
                texts.push(json!({"info": dr.line.info, "topic": dr.topic, "event": dr.line.event, "use": dr.line.use_line, "text": t.join(" / ")}));
            }
            lines.push(dr.line);
        }
        // The race the voice is offered to is the speaker's ([`fnv_game::player_voice::race_kind`]).
        let race_edid = l.data.actors.get(&base).and_then(|x| x.race).and_then(|r| l.data.edid_of.get(&r).cloned()).unwrap_or_default();
        let set = VoiceSet {
            format: FORMAT.into(),
            id: d.id.into(),
            name: d.name.into(),
            sex: d.sex.into(),
            race: fnv_game::player_voice::race_kind(&race_edid).into(),
            blurb: String::new(),
            speaker: if d.pack { String::new() } else { form_ref(&l.data, base) },
            rate: d.rate,
            companion: (d.companion && !d.pack).then(|| form_ref(&l.data, base)),
            lines,
        };
        let mut by_event: BTreeMap<String, [u32; 2]> = BTreeMap::new();
        for x in &set.lines {
            let e = by_event.entry(x.event.clone()).or_default();
            e[0] += 1;
            e[1] += u32::from(x.use_line);
        }
        if let Some(out) = d.out {
            let body = serde_json::to_string_pretty(&set)?;
            std::fs::write(out, body + "\n").with_context(|| out.display().to_string())?;
        }
        let mut v = json!({
            "format": "fnv.voice.set_draft/1",
            "speaker": l.data.name(base),
            "voice_type": vt.map(|t| l.data.name(t)),
            "race": race_edid,
            "speaker_female": l.data.actors.get(&base).map(|x| x.female),
            "lines": set.lines.len(),
            "without_file": no_file,
            "not_for_a_set": skipped,
            "more_than_one_response": multi,
            "by_event": by_event.iter().map(|(k, [n, used])| (k.clone(), json!({"lines": n, "in_use": used}))).collect::<serde_json::Map<_, _>>(),
            "written": d.out.map(|p| p.display().to_string()),
        });
        if d.text {
            v["texts"] = json!(texts);
        } else {
            v["set"] = serde_json::to_value(&set)?;
        }
        Ok(v)
    })
}

/// `fnvtool voice set-places` (see its help).
pub fn places(a: &ScriptArgs, file: &Path, prefixes: &[String], given: &[String]) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(file).with_context(|| file.display().to_string())?;
    let mut set: VoiceSet = serde_json::from_str(&text).with_context(|| file.display().to_string())?;
    with_data(a, |l| {
        let d = &l.data;
        let wanted = |t: FormId| {
            let name = d.name(t);
            prefixes.iter().any(|p| name.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase()))
        };
        // Scripts naming each wanted topic, the bases that run them, their references.
        let mut scripts_of: BTreeMap<FormId, Vec<FormId>> = BTreeMap::new();
        for (&scpt, &unit) in &d.scripts {
            for slot in &d.unit(unit).unit.refs {
                if let fnv_script::unit::RefSlot::Form(t) = slot {
                    if d.topic_defs.contains_key(&FormId(*t)) && wanted(FormId(*t)) {
                        scripts_of.entry(FormId(*t)).or_default().push(scpt);
                    }
                }
            }
        }
        let mut bases_of_script: BTreeMap<FormId, Vec<FormId>> = BTreeMap::new();
        for (&base, &scpt) in &d.form_script {
            bases_of_script.entry(scpt).or_default().push(base);
        }
        let mut refs_of_base: BTreeMap<FormId, Vec<FormId>> = BTreeMap::new();
        for (&r, p) in &l.refs.refs {
            refs_of_base.entry(p.base).or_default().push(r);
        }
        let mut rows = Vec::new();
        for line in set.lines.iter_mut() {
            // A line that carries its sound (a follower mod's) is said without the mod's
            // plugin: its places are only the cells and worldspaces of other plugins.
            let carried = line.file.is_some();
            let own_plugin = line.info.split_once('|').map(|(p, _)| p.to_ascii_lowercase()).unwrap_or_default();
            let Some(info) = fnv_game::player_voice::resolve_ref(d, &line.info) else { continue };
            let Some(def) = d.infos.get(&info) else { continue };
            let Some(topic) = def.topic.filter(|&t| wanted(t)) else { continue };
            let mut near: Vec<FormId> = scripts_of
                .get(&topic)
                .into_iter()
                .flatten()
                .flat_map(|s| bases_of_script.get(s).into_iter().flatten())
                .flat_map(|b| refs_of_base.get(b).into_iter().flatten())
                .copied()
                .collect();
            near.sort();
            near.dedup();
            if carried {
                near.clear();
            }
            let mut inside: Vec<FormId> = def
                .conditions
                .iter()
                .filter(|c| matches!(function_name(l, c.opcode()).as_str(), "GetInCell" | "GetInWorldspace" | "GetIsCurrentWorldspace"))
                .filter_map(|c| c.params.first().and_then(ref_arg))
                .collect();
            if let Some((_, list)) = given.iter().filter_map(|g| g.split_once('=')).find(|(t, _)| t.eq_ignore_ascii_case(&d.name(topic))) {
                for e in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                    match d.edids.get(&e.to_ascii_lowercase()) {
                        Some(&f) => inside.push(f),
                        None => bail!("--place {}: no record {e}", d.name(topic)),
                    }
                }
            }
            inside.sort();
            inside.dedup();
            if carried {
                inside.retain(|&r| !form_ref(d, r).to_ascii_lowercase().starts_with(&format!("{own_plugin}|")));
            }
            if near.is_empty() && inside.is_empty() {
                let conds: Vec<String> = def.conditions.iter().map(|c| format!("{}({})", function_name(l, c.opcode()), c.params.iter().filter_map(ref_arg).map(|r| d.name(r)).collect::<Vec<_>>().join(","))).collect();
                rows.push(json!({"info": line.info, "topic": d.name(topic), "found": "nothing", "conditions": conds}));
                continue;
            }
            line.event = "place".into();
            line.use_line = true;
            line.near = near.iter().map(|&r| form_ref(d, r)).collect();
            line.inside = inside.iter().map(|&r| form_ref(d, r)).collect();
            rows.push(json!({"info": line.info, "topic": d.name(topic), "near": near.iter().map(|&r| d.name(r)).collect::<Vec<_>>(), "inside": inside.iter().map(|&r| d.name(r)).collect::<Vec<_>>()}));
        }
        std::fs::write(file, serde_json::to_string_pretty(&set)? + "
").with_context(|| file.display().to_string())?;
        Ok(json!({"format": "fnv.voice.set_places/1", "set": set.id, "lines": rows}))
    })
}

/// `fnvtool voice set-check`: each set resolves against the load order, and every
/// response of every line in use has its file in the set's speaker's voice.
pub fn check(a: &ScriptArgs, files: &[std::path::PathBuf], mod_dir: Option<&Path>) -> anyhow::Result<Value> {
    let vfs = Arc::new(crate::voice_cmd::open_vfs(&a.lo.data_dir, &a.lo.plugins)?);
    with_data(a, |l| {
        let bank = VoiceBank::build(&l.lo, vfs);
        let mut rows = Vec::new();
        let mut bad = 0;
        for f in files {
            let text = std::fs::read_to_string(f).with_context(|| f.display().to_string())?;
            let full: VoiceSet = serde_json::from_str(&text).with_context(|| f.display().to_string())?;
            // Lines that carry their sound: their file is in the data or the mod folder,
            // and they have a subtitle. The rest resolve against the load order.
            let mut missing = Vec::new();
            let mut carried = 0;
            let mut carried_events: BTreeMap<String, usize> = BTreeMap::new();
            for x in full.lines.iter().filter(|x| x.use_line) {
                let Some(file) = &x.file else { continue };
                carried += 1;
                *carried_events.entry(x.event.clone()).or_default() += 1;
                let there = bank.vfs().exists(file) || mod_dir.is_some_and(|m| m.join(file.replace('\\', "/")).is_file());
                if !there {
                    missing.push(format!("{} ({file})", x.info));
                }
                if x.text.as_deref().is_none_or(str::is_empty) {
                    missing.push(format!("{}: no subtitle", x.info));
                }
            }
            let mut set = full.clone();
            set.lines.retain(|x| x.file.is_none());
            let (v, unresolved) = match PlayerVoice::resolve(&set, &l.data) {
                Ok(x) => x,
                Err(e) => {
                    bad += 1;
                    rows.push(json!({"file": f.display().to_string(), "error": e}));
                    continue;
                }
            };
            let mut responses = carried;
            for &info in &v.lines {
                let n = l.data.infos.get(&info).map_or(0, |d| d.responses.len());
                for k in 0..n {
                    responses += 1;
                    if bank.file(info.0, k, v.base.0).is_none() {
                        missing.push(format!("{} response {k}", form_ref(&l.data, info)));
                    }
                }
            }
            let mut events = carried_events;
            for (e, ls) in &v.by_event {
                *events.entry(e.clone()).or_default() += ls.len();
            }
            let first_person = v.first_person.len() + full.lines.iter().filter(|x| x.use_line && x.file.is_some() && x.first_person).count();
            bad += usize::from(!missing.is_empty() || unresolved > 0);
            rows.push(json!({
                "file": f.display().to_string(), "id": v.id, "speaker": l.data.name(v.base), "rate": v.rate,
                "lines_in_use": v.lines.len() + carried, "carried": carried, "first_person": first_person, "responses": responses,
                "unresolved": unresolved, "missing_files": missing, "by_event": events,
            }));
        }
        if bad > 0 {
            eprintln!("voice set-check: {bad} set(s) with problems");
        }
        let v = json!({"format": "fnv.voice.set_check/1", "sets": rows, "ok": bad == 0});
        if bad > 0 {
            println!("{}", serde_json::to_string_pretty(&v)?);
            bail!("{bad} voice set(s) do not check");
        }
        Ok(v)
    })
}
