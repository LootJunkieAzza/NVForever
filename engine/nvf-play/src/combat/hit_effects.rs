//! Hit effects (docs/spec-notes/hit-effects.md): the magic items a hit applies to the
//! one hit, through `fnv_game::Game::apply_hit_effect` (the drain by the second, the
//! resistances, the refresh, deaths are the game's):
//!
//! - a weapon's object effect (WEAP `EITM`: the flamer's and the Shishkebab's
//!   "Burning");
//! - a blast's (EXPL `EITM`: the incinerator's and the incendiary grenade's);
//! - an actor's unarmed attack effect (`EITM` on its base, `EAMT` the attack it needs)
//!   for a blow without a weapon: a cazador's or a radscorpion's poison.
//!
//! The effects are applied before the hit's damage, so a killing hit leaves only those
//! that outlast a death ("No Death Dispel": a corpse burns on) (§4). The player in god
//! mode takes none.
//!
//! Log: `hit effect: <item> on <target> by <attacker>: <effects>` when one starts (a
//! repeat that only starts the time again is counted, not logged), `hit effect: <item>
//! on <target> ends (…)` when its last effect ends, and at exit `hit effects summary: …`.

use bevy::prelude::*;
use fnv_combat::data::CombatForms;
use fnv_game::game::Game;
use fnv_sim::FormId;
use std::collections::BTreeMap;

/// The magic items applied by hits and still active, for the log and the summary.
#[derive(Default, Debug, Clone)]
pub struct HitEffects {
    /// (target, item) → the effect ids active from it, and its name for the log.
    live: BTreeMap<(FormId, FormId), (Vec<u32>, String)>,
    pub applied: u32,
    pub refreshed: u32,
    pub ended: u32,
    /// Items that started on someone, by name, with counts.
    pub by_item: BTreeMap<String, u32>,
    /// Unarmed effects whose attack animation is not told apart here, reported once
    /// each.
    told: Vec<FormId>,
}

/// What a hit applies: the weapon's object effect, or with no weapon (fists, a
/// creature's own attack) the attacker's unarmed attack effect.
pub fn hit_items(forms: &CombatForms, game: &Game, attacker: Option<FormId>, weapon: Option<FormId>, unarmed: bool, fx: &mut HitEffects) -> Vec<FormId> {
    let mut out = Vec::new();
    if let Some(e) = weapon.and_then(|w| forms.weapons.get(&w)).and_then(|w| w.enchantment) {
        out.push(e);
    }
    if unarmed {
        if let Some((e, anim)) = attacker.and_then(|a| game.unarmed_effect(a)) {
            // "Any" goes with every blow. Another attack (a radscorpion's
            // AttackForwardPower) is not told apart: the creatures' attacks here are
            // their h2hattack clips taken in turn, power and forward ones left out, so it
            // goes with every blow too [design, hit-effects.md §1].
            if anim.is_some_and(|a| !a.eq_ignore_ascii_case("Any")) && !fx.told.contains(&e) {
                fx.told.push(e);
                info!("hit effect: {} names the attack {} for its blows; every blow applies it here", game.data.name(e), anim.unwrap_or("?"));
            }
            out.push(e);
        } else {
            debug!("hit effect: a blow by {} has no unarmed attack effect", attacker.map_or("?".to_string(), |a| who(game, a)));
        }
    }
    out
}

/// Apply `items` to `target` for a hit by `attacker` (before the hit's damage).
pub fn apply(game: &mut Game, fx: &mut HitEffects, target: FormId, items: &[FormId], attacker: Option<FormId>) {
    for &item in items {
        let applied = game.apply_hit_effect(target, item, attacker);
        if applied.is_empty() {
            debug!("hit effect: {} on {}: nothing applied (dead, unharmable, resisted or its conditions)", game.data.name(item), who(game, target));
            continue;
        }
        let name = game.data.name(item).to_string();
        let entry = fx.live.entry((target, item)).or_insert_with(|| (Vec::new(), name.clone()));
        let fresh: Vec<_> = applied.iter().filter(|a| !a.extended).collect();
        for a in &applied {
            if !entry.0.contains(&a.id) {
                entry.0.push(a.id);
            }
        }
        if fresh.is_empty() {
            fx.refreshed += 1;
            continue;
        }
        fx.applied += 1;
        *fx.by_item.entry(name.clone()).or_default() += 1;
        let parts: Vec<String> = applied
            .iter()
            .map(|a| {
                let what = match a.change {
                    Some(c) if c.kind == fnv_game::aid::ChangeKind::PerSecond => format!("{:+.1} {}/s", c.amount, av_name(c.code)),
                    Some(c) => format!("{:+.1} {}", c.amount, av_name(c.code)),
                    None => "script or look".to_string(),
                };
                format!("{} {what} for {:.0} s", game.data.name(a.effect), a.duration)
            })
            .collect();
        let by = attacker.map_or("?".to_string(), |a| who(game, a));
        info!("hit effect: {name} on {} by {by}: {}", who(game, target), parts.join(", "));
    }
}

/// A reference's name for the log ("the player" for the player).
fn who(game: &Game, r: FormId) -> String {
    if r == FormId::PLAYER_REF {
        "the player".to_string()
    } else {
        game.data.name(r).to_string()
    }
}

fn av_name(code: u16) -> String {
    fnv_game::functions::AV_NAMES.get(code as usize).map_or_else(|| format!("AV {code}"), |n| n.to_string())
}

/// Items whose effects have all ended since the last frame: logged and forgotten.
pub fn watch(game: &Game, fx: &mut HitEffects) {
    let mut done = Vec::new();
    for (&(target, item), (ids, name)) in fx.live.iter_mut() {
        ids.retain(|id| game.effects.iter().any(|e| e.id == *id));
        if ids.is_empty() {
            let why = if game.is_dead(target) { "its time is up or the target died" } else { "its time is up" };
            info!("hit effect: {name} on {} ends ({why})", who(game, target));
            done.push((target, item));
        }
    }
    for k in done {
        fx.live.remove(&k);
        fx.ended += 1;
    }
}

/// The exit summary line.
pub fn summary(fx: &HitEffects) -> String {
    let items: Vec<String> = fx.by_item.iter().map(|(n, c)| format!("{n} ×{c}")).collect();
    format!(
        "hit effects summary: {} applied ({}), {} refreshed by later hits, {} ended, {} still active",
        fx.applied,
        if items.is_empty() { "none".to_string() } else { items.join(", ") },
        fx.refreshed,
        fx.ended,
        fx.live.len()
    )
}
