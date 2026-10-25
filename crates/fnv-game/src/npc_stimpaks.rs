//! NPCs heal themselves with the stimpaks they carry (docs/design/combat-ai.md §20; owner
//! request 2026-10-25). The rules are **[design]** unless tagged: combat is not a parity
//! target (docs/design/combat.md).
//!
//! - **Who**: a living NPC (NPC_, not a creature, not the player) that is not knocked
//!   out, on furniture or in a conversation, while it is in Combat or within
//!   [`HURT_WINDOW`] seconds of its health last falling ("when they take damage"). An NPC
//!   hurt before anyone watched it (a quest's wounded soldier) is left as it is.
//! - **What**: a carried ingestible of equip type Stimpak (ETYP 11 [documented: xEdit
//!   wbEquipTypeEnum]: the Stimpak and the Super Stimpak [observed: FalloutNV.esm]) that
//!   restores health; how much it restores is what using it would give this NPC now
//!   (`Game::aid_preview`: its Medicine's potency, its perks, Hardcore's heal over time
//!   counted whole).
//! - **Not wasteful** ([`pick`]): a stimpak is used only when at most [`MAX_WASTE`] of
//!   what it restores would go past full health; of those, the one that restores most.
//!   Below [`LOW_HEALTH`] of its health it uses the one that restores least even if more
//!   of it goes to waste. An NPC one hit short of full uses nothing.
//! - **One at a time**: none while one is being used, nor for [`REUSE_SECONDS`] (or the
//!   stimpak's own effect duration, when longer: Hardcore's heal over 6 s) after one
//!   took effect, so health can show what the last one did.
//! - **How**: the engine (`npc_stimpaks.rs`) plays the idle the tree has for it and asks
//!   [`Game::begin_stimpak`]; while it plays the NPC holds `used_item` (GetIsUsedItem
//!   answers 1 for the stimpak), the combat AI holds its fire, and at the injection
//!   [`Game::inject_stimpak`] takes one from its inventory and uses it
//!   ([`Game::ingest`]), playing `NPCHumanUsingStimpak` at it [hypothesis: that sound is
//!   meant for this, by its name; when it plays]. Stopped before the injection (the NPC
//!   died, was knocked out), nothing is used.
//! - **The idle**: the children of `ActStimpakSource` (0x00023035) in the tree's order,
//!   the first whose own conditions pass: `3rdP2HHStimpak` (GetWeaponAnimType == 7: a
//!   two-handed heavy weapon) else `3rdPSpecialIdleStimpak` [observed: FalloutNV.esm].
//!   `ActStimpakSource`'s own condition (GetIsUsedItem Stimpak) is not asked: it would
//!   leave a Super Stimpak without an idle [design].
//!
//! Log/trace: `EventKind::Ingested` as for any use; the engine logs the rest.

use crate::functions::actor as fa;
use crate::game::Game;
use crate::sound::SoundRequest;
use fnv_sim::FormId;
use std::collections::{BTreeMap, HashMap};

/// Health below this share of its maximum: a stimpak is used even if much of it would go
/// past full health [design].
pub const LOW_HEALTH: f64 = 0.35;

/// At most this share of a stimpak's heal may go past full health [design: owner
/// 2026-10-25, "i dont want them using stims to recover only 1 hp if they have almost
/// full health"].
pub const MAX_WASTE: f64 = 0.25;

/// Seconds after its health last fell that an NPC out of Combat still heals [design].
pub const HURT_WINDOW: f64 = 15.0;

/// Seconds after a stimpak took effect before the next may be used [design].
pub const REUSE_SECONDS: f64 = 3.0;

/// The idle tree node whose children are the stimpak idles [observed: FalloutNV.esm].
pub const STIMPAK_SOURCE: &str = "ActStimpakSource";

/// The sound of an NPC using a stimpak [observed: FalloutNV.esm SOUN 0x000296B9].
pub const USING_SOUND: &str = "NPCHumanUsingStimpak";

/// Equip type Stimpak [documented: xEdit wbEquipTypeEnum].
const EQUIP_STIMPAK: i64 = 11;

/// A stimpak being used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StimpakUse {
    pub item: FormId,
    /// It took effect (one was used).
    pub injected: bool,
}

/// What the NPCs' stimpak use keeps. Not saved.
#[derive(Debug, Default)]
pub struct NpcStimpaks {
    /// The stimpak each NPC is using.
    pub using: BTreeMap<FormId, StimpakUse>,
    /// Each watched NPC's health when last looked at, and the game time it last fell.
    seen: HashMap<FormId, (f64, Option<f64>)>,
    /// Until when (game time) each NPC waits before using another.
    wait_until: HashMap<FormId, f64>,
}

/// Which carried stimpak to use (see the module notes): `carried` holds each stimpak and
/// the health it would restore.
pub fn pick(health: f64, max: f64, carried: &[(FormId, f64)]) -> Option<FormId> {
    let missing = max - health;
    if missing <= 0.0 || max <= 0.0 {
        return None;
    }
    let heals = carried.iter().filter(|(_, h)| *h > 0.0);
    let fits = heals.clone().filter(|(_, h)| missing >= h * (1.0 - MAX_WASTE)).max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
    if let Some((item, _)) = fits {
        return Some(*item);
    }
    if health < max * LOW_HEALTH {
        return heals.min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0))).map(|(i, _)| *i);
    }
    None
}

impl Game<'_> {
    /// The stimpaks `actor` carries and the health each would restore it now.
    pub fn carried_stimpaks(&mut self, actor: FormId) -> Vec<(FormId, f64)> {
        let data = self.data;
        let Some(inv) = self.world.peek(actor).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
        let items: Vec<FormId> = inv.iter().filter(|(i, n)| *n > 0 && data.ingestibles.get(i).is_some_and(|d| d.equip_type == EQUIP_STIMPAK)).map(|(i, _)| i).collect();
        items.into_iter().map(|i| (i, self.aid_preview(actor, i).iter().filter(|(c, _)| *c == crate::actor_values::HEALTH).map(|(_, a)| a).sum())).collect()
    }

    /// `actor`'s health now and its maximum.
    pub fn health_of(&mut self, actor: FormId) -> (f64, f64) {
        let max = fa::actor_value_base(self, actor, crate::actor_values::HEALTH);
        (fa::actor_value(self, actor, crate::actor_values::HEALTH).unwrap_or(max), max)
    }

    /// Whether `actor` is using a stimpak now (the combat AI holds its fire).
    pub fn using_stimpak(&self, actor: FormId) -> bool {
        self.npc_stimpaks.using.contains_key(&actor)
    }

    /// Look at a drawn NPC (every frame): note when its health falls, and say which
    /// stimpak it should use now and the idle to play for it, by the module's rules.
    /// `free`: nothing the engine plays on it stands in the way (an idle a script or a
    /// scene asked for, a reload, a blow under way).
    pub fn stimpak_wanted(&mut self, actor: FormId, free: bool) -> Option<(FormId, FormId)> {
        let npc = crate::functions::base_of(self, actor).and_then(|b| self.data.sig(b)) == Some(*b"NPC_");
        if actor == FormId::PLAYER_REF || !npc {
            return None;
        }
        let now = self.time;
        let health = fa::actor_value(self, actor, crate::actor_values::HEALTH).ok()?;
        let seen = self.npc_stimpaks.seen.entry(actor).or_insert((health, None));
        if health < seen.0 {
            seen.1 = Some(now);
        }
        seen.0 = health;
        let hurt_lately = seen.1.is_some_and(|t| now - t <= HURT_WINDOW);
        if !free || self.using_stimpak(actor) || self.npc_stimpaks.wait_until.get(&actor).is_some_and(|t| now < *t) {
            return None;
        }
        if self.is_dead(actor) || self.is_out_cold(actor) || self.furniture_use(actor).is_some() || self.talk_parts().iter().any(|p| p.actor == actor) {
            return None;
        }
        if !(hurt_lately || self.awareness.state(actor).in_combat()) {
            return None;
        }
        let max = fa::actor_value_base(self, actor, crate::actor_values::HEALTH);
        if health >= max {
            return None;
        }
        let carried = self.carried_stimpaks(actor);
        let item = pick(health, max, &carried)?;
        let idle = self.stimpak_idle(actor)?;
        Some((item, idle))
    }

    /// The idle `actor` plays to use a stimpak (module notes), with `item` as what it
    /// uses so the tree's conditions see it.
    fn stimpak_idle(&mut self, actor: FormId) -> Option<FormId> {
        let data = self.data;
        let source = data.by_edid(STIMPAK_SOURCE)?;
        let kids = data.idle_children.get(&Some(source))?.clone();
        kids.into_iter().find(|i| data.idles.get(i).is_some_and(|d| d.has_file()) && self.idle_allowed(actor, *i))
    }

    /// `actor` starts using `item` (it plays `idle`, asked here).
    pub fn begin_stimpak(&mut self, actor: FormId, item: FormId, idle: FormId) {
        self.npc_stimpaks.using.insert(actor, StimpakUse { item, injected: false });
        self.extra.entry(actor).or_default().used_item = Some(item);
        self.ask_idle(actor, idle, crate::idles::IdleSource::Stimpak);
    }

    /// The injection: one stimpak leaves `actor`'s inventory and takes effect. Returns
    /// the health it had and has now, or `None` when it was not using one or no longer
    /// carries it.
    pub fn inject_stimpak(&mut self, actor: FormId) -> Option<(f64, f64)> {
        let u = self.npc_stimpaks.using.get_mut(&actor).filter(|u| !u.injected)?;
        u.injected = true;
        let item = u.item;
        if self.is_dead(actor) || self.is_out_cold(actor) || self.world.remove_item(actor, item, 1).ok()? < 1 {
            return None;
        }
        self.emit(crate::trace::EventKind::ItemRemoved { from: actor, item, count: 1, silent: true });
        let before = fa::actor_value(self, actor, crate::actor_values::HEALTH).unwrap_or(0.0);
        let effect_secs = self.data.effect_items.get(&item).map_or(0.0, |l| l.iter().map(|e| e.duration as f64).fold(0.0, f64::max));
        self.ingest(actor, item);
        if let Some(sound) = self.data.by_edid(USING_SOUND) {
            self.ask_sound(SoundRequest { sound, at: Some(actor), system: false });
        }
        let after = fa::actor_value(self, actor, crate::actor_values::HEALTH).unwrap_or(before);
        self.npc_stimpaks.wait_until.insert(actor, self.time + REUSE_SECONDS.max(effect_secs));
        // Its own heal is no hurt.
        if let Some(s) = self.npc_stimpaks.seen.get_mut(&actor) {
            s.0 = after;
        }
        Some((before, after))
    }

    /// `actor`'s stimpak idle ended or was stopped: it uses nothing more.
    pub fn end_stimpak(&mut self, actor: FormId) {
        if let Some(u) = self.npc_stimpaks.using.remove(&actor) {
            if let Some(e) = self.extra.get_mut(&actor).filter(|e| e.used_item == Some(u.item)) {
                e.used_item = None;
            }
        }
    }

    /// Forget NPCs no longer drawn (`keep`: those still watched).
    pub fn forget_stimpak_watch(&mut self, keep: impl Fn(FormId) -> bool) {
        let s = &mut self.npc_stimpaks;
        s.seen.retain(|a, _| keep(*a));
        let gone: Vec<FormId> = s.using.keys().copied().filter(|a| !keep(*a)).collect();
        for a in gone {
            self.end_stimpak(a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STIM: FormId = FormId(0x15169);
    const SUPER: FormId = FormId(0xCCEF2);

    #[test]
    fn nearly_full_health_uses_nothing() {
        // 1 HP short of 100 with a 30 HP stimpak: almost all of it would be wasted.
        assert_eq!(pick(99.0, 100.0, &[(STIM, 30.0)]), None);
        // 20 short: 10 of 30 would be wasted (a third), more than allowed.
        assert_eq!(pick(80.0, 100.0, &[(STIM, 30.0)]), None);
        assert_eq!(pick(100.0, 100.0, &[(STIM, 30.0)]), None);
    }

    #[test]
    fn a_stimpak_is_used_when_little_of_it_goes_to_waste() {
        // 24 short of a 30 HP heal: 6 wasted, a fifth.
        assert_eq!(pick(76.0, 100.0, &[(STIM, 30.0)]), Some(STIM));
        assert_eq!(pick(40.0, 100.0, &[(STIM, 30.0)]), Some(STIM));
    }

    #[test]
    fn of_those_that_fit_the_biggest_heal_is_used() {
        let both = [(STIM, 30.0), (SUPER, 100.0)];
        // 40 short: the super stimpak would waste 60, the stimpak fits.
        assert_eq!(pick(160.0, 200.0, &both), Some(STIM));
        // 90 short: both fit, the super stimpak heals more.
        assert_eq!(pick(110.0, 200.0, &both), Some(SUPER));
    }

    #[test]
    fn low_health_uses_the_smallest_heal_even_with_waste() {
        // 8 of 30 HP: 22 short of a 30 HP heal (8 wasted), but below the low share.
        assert_eq!(pick(8.0, 30.0, &[(STIM, 30.0), (SUPER, 100.0)]), Some(STIM));
        assert_eq!(pick(20.0, 40.0, &[(STIM, 30.0)]), None, "half health is not low");
    }

    #[test]
    fn an_item_that_restores_no_health_is_not_used() {
        assert_eq!(pick(10.0, 100.0, &[(STIM, 0.0)]), None);
        assert_eq!(pick(10.0, 100.0, &[]), None);
    }
}
