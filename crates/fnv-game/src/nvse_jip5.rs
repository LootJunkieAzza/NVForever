//! JIP LN NVSE, batch 5 (#1006): body-part data, land textures, texture sets' decal data
//! and flags, worldspaces' water [documented: each page in the wiki export;
//! docs/spec-notes/jip-records-5.md]. Setters change the record for the session, read by
//! the getters.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use serde_json::Value as Json;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn json_form(v: Option<&Json>) -> u32 {
    v.and_then(Json::as_str).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).unwrap_or(0)
}

fn json_num(v: Option<&Json>) -> f64 {
    match v {
        Some(Json::Object(o)) => o.get("value").and_then(Json::as_f64).unwrap_or(0.0),
        Some(v) => v.as_f64().or_else(|| v.as_bool().map(|b| f64::from(u8::from(b)))).unwrap_or(0.0),
        None => 0.0,
    }
}

/// A body part's numeric trait 0-18 [documented: GetBodyPartTraitNumeric's table]: the
/// BPND field.
const PART_NUMERIC: [&str; 19] = [
    "actor_value",
    "damage_mult",
    "health_percent",
    "to_hit_chance",
    "tracking_max_angle",
    "severable_debris_count",
    "severable_debris_scale",
    "severable_decal_count",
    "explodable_explosion_chance_percent",
    "limb_replacement_scale",
    "explodable_debris_count",
    "explodable_debris_scale",
    "explodable_decal_count",
    "gore_effects_positioning.position.x",
    "gore_effects_positioning.position.y",
    "gore_effects_positioning.position.z",
    "gore_effects_positioning.rotation_radians.x",
    "gore_effects_positioning.rotation_radians.y",
    "gore_effects_positioning.rotation_radians.z",
];

/// A body part's form trait 0-5 [documented: GetBodyPartTraitForm's table].
const PART_FORMS: [&str; 6] = ["severable_explosion", "explodable_explosion", "severable_debris", "explodable_debris", "severable_impact_dataset", "explodable_impact_dataset"];

/// A texture set's numeric trait 0-7, the decal data (DODT) [documented:
/// GetTextureSetTraitNumeric's table; 8, the colour, is worked out apart].
const DECAL_NUMERIC: [&str; 8] = ["min_width", "max_width", "min_height", "max_height", "depth", "shininess", "parallax_scale", "parallax_passes"];

fn path_get<'a>(v: &'a Json, path: &str) -> Option<&'a Json> {
    path.split('.').try_fold(v, |v, k| v.get(k))
}

impl Game<'_> {
    /// The BPND of a body-part data's part of type `part` (0 torso … 14 weapon), the
    /// record's field as decoded [documented: the part type IDs; the layout: xEdit BPTD].
    fn body_part(&self, bptd: FormId, part: i64) -> Option<&Json> {
        self.data.record_fields.get(&bptd)?.iter().filter(|(s, _)| s == b"BPND").map(|(_, v)| v).find(|v| v.get("part_type").and_then(Json::as_i64) == Some(part))
    }

    /// A BPND value with the session's change first.
    fn body_part_value(&self, bptd: FormId, part: i64, path: &str) -> Option<Json> {
        if let Some(v) = self.field_overrides.get(&(bptd, format!("BPND#{part}/{path}"))) {
            return Some(match v {
                XVal::Num(x) => serde_json::json!(x),
                XVal::Ref(r) => serde_json::json!(format!("0x{r:08X}")),
                XVal::Str(s) => serde_json::json!(s),
                _ => Json::Null,
            });
        }
        path_get(self.body_part(bptd, part)?, path).cloned()
    }

    /// A land texture's grasses (GNAM), LandTextureAddGrass's and RemoveGrass's changes
    /// first.
    fn land_grasses(&self, ltex: FormId) -> Vec<FormId> {
        if let Some(v) = self.other.ltex_grasses.get(&ltex) {
            return v.clone();
        }
        self.data.record_fields.get(&ltex).into_iter().flatten().filter(|(s, _)| s == b"GNAM").map(|(_, v)| FormId(json_form(Some(v)))).filter(|f| f.0 != 0).collect()
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let of_sig = |g: &Game, f: Option<FormId>, sig: &[u8; 4]| f.filter(|&f| g.data.sig(f) == Some(*sig));
    let field = |g: &Game, f: FormId, sub: &str, path: &str| crate::nvse::record_value(g, f, sub, path);
    let zero = XVal::Num(0.0);
    let v = match name {
        // `bpd part` [documented: GetBodyPartDataHasPart, part types 0-14].
        "getbodypartdatahaspart" => yes(of_sig(g, form(args, 0), b"BPTD").is_some_and(|b| g.body_part(b, num(args, 1, -1.0) as i64).is_some())),
        // `bpd [ragdoll]` (RAGA) and `bpd [path]` (the skeleton, MODL) [documented:
        // Get/SetBodyPartDataRagdoll; Get/SetBodyPartDataSkeleton].
        "getbodypartdataragdoll" => XVal::Ref(match of_sig(g, form(args, 0), b"BPTD").and_then(|b| field(g, b, "RAGA", "")) {
            Some(XVal::Ref(r)) => r,
            _ => 0,
        }),
        "setbodypartdataragdoll" => {
            if let Some(b) = of_sig(g, form(args, 0), b"BPTD") {
                g.field_overrides.insert((b, "RAGA/".into()), XVal::Ref(form(args, 1).map_or(0, |r| r.0)));
            }
            zero
        }
        "getbodypartdataskeleton" => XVal::Str(match of_sig(g, form(args, 0), b"BPTD").and_then(|b| field(g, b, "MODL", "")) {
            Some(XVal::Str(s)) => s,
            _ => String::new(),
        }),
        "setbodypartdataskeleton" => {
            if let Some(b) = of_sig(g, form(args, 0), b"BPTD") {
                let p = match args.get(1) {
                    Some(XVal::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                g.field_overrides.insert((b, "MODL/".into()), XVal::Str(p));
            }
            zero
        }
        // `bpd part trait [value]` [documented: Get/SetBodyPartTraitNumeric, traits 0-18;
        // Get/SetBodyPartTraitForm, traits 0-5].
        "getbodyparttraitnumeric" | "setbodyparttraitnumeric" | "getbodyparttraitform" | "setbodyparttraitform" => {
            let forms = name.ends_with("form");
            let Some(b) = of_sig(g, form(args, 0), b"BPTD") else { return Some(Ok(if forms { XVal::Ref(0) } else { zero })) };
            let part = num(args, 1, -1.0) as i64;
            let t = num(args, 2, -1.0);
            let path = usize::try_from(t as i64).ok().and_then(|i| if forms { PART_FORMS.get(i) } else { PART_NUMERIC.get(i) }).copied();
            let Some(path) = path.filter(|_| g.body_part(b, part).is_some()) else { return Some(Ok(if forms { XVal::Ref(0) } else { zero })) };
            if name.starts_with("set") {
                let v = if forms { XVal::Ref(form(args, 3).map_or(0, |f| f.0)) } else { XVal::Num(num(args, 3, 0.0)) };
                g.field_overrides.insert((b, format!("BPND#{part}/{path}")), v);
                zero
            } else if forms {
                XVal::Ref(json_form(g.body_part_value(b, part, path).as_ref()))
            } else {
                XVal::Num(json_num(g.body_part_value(b, part, path).as_ref()))
            }
        }
        // `bpd part flag [value]`: BPND's flag bits 0-6 [documented: Get/SetBodyPartFlag].
        "getbodypartflag" | "setbodypartflag" => {
            let Some(b) = of_sig(g, form(args, 0), b"BPTD") else { return Some(Ok(zero)) };
            let (part, bit) = (num(args, 1, -1.0) as i64, num(args, 2, -1.0) as i64);
            if !(0..=6).contains(&bit) || g.body_part(b, part).is_none() {
                return Some(Ok(zero));
            }
            let flags = json_num(g.body_part_value(b, part, "flags").as_ref()) as u64;
            if name == "setbodypartflag" {
                let new = if num(args, 3, 0.0) != 0.0 { flags | (1 << bit) } else { flags & !(1 << bit) };
                g.field_overrides.insert((b, format!("BPND#{part}/flags")), XVal::Num(new as f64));
                zero
            } else {
                yes(flags & (1 << bit) != 0)
            }
        }
        // `ltex [index]`: the grasses (GNAM), by index; their number [documented:
        // GetLandTextureNthGrass; GetLandTextureNumGrasses]; LandTextureAddGrass and
        // LandTextureRemoveGrass `ltex grass` [documented].
        "getlandtexturenumgrasses" => XVal::Num(of_sig(g, form(args, 0), b"LTEX").map_or(0, |l| g.land_grasses(l).len()) as f64),
        "getlandtexturenthgrass" => {
            let list = of_sig(g, form(args, 0), b"LTEX").map(|l| g.land_grasses(l)).unwrap_or_default();
            XVal::Ref(usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| list.get(i)).map_or(0, |f| f.0))
        }
        "landtextureaddgrass" | "landtextureremovegrass" => {
            let (Some(l), Some(gr)) = (of_sig(g, form(args, 0), b"LTEX"), form(args, 1)) else { return Some(Ok(zero)) };
            // 1 when the list changed, 0 for a grass already there or not there [observed:
            // jip5 ltex.addgrass.result, add twice 1 0, remove twice 1 0].
            let mut list = g.land_grasses(l);
            let had = list.contains(&gr);
            if name == "landtextureaddgrass" {
                if !had {
                    list.push(gr);
                }
            } else {
                list.retain(|&x| x != gr);
            }
            g.other.ltex_grasses.insert(l, list);
            yes(had == (name == "landtextureremovegrass"))
        }
        // `ltex [set]`: the texture set (TNAM) [documented: Get/SetLandTextureTextureSet].
        "getlandtexturetextureset" => XVal::Ref(match of_sig(g, form(args, 0), b"LTEX").and_then(|l| field(g, l, "TNAM", "")) {
            Some(XVal::Ref(r)) => r,
            _ => 0,
        }),
        "setlandtexturetextureset" => {
            if let Some(l) = of_sig(g, form(args, 0), b"LTEX") {
                g.field_overrides.insert((l, "TNAM/".into()), XVal::Ref(form(args, 1).map_or(0, |t| t.0)));
            }
            zero
        }
        // `ltex trait [value]`: 0 material type, 1 friction, 2 restitution (HNAM),
        // 3 specular exponent (SNAM) [documented: Get/SetLandTextureTraitNumeric].
        "getlandtexturetraitnumeric" | "setlandtexturetraitnumeric" => {
            let Some(l) = of_sig(g, form(args, 0), b"LTEX") else { return Some(Ok(zero)) };
            let (sub, path) = match num(args, 1, -1.0) as i64 {
                0 => ("HNAM", "material_type"),
                1 => ("HNAM", "friction"),
                2 => ("HNAM", "restitution"),
                3 => ("SNAM", ""),
                _ => return Some(Ok(zero)),
            };
            if name.starts_with("set") {
                g.field_overrides.insert((l, format!("{sub}/{path}")), XVal::Num(num(args, 2, 0.0).trunc()));
                zero
            } else {
                XVal::Num(field(g, l, sub, path).and_then(|v| v.num().ok()).unwrap_or(0.0))
            }
        }
        // `txst trait [value]`: the decal data (DODT) 0-7, and 8 its colour as
        // R × 1000000 + G × 1000 + B [documented: Get/SetTextureSetTraitNumeric, "Color
        // (RRRGGGBBB)"]; 0 without decal data [hypothesis].
        "gettexturesettraitnumeric" | "settexturesettraitnumeric" => {
            let Some(t) = of_sig(g, form(args, 0), b"TXST") else { return Some(Ok(zero)) };
            let id = num(args, 1, -1.0) as i64;
            let set = name.starts_with("set");
            match usize::try_from(id).ok() {
                Some(i) if i < DECAL_NUMERIC.len() => {
                    if set {
                        g.field_overrides.insert((t, format!("DODT/{}", DECAL_NUMERIC[i])), XVal::Num(num(args, 2, 0.0)));
                        zero
                    } else {
                        XVal::Num(field(g, t, "DODT", DECAL_NUMERIC[i]).and_then(|v| v.num().ok()).unwrap_or(0.0))
                    }
                }
                Some(8) => {
                    let parts = ["color.red", "color.green", "color.blue"];
                    if set {
                        let v = num(args, 2, 0.0).max(0.0) as u64;
                        for (k, p) in [(v / 1_000_000) % 1000, (v / 1000) % 1000, v % 1000].into_iter().zip(parts) {
                            g.field_overrides.insert((t, format!("DODT/{p}")), XVal::Num(k.min(255) as f64));
                        }
                        zero
                    } else {
                        let c: Vec<f64> = parts.iter().map(|p| field(g, t, "DODT", p).and_then(|v| v.num().ok()).unwrap_or(0.0)).collect();
                        XVal::Num(c[0] * 1_000_000.0 + c[1] * 1000.0 + c[2])
                    }
                }
                _ => zero,
            }
        }
        // `txst flag [value]`: 0 No Specular Map (DNAM bit 0); 1 Parallax, 2 Alpha
        // Blending, 3 Alpha Testing (DODT's flag bits 0-2) [documented: Get/
        // SetTextureSetFlag's table; the bits: the records' decoded flag names].
        "gettexturesetflag" | "settexturesetflag" => {
            let Some(t) = of_sig(g, form(args, 0), b"TXST") else { return Some(Ok(zero)) };
            let (sub, path, bit) = match num(args, 1, -1.0) as i64 {
                0 => ("DNAM", "", 0),
                1 => ("DODT", "flags", 0),
                2 => ("DODT", "flags", 1),
                3 => ("DODT", "flags", 2),
                _ => return Some(Ok(zero)),
            };
            let now = field(g, t, sub, path).and_then(|v| v.num().ok()).unwrap_or(0.0) as u64;
            if name.starts_with("set") {
                let new = if num(args, 2, 0.0) != 0.0 { now | (1 << bit) } else { now & !(1 << bit) };
                g.field_overrides.insert((t, format!("{sub}/{path}")), XVal::Num(new as f64));
                zero
            } else {
                yes(now & (1 << bit) != 0)
            }
        }
        // `world [value]`: the water noise texture (XNAM) and water type (NAM2)
        // [documented: Get/SetWorldspaceNoiseTexture; Get/SetWorldspaceWaterType].
        "getworldspacenoisetexture" => XVal::Str(match of_sig(g, form(args, 0), b"WRLD").and_then(|w| field(g, w, "XNAM", "")) {
            Some(XVal::Str(s)) => s,
            _ => String::new(),
        }),
        "setworldspacenoisetexture" => {
            if let Some(w) = of_sig(g, form(args, 0), b"WRLD") {
                let p = match args.get(1) {
                    Some(XVal::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                g.field_overrides.insert((w, "XNAM/".into()), XVal::Str(p));
            }
            zero
        }
        "getworldspacewatertype" => XVal::Ref(match of_sig(g, form(args, 0), b"WRLD").and_then(|w| field(g, w, "NAM2", "")) {
            Some(XVal::Ref(r)) => r,
            _ => 0,
        }),
        "setworldspacewatertype" => {
            if let Some(w) = of_sig(g, form(args, 0), b"WRLD") {
                g.field_overrides.insert((w, "NAM2/".into()), XVal::Ref(form(args, 1).map_or(0, |t| t.0)));
            }
            zero
        }
        _ => return None,
    };
    let _ = (ctx, subject);
    Some(Ok(v))
}
