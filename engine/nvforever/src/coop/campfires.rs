//! Lit campfires in co-op (owner's request 2026-11-07, after queue item 54): a campfire of
//! Burning Campfire Redone (`fnv_game::campfire`) that one player lights burns in every
//! player's game, and goes out in all of them together [design].
//!
//! A fire is a reference made at run time (`PlaceAtMe`), numbered by each game on its own,
//! so the campfire (a placed reference, the same in every game) is what goes on the wire:
//!
//! - **The host's list.** The host keeps the campfires lit in its game (by its player, with
//!   either action, or for a guest) and lists those still burning
//!   ([`net::LIST_CAMPFIRE`], sent as the doors' list is), each with whether its silent
//!   fires still burn beside the one with the sound. The host's fires burn and go out by
//!   their own scripts, as in a game alone.
//! - **A guest follows the list.** A listed campfire burns here: lit when it does not
//!   (the plugin's five fires, `Game::light_campfire`), its fires' timers kept from running
//!   out while it is listed (`Game::hold_campfire`), its silent fires out when the host's
//!   are. A campfire that leaves the list goes out here (`Game::put_out_campfire`). So a
//!   guest that joins, or comes to the campfire's cell, while it burns finds it burning,
//!   and every game's fire goes out when the host's does.
//! - **A guest lights one.** It burns here at once and the guest asks the host
//!   (`net::Msg::Campfire`); the host lights it (or starts its fires again when it burns
//!   already: one fire, nothing stacked) and lists it. Until the host's list has it, for
//!   [`OWN_HELD`] seconds, the guest's own fire is not put out.
//!
//! Log: `co-op: campfire 0x… lit for <guest>: …`, `co-op: campfire 0x… burns in the
//! host's game: lit here (5 fires)`, `co-op: campfire 0x…: out in the host's game, N
//! fires out here`.

use super::{label, Coop};
use bevy::prelude::*;
use fnv_game::campfire::Lit;
use fnv_sim::FormId;
use nvf_coop::net;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The host makes its list again this often (seconds).
const REMAKE: f32 = 0.5;
/// A guest keeps the listed fires burning this often (seconds); a fire burns 25 s at
/// least (`fnv_game::campfire`).
const HOLD_EVERY: f32 = 1.0;
/// A campfire a guest lit is its own this long (seconds), until the host's list has it.
const OWN_HELD: f32 = 3.0;
/// A guest's request comes `net::HIT_COPIES` times: the copies within this long
/// (seconds) of the first are one request.
const COPIES_WITHIN: f32 = 0.5;

/// What co-op keeps of the campfires ([`Coop::campfires`]).
#[derive(Default)]
pub(super) struct Shared {
    /// A host's: the campfires lit in its game, and guests' requests (who, which) not yet
    /// done; when its list was last made.
    lit: BTreeSet<u32>,
    pub(super) asked: Vec<(u8, u32)>,
    asked_at: HashMap<(u8, u32), f32>,
    made: Option<f32>,
    /// A guest's: the host's list as last heard (campfire, silent fires burning); the
    /// campfires burning here for it or lit by this player; those this player lit lately
    /// (when); when the fires were last kept.
    pub(super) host: Option<BTreeMap<u32, bool>>,
    here: BTreeSet<u32>,
    own: HashMap<u32, f32>,
    held: Option<f32>,
}

impl Shared {
    /// A page of the host's campfire list, heard by a guest: the list is one page (a list
    /// of more than `net::PRESENCE_PAGE` campfires is not followed).
    pub(super) fn hear(&mut self, pages: u16, refs: Vec<(u32, bool)>) {
        if pages == 1 {
            self.host = Some(refs.into_iter().collect());
        }
    }
}

/// Campfires in co-op (module notes).
pub(super) fn share_campfires(mut coop: ResMut<Coop>, time: Res<Time>, sim: Option<ResMut<nvf_play::sim::SimState>>, script: Option<ResMut<nvf_play::script::ScriptState>>) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let (Some(link), Some(mut sim), Some(mut script)) = (coop.link.as_ref(), sim, script) else {
        if coop.link.is_none() {
            coop.campfires = Shared::default();
        }
        return;
    };
    let script = &mut *script;
    let mine = std::mem::take(&mut script.campfires_lit);
    let Some(rt) = script.rt.as_mut() else { return };
    let c = &mut coop.campfires;
    let peers = &coop.peers;
    let name = |id: u8| label(id, &peers.get(&id).map_or(String::new(), |p| p.name.clone()));
    if link.is_host() {
        c.lit.extend(mine);
        let mut asked = std::mem::take(&mut c.asked);
        c.asked_at.retain(|_, at| now - *at < COPIES_WITHIN);
        asked.retain(|&k| c.asked_at.insert(k, now).is_none());
        let remake = !asked.is_empty() || c.made.is_none_or(|at| now - at >= REMAKE);
        let list = nvf_play::script::with_world(rt, &mut sim, |rt| {
            for (id, r) in asked {
                match rt.game.light_campfire(FormId(r)) {
                    Ok(Lit::Placed(fires)) => info!("co-op: campfire {r:#010X} lit for {}: {} fires placed", name(id), fires.len()),
                    Ok(Lit::Restarted(fires)) => info!("co-op: campfire {r:#010X} lit for {}: burning already, {} fires start again", name(id), fires.len()),
                    Err(e) => {
                        warn!("co-op: campfire {r:#010X} not lit for {}: {e}", name(id));
                        continue;
                    }
                }
                c.lit.insert(r);
            }
            if !remake {
                return None;
            }
            c.lit.retain(|&r| !rt.game.campfire_fires(FormId(r)).is_empty());
            Some(c.lit.iter().map(|&r| (r, rt.game.campfire_full(FormId(r)))).collect::<Vec<_>>())
        });
        if let Some(list) = list {
            c.made = Some(now);
            link.set_presence(net::LIST_CAMPFIRE, list);
        }
        return;
    }
    for r in mine {
        if c.own.insert(r, now).is_none() {
            info!("co-op: this player lit campfire {r:#010X}: the host lights it too");
        }
        c.here.insert(r);
        link.send_campfire(r);
    }
    c.own.retain(|_, at| now - *at < OWN_HELD);
    let Some(host) = c.host.as_ref() else { return };
    if c.held.is_some_and(|at| now - at < HOLD_EVERY) {
        return;
    }
    c.held = Some(now);
    let (here, own) = (&mut c.here, &c.own);
    nvf_play::script::with_world(rt, &mut sim, |rt| {
        for (&r, &full) in host {
            let id = FormId(r);
            if rt.game.campfire_fires(id).is_empty() {
                match rt.game.light_campfire(id) {
                    Ok(lit) => {
                        let n = match lit {
                            Lit::Placed(f) | Lit::Restarted(f) => f.len(),
                        };
                        info!("co-op: campfire {r:#010X} burns in the host's game: lit here ({n} fires)");
                    }
                    Err(e) => {
                        warn!("co-op: campfire {r:#010X} burns in the host's game, not here: {e}");
                        continue;
                    }
                }
            }
            here.insert(r);
            rt.game.hold_campfire(id);
            if !full {
                let n = rt.game.put_out_campfire(id, true);
                if n > 0 {
                    info!("co-op: campfire {r:#010X}: its silent fires are out in the host's game, {n} out here");
                }
            }
        }
        here.retain(|r| {
            if host.contains_key(r) || own.contains_key(r) {
                return true;
            }
            let n = rt.game.put_out_campfire(FormId(*r), false);
            info!("co-op: campfire {r:#010X}: out in the host's game, {n} fires out here");
            false
        });
    });
}
