//! JohnnyGuitar NVSE, batch 2 (#1007): cells' acoustic space and interior lighting,
//! references' and cells' encounter zones, message icons, image space modifiers'
//! animatable flag and weapons' VATS data [documented: each page in the wiki export;
//! docs/spec-notes/ext-jg2.md]. Setters change the record for the session, read by the
//! getters.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

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

/// An interior lighting trait 0-15 [documented: GetInteriorLightingTraitNumeric's table;
/// observed: jg2, PrimmNashResidence, every trait]:
/// the CELL XCLL field.
fn interior_trait(id: f64) -> Option<String> {
    let rgb = ["red", "green", "blue"];
    let id = id as i64;
    Some(match id {
        0..=2 => format!("ambient_color.{}", rgb[id as usize]),
        3..=5 => format!("directional_color.{}", rgb[(id - 3) as usize]),
        6 => "directional_rotation_xy".into(),
        7 => "directional_rotation_z".into(),
        8 => "directional_fade".into(),
        9..=11 => format!("fog_color.{}", rgb[(id - 9) as usize]),
        12 => "fog_near".into(),
        13 => "fog_far".into(),
        14 => "fog_power".into(),
        15 => "fog_clip_dist".into(),
        _ => return None,
    })
}

/// A weapon VATS trait 1-5 [documented: GetWeaponVATSTraitNumeric's table: skill, damage
/// mult, AP, silent, mod required]: the WEAP VATS field.
fn vats_trait(id: f64) -> Option<&'static str> {
    Some(match id as i64 {
        1 => "skill",
        2 => "dam_mult",
        3 => "ap",
        4 => "silent",
        5 => "mod_required",
        _ => return None,
    })
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let field = |g: &Game, f: FormId, sub: &str, path: &str| crate::nvse::record_value(g, f, sub, path);
    let of_sig = |g: &Game, f: Option<FormId>, sig: &[u8; 4]| f.filter(|&f| g.data.sig(f) == Some(*sig));
    let as_num = |v: Option<XVal>| match v {
        Some(XVal::Num(x)) => x,
        Some(XVal::Str(s)) if s.eq_ignore_ascii_case("true") => 1.0,
        _ => 0.0,
    };
    let zero = XVal::Num(0.0);
    let v = match name {
        // `cell`: its acoustic space (XCAS) [documented: GetAcousticSpace]; SetAcousticSpace
        // `cell [space]`, none clearing it [documented].
        "getacousticspace" => XVal::Ref(match of_sig(g, form(args, 0), b"CELL").and_then(|c| field(g, c, "XCAS", "")) {
            Some(XVal::Ref(r)) => r,
            _ => 0,
        }),
        "setacousticspace" => {
            if let Some(c) = of_sig(g, form(args, 0), b"CELL") {
                g.field_overrides.insert((c, "XCAS/".into()), XVal::Ref(form(args, 1).map_or(0, |s| s.0)));
            }
            zero
        }
        // `cell trait [value]`: the interior's lighting (XCLL) [documented: Get/
        // SetInteriorLightingTraitNumeric, traits 0-15].
        "getinteriorlightingtraitnumeric" | "setinteriorlightingtraitnumeric" => {
            let (Some(c), Some(path)) = (of_sig(g, form(args, 0), b"CELL"), interior_trait(num(args, 1, -1.0))) else { return Some(Ok(zero)) };
            if name.starts_with("set") {
                g.field_overrides.insert((c, format!("XCLL/{path}")), XVal::Num(num(args, 2, 0.0)));
                zero
            } else {
                XVal::Num(as_num(field(g, c, "XCLL", &path)))
            }
        }
        // The calling reference's encounter zone, else none [documented:
        // GetRefEncounterZone, alias GetRefZone]; SetRefEncounterZone `[zone]` and
        // SetCellEncounterZone `cell [zone]`, none removing it, 1 when done [documented].
        "getrefencounterzone" | "getrefzone" => {
            let z = subject.and_then(|r| g.other.ref_zones.get(&r).copied().unwrap_or_else(|| g.world.placed.get(r).and_then(|p| p.encounter_zone)));
            XVal::Ref(z.map_or(0, |z| z.0))
        }
        "setrefencounterzone" | "setrefzone" => {
            let Some(r) = subject.filter(|&r| g.world.placed.get(r).is_some()) else { return Some(Ok(zero)) };
            let z = form(args, 0).filter(|z| g.world.forms.zones.contains_key(z));
            if form(args, 0).is_some() && z.is_none() {
                return Some(Ok(zero));
            }
            g.other.ref_zones.insert(r, z);
            XVal::Num(1.0)
        }
        "setcellencounterzone" | "setcellzone" => {
            let Some(c) = of_sig(g, form(args, 0), b"CELL") else { return Some(Ok(zero)) };
            let z = form(args, 1).filter(|z| g.world.forms.zones.contains_key(z));
            if form(args, 1).is_some() && z.is_none() {
                return Some(Ok(zero));
            }
            g.other.cell_zones.insert(c, z);
            XVal::Num(1.0)
        }
        // `form [female]`: the message (small) icon, MICO, an armor's female one MIC2
        // [documented: GetMessageIconPath; the subrecords: xEdit ARMO]; SetMessageIconPath
        // `path form [female]` [documented].
        "getmessageiconpath" | "setmessageiconpath" => {
            let set = name.starts_with("set");
            let at = usize::from(set);
            let Some(f) = form(args, at) else { return Some(Ok(XVal::Str(String::new()))) };
            let sub = if g.data.sig(f) == Some(*b"ARMO") && num(args, at + 1, 0.0) != 0.0 { "MIC2" } else { "MICO" };
            if set {
                let p = match args.first() {
                    Some(XVal::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                g.field_overrides.insert((f, format!("{sub}/")), XVal::Str(p));
                zero
            } else {
                XVal::Str(match field(g, f, sub, "") {
                    Some(XVal::Str(s)) => s,
                    _ => String::new(),
                })
            }
        }
        // `modifier [value]`: the image space modifier's Animatable flag (IMAD DNAM)
        // [documented: Get/SetIMODAnimatable].
        "getimodanimatable" => yes(of_sig(g, form(args, 0), b"IMAD").is_some_and(|m| as_num(field(g, m, "DNAM", "animatable")) != 0.0)),
        "setimodanimatable" => {
            if let Some(m) = of_sig(g, form(args, 0), b"IMAD") {
                g.field_overrides.insert((m, "DNAM/animatable".into()), XVal::Num(f64::from(u8::from(num(args, 1, 0.0) != 0.0))));
            }
            zero
        }
        // `weapon trait [value]` [documented: Get/SetWeaponVATSTraitNumeric, traits 1-5].
        "getweaponvatstraitnumeric" | "setweaponvatstraitnumeric" => {
            let (Some(w), Some(path)) = (of_sig(g, form(args, 0), b"WEAP"), vats_trait(num(args, 1, 0.0))) else { return Some(Ok(zero)) };
            if name.starts_with("set") {
                g.field_overrides.insert((w, format!("VATS/{path}")), XVal::Num(num(args, 2, 0.0)));
                zero
            } else {
                XVal::Num(as_num(field(g, w, "VATS", path)))
            }
        }
        _ => return None,
    };
    let _ = ctx;
    Some(Ok(v))
}
