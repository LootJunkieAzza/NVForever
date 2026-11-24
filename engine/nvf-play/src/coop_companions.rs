//! The other co-op players as this player's companions (queue item 65): each game tells
//! the others its player's Charisma and whether its Ferocious Loyalty acts now, and puts
//! what they give into `Game::coop_from_others` (`fnv_game::coop_perks::FromOthers`:
//! Companion Nerve from the highest Charisma of the others, Ferocious Loyalty's Damage
//! Resistance while another player with it is below half health). Combat reads it as a
//! companion's (`crate::combat`: `companion_nerve`, `target_armor`, `remote_hits`).
//!
//! A player's own perks never count for itself; a player gone from the session, or not
//! heard for [`STALE`] seconds, gives nothing; out of a session it is all 0 [design].
//!
//! **The companions' follow perks (item 67).** Each player also sends the follow perks
//! its own companions give it now (`fnv_game::coop_perks::COMPANION_PERKS`: Boone's
//! Spotting, Cass's Whiskey Rose ...), which the original gives the player while that
//! companion follows and takes back when it is dismissed, dies or leaves (the companions'
//! scripts). Owner 2026-11-20: "While a companion is in the party, all players get the
//! perk" [design]: whoever hired it, every other player has the same perks
//! ([`fnv_game::coop_perks::mirror_perk`]: the perk itself, so its entries, abilities and
//! the Pip-Boy's perk page are the original's), and loses each when no other player's
//! companion gives it any more, when that player is no longer heard, or when this one
//! leaves the session. A player that joins while another has a companion gets its perk
//! with that player's first message. A perk the player had of its own is not touched, and
//! a lent perk is never sent on. They are the session's: no player's own save keeps them
//! (`fnv_game::coop_story`; the host's save too, `crate::save`).
//!
//! Message: Charisma, Ferocious Loyalty (0/1), the number of companion perks, each
//! perk's FormID (4 bytes, little-endian; host and guests load the same plugins).

use crate::coop_tables::{game, Tables};
use crate::script::ScriptState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;
use std::collections::HashMap;

/// Each game says its side this often, and again at once when it changes (real seconds).
const EVERY: f64 = 2.0;
/// A player not heard for this long gives nothing (real seconds).
const STALE: f64 = 7.0;

pub struct CoopCompanionsPlugin;

impl Plugin for CoopCompanionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, exchange.in_set(PlaySet::Ui));
    }
}

#[derive(Default)]
struct Seen {
    sent: f64,
    last: Option<(u8, bool)>,
    others: HashMap<u8, (u8, bool, f64)>,
    given: fnv_game::coop_perks::FromOthers,
    /// The follow perks this player's own companions give, as last sent.
    last_perks: Vec<u32>,
    /// Each other player's companions' follow perks, as last heard.
    party_perks: HashMap<u8, Vec<FormId>>,
    /// Follow perks this player has because another player's companion gives them.
    mirrored: Vec<FormId>,
}

fn exchange(time: Res<Time<Real>>, tables: Option<ResMut<Tables>>, script: Option<ResMut<ScriptState>>, mut seen: Local<Seen>) {
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let local = time.elapsed_secs_f64();
    let Some(mut tables) = tables.filter(|t| t.on) else {
        if seen.last.is_some() || rt.game.coop_from_others != Default::default() || !seen.mirrored.is_empty() {
            for p in std::mem::take(&mut seen.mirrored) {
                info!("co-op companions: out of the session: {} taken back", rt.game.data.name(p));
                fnv_game::coop_perks::unmirror_perk(&mut rt.game, p);
            }
            *seen = Seen::default();
            rt.game.coop_from_others = Default::default();
            info!("co-op companions: out of the session: nothing from other players");
        }
        return;
    };
    let mine = fnv_game::coop_perks::my_side(&mut rt.game);
    let perks: Vec<u32> = fnv_game::coop_perks::own_companion_perks(&rt.game, &seen.mirrored).iter().map(|p| p.0).collect();
    if perks != seen.last_perks {
        info!("co-op companions: this player's companions' perks now: {}", if perks.is_empty() { "none".to_string() } else { perks.iter().map(|p| rt.game.data.name(FormId(*p))).collect::<Vec<_>>().join(", ") });
        seen.sent = f64::NEG_INFINITY;
        seen.last_perks = perks.clone();
    }
    if seen.last != Some(mine) || local - seen.sent >= EVERY {
        if seen.last != Some(mine) {
            info!("co-op companions: this player tells the others Charisma {}, Ferocious Loyalty {}", mine.0, if mine.1 { "acting" } else { "not acting" });
        }
        seen.sent = local;
        seen.last = Some(mine);
        let mut blob = vec![mine.0, u8::from(mine.1), perks.len().min(255) as u8];
        for p in perks.iter().take(255) {
            blob.extend(p.to_le_bytes());
        }
        tables.send(0, game::PERKS, blob);
    }
    for (from, _, blob) in tables.take_actions(game::PERKS) {
        if let [charisma, loyal, ..] = blob[..] {
            seen.others.insert(from, (charisma, loyal != 0, local));
        }
        let n = blob.get(2).copied().unwrap_or(0) as usize;
        let list: Vec<FormId> = blob.get(3..3 + 4 * n).unwrap_or_default().as_chunks::<4>().0.iter().map(|c| FormId(u32::from_le_bytes(*c))).collect();
        seen.party_perks.insert(from, list);
    }
    let me = tables.me;
    seen.others.retain(|p, (.., at)| Some(*p) != me && tables.names.contains_key(p) && local - *at < STALE);
    // The other players' companions' perks on this player (module notes).
    let s = &mut *seen;
    s.party_perks.retain(|p, _| s.others.contains_key(p));
    let want = fnv_game::coop_perks::party_perks(s.party_perks.values().map(Vec::as_slice));
    let (lent, taken) = fnv_game::coop_perks::sync_party_perks(&mut rt.game, &mut s.mirrored, &want);
    for p in lent {
        info!("co-op companions: another player's companion gives {} to this player too", rt.game.data.name(p));
    }
    for p in taken {
        info!("co-op companions: no other player's companion gives {} now: this player loses it", rt.game.data.name(p));
    }
    let given = fnv_game::coop_perks::from_others(seen.others.values().map(|(c, l, _)| (*c, *l)), fnv_game::coop_perks::loyalty_dr(rt.game.data));
    if given != seen.given {
        info!("co-op companions: from the other players: Companion Nerve {:+.0} %, Damage Resistance {:+.0} (Ferocious Loyalty)", given.nerve * 100.0, given.dr);
        seen.given = given;
    }
    rt.game.coop_from_others = given;
}
