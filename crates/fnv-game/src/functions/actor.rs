//! Actor functions: actor values, death, factions, voice, sex, race, flags.
//!
//! Actor value codes are the indices of `ActorValue` in `schemas/fnv/common.json`
//! [documented: xEdit; scripts.md §2.4]. S.P.E.C.I.A.L. (codes 5–11) live in the Layer 4
//! world (base from `NPC_ DATA`); the body part conditions (codes 25–31) start at 100
//! (undamaged) and the other values at 0 here [hypothesis: their base values (skills,
//! health, derived stats) are not read yet; a new character has no crippled limbs] and
//! change through the setters.

use super::{base_of, none, yes, Args, Ctx};
use crate::game::{event, Game};
use fnv_script::vm::Value;
use fnv_sim::forms::SPECIAL;
use fnv_sim::FormId;

fn special(code: u16) -> Option<&'static str> {
    (5..=11).contains(&code).then(|| SPECIAL[(code - 5) as usize])
}

pub fn actor_value(g: &mut Game, actor: FormId, code: u16) -> Result<f64, String> {
    match special(code) {
        Some(name) => g.world.actor_value(actor, name).map(|v| v as f64).map_err(|e| e.to_string()),
        None => Ok(g.extra.get(&actor).and_then(|e| e.actor_values.get(&code)).copied().unwrap_or(default_value(code))),
    }
}

fn set_value(g: &mut Game, actor: FormId, code: u16, v: f64) -> Result<(), String> {
    match special(code) {
        Some(name) => g.world.set_actor_value(actor, name, v as f32).map_err(|e| e.to_string()),
        None => {
            g.extra.entry(actor).or_default().actor_values.insert(code, v);
            Ok(())
        }
    }
}

pub fn get_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(actor_value(g, r, a.code(0)?)?))
}

/// The base value: for S.P.E.C.I.A.L. the world's value (base unless set), otherwise
/// the stored value [hypothesis: no separate modifiers are kept yet].
pub fn get_base_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    get_actor_value(g, ctx, a)
}

/// `SetActorValue` / `ForceActorValue`: the value becomes the given number.
pub fn set_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    set_value(g, r, a.code(0)?, a.num(1)?)?;
    none()
}

/// `ModActorValue` / `RestoreActorValue`: add the amount [hypothesis: Restore is not
/// capped at the base value yet].
pub fn mod_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = a.code(0)?;
    let v = actor_value(g, r, code)? + a.num(1)?;
    set_value(g, r, code, v)?;
    none()
}

pub fn damage_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = a.code(0)?;
    let v = actor_value(g, r, code)? - a.num(1)?;
    set_value(g, r, code, v)?;
    none()
}

pub fn get_dead(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.extra.get(&r).is_some_and(|e| e.dead))
}

/// The actor dies; its OnDeath blocks run [hypothesis: at once], and the kill counts
/// for GetDeadCount of its base.
pub fn kill_actor(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if g.extra.get(&r).is_some_and(|e| e.dead) {
        return none();
    }
    g.extra.entry(r).or_default().dead = true;
    if let Some(b) = base_of(g, r) {
        *g.player.dead_count.entry(b).or_default() += 1;
    }
    let killer = a.opt_form(0);
    g.ref_event(r, event::ON_DEATH, killer.map(|k| Value::Ref(k.0)), killer);
    none()
}

pub fn resurrect_actor(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().dead = false;
    none()
}

pub fn get_dead_count(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let b = a.form(0)?;
    Ok(Value::Num(g.player.dead_count.get(&b).copied().unwrap_or(0) as f64))
}

/// Faction rank of an actor: a run-time change, else the base form's `SNAM`; −1 when not
/// a member.
pub fn faction_rank(g: &Game, r: FormId, f: FormId) -> i32 {
    if let Some(&rank) = g.extra.get(&r).and_then(|e| e.factions.get(&f)) {
        return rank as i32;
    }
    base_of(g, r)
        .and_then(|b| g.data.actors.get(&b))
        .and_then(|a| a.factions.iter().find(|(x, _)| *x == f))
        .map(|(_, rank)| *rank as i32)
        .unwrap_or(-1)
}

pub fn get_in_faction(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(faction_rank(g, r, a.form(0)?) >= 0)
}

pub fn get_faction_rank(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(faction_rank(g, r, a.form(0)?) as f64))
}

/// `AddToFaction` / `SetFactionRank`: the actor reference joins at the rank.
pub fn add_to_faction(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let f = a.form(0)?;
    let rank = a.num_or(1, 0.0) as i8;
    g.extra.entry(r).or_default().factions.insert(f, rank);
    none()
}

pub fn remove_from_faction(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().factions.insert(a.form(0)?, -1);
    none()
}

/// The voice type can be given directly or as a form list of voice types.
pub fn get_is_voice_type(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let v = a.form(0)?;
    let Some(voice) = base_of(g, r).and_then(|b| g.data.actors.get(&b)).and_then(|d| d.voice) else { return yes(false) };
    yes(voice == v || super::refs::list_members(g, v).contains(&voice))
}

fn is_female(g: &Game, r: FormId) -> bool {
    if r == FormId::PLAYER_REF {
        if let Some(f) = g.player.female {
            return f;
        }
    }
    base_of(g, r).and_then(|b| g.data.actors.get(&b)).is_some_and(|a| a.female)
}

/// Sex codes: 0 male, 1 female (scripts.md §2.4).
pub fn get_is_sex(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(is_female(g, r) == (a.code(0)? == 1))
}

pub fn get_pc_is_sex(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(is_female(g, FormId::PLAYER_REF) == (a.code(0)? == 1))
}

/// `SexChange [sex]`: without a calling reference it changes the player; without a sex
/// it flips [documented: catalogue summary]. Other actors: not modelled.
pub fn sex_change(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    if r != FormId::PLAYER_REF {
        return Err("SexChange on an actor other than the player is not modelled".into());
    }
    let female = match a.code(0) {
        Ok(c) => c == 1,
        Err(_) => !is_female(g, r),
    };
    g.player.female = Some(female);
    none()
}

pub fn get_is_race(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(base_of(g, r).and_then(|b| g.data.actors.get(&b)).and_then(|d| d.race) == Some(a.form(0)?))
}

pub fn get_is_class(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(base_of(g, r).and_then(|b| g.data.actors.get(&b)).and_then(|d| d.class) == Some(a.form(0)?))
}

macro_rules! flag {
    ($set:ident, $get:ident, $field:ident) => {
        pub fn $set(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
            let r = ctx.subject()?;
            g.extra.entry(r).or_default().$field = a.num_or(0, 1.0) != 0.0;
            none()
        }
        #[allow(dead_code)]
        pub fn $get(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
            let r = ctx.subject()?;
            yes(g.extra.get(&r).is_some_and(|e| e.$field))
        }
    };
}

flag!(set_ghost, get_ghost, ghost);
flag!(set_restrained, get_restrained, restrained);
flag!(set_unconscious, get_unconscious, unconscious);
flag!(set_player_teammate, get_player_teammate, teammate);

/// Health back to its base: the stored Health (code 16) change is dropped.
pub fn reset_health(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if let Some(e) = g.extra.get_mut(&r) {
        e.actor_values.remove(&16);
    }
    none()
}

/// Recorded on the actor and made its current package (the package runs in Layer 6);
/// also traced as a stub call.
pub fn add_script_package(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let p = a.form(0)?;
    let e = g.extra.entry(r).or_default();
    e.packages.push(p);
    e.current_package = Some(p);
    g.stub_note(ctx, a.0);
    none()
}

/// The script package is dropped; the actor chooses again when its package is next
/// asked for.
pub fn remove_script_package(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if let Some(e) = g.extra.get_mut(&r) {
        e.packages.pop();
        e.current_package = None;
    }
    g.stub_note(ctx, a.0);
    none()
}

/// `EvaluatePackage`: see [`Game::evaluate_package`].
pub fn evaluate_package(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.evaluate_package(r);
    none()
}

/// 1 when the actor's current package is the given one [documented: catalogue summary].
pub fn get_is_current_package(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let p = a.form(0)?;
    yes(g.current_package(r) == Some(p))
}

/// The starting value of an actor value this crate keeps (see the module notes).
fn default_value(code: u16) -> f64 {
    if (25..=31).contains(&code) {
        100.0
    } else {
        0.0
    }
}

/// Current health over base health [documented: catalogue notes]. Without a damage
/// model a living actor is at full health and a dead one at 0 [hypothesis].
pub fn get_health_percentage(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(if g.extra.get(&r).is_some_and(|e| e.dead) { 0.0 } else { 1.0 }))
}
