//! The most called LU, JohnnyGuitar, ShowOff and kNVSE functions the mod lists use past
//! JIP (docs/handoff/modlist-function-counts.json) [documented: their pages; observed:
//! the peer's plugins sets in the original game]. Session state lives in
//! `crate::nvse_gaps::GapsState` where a setter needs it.

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
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

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// The script extender files NVForever stands in for, by name without `.dll`, without
/// regard to case; a name with `.dll` is not found [observed: plugins1 and plugins2, the
/// five plugins of the owner's Data\NVSE\Plugins and the extender's own nvse_1_4 and
/// nvse_steam_loader give 1, "jip_nvse.dll" 0].
const DLLS: [&str; 7] = ["jip_nvse", "johnnyguitar", "showoffnvse", "knvse", "mcm", "nvse_1_4", "nvse_steam_loader"];

/// ALCH ENIT flag bits by LU's flag ID: 0 none, 1 and 3 bit 0 (No Auto-Calc), 2 bit 1
/// (Food Item) [observed: plugins1 and plugins2, Cram and Beer (flags 3) read 0 1 1 1,
/// Nuka-Cola, a poison (flags 1) and the stimpak (flags 5) 0 1 0 1, Silver Sting (0)
/// 0 0 0 0; hypothesis: what IDs 1 and 3 stand for].
const INGESTIBLE_BITS: [Option<u32>; 4] = [None, Some(0), Some(1), Some(0)];

/// ShowOff's easing of `t` (0–1) by name and mode, 0 in, 1 out, 2 in-out, the Penner
/// curves; an unknown name gives -1 [observed: plugins1 and plugins2, quad, cubic, quart,
/// quint, sine, expo, circ, back, elastic and bounce at 0.25 in all three modes;
/// "linear" -1].
fn ease(t: f64, name: &str, mode: i64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let f: fn(f64) -> f64 = match name.to_ascii_lowercase().as_str() {
        "quad" => |t| t * t,
        "cubic" => |t| t * t * t,
        "quart" => |t| t.powi(4),
        "quint" => |t| t.powi(5),
        "sine" => |t| 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
        "expo" => |t| if t <= 0.0 { 0.0 } else { 2f64.powf(10.0 * t - 10.0) },
        "circ" => |t| 1.0 - (1.0 - t * t).max(0.0).sqrt(),
        "back" => |t| {
            let c1 = 1.70158;
            (c1 + 1.0) * t * t * t - c1 * t * t
        },
        "elastic" => |t| {
            if t <= 0.0 || t >= 1.0 {
                return t;
            }
            let c4 = 2.0 * std::f64::consts::PI / 3.0;
            -(2f64.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
        },
        "bounce" => |t| 1.0 - bounce_out(1.0 - t),
        _ => return -1.0,
    };
    // Penner's in-out back and elastic have constants of their own.
    let lower = name.to_ascii_lowercase();
    if mode == 2 && lower == "back" {
        let c2 = 1.70158 * 1.525;
        return if t < 0.5 {
            (2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2) / 2.0
        } else {
            ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
        };
    }
    if mode == 2 && lower == "elastic" {
        let c5 = 2.0 * std::f64::consts::PI / 4.5;
        return if t <= 0.0 || t >= 1.0 {
            t
        } else if t < 0.5 {
            -(2f64.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
        } else {
            2f64.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin() / 2.0 + 1.0
        };
    }
    match mode {
        0 => f(t),
        1 => 1.0 - f(1.0 - t),
        _ if t < 0.5 => f(2.0 * t) / 2.0,
        _ => 1.0 - f(2.0 - 2.0 * t) / 2.0,
    }
}

/// Penner's bounce, out.
fn bounce_out(t: f64) -> f64 {
    let (n1, d1) = (7.5625, 2.75);
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- LU ----
        // IDLE MODL; the setter for the session [observed: plugins1].
        "getidleanimpath" => match form(args, 0).and_then(|f| crate::nvse::record_value(g, f, "MODL", "")) {
            Some(XVal::Str(s)) => XVal::Str(s),
            _ => XVal::Str(String::new()),
        },
        "setidleanimpath" => {
            if let Some(f) = form(args, 0) {
                g.field_overrides.insert((f, "MODL/".into()), XVal::Str(text(args, 1)));
            }
            zero
        }
        // An entry point's second value; none reads -1 and the setter leaves it
        // [observed: plugins1, Toughness's first entry].
        "getnthperkentryvalue2" | "setnthperkentryvalue2" => XVal::Num(-1.0),
        // [observed: plugins1, Recipe127mm 4 inputs, the first 00140AAA; category
        // 00153621, subcategory 0013E44E]
        "getrecipeinputforms" => {
            let ins: Vec<XVal> = form(args, 0).and_then(|f| g.data.recipes.get(&f)).map(|r| r.ingredients.iter().map(|(f, _)| XVal::Ref(f.0)).collect()).unwrap_or_default();
            list(g, ins)
        }
        "getrecipecategory" | "getrecipesubcategory" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let sub = name == "getrecipesubcategory";
            let r = g.data.recipes.get(&f);
            let rec = r.and_then(|r| if sub { r.subcategory } else { r.category });
            XVal::Ref(g.gaps.recipe_categories.get(&(f, sub)).copied().or(rec).map_or(0, |c| c.0))
        }
        // `item [base]`: the count in the base's item list, leveled entries not rolled
        // [observed: plugins1, a stimpak on Doc's base and on Doc 1].
        "basegetitemcount" => {
            let item = form(args, 0).unwrap_or(FormId(0));
            let b = form(args, 1).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            XVal::Num(f64::from(g.world.base_entries(b).unwrap_or_default().iter().filter(|e| e.item == item).map(|e| e.count).sum::<i32>()))
        }
        // The calling script's plugin; a runtime script has none [observed: plugins1, 0].
        "getselfmodindex" => zero,
        // The rank the actor has, 0 without [observed: plugins1, the player without
        // Toughness 0].
        "hasperkrank" => {
            let p = form(args, 0).unwrap_or(FormId(0));
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            XVal::Num(if a == FormId::PLAYER_REF { f64::from(g.player.perks.get(&p).copied().unwrap_or(0)) } else { 0.0 })
        }
        "getingestibleflag" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let flags = crate::nvse::record_value(g, f, "ENIT", "flags").and_then(|v| v.num().ok()).unwrap_or(0.0) as u32;
            let k = num(args, 1, -1.0);
            yes((0.0..4.0).contains(&k) && INGESTIBLE_BITS[k as usize].is_some_and(|b| flags >> b & 1 != 0))
        }
        // [observed: plugins1, 2]
        "getdefaultmessagetime" => XVal::Num(2.0),
        // ---- JohnnyGuitar ----
        // The current value by code [observed: plugins1, Strength 5, Health 200, Barter
        // and Guns 17].
        "getactorvaluealt" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            XVal::Num(crate::functions::actor::actor_value(g, a, num(args, 0, 0.0) as u16).unwrap_or(0.0))
        }
        // The value within the range the game counts [observed: plugins1, Health 200,
        // Strength 5].
        "getthresholdedactorvalue" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let code = match args.first() {
                Some(XVal::Num(c)) => *c as u16,
                Some(XVal::Str(n)) => match crate::functions::av_code(g, n) {
                    Ok(c) => c,
                    Err(_) => return Some(Ok(zero)),
                },
                _ => return Some(Ok(zero)),
            };
            XVal::Num(crate::functions::actor::actor_value_capped(g, a, code))
        }
        "getquestfailed" => yes(form(args, 0).is_some_and(|q| g.quest_state(q).failed)),
        // RACE DATA flags bit by bit [observed: plugins1, CaucasianOld 0 0 0].
        "getraceflag" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let flags = crate::nvse::record_value(g, f, "DATA", "flags").and_then(|v| v.num().ok()).unwrap_or(0.0) as u64;
            let k = num(args, 1, -1.0);
            yes((0.0..32.0).contains(&k) && flags >> (k as u64) & 1 != 0)
        }
        "isdllloaded" => {
            let n = text(args, 0).to_ascii_lowercase();
            yes(DLLS.contains(&n.as_str()))
        }
        // `on [ask]`: returns 1; GetAlwaysRun reads the engine's switch, which takes the
        // ask at its next frame [observed: plugins1].
        "setalwaysrun" => {
            let on = num(args, 0, 1.0) != 0.0;
            g.gaps.always_run_request = Some(on);
            g.gaps.always_run = on;
            XVal::Num(1.0)
        }
        "getalwaysrun" => yes(g.gaps.always_run),
        // `value inMin inMax outMin outMax`, not clamped [observed: plugins1, 5 of 0–10
        // onto 100–200 is 150, 15 is 250].
        "remap" => {
            let (v, a, b, c, d) = (num(args, 0, 0.0), num(args, 1, 0.0), num(args, 2, 1.0), num(args, 3, 0.0), num(args, 4, 1.0));
            XVal::Num(if b == a { c } else { c + (v - a) * (d - c) / (b - a) })
        }
        "getvector3ddistance" => {
            let p: Vec<f64> = (0..6).map(|k| num(args, k, 0.0)).collect();
            XVal::Num(((p[3] - p[0]).powi(2) + (p[4] - p[1]).powi(2) + (p[5] - p[2]).powi(2)).sqrt())
        }
        // Forms only the masters define [observed: plugins1, the stimpak 0 and 0;
        // hypothesis: NVForever does not follow overrides by index here].
        "getformoverrideindex" | "isformoverridden" => zero,
        // ZNAM; Set… for the session, none leaving the sound [observed: plugins2, the
        // beep reads back, a null set does not clear it].
        "getputdownsound" => match form(args, 0).and_then(|f| crate::nvse::record_value(g, f, "ZNAM", "")) {
            Some(XVal::Ref(r)) => XVal::Ref(r),
            _ => XVal::Ref(0),
        },
        "setpickupsound" | "setputdownsound" => {
            if let (Some(f), Some(s)) = (form(args, 0), form(args, 1)) {
                let sub = if name == "setpickupsound" { "YNAM/" } else { "ZNAM/" };
                g.field_overrides.insert((f, sub.into()), XVal::Ref(s.0));
            }
            zero
        }
        // [observed: plugins2, the 127mm recipe's subcategory set and back]
        "setrecipecategory" | "setrecipesubcategory" => {
            if let (Some(f), Some(c)) = (form(args, 0), form(args, 1)) {
                g.gaps.recipe_categories.insert((f, name == "setrecipesubcategory"), c);
            }
            zero
        }
        // ACTI SNAM and WNAM [observed: plugins2, the electric box none and none].
        "getactivatorsoundactivate" | "getactivatorwatertype" | "getactivatorsoundlooping" => {
            let sub = match name {
                "getactivatorsoundactivate" => "VNAM",
                "getactivatorwatertype" => "WNAM",
                _ => "SNAM",
            };
            match form(args, 0).and_then(|f| crate::nvse::record_value(g, f, sub, "")) {
                Some(XVal::Ref(r)) => XVal::Ref(r),
                _ => XVal::Ref(0),
            }
        }
        // `list form`: the index it was at, -1 when absent [observed: plugins2, the added
        // stimpak came off from index 0 and a second try gave -1].
        "removeformfromformlist" => {
            let (Some(l), Some(f)) = (form(args, 0), form(args, 1)) else { return Some(Ok(XVal::Num(-1.0))) };
            let mut members = crate::functions::list_members(g, l);
            let Some(i) = members.iter().position(|&m| m == f) else { return Some(Ok(XVal::Num(-1.0))) };
            members.remove(i);
            if let Some(added) = g.form_lists.get_mut(&l) {
                added.retain(|&m| m != f);
            }
            g.list_edits.insert(l, members);
            XVal::Num(i as f64)
        }
        // A map marker's WMI1; the setter for the session [observed: plugins2, the
        // Goodsprings marker's RepNVGoodsprings, set to Primm's and back].
        "getmapmarkerrep" => {
            let r = subject.unwrap_or(FormId(0));
            let rec = g.world.placed.get(r).and_then(|p| p.marker_reputation);
            XVal::Ref(g.gaps.marker_reputations.get(&r).copied().unwrap_or(rec).map_or(0, |f| f.0))
        }
        "setmapmarkerrep" => {
            if let Some(r) = subject {
                g.gaps.marker_reputations.insert(r, form(args, 0));
            }
            zero
        }
        // ---- ShowOff ----
        "applyeasing" => XVal::Num(ease(num(args, 0, 0.0), &text(args, 1), num(args, 2, 0.0) as i64)),
        // DNAM animation type, unarmed 0 [observed: plugins1, the unarmed player 0].
        "getequippedweapontype" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let w = crate::functions::items::equipped_weapon(g, a);
            XVal::Num(w.and_then(|w| crate::nvse::record_value(g, w, "DNAM", "animation_type")).and_then(|v| v.num().ok()).unwrap_or(0.0))
        }
        "isaiming" | "isblockingoraiming" => yes(subject.is_none_or(|a| a == FormId::PLAYER_REF) && g.jip2.told.aiming),
        // Carrying more than the capacity [observed: plugins1, 0].
        "getisplayeroverencumbered" => {
            let w = g.world.carried_weight(FormId::PLAYER_REF).unwrap_or(0.0);
            let cap = crate::functions::actor::actor_value(g, FormId::PLAYER_REF, crate::actor_values::CARRY_WEIGHT).unwrap_or(0.0);
            yes(w > cap)
        }
        // A weapon of a gun skill [observed: plugins1, the pistol 1, the knife 0].
        "isweaponranged" => {
            let w = form(args, 0).or_else(|| subject.and_then(|a| crate::functions::items::equipped_weapon(g, a))).unwrap_or(FormId(0));
            let s = g.data.weapon_skills.get(&w).map(|&s| s as u16);
            yes(s.is_some_and(|s| s != crate::actor_values::MELEE_WEAPONS && s != crate::actor_values::UNARMED))
        }
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_match_the_observed_rows() {
        let f = |n: &str, m: i64| (ease(0.25, n, m) * 10000.0).floor();
        assert_eq!([f("quad", 0), f("quad", 1), f("quad", 2)], [625.0, 4375.0, 1250.0]);
        assert_eq!([f("cubic", 0), f("cubic", 1), f("cubic", 2)], [156.0, 5781.0, 625.0]);
        assert_eq!([f("sine", 0), f("sine", 1), f("sine", 2)], [761.0, 3826.0, 1464.0]);
        assert_eq!(ease(0.25, "linear", 0), -1.0);
        // [observed: plugins2]
        assert_eq!([f("quart", 0), f("quart", 1), f("quart", 2)], [39.0, 6835.0, 312.0]);
        assert_eq!([f("quint", 0), f("quint", 1), f("quint", 2)], [9.0, 7626.0, 156.0]);
        assert_eq!([f("expo", 0), f("expo", 1), f("expo", 2)], [55.0, 8232.0, 156.0]);
        assert_eq!([f("circ", 0), f("circ", 1), f("circ", 2)], [317.0, 6614.0, 669.0]);
        assert_eq!([f("back", 0), f("back", 1), f("back", 2)], [-642.0, 8174.0, -997.0]);
        assert_eq!([f("elastic", 0), f("elastic", 1), f("elastic", 2)], [-56.0, 9116.0, 119.0]);
        assert_eq!([f("bounce", 0), f("bounce", 1), f("bounce", 2)], [273.0, 4726.0, 1171.0]);
    }
}
