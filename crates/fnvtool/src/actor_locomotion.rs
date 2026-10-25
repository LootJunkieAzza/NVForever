//! `fnvtool actor locomotion`: the locomotion clips of a skeleton folder and their root
//! motion. Contract: docs/spec-notes/actors.md §8.5.

use fnv_actor::anims;
use fnv_actor::locomotion::{root_motion, root_track, yaw_of, Body, LocomotionSet, Move};
use fnv_nif::anim::{Channel, TrackData};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn kind<const N: usize>(c: &Channel<N>) -> &'static str {
    match c {
        Channel::Static(Some(_)) => "static",
        Channel::Static(None) => "none",
        Channel::Keys(_) => "keys",
        Channel::BSpline { .. } => "bspline",
        Channel::Path(_) => "path",
    }
}

fn r3(v: [f32; 3]) -> Value {
    json!(v.map(|x| (f64::from(x) * 1000.0).round() / 1000.0))
}

/// The stance prefix of a locomotion file stem (`1hpfastforward` → `1hp`): the stem
/// minus the longest move name it ends with.
pub fn stance_prefix(stem: &str) -> Option<String> {
    Move::ALL
        .iter()
        .map(|m| m.group().to_ascii_lowercase())
        .filter(|g| stem.ends_with(g.as_str()))
        .max_by_key(String::len)
        .map(|g| stem[..stem.len() - g.len()].to_string())
}

pub fn report(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, folder: &str, samples: usize) -> Value {
    let folder = anims::normalize(folder).trim_end_matches('\\').to_string();
    let loco = format!("{folder}\\locomotion\\");
    // Also a move's clip kept beside the skeleton (`creatures\blowfly\mtforward.kf`,
    // mistergutsy, queenant [observed: the base meshes archive, 2026-10-25]), the last
    // place the engine looks (`fnv_actor::locomotion::clip_candidates`).
    let beside = format!("{folder}\\");
    let paths: BTreeSet<String> = vfs
        .paths(|p| {
            p.ends_with(".kf")
                && (p.starts_with(&loco)
                    || p.strip_prefix(&beside).is_some_and(|rest| !rest.contains('\\') && stance_prefix(rest.trim_end_matches(".kf")).is_some()))
        })
        .into_iter()
        .collect();
    let mut files = Vec::new();
    let mut prefixes = BTreeSet::new();
    for path in &paths {
        let stem = anims::file_name(path).trim_end_matches(".kf");
        if let Some(p) = stance_prefix(stem) {
            prefixes.insert(p);
        }
        let data = match vfs.read(path) {
            Some(Ok(d)) => d,
            other => {
                files.push(json!({"path": path, "error": other.map_or("not found".to_string(), |r| r.err().map(|e| e.to_string()).unwrap_or_default())}));
                continue;
            }
        };
        let file = match fnv_nif::file::NifFile::parse(&data, Some(nif)) {
            Ok(f) => f,
            Err(e) => {
                files.push(json!({"path": path, "error": e.to_string()}));
                continue;
            }
        };
        let dec = fnv_nif::scene::Decoded::new(&file, &data, nif);
        let mut seqs = Vec::new();
        for s in fnv_nif::anim::sequences(&file, &dec) {
            let rm = root_motion(&s);
            let mut v = json!({
                "name": s.name,
                "start_time": s.start_time,
                "stop_time": s.stop_time,
                "cycle_type": s.cycle_type,
                "frequency": s.frequency,
                "accum_root": rm.node,
                "root_driven": rm.driven,
                "root_translation": r3(rm.translation),
                "root_yaw_deg": (f64::from(rm.yaw.to_degrees()) * 100.0).round() / 100.0,
                "speed": (f64::from(rm.speed()) * 100.0).round() / 100.0,
                "turn_deg_per_s": (f64::from(rm.turn_rate().to_degrees()) * 100.0).round() / 100.0,
                "text_keys": s.text_keys.iter().map(|(t, k)| json!([t, k])).collect::<Vec<_>>(),
            });
            if let Some(tt) = root_track(&s) {
                v["root_channels"] = json!({"translation": kind(&tt.translation), "rotation": if tt.euler.is_some() { "euler" } else { kind(&tt.rotation) }, "scale": kind(&tt.scale)});
                v["root_start"] = json!({
                    "translation": tt.translation.sample(s.start_time).map(r3),
                    "rotation": tt.rotation_at(s.start_time).map(|q| q.map(|x| (f64::from(x) * 1e4).round() / 1e4)),
                    "scale": tt.scale.sample(s.start_time),
                });
            }
            if samples > 1 {
                let mut by_node = BTreeMap::new();
                let root = rm.node.clone().unwrap_or_default();
                for t in &s.tracks {
                    let (Some(n), TrackData::Transform(tt)) = (&t.node_name, &t.data) else { continue };
                    if *n != root && n != "Bip01 NonAccum" {
                        continue;
                    }
                    let rows: Vec<Value> = (0..samples)
                        .map(|i| {
                            let time = s.start_time + (s.stop_time - s.start_time) * i as f32 / (samples - 1) as f32;
                            json!({
                                "t": (f64::from(time) * 1000.0).round() / 1000.0,
                                "translation": tt.translation.sample(time).map(r3),
                                "yaw_deg": tt.rotation_at(time).map(|q| (f64::from(yaw_of(q).to_degrees()) * 100.0).round() / 100.0),
                            })
                        })
                        .collect();
                    by_node.insert(n.clone(), rows);
                }
                v["samples"] = json!(by_node);
            }
            seqs.push(v);
        }
        files.push(json!({"path": path, "sequences": seqs}));
    }
    let exists = |p: &str| paths.contains(p);
    let mut sets = Vec::new();
    for body in [Body::Male, Body::Female, Body::Child] {
        for prefix in &prefixes {
            let set = LocomotionSet::find(&folder, body, prefix, exists);
            let missing: Vec<&str> = Move::ALL.iter().filter(|m| !set.clips.contains_key(m)).map(|m| m.group()).collect();
            sets.push(json!({"body": body, "prefix": prefix, "clips": set.clips, "missing": missing}));
        }
    }
    json!({
        "schema": "fnv.actor.locomotion/1",
        "folder": folder,
        "files": files,
        "prefixes": prefixes,
        "sets": sets,
        "archives": vfs.archives().collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_from_stems() {
        assert_eq!(stance_prefix("1hpfastforward").as_deref(), Some("1hp"));
        assert_eq!(stance_prefix("mtidle").as_deref(), Some("mt"));
        assert_eq!(stance_prefix("pamtforward").as_deref(), Some("pamt"));
        assert_eq!(stance_prefix("mtturnright").as_deref(), Some("mt"));
        assert_eq!(stance_prefix("handgrip"), None);
    }
}
