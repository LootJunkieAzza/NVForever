//! `fnvtool world radio`: radio stations, their placed references and broadcast range,
//! the receivers that play them and the Radio lines their quests give them
//! (docs/spec-notes/radio.md). Texts are game data and are not printed.

use crate::esm_cmd::open_load_order;
use crate::WorldArgs;
use anyhow::Context;
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_game::data::topic_type;
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};
use fnv_world::radio::{self, RadioForms};
use fnv_world::WorldIndex;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// GetIsID's function index (opcode 0x1048) [documented: the function catalogue].
const GET_IS_ID: u16 = 72;

fn fid(v: u32) -> Value {
    json!(formid_str(v))
}

fn count<K: Ord + ToString>(m: &BTreeMap<K, u64>) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<Map<_, _>>())
}

pub fn run(w: &WorldArgs, functions: Option<&std::path::Path>) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let tp = functions.map(|p| p.to_path_buf()).unwrap_or_else(|| w.schema.join("..").join("script").join("functions.json"));
    let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
    let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
    let table = FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &fjson, &refs);
    let ix = WorldIndex::build(&lo, &schema);
    let vfs = fnv_vfs::Vfs::open_default(&w.lo.data_dir).with_context(|| w.lo.data_dir.display().to_string())?;
    let voices = fnv_voice::index::VoiceIndex::build(&lo);
    let mut v = survey(&lo, &schema, &ix, &data, &vfs, &voices);
    v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
    Ok(v)
}

fn survey(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, data: &GameData, vfs: &fnv_vfs::Vfs, voices: &fnv_voice::index::VoiceIndex) -> Value {
    let forms = RadioForms::build(lo, schema);
    let sounds = fnv_combat::fx::FxForms::build(lo, schema).sounds;
    let (station_refs, receivers) = radio::placed(lo, schema, ix, &forms, ix.cells.keys().copied());
    let edid = |f: u32| data.edid_of.get(&FormId(f)).cloned();
    let sig = |f: u32| data.sigs.get(&FormId(f)).map(|s| String::from_utf8_lossy(s).into_owned());
    let cell_name = |c: u32| ix.cells.get(&c).and_then(|c| c.edid.clone());
    let sound = |id: Option<u32>| match id {
        Some(id) => match sounds.get(&FormId(id)) {
            Some(s) => json!({"form": fid(id), "edid": s.edid, "file": s.file, "flags": s.flags, "min_distance": s.min_distance,
                "max_distance": s.max_distance, "static_attenuation_db": s.static_attenuation_db}),
            None => json!({"form": fid(id)}),
        },
        None => Value::Null,
    };

    // Quests whose own conditions ask GetIsID of a form: the station they speak through.
    let mut quests_of: BTreeMap<u32, Vec<FormId>> = BTreeMap::new();
    let mut line_conditions_of: BTreeMap<u32, u64> = BTreeMap::new();
    for (q, d) in &data.quests {
        for c in d.conditions.iter().filter(|c| c.function == GET_IS_ID) {
            if let Some(fnv_script::vm::Value::Ref(r)) = c.params.first().and_then(|p| p.value()) {
                quests_of.entry(r).or_default().push(*q);
            }
        }
    }
    for d in data.infos.values() {
        for c in d.conditions.iter().filter(|c| c.function == GET_IS_ID) {
            if let Some(fnv_script::vm::Value::Ref(r)) = c.params.first().and_then(|p| p.value()) {
                if forms.stations.contains_key(&r) {
                    *line_conditions_of.entry(r).or_default() += 1;
                }
            }
        }
    }

    let mut flag_combos: BTreeMap<String, u64> = BTreeMap::new();
    let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
    let mut song_files: BTreeSet<String> = BTreeSet::new();
    let mut stations = Vec::new();
    for s in forms.stations.values() {
        *flag_combos.entry(format!("0x{:08X}", s.flags)).or_default() += 1;
        let quests: Vec<FormId> = quests_of.get(&s.form_id).cloned().unwrap_or_default();
        let mut by_topic: BTreeMap<String, u64> = BTreeMap::new();
        let mut c: BTreeMap<&str, u64> = BTreeMap::new();
        let mut files: BTreeMap<String, u64> = BTreeMap::new();
        let mut link_topics: BTreeSet<FormId> = BTreeSet::new();
        for (i, d) in data.infos.iter().filter(|(_, d)| d.quest.is_some_and(|q| quests.contains(&q))) {
            *c.entry("lines").or_default() += 1;
            if d.info_type == topic_type::RADIO {
                *c.entry("radio_lines").or_default() += 1;
            }
            let t = d.topic.and_then(|t| data.topic_defs.get(&t));
            *by_topic.entry(t.and_then(|t| t.edid.clone()).unwrap_or_default()).or_default() += 1;
            link_topics.extend(d.choices.iter().copied());
            for k in ["random", "random_end", "say_once", "with_conditions", "with_begin_script", "with_end_script", "with_choices"] {
                let yes = match k {
                    "random" => d.has(fnv_game::data::info_flag::RANDOM),
                    "random_end" => d.has(fnv_game::data::info_flag::RANDOM_END),
                    "say_once" => d.say_once,
                    "with_conditions" => !d.conditions.is_empty(),
                    "with_begin_script" => d.begin.is_some(),
                    "with_end_script" => d.end.is_some(),
                    _ => !d.choices.is_empty(),
                };
                if yes {
                    *c.entry(k).or_default() += 1;
                }
            }
            for (k, r) in d.responses.iter().enumerate() {
                *c.entry("responses").or_default() += 1;
                let voice = s.voice_type.is_some_and(|vt| voices.paths(i.0, k, vt).iter().any(|p| vfs.exists(p)));
                if voice {
                    *c.entry("responses_with_voice_file").or_default() += 1;
                }
                match r.sound.and_then(|id| sounds.get(&id)) {
                    Some(snd) => {
                        *c.entry("responses_with_sound").or_default() += 1;
                        if voice {
                            *c.entry("responses_with_sound_and_voice_file").or_default() += 1;
                        }
                        let file = snd.file.clone().unwrap_or_default().to_ascii_lowercase().replace('/', "\\");
                        let path = fnv_vfs::normalize(&format!("sound\\{file}"));
                        let ext = file.rsplit_once('.').map_or("folder", |(_, e)| e).to_string();
                        *files.entry(ext).or_default() += 1;
                        if !vfs.exists(&path) {
                            *c.entry("sound_files_missing").or_default() += 1;
                        }
                        if file.starts_with("songs\\") {
                            song_files.insert(path);
                        }
                        let two_d = snd.has("2D");
                        *c.entry(if two_d { "sounds_2d" } else { "sounds_not_2d" }).or_default() += 1;
                    }
                    None if r.sound.is_some() => *c.entry("responses_with_unknown_sound").or_default() += 1,
                    None => {}
                }
            }
        }
        for (k, n) in &c {
            *totals.entry(k).or_default() += n;
        }
        let refs: Vec<Value> = station_refs
            .iter()
            .filter(|r| r.base == s.form_id)
            .map(|r| {
                json!({"ref": fid(r.ref_id), "cell": cell_name(r.cell), "initially_disabled": r.initially_disabled,
                    "radio_data": r.radio.map(|(radius, range, statics, pos)| json!({"range_radius": radius, "range": format!("{range:?}"),
                        "static_percent": statics, "position_ref": pos.map(fid)})),
                    "broadcasts_from": radio::broadcast(lo, schema, ix, r).and_then(|b| b.exterior).map(|(w, p)| json!({"world": fid(w), "position": p}))})
            })
            .collect();
        let placed_receivers = receivers.iter().filter(|r| r.station == s.form_id || station_refs.iter().any(|sr| sr.ref_id == r.station && sr.base == s.form_id)).count();
        if !s.is_radio() && quests.is_empty() && placed_receivers == 0 {
            continue;
        }
        stations.push(json!({
            "form": fid(s.form_id), "edid": s.edid, "flags": format!("0x{:08X}", s.flags), "radio_station_flag": s.is_radio(),
            "continuous_flag": s.continuous(), "on_pipboy": s.on_pipboy(), "voice_type": s.voice_type.map(fid), "voice_type_edid": s.voice_type.and_then(edid),
            "template": sound(s.template), "looping": sound(s.looping), "refs": refs,
            "quests": quests.iter().map(|q| json!({"form": fid(q.0), "edid": edid(q.0)})).collect::<Vec<_>>(),
            "lines_conditioned_on_it": line_conditions_of.get(&s.form_id).copied().unwrap_or(0),
            "lines": count(&c), "lines_by_topic": count(&by_topic), "sound_file_types": count(&files),
            "topics_linked_to": link_topics.iter().map(|t| data.topic_defs.get(t).and_then(|d| d.edid.clone())).collect::<Vec<_>>(),
            "receivers_placed": placed_receivers,
        }));
    }

    // Receivers: what their station field points at, scripts, placements.
    let mut target_sigs: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_script: BTreeMap<String, u64> = BTreeMap::new();
    let bases: Vec<Value> = forms
        .receivers
        .values()
        .map(|b| {
            *target_sigs.entry(sig(b.station).unwrap_or_else(|| "none".into())).or_default() += 1;
            let placed: Vec<&radio::Receiver> = receivers.iter().filter(|r| r.base == b.form_id).collect();
            *by_script.entry(b.script.and_then(edid).unwrap_or_else(|| "(none)".into())).or_default() += placed.len() as u64;
            let station_base = station_refs.iter().find(|r| r.ref_id == b.station).map_or(b.station, |r| r.base);
            json!({"form": fid(b.form_id), "edid": b.edid, "station": fid(b.station), "station_sig": sig(b.station),
                "station_edid": edid(b.station), "station_base_edid": edid(station_base), "template": sound(b.template),
                "script": b.script.and_then(edid), "destructible": b.destructible, "placed": placed.len(),
                "placed_initially_disabled": placed.iter().filter(|r| r.initially_disabled).count(),
                "placed_with_enable_parent": placed.iter().filter(|r| r.enable_parent.is_some()).count(),
                "placed_refs": placed.iter().map(|r| json!([fid(r.ref_id), cell_name(r.cell)])).collect::<Vec<_>>()})
        })
        .collect();
    let mut range_types: BTreeMap<String, u64> = BTreeMap::new();
    for r in &station_refs {
        *range_types.entry(r.radio.map_or("no XRDO".into(), |x| format!("{:?}", x.1))).or_default() += 1;
    }
    let goodsprings: Vec<Value> = receivers
        .iter()
        .filter(|r| cell_name(r.cell).is_some_and(|c| c.to_ascii_lowercase().contains("goodsprings") || c.starts_with("GS")))
        .map(|r| json!({"ref": fid(r.ref_id), "base": edid(r.base), "station": edid(r.station), "cell": cell_name(r.cell), "position": r.position,
            "initially_disabled": r.initially_disabled}))
        .collect();
    json!({
        "schema": "fnv.world.radio/1",
        "counts": {
            "talking_activators": forms.stations.len(),
            "talking_activators_with_radio_station_flag": forms.stations.values().filter(|s| s.is_radio()).count(),
            "talking_activators_with_continuous_flag": forms.stations.values().filter(|s| s.continuous()).count(),
            "radio_stations_on_the_pipboy": forms.stations.values().filter(|s| s.on_pipboy()).count(),
            "talking_activator_flags": count(&flag_combos),
            "station_refs_placed": station_refs.len(),
            "station_ref_range_types": count(&range_types),
            "receiver_bases": forms.receivers.len(),
            "receiver_station_field_points_at": count(&target_sigs),
            "receivers_placed": receivers.len(),
            "receivers_initially_disabled": receivers.iter().filter(|r| r.initially_disabled).count(),
            "receivers_placed_by_script": count(&by_script),
            "station_lines": count(&totals.iter().map(|(k, v)| (k.to_string(), *v)).collect()),
            "song_files_named": song_files.len(),
            "song_files_present": song_files.iter().filter(|p| vfs.exists(p)).count(),
        },
        "stations": stations,
        "receiver_bases": bases,
        "goodsprings_receivers": goodsprings,
    })
}
