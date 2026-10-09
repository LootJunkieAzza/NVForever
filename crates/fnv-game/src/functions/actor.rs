//! Actor functions: actor values, death, factions, voice, sex, race, flags.
//!
//! Actor value codes are the indices of `ActorValue` in `schemas/fnv/common.json`
//! [documented: xEdit; scripts.md §2.4]. S.P.E.C.I.A.L. (codes 5–11) live in the Layer 4
//! world (base from `NPC_ DATA`); Health (code 16) starts at the actor's maximum health
//! (`crate::combat`); the body part conditions (codes 25–31) start at 100 (undamaged)
//! and the other values at 0 here [hypothesis: their base values (skills, derived stats)
//! are not read yet; a new character has no crippled limbs] and change through the
//! setters.
//!
//! Each value is a permanent part (base plus permanent modifiers: SetActorValue,
//! ModActorValue, ForceActorValue) minus a damage part (DamageActorValue, healed by
//! RestoreActorValue, which never heals past zero damage) [documented: catalogue
//! summaries of DamageActorValue and RestoreActorValue; observed: Contreras' supplies
//! restoring 100 of a head condition damaged by 40 left it at 100,
//! `L5.quest.effect_start`, layer5-captures.md §9.3 b]. Base and permanent modifiers are
//! kept together [hypothesis: nothing captured tells them apart yet].

use super::{base_of, none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::forms::SPECIAL;
use fnv_sim::FormId;

fn special(code: u16) -> Option<&'static str> {
    (5..=11).contains(&code).then(|| SPECIAL[(code - 5) as usize])
}

/// The names of the actor value codes 0–76 [documented: xEdit `ActorValue`,
/// `schemas/fnv/common.json`].
const AV_NAMES: [&str; 77] = [
    "Aggression", "Confidence", "Energy", "Responsibility", "Mood", "Strength", "Perception", "Endurance",
    "Charisma", "Intelligence", "Agility", "Luck", "Action Points", "Carry Weight", "Critical Chance",
    "Heal Rate", "Health", "Melee Damage", "Damage Resistance", "Poison Resistance", "Rad Resistance",
    "Speed Multiplier", "Fatigue", "Karma", "XP", "Perception Condition", "Endurance Condition",
    "Left Attack Condition", "Right Attack Condition", "Left Mobility Condition", "Right Mobility Condition",
    "Brain Condition", "Barter", "Big Guns", "Energy Weapons", "Explosives", "Lockpick", "Medicine",
    "Melee Weapons", "Repair", "Science", "Guns", "Sneak", "Speech", "Survival", "Unarmed",
    "Inventory Weight", "Paralysis", "Invisibility", "Chameleon", "Night Eye", "Turbo", "Fire Resistance",
    "Water Breathing", "Rad Level", "Bloody Mess", "Unarmed Damage", "Assistance", "Electric Resistance",
    "Frost Resistance", "Energy Resistance", "EMP Resistance", "Variable01", "Variable02", "Variable03",
    "Variable04", "Variable05", "Variable06", "Variable07", "Variable08", "Variable09", "Variable10",
    "Ignore Crippled Limbs", "Dehydration", "Hunger", "Sleep Deprivation", "Damage Threshold",
];

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '_').flat_map(char::to_lowercase).collect()
}

/// An actor value code from its xEdit name (`Perception Condition`), without regard to
/// case and spaces.
pub fn av_by_name(name: &str) -> Option<u16> {
    let want = squash(name);
    AV_NAMES.iter().position(|n| squash(n) == want).map(|i| i as u16)
}

/// An actor value code from its name: the script name the decoder uses
/// (`encodings.json`, e.g. `NightEye`, `PerceptionCondition`) or the xEdit name, either
/// without regard to case and spaces (`Nighteye`, `Night Eye`), or a number.
pub fn av_code(g: &Game, name: &str) -> Result<u16, String> {
    if let Ok(n) = name.parse::<u16>() {
        return Ok(n);
    }
    let want = squash(name);
    if let Some(m) = g.data.table.enums.get("actor_value") {
        if let Some((&c, _)) = m.iter().find(|(_, n)| squash(n) == want) {
            return Ok(c);
        }
    }
    AV_NAMES.iter().position(|n| squash(n) == want).map(|i| i as u16).ok_or_else(|| format!("no actor value named {name:?}"))
}

/// The permanent part of an actor value (base and permanent modifiers). Health's base is
/// the actor's maximum health (`crate::combat`) until a script sets it.
fn permanent(g: &mut Game, actor: FormId, code: u16) -> Result<f64, String> {
    match special(code) {
        Some(name) => g.world.actor_value(actor, name).map(|v| v as f64).map_err(|e| e.to_string()),
        None => match g.extra.get(&actor).and_then(|e| e.actor_values.get(&code)).copied() {
            Some(v) => Ok(v),
            None if code == crate::combat::HEALTH => Ok(g.max_health(actor)),
            None => Ok(default_value(code)),
        },
    }
}

/// The permanent part of an actor value, 0 when it cannot be read.
pub(crate) fn actor_value_base(g: &mut Game, actor: FormId, code: u16) -> f64 {
    permanent(g, actor, code).unwrap_or(0.0)
}

/// Add `d` to an actor value's damage part (as DamageActorValue).
pub(crate) fn add_damage(g: &mut Game, actor: FormId, code: u16, d: f64) {
    let total = damage(g, actor, code) + d.max(0.0);
    set_damage(g, actor, code, total);
}

/// The damage an actor value has taken (0 or more).
pub fn damage(g: &Game, actor: FormId, code: u16) -> f64 {
    g.extra.get(&actor).and_then(|e| e.av_damage.get(&code)).copied().unwrap_or(0.0)
}

fn set_damage(g: &mut Game, actor: FormId, code: u16, d: f64) {
    let e = g.extra.entry(actor).or_default();
    if d > 0.0 {
        e.av_damage.insert(code, d);
    } else {
        e.av_damage.remove(&code);
    }
}

/// The current value: the permanent part minus the damage.
pub fn actor_value(g: &mut Game, actor: FormId, code: u16) -> Result<f64, String> {
    Ok(permanent(g, actor, code)? - damage(g, actor, code))
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

/// Make the current value exactly `v` with no damage (the state a capture save reports:
/// the runner's `given` of kind `actor_value`).
pub fn set_current(g: &mut Game, actor: FormId, code: u16, v: f64) -> Result<(), String> {
    set_damage(g, actor, code, 0.0);
    set_value(g, actor, code, v)
}

pub fn get_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(actor_value(g, r, a.code(0)?)?))
}

/// The base value: the permanent part, without damage [hypothesis: base and permanent
/// modifiers are not kept apart].
pub fn get_base_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(permanent(g, r, a.code(0)?)?))
}

/// `SetActorValue`: the base value becomes the given number; damage stays [documented:
/// catalogue summary, "sets the base value"].
pub fn set_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    set_value(g, r, a.code(0)?, a.num(1)?)?;
    none()
}

/// `ForceActorValue`: the current value becomes the given number, by changing the
/// permanent part by the difference [documented: catalogue notes].
pub fn force_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = a.code(0)?;
    let diff = a.num(1)? - actor_value(g, r, code)?;
    let v = permanent(g, r, code)? + diff;
    set_value(g, r, code, v)?;
    none()
}

/// `ModActorValue`: a permanent change by the amount [documented: catalogue summary].
pub fn mod_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = a.code(0)?;
    let v = permanent(g, r, code)? + a.num(1)?;
    set_value(g, r, code, v)?;
    none()
}

/// `DamageActorValue`: the damage grows by the amount [documented: catalogue summary; a
/// negative amount heals at most the damage taken, hypothesis].
pub fn damage_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = a.code(0)?;
    let d = damage(g, r, code) + a.num(1)?;
    set_damage(g, r, code, d);
    none()
}

/// `RestoreActorValue`: the damage shrinks by the amount, never below zero, so the value
/// never rises past its permanent part [documented: catalogue notes; observed:
/// `L5.quest.effect_start`].
pub fn restore_actor_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = a.code(0)?;
    let d = damage(g, r, code) - a.num(1)?.max(0.0);
    set_damage(g, r, code, d);
    none()
}

pub fn get_dead(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.extra.get(&r).is_some_and(|e| e.dead))
}

/// The actor dies; its OnDeath blocks run [hypothesis: at once], and the kill counts
/// for GetDeadCount of its base (`Game::kill`, shared with combat deaths).
pub fn kill_actor(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.kill(r, a.opt_form(0));
    none()
}

/// `actor.DetonatePlacedExplosives`: the actor's placed explosives that detonate (C-4)
/// go off [documented: GECK wiki Projectile, "Detonates"]. The projectiles are the
/// engine's: the request waits in `Game::combat_requests`.
pub fn detonate_placed_explosives(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.on.or(ctx.self_ref).unwrap_or(FormId::PLAYER_REF);
    g.combat_requests.push(crate::combat::CombatRequest::Detonate(r));
    none()
}

/// The body location of the actor's last hit, −1 for none [documented: catalogue notes;
/// explosions give −1].
pub fn get_hit_location(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(g.extra.get(&r).and_then(|e| e.last_hit).map_or(-1, |h| h.location) as f64))
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

fn actor_def<'d>(g: &'d Game, r: FormId) -> Option<&'d crate::data::ActorDef> {
    base_of(g, r).and_then(|b| g.data.actors.get(&b))
}

/// 1 for an actor whose race has the Child flag (RACE DATA) [documented: catalogue
/// summary of IsChild].
pub fn is_child(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(actor_def(g, r).and_then(|d| d.race).is_some_and(|race| g.data.child_races.contains(&race)))
}

/// 1 for a creature of the given type (CREA DATA type) [documented: catalogue summary];
/// 0 for NPCs.
pub fn get_is_creature_type(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let t = a.code(0)? as i64;
    yes(actor_def(g, r).and_then(|d| d.creature_type) == Some(t))
}

/// Essential: the reference's own flag (SetActorRefEssential) or its base's (ACBS,
/// changed by SetEssential). The reference flag cannot clear the base's [documented:
/// catalogue notes of SetActorRefEssential].
pub fn essential(g: &Game, r: FormId) -> bool {
    let base = base_of(g, r);
    let base_flag = base
        .and_then(|b| g.forms.essential.get(&b).copied())
        .unwrap_or_else(|| base.and_then(|b| g.data.actors.get(&b)).is_some_and(|d| d.essential));
    base_flag || g.extra.get(&r).and_then(|e| e.essential).unwrap_or(false)
}

/// `SetEssential base flag`: the base form's flag, for every reference [documented:
/// catalogue summary].
pub fn set_essential(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let b = a.form(0)?;
    let b = base_of(g, b).filter(|_| g.world.placed.get(b).is_some()).unwrap_or(b);
    g.forms.essential.insert(b, a.num_or(1, 1.0) != 0.0);
    none()
}

pub fn set_actor_ref_essential(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().essential = Some(a.num_or(0, 1.0) != 0.0);
    none()
}

pub fn is_essential(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(essential(g, r))
}

/// `actor.SetActorFullName message`: the base's name becomes the message's title, for
/// every reference [documented: catalogue notes]; kept for the engine's name display.
pub fn set_actor_full_name(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let m = a.form(0)?;
    let b = base_of(g, r).unwrap_or(r);
    g.forms.names.insert(b, m);
    none()
}

/// 1 when the given actor killed the caller (KillActor's killer) [documented:
/// catalogue summary].
pub fn is_killer(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let k = a.form(0)?;
    yes(g.extra.get(&r).and_then(|e| e.killer) == Some(k))
}

/// The factions an actor is in now: its base's (SNAM) unless removed, and those added
/// at run time.
pub fn factions_of(g: &Game, r: FormId) -> Vec<FormId> {
    let mut out: Vec<FormId> = actor_def(g, r).map(|d| d.factions.iter().map(|f| f.0).collect()).unwrap_or_default();
    if let Some(e) = g.extra.get(&r) {
        out.extend(e.factions.keys().copied());
    }
    out.sort();
    out.dedup();
    out.retain(|&f| faction_rank(g, r, f) >= 0);
    out
}

/// The group combat reaction of faction `a` towards `b`: a run-time SetEnemy/SetAlly,
/// else FACT XNAM, else neutral (0) [documented: catalogue notes of
/// GetFactionCombatReaction: 0 neutral for unlisted pairs, 1 enemy, 2 ally, 3 friend].
pub fn reaction(g: &Game, a: FormId, b: FormId) -> i64 {
    if let Some(&v) = g.forms.faction_reactions.get(&a).and_then(|m| m.get(&b)) {
        return v;
    }
    g.data.faction_reactions.get(&a).and_then(|l| l.iter().find(|x| x.0 == b)).map(|x| x.1).unwrap_or(0)
}

pub fn get_faction_combat_reaction(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(reaction(g, a.form(0)?, a.form(1)?) as f64))
}

/// `actor.GetFactionRelation other` (0 neutral, 1 enemy, 2 ally, 3 friend)
/// [documented: GECK wiki GetFactionRelation]. How the factions combine is a
/// [hypothesis]: the reactions of the caller's factions towards the other's factions,
/// enemy first, then friend, then ally; the player counts as an enemy of a caller in a
/// faction the player is a crime enemy of.
pub fn get_faction_relation(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let o = a.form(0)?;
    let mine = factions_of(g, r);
    if o == FormId::PLAYER_REF && mine.iter().any(|f| g.player.enemy_of.contains(f)) {
        return Ok(Value::Num(1.0));
    }
    let theirs = factions_of(g, o);
    let all: Vec<i64> = mine.iter().flat_map(|&x| theirs.iter().map(move |&y| (x, y))).map(|(x, y)| reaction(g, x, y)).collect();
    let v = if all.contains(&1) {
        1
    } else if all.contains(&3) {
        3
    } else if all.contains(&2) {
        2
    } else {
        0
    };
    Ok(Value::Num(v as f64))
}

/// `SetEnemy f1 f2 [flag12 flag21]`: each direction becomes enemy (flag 0, the default)
/// or neutral (1) [documented: catalogue notes].
pub fn set_enemy(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let (f1, f2) = (a.form(0)?, a.form(1)?);
    let v = |x: f64| if x != 0.0 { 0 } else { 1 };
    let (r12, r21) = (v(a.num_or(2, 0.0)), v(a.num_or(3, 0.0)));
    g.forms.faction_reactions.entry(f1).or_default().insert(f2, r12);
    g.forms.faction_reactions.entry(f2).or_default().insert(f1, r21);
    none()
}

/// `SetAlly f1 f2 [flag12 flag21]`: each direction becomes ally (flag 0, the default)
/// or friend (1) [documented: catalogue notes].
pub fn set_ally(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let (f1, f2) = (a.form(0)?, a.form(1)?);
    let v = |x: f64| if x != 0.0 { 3 } else { 2 };
    let (r12, r21) = (v(a.num_or(2, 0.0)), v(a.num_or(3, 0.0)));
    g.forms.faction_reactions.entry(f1).or_default().insert(f2, r12);
    g.forms.faction_reactions.entry(f2).or_default().insert(f1, r21);
    none()
}

/// The actor leaves every faction it is in [documented: catalogue summary].
pub fn remove_from_all_factions(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    for f in factions_of(g, r) {
        g.extra.entry(r).or_default().factions.insert(f, -1);
    }
    none()
}

/// The caller's rank minus the target's in a faction [documented: catalogue summary];
/// a non-member's rank counts as −1 [hypothesis].
pub fn get_faction_rank_difference(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let f = a.form(0)?;
    let t = a.form(1)?;
    Ok(Value::Num((faction_rank(g, r, f) - faction_rank(g, t, f)) as f64))
}

/// `SetPCEnemyOfFaction faction flag` [documented: catalogue summary].
pub fn set_pc_enemy_of_faction(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let f = a.form(0)?;
    if a.num_or(1, 1.0) != 0.0 {
        g.player.enemy_of.insert(f);
    } else {
        g.player.enemy_of.remove(&f);
    }
    none()
}

pub fn get_pc_enemy_of_faction(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.player.enemy_of.contains(&a.form(0)?))
}

pub fn clear_faction_player_enemy_flag(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.enemy_of.remove(&a.form(0)?);
    none()
}

/// 1 when the player is a crime enemy of any faction the caller is in [documented:
/// catalogue summary].
pub fn get_actor_faction_player_enemy(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(factions_of(g, r).iter().any(|f| g.player.enemy_of.contains(f)))
}

flag!(ignore_crime, get_ignore_crime, ignore_crime);
flag!(set_ignore_friendly_hits, get_ignore_friendly_hits, ignore_friendly_hits);

/// `actor.SetLevel level [levelToPC min max]`: the player's level, or an actor's
/// (with the level-to-player flag the level is an offset from the player's, kept within
/// min and max when given) [documented: catalogue summary and notes]. Re-rolling an
/// actor's inventory and the player's stats are not modelled.
pub fn set_level(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    let mut level = a.int(0)?;
    if r == FormId::PLAYER_REF {
        g.player.level = Some(level);
        return none();
    }
    if a.num_or(1, 0.0) != 0.0 {
        level += g.player.level.unwrap_or(g.world.config().player_level as i32);
        if let Ok(min) = a.int(2) {
            level = level.max(min);
        }
        if let Some(max) = a.int(3).ok().filter(|&m| m > 0) {
            level = level.min(max);
        }
    }
    g.extra.entry(r).or_default().level = Some(level);
    none()
}

/// How many actors are the player's teammates now [documented: catalogue summary].
pub fn get_player_teammate_count(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let n = g.extra.iter().filter(|(_, e)| e.teammate && !e.dead).count();
    Ok(Value::Num(n as f64))
}

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

/// Current health over base health [documented: catalogue notes]; a dead actor is at 0.
pub fn get_health_percentage(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if g.extra.get(&r).is_some_and(|e| e.dead) {
        return Ok(Value::Num(0.0));
    }
    let base = permanent(g, r, crate::combat::HEALTH)?;
    let now = base - damage(g, r, crate::combat::HEALTH);
    Ok(Value::Num(if base > 0.0 { (now / base).clamp(0.0, 1.0) } else { 0.0 }))
}
