//! `fnvtool world audio`: music and placed sounds (docs/spec-notes/audio.md).

use anyhow::{anyhow, Context};
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::FormId;
use fnv_world::audio::{self, AudioForms, AudioMarker, MediaSetKind, LAYERS};
use fnv_world::WorldIndex;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

fn fid(v: u32) -> Value {
    json!(formid_str(v))
}

fn opt_fid(v: Option<u32>) -> Value {
    v.map_or(Value::Null, fid)
}

/// The music and acoustic-space forms, and for a cell what sounds there.
pub fn audio(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, cell: Option<&str>, at: Option<&str>, hour: f32) -> anyhow::Result<Value> {
    let forms = AudioForms::build(lo, schema);
    let sounds = fnv_combat::fx::FxForms::build(lo, schema).sounds;
    let sound = |id: Option<u32>| match id {
        Some(id) => match sounds.get(&FormId(id)) {
            Some(s) => json!({"form_id": fid(id), "edid": s.edid, "file": s.file, "flags": s.flags, "min_distance": s.min_distance,
                "max_distance": s.max_distance, "static_attenuation_db": s.static_attenuation_db,
                "random_chance_percent": s.random_chance_percent, "start_hour": s.start_hour, "stop_hour": s.stop_hour}),
            None => json!({"form_id": fid(id)}),
        },
        None => Value::Null,
    };
    let set = |id: u32| match forms.sets.get(&id) {
        Some(s) => {
            let tracks: Map<String, Value> =
                (0..6).filter_map(|k| Some((LAYERS[k].to_string(), json!({"file": s.track(k)?, "db": s.db[k], "boundary_percent": s.boundary_percent[k]})))).collect();
            json!({"form_id": fid(id), "edid": s.edid, "kind": format!("{:?}", s.kind), "tracks": tracks, "times": s.times, "intro": opt_fid(s.intro), "outro": opt_fid(s.outro)})
        }
        None => json!({"form_id": fid(id)}),
    };
    let music = |id: Option<u32>| match id.and_then(|i| forms.music.get(&i)) {
        Some(m) => json!({"form_id": fid(m.form_id), "edid": m.edid, "file": m.file, "db": m.db}),
        None => opt_fid(id),
    };
    let controller = |id: Option<u32>| match id.and_then(|i| forms.controllers.get(&i)) {
        Some(c) => {
            let list = |v: &[u32]| v.iter().map(|&i| json!({"form_id": fid(i), "edid": forms.sets.get(&i).and_then(|s| s.edid.clone())})).collect::<Vec<_>>();
            json!({"form_id": fid(c.form_id), "edid": c.edid, "day_start_hour": c.day_start_hour, "night_start_hour": c.night_start_hour,
                "location_delay": c.location_delay, "retrigger_delay": c.retrigger_delay, "faction": opt_fid(c.faction),
                "neutral": list(&c.neutral), "ally": list(&c.ally), "friend": list(&c.friend), "enemy": list(&c.enemy),
                "location": list(&c.location), "battle": list(&c.battle)})
        }
        None => opt_fid(id),
    };
    let kinds = |k: MediaSetKind| forms.sets.values().filter(|s| s.kind == k).count();
    let mut out = json!({
        "schema": "fnv.world.audio/1",
        "counts": {
            "music_types": forms.music.len(),
            "media_sets": {"total": forms.sets.len(), "battle": kinds(MediaSetKind::Battle), "location": kinds(MediaSetKind::Location),
                "dungeon": kinds(MediaSetKind::Dungeon), "incidental": kinds(MediaSetKind::Incidental), "none": kinds(MediaSetKind::None)},
            "controllers": forms.controllers.len(),
            "controllers_with_faction": forms.controllers.values().filter(|c| c.faction.is_some()).count(),
            "controllers_with_location_set": forms.controllers.values().filter(|c| !c.location.is_empty()).count(),
            "controllers_with_several_location_sets": forms.controllers.values().filter(|c| c.location.len() > 1).count(),
            "controllers_with_faction_and_location_set": forms.controllers.values().filter(|c| c.faction.is_some() && !c.location.is_empty()).count(),
            "controllers_without_any_set": forms.controllers.values().filter(|c| c.location.is_empty() && c.neutral.is_empty()).count(),
            "acoustic_spaces": forms.spaces.len(),
        },
        "music_types": forms.music.values().map(|m| music(Some(m.form_id))).collect::<Vec<_>>(),
    });
    let Some(name) = cell else {
        out["world"] = survey(lo, schema, ix, &sounds, &sound);
        return Ok(out);
    };
    let c = ix.cell_by_name(name).ok_or_else(|| anyhow!("no cell {name}"))?;
    let a = audio::cell_audio(lo, schema, c.form_id);
    let (am, sm) = audio::markers(lo, schema, ix, audio::cells_in_earshot(ix, c.form_id));
    let space = a.acoustic_space.and_then(|s| forms.spaces.get(&s));
    out["cell"] = json!({
        "form_id": fid(c.form_id), "edid": c.edid, "interior": c.is_interior(),
        "music_type": music(a.music),
        "world_music_type": music(c.world.and_then(|w| audio::world_music(lo, schema, w))),
        "acoustic_space": match space {
            Some(s) => json!({"form_id": fid(s.form_id), "edid": s.edid, "dawn": sound(s.loops[0]), "afternoon": sound(s.loops[1]),
                "dusk": sound(s.loops[2]), "night": sound(s.loops[3]), "walla": sound(s.walla), "walla_trigger_count": s.walla_trigger_count,
                "region": opt_fid(s.region), "environment": s.environment, "is_interior": s.interior}),
            None => opt_fid(a.acoustic_space),
        },
    });
    let marker = |m: &AudioMarker| {
        json!({"ref": fid(m.ref_id), "cell": fid(m.cell), "name": m.name, "position": m.position, "radius": m.radius,
            "controller": opt_fid(m.controller), "controller_edid": m.controller.and_then(|i| forms.controllers.get(&i)).and_then(|c| c.edid.clone()),
            "use_controller_values": m.use_controller_values, "layer2_percent": m.layer2_percent, "layer3_percent": m.layer3_percent,
            "initially_disabled": m.initially_disabled})
    };
    let Some(at) = at else {
        out["audio_markers"] = am.iter().map(marker).collect();
        let mut ids: Vec<u32> = am.iter().filter_map(|m| m.controller).collect();
        ids.sort_unstable();
        ids.dedup();
        out["controllers"] = ids.into_iter().map(|i| controller(Some(i))).collect();
        out["sound_markers"] = sm
            .iter()
            .map(|m| {
                json!({"ref": fid(m.ref_id), "cell": fid(m.cell), "position": m.position, "sound": sound(Some(m.sound)),
                    "initially_disabled": m.initially_disabled,
                    "enable_parent": m.enable_parent.map(|(p, opposite)| json!({"ref": fid(p), "opposite": opposite}))})
            })
            .collect();
        return Ok(out);
    };
    let p: Vec<f32> = at.split(',').map(|t| t.trim().parse::<f32>()).collect::<Result<_, _>>().with_context(|| format!("--at {at}"))?;
    let [x, y, z] = p[..] else { return Err(anyhow!("--at takes x,y,z")) };
    let heard = audio::marker_at(&am, [x, y, z], |m| !m.initially_disabled).and_then(|(m, d)| {
        let ctl = forms.controllers.get(&m.controller?)?;
        // The game picks one of the controller's sets; the first stands for them here.
        let s = forms.sets.get(ctl.sets().first()?)?;
        let day = ctl.is_day(hour);
        let l = audio::layers(s, m, d, day);
        let base = if day { 0 } else { 3 };
        let tracks: Vec<Value> =
            (0..3).filter(|&k| l.on[k]).filter_map(|k| Some(json!({"layer": LAYERS[base + k], "file": s.track(base + k)?, "db": s.db[base + k]}))).collect();
        Some(json!({"marker": marker(m), "distance": d, "controller": controller(m.controller), "set": set(s.form_id), "sets_heard": ctl.sets().len(), "day": day, "tracks": tracks}))
    });
    let mut near: Vec<(f32, Value)> = sm
        .iter()
        .filter_map(|m| {
            let s = sounds.get(&FormId(m.sound))?;
            let q = m.position;
            let d = ((q[0] - x).powi(2) + (q[1] - y).powi(2) + (q[2] - z).powi(2)).sqrt();
            (d < s.max_distance).then(|| (d, json!({"ref": fid(m.ref_id), "distance": d, "sound": sound(Some(m.sound)), "initially_disabled": m.initially_disabled})))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    out["at"] = json!({"position": [x, y, z], "hour": hour, "music": heard, "acoustic_loop": sound(space.and_then(|s| s.loop_at(hour))),
        "sound_markers_in_reach": near.into_iter().map(|n| n.1).collect::<Vec<_>>()});
    Ok(out)
}

/// The whole load order: where the markers are and what they place.
fn survey(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, sounds: &std::collections::HashMap<FormId, fnv_combat::fx::Sound>, sound: &dyn Fn(Option<u32>) -> Value) -> Value {
    let (mut cells_with_music, mut cells_with_space) = (0, 0);
    let mut music_types: BTreeMap<u32, usize> = BTreeMap::new();
    for &c in ix.cells.keys() {
        let a = audio::cell_audio(lo, schema, c);
        cells_with_music += a.music.is_some() as u32;
        cells_with_space += a.acoustic_space.is_some() as u32;
        if let Some(m) = a.music {
            *music_types.entry(m).or_default() += 1;
        }
    }
    let (am, sm) = audio::markers(lo, schema, ix, ix.cells.keys().copied());
    let interior = |c: u32| ix.cells.get(&c).is_some_and(|c| c.world.is_none());
    let mut by_sound: BTreeMap<u32, usize> = BTreeMap::new();
    for m in &sm {
        *by_sound.entry(m.sound).or_default() += 1;
    }
    let flag = |name: &str| sm.iter().filter(|m| sounds.get(&FormId(m.sound)).is_some_and(|s| s.has(name))).count();
    let mut top: Vec<(u32, usize)> = by_sound.iter().map(|(&s, &n)| (s, n)).collect();
    top.sort_by_key(|&(s, n)| (std::cmp::Reverse(n), s));
    // Of the placed sounds: the random chances of the ones played at random, the hours
    // the sounds are limited to, and the cells with a music type that hold audio markers.
    let mut chances: BTreeMap<String, usize> = BTreeMap::new();
    let mut hours: BTreeMap<String, usize> = BTreeMap::new();
    for m in &sm {
        let Some(s) = sounds.get(&FormId(m.sound)) else { continue };
        if s.has("Play At Random") {
            *chances.entry(format!("{}", s.random_chance_percent)).or_default() += 1;
        }
        *hours.entry(format!("{:.2}-{:.2}", s.start_hour, s.stop_hour)).or_default() += 1;
    }
    let marker_cells: std::collections::BTreeSet<u32> = am.iter().map(|m| m.cell).collect();
    let silent_with_marker = marker_cells.iter().filter(|&&c| audio::cell_audio(lo, schema, c).music.is_some()).count();
    let radii: Vec<f32> = am.iter().map(|m| m.radius).filter(|r| *r > 0.0).collect();
    // The markers' own layer trigger values, by whether the marker uses its controller's.
    let mut triggers: BTreeMap<(bool, String, String), usize> = BTreeMap::new();
    for m in &am {
        *triggers.entry((m.use_controller_values, format!("{:.3}", m.layer2_percent), format!("{:.3}", m.layer3_percent))).or_default() += 1;
    }
    json!({
        "cells_with_music_type_and_audio_markers": silent_with_marker,
        "cells_with_audio_markers": marker_cells.len(),
        "random_sound_marker_chances": chances,
        "sound_marker_hours": hours,
        "marker_trigger_values": triggers.iter().map(|((own, l2, l3), n)| json!({"use_controller_values": own, "layer2": l2, "layer3": l3, "markers": n})).collect::<Vec<_>>(),
        "cells_with_music_type": cells_with_music,
        "cells_by_music_type": music_types.iter().map(|(&m, &n)| json!({"music_type": fid(m), "cells": n})).collect::<Vec<_>>(),
        "cells_with_acoustic_space": cells_with_space,
        "audio_markers": {"total": am.len(), "interior": am.iter().filter(|m| interior(m.cell)).count(),
            "without_controller": am.iter().filter(|m| m.controller.is_none()).count(),
            "without_radius": am.iter().filter(|m| m.radius <= 0.0).count(),
            "radius_min": radii.iter().copied().fold(f32::INFINITY, f32::min), "radius_max": radii.iter().copied().fold(0.0, f32::max),
            "use_controller_values": am.iter().filter(|m| m.use_controller_values).count(),
            "initially_disabled": am.iter().filter(|m| m.initially_disabled).count()},
        "sound_markers": {"total": sm.len(), "interior": sm.iter().filter(|m| interior(m.cell)).count(), "distinct_sounds": by_sound.len(),
            "loop": flag("Loop"), "play_at_random": flag("Play At Random"), "two_d": flag("2D"), "random_location": flag("Random Location"),
            "initially_disabled": sm.iter().filter(|m| m.initially_disabled).count(),
            "with_enable_parent": sm.iter().filter(|m| m.enable_parent.is_some()).count(),
            "most_placed": top.iter().take(12).map(|&(s, n)| json!({"sound": sound(Some(s)), "placed": n})).collect::<Vec<_>>()},
    })
}
