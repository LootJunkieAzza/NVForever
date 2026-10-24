//! Diagnostic: every drawn shape that blends, alpha-tests or uses an effect shader, one
//! JSON line each, for auditing how effect models (light beams, glows, smoke, dust, fire)
//! are drawn (docs/spec-notes/nif.md §6 "Effect audit"). Per shape: the model path and
//! shape name, the effective NiAlphaProperty (flags, threshold), the shader property
//! (type, shader type, flags, flags 2, view-angle falloff, texture), NiMaterialProperty
//! (emissive colour and multiplier, alpha), NiZBufferProperty and NiStencilProperty flags,
//! whether an NiVertexColorProperty is present, the controllers on the shape and its
//! properties, whether it sits under a billboard node, its vertex count, box, and the
//! range of its vertex alpha and vertex colour.
//!
//! cargo run --release -p fnv-nif --example fx_audit -- <schema dir> <archive.bsa>... > fx.jsonl
//!
//! Takes archive paths so a survey names what it reads (later archives override earlier
//! ones; a later archive's copy of a path replaces the earlier line set).

use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

fn controllers(dec: &Decoded, file: &NifFile, start: Option<i32>) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = start;
    while let Some(c) = cur.filter(|&c| c >= 0) {
        if out.len() > 16 {
            break;
        }
        out.push(file.blocks[c as usize].type_name.clone());
        cur = dec.get(c).and_then(|s| s.block_ref("next_controller"));
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut by_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            let objects = scene_objects(&file, &dec);
            let by_block: HashMap<usize, _> = objects.iter().map(|o| (o.block, o)).collect();
            // Particle systems are not in scene_objects: their parent is the node listing them.
            let mut drawn: Vec<(usize, Option<String>, String, Option<usize>)> = objects
                .iter()
                .filter(|o| !o.is_node)
                .map(|o| (o.block, o.name.clone(), o.type_name.clone(), o.parent))
                .collect();
            for (i, b) in file.blocks.iter().enumerate() {
                if !dec.is_a(i, "NiParticles") {
                    continue;
                }
                let parent = objects.iter().find(|o| o.children.contains(&i)).map(|o| o.block);
                let name = dec.blocks[i].as_ref().ok().and_then(|s| s.text("name")).map(str::to_string);
                drawn.push((i, name, b.type_name.clone(), parent));
            }
            let mut lines = Vec::new();
            for (block, name, kind, parent) in drawn {
                let particles = dec.is_a(block, "NiParticles");
                let mut chain = vec![block];
                let mut cur = parent;
                while let Some(p) = cur {
                    if chain.len() > 512 {
                        break;
                    }
                    chain.push(p);
                    cur = by_block.get(&p).and_then(|x| x.parent);
                }
                let hidden = chain.iter().any(|&b| {
                    let s = dec.blocks[b].as_ref().ok();
                    let name = s.and_then(|s| s.text("name")).unwrap_or("");
                    s.and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0
                        || (name.len() >= 12 && name[..12].eq_ignore_ascii_case("editormarker"))
                });
                if hidden {
                    continue;
                }
                let billboard = chain.iter().any(|&b| file.blocks[b].type_name == "NiBillboardNode");
                let mut alpha = Value::Null;
                let mut shader = Value::Null;
                let mut material = Value::Null;
                let mut zbuf = Value::Null;
                let mut stencil = Value::Null;
                let mut vcprop = Value::Null;
                let mut base_texture = Value::Null;
                let mut prop_ctrls: Vec<String> = Vec::new();
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        let ty = file.blocks[r].type_name.as_str();
                        if b == block {
                            prop_ctrls.extend(controllers(&dec, &file, s.block_ref("controller")));
                        }
                        match ty {
                            "NiAlphaProperty" => {
                                alpha = json!([s.int("flags").unwrap_or(0), s.int("threshold").unwrap_or(0)])
                            }
                            "NiMaterialProperty" => {
                                material = json!({"emissive": s.floats("emissive_color"), "mult": s.float("emissive_mult"),
                                    "alpha": s.float("alpha")})
                            }
                            "NiZBufferProperty" => zbuf = json!(s.int("flags")),
                            "NiStencilProperty" => {
                                stencil = json!([s.int("flags"), s.int("draw_mode")])
                            }
                            "NiVertexColorProperty" => {
                                vcprop = json!([s.int("flags"), s.int("vertex_mode"), s.int("lighting_mode")])
                            }
                            "NiTexturingProperty" => {
                                base_texture = json!(s
                                    .strukt("base_texture")
                                    .and_then(|b| b.block_ref("source"))
                                    .and_then(|r| dec.get(r))
                                    .and_then(|t| t.text("file_name")))
                            }
                            t if dec.is_a(r, "BSShaderProperty") => {
                                let tex = s.text("file_name").map(str::to_string).or_else(|| {
                                    s.block_ref("texture_set")
                                        .and_then(|t| dec.get(t))
                                        .and_then(|ts| ts.list("textures"))
                                        .and_then(|l| l.first().cloned())
                                        .and_then(|v| match v {
                                            fnv_nif::Value::Str { text, .. } => text,
                                            _ => None,
                                        })
                                });
                                let falloff = s.float("falloff_start_angle").map(|_| {
                                    ["falloff_start_angle", "falloff_stop_angle", "falloff_start_opacity", "falloff_stop_opacity"]
                                        .map(|k| s.float(k))
                                });
                                shader = json!({"type": t, "shader_type": s.int("shader_type"),
                                    "flags": s.int("shader_flags"), "flags2": s.int("shader_flags_2"),
                                    "clamp": s.int("texture_clamp_mode"), "falloff": falloff, "texture": tex});
                            }
                            _ => {}
                        }
                    }
                }
                let shape = dec.blocks[block].as_ref().ok();
                let ctrls = controllers(&dec, &file, shape.and_then(|s| s.block_ref("controller")));
                let modifiers: Vec<String> = shape
                    .and_then(|s| s.refs("modifiers"))
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|&m| usize::try_from(m).ok())
                    .map(|m| file.blocks[m].type_name.clone())
                    .collect();
                let data = shape.and_then(|s| s.block_ref("data")).and_then(|d| dec.get(d));
                let (mut nv, mut amin, mut amax, mut cmin, mut cmax) = (0usize, 1f32, 0f32, [1f32; 3], [0f32; 3]);
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                if let Some(d) = data {
                    let m = mesh_data(d);
                    nv = m.positions.len();
                    for p in &m.positions {
                        for i in 0..3 {
                            lo[i] = lo[i].min(p[i]);
                            hi[i] = hi[i].max(p[i]);
                        }
                    }
                    for c in &m.colors {
                        amin = amin.min(c[3]);
                        amax = amax.max(c[3]);
                        for i in 0..3 {
                            cmin[i] = cmin[i].min(c[i]);
                            cmax[i] = cmax[i].max(c[i]);
                        }
                    }
                    if m.colors.is_empty() {
                        (amin, amax) = (f32::NAN, f32::NAN);
                    }
                }
                let flags = alpha.get(0).and_then(Value::as_i64).unwrap_or(0);
                let effect_shader = shader.get("type").and_then(Value::as_str).is_some_and(|t| t != "BSShaderPPLightingProperty");
                if flags & 0x201 == 0 && !effect_shader && !particles {
                    continue;
                }
                lines.push(json!({
                    "path": path, "shape": name, "kind": kind, "alpha": alpha, "shader": shader,
                    "base_texture": base_texture, "material": material, "zbuffer": zbuf, "stencil": stencil,
                    "vertex_color_prop": vcprop, "ctrls": ctrls, "prop_ctrls": prop_ctrls, "billboard": billboard, "modifiers": modifiers,
                    "nv": nv, "box": if nv > 0 { json!([lo, hi]) } else { Value::Null },
                    "valpha": if amin.is_nan() { Value::Null } else { json!([amin, amax]) },
                    "vcolor": if amin.is_nan() { Value::Null } else { json!([cmin, cmax]) },
                }));
            }
            by_path.insert(path.to_ascii_lowercase(), lines);
        }
    }
    for lines in by_path.values() {
        for l in lines {
            println!("{l}");
        }
    }
}
