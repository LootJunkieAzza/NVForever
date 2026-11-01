//! JIP LN NVSE, batch 7 (#1006): ammunition, ammo effects, projectiles, explosions, impact
//! data, weathers and climates, armour [documented: each page in the wiki export;
//! docs/spec-notes/jip-records-7.md]. Setters change the form for the session, read by the
//! getters (this file's and the earlier batches').

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::HashMap;

/// One climate weather (WLST): the weather, its chance, the global that gates it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateWeather {
    pub weather: FormId,
    pub chance: i64,
    pub global: Option<FormId>,
}

/// What this batch keeps for the session (nothing is saved [hypothesis], except where a
/// page says so: SetDamageToArmorMaxPercent "is saved in the game" [documented], kept in
/// the session only for now).
#[derive(Debug, Default)]
pub struct Jip7State {
    /// AddAmmoEffect's and RemoveAmmoEffect's lists, by ammunition (`Game::ammo_effects`).
    pub ammo_effects: HashMap<FormId, Vec<FormId>>,
    /// ClimateAddWeatherType's and ClimateRemoveWeatherType's lists, by climate
    /// (`Game::climate_weathers`).
    pub climate_weathers: HashMap<FormId, Vec<ClimateWeather>>,
    /// SetWeatherTransitionTimeOverride's seconds (`None`: the weathers' own).
    pub weather_transition: Option<f64>,
    /// SetArmorConditionPenalty's multiplier (`None`: the default 1.0).
    pub armor_condition_penalty: Option<f64>,
    /// SetDamageToArmorMaxPercent's share (`None`: not set).
    pub damage_to_armor_max: Option<f64>,
}

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

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k) {
        Some(XVal::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn json_form(v: Option<&Json>) -> Option<FormId> {
    v.and_then(Json::as_str).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).filter(|&f| f != 0).map(FormId)
}

/// AMMO numeric traits 0-3 [documented: SetAmmoTraitNumeric's table: 0 speed, 1
/// projectiles per shot, 2 ammo consumed percent, 3 clip rounds]; the fields: xEdit AMMO.
const AMMO_NUMERIC: [(&str, &str); 4] = [("DATA", "speed"), ("DAT2", "projectiles_per_shot"), ("DAT2", "consumed_percentage"), ("DATA", "clip_rounds")];

/// AMEF numeric traits 0-2 [documented: SetAmmoEffectTraitNumeric's table] as
/// GetAmmoEffectTraitNumeric reads them (`nvse_gaps`).
const AMEF_NUMERIC: [&str; 3] = ["type", "operation", "value"];

/// PROJ numeric traits 0-14 [documented: SetProjectileTraitNumeric's table]; 14 the
/// detection sound level is VNAM [the field: xEdit PROJ].
const PROJ_NUMERIC: [(&str, &str); 15] = [
    ("DATA", "type"),
    ("DATA", "gravity"),
    ("DATA", "speed"),
    ("DATA", "range"),
    ("DATA", "tracer_chance"),
    ("DATA", "explosion_alt_trigger_proximity"),
    ("DATA", "explosion_alt_trigger_timer"),
    ("DATA", "muzzle_flash_duration"),
    ("DATA", "fade_duration"),
    ("DATA", "impact_force"),
    ("DATA", "rotation.x"),
    ("DATA", "rotation.y"),
    ("DATA", "rotation.z"),
    ("DATA", "bouncy_mult"),
    ("VNAM", ""),
];

/// EXPL numeric traits 0-7 [documented: SetExplosionTraitNumeric's table].
const EXPL_NUMERIC: [&str; 8] = ["force", "damage", "radius", "is_radius", "radiation_level", "radiation_dissipation_time", "radiation_radius", "sound_level"];

/// IPCT numeric traits of SetImpactDataTraitNumeric, 0-14 [documented: its table]: the
/// impact data (DATA) 0-4, the decal data (DODT) 5-12; 13 the flags and 14 the colour are
/// worked out apart. GetImpactDataTrait numbers the same with the three forms (texture
/// set, sounds 1 and 2) at 5-7, so its 8-17 are these 5-14 [documented: its table].
const IPCT_NUMERIC: [(&str, &str); 13] = [
    ("DATA", "effect_duration"),
    ("DATA", "effect_orientation"),
    ("DATA", "angle_threshold"),
    ("DATA", "placement_radius"),
    ("DATA", "sound_level"),
    ("DODT", "min_width"),
    ("DODT", "max_width"),
    ("DODT", "min_height"),
    ("DODT", "max_height"),
    ("DODT", "depth"),
    ("DODT", "shininess"),
    ("DODT", "parallax_scale"),
    ("DODT", "parallax_passes"),
];

/// IPCT form traits 0-2 [documented: SetImpactDataTraitForm's table].
const IPCT_FORMS: [&str; 3] = ["DNAM", "SNAM", "NAM1"];

/// WTHR cloud texture layers 0-3 [documented: Get/SetWeatherTexture, layers 0-3; the
/// fields: xEdit WTHR DNAM, CNAM, ANAM, BNAM].
const WEATHER_LAYERS: [&str; 4] = ["DNAM", "CNAM", "ANAM", "BNAM"];

/// ARMO BMDT general flags: bit 3 Medium, bit 7 Heavy [the bits: xEdit ARMO]; neither
/// is Light.
const MEDIUM: u64 = 1 << 3;
const HEAVY: u64 = 1 << 7;

impl Game<'_> {
    /// An ammunition's effects (RCIL), AddAmmoEffect's and RemoveAmmoEffect's changes
    /// first.
    pub fn ammo_effects(&self, ammo: FormId) -> Vec<FormId> {
        if let Some(v) = self.other.jip7.ammo_effects.get(&ammo) {
            return v.clone();
        }
        self.data.record_all(ammo, "RCIL").into_iter().filter_map(|v| json_form(Some(v))).collect()
    }

    /// A climate's weathers (WLST), ClimateAddWeatherType's and ClimateRemoveWeatherType's
    /// changes first.
    pub fn climate_weathers(&self, climate: FormId) -> Vec<ClimateWeather> {
        if let Some(v) = self.other.jip7.climate_weathers.get(&climate) {
            return v.clone();
        }
        let list = self.data.record_field(climate, "WLST", "").and_then(Json::as_array);
        list.into_iter()
            .flatten()
            .filter_map(|e| {
                let weather = json_form(e.get("weather"))?;
                Some(ClimateWeather { weather, chance: e.get("chance").and_then(Json::as_i64).unwrap_or(0), global: json_form(e.get("global")) })
            })
            .collect()
    }

    /// An armour's class: 1 light, 2 medium, 3 heavy (BMDT's general flags), SetArmorClass'
    /// change first [documented: SetArmorClass's table].
    pub fn armor_class(&self, armor: FormId) -> u8 {
        let flags = self.num_field(armor, "BMDT", "general_flags") as u64;
        if flags & HEAVY != 0 {
            3
        } else if flags & MEDIUM != 0 {
            2
        } else {
            1
        }
    }

    /// SetArmorConditionPenalty's multiplier, 1.0 until a script sets it [documented:
    /// "1.0, by default"].
    pub fn armor_condition_penalty(&self) -> f64 {
        self.other.jip7.armor_condition_penalty.unwrap_or(1.0)
    }

    /// SetWeatherTransitionTimeOverride's seconds, `None` without one [documented: "no
    /// arguments (or with 0) removes the override"].
    pub fn weather_transition_override(&self) -> Option<f64> {
        self.other.jip7.weather_transition
    }

    /// A record's number (subrecord `sub`, then `path`), with the session's changes; 0
    /// when it has none.
    pub fn num_field(&self, f: FormId, sub: &str, path: &str) -> f64 {
        crate::nvse::record_value(self, f, sub, path).and_then(|v| v.num().ok()).unwrap_or(0.0)
    }

    fn set_field(&mut self, f: FormId, sub: &str, path: &str, v: XVal) {
        self.field_overrides.insert((f, format!("{sub}/{path}")), v);
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let of_sig = |g: &Game, f: Option<FormId>, sig: &[u8; 4]| f.filter(|&f| g.data.sig(f) == Some(*sig));
    let zero = XVal::Num(0.0);
    // The setters of a trait return 0 [hypothesis: as the record setters of batch 5 were
    // observed to, jip-records-5.md].
    let v = match name {
        // ---- ammunition
        "setammotraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| AMMO_NUMERIC.get(i));
            if let (Some(a), Some((sub, path))) = (of_sig(g, form(args, 0), b"AMMO"), t) {
                let v = num(args, 2, 0.0);
                // Projectiles per shot and clip rounds are whole numbers.
                let v = if matches!(*path, "projectiles_per_shot" | "clip_rounds") { v.max(0.0).trunc() } else { v };
                g.set_field(a, sub, path, XVal::Num(v));
            }
            zero
        }
        // `ammo [form]`: left out, none [documented: SetAmmoProjectile, SetAmmoCasing].
        "setammoprojectile" | "setammocasing" => {
            if let Some(a) = of_sig(g, form(args, 0), b"AMMO") {
                let path = if name == "setammoprojectile" { "projectile" } else { "consumed_ammo" };
                g.set_field(a, "DAT2", path, XVal::Ref(form(args, 1).map_or(0, |f| f.0)));
            }
            zero
        }
        // `ammo effect`: 1 when the list changed [documented: `successful`; hypothesis: 0
        // for an effect already there, or not there to remove].
        "addammoeffect" | "removeammoeffect" => {
            let (Some(a), Some(e)) = (of_sig(g, form(args, 0), b"AMMO"), of_sig(g, form(args, 1), b"AMEF")) else { return Some(Ok(zero)) };
            let mut list = g.ammo_effects(a);
            let had = list.contains(&e);
            let changed = if name == "addammoeffect" { !had } else { had };
            if name == "addammoeffect" {
                if !had {
                    list.push(e);
                }
            } else {
                list.retain(|&x| x != e);
            }
            g.other.jip7.ammo_effects.insert(a, list);
            yes(changed)
        }
        "setammoeffecttraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| AMEF_NUMERIC.get(i));
            if let (Some(e), Some(path)) = (of_sig(g, form(args, 0), b"AMEF"), t) {
                let v = num(args, 2, 0.0);
                g.set_field(e, "DATA", path, XVal::Num(if *path == "value" { v } else { v.max(0.0).trunc() }));
            }
            zero
        }
        // ---- projectiles and explosions
        "setprojectiletraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| PROJ_NUMERIC.get(i));
            if let (Some(p), Some((sub, path))) = (of_sig(g, form(args, 0), b"PROJ"), t) {
                g.set_field(p, sub, path, XVal::Num(num(args, 2, 0.0)));
            }
            zero
        }
        // `projectile [light]`: left out, none [documented: SetProjectileLight].
        "setprojectilelight" => {
            if let Some(p) = of_sig(g, form(args, 0), b"PROJ") {
                g.set_field(p, "DATA", "light", XVal::Ref(form(args, 1).map_or(0, |f| f.0)));
            }
            zero
        }
        // `projectile path`: the muzzle flash model (NAM1) [documented:
        // SetProjectileMuzzleFlash; the field: xEdit PROJ].
        "setprojectilemuzzleflash" => {
            if let (Some(p), Some(path)) = (of_sig(g, form(args, 0), b"PROJ"), text(args, 1)) {
                g.set_field(p, "NAM1", "", XVal::Str(path));
            }
            zero
        }
        "setexplosiontraitnumeric" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| EXPL_NUMERIC.get(i));
            if let (Some(e), Some(path)) = (of_sig(g, form(args, 0), b"EXPL"), t) {
                g.set_field(e, "DATA", path, XVal::Num(num(args, 2, 0.0)));
            }
            zero
        }
        // ---- impact data
        // `ipct trait`: 0-4 the impact data, 5-7 the texture set and sounds, 8-15 the
        // decal data, 16 the flags (1 no decal data, 2 parallax, 4 alpha blending, 8 alpha
        // testing), 17 the colour as R × 1000000 + G × 1000 + B [documented:
        // GetImpactDataTrait's tables]; past them 0.
        "getimpactdatatrait" => {
            let Some(i) = of_sig(g, form(args, 0), b"IPCT") else { return Some(Ok(zero)) };
            match num(args, 1, -1.0) as i64 {
                t @ 0..=4 => XVal::Num(g.num_field(i, IPCT_NUMERIC[t as usize].0, IPCT_NUMERIC[t as usize].1)),
                t @ 5..=7 => XVal::Ref(match crate::nvse::record_value(g, i, IPCT_FORMS[(t - 5) as usize], "") {
                    Some(XVal::Ref(r)) => r,
                    _ => 0,
                }),
                t @ 8..=15 => {
                    let (sub, path) = IPCT_NUMERIC[(t - 3) as usize];
                    XVal::Num(g.num_field(i, sub, path))
                }
                16 => XVal::Num(impact_flags(g, i) as f64),
                17 => XVal::Num(impact_colour(g, i)),
                _ => zero,
            }
        }
        "setimpactdatatraitnumeric" => {
            let Some(i) = of_sig(g, form(args, 0), b"IPCT") else { return Some(Ok(zero)) };
            let v = num(args, 2, 0.0);
            match num(args, 1, -1.0) as i64 {
                t @ 0..=12 => {
                    let (sub, path) = IPCT_NUMERIC[t as usize];
                    let v = if matches!(path, "effect_orientation" | "sound_level" | "parallax_passes") { v.max(0.0).trunc() } else { v };
                    g.set_field(i, sub, path, XVal::Num(v));
                }
                13 => {
                    let f = v.max(0.0) as u64;
                    let data = g.num_field(i, "DATA", "flags") as u64;
                    g.set_field(i, "DATA", "flags", XVal::Num(((data & !1) | (f & 1)) as f64));
                    let decal = g.num_field(i, "DODT", "flags") as u64;
                    g.set_field(i, "DODT", "flags", XVal::Num(((decal & !7) | ((f >> 1) & 7)) as f64));
                }
                14 => {
                    let c = v.max(0.0) as u64;
                    for (k, p) in [(c / 1_000_000) % 1000, (c / 1000) % 1000, c % 1000].into_iter().zip(["color.red", "color.green", "color.blue"]) {
                        g.set_field(i, "DODT", p, XVal::Num(k.min(255) as f64));
                    }
                }
                _ => {}
            }
            zero
        }
        "setimpactdatatraitform" => {
            let t = usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| IPCT_FORMS.get(i));
            if let (Some(i), Some(sub)) = (of_sig(g, form(args, 0), b"IPCT"), t) {
                g.set_field(i, sub, "", XVal::Ref(form(args, 2).map_or(0, |f| f.0)));
            }
            zero
        }
        // `weapon dataset`: WEAP INAM [documented: SetImpactDataSet; the field: xEdit WEAP].
        "setimpactdataset" => {
            if let Some(w) = of_sig(g, form(args, 0), b"WEAP") {
                g.set_field(w, "INAM", "", XVal::Ref(of_sig(g, form(args, 1), b"IPDS").map_or(0, |f| f.0)));
            }
            zero
        }
        // `[baseActor]`, else the calling reference's base: NAM4 0-11 [documented:
        // Get/SetImpactMaterialType's table; the field: xEdit NPC_/CREA]; not an actor 0.
        "getimpactmaterialtype" => {
            let a = form(args, 0).or(subject).map(|f| base(g, f)).filter(|&a| matches!(g.data.sig(a).as_ref(), Some(b"NPC_" | b"CREA")));
            XVal::Num(a.map_or(0.0, |a| g.num_field(a, "NAM4", "")))
        }
        "setimpactmaterialtype" => {
            let a = form(args, 1).or(subject).map(|f| base(g, f)).filter(|&a| matches!(g.data.sig(a).as_ref(), Some(b"NPC_" | b"CREA")));
            let m = num(args, 0, -1.0);
            if let Some(a) = a.filter(|_| (0.0..=11.0).contains(&m)) {
                g.set_field(a, "NAM4", "", XVal::Num(m.trunc()));
            }
            zero
        }
        // ---- weathers and climates
        // The precipitation model (MODL) and the cloud layers 0-3 [documented: the pages].
        "getweatherprecipitationmodel" | "getweathertexture" => {
            let w = of_sig(g, form(args, 0), b"WTHR");
            let sub = if name == "getweathertexture" { usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| WEATHER_LAYERS.get(i)).copied() } else { Some("MODL") };
            XVal::Str(match w.zip(sub).and_then(|(w, sub)| crate::nvse::record_value(g, w, sub, "")) {
                Some(XVal::Str(s)) => s,
                _ => String::new(),
            })
        }
        "setweatherprecipitationmodel" | "setweathertexture" => {
            let w = of_sig(g, form(args, 0), b"WTHR");
            let (sub, k) = if name == "setweathertexture" { (usize::try_from(num(args, 1, -1.0) as i64).ok().and_then(|i| WEATHER_LAYERS.get(i)).copied(), 2) } else { (Some("MODL"), 1) };
            if let (Some(w), Some(sub), Some(path)) = (w, sub, text(args, k)) {
                g.set_field(w, sub, "", XVal::Str(path));
            }
            zero
        }
        // `[seconds]`: 0 or none removes the override [documented].
        "setweathertransitiontimeoverride" => {
            let s = num(args, 0, 0.0);
            g.other.jip7.weather_transition = (s > 0.0).then_some(s);
            zero
        }
        // `climate weather chance [global]`: a weather already listed takes the chance and
        // global [documented: ClimateAddWeatherType]; 1 when done [documented:
        // `successful`].
        "climateaddweathertype" => {
            let (Some(c), Some(w)) = (of_sig(g, form(args, 0), b"CLMT"), of_sig(g, form(args, 1), b"WTHR")) else { return Some(Ok(zero)) };
            let entry = ClimateWeather { weather: w, chance: num(args, 2, 0.0).clamp(0.0, 100.0) as i64, global: of_sig(g, form(args, 3), b"GLOB") };
            let mut list = g.climate_weathers(c);
            match list.iter_mut().find(|e| e.weather == w) {
                Some(e) => *e = entry,
                None => list.push(entry),
            }
            g.other.jip7.climate_weathers.insert(c, list);
            yes(true)
        }
        "climateremoveweathertype" => {
            let (Some(c), Some(w)) = (of_sig(g, form(args, 0), b"CLMT"), form(args, 1)) else { return Some(Ok(zero)) };
            let mut list = g.climate_weathers(c);
            let had = list.iter().any(|e| e.weather == w);
            list.retain(|e| e.weather != w);
            g.other.jip7.climate_weathers.insert(c, list);
            yes(had)
        }
        // ---- armour
        // `armor class`: 1 light, 2 medium, 3 heavy [documented: SetArmorClass's table].
        "setarmorclass" => {
            let class = num(args, 1, 0.0) as i64;
            if let Some(a) = of_sig(g, form(args, 0), b"ARMO").filter(|_| (1..=3).contains(&class)) {
                let flags = g.num_field(a, "BMDT", "general_flags") as u64 & !(MEDIUM | HEAVY);
                let flags = flags | if class == 2 { MEDIUM } else if class == 3 { HEAVY } else { 0 };
                g.set_field(a, "BMDT", "general_flags", XVal::Num(flags as f64));
            }
            zero
        }
        // `armor path`: the ragdoll constraint template (BMCT) [documented: SetArmorRCT].
        "setarmorrct" => {
            if let (Some(a), Some(path)) = (of_sig(g, form(args, 0), b"ARMO"), text(args, 1)) {
                g.set_field(a, "BMCT", "", XVal::Str(path));
            }
            zero
        }
        // `[multiplier]`: sets it, and returns it; left out, returns it unchanged
        // [documented: SetArmorConditionPenalty].
        "setarmorconditionpenalty" => {
            if let Some(XVal::Num(m)) = args.first() {
                g.other.jip7.armor_condition_penalty = Some(*m);
            }
            XVal::Num(g.armor_condition_penalty())
        }
        // `[maxPercent]` 0-1, the same way [documented: SetDamageToArmorMaxPercent]; 1
        // (no cap) until a script sets it [hypothesis: the page gives no default].
        "setdamagetoarmormaxpercent" => {
            if let Some(XVal::Num(m)) = args.first() {
                g.other.jip7.damage_to_armor_max = Some(m.clamp(0.0, 1.0));
            }
            XVal::Num(g.other.jip7.damage_to_armor_max.unwrap_or(1.0))
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// An impact data's flags as GetImpactDataTrait 16 gives them: 1 No Decal Data (DATA's
/// flag bit 0), 2 Parallax, 4 Alpha Blending, 8 Alpha Testing (DODT's flag bits 0-2)
/// [documented: the page's table; the bits: xEdit IPCT DATA and the decal data].
fn impact_flags(g: &Game, i: FormId) -> u64 {
    let data = g.num_field(i, "DATA", "flags") as u64 & 1;
    let decal = g.num_field(i, "DODT", "flags") as u64 & 7;
    data | (decal << 1)
}

/// The decal colour as R × 1000000 + G × 1000 + B [documented: "Color (RRRGGGBBB)"].
fn impact_colour(g: &Game, i: FormId) -> f64 {
    let c: Vec<f64> = ["color.red", "color.green", "color.blue"].iter().map(|p| g.num_field(i, "DODT", p)).collect();
    c[0] * 1_000_000.0 + c[1] * 1000.0 + c[2]
}
