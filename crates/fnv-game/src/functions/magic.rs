//! Magic effects as state: spells added to actors, items cast or consumed, the
//! effects active on each actor, and their effect scripts (ScriptEffectStart /
//! Update / Finish). The rules: [`Game::apply_magic_item`] and docs/spec-notes/
//! scripts.md §7.6. What an effect does to actor values: `crate::aid`
//! (docs/spec-notes/aid-items.md); shaders and sounds are Layer 6.

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
    g.add_spell(r, s);
    none()
}

/// `actor.RemoveSpell spell`: the spell is removed with its active effects (their
/// finish blocks run) [hypothesis]; an authored ability too, which is not given again
/// (`crate::abilities`) [hypothesis]. A worn item's enchantment is no spell of the actor
/// and stays while the item is worn [hypothesis; aid-items.md §13].
pub fn remove_spell(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = a.form(0)?;
    g.sync_actor_effects(r);
    if let Some(e) = g.extra.get_mut(&r) {
        e.spells.retain(|&x| x != s);
    }
    g.dispel(r, ctx.inst, |e| e.source == s && e.worn.is_none());
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

/// Every effect on the actor ends except those of added spells (abilities, authored or
/// not) and of worn enchantments [hypothesis: the catalogue says all effects; abilities
/// are not "spells cast"].
pub fn dispel_all_spells(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.dispel(r, ctx.inst, |e| !e.permanent);
    none()
}

/// 1 while an effect of the given magic item is on the actor [documented: catalogue
/// summary; GECK wiki IsSpellTarget: an actor effect (abilities included), an object
/// effect or an ingestible], its authored abilities and worn enchantments included
/// (`crate::abilities`), an effect whose conditions fail too [hypothesis].
pub fn is_spell_target(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = a.form(0)?;
    g.sync_actor_effects(r);
    yes(g.effects.iter().any(|e| e.target == r && e.source == s))
}

/// 1 while an effect with the given base effect affects the reference [documented:
/// catalogue summary; GECK wiki HasMagicEffect "currently being affected"]: not an
/// ability's effect whose conditions fail [hypothesis].
pub fn has_magic_effect(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let m = a.form(0)?;
    g.sync_actor_effects(r);
    yes(g.effects.iter().any(|e| e.target == r && e.effect == m && !e.inactive))
}

/// Seconds since the last effect update inside ScriptEffectUpdate, 0 elsewhere
/// [documented: GECK wiki ScriptEffectElapsedSeconds].
pub fn script_effect_elapsed_seconds(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(if matches!(ctx.inst, Some(InstKey::Effect { .. })) { g.effect_elapsed } else { 0.0 }))
}

/// `actor.GetSpellUsageNum ingestible`: the uses of the ingestible while its usage
/// monitor is active, times the monitor's magnitude [documented: GECK wiki
/// GetSpellUsageNum; aid-items.md §5.1].
pub fn get_spell_usage_num(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(g.spell_usage(r, a.form(0)?)))
}
