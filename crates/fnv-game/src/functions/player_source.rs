//! Player-only source commands (docs/spec-notes/player-source-commands.md).
//! Named calls deliberately do not assert a native GECK parameter encoding.

use super::{actor, reputation, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::{ArgValue, Value, XVal};
use fnv_sim::FormId;

pub(crate) fn handles(name: &str) -> bool {
    ["AdvancePCLevel", "AdvLevel", "ModPCAttribute", "RemoveReputationExact"].iter().any(|n| n.eq_ignore_ascii_case(name))
}

pub(crate) fn call(g: &mut Game, ctx: &Ctx, receiver: Option<Value>, name: &str, values: &[XVal]) -> Option<Result<XVal, String>> {
    let count = match name {
        "advancepclevel" | "advlevel" => 0,
        "modpcattribute" => 2,
        "removereputationexact" => 3,
        _ => return None,
    };
    Some((|| {
        // Keep an explicit empty receiver distinct from an omitted receiver before
        // Host::ext_call converts it to Option<FormId>.
        if let Some(value) = receiver {
            let r = value.as_form().map(FormId).ok_or("invalid calling reference")?;
            if r != FormId::PLAYER_REF && g.world.placed.get(r).is_none() && g.world.peek(r).is_none() {
                return Err("invalid calling reference".into());
            }
        }
        if values.len() != count {
            return Err(format!("{name} expects {count} arguments"));
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
            "modpcattribute" => {
                let code = actor::av_code(g, a.str(0)?)?;
                if !crate::actor_values::SPECIALS.contains(&code) {
                    return Err("ModPCAttribute requires a SPECIAL attribute".into());
                }
                // Integer admission matches the documented signature; use the same
                // base writer as the attribute sheet so pools and carry state agree.
                let delta = a.int(1)?;
                if !crate::revision::keeps(ctx, FormId::PLAYER_REF, || format!("the attribute {} (points are moved on the attributes page)", actor::AV_NAMES[code as usize])) {
                    actor::shift_base(g, FormId::PLAYER_REF, code, f64::from(delta))?;
                }
            }
            "removereputationexact" => { reputation::remove_reputation_exact(g, ctx, &a)?; }
            _ => unreachable!(),
        }
        Ok(XVal::Num(0.0))
    })())
}
