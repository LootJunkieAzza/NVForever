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
    yes(g.world.config().hardcore)
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
