//! `fnvtool actor creator` (fnv.actor.creator/1, docs/spec-notes/character-creator.md
//! §7): what the character creator offers — playable races with their hairs and eyes,
//! beards, the authored hair colours, the starting faces — and its sliders, with each
//! slider's range for each sex, the player record's reading, and what one unit of it
//! does: for a texture slider the colour it adds to the body skin (one colour per mode,
//! actors.md §4.1), for a shape slider the head vertex it moves most and which way.

use fnv_actor::creator::{Creator, Space};
use fnv_actor::facegen::{Egm, Egt};
use fnv_actor::records::FaceGen;
use fnv_actor::ActorForms;
use fnv_sim::FormId;
use serde_json::{json, Value};

const HEAD_EGM: [&str; 2] = ["meshes\\characters\\head\\headhuman.egm", "meshes\\characters\\head\\headfemale.egm"];
const HEAD_TRI: [&str; 2] = ["meshes\\characters\\head\\headhuman.tri", "meshes\\characters\\head\\headfemale.tri"];
const BODY_EGT: [&str; 2] = ["meshes\\characters\\_male\\upperbodyhumanmale.egt", "meshes\\characters\\_male\\upperbodyhumanfemale.egt"];

fn round(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

pub fn report(forms: &ActorForms, read: impl Fn(&str) -> Option<Vec<u8>>) -> Value {
    let c = Creator::build(forms, &read);
    let cat = &c.catalogue;
    let name_of = |id: FormId| -> String {
        forms.npcs.get(&id).and_then(|n| n.edid.clone()).unwrap_or_else(|| id.to_string())
    };
    let races: Vec<Value> = cat
        .races
        .iter()
        .map(|r| {
            let sex = |i: usize| {
                json!({
                    "hairs": r.hairs[i].iter().map(|h| &h.name).collect::<Vec<_>>(),
                    "eyes": r.eyes[i].iter().map(|e| &e.name).collect::<Vec<_>>(),
                    "default_hair": r.default_hair[i].map(|h| h.to_string()),
                    "presets": cat.presets(r.id, i == 1).iter().map(|f| name_of(f.npc)).collect::<Vec<_>>(),
                    "faces": cat.faces.iter().filter(|f| f.race == r.id && f.female == (i == 1)).count(),
                })
            };
            json!({ "id": r.id.to_string(), "edid": r.edid, "name": r.name, "older": r.older.map(|o| o.to_string()), "male": sex(0), "female": sex(1) })
        })
        .collect();
    let player = forms.npcs.get(&FormId::PLAYER_BASE);
    let player_race = player.and_then(|p| p.race).and_then(|r| forms.races.get(&r));
    let effects: Vec<Option<(Egm, Vec<[f32; 3]>, Egt)>> = (0..2)
        .map(|i| {
            let egm = Egm::parse(&read(HEAD_EGM[i])?).ok()?;
            let tri = fnv_actor::tri::Tri::parse(&read(HEAD_TRI[i])?).ok()?;
            let egt = Egt::parse(&read(BODY_EGT[i])?).ok()?;
            Some((egm, tri.positions[..tri.vertices].to_vec(), egt))
        })
        .collect();
    let sliders: Vec<Value> = c.sliders[0]
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let f = &c.sliders[1][k];
            let mut v = json!({
                "label": s.label, "group": s.group, "aspect": s.aspect, "low": s.low, "high": s.high,
                "space": format!("{:?}", s.space()),
                "range_male": [round(s.range.0, 3), round(s.range.1, 3)],
                "range_female": [round(f.range.0, 3), round(f.range.1, 3)],
            });
            if let (Some(p), Some(r)) = (player.and_then(|p| p.facegen.as_ref()), player_race) {
                v["player"] = json!(round(s.value(&fnv_actor::creator::full(p), r.facegen[0].as_ref()), 3));
            }
            // One unit along the slider, from a blank face, on the male files.
            if let Some((egm, positions, egt)) = &effects[0] {
                let mut fg = fnv_actor::creator::full(&FaceGen::default());
                let at0 = s.value(&fg, None);
                s.set(&mut fg, None, at0 + 1.0);
                match s.space() {
                    Space::Skin => {
                        let off = egt.offset(&fg.fgts, &[]);
                        let n = off.len().max(1) as f32;
                        let mean = [0, 1, 2].map(|i| off.iter().map(|o| o[i]).sum::<f32>() / n);
                        v["body_colour_per_unit"] = json!(mean.map(|x| round(f64::from(x), 2)));
                    }
                    _ => {
                        let mut moved = positions.clone();
                        egm.apply(&mut moved, &fg.fggs, &fg.fgga);
                        let (i, d) = (0..positions.len())
                            .map(|i| (i, [0, 1, 2].map(|k| moved[i][k] - positions[i][k])))
                            .max_by(|a, b| a.1.iter().map(|x| x * x).sum::<f32>().total_cmp(&b.1.iter().map(|x| x * x).sum::<f32>()))
                            .unwrap_or((0, [0.0; 3]));
                        v["most_moved"] = json!({ "vertex": i, "at": positions.get(i).map(|p| p.map(|x| round(f64::from(x), 2))), "by": d.map(|x| round(f64::from(x), 3)) });
                    }
                }
            }
            v
        })
        .collect();
    let by_space = [Space::Shape, Space::Asymmetry, Space::Skin].map(|sp| c.sliders[0].iter().filter(|s| s.space() == sp).count());
    json!({
        "schema": "fnv.actor.creator/1",
        "ctl_problem": c.ctl_problem,
        "counts": {
            "races": cat.races.len(), "facial_hair": cat.facial_hair.len(), "brows": cat.brows.len(),
            "authored_faces": cat.faces.len(), "presets": cat.faces.iter().filter(|f| f.preset).count(),
            "sliders": c.sliders[0].len(),
            "sliders_by_space": by_space,
        },
        "races": races,
        "facial_hair": cat.facial_hair.iter().map(|h| &h.name).collect::<Vec<_>>(),
        "brows": cat.brows.iter().map(|h| &h.name).collect::<Vec<_>>(),
        "hair_colours": cat.hair_colours.iter().take(16).map(|(c, n)| json!({ "rgb": c, "npcs": n })).collect::<Vec<_>>(),
        "sliders": sliders,
    })
}
