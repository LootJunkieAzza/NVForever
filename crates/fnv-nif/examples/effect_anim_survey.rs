//! Survey of everything that animates a model or its material by itself, for
//! docs/spec-notes/effects-animation-audit.md: over every `.nif` and `.kf` in the given
//! archives, counts
//! - every controller block (`NiTimeController` subtypes and particle `Ctlr`s) by type,
//!   by what it drives (texture slot and operation of a NiTextureTransformController,
//!   colour of a NiMaterialColorController, the target's type for the rest), by how it
//!   runs ("seq": manager controlled, flag bit 5, played by a sequence; "free": active,
//!   flag bit 3, not manager controlled; "off": neither) and by whether its values change
//!   ("moves": the keys of its float or point3 interpolator differ, or it is a B-spline;
//!   "still": they do not; "-": not a float or point3 interpolator);
//! - every controlled block of every NiControllerSequence by controller type, controller
//!   ID and interpolator type, split between `.nif` (embedded) and `.kf` files;
//! - shader properties by type and `shader_type`, and the shader flags that may mean
//!   motion (refraction, fire refraction, dynamic alpha), with their
//!   BSShaderPPLightingProperty refraction fields;
//! - shapes whose emissive colour or multiplier a controller drives, by shader: the
//!   stored emissive × multiplier and the mean of the keys × multiplier;
//! - NiTexturingProperty base-texture transforms that are not the identity (a stored
//!   scale, offset or rotation).
//!
//! Each row lists files, blocks and up to three example paths; with `--list TEXT`, every
//! file of the rows whose key contains TEXT.
//!
//! cargo run --release -p fnv-nif --example effect_anim_survey -- <schema dir> [--list TEXT] <archive.bsa>...

use fnv_nif::anim::{sequences, transform_track, value_track, Channel, ValueTrack};
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Row {
    blocks: usize,
    files: BTreeSet<String>,
}

fn add(m: &mut BTreeMap<String, Row>, key: String, path: &str) {
    let r = m.entry(key).or_default();
    r.blocks += 1;
    r.files.insert(path.to_string());
}

fn print(title: &str, m: &BTreeMap<String, Row>, list: Option<&str>) {
    println!("\n## {title}\n");
    println!("{:>6} {:>7}  key  (examples)", "files", "blocks");
    for (k, r) in m {
        let ex: Vec<&str> = r.files.iter().take(3).map(String::as_str).collect();
        println!("{:>6} {:>7}  {k}  ({})", r.files.len(), r.blocks, ex.join(", "));
        if list.is_some_and(|l| k.contains(l)) {
            for f in &r.files {
                println!("              {f}");
            }
        }
    }
}

/// Whether a value track changes over time.
fn moves(v: &ValueTrack) -> bool {
    fn keys<const N: usize>(c: &Channel<N>) -> bool {
        match c {
            Channel::Static(_) => false,
            Channel::BSpline { .. } | Channel::Path(_) => true,
            Channel::Keys(k) => k.keys.iter().any(|x| x.value != k.keys[0].value || x.forward.iter().chain(&x.backward).any(|t| *t != 0.0)),
        }
    }
    match v {
        ValueTrack::Float(c) => keys(c),
        ValueTrack::Point3(c) => keys(c),
    }
}

fn main() {
    let mut a: Vec<String> = std::env::args().collect();
    let list = a.iter().position(|x| x == "--list").map(|i| {
        a.remove(i);
        a.remove(i)
    });
    let list = list.as_deref();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut controllers: BTreeMap<String, Row> = BTreeMap::new();
    let mut seq_tracks: BTreeMap<String, Row> = BTreeMap::new();
    let mut shaders: BTreeMap<String, Row> = BTreeMap::new();
    let mut transforms: BTreeMap<String, Row> = BTreeMap::new();
    let mut unknown: BTreeMap<String, Row> = BTreeMap::new();
    let mut glows: BTreeMap<String, Row> = BTreeMap::new();
    let mut bsx_of_free: BTreeMap<String, Row> = BTreeMap::new();
    let (mut files, mut failed) = (0usize, 0usize);
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif") || p.ends_with(".kf")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else {
                failed += 1;
                continue;
            };
            files += 1;
            let kf = path.ends_with(".kf");
            let dec = Decoded::new(&file, &data, &schema);
            // The file's BSXFlags value (an NiIntegerExtraData named BSX).
            let bsx = file.blocks.iter().enumerate().find(|(_, b)| b.type_name == "BSXFlags").and_then(|(i, _)| dec.blocks[i].as_ref().ok()).and_then(|s| s.int("integer_data"));
            let type_of = |r: i64| usize::try_from(r).ok().and_then(|i| file.blocks.get(i)).map_or("none", |b| b.type_name.as_str());
            for (i, b) in file.blocks.iter().enumerate() {
                let ty = b.type_name.as_str();
                let Ok(s) = &dec.blocks[i] else {
                    add(&mut unknown, ty.to_string(), &path);
                    continue;
                };
                // Shapes whose emissive colour a controller drives: the stored emissive ×
                // multiplier and the keys' mean × multiplier, by shader, to judge how an
                // unlit shape's glow is meant.
                if let Some(props) = s.refs("properties").filter(|_| !kf) {
                    let block = |r: &i32| usize::try_from(*r).ok().and_then(|i| dec.blocks.get(i)).and_then(|b| b.as_ref().ok());
                    let shader = props.iter().map(|r| type_of(*r as i64)).find(|t| t.starts_with("BSShader")).unwrap_or("no shader");
                    for r in props {
                        if type_of(*r as i64) != "NiMaterialProperty" {
                            continue;
                        }
                        let Some(m) = block(r) else { continue };
                        let mult = m.float("emissive_mult").unwrap_or(1.0);
                        let stored = m.floats("emissive_color").map_or(0.0, |c| c.iter().cloned().fold(0.0, f32::max)) * mult;
                        let mut c = m.int("controller").unwrap_or(-1);
                        let mut depth = 0;
                        while c >= 0 && depth < 16 {
                            depth += 1;
                            let Some(cb) = block(&(c as i32)) else { break };
                            let cty = type_of(c);
                            let glow = (cty == "NiMaterialColorController" && cb.int("target_color") == Some(3)) || cty == "BSMaterialEmittanceMultController";
                            if glow {
                                let interp = cb.block_ref("interpolator").unwrap_or(-1);
                                let mean = match value_track(&dec, interp, type_of(interp as i64)) {
                                    Some(ValueTrack::Point3(Channel::Keys(k))) if !k.keys.is_empty() => {
                                        Some(k.keys.iter().map(|x| x.value.iter().cloned().fold(0.0, f32::max)).sum::<f32>() / k.keys.len() as f32 * mult)
                                    }
                                    Some(ValueTrack::Float(Channel::Keys(k))) if !k.keys.is_empty() => {
                                        Some(k.keys.iter().map(|x| x.value[0]).sum::<f32>() / k.keys.len() as f32 * stored.max(1e-6) / mult.max(1e-6))
                                    }
                                    _ => None,
                                };
                                let bucket = |v: f32| if v > 3.0 { ">3".to_string() } else { format!("{:.1}", v) };
                                add(&mut glows, format!("{shader} | {cty} | stored {} | keys {}", bucket(stored), mean.map_or("seq".into(), bucket)), &path);
                            }
                            c = cb.int("next_controller").unwrap_or(-1);
                        }
                    }
                }
                if ty.ends_with("Controller") || ty.contains("Ctlr") {
                    if ty == "NiControllerManager" || ty == "NiMultiTargetTransformController" {
                        add(&mut controllers, ty.to_string(), &path);
                        continue;
                    }
                    let flags = s.int("flags").unwrap_or(0);
                    let run = if flags & 0x20 != 0 {
                        "seq"
                    } else if flags & 0x8 != 0 {
                        "free"
                    } else {
                        "off"
                    };
                    let target = type_of(s.int("target").unwrap_or(-1));
                    let what = match ty {
                        "NiTextureTransformController" => format!(
                            "slot {} op {} shader_map {}",
                            s.int("texture_slot").unwrap_or(-1),
                            s.int("operation").unwrap_or(-1),
                            s.int("shader_map").unwrap_or(0)
                        ),
                        "NiMaterialColorController" => format!("color {}", s.int("target_color").unwrap_or(-1)),
                        _ => format!("on {target}"),
                    };
                    let interp = s.block_ref("interpolator").unwrap_or(-1);
                    let ity = type_of(interp as i64);
                    let motion = match (value_track(&dec, interp, ity), transform_track(&dec, interp, ity)) {
                        (Some(v), _) if moves(&v) => "moves",
                        (Some(_), _) => "still",
                        (None, Some(t)) => {
                            // Sampled over the controller's span: any change is motion.
                            let (a, z) = (s.float("start_time").unwrap_or(0.0), s.float("stop_time").unwrap_or(0.0));
                            let at = |x: f32| (t.translation.sample(x), t.rotation_at(x), t.scale.sample(x));
                            let first = at(a);
                            let n = 16;
                            if (1..=n).any(|k| {
                                let x = at(a + (z - a) * k as f32 / n as f32);
                                let d = |p: Option<[f32; 3]>, q: Option<[f32; 3]>| match (p, q) {
                                    (Some(p), Some(q)) => p.iter().zip(q).any(|(u, v)| (u - v).abs() > 1e-3),
                                    _ => false,
                                };
                                let dr = match (x.1, first.1) {
                                    (Some(p), Some(q)) => p.iter().zip(q).map(|(u, v)| u * v).sum::<f32>().abs() < 0.99999,
                                    _ => false,
                                };
                                d(x.0, first.0) || dr || x.2.zip(first.2).is_some_and(|(p, q)| (p[0] - q[0]).abs() > 1e-3)
                            }) {
                                "moves"
                            } else {
                                "still"
                            }
                        }
                        _ => "-",
                    };
                    // A span of a frame or two is a setup, not an animation.
                    let span = s.float("stop_time").unwrap_or(0.0) - s.float("start_time").unwrap_or(0.0);
                    let motion = if run != "seq" && motion == "moves" && span <= 0.1 { "moves (frame)" } else { motion };
                    let src = if kf { "kf" } else { "nif" };
                    add(&mut controllers, format!("{ty} | {what} | {run} | {motion} | {src}"), &path);
                    if ty == "NiTransformController" && run == "free" {
                        add(&mut bsx_of_free, format!("{motion} | BSX {}", bsx.map_or("none".into(), |b| format!("{b:#x}"))), &path);
                    }
                }
                if ty.starts_with("BSShader") || ty.ends_with("ShaderProperty") {
                    let st = s.int("shader_type").unwrap_or(-1);
                    add(&mut shaders, format!("{ty} type {st}"), &path);
                    let f = s.int("shader_flags").unwrap_or(0);
                    for (bit, name) in [(15, "refraction"), (16, "fire_refraction"), (19, "dynamic_alpha"), (21, "window_environment_mapping")] {
                        if f & (1 << bit) != 0 {
                            let extra = match ty {
                                "BSShaderPPLightingProperty" => format!(
                                    " strength {:?} fire period {:?}",
                                    s.float("refraction_strength"),
                                    s.int("refraction_fire_period")
                                ),
                                _ => String::new(),
                            };
                            add(&mut shaders, format!("flag {name} on {ty}{extra}"), &path);
                        }
                    }
                    if s.int("controller").is_some_and(|c| c >= 0) {
                        add(&mut shaders, format!("{ty} with a controller ({})", type_of(s.int("controller").unwrap())), &path);
                    }
                }
                if ty == "NiTexturingProperty" {
                    if let Some(t) = s.strukt("base_texture").filter(|t| t.get("has_texture_transform").is_some_and(|v| matches!(v, fnv_nif::Value::Bool(true) | fnv_nif::Value::Int(1)))) {
                        let tr = t.floats("translation").unwrap_or(&[0.0, 0.0]).to_vec();
                        let sc = t.floats("scale").unwrap_or(&[1.0, 1.0]).to_vec();
                        let rot = t.float("rotation").unwrap_or(0.0);
                        let ident = tr.iter().all(|x| *x == 0.0) && sc.iter().all(|x| *x == 1.0) && rot == 0.0;
                        let animated = s.int("controller").is_some_and(|c| c >= 0);
                        add(
                            &mut transforms,
                            format!("{} method {} controller {}", if ident { "identity" } else { "not identity" }, t.int("transform_method").unwrap_or(-1), animated),
                            &path,
                        );
                    }
                }
            }
            for seq in sequences(&file, &dec) {
                for t in &seq.tracks {
                    let ctype = t.controller_type.as_deref().unwrap_or("?");
                    if ctype == "NiTransformController" || ctype == "NiKeyframeController" {
                        continue;
                    }
                    let kind = match &t.data {
                        fnv_nif::anim::TrackData::Transform(_) => "transform".to_string(),
                        fnv_nif::anim::TrackData::Other(k) => k.clone(),
                        fnv_nif::anim::TrackData::None => "none".to_string(),
                    };
                    let decoded = if t.values.is_some() || t.bools.is_some() { "read" } else { "not read" };
                    add(
                        &mut seq_tracks,
                        format!("{} | {ctype} | {} | {kind} ({decoded})", if kf { "kf" } else { "nif" }, t.controller_id.as_deref().unwrap_or("")),
                        &path,
                    );
                }
            }
        }
    }
    println!("# effect animation survey: {files} files read, {failed} not parsed");
    print("Controllers (type | drives | runs | values | file kind)", &controllers, list);
    print("Sequence tracks other than transforms (file kind | controller | ID | interpolator)", &seq_tracks, list);
    print("Shader properties and motion flags", &shaders, list);
    print("Base texture transforms", &transforms, list);
    print("Blocks not decoded (type)", &unknown, list);
    print("Files with free NiTransformControllers by BSXFlags (motion | BSX)", &bsx_of_free, list);
    print("Driven glows (shader | controller | stored emissive x mult | keys' mean x mult)", &glows, list);
}
