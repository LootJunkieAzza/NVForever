//! Magic effects as state: spells added to actors, items cast or consumed, the
//! effects active on each actor, and their effect scripts (ScriptEffectStart /
//! Update / Finish). The rules: [`Game::apply_magic_item`] and docs/spec-notes/
//! scripts.md §7.6. What an effect does besides its script (actor value changes,
//! shaders, sounds) is Layer 6.

use super::{none, yes, Args, Ctx};
use crate::game::{Game, InstKey};
use fnv_script::vm::Value;

fn magic_item(g: &Game, a: &Args) -> Result<fnv_sim::FormId, String> {
    let s = a.form(0)?;
    if g.data.effect_items.contains_key(&s) {
        Ok(s)
    } else {
        Err(format!("{} is not a spell, enchantment or ingestible", g.data.name(s)))
    }
}

/// `actor.AddSpell spell`: the spell's effects apply at once and last until removed
/// [documented: catalogue summary of AddSpell; hypothesis: every effect of an added
/// spell lasts, as the shipped uses are abilities and addictions]. Adding a spell the
/// actor already has does nothing [hypothesis].
pub fn add_spell(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = magic_item(g, a)?;
    let e = g.extra.entry(r).or_default();
    if e.spells.contains(&s) {
        return none();
    }
    e.spells.push(s);
    g.apply_magic_item(r, s, None, true);
    none()
}

/// `actor.RemoveSpell spell`: the spell is removed with its active effects (their
/// finish blocks run) [hypothesis].
pub fn remove_spell(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = a.form(0)?;
    if let Some(e) = g.extra.get_mut(&r) {
        e.spells.retain(|&x| x != s);
    }
    g.dispel(r, ctx.inst, |e| e.source == s);
    none()
}

/// `ref.CastImmediateOnSelf spell`: the spell's effects apply to the caller at once,
/// with their durations [documented: catalogue summary; hypothesis: no casting
/// animation or projectile is involved].
pub fn cast_immediate_on_self(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = magic_item(g, a)?;
    g.apply_magic_item(r, s, Some(r), false);
    none()
}

/// `actor.Dispel spell`: the effects of that spell end [documented: catalogue summary];
/// see [`Game::dispel`] for a script dispelling its own effect.
pub fn dispel(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = a.form(0)?;
    g.dispel(r, ctx.inst, |e| e.source == s && !e.permanent);
    none()
}

/// Every effect on the actor ends except those of added spells (abilities)
/// [hypothesis: the catalogue says all effects; abilities are not "spells cast"].
pub fn dispel_all_spells(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.dispel(r, ctx.inst, |e| !e.permanent);
    none()
}

/// 1 while an effect of the given magic item is active on the actor [documented:
/// catalogue summary].
pub fn is_spell_target(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = a.form(0)?;
    yes(g.effects.iter().any(|e| e.target == r && e.source == s))
}

/// 1 while an effect with the given base effect is active on the reference
/// [documented: catalogue summary].
pub fn has_magic_effect(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let m = a.form(0)?;
    yes(g.effects.iter().any(|e| e.target == r && e.effect == m))
}

/// Seconds since the last effect update inside ScriptEffectUpdate, 0 elsewhere
/// [documented: GECK wiki ScriptEffectElapsedSeconds].
pub fn script_effect_elapsed_seconds(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(if matches!(ctx.inst, Some(InstKey::Effect { .. })) { g.effect_elapsed } else { 0.0 }))
}
