//! The effect content of shots (docs/spec-notes/combat-rules.md §16): what a weapon, a
//! projectile, an explosion and an impact sound and look like. Read with the record
//! layouts in `schemas/fnv` (WEAP and SOUN in `objects.json`; PROJ and EXPL in
//! `combat.json`; ADDN, IPCT and IPDS in `placed.json`), all [documented: xEdit]. This
//! module keeps the records' own names; what the engine does with them is its business.
//!
//! Kept apart from [`crate::data::CombatForms`]: the rules do not need any of it.

use std::collections::HashMap;

use fnv_esm::{LoadOrder, Record, Schema};
use fnv_sim::extract::{self, all, first, global, int, local_fid, num, text, Decoded};
use fnv_sim::FormId;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

/// SOUN.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sound {
    pub edid: Option<String>,
    /// `FNAM`: a file or a folder of files under `sound\`.
    pub file: Option<String>,
    /// `SNDD` flag names (`Loop`, `2D`, `Random Frequency Shift`, …).
    pub flags: Vec<String>,
    /// Game units: the stored bytes times 5 and times 100 [documented: xEdit].
    pub min_distance: f32,
    pub max_distance: f32,
    /// Decibels taken off (the stored value is in hundredths).
    pub static_attenuation_db: f32,
    pub frequency_adjust_percent: f32,
    /// `RNAM` "Random Chance %".
    pub random_chance_percent: f32,
    /// `SNDD` start and stop time in hours: the stored byte over 256 is the fraction of
    /// a day [documented: xEdit wbAlocTime].
    pub start_hour: f32,
    pub stop_hour: f32,
}

impl Sound {
    pub fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }
}

/// The sounds and the shell casing of a WEAP.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WeaponFx {
    /// `SNAM`, first and second: Shoot 3D and Shoot Dist.
    pub shoot_3d: Option<FormId>,
    pub shoot_dist: Option<FormId>,
    /// `XNAM`: Shoot 2D.
    pub shoot_2d: Option<FormId>,
    /// `NAM7`: Shoot 3D Looping.
    pub shoot_loop: Option<FormId>,
    /// `TNAM`: Melee Swing / Gun No Ammo.
    pub no_ammo: Option<FormId>,
    /// `NAM9`, `NAM8`.
    pub equip: Option<FormId>,
    pub unequip: Option<FormId>,
    /// `MOD2`: the shell casing model.
    pub shell_casing_model: Option<String>,
    /// `INAM`.
    pub impact_dataset: Option<FormId>,
}

/// The sounds of a PROJ and of an EXPL.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectileFx {
    pub sound: Option<FormId>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExplosionFx {
    pub model: Option<String>,
    pub sound_1: Option<FormId>,
    pub sound_2: Option<FormId>,
    pub impact_dataset: Option<FormId>,
}

/// ADDN: the model an `AddOnNode<index>` of a NIF stands for.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AddonNode {
    pub edid: Option<String>,
    pub index: i32,
    pub model: Option<String>,
    pub sound: Option<FormId>,
    pub particle_cap: u32,
}

/// IPCT.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Impact {
    pub edid: Option<String>,
    pub model: Option<String>,
    pub effect_duration: f32,
    /// Surface Normal, Projectile Vector or Projectile Reflection.
    pub orientation: Option<String>,
    pub texture_set: Option<FormId>,
    pub sound_1: Option<FormId>,
    pub sound_2: Option<FormId>,
}

/// The materials of an IPDS, in the record's order.
pub const IMPACT_MATERIALS: [&str; 12] =
    ["stone", "dirt", "grass", "glass", "metal", "wood", "organic", "cloth", "water", "hollow_metal", "organic_bug", "organic_glow"];

/// The effect forms of a load order.
#[derive(Debug, Default)]
pub struct FxForms {
    pub sounds: HashMap<FormId, Sound>,
    pub weapons: HashMap<FormId, WeaponFx>,
    pub projectiles: HashMap<FormId, ProjectileFx>,
    pub explosions: HashMap<FormId, ExplosionFx>,
    /// By node index (`DATA`).
    pub addons: HashMap<i32, AddonNode>,
    pub impacts: HashMap<FormId, Impact>,
    /// IPDS: the IPCT of each of [`IMPACT_MATERIALS`].
    pub impact_sets: HashMap<FormId, [Option<FormId>; 12]>,
    /// NPC_ and CREA `NAM4` Impact Material Type, an index into [`IMPACT_MATERIALS`]
    /// (the enumeration lists the same twelve in the same order) [documented: xEdit].
    pub actor_impact: HashMap<FormId, usize>,
}

const SIGS: &[&[u8; 4]] = &[b"SOUN", b"WEAP", b"PROJ", b"EXPL", b"ADDN", b"IPCT", b"IPDS"];

enum Loaded {
    Sound(Sound),
    Weapon(WeaponFx),
    Projectile(ProjectileFx),
    Explosion(ExplosionFx),
    Addon(AddonNode),
    Impact(Impact),
    ImpactSet([Option<FormId>; 12]),
}

fn path(v: Option<&Value>) -> Option<String> {
    v.and_then(text).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// A subrecord's bytes as a string (for a subrecord the record's layout does not list).
fn raw_string(r: &Record, sig: &[u8; 4]) -> Option<String> {
    let subs = r.subrecords().ok()?;
    let s = subs.iter().find(|s| &s.sig == sig)?;
    let end = s.data.iter().position(|&b| b == 0).unwrap_or(s.data.len());
    let t = String::from_utf8_lossy(&s.data[..end]).trim().to_string();
    (!t.is_empty()).then_some(t)
}

impl FxForms {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> FxForms {
        let mut wanted: Vec<(FormId, usize, &Record)> = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if SIGS.contains(&&r.header.sig) && r.header.flags & 0x20 == 0 {
                wanted.push((FormId(g), d.plugin as usize, r));
            }
        }
        wanted.sort_by_key(|w| w.0);
        let loaded: Vec<(FormId, Loaded)> = wanted
            .par_iter()
            .filter_map(|&(id, pi, r)| {
                let d = extract::decode(r, schema).ok()?;
                // A FormID of this record as a global one; null and unresolved: `None`.
                let fid = |v: Option<&Value>| v.and_then(local_fid).and_then(|l| global(lo, pi, l).ok().flatten());
                let field = |v: Option<&Value>, key: &str| fid(v.and_then(|v| v.get(key)));
                let edid = first(&d, b"EDID").and_then(text);
                Some((id, load_one(r, &d, edid, &fid, &field)?))
            })
            .collect();
        let mut out = FxForms { actor_impact: actor_impact_materials(lo), ..FxForms::default() };
        for (id, l) in loaded {
            match l {
                Loaded::Sound(s) => {
                    out.sounds.insert(id, s);
                }
                Loaded::Weapon(w) => {
                    out.weapons.insert(id, w);
                }
                Loaded::Projectile(p) => {
                    out.projectiles.insert(id, p);
                }
                Loaded::Explosion(e) => {
                    out.explosions.insert(id, e);
                }
                Loaded::Addon(a) => {
                    out.addons.insert(a.index, a);
                }
                Loaded::Impact(i) => {
                    out.impacts.insert(id, i);
                }
                Loaded::ImpactSet(s) => {
                    out.impact_sets.insert(id, s);
                }
            }
        }
        out
    }

    /// The IPCT of `set` for a material of [`IMPACT_MATERIALS`].
    pub fn impact(&self, set: FormId, material: &str) -> Option<&Impact> {
        self.impacts.get(&self.impact_id(set, material)?)
    }

    /// The impact material of an actor whose base is `base` (NPC_ or CREA `NAM4`).
    pub fn actor_material(&self, base: FormId) -> Option<&'static str> {
        self.actor_impact.get(&base).and_then(|&k| IMPACT_MATERIALS.get(k)).copied()
    }

    pub fn impact_id(&self, set: FormId, material: &str) -> Option<FormId> {
        let k = IMPACT_MATERIALS.iter().position(|m| *m == material)?;
        self.impact_sets.get(&set)?[k]
    }
}

/// `NAM4` of every NPC_ and CREA, read from the raw subrecord (a `u32` enumeration,
/// [documented: xEdit]); values outside the twelve are left out.
fn actor_impact_materials(lo: &LoadOrder) -> HashMap<FormId, usize> {
    let actors: Vec<(FormId, &Record)> = lo
        .index
        .iter()
        .filter_map(|(&g, defs)| {
            let d = defs.last()?;
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            (matches!(&r.header.sig, b"NPC_" | b"CREA") && r.header.flags & 0x20 == 0).then_some((FormId(g), r))
        })
        .collect();
    actors
        .par_iter()
        .filter_map(|&(id, r)| {
            let subs = r.subrecords().ok()?;
            let s = subs.iter().find(|s| &s.sig == b"NAM4")?;
            let v = u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?) as usize;
            (v < IMPACT_MATERIALS.len()).then_some((id, v))
        })
        .collect()
}

fn load_one(r: &Record, d: &[Decoded], edid: Option<String>, fid: &dyn Fn(Option<&Value>) -> Option<FormId>, field: &dyn Fn(Option<&Value>, &str) -> Option<FormId>) -> Option<Loaded> {
    Some(match &r.header.sig {
        b"SOUN" => {
            // `SNDD`, or the older `SNDX` with the same first fields.
            let v = first(d, b"SNDD").or_else(|| first(d, b"SNDX"));
            let n = |key: &str| v.and_then(|v| num(v, key)).unwrap_or(0.0) as f32;
            let flags = v.and_then(|v| v.get("flags"));
            Loaded::Sound(Sound {
                edid,
                file: path(first(d, b"FNAM")),
                flags: flags.and_then(|f| f.get("set")).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                min_distance: n("min_attenuation_distance") * 5.0,
                max_distance: n("max_attenuation_distance") * 100.0,
                static_attenuation_db: n("static_attenuation_cdb") / 100.0,
                frequency_adjust_percent: n("frequency_adjustment_percent"),
                random_chance_percent: first(d, b"RNAM").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                start_hour: n("start_time") / 256.0 * 24.0,
                stop_hour: n("stop_time") / 256.0 * 24.0,
            })
        }
        b"WEAP" => {
            let shoot: Vec<&Value> = all(d, b"SNAM").collect();
            Loaded::Weapon(WeaponFx {
                shoot_3d: fid(shoot.first().copied()),
                shoot_dist: fid(shoot.get(1).copied()),
                shoot_2d: fid(first(d, b"XNAM")),
                shoot_loop: fid(first(d, b"NAM7")),
                no_ammo: fid(first(d, b"TNAM")),
                equip: fid(first(d, b"NAM9")),
                unequip: fid(first(d, b"NAM8")),
                shell_casing_model: path(first(d, b"MOD2")).or_else(|| raw_string(r, b"MOD2")),
                impact_dataset: fid(first(d, b"INAM")),
            })
        }
        b"PROJ" => Loaded::Projectile(ProjectileFx { sound: field(first(d, b"DATA"), "sound") }),
        b"EXPL" => {
            let v = first(d, b"DATA");
            Loaded::Explosion(ExplosionFx {
                model: path(first(d, b"MODL")),
                sound_1: field(v, "sound_1"),
                sound_2: field(v, "sound_2"),
                impact_dataset: field(v, "impact_dataset"),
            })
        }
        b"ADDN" => Loaded::Addon(AddonNode {
            edid,
            index: first(d, b"DATA").and_then(Value::as_i64).unwrap_or(-1) as i32,
            model: path(first(d, b"MODL")),
            sound: fid(first(d, b"SNAM")),
            particle_cap: first(d, b"DNAM").and_then(|v| int(v, "master_particle_system_cap")).unwrap_or(0).max(0) as u32,
        }),
        b"IPCT" => {
            let v = first(d, b"DATA");
            Loaded::Impact(Impact {
                edid,
                model: path(first(d, b"MODL")),
                effect_duration: v.and_then(|v| num(v, "effect_duration")).unwrap_or(0.0) as f32,
                orientation: v.and_then(|v| v.get("effect_orientation")).and_then(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                }),
                texture_set: fid(first(d, b"DNAM")),
                sound_1: fid(first(d, b"SNAM")),
                sound_2: fid(first(d, b"NAM1")),
            })
        }
        b"IPDS" => {
            let v = first(d, b"DATA");
            Loaded::ImpactSet(std::array::from_fn(|k| field(v, IMPACT_MATERIALS[k])))
        }
        _ => return None,
    })
}

/// The material of [`IMPACT_MATERIALS`] a collision material stands for.
///
/// The collision codes are nif.xml's `Fallout3HavokMaterial` [documented]: 32 materials,
/// then the same again as platforms, stairs and stair platforms (code mod 32). Which of
/// the twelve impact materials each one takes is ours [hypothesis, from the names]: the
/// record has no table for it. No code (terrain) counts as dirt [design].
pub fn impact_material(code: Option<u32>) -> &'static str {
    let Some(code) = code else { return "dirt" };
    match code % 32 {
        0 | 10 | 19 => "stone",
        1 => "cloth",
        2 | 18 => "dirt",
        3 | 24 => "glass",
        4 => "grass",
        6 | 7 => "organic",
        8 => "water",
        9 | 12 => "wood",
        // Hollow, sheet, vehicle bodies and hollow parts, barrels, cans, carts, boxes.
        16 | 17 | 20 | 22 | 23 | 25 | 28 | 29 => "hollow_metal",
        // Metal, heavy metal, chain, caps, elevators, solid parts, guns, and the rest.
        _ => "metal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_materials_to_impact_materials() {
        assert_eq!(impact_material(None), "dirt");
        assert_eq!(impact_material(Some(0)), "stone");
        assert_eq!(impact_material(Some(9)), "wood");
        assert_eq!(impact_material(Some(5)), "metal");
        assert_eq!(impact_material(Some(16)), "hollow_metal");
        // Wood stairs (73) and a glass platform (35) are wood and glass.
        assert_eq!(impact_material(Some(73)), "wood");
        assert_eq!(impact_material(Some(35)), "glass");
        for code in 0..128 {
            assert!(IMPACT_MATERIALS.contains(&impact_material(Some(code))));
        }
    }
}
