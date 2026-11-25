//! Player-only source commands (docs/spec-notes/player-source-commands.md).
//! Named calls deliberately do not assert a native GECK parameter encoding.

use super::{actor, reputation, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::{ArgValue, Value, XVal};
use fnv_sim::FormId;

pub(crate) fn handles(name: &str) -> bool {
    ["AdvancePCLevel", "AdvLevel", "ModPCAttribute", "RemoveReputationExact", "SetPCExpelled", "ShowNameMenu", "KillAllActors", "KillAll"]
        .iter()
        .any(|n| n.eq_ignore_ascii_case(name))
}

pub(crate) fn call(g: &mut Game, ctx: &Ctx, receiver: Option<Value>, name: &str, values: &[XVal]) -> Option<Result<XVal, String>> {
    let (min_args, max_args) = match name {
        "advancepclevel" | "advlevel" | "shownamemenu" => (0, 0),
        "killallactors" | "killall" => (0, 1),
        "modpcattribute" | "setpcexpelled" => (2, 2),
        "removereputationexact" => (3, 3),
        _ => return None,
    };
    Some((|| {
        // Valid explicit receivers and explicit null (`0`) receivers are ignored and
        // still target the player [observed: w4g_06_player_cmds_explicit_receiver];
        // non-zero unknown references are still rejected [hypothesis].
        if let Some(value) = receiver {
            let explicit_null = matches!(value, Value::Ref(0)) || matches!(value, Value::Num(x) if x == 0.0);
            if !explicit_null {
                let r = value.as_form().map(FormId).ok_or("invalid calling reference")?;
                if r != FormId::PLAYER_REF && g.world.placed.get(r).is_none() && g.world.peek(r).is_none() {
                    return Err("invalid calling reference".into());
                }
            }
        }
        if values.len() < min_args || values.len() > max_args {
            return Err(if min_args == max_args {
                format!("{name} expects {min_args} arguments")
            } else {
                format!("{name} expects {min_args}..={max_args} arguments")
            });
        }
        let args: Vec<ArgValue> = values.iter().map(|v| match v {
            XVal::Num(n) if n.is_finite() => Ok(ArgValue::Value(Value::Num(*n))),
            XVal::Ref(r) => Ok(ArgValue::Value(Value::Ref(*r))),
            XVal::Str(s) => Ok(ArgValue::Str(s.clone())),
            _ => Err("unsupported player command argument".to_string()),
        }).collect::<Result<_, _>>()?;
        let a = Args(&args);
        match name {
            "advancepclevel" | "advlevel" => { crate::levelup::advance(g); }
            "shownamemenu" => { g.open_menu(crate::game::menus::NAME); }
            "killallactors" | "killall" => {
                let killer = match values.first() {
                    None | Some(XVal::Ref(0)) => None,
                    Some(XVal::Ref(r)) => {
                        let k = FormId(*r);
                        if k != FormId::PLAYER_REF && g.world.placed.get(k).is_none() && g.world.peek(k).is_none() {
                            return Err("invalid killer reference".into());
                        }
                        Some(k)
                    }
                    Some(_) => return Err("KillAllActors killer argument must be a reference".into()),
                };
                super::console::kill_all_actors_in_loaded(g, killer);
            }
            "modpcattribute" => {
                let code = actor::av_code(g, a.str(0)?)?;
                // Integer admission matches the documented signature; valid non-SPECIAL
                // actor values compile, continue without error, and do not mutate state
                // [observed: `w6d5_26_player_source_modpcattribute_edges`, Health 200->200].
                let delta = a.int(1)?;
                if crate::actor_values::SPECIALS.contains(&code)
                    && !crate::revision::keeps(ctx, FormId::PLAYER_REF, || format!("the attribute {} (points are moved on the attributes page)", actor::AV_NAMES[code as usize]))
                {
                    actor::shift_base(g, FormId::PLAYER_REF, code, f64::from(delta))?;
                }
            }
            "removereputationexact" => { reputation::remove_reputation_exact(g, ctx, &a)?; }
            "setpcexpelled" => { actor::set_pc_expelled(g, &a)?; }
            _ => unreachable!(),
        }
        Ok(XVal::Num(0.0))
    })())
}
