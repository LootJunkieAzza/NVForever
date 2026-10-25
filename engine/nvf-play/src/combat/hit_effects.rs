//! Hit effects (docs/spec-notes/hit-effects.md): the magic items a hit applies to the
//! one hit, through `fnv_game::Game::apply_hit_effect` (the drain by the second, the
//! resistances, the refresh, deaths are the game's):
//!
//! - a weapon's object effect (WEAP `EITM`: the flamer's and the Shishkebab's
//!   "Burning");
//! - a blast's (EXPL `EITM`: the incinerator's and the incendiary grenade's);
//! - an actor's unarmed attack effect (`EITM` on its base, `EAMT` the attack it needs)
//!   for a blow without a weapon: a cazador's poison with every blow, a radscorpion's
//!   with its forward power attack (its sting) only ([`applies`]).
//!
//! A weapon's object effect with an area (Lanius's sword, the Holorifle, Meanie's
//! sledgehammer) also reaches the actors around the one hit ([`spread`],
//! `fnv_game::area_effects`); a blast's does not (the blast's radius picks its targets).
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
/// creature's own attack) the attacker's unarmed attack effect; `anim`: the blow's power
/// attack group ("AttackForwardPower"), `None` for an ordinary blow.
pub fn hit_items(forms: &CombatForms, game: &Game, attacker: Option<FormId>, weapon: Option<FormId>, unarmed: bool, anim: Option<&str>, fx: &mut HitEffects) -> Vec<FormId> {
    let mut out = Vec::new();
    if let Some(e) = weapon.and_then(|w| forms.weapons.get(&w)).and_then(|w| w.enchantment) {
        out.push(e);
    }
    if unarmed {
        if let Some((e, needs)) = attacker.and_then(|a| game.unarmed_effect(a)) {
            if applies(needs, anim) {
                out.push(e);
            } else {
                debug!("hit effect: {} needs the attack {}; this blow is {}", game.data.name(e), needs.unwrap_or("?"), anim.unwrap_or("an ordinary one"));
            }
            // An ordinary attack group (AttackLeft …) is not told apart (module notes).
            if needs.is_some_and(|n| !n.eq_ignore_ascii_case("Any") && !is_power(n)) && !fx.told.contains(&e) {
                fx.told.push(e);
                info!("hit effect: {} names the attack {} for its blows; every ordinary blow applies it here", game.data.name(e), needs.unwrap_or("?"));
            }
        } else {
            debug!("hit effect: a blow by {} has no unarmed attack effect", attacker.map_or("?".to_string(), |a| who(game, a)));
        }
    }
    out
}

/// Whether an unarmed attack effect naming the attack group `needs` (EAMT) goes with a
/// blow of power attack group `blow` (`None`: an ordinary blow): "Any" (or none named)
/// with every blow; a power attack's group only with that power attack (a radscorpion's
/// sting, AttackForwardPower); an ordinary group with every ordinary blow, as the
/// ordinary attacks are not told apart here [design, hit-effects.md §1].
pub fn applies(needs: Option<&str>, blow: Option<&str>) -> bool {
    match needs.filter(|n| !n.eq_ignore_ascii_case("Any")) {
        None => true,
        Some(n) if is_power(n) => blow.is_some_and(|b| b.eq_ignore_ascii_case(n)),
        Some(_) => blow.is_none(),
    }
}

fn is_power(group: &str) -> bool {
    group.to_ascii_lowercase().ends_with("power")
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

/// A weapon hit's object effects reach the actors around `target` when they have an area
/// (`fnv_game::Game::spread_area`, as a hit's; it logs `area effect: …`).
pub fn spread(game: &mut Game, fx: &mut HitEffects, target: FormId, items: &[FormId], attacker: Option<FormId>) {
    for &item in items {
        let reached = game.spread_area(target, item, attacker, true);
        if reached.is_empty() {
            continue;
        }
        let name = game.data.name(item).to_string();
        for (r, ids) in &reached {
            if ids.is_empty() {
                continue;
            }
            fx.applied += 1;
            *fx.by_item.entry(name.clone()).or_default() += 1;
            let entry = fx.live.entry((*r, item)).or_insert_with(|| (Vec::new(), name.clone()));
            for id in ids {
                if !entry.0.contains(id) {
                    entry.0.push(*id);
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::applies;

    #[test]
    fn an_unarmed_effect_goes_with_the_attack_it_names() {
        assert!(applies(Some("Any"), None) && applies(Some("Any"), Some("AttackPower")) && applies(None, None));
        assert!(applies(Some("AttackForwardPower"), Some("AttackForwardPower")));
        assert!(!applies(Some("AttackForwardPower"), Some("AttackPower")));
        assert!(!applies(Some("AttackForwardPower"), None));
        assert!(applies(Some("AttackLeft"), None) && !applies(Some("AttackLeft"), Some("AttackPower")));
    }
}
