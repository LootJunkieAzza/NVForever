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

/// The script extender plugin files NVForever stands in for, by name without `.dll`
/// [observed: plugins1, "johnnyguitar" and "jip_nvse" give 1; the files in the owner's
/// Data\NVSE\Plugins: jip_nvse, johnnyguitar, ShowOffNVSE, kNVSE, MCM].
const DLLS: [&str; 5] = ["jip_nvse", "johnnyguitar", "showoffnvse", "knvse", "mcm"];

/// ALCH ENIT flag bits by LU's flag ID [observed: plugins1, the stimpak (flags 5: No
/// Auto-Calc, Medicine) reads 0 1 0 1, Silver Sting 0 0 0 0; hypothesis: 0 food, 2 the
/// next bit].
const INGESTIBLE_BITS: [u32; 4] = [1, 2, 3, 0];

/// ShowOff's easing of `t` (0–1) by name and mode, 0 in, 1 out, 2 in-out; an unknown
/// name gives -1 [observed: plugins1, quad, cubic and sine at 0.25 in all three modes;
/// "linear" -1; hypothesis: quart, quint, expo and circ the same way].
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
        _ => return -1.0,
    };
    match mode {
        0 => f(t),
        1 => 1.0 - f(1.0 - t),
        _ if t < 0.5 => f(2.0 * t) / 2.0,
        _ => 1.0 - f(2.0 - 2.0 * t) / 2.0,
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
            let r = form(args, 0).and_then(|f| g.data.recipes.get(&f));
            XVal::Ref(r.and_then(|r| if name == "getrecipecategory" { r.category } else { r.subcategory }).map_or(0, |c| c.0))
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
            yes((0.0..4.0).contains(&k) && flags >> INGESTIBLE_BITS[k as usize] & 1 != 0)
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
            let n = n.strip_suffix(".dll").unwrap_or(&n);
            yes(DLLS.contains(&n))
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
    }
}
