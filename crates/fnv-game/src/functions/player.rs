//! Player functions: experience, perks, notes, tag skills, achievements, controls.

use super::{none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;

pub fn reward_xp(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.xp += a.num(0)?.trunc();
    none()
}

/// Perks are kept for the player only; other actors have none [hypothesis: NPC perk
/// lists are not read yet].
pub fn has_perk(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(r == FormId::PLAYER_REF && g.player.perks.contains_key(&a.form(0)?))
}

pub fn add_perk(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if ctx.subject_or_player() != FormId::PLAYER_REF {
        return Err("perks of actors other than the player are not modelled".into());
    }
    *g.player.perks.entry(a.form(0)?).or_default() += 1;
    none()
}

pub fn remove_perk(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if ctx.subject_or_player() == FormId::PLAYER_REF {
        g.player.perks.remove(&a.form(0)?);
    }
    none()
}

pub fn get_has_note(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.player.notes.contains(&a.form(0)?))
}

pub fn add_note(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.notes.insert(a.form(0)?);
    none()
}

pub fn is_hardcore(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.player.hardcore.unwrap_or(g.world.config().hardcore))
}

pub fn is_player_tag_skill(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.player.tag_skills.contains(&a.code(0)?))
}

pub fn set_player_tag_skill(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.tag_skills.insert(a.code(0)?);
    none()
}

pub fn add_achievement(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.achievements.insert(a.int(0)?);
    none()
}

/// The seven control groups: movement, Pip-Boy, fighting, POV, looking, rollover text,
/// sneaking. With no parameters `DisablePlayerControls` disables the first four and
/// `EnablePlayerControls` enables all seven [documented: GECK wiki pages of both].
fn controls(a: &Args, default: [bool; 7]) -> [bool; 7] {
    if a.is_empty() {
        return default;
    }
    let mut out = [false; 7];
    for (i, o) in out.iter_mut().enumerate() {
        *o = a.num_or(i, 0.0) != 0.0;
    }
    out
}

pub fn disable_player_controls(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let which = controls(a, [true, true, true, true, false, false, false]);
    for (d, w) in g.player.controls_disabled.iter_mut().zip(which) {
        *d |= w;
    }
    none()
}

pub fn enable_player_controls(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let which = controls(a, [true; 7]);
    for (d, w) in g.player.controls_disabled.iter_mut().zip(which) {
        if w {
            *d = false;
        }
    }
    none()
}

/// 1 when any of the selected groups is disabled (all groups when none is selected)
/// [documented: catalogue summary].
pub fn get_player_controls_disabled(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let which = controls(a, [true; 7]);
    yes(g.player.controls_disabled.iter().zip(which).any(|(d, w)| *d && w))
}

/// The player's level from the configuration; other actors are not modelled.
pub fn get_level(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    if ctx.subject_or_player() != FormId::PLAYER_REF {
        return Err("levels of actors other than the player are not modelled".into());
    }
    Ok(Value::Num(g.world.config().player_level as f64))
}

pub fn remove_note(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.notes.remove(&a.form(0)?);
    none()
}

pub fn reset_xp(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.player.xp = 0.0;
    none()
}

/// Axis 0 is infamy, 1 fame [documented: GECK wiki GetReputation].
fn rep_axis(a: &Args) -> Result<usize, String> {
    match a.int(1)? {
        0 => Ok(0),
        1 => Ok(1),
        o => Err(format!("reputation axis {o} is not 0 (infamy) or 1 (fame)")),
    }
}

pub fn get_reputation(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.form(0)?;
    let i = rep_axis(a)?;
    Ok(Value::Num(g.player.reputation.get(&r).map(|v| v[i]).unwrap_or(0.0)))
}

/// Sets the amount exactly [documented: GECK wiki SetReputation].
pub fn set_reputation(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.form(0)?;
    let i = rep_axis(a)?;
    g.player.reputation.entry(r).or_default()[i] = a.num(2)?;
    none()
}

/// Karma (actor value 23) changes by the amount, kept within the iKarmaMin/iKarmaMax game
/// settings when a plugin defines them [documented: GECK wiki RewardKarma].
pub fn reward_karma(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let p = FormId::PLAYER_REF;
    let mut v = super::actor::actor_value(g, p, 23)? + a.num(0)?;
    if let Some(max) = g.world.forms.setting_f32("iKarmaMax") {
        v = v.min(max as f64);
    }
    if let Some(min) = g.world.forms.setting_f32("iKarmaMin") {
        v = v.max(min as f64);
    }
    g.extra.entry(p).or_default().actor_values.insert(23, v);
    none()
}

pub fn set_hardcore(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.hardcore = Some(a.num(0)? != 0.0);
    none()
}

/// Map markers made visible (fast travel is Layer 6).
pub fn show_map(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.map_markers.insert(a.form(0)?);
    none()
}

pub fn get_map_marker_visible(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.player.map_markers.contains(&r))
}

/// The player's miscellaneous statistics by code (the source names them as strings; the
/// bytes hold the code, scripts.md §2.4). They start at 0 and change through
/// ModPCMiscStat only [hypothesis: the game's own counters are Layer 6].
pub fn get_pc_misc_stat(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(g.player.misc_stats.get(&a.code(0)?).copied().unwrap_or(0.0)))
}

pub fn mod_pc_misc_stat(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    *g.player.misc_stats.entry(a.code(0)?).or_default() += a.num(1)?;
    none()
}
